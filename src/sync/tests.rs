use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use bronze_wire::{Precondition, Version, prefer_wait, vault_path};

use super::address::ServerAddress;
use super::engine::{Engine, Event, Halt};
use super::remote::{Pulled, Pushed, Remote};
use super::seal::Seal;
use super::status::{Offline, Refusal};
use super::test_server::TestServer;
use super::*;
use crate::board::{Board, NoteId, Place, Replica, Scope};

const LIVE: Duration = Duration::from_secs(2);
const PATIENCE: Duration = Duration::from_secs(30);

struct Machine {
    board: Board,
    sync: Syncing,
    _folder: tempfile::TempDir,
}

struct Desk {
    server: TestServer,
    wake: Sender<()>,
    woken: Receiver<()>,
}

impl Machine {
    fn with(lines: &[&str]) -> Machine {
        let mut board = Board::default();
        for line in lines {
            board.submit(line);
        }
        let folder = tempfile::tempdir().unwrap();
        Machine { board, sync: Syncing::saved_in(folder.path()), _folder: folder }
    }

    fn edit<R>(&mut self, change: impl FnOnce(&mut Board) -> R) -> R {
        let result = change(&mut self.board);
        self.sync.edited(self.board.replica());
        result
    }

    fn pump(&mut self) {
        for delivery in self.sync.arrived() {
            if let Delivery::Remote(replica) = delivery {
                self.edit(|board| board.join(&replica));
            }
        }
    }

    fn id(&self, text: &str) -> NoteId {
        self.board.notes().find(|note| note.text == text).unwrap_or_else(|| panic!("no note {text:?}")).id
    }

    fn outline(&self) -> Vec<String> {
        let mut outline = Vec::new();
        for list in self.board.lists(Scope::All) {
            outline.extend(list.name.map(|name| format!("# {name}")));
            outline.extend(list.notes.iter().map(|n| format!("[{}] {}", if n.done { 'x' } else { ' ' }, n.text)));
        }
        outline
    }
}

impl Desk {
    fn new() -> Desk {
        Desk::on(TestServer::start())
    }

    fn on(server: TestServer) -> Desk {
        let (wake, woken) = mpsc::channel();
        Desk { server, wake, woken }
    }

    fn start(&self, machine: &mut Machine) -> SyncCode {
        let code = Setup::Start(self.server.address.clone()).code();
        self.join(machine, &code);
        code
    }

    fn join(&self, machine: &mut Machine, code: &SyncCode) {
        let wake = self.wake.clone();
        machine.sync.start(code.clone(), machine.board.replica(), move || _ = wake.send(())).unwrap();
    }

    fn until(
        &self,
        machines: &mut [&mut Machine],
        within: Duration,
        what: &str,
        done: impl Fn(&[&mut Machine]) -> bool,
    ) -> Duration {
        let began = Instant::now();
        loop {
            for machine in machines.iter_mut() {
                machine.pump();
            }
            if done(machines) {
                return began.elapsed();
            }
            let left = within.saturating_sub(began.elapsed());
            if left.is_zero() || self.woken.recv_timeout(left).is_err() {
                let states: Vec<String> =
                    machines.iter().map(|m| format!("{} {:?}", m.sync.status(), m.outline())).collect();
                panic!("{what} within {within:?}; machines: {states:#?}");
            }
        }
    }

    fn converge(&self, machines: &mut [&mut Machine]) -> Vec<String> {
        self.until(machines, PATIENCE, "every machine synced the same notes", |machines| {
            machines.iter().all(|m| m.sync.status() == Status::Synced && m.outline() == machines[0].outline())
        });
        machines[0].outline()
    }

    fn status(&self, machines: &mut [&mut Machine], status: Status) {
        let what = format!("every machine says {status}");
        self.until(machines, PATIENCE, &what, |machines| machines.iter().all(|m| m.sync.status() == status));
    }
}

