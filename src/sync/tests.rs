use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use bronze_wire::{Precondition, Version, prefer_wait, vault_path};

use super::remote::{Pulled, Pushed, Remote};
use super::seal::Seal;
use super::test_server::TestServer;
use super::*;
use crate::board::{Board, NoteId, Place, Scope};

const LIVE: Duration = Duration::from_secs(2);
const PATIENCE: Duration = Duration::from_secs(30);

struct Machine {
    board: Board,
    sync: Sync,
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
        Machine { board, sync: Sync::saved_in(folder.path()), _folder: folder }
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
    match remote.push(Precondition::Replace(version), sealed) {
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
