use std::time::Duration;

use bronze_wire::{AuthToken, Precondition, VaultId, Version, prefer_wait, vault_path};
use ureq::Agent;
use ureq::http::{HeaderMap, header};
use ureq::tls::{RootCerts, TlsConfig};

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

    pub fn push(&self, precondition: Precondition, sealed: &[u8]) -> Result<Pushed, Failure> {
        let (name, value) = precondition.header();
        let mut response = self
            .agent
            .put(&self.url)
            .header(header::AUTHORIZATION, &self.authorization)
            .header(name, value)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .config()
            .timeout_global(Some(EXCHANGE))
            .build()
            .send(sealed)
            .map_err(offline)?;
        match (response.status().as_u16(), version(response.headers())) {
            (200..=299, Some(version)) => Ok(Pushed::Written(version)),
            (412, Some(_)) => Ok(Pushed::Stale),
            (412 | 404, _) => Ok(Pushed::Missing),
            (status, _) => Err(failure(status, response.body_mut())),
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
        assert_eq!(remote.push(Precondition::Replace(Version::from_bytes([1; 16])), b"x"), Ok(Pushed::Missing));
        let one = written(remote.push(Precondition::Create, b"one"));
        assert_eq!(remote.push(Precondition::Create, b"again"), Ok(Pushed::Stale));
        assert_eq!(remote.pull(None), Ok(Pulled::Snapshot(one, b"one".to_vec())));
        let two = written(remote.push(Precondition::Replace(one), b"two"));
        assert_eq!(remote.push(Precondition::Replace(one), b"three"), Ok(Pushed::Stale));
        assert_eq!(remote.pull(Some(one)), Ok(Pulled::Snapshot(two, b"two".to_vec())));
        let stranger = Remote::new(&server.address, &vault, &AuthToken::from_bytes([9; 32]));
        assert_eq!(stranger.pull(None), Err(Failure::Refused(Refusal::WrongKey)));
        assert_eq!(stranger.push(Precondition::Replace(two), b"x"), Err(Failure::Refused(Refusal::WrongKey)));
        let oversized = vec![0; bronze_wire::DEFAULT_MAX_BYTES as usize + 1];
        assert_eq!(remote.push(Precondition::Replace(two), &oversized), Err(Failure::Refused(Refusal::TooLarge)));
    }

    #[test]
    fn a_watch_answers_the_next_write_instead_of_its_timeout() {
        let server = TestServer::start();
        let (remote, _) = remote_on(&server);
        let one = written(remote.push(Precondition::Create, b"one"));
        let watcher = remote.clone();
        let began = Instant::now();
        let watch = thread::spawn(move || watcher.pull(Some(one)));
        let two = written(remote.push(Precondition::Replace(one), b"two"));
        assert_eq!(watch.join().unwrap(), Ok(Pulled::Snapshot(two, b"two".to_vec())));
        assert!(began.elapsed() < WATCH / 2, "answered after {:?}", began.elapsed());
    }

    #[test]
    fn a_stopped_server_reads_as_offline() {
        let mut server = TestServer::start();
        let (remote, _) = remote_on(&server);
        server.stop();
        assert!(matches!(remote.pull(None), Err(Failure::Offline(Offline::Unreachable(_)))));
        assert!(matches!(remote.push(Precondition::Create, b"x"), Err(Failure::Offline(Offline::Unreachable(_)))));
    }
}