fn vault(code: &SyncCode) -> (Remote, Seal) {
    let derived = code.key.derive();
    (Remote::new(&code.server, &derived.vault, &derived.token), Seal::new(&derived.vault, &derived.cipher))
}

fn stored(remote: &Remote) -> (Version, Vec<u8>) {
    match remote.pull(None) {
        Ok(Pulled::Snapshot(version, sealed)) => (version, sealed),
        other => panic!("the server holds no board: {other:?}"),
    }
}

fn written_within(code: &SyncCode, seen: Version, wait: Duration) -> bool {
    let derived = code.key.derive();
    let response = ureq::get(code.server.route(&vault_path(&derived.vault)))
        .header("authorization", derived.token.authorization())
        .header("if-none-match", seen.etag())
        .header("prefer", prefer_wait(wait))
        .call()
        .unwrap();
    response.status() != 304
}

fn replace(remote: &Remote, sealed: &[u8]) -> Version {
    let (version, _) = stored(remote);
    match remote.push(Precondition::Replace(version), sealed, || true) {
        Ok(Pushed::Written(version)) => version,
        other => panic!("the server kept its board: {other:?}"),
    }
}

#[test]
fn first_connect_brings_each_machines_notes_to_the_other() {
    let desk = Desk::new();
    let mut a = Machine::with(&["Ask for a rollout plan", "Compare the two cache designs"]);
    let mut b = Machine::with(&["# Work", "Summarize the failing test output"]);
    let code = desk.start(&mut a);
    desk.status(&mut [&mut a], Status::Synced);
    desk.join(&mut b, &code);
    let outline = desk.converge(&mut [&mut a, &mut b]);
    assert_eq!(
        outline,
        [
            "[ ] Ask for a rollout plan",
            "[ ] Compare the two cache designs",
            "# Work",
            "[ ] Summarize the failing test output"
        ]
    );
}

#[test]
fn a_live_edit_arrives_on_the_other_machine_within_two_seconds() {
    let desk = Desk::new();
    let (mut a, mut b) = (Machine::with(&["First"]), Machine::with(&[]));
    let code = desk.start(&mut a);
    desk.join(&mut b, &code);
    desk.converge(&mut [&mut a, &mut b]);
    a.edit(|board| board.capture("Typed on a"));
    let took =
        desk.until(&mut [&mut b], LIVE, "the edit arrived", |b| b[0].board.notes().any(|n| n.text == "Typed on a"));
    eprintln!("a live edit arrived in {took:?}");
    let id = b.id("First");
    b.edit(|board| board.toggle(&[id]));
    desk.until(&mut [&mut a], LIVE, "the check arrived back", |a| a[0].outline()[0] == "[x] First");
}

#[test]
fn edits_made_while_the_server_is_down_meet_when_it_returns() {
    let mut desk = Desk::new();
    let (mut a, mut b) = (Machine::with(&["Shared"]), Machine::with(&[]));
    let code = desk.start(&mut a);
    desk.join(&mut b, &code);
    desk.converge(&mut [&mut a, &mut b]);
    desk.server.stop();
    a.edit(|board| board.capture("Written on a while offline"));
    let shared = b.id("Shared");
    b.edit(|board| board.toggle(&[shared]));
    b.edit(|board| board.capture("Written on b while offline"));
    desk.until(&mut [&mut a, &mut b], PATIENCE, "both machines are offline", |machines| {
        machines.iter().all(|m| matches!(m.sync.status(), Status::Offline(Offline::Unreachable(_))))
    });
    assert_eq!(a.outline(), ["[ ] Shared", "[ ] Written on a while offline"], "a keeps its own edit");
    desk.server.resume();
    let mut outline = desk.converge(&mut [&mut a, &mut b]);
    outline.sort_unstable();
    assert_eq!(outline, ["[ ] Written on a while offline", "[ ] Written on b while offline", "[x] Shared"]);
}

