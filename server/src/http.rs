use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, FromRequest, FromRequestParts, Path, Request, State};
use axum::http::header::{self, AsHeaderName, ETAG};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use bronze_wire::{AuthToken, HEALTH_ROUTE, Malformed, Precondition, VAULT_ROUTE, VaultId, Version, wait_preference};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::time::{Instant, sleep_until};

use crate::store::{Read, Store, Write, Written};
use crate::{Limits, Reject};

#[derive(Clone)]
struct App(Arc<Shared>);

struct Shared {
    store: Store,
    limits: Limits,
    writes: watch::Sender<()>,
    stopping: watch::Receiver<bool>,
}

pub async fn serve(
    listener: TcpListener,
    store: Store,
    limits: Limits,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let (stop, stopping) = watch::channel(false);
    let body_limit = DefaultBodyLimit::max(limits.max_bytes);
    let app = App(Arc::new(Shared { store, limits, writes: watch::Sender::new(()), stopping }));
    let router = Router::new()
        .route(HEALTH_ROUTE, get(async || "ok"))
        .route(VAULT_ROUTE, get(read).put(write))
        .layer(body_limit)
        .with_state(app);
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            shutdown.await;
            stop.send_replace(true);
        })
        .await
}

async fn read(State(app): State<App>, poll: Poll) -> Result<Response, Reject> {
    let deadline = Instant::now() + poll.wait;
    let mut writes = app.0.writes.subscribe();
    let mut stopping = app.0.stopping.clone();
    loop {
        let Caller { vault, token } = poll.caller.clone();
        match app.blocking(move |store, _| store.read(&vault, &token, poll.seen)).await? {
            Read::Changed { version, bytes } => {
                let headers = [(ETAG, version.etag()), (header::CONTENT_TYPE, "application/octet-stream".to_owned())];
                return Ok((StatusCode::OK, headers, bytes).into_response());
            }
            Read::Unchanged(version) => {
                let woke = tokio::select! {
                    changed = writes.changed() => changed.is_ok(),
                    _ = sleep_until(deadline) => false,
                    _ = stopping.wait_for(|stopping| *stopping) => false,
                };
                if !woke {
                    return Ok((StatusCode::NOT_MODIFIED, [(ETAG, version.etag())]).into_response());
                }
            }
        }
    }
}

async fn write(State(app): State<App>, write: Write) -> Result<Response, Reject> {
    let written = app.blocking(move |store, limits| store.write(write, limits)).await?;
    app.0.writes.send_replace(());
    Ok(match written {
        Written::Created(version) => (StatusCode::CREATED, [(ETAG, version.etag())]).into_response(),
        Written::Replaced(version) => (StatusCode::NO_CONTENT, [(ETAG, version.etag())]).into_response(),
    })
}

impl App {
    async fn blocking<T: Send + 'static>(
        &self,
        work: impl FnOnce(&Store, &Limits) -> Result<T, Reject> + Send + 'static,
    ) -> Result<T, Reject> {
        let shared = self.0.clone();
        tokio::task::spawn_blocking(move || work(&shared.store, &shared.limits))
            .await
            .map_err(|error| Reject::Internal(error.into()))?
    }
}

#[derive(Clone)]
struct Caller {
    vault: VaultId,
    token: AuthToken,
}

impl FromRequestParts<App> for Caller {
    type Rejection = Reject;

    async fn from_request_parts(parts: &mut Parts, app: &App) -> Result<Self, Reject> {
        let Path(vault) = Path::<String>::from_request_parts(parts, app).await.map_err(|_| Reject::NoVault)?;
        Ok(Self {
            vault: vault.parse().map_err(|_| Reject::NoVault)?,
            token: text(&parts.headers, header::AUTHORIZATION)
                .and_then(|value| AuthToken::from_authorization(value).ok())
                .ok_or(Reject::Unauthorized)?,
        })
    }
}

struct Poll {
    caller: Caller,
    seen: Option<Version>,
    wait: Duration,
}

impl FromRequestParts<App> for Poll {
    type Rejection = Reject;

    async fn from_request_parts(parts: &mut Parts, app: &App) -> Result<Self, Reject> {
        let caller = Caller::from_request_parts(parts, app).await?;
        let seen = text(&parts.headers, header::IF_NONE_MATCH)
            .map(Version::from_etag)
            .transpose()
            .map_err(Reject::Malformed)?;
        let wait = match seen {
            Some(_) => parts
                .headers
                .get_all(bronze_wire::header::PREFER)
                .iter()
                .filter_map(|value| value.to_str().ok())
                .find_map(wait_preference)
                .unwrap_or_default(),
            None => Duration::ZERO,
        };
        Ok(Self { caller, seen, wait })
    }
}

impl FromRequest<App> for Write {
    type Rejection = Reject;

    async fn from_request(request: Request, app: &App) -> Result<Self, Reject> {
        let (mut parts, body) = request.into_parts();
        let Caller { vault, token } = Caller::from_request_parts(&mut parts, app).await?;
        let precondition = Precondition::from_headers(
            text(&parts.headers, header::IF_MATCH),
            text(&parts.headers, header::IF_NONE_MATCH),
        )
        .map_err(Reject::Malformed)?
        .ok_or(Reject::PreconditionRequired)?;
        let bytes = Bytes::from_request(Request::from_parts(parts, body), app).await.map_err(|rejection| {
            match rejection.status() {
                StatusCode::PAYLOAD_TOO_LARGE => Reject::TooLarge,
                _ => Reject::Malformed(Malformed("body")),
            }
        })?;
        Ok(Self { vault, token, precondition, bytes: bytes.into() })
    }
}

fn text(headers: &HeaderMap, name: impl AsHeaderName) -> Option<&str> {
    headers.get(name)?.to_str().ok()
}

impl IntoResponse for Reject {
    fn into_response(self) -> Response {
        match self {
            Self::NoVault => (StatusCode::NOT_FOUND, "no such vault").into_response(),
            Self::Unauthorized => {
                (StatusCode::UNAUTHORIZED, [(header::WWW_AUTHENTICATE, "Bearer")], "wrong token").into_response()
            }
            Self::Malformed(malformed) => (StatusCode::BAD_REQUEST, malformed.to_string()).into_response(),
            Self::PreconditionRequired => {
                (StatusCode::PRECONDITION_REQUIRED, "send If-Match or If-None-Match: *").into_response()
            }
            Self::PreconditionFailed(Some(current)) => {
                (StatusCode::PRECONDITION_FAILED, [(ETAG, current.etag())], "stale version").into_response()
            }
            Self::PreconditionFailed(None) => (StatusCode::PRECONDITION_FAILED, "no such vault").into_response(),
            Self::TooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "board too large").into_response(),
            Self::VaultLimitReached => {
                (StatusCode::FORBIDDEN, "this server holds all the vaults it allows").into_response()
            }
            Self::Internal(error) => {
                eprintln!("bronze-sync: {error}");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}
