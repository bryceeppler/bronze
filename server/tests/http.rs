use std::path::Path;
use std::time::{Duration, Instant};

use bronze_sync::{Limits, Store, serve};
use reqwest::header::{AUTHORIZATION, ETAG, IF_MATCH, IF_NONE_MATCH, WWW_AUTHENTICATE};
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use tempfile::TempDir;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

struct Server {
    base: String,
    stop: oneshot::Sender<()>,
    task: JoinHandle<std::io::Result<()>>,
}

impl Server {
    async fn start(data: &Path, limits: Limits) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let store = Store::open(data).unwrap();
        let (stop, stopped) = oneshot::channel::<()>();
        let task = tokio::spawn(serve(listener, store, limits, async {
            let _ = stopped.await;
        }));
        Self { base, stop, task }
    }

    async fn stop(self) {
        self.stop.send(()).unwrap();
        self.task.await.unwrap().unwrap();
    }

    fn vault(&self) -> Vault {
        self.open(&random_hex(), random_hex())
    }

    fn open(&self, id: &str, token: String) -> Vault {
        Vault { id: id.to_owned(), url: format!("{}/v1/vaults/{id}", self.base), token, client: Client::new() }
    }
}

struct Vault {
    id: String,
    url: String,
    token: String,
    client: Client,
}

impl Vault {
    fn get(&self) -> RequestBuilder {
        self.client.get(&self.url).header(AUTHORIZATION, format!("Bearer {}", self.token))
    }

    fn put(&self) -> RequestBuilder {
        self.client.put(&self.url).header(AUTHORIZATION, format!("Bearer {}", self.token))
    }

    async fn create(&self, body: &'static str) -> String {
        let created = self.put().header(IF_NONE_MATCH, "*").body(body).send().await.unwrap();
        assert_eq!(created.status(), StatusCode::CREATED);
        etag(&created).unwrap()
    }

    async fn replace(&self, version: &str, body: &'static str) -> Response {
        self.put().header(IF_MATCH, version).body(body).send().await.unwrap()
    }

    async fn poll(&self, seen: &str, wait: &str) -> Response {
        self.get().header(IF_NONE_MATCH, seen).header("Prefer", wait).send().await.unwrap()
    }

    async fn body(&self) -> (Option<String>, String) {
        let read = self.get().send().await.unwrap();
        assert_eq!(read.status(), StatusCode::OK);
        (etag(&read), read.text().await.unwrap())
    }
}

fn random_hex() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).unwrap();
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn etag(response: &Response) -> Option<String> {
    Some(response.headers().get(ETAG)?.to_str().unwrap().to_owned())
}

fn busy_store(data: &Path) -> rusqlite::Connection {
    let connection = rusqlite::Connection::open(data.join("bronze-sync.sqlite3")).unwrap();
    connection.execute_batch("BEGIN IMMEDIATE").unwrap();
    connection
}

async fn started(limits: Limits) -> (TempDir, Server) {
    let data = tempfile::tempdir().unwrap();
    let server = Server::start(data.path(), limits).await;
    (data, server)
}

#[tokio::test]
async fn create_then_replace_and_refuse_a_stale_version() {
    let (_data, server) = started(Limits::default()).await;
    let vault = server.vault();
    let first = vault.create("one").await;
    assert_eq!(vault.body().await, (Some(first.clone()), "one".to_owned()));

    let replaced = vault.replace(&first, "two").await;
    assert_eq!(replaced.status(), StatusCode::NO_CONTENT);
    let second = etag(&replaced).unwrap();
    assert_ne!(second, first);

    let stale = vault.replace(&first, "three").await;
    assert_eq!((stale.status(), etag(&stale)), (StatusCode::PRECONDITION_FAILED, Some(second.clone())));
    let recreate = vault.put().header(IF_NONE_MATCH, "*").body("four").send().await.unwrap();
    assert_eq!((recreate.status(), etag(&recreate)), (StatusCode::PRECONDITION_FAILED, Some(second.clone())));
    assert_eq!(vault.body().await, (Some(second), "two".to_owned()));
    server.stop().await;
}

