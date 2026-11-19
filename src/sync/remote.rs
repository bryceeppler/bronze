use std::io::{self, Read};
use std::time::Duration;

use bronze_wire::{AuthToken, Precondition, VaultId, Version, prefer_wait, vault_path};
use ureq::http::{HeaderMap, header};
use ureq::tls::{RootCerts, TlsConfig};
use ureq::{Agent, SendBody};

use super::address::ServerAddress;
use super::status::{Failure, Offline, Refusal};

pub const WATCH: Duration = Duration::from_secs(25);
const CONNECT: Duration = Duration::from_secs(10);
const EXCHANGE: Duration = Duration::from_secs(60);
const MAX_SNAPSHOT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_SENTENCE_BYTES: u64 = 1024;

#[derive(Clone)]
pub struct Remote {
    agent: Agent,
    url: String,
    authorization: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Pulled {
    Snapshot(Version, Vec<u8>),
    Unchanged,
    Missing,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Pushed {
    Written(Version),
    Stale,
    Missing,
}

impl Remote {
    pub fn new(server: &ServerAddress, vault: &VaultId, token: &AuthToken) -> Remote {
        let agent = Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_connect(Some(CONNECT))
            .tls_config(TlsConfig::builder().root_certs(RootCerts::PlatformVerifier).build())
            .user_agent(concat!("bronze/", env!("CARGO_PKG_VERSION")))
            .build()
            .new_agent();
        Remote { agent, url: server.route(&vault_path(vault)), authorization: token.authorization() }
    }

    pub fn pull(&self, seen: Option<Version>) -> Result<Pulled, Failure> {
        let request = self.agent.get(&self.url).header(header::AUTHORIZATION, &self.authorization);
        let (request, timeout) = match seen {
            Some(version) => (
                request
                    .header(header::IF_NONE_MATCH, version.etag())
                    .header(bronze_wire::header::PREFER, prefer_wait(WATCH)),
                WATCH + EXCHANGE,
            ),
            None => (request, EXCHANGE),
        };
        let mut response = request.config().timeout_global(Some(timeout)).build().call().map_err(offline)?;
        match response.status().as_u16() {
            200 => {
                let version = version(response.headers()).ok_or(Failure::Offline(Offline::Unexpected(200)))?;
                let sealed =
                    response.body_mut().with_config().limit(MAX_SNAPSHOT_BYTES).read_to_vec().map_err(offline)?;
                Ok(Pulled::Snapshot(version, sealed))
            }
            304 => Ok(Pulled::Unchanged),
            404 => Ok(Pulled::Missing),
            status => Err(failure(status, response.body_mut())),
        }
    }

    pub fn push(
        &self,
        precondition: Precondition,
        sealed: &[u8],
        still_wanted: impl Fn() -> bool,
    ) -> Result<Pushed, Failure> {
        let (name, value) = precondition.header();
        let mut body = WhileWanted { bytes: sealed, still_wanted };
        let mut response = self
            .agent
            .put(&self.url)
            .header(header::AUTHORIZATION, &self.authorization)
            .header(name, value)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .header(header::CONTENT_LENGTH, sealed.len())
            .config()
            .timeout_global(Some(EXCHANGE))
            .build()
            .send(SendBody::from_reader(&mut body))
            .map_err(offline)?;
        match (response.status().as_u16(), version(response.headers())) {
            (200..=299, Some(version)) => Ok(Pushed::Written(version)),
            (412, Some(_)) => Ok(Pushed::Stale),
            (412 | 404, _) => Ok(Pushed::Missing),
            (status, _) => Err(failure(status, response.body_mut())),
        }
    }
}

struct WhileWanted<'a, W> {
    bytes: &'a [u8],
    still_wanted: W,
}

impl<W: Fn() -> bool> Read for WhileWanted<'_, W> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if (self.still_wanted)() {
            self.bytes.read(buffer)
        } else {
            Err(io::Error::other("the upload is no longer wanted"))
        }
    }
}

fn version(headers: &HeaderMap) -> Option<Version> {
    Version::from_etag(headers.get(header::ETAG)?.to_str().ok()?).ok()
}

fn failure(status: u16, body: &mut ureq::Body) -> Failure {
    match status {
        401 => Failure::Refused(Refusal::WrongKey),
        403 => {
            let sentence = body.with_config().limit(MAX_SENTENCE_BYTES).read_to_string().unwrap_or_default();
            Failure::Refused(Refusal::Declined(sentence.trim().to_owned()))
        }
        413 => Failure::Refused(Refusal::TooLarge),
        500..=599 => Failure::Offline(Offline::ServerError(status)),
        _ => Failure::Offline(Offline::Unexpected(status)),
    }
}

