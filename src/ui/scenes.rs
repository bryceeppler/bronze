use rune::TextEdit;

use super::app::{App, Click, Overlay};
use crate::board::NoteId;

type Scene = (&'static str, fn(&mut App));

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
    ("menu", |s| {
        sections(s);
        s.composer.focused = false;
        s.overlay = Overlay::Menu;
    }),
];

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