#[test]
fn conflicting_edits_to_the_same_notes_converge() {
    let mut desk = Desk::new();
    let notes = ["One", "Two", "Three", "Four", "Five"];
    let (mut a, mut b) = (Machine::with(&notes), Machine::with(&[]));
    let code = desk.start(&mut a);
    desk.join(&mut b, &code);
    desk.converge(&mut [&mut a, &mut b]);
    desk.server.stop();
    let (one, two, three, four, five) = (a.id("One"), a.id("Two"), a.id("Three"), a.id("Four"), a.id("Five"));
    a.edit(|board| board.toggle(&[one]));
    a.edit(|board| board.move_note(two, Place::Before(five)));
    a.edit(|board| board.merge(&[three, four]));
    b.edit(|board| board.move_note(two, Place::Before(one)));
    b.edit(|board| board.remove(&[one]));
    b.edit(|board| board.toggle(&[three]));
    b.edit(|board| board.remove(&[five]));
    desk.server.resume();
    let outline = desk.converge(&mut [&mut a, &mut b]);
    let mut texts: Vec<&str> = outline.iter().map(|line| &line[4..]).collect();
    texts.sort_unstable();
    assert_eq!(
        texts,
        ["Three\n\nFour", "Two"],
        "a delete beats a check or a move, and a merge folds its notes: {outline:?}"
    );
}

#[test]
fn a_wiped_server_gets_every_note_back_from_the_machines() {
    let mut desk = Desk::new();
    let (mut a, mut b) = (Machine::with(&["Kept by a"]), Machine::with(&["Kept by b"]));
    let code = desk.start(&mut a);
    desk.join(&mut b, &code);
    let before = desk.converge(&mut [&mut a, &mut b]);
    desk.server.wipe();
    a.edit(|board| board.capture("Written after the wipe"));
    let after = desk.converge(&mut [&mut a, &mut b]);
    assert_eq!(after, [&before[..], &["[ ] Written after the wipe".to_owned()]].concat());
    let mut fresh = Machine::with(&[]);
    desk.join(&mut fresh, &code);
    assert_eq!(desk.converge(&mut [&mut a, &mut b, &mut fresh]), after);
}

#[test]
fn a_key_for_another_vault_keeps_its_notes_apart() {
    let desk = Desk::new();
    let (mut a, mut b, mut other) =
        (Machine::with(&["In vault one"]), Machine::with(&[]), Machine::with(&["In vault two"]));
    let one = desk.start(&mut a);
    let two = desk.start(&mut other);
    assert_ne!(one.key, two.key);
    desk.join(&mut b, &one);
    assert_eq!(desk.converge(&mut [&mut a, &mut b]), ["[ ] In vault one"]);
    other.edit(|board| board.capture("Also in vault two"));
    desk.status(&mut [&mut other], Status::Synced);
    a.edit(|board| board.capture("Also in vault one"));
    assert_eq!(desk.converge(&mut [&mut a, &mut b]), ["[ ] In vault one", "[ ] Also in vault one"]);
    assert_eq!(other.outline(), ["[ ] In vault two", "[ ] Also in vault two"]);
}

#[test]
fn machines_move_to_a_new_vault_with_every_note_once() {
    let desk = Desk::new();
    let (mut a, mut b) = (Machine::with(&["# Work", "From a"]), Machine::with(&["From b"]));
    let leaked = desk.start(&mut a);
    desk.join(&mut b, &leaked);
    let before = desk.converge(&mut [&mut a, &mut b]);
    a.sync.stop().unwrap();
    b.sync.stop().unwrap();
    b.edit(|board| board.capture("Written on b between vaults"));
    let fresh = desk.start(&mut a);
    assert_ne!(leaked.key, fresh.key);
    desk.join(&mut b, &fresh);
    let after = desk.converge(&mut [&mut a, &mut b]);
    assert_eq!(after.len(), before.len() + 1, "{after:?}");
    assert!(before.iter().all(|line| after.iter().filter(|other| *other == line).count() == 1), "{after:?}");
}