#[tokio::test]
async fn a_write_without_a_precondition_is_428() {
    let (_data, server) = started(Limits::default()).await;
    let vault = server.vault();
    assert_eq!(vault.put().body("one").send().await.unwrap().status(), StatusCode::PRECONDITION_REQUIRED);
    let first = vault.create("one").await;
    assert_eq!(vault.put().body("two").send().await.unwrap().status(), StatusCode::PRECONDITION_REQUIRED);
    assert_eq!(vault.body().await, (Some(first), "one".to_owned()));
    server.stop().await;
}

#[tokio::test]
async fn a_board_over_the_size_cap_is_413() {
    let (_data, server) = started(Limits { max_bytes: 16, ..Limits::default() }).await;
    let vault = server.vault();
    let too_large = vault.put().header(IF_NONE_MATCH, "*").body("x".repeat(17)).send().await.unwrap();
    assert_eq!(too_large.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(vault.get().send().await.unwrap().status(), StatusCode::NOT_FOUND);

    let first = vault.create("sixteen  bytes!!").await;
    assert_eq!(vault.replace(&first, "seventeen bytes!!").await.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(vault.body().await, (Some(first), "sixteen  bytes!!".to_owned()));
    server.stop().await;
}

#[tokio::test]
async fn a_wrong_or_missing_token_is_401() {
    let (_data, server) = started(Limits::default()).await;
    let vault = server.vault();
    let first = vault.create("one").await;
    let intruder = server.open(&vault.id, random_hex());

    let read = intruder.get().send().await.unwrap();
    assert_eq!(read.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(read.headers()[WWW_AUTHENTICATE], "Bearer");
    assert_eq!(intruder.replace(&first, "mine").await.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(intruder.put().header(IF_NONE_MATCH, "*").send().await.unwrap().status(), StatusCode::UNAUTHORIZED);
    assert_eq!(Client::new().get(&vault.url).send().await.unwrap().status(), StatusCode::UNAUTHORIZED);
    assert_eq!(vault.body().await, (Some(first), "one".to_owned()));
    server.stop().await;
}

#[tokio::test]
async fn an_unknown_vault_is_404_and_replacing_it_is_412() {
    let (_data, server) = started(Limits::default()).await;
    let vault = server.vault();
    assert_eq!(vault.get().send().await.unwrap().status(), StatusCode::NOT_FOUND);
    let replace = vault.replace("\"00000000000000000000000000000000\"", "one").await;
    assert_eq!((replace.status(), etag(&replace)), (StatusCode::PRECONDITION_FAILED, None));
    let uppercase = server.open(&vault.id.to_uppercase(), vault.token.clone());
    assert_eq!(uppercase.get().send().await.unwrap().status(), StatusCode::NOT_FOUND);
    server.stop().await;
}

#[tokio::test]
async fn the_vault_cap_refuses_new_vaults_only() {
    let (_data, server) = started(Limits { max_vaults: 2, ..Limits::default() }).await;
    let first = server.vault();
    let version = first.create("one").await;
    server.vault().create("two").await;
    let third = server.vault();
    assert_eq!(third.put().header(IF_NONE_MATCH, "*").send().await.unwrap().status(), StatusCode::FORBIDDEN);
    assert_eq!(third.get().send().await.unwrap().status(), StatusCode::NOT_FOUND);
    assert_eq!(first.replace(&version, "three").await.status(), StatusCode::NO_CONTENT);
    server.stop().await;
}

#[tokio::test]
async fn bytes_survive_a_restart() {
    let data = tempfile::tempdir().unwrap();
    let server = Server::start(data.path(), Limits::default()).await;
    let vault = server.vault();
    let version = vault.create("kept").await;
    server.stop().await;

    let server = Server::start(data.path(), Limits::default()).await;
    let vault = server.open(&vault.id, vault.token);
    assert_eq!(vault.body().await, (Some(version), "kept".to_owned()));
    server.stop().await;
}

#[tokio::test]
async fn a_wiped_server_lets_a_machine_create_its_vault_again() {
    let data = tempfile::tempdir().unwrap();
    let server = Server::start(data.path(), Limits { max_vaults: 1, ..Limits::default() }).await;
    let vault = server.vault();
    let lost = vault.create("before").await;
    server.stop().await;
    std::fs::remove_dir_all(data.path()).unwrap();

    let server = Server::start(data.path(), Limits { max_vaults: 1, ..Limits::default() }).await;
    let vault = server.open(&vault.id, vault.token);
    assert_eq!(vault.get().send().await.unwrap().status(), StatusCode::NOT_FOUND);
    let stale = vault.replace(&lost, "after").await;
    assert_eq!((stale.status(), etag(&stale)), (StatusCode::PRECONDITION_FAILED, None));
    let created = vault.create("after").await;
    assert_eq!(vault.body().await, (Some(created), "after".to_owned()));
    server.stop().await;
}

#[tokio::test]
async fn a_long_poll_answers_at_once_when_the_version_differs() {
    let (_data, server) = started(Limits::default()).await;
    let vault = server.vault();
    let first = vault.create("one").await;
    let second = etag(&vault.replace(&first, "two").await).unwrap();

    let started = Instant::now();
    let poll = vault.poll(&first, "wait=55").await;
    assert_eq!((poll.status(), etag(&poll)), (StatusCode::OK, Some(second)));
    assert_eq!(poll.text().await.unwrap(), "two");
    assert!(started.elapsed() < Duration::from_secs(1), "{:?}", started.elapsed());
    server.stop().await;
}

#[tokio::test]
async fn a_long_poll_wakes_on_a_write() {
    let (_data, server) = started(Limits::default()).await;
    let vault = server.vault();
    let first = vault.create("one").await;
    let parked = {
        let (get, first) = (vault.get(), first.clone());
        tokio::spawn(async move { get.header(IF_NONE_MATCH, first).header("Prefer", "wait=55").send().await.unwrap() })
    };
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!parked.is_finished());

    let written = Instant::now();
    let second = etag(&vault.replace(&first, "two").await).unwrap();
    let woke = parked.await.unwrap();
    assert!(written.elapsed() < Duration::from_secs(1), "{:?}", written.elapsed());
    assert_eq!((woke.status(), etag(&woke)), (StatusCode::OK, Some(second)));
    assert_eq!(woke.text().await.unwrap(), "two");
    server.stop().await;
}

#[tokio::test]
async fn a_long_poll_answers_304_at_its_timeout() {
    let (_data, server) = started(Limits::default()).await;
    let vault = server.vault();
    let first = vault.create("one").await;

    let started = Instant::now();
    let poll = vault.poll(&first, "wait=1").await;
    let waited = started.elapsed();
    assert_eq!((poll.status(), etag(&poll)), (StatusCode::NOT_MODIFIED, Some(first.clone())));
    assert!(waited >= Duration::from_secs(1) && waited < Duration::from_secs(2), "{waited:?}");

    let started = Instant::now();
    let unchanged = vault.get().header(IF_NONE_MATCH, &first).send().await.unwrap();
    assert_eq!(unchanged.status(), StatusCode::NOT_MODIFIED);
    assert!(started.elapsed() < Duration::from_secs(1));
    server.stop().await;
}

#[tokio::test]
async fn a_write_whose_writer_hangs_up_still_wakes_parked_long_polls() {
    let (data, server) = started(Limits::default()).await;
    let vault = server.vault();
    let first = vault.create("one").await;
    let parked = {
        let get = vault.get().header(IF_NONE_MATCH, &first).header("Prefer", "wait=3");
        tokio::spawn(async move {
            let woke = get.send().await.unwrap();
            (woke.status(), etag(&woke), woke.text().await.unwrap())
        })
    };
    tokio::time::sleep(Duration::from_millis(300)).await;

    let busy = busy_store(data.path());
    let mut writer = TcpStream::connect(server.base.trim_start_matches("http://")).await.unwrap();
    let put = format!(
        "PUT /v1/vaults/{} HTTP/1.1\r\nHost: bronze\r\nAuthorization: Bearer {}\r\nIf-Match: {first}\r\nContent-Length: 3\r\n\r\ntwo",
        vault.id, vault.token
    );
    writer.write_all(put.as_bytes()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    writer.set_zero_linger().unwrap();
    drop(writer);
    tokio::time::sleep(Duration::from_millis(200)).await;
    busy.execute_batch("ROLLBACK").unwrap();
    drop(busy);

    let woke = parked.await.unwrap();
    let (current, body) = vault.body().await;
    assert_eq!(body, "two");
    assert_eq!(woke, (StatusCode::OK, current, "two".to_owned()));
    server.stop().await;
}

#[tokio::test]
async fn a_long_poll_answers_by_its_deadline_while_the_store_is_busy() {
    let (data, server) = started(Limits::default()).await;
    let vault = server.vault();
    let first = vault.create("one").await;
    let other = server.vault();
    let other_first = other.create("other").await;

    let busy = busy_store(data.path());
    let stuck_write = tokio::spawn(other.put().header(IF_MATCH, &other_first).body("two").send());
    tokio::time::sleep(Duration::from_millis(200)).await;
    let poll = tokio::spawn(vault.get().header(IF_NONE_MATCH, &first).header("Prefer", "wait=1").send());
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let answered_in_time = poll.is_finished();
    busy.execute_batch("ROLLBACK").unwrap();
    drop(busy);

    let poll = poll.await.unwrap().unwrap();
    assert!(answered_in_time, "a wait=1 poll was still pending 2.5s later");
    assert_eq!((poll.status(), etag(&poll)), (StatusCode::NOT_MODIFIED, Some(first)));
    assert_eq!(stuck_write.await.unwrap().unwrap().status(), StatusCode::NO_CONTENT);
    server.stop().await;
}

#[tokio::test]
async fn stopping_the_server_ends_parked_long_polls() {
    let (_data, server) = started(Limits::default()).await;
    let vault = server.vault();
    let first = vault.create("one").await;
    let parked = {
        let get = vault.get().header(IF_NONE_MATCH, &first).header("Prefer", "wait=55");
        tokio::spawn(async move { get.send().await.unwrap().status() })
    };
    tokio::time::sleep(Duration::from_millis(300)).await;

    let stopping = Instant::now();
    server.stop().await;
    assert_eq!(parked.await.unwrap(), StatusCode::NOT_MODIFIED);
    assert!(stopping.elapsed() < Duration::from_secs(1), "{:?}", stopping.elapsed());
}

#[tokio::test(flavor = "multi_thread")]
async fn fifty_long_polls_on_one_vault_all_wake_within_a_second_of_a_write() {
    let (_data, server) = started(Limits::default()).await;
    let vault = server.vault();
    let first = vault.create("one").await;
    let polls: Vec<_> = (0..50)
        .map(|_| {
            let get = Client::new().get(&vault.url).header(AUTHORIZATION, format!("Bearer {}", vault.token));
            let get = get.header(IF_NONE_MATCH, &first).header("Prefer", "wait=55");
            tokio::spawn(async move {
                let woke = get.send().await.unwrap();
                (Instant::now(), woke.status(), woke.text().await.unwrap())
            })
        })
        .collect();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(polls.iter().all(|poll| !poll.is_finished()));

    let written = Instant::now();
    assert_eq!(vault.replace(&first, "two").await.status(), StatusCode::NO_CONTENT);
    let mut slowest = Duration::ZERO;
    for poll in polls {
        let (woke, status, body) = poll.await.unwrap();
        assert_eq!((status, body.as_str()), (StatusCode::OK, "two"));
        slowest = slowest.max(woke - written);
    }
    println!("slowest of 50 long-polls woke {slowest:?} after the write began");
    assert!(slowest < Duration::from_secs(1), "{slowest:?}");
    server.stop().await;
}
