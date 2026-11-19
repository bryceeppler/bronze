use std::collections::HashMap;
use std::thread;

use futures_util::StreamExt;
use rune::ShortcutError;
use zbus::{Connection, OwnedObjectPath, OwnedVariant, Proxy, Variant, fdo};

use super::{OnShortcut, ShortcutEvent};
use crate::ui::Global;

const DEST: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const GLOBAL_SHORTCUTS: &str = "org.freedesktop.portal.GlobalShortcuts";

type Results = HashMap<String, OwnedVariant>;

pub fn bind(on: OnShortcut) {
    thread::Builder::new()
        .name("bronze-shortcut-portal".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .build()
                .expect("a runtime for the shortcut portal");
            if let Err(error) = runtime.block_on(serve(&on)) {
                for global in Global::ALL {
                    on(ShortcutEvent::Refused(global, error.clone()));
                }
            }
        })
        .expect("spawn the shortcut portal thread");
}

async fn serve(on: &OnShortcut) -> Result<(), ShortcutError> {
    let connection = Connection::session().await.map_err(|_| ShortcutError::Unsupported)?;
    register_app_id(&connection).await;
    let portal = Proxy::new(&connection, DEST, PATH, GLOBAL_SHORTCUTS).await.map_err(refusal)?;
    portal.get_property::<u32>("version").await.map_err(|_| ShortcutError::Unsupported)?;
    let mut presses = portal.receive_signal("Activated").await.map_err(refusal)?;

    let options = HashMap::from([
        ("handle_token", Variant::from("bronze_create")),
        ("session_handle_token", Variant::from("bronze")),
    ]);
    let created = request(&connection, &portal, "bronze_create", "CreateSession", &(options,)).await?;
    let session = session_handle(&created)?;

    let wanted: Vec<(&str, HashMap<&str, Variant>)> = Global::ALL
        .iter()
        .map(|global| {
            let trigger = Variant::from(trigger(*global));
            (
                global.id(),
                HashMap::from([("description", Variant::from(global.what())), ("preferred_trigger", trigger)]),
            )
        })
        .collect();
    let options = HashMap::from([("handle_token", Variant::from("bronze_bind"))]);
    let bound = request(&connection, &portal, "bronze_bind", "BindShortcuts", &(&session, wanted, "", options)).await?;
    let bound: Vec<(String, Results)> = bound
        .get("shortcuts")
        .and_then(|shortcuts| shortcuts.try_clone().ok())
        .and_then(|shortcuts| shortcuts.try_into().ok())
        .unwrap_or_default();
    for global in Global::ALL.into_iter().filter(|global| bound.iter().all(|(id, _)| id != global.id())) {
        on(ShortcutEvent::Refused(global, ShortcutError::PermissionDenied));
    }

    while let Some(press) = presses.next().await {
        let Ok((from, id, ..)) = press.body().deserialize::<(OwnedObjectPath, String, u64, Results)>() else {
            continue;
        };
        if from == session
            && let Some(global) = Global::from_id(&id)
        {
            on(ShortcutEvent::Pressed(global));
        }
    }
    Ok(())
}

async fn register_app_id(connection: &Connection) {
    let registered = match Proxy::new(connection, DEST, PATH, "org.freedesktop.host.portal.Registry").await {
        Ok(registry) => registry.call::<_, _, ()>("Register", &(crate::APP_ID, HashMap::<&str, Variant>::new())).await,
        Err(error) => Err(error),
    };
    if let Err(error) = registered {
        eprintln!("bronze: the desktop portal did not take Bronze's app id: {error}");
    }
}

fn trigger(global: Global) -> String {
    let (modifiers, key) = global.chord().rsplit_once('+').unwrap_or(("", global.chord()));
    modifiers.split('+').filter(|m| !m.is_empty()).map(|m| m.to_uppercase() + "+").collect::<String>() + key
}

async fn request<B>(
    connection: &Connection,
    portal: &Proxy<'_>,
    token: &str,
    method: &str,
    body: &B,
) -> Result<Results, ShortcutError>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    let sender = connection.unique_name().ok_or(ShortcutError::Unsupported)?.trim_start_matches(':').replace('.', "_");
    let path = format!("{PATH}/request/{sender}/{token}");
    let request =
        Proxy::new(connection, DEST, path.as_str(), "org.freedesktop.portal.Request").await.map_err(refusal)?;
    let mut responses = request.receive_signal("Response").await.map_err(refusal)?;
    let _: OwnedObjectPath = portal.call(method, body).await.map_err(refusal)?;
    let response = responses.next().await.ok_or_else(|| ShortcutError::Backend("the portal went away".into()))?;
    let (status, results): (u32, Results) = response.body().deserialize().map_err(refusal)?;
    match status {
        0 => Ok(results),
        1 => Err(ShortcutError::PermissionDenied),
        other => Err(ShortcutError::Backend(format!("the portal answered {other}"))),
    }
}

fn session_handle(created: &Results) -> Result<OwnedObjectPath, ShortcutError> {
    let handle = created.get("session_handle").ok_or_else(|| ShortcutError::Backend("no session handle".into()))?;
    let object_path = || handle.try_clone().ok().and_then(|handle| OwnedObjectPath::try_from(handle).ok());
    match <&str>::try_from(handle) {
        Ok(path) => OwnedObjectPath::try_from(path).ok(),
        Err(_) => object_path(),
    }
    .ok_or_else(|| ShortcutError::Backend("the session handle is not a path".into()))
}

fn refusal(error: zbus::Error) -> ShortcutError {
    match &error {
        zbus::Error::FDO(fdo)
            if matches!(
                **fdo,
                fdo::Error::ServiceUnknown(_) | fdo::Error::UnknownInterface(_) | fdo::Error::UnknownMethod(_)
            ) =>
        {
            ShortcutError::Unsupported
        }
        zbus::Error::MethodError(name, ..)
            if matches!(
                name.as_str(),
                "org.freedesktop.portal.Error.NotAllowed" | "org.freedesktop.portal.Error.Cancelled"
            ) =>
        {
            ShortcutError::PermissionDenied
        }
        _ => ShortcutError::Backend(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::trigger;
    use crate::ui::Global;

    #[test]
    fn the_desktop_portal_is_asked_for_the_same_keys_as_the_chords() {
        assert_eq!(trigger(Global::Capture), "CTRL+ALT+c");
        assert_eq!(trigger(Global::ShowOrHide), "CTRL+ALT+space");
    }
}
