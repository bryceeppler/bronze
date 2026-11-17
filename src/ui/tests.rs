use std::rc::Rc;

use rune::clipboard::{self, MemoryClipboard};
use rune::{AppSpec, Env, Key, KeyEvent, Mods, PhysicalInput, Runtime};

use super::app::{Panel as Shown, card_key};
use super::commands::Global;
use super::{App, Saved, Window, on_key, view};
use crate::board::{Board, NoteId, Scope};
use crate::platform::Fake;
use crate::prefs::Prefs;
use crate::store::{self, Store};

struct Panel {
    rt: Runtime<App>,
    clipboard: MemoryClipboard,
    platform: Rc<Fake>,
}

impl Panel {
    fn new() -> Panel {
        Panel::with(Saved {
            board: Board::default(),
            notes: Store::Memory,
            prefs: Prefs::default(),
            preferences: Store::Memory,
        })
    }

    fn with(saved: Saved) -> Panel {
        let platform = Rc::new(Fake::default());
        let unbound = rune::App::new(AppSpec::new("Bronze", (360.0, 640.0), || unreachable!(), view)).handle();
        let mut rt = Runtime::new(
            App::new(saved, platform.clone(), Window::new(unbound)),
            view,
            (360.0, 640.0),
            Env { system_dark: false, scale: 2.0 },
        );
        rt.on_key = Some(on_key);
        rt.poll = Some(|s, clock, tx| s.poll(clock, tx));
        let clipboard = clipboard::memory();
        rt.use_clipboard(&clipboard);
        let mut panel = Panel { rt, clipboard, platform };
        panel.settle();
        panel
    }

    fn press(&mut self, global: Global) {
        self.rt.update(|s, tx| s.global(global, tx));
        self.settle();
    }

    fn settle(&mut self) {
        for _ in 0..30 {
            self.rt.frame(1.0 / 60.0);
        }
    }

    fn key(&mut self, key: Key, mods: Mods) {
        self.rt.key(KeyEvent { key, mods });
        self.settle();
    }

    fn command(&mut self, c: char) {
        self.key(Key::Command(c), Mods { cmd: true, ..Mods::default() });
    }

    fn type_text(&mut self, text: &str) {
        self.key(Key::Text(text.into()), Mods::default());
    }

    fn submit(&mut self, text: &str) {
        self.type_text(text);
        self.key(Key::Enter, Mods::default());
    }

    fn at(&self, key: &str) -> (f32, f32) {
        self.rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"))
    }

    fn with_text(&self, text: &str) -> (f32, f32) {
        let info = self
            .rt
            .elements()
            .into_iter()
            .find(|e| e.text.as_deref() == Some(text))
            .unwrap_or_else(|| panic!("no element reads {text:?}"));
        (info.rect[0] + info.rect[2] / 2.0, info.rect[1] + info.rect[3] / 2.0)
    }

    fn menu_item(&self, label: &str) -> (f32, f32) {
        let info = self
            .rt
            .elements()
            .into_iter()
            .find(|e| {
                e.key.as_deref().is_some_and(|k| k.starts_with("overflow-item")) && e.label.as_deref() == Some(label)
            })
            .unwrap_or_else(|| panic!("no menu item {label:?}"));
        (info.rect[0] + info.rect[2] / 2.0, info.rect[1] + info.rect[3] / 2.0)
    }

    fn click(&mut self, (x, y): (f32, f32), mods: Mods) {
        self.rt.physical_input(PhysicalInput::Modifiers(mods));
        self.rt.pointer_down(x, y);
        self.rt.pointer_up(x, y);
        self.settle();
        self.rt.physical_input(PhysicalInput::Modifiers(Mods::default()));
    }

    fn secondary_click(&mut self, (x, y): (f32, f32)) {
        self.rt.secondary_down(x, y);
        self.rt.secondary_up(x, y);
        self.settle();
    }

