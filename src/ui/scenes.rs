use std::rc::Rc;

use rune::{ShortcutError, TextEdit};

use super::app::{App, Click, Overlay, SyncSheet};
use super::commands::Global;
use crate::board::NoteId;
use crate::platform::{Access, Fake};
use crate::sync::{Offline, Refusal, Setup, Status, SyncCode};

type Scene = (&'static str, fn(&mut App));

const SERVER: &str = "https://sync.example.com";
const MADE_UP_CODE: &str =
    "https://sync.example.com#bronze1-000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f-630dcd29";

pub const SCENES: &[Scene] = &[
    ("empty", |_| {}),
    ("notes", notes),
    ("sections", sections),
    ("selected", selected),
    ("search", search),
    ("switcher", |s| {
        sections(s);
        s.composer.focused = false;
        s.open_switcher();
    }),
    ("shortcuts", |s| {
        sections(s);
        s.composer.focused = false;
        s.overlay = Overlay::Shortcuts;
    }),
    ("shortcut-taken", |s| {
        notes(s);
        s.shortcut_unavailable(Global::Capture, ShortcutError::Conflict);
    }),
    ("needs-access", |s| {
        notes(s);
        let platform = Fake::default();
        platform.set_access(Access::Missing);
        s.use_platform(Rc::new(platform));
    }),
    ("menu", |s| {
        sections(s);
        s.composer.focused = false;
        s.overlay = Overlay::Menu;
    }),
    ("menu-synced", |s| {
        notes(s);
        synced(s, Status::Synced);
        s.overlay = Overlay::Menu;
    }),
    ("menu-not-synced", |s| {
        notes(s);
        synced(s, Status::Offline(Offline::Unreachable("connection refused".into())));
        s.overlay = Overlay::Menu;
    }),
    ("sync-off", |s| entered(s, "")),
    ("sync-start", |s| entered(s, SERVER)),
    ("sync-join", |s| entered(s, MADE_UP_CODE)),
    ("sync-not-address", |s| entered(s, "sync.example.com")),
    ("sync-not-secure", |s| entered(s, "http://sync.example.com")),
    ("sync-cut-short", |s| entered(s, &MADE_UP_CODE[..60])),
    ("sync-typo", |s| entered(s, &MADE_UP_CODE.replace("0a0b", "0a0c"))),
    ("sync-new-code", |s| {
        notes(s);
        synced(s, Status::Connecting);
        s.overlay = Overlay::Sync(SyncSheet::NewCode { copied: false });
    }),
    ("sync-synced", |s| linked(s, Status::Synced)),
    ("sync-connecting", |s| linked(s, Status::Connecting)),
    ("sync-unreachable", |s| linked(s, Status::Offline(Offline::Unreachable("connection refused".into())))),
    ("sync-server-error", |s| linked(s, Status::Offline(Offline::ServerError(502)))),
    ("sync-wrong-key", |s| linked(s, Status::Refused(Refusal::WrongKey))),
    ("sync-declined", |s| {
        linked(s, Status::Refused(Refusal::Declined("this server holds all the vaults it allows".into())));
    }),
    ("sync-too-large", |s| linked(s, Status::Refused(Refusal::TooLarge))),
    ("sync-update-bronze", |s| linked(s, Status::Refused(Refusal::NewerFormat))),
    ("sync-damaged", |s| linked(s, Status::Refused(Refusal::Damaged))),
];

fn made_up_code() -> SyncCode {
    match Setup::parse(MADE_UP_CODE) {
        Ok(Setup::Join(code)) => code,
        _ => unreachable!("the made-up sync code parses"),
    }
}

fn synced(s: &mut App, status: Status) {
    s.composer.focused = false;
    s.pretend_sync(status, made_up_code());
}

fn entered(s: &mut App, typed: &str) {
    notes(s);
    s.composer.focused = false;
    s.open_sync();
    s.overlay_field = TextEdit::new(typed);
    s.overlay_field.focused = true;
}

fn linked(s: &mut App, status: Status) {
    notes(s);
    synced(s, status);
    s.open_sync();
}

fn add(s: &mut App, text: &str, done: bool) -> NoteId {
    s.edit(|board| {
        let id = board.capture(text);
        if done {
            board.toggle(&[id]);
        }
        id
    })
}

fn notes(s: &mut App) {
    add(s, "Ask for a **migration plan** before anyone touches the schema", false);
    add(s, "Summarize the failing test output and suggest the smallest fix", true);
    add(s, "Check whether `retry_after` is read in seconds or milliseconds", true);
    add(
        s,
        "Draft release notes from the merged pull requests, grouped by area, and keep each bullet under twelve words so they read well in the changelog and in the announcement email",
        false,
    );
    add(s, "Rename the *staging* bucket once the backups finish", false);
}

fn sections(s: &mut App) {
    add(s, "Reply to the design review thread", false);
    s.edit(|board| board.submit("# Research"));
    add(
        s,
        "**Caching layers.** Compare write-through with read-through for the pricing service, and note where invalidation gets *hard* once regional replicas lag the primary by more than a second.",
        false,
    );
    add(s, "Find out why `snake_case_keys` break the importer", false);
    s.edit(|board| board.submit("# Launch"));
    add(s, "Write the announcement in plain language", true);
    add(s, "Prompt: review the onboarding copy for **jargon** and suggest shorter words", false);
}

fn selected(s: &mut App) {
    sections(s);
    let ids: Vec<NoteId> = s.shown_ids();
    s.click(ids[1], Click::Only);
    s.click(ids[2], Click::Toggle);
    s.composer.focused = false;
}

fn search(s: &mut App) {
    sections(s);
    s.composer.focused = false;
    s.search = TextEdit::new("write");
    s.search.focused = true;
}