#[test]
fn a_full_server_declines_a_new_vault_in_its_own_words() {
    let desk = Desk::on(TestServer::holding(0));
    let mut a = Machine::with(&["Stays here"]);
    desk.start(&mut a);
    let declined = Status::Refused(Refusal::Declined("this server holds all the vaults it allows".into()));
    desk.status(&mut [&mut a], declined);
    assert_eq!(a.outline(), ["[ ] Stays here"]);
}

#[test]
fn a_damaged_server_copy_is_never_overwritten() {
    let mut damaged = vec![1];
    damaged.extend([0xd5; 120]);
    unopenable_copy_is_kept(&damaged, Refusal::Damaged);
}

#[test]
fn a_snapshot_from_a_newer_bronze_is_never_overwritten() {
    let mut newer = vec![2];
    newer.extend([0x7e; 120]);
    unopenable_copy_is_kept(&newer, Refusal::NewerFormat);
}

fn unopenable_copy_is_kept(sealed: &[u8], refusal: Refusal) {
    let desk = Desk::new();
    let mut a = Machine::with(&["Before"]);
    let code = desk.start(&mut a);
    desk.status(&mut [&mut a], Status::Synced);
    let (remote, seal) = vault(&code);
    let unopenable = replace(&remote, sealed);
    desk.status(&mut [&mut a], Status::Refused(refusal));
    a.edit(|board| board.capture("Written while refused"));
    assert_eq!(a.outline(), ["[ ] Before", "[ ] Written while refused"], "local edits still apply");
    assert!(!written_within(&code, unopenable, Duration::from_secs(3)), "no machine wrote over the copy");
    let mut elsewhere = Board::default();
    elsewhere.capture("Fixed elsewhere");
    replace(&remote, &seal.seal(elsewhere.replica()));
    let mut fresh = Machine::with(&[]);
    desk.join(&mut fresh, &code);
    let outline = desk.converge(&mut [&mut a, &mut fresh]);
    assert_eq!(outline.len(), 3, "the edit made while refused reached the server: {outline:?}");
}

const GUARD: Duration = Duration::from_secs(10);

struct Wire {
    address: ServerAddress,
    heard: Receiver<Heard>,
    open: usize,
}

enum Heard {
    Opened,
    Request(String, Sender<Vec<u8>>),
    Closed,
}

impl Wire {
    fn listen() -> Wire {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = ServerAddress::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let (tell, heard) = mpsc::channel();
        thread::spawn(move || {
            for stream in listener.incoming().map_while(Result::ok) {
                if tell.send(Heard::Opened).is_err() {
                    return;
                }
                let tell = tell.clone();
                thread::spawn(move || hold(stream, &tell));
            }
        });
        Wire { address, heard, open: 0 }
    }

    fn hear(&mut self) -> Heard {
        let heard = self.heard.recv_timeout(GUARD).expect("the client opened, asked or closed something");
        match heard {
            Heard::Opened => self.open += 1,
            Heard::Closed => self.open -= 1,
            Heard::Request(..) => {}
        }
        heard
    }

    fn request(&mut self) -> (String, Sender<Vec<u8>>) {
        loop {
            if let Heard::Request(head, answer) = self.hear() {
                return (head, answer);
            }
        }
    }

    fn closes_with_no_other_request(&mut self) {
        while self.open > 0 {
            if let Heard::Request(head, _) = self.hear() {
                panic!("a request started after stop: {}", head.lines().next().unwrap_or_default());
            }
        }
    }
}