fn offline(error: ureq::Error) -> Failure {
    Failure::Offline(Offline::Unreachable(error.to_string()))
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Instant;

    use bronze_wire::AuthToken;

    use super::*;
    use crate::sync::Setup;
    use crate::sync::test_server::TestServer;

    fn remote_on(server: &TestServer) -> (Remote, VaultId) {
        let derived = Setup::Start(server.address.clone()).code().key.derive();
        (Remote::new(&server.address, &derived.vault, &derived.token), derived.vault)
    }

    fn written(pushed: Result<Pushed, Failure>) -> Version {
        match pushed {
            Ok(Pushed::Written(version)) => version,
            other => panic!("not written: {other:?}"),
        }
    }

    #[test]
    fn the_client_reads_every_answer_the_wire_gives_it() {
        let server = TestServer::start();
        let (remote, vault) = remote_on(&server);
        assert_eq!(remote.pull(None), Ok(Pulled::Missing));
        assert_eq!(
            remote.push(Precondition::Replace(Version::from_bytes([1; 16])), b"x", || true),
            Ok(Pushed::Missing)
        );
        let one = written(remote.push(Precondition::Create, b"one", || true));
        assert_eq!(remote.push(Precondition::Create, b"again", || true), Ok(Pushed::Stale));
        assert_eq!(remote.pull(None), Ok(Pulled::Snapshot(one, b"one".to_vec())));
        let two = written(remote.push(Precondition::Replace(one), b"two", || true));
        assert_eq!(remote.push(Precondition::Replace(one), b"three", || true), Ok(Pushed::Stale));
        assert_eq!(remote.pull(Some(one)), Ok(Pulled::Snapshot(two, b"two".to_vec())));
        let stranger = Remote::new(&server.address, &vault, &AuthToken::from_bytes([9; 32]));
        assert_eq!(stranger.pull(None), Err(Failure::Refused(Refusal::WrongKey)));
        assert_eq!(stranger.push(Precondition::Replace(two), b"x", || true), Err(Failure::Refused(Refusal::WrongKey)));
        let oversized = vec![0; bronze_wire::DEFAULT_MAX_BYTES as usize + 1];
        assert_eq!(
            remote.push(Precondition::Replace(two), &oversized, || true),
            Err(Failure::Refused(Refusal::TooLarge))
        );
    }

    #[test]
    fn a_watch_answers_the_next_write_instead_of_its_timeout() {
        let server = TestServer::start();
        let (remote, _) = remote_on(&server);
        let one = written(remote.push(Precondition::Create, b"one", || true));
        let watcher = remote.clone();
        let began = Instant::now();
        let watch = thread::spawn(move || watcher.pull(Some(one)));
        let two = written(remote.push(Precondition::Replace(one), b"two", || true));
        assert_eq!(watch.join().unwrap(), Ok(Pulled::Snapshot(two, b"two".to_vec())));
        assert!(began.elapsed() < WATCH / 2, "answered after {:?}", began.elapsed());
    }

    #[test]
    fn an_upload_nobody_wants_any_more_never_reaches_the_server() {
        let server = TestServer::start();
        let (remote, _) = remote_on(&server);
        assert!(matches!(remote.push(Precondition::Create, b"board", || false), Err(Failure::Offline(_))));
        assert_eq!(remote.pull(None), Ok(Pulled::Missing));
    }

    #[test]
    fn an_upload_is_abandoned_part_way_once_it_is_no_longer_wanted() {
        const BOARD_BYTES: usize = 64 * 1024 * 1024;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = ServerAddress::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let derived = Setup::Start(address.clone()).code().key.derive();
        let remote = Remote::new(&address, &derived.vault, &derived.token);
        let (head_arrived, on_head) = mpsc::channel();
        let (read_on, may_read_on) = mpsc::channel::<()>();
        let receiving = thread::spawn(move || {
            let mut stream = BufReader::new(listener.accept().unwrap().0);
            let mut line = String::new();
            while stream.read_line(&mut line).unwrap() > 2 {
                line.clear();
            }
            head_arrived.send(()).unwrap();
            may_read_on.recv().unwrap();
            let mut received = 0;
            while let Ok(read @ 1..) = stream.read(&mut [0; 64 * 1024]) {
                received += read;
            }
            received
        });
        let wanted = Arc::new(AtomicBool::new(true));
        let still_wanted = wanted.clone();
        let uploading = thread::spawn(move || {
            remote.push(Precondition::Create, &vec![0; BOARD_BYTES], || still_wanted.load(Ordering::SeqCst))
        });
        on_head.recv().unwrap();
        wanted.store(false, Ordering::SeqCst);
        read_on.send(()).unwrap();
        assert!(matches!(uploading.join().unwrap(), Err(Failure::Offline(_))));
        assert!(receiving.join().unwrap() < BOARD_BYTES);
    }

    #[test]
    fn a_stopped_server_reads_as_offline() {
        let mut server = TestServer::start();
        let (remote, _) = remote_on(&server);
        server.stop();
        assert!(matches!(remote.pull(None), Err(Failure::Offline(Offline::Unreachable(_)))));
        assert!(matches!(
            remote.push(Precondition::Create, b"x", || true),
            Err(Failure::Offline(Offline::Unreachable(_)))
        ));
    }
}