    fn drag(&mut self, from: (f32, f32), to: (f32, f32)) {
        self.rt.pointer_down(from.0, from.1);
        for step in 1..=12 {
            let t = step as f32 / 12.0;
            self.rt.pointer_move(from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
            self.rt.frame(1.0 / 60.0);
        }
        self.rt.pointer_up(to.0, to.1);
        self.settle();
    }

    fn board(&self) -> &Board {
        self.rt.state.board()
    }

    fn id(&self, text: &str) -> NoteId {
        self.board().notes().find(|n| n.text == text).map(|n| n.id).unwrap_or_else(|| panic!("no note {text:?}"))
    }

    fn texts(&self) -> Vec<String> {
        self.board().notes().map(|n| n.text.clone()).collect()
    }
}

const PLAIN: Mods = Mods { shift: false, cmd: false, alt: false, ctrl: false, logo: false };
const SHIFT: Mods = Mods { shift: true, ..PLAIN };
const CMD: Mods = Mods { cmd: true, ..PLAIN };

#[test]
fn the_core_loop_captures_sections_copies_merges_and_clears() {
    let mut p = Panel::new();
    p.submit("# Research");
    p.submit("Compare the two cache designs");
    p.submit("Ask for a rollout plan");
    let (first, second) = (p.id("Compare the two cache designs"), p.id("Ask for a rollout plan"));

    p.click(p.at(&format!("{}/check", card_key(second))), PLAIN);
    assert!(p.board().notes().find(|n| n.id == second).is_some_and(|n| n.done));

    p.command('k');
    p.key(Key::Enter, PLAIN);
    assert_eq!(p.rt.state.scope(), Scope::All, "Show All is the switcher's first entry");

    p.click(p.at(&card_key(first)), PLAIN);
    p.click(p.at(&card_key(second)), SHIFT);
    p.command('c');
    assert_eq!(
        p.clipboard.text().as_deref().map(String::as_str),
        Some("Compare the two cache designs\n\nAsk for a rollout plan")
    );

    p.command('j');
    assert_eq!(p.texts(), ["Compare the two cache designs\n\nAsk for a rollout plan"]);

    p.click(p.at("more"), PLAIN);
    p.click(p.with_text("Clear Done"), PLAIN);
    let research = &p.board().sections()[0];
    assert_eq!(research.name, "Research");
    assert_eq!(research.notes.len(), 1, "the merged note is open, so Clear Done keeps it");
    assert_eq!(p.board().active().map(|s| s.id), Some(research.id));
}

#[test]
fn shift_enter_writes_a_newline_and_enter_submits() {
    let mut p = Panel::new();
    p.type_text("first line");
    p.key(Key::Enter, SHIFT);
    p.submit("second line");
    assert_eq!(p.texts(), ["first line\nsecond line"]);
    assert!(p.rt.state.composer.text.is_empty());
}

#[test]
fn selection_keys_toggle_and_delete_and_escape_clears() {
    let mut p = Panel::new();
    for text in ["one", "two", "three"] {
        p.submit(text);
    }
    p.click(p.at(&card_key(p.id("one"))), PLAIN);
    p.click(p.at(&card_key(p.id("three"))), CMD);
    p.key(Key::Text(" ".into()), PLAIN);
    let done: Vec<_> = p.board().notes().filter(|n| n.done).map(|n| n.text.clone()).collect();
    assert_eq!(done, ["one", "three"]);

    p.key(Key::Backspace, PLAIN);
    assert_eq!(p.texts(), ["two"]);

    p.click(p.at(&card_key(p.id("two"))), PLAIN);
    p.key(Key::Escape, PLAIN);
    assert!(p.rt.state.selected().is_empty());
    p.key(Key::Delete, PLAIN);
    assert_eq!(p.texts(), ["two"], "nothing selected, nothing deleted");
}

#[test]
fn dragging_a_note_reorders_it_and_moves_it_into_a_section() {
    let mut p = Panel::new();
    for text in ["alpha", "beta", "gamma"] {
        p.submit(text);
    }
    p.drag(p.at(&card_key(p.id("gamma"))), p.at(&card_key(p.id("alpha"))));
    assert_eq!(p.board().loose().iter().map(|n| n.text.as_str()).collect::<Vec<_>>(), ["gamma", "alpha", "beta"]);

    p.click(p.at("composer"), PLAIN);
    p.submit("# Later");
    p.submit("delta");
    let below_delta = {
        let (x, y) = p.at(&card_key(p.id("delta")));
        (x, y + 30.0)
    };
    p.drag(p.at(&card_key(p.id("alpha"))), below_delta);
    assert_eq!(p.board().loose().iter().map(|n| n.text.as_str()).collect::<Vec<_>>(), ["gamma", "beta"]);
    assert_eq!(p.board().sections()[0].notes.iter().map(|n| n.text.as_str()).collect::<Vec<_>>(), ["delta", "alpha"]);
}

#[test]
fn search_shows_only_matching_notes() {
    let mut p = Panel::new();
    for text in ["Buy oat milk", "Review the MILK carton design", "Call the bank"] {
        p.submit(text);
    }
    p.click(p.at("search"), PLAIN);
    p.type_text("milk");
    assert!(p.rt.find(&card_key(p.id("Buy oat milk"))).is_some());
    assert!(p.rt.find(&card_key(p.id("Review the MILK carton design"))).is_some());
    assert!(p.rt.find(&card_key(p.id("Call the bank"))).is_none());
}

#[test]
fn picking_a_section_focuses_the_list_on_it() {
    let mut p = Panel::new();
    p.submit("loose");
    p.submit("# Errands");
    p.submit("groceries");
    p.submit("# Work");
    p.command('k');
    p.type_text("err");
    p.key(Key::Enter, PLAIN);
    assert_eq!(p.board().active().map(|s| s.name.as_str()), Some("Errands"));
    assert!(p.rt.find(&card_key(p.id("groceries"))).is_some());
    assert!(p.rt.find(&card_key(p.id("loose"))).is_none());
}

#[test]
fn a_section_header_renames_and_deletes_its_section() {
    let mut p = Panel::new();
    p.submit("# Drafts");
    p.submit("outline");
    let header = format!("section-{}", p.board().sections()[0].id);

    p.secondary_click(p.at(&header));
    p.click(p.with_text("Rename Section"), PLAIN);
    p.command('a');
    p.submit("Ideas");
    assert_eq!(p.board().sections()[0].name, "Ideas");

    p.secondary_click(p.at(&header));
    p.click(p.with_text("Delete Section"), PLAIN);
    assert!(p.board().sections().is_empty());
    assert_eq!(p.texts(), ["outline"]);
}

#[test]
fn merge_from_the_context_menu() {
    let mut p = Panel::new();
    p.submit("left");
    p.submit("right");
    p.click(p.at(&card_key(p.id("left"))), PLAIN);
    p.click(p.at(&card_key(p.id("right"))), CMD);
    p.secondary_click(p.at(&card_key(p.id("right"))));
    p.click(p.with_text("Merge"), PLAIN);
    assert_eq!(p.texts(), ["left\n\nright"]);
}

#[test]
fn the_menu_sends_new_notes_to_a_section_without_changing_the_view() {
    let mut p = Panel::new();
    p.submit("# Errands");
    p.submit("# Work");
    p.click(p.at("more"), PLAIN);
    p.click(p.menu_item("Errands"), PLAIN);
    p.click(p.at("composer"), PLAIN);
    p.submit("milk");
    assert_eq!(p.board().sections()[0].notes.iter().map(|n| n.text.as_str()).collect::<Vec<_>>(), ["milk"]);
    assert_eq!(p.rt.state.scope(), Scope::All);

    p.click(p.at("more"), PLAIN);
    p.click(p.menu_item("Show Only Errands"), PLAIN);
    assert_eq!(p.rt.state.scope(), Scope::Active);
}

fn settle_for(p: &mut Panel, seconds: f64) {
    for _ in 0..(seconds * 60.0) as usize {
        p.rt.frame(1.0 / 60.0);
    }
}

#[test]
fn the_capture_shortcut_adds_the_selection_verbatim_to_the_active_section_and_leaves_the_clipboard_alone() {
    let mut p = Panel::new();
    p.submit("# Research");
    p.clipboard.set_text("copied earlier");
    p.platform.select(Some("  let cache = Lru::new(64);\n"));
    p.press(Global::Capture);
    let research = &p.board().sections()[0];
    assert_eq!(research.notes.iter().map(|n| n.text.as_str()).collect::<Vec<_>>(), ["  let cache = Lru::new(64);\n"]);
    assert_eq!(p.clipboard.text().as_deref().map(String::as_str), Some("copied earlier"));
}

#[test]
fn capture_with_nothing_selected_adds_nothing_and_says_so_briefly() {
    let mut p = Panel::new();
    p.platform.select(Some(" \n"));
    p.press(Global::Capture);
    assert!(p.texts().is_empty());
    assert!(p.rt.find("notice").is_some());
    settle_for(&mut p, 3.0);
    assert!(p.rt.find("notice").is_none());
}

#[test]
fn capture_while_hidden_keeps_the_panel_hidden() {
    let mut p = Panel::new();
    p.press(Global::ShowOrHide);
    assert_eq!(p.rt.state.panel, Shown::Hidden);
    p.platform.select(Some("from the terminal"));
    p.press(Global::Capture);
    assert_eq!(p.texts(), ["from the terminal"]);
    assert_eq!(p.rt.state.panel, Shown::Hidden);
}

#[test]
fn the_show_or_hide_shortcut_toggles_the_panel_and_focuses_the_composer() {
    let mut p = Panel::new();
    p.submit("a note");
    p.click(p.at(&card_key(p.id("a note"))), PLAIN);
    assert!(!p.rt.state.composer.focused);
    p.press(Global::ShowOrHide);
    assert_eq!(p.rt.state.panel, Shown::Hidden);
    p.press(Global::ShowOrHide);
    assert_eq!(p.rt.state.panel, Shown::Shown);
    assert!(p.rt.state.composer.focused);
}

#[test]
fn escape_clears_the_selection_then_closes_an_overlay_then_hides_the_panel() {
    let mut p = Panel::new();
    p.submit("a note");
    p.click(p.at(&card_key(p.id("a note"))), PLAIN);
    p.key(Key::Escape, PLAIN);
    assert!(p.rt.state.selected().is_empty());
    assert_eq!(p.rt.state.panel, Shown::Shown);

    p.click(p.at("more"), PLAIN);
    p.click(p.menu_item("Keyboard Shortcuts"), PLAIN);
    p.key(Key::Escape, PLAIN);
    assert!(p.rt.find("shortcuts").is_none());
    assert_eq!(p.rt.state.panel, Shown::Shown);

    p.key(Key::Escape, PLAIN);
    assert_eq!(p.rt.state.panel, Shown::Hidden);
}

#[test]
fn keep_on_top_and_the_list_view_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let open = || {
        let (board, notes) = store::open::<Board>(dir.path());
        let (prefs, preferences) = store::open::<Prefs>(dir.path());
        (Saved { board, notes: notes.clone(), prefs, preferences: preferences.clone() }, notes, preferences)
    };
    let (saved, notes, preferences) = open();
    let mut p = Panel::with(saved);
    assert!(p.rt.state.keeps_on_top(), "on by default");
    p.submit("# Errands");
    p.click(p.at("more"), PLAIN);
    p.click(p.menu_item("Keep on Top"), PLAIN);
    p.click(p.at("more"), PLAIN);
    p.click(p.menu_item("Show Only Errands"), PLAIN);
    drop(p);
    notes.flush();
    preferences.flush();

    let (saved, ..) = open();
    let p = Panel::with(saved);
    assert!(!p.rt.state.keeps_on_top());
    assert_eq!(p.rt.state.scope(), Scope::Active);
}

#[test]
fn reveal_notes_file_asks_the_system_to_show_the_notes_file() {
    let dir = tempfile::tempdir().unwrap();
    let (board, notes) = store::open::<Board>(dir.path());
    let mut p = Panel::with(Saved { board, notes, prefs: Prefs::default(), preferences: Store::Memory });
    p.click(p.at("more"), PLAIN);
    p.click(p.menu_item("Reveal Notes File"), PLAIN);
    assert_eq!(p.platform.revealed(), [dir.path().join("notes.json")]);
}