fn hold(stream: TcpStream, tell: &Sender<Heard>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = stream;
    loop {
        let mut head = String::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                _ = tell.send(Heard::Closed);
                return;
            }
            if line == "\r\n" {
                break;
            }
            head.push_str(&line);
        }
        let length = head
            .lines()
            .find_map(|line| line.to_ascii_lowercase().strip_prefix("content-length:").map(|n| n.trim().to_owned()))
            .map_or(0, |n| n.parse().unwrap_or(0));
        let mut body = vec![0; length];
        if reader.read_exact(&mut body).is_err() {
            _ = tell.send(Heard::Closed);
            return;
        }
        let (answer, answered) = mpsc::channel();
        _ = tell.send(Heard::Request(head, answer));
        match answered.recv() {
            Ok(bytes) if writer.write_all(&bytes).is_ok() => {}
            _ => {
                _ = tell.send(Heard::Closed);
                return;
            }
        }
    }
}

fn answer(status: &str, version: Option<Version>, body: &[u8]) -> Vec<u8> {
    let etag = version.map(|version| format!("etag: {}\r\n", version.etag())).unwrap_or_default();
    let head = format!("HTTP/1.1 {status}\r\n{etag}content-length: {}\r\n\r\n", body.len());
    [head.as_bytes(), body].concat()
}

#[test]
fn a_stop_before_the_first_request_means_no_request_is_ever_made() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = ServerAddress::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let (events, inbox) = mpsc::channel();
    let halt = Halt::default();
    let engine = Engine::new(&Setup::Start(server).code(), Replica::default(), events.clone(), halt.clone(), |_| {});
    halt.halt();
    events.send(Event::Stop).unwrap();
    let (done, finished) = mpsc::channel();
    thread::spawn(move || {
        engine.run(&inbox);
        _ = done.send(());
    });
    let finished = finished.recv_timeout(GUARD).is_ok();
    listener.set_nonblocking(true).unwrap();
    let connected = listener.accept().is_ok();
    assert!(finished && !connected, "stopped before it ran, the engine finished: {finished}, connected: {connected}");
}

#[test]
fn a_stop_while_the_first_get_is_held_lets_no_put_follow() {
    let mut wire = Wire::listen();
    let folder = tempfile::tempdir().unwrap();
    let mut syncing = Syncing::saved_in(folder.path());
    syncing.start(Setup::Start(wire.address.clone()).code(), &Replica::default(), || {}).unwrap();
    let (get, held) = wire.request();
    assert!(get.starts_with("GET "), "{get}");
    syncing.stop().unwrap();
    held.send(answer("404 Not Found", None, b"")).unwrap();
    wire.closes_with_no_other_request();
}

#[test]
fn a_stop_while_the_board_is_being_sealed_lets_no_put_follow() {
    const SEALS_FOR_LONGER_THAN: Duration = Duration::from_millis(5);
    let mut wire = Wire::listen();
    let folder = tempfile::tempdir().unwrap();
    let mut syncing = Syncing::saved_in(folder.path());
    let mut board = Board::seeded(1);
    board.capture("eight mebibytes ".repeat(512 * 1024));
    syncing.start(Setup::Start(wire.address.clone()).code(), board.replica(), || {}).unwrap();
    let (_, held) = wire.request();
    held.send(answer("404 Not Found", None, b"")).unwrap();
    thread::sleep(SEALS_FOR_LONGER_THAN);
    syncing.stop().unwrap();
    wire.closes_with_no_other_request();
}

#[test]
fn a_stop_while_synced_lets_the_long_poll_answer_and_starts_no_other() {
    let mut wire = Wire::listen();
    let folder = tempfile::tempdir().unwrap();
    let mut syncing = Syncing::saved_in(folder.path());
    let code = Setup::Start(wire.address.clone()).code();
    syncing.start(code.clone(), &Replica::default(), || {}).unwrap();
    let (_, held) = wire.request();
    let (_, seal) = vault(&code);
    let version = Version::from_bytes([7; 16]);
    held.send(answer("200 OK", Some(version), &seal.seal(&Replica::default()))).unwrap();
    let (watch, held) = wire.request();
    assert!(watch.to_ascii_lowercase().contains("if-none-match"), "{watch}");
    syncing.stop().unwrap();
    held.send(answer("304 Not Modified", Some(version), b"")).unwrap();
    wire.closes_with_no_other_request();
}
