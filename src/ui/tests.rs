use std::fs;
use std::path::Path;
use std::rc::Rc;
use std::thread;
use std::time::{Duration, Instant};

use rune::clipboard::{self, MemoryClipboard};
use rune::{AppSpec, Env, Key, KeyEvent, Mods, PhysicalInput, Runtime};

use super::app::{Panel as Shown, card_key};
use super::commands::Global;
use super::{App, Saved, Window, on_key, view};
use crate::board::{Board, NoteId, Scope};
use crate::platform::{Access, Fake};
use crate::prefs::Prefs;
use crate::store::{self, Store};
use crate::sync::test_server::TestServer;
use crate::sync::{Offline, Status, Syncing};

struct Panel {
    rt: Runtime<App>,
    clipboard: MemoryClipboard,
    platform: Rc<Fake>,
}

impl Panel {
    fn new() -> Panel {
        Panel::with(Panel::blank())
    }

    fn blank() -> Saved {
        Saved {
            board: Board::default(),
            notes: Store::Memory,
            prefs: Prefs::default(),
            preferences: Store::Memory,
            sync: Syncing::unsaved(),
        }
    }

    fn with(saved: Saved) -> Panel {
        Panel::on(saved, Fake::default())
    }

    fn lacking_access() -> Panel {
        Panel::with_access(Access::Missing)
    }

    fn with_access(access: Access) -> Panel {
        let platform = Fake::default();
        platform.set_access(access);
        Panel::on(Panel::blank(), platform)
    }

    fn open_link(&mut self, link: &str) {
        self.rt.update(|s, tx| s.open_link(link, tx));
        self.settle();
    }

    fn on(saved: Saved, platform: Fake) -> Panel {
        Panel::launched(saved, platform, None)
    }

    fn launched(saved: Saved, platform: Fake, link: Option<Global>) -> Panel {
        let platform = Rc::new(platform);
        let unbound = rune::App::new(AppSpec::new("Bronze", (360.0, 640.0), || unreachable!(), view)).handle();
        let mut rt = Runtime::new(
            App::new(saved, platform.clone(), Window::new(unbound)).launched_with(link),
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
fn the_switcher_scrolls_to_keep_the_picked_section_in_the_window() {
    let mut board = Board::default();
    for number in 1..=25 {
        board.submit(&format!("# Section {number:02}"));
    }
    let mut p = Panel::with(Saved {
        board,
        notes: Store::Memory,
        prefs: Prefs::default(),
        preferences: Store::Memory,
        sync: Syncing::unsaved(),
    });
    p.command('k');
    for _ in 0..25 {
        p.key(Key::Down, PLAIN);
    }
    let last = p.rt.elements().into_iter().find(|e| e.key.as_deref() == Some("pick-25")).unwrap();
    assert!(last.rect[1] >= 0.0 && last.rect[1] + last.rect[3] <= 640.0, "picked row at {:?}", last.rect);

    p.type_text("Section");
    let list = p.rt.elements().into_iter().find(|e| e.key.as_deref() == Some("switcher-list")).unwrap().rect;
    let first = p.rt.elements().into_iter().find(|e| e.key.as_deref() == Some("pick-0")).unwrap().rect;
    assert!(first[1] >= list[1] && first[1] + first[3] <= list[1] + list[3], "first match at {first:?} in {list:?}");

    p.key(Key::Enter, PLAIN);
    assert_eq!(p.board().active().map(|s| s.name.as_str()), Some("Section 01"));
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
        (
            Saved { board, notes: notes.clone(), prefs, preferences: preferences.clone(), sync: Syncing::unsaved() },
            notes,
            preferences,
        )
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
    let mut p = Panel::with(Saved {
        board,
        notes,
        prefs: Prefs::default(),
        preferences: Store::Memory,
        sync: Syncing::unsaved(),
    });
    p.click(p.at("more"), PLAIN);
    p.click(p.menu_item("Reveal Notes File"), PLAIN);
    assert_eq!(p.platform.revealed(), [dir.path().join("notes.json")]);
}

#[test]
fn the_access_notice_asks_and_follows_access_as_it_is_granted_and_revoked() {
    let mut p = Panel::lacking_access();
    assert!(p.rt.find("notice").is_some());
    p.click(p.at("open-settings"), PLAIN);
    assert_eq!(p.platform.asked_for_access(), 1);

    p.platform.set_access(Access::Granted);
    settle_for(&mut p, 1.5);
    assert!(p.rt.find("notice").is_none());

    p.platform.set_access(Access::Missing);
    settle_for(&mut p, 1.5);
    assert!(p.rt.find("open-settings").is_some());
}

#[test]
fn nothing_selected_shows_over_missing_access_and_gives_way_to_it() {
    let mut p = Panel::lacking_access();
    p.platform.select(None);
    p.press(Global::Capture);
    assert!(p.rt.find("open-settings").is_none());
    settle_for(&mut p, 3.0);
    assert!(p.rt.find("open-settings").is_some());
}

fn on_folder(dir: &Path) -> (Panel, Store<Board>) {
    let (board, notes) = store::open::<Board>(dir);
    let saved = Saved {
        board,
        notes: notes.clone(),
        prefs: Prefs::default(),
        preferences: Store::Memory,
        sync: Syncing::saved_in(dir),
    };
    (Panel::with(saved), notes)
}

fn frames_until(p: &mut Panel, what: &str, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !done(&p.rt.state) {
        assert!(Instant::now() < deadline, "{what}; sync is {}", p.rt.state.sync_status());
        p.rt.frame(1.0 / 60.0);
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn without_sync_json_sync_is_off() {
    let dir = tempfile::tempdir().unwrap();
    let (p, _) = on_folder(dir.path());
    assert_eq!(p.rt.state.sync_status(), Status::Off);
    assert_eq!(p.rt.state.sync_code(), None);
    assert!(!dir.path().join("sync.json").exists());
}

impl Panel {
    fn open_sync(&mut self) {
        self.click(self.at("more"), PLAIN);
        self.click(self.menu_item("Sync\u{2026}"), PLAIN);
    }

    fn element(&self, key: &str) -> Option<rune::ElementInfo> {
        self.rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key))
    }

    fn text_at(&self, key: &str) -> Option<String> {
        self.element(key).and_then(|e| e.text)
    }

    fn reads(&self, text: &str) -> bool {
        self.rt.elements().iter().any(|e| e.text.as_deref() == Some(text))
    }

    fn paste(&mut self, text: &str) {
        self.rt.paste(text);
        self.settle();
    }

    fn clipboard_text(&self) -> String {
        self.clipboard.text().map(|text| text.as_str().to_owned()).unwrap_or_default()
    }

    fn start_vault(&mut self, server: &str) -> String {
        self.open_sync();
        self.type_text(server);
        assert_eq!(self.element("sync-go").and_then(|e| e.label).as_deref(), Some("Start New Vault"));
        self.key(Key::Enter, PLAIN);
        assert!(self.reads("Your Sync Code"), "the new code is shown");
        self.click(self.at("sync-copy"), PLAIN);
        let code = self.clipboard_text();
        assert_eq!(self.text_at("sync-code"), Some(code.clone()), "the code on the clipboard is the one shown");
        self.click(self.at("sync-done"), PLAIN);
        assert!(self.rt.find("sync").is_none(), "Done closes the sheet");
        code
    }

    fn join_vault(&mut self, code: &str) {
        self.open_sync();
        self.paste(code);
        assert_eq!(self.element("sync-go").and_then(|e| e.label).as_deref(), Some("Join Vault"));
        self.click(self.at("sync-go"), PLAIN);
    }
}

#[test]
fn a_new_vault_shows_its_code_once_and_a_second_machine_joins_with_it() {
    let server = TestServer::start();
    let host = server.address.to_string().replace("http://", "");
    let (dir_a, dir_b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (mut a, _) = on_folder(dir_a.path());
    a.submit("Ask for a rollout plan");
    a.submit("Compare the two cache designs");
    let code = a.start_vault(&server.address.to_string());
    let file = dir_a.path().join("sync.json");
    assert!(fs::read_to_string(&file).unwrap().contains(code.split_once('#').unwrap().1));
    #[cfg(unix)]
    assert_eq!(std::os::unix::fs::PermissionsExt::mode(&fs::metadata(&file).unwrap().permissions()) & 0o777, 0o600);
    frames_until(&mut a, "a synced", |s| s.sync_status() == Status::Synced);

    let (mut b, b_notes) = on_folder(dir_b.path());
    b.submit("From b");
    b.open_sync();
    b.paste(&code);
    assert!(b.reads(&format!("Joins your vault on {host}")));
    assert!(b.reads("Notes already on this machine are added to the vault. Nothing is lost."));
    b.click(b.at("sync-go"), PLAIN);
    frames_until(&mut b, "b has a's notes", |s| s.board().notes().count() == 3);
    frames_until(&mut a, "a has b's note", |s| s.board().notes().count() == 3);
    assert_eq!(a.texts(), b.texts());
    frames_until(&mut b, "b synced", |s| s.sync_status() == Status::Synced);
    b.settle();
    assert_eq!(b.text_at("sync-status"), Some(format!("Synced with {host}")));
    assert!(b.text_at("sync-code").is_some_and(|shown| !shown.contains(&code[code.len() - 20..code.len() - 4])));

    b.key(Key::Escape, PLAIN);
    assert!(b.rt.find("sync").is_none());
    a.settle();
    a.click(a.at(&card_key(a.id("Ask for a rollout plan"))), PLAIN);
    b.click(b.at(&card_key(b.id("Ask for a rollout plan"))), PLAIN);
    b.key(Key::Backspace, PLAIN);
    frames_until(&mut a, "the delete arrived", |s| s.board().notes().count() == 2);
    assert!(!a.texts().contains(&"Ask for a rollout plan".to_owned()));
    assert_eq!(a.rt.state.selected(), [], "the selection let go of the deleted note");
    b_notes.flush();
    let saved = fs::read_to_string(dir_b.path().join("notes.json")).unwrap();
    assert!(saved.contains("Compare the two cache designs"), "a note from a is saved on b: {saved}");
}

#[test]
fn each_bad_entry_says_what_is_wrong_and_starts_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (mut p, _) = on_folder(dir.path());
    p.open_sync();
    let made_up = "https://sync.example.com#bronze1-000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
    for (typed, sentence) in [
        ("sync.example.com", "This is not a server address or a sync code. A server address starts with https://."),
        ("http://sync.example.com", "Use https. Plain http works only for localhost."),
        (
            &made_up[..60],
            "This code is cut short or has extra characters. Copy the whole code again from the other machine.",
        ),
        (
            &format!("{made_up}-630dcd28"),
            "This code has a typo somewhere, so its check doesn't match. Copy it again from the other machine.",
        ),
    ] {
        p.command('a');
        p.type_text(typed);
        assert_eq!(p.text_at("sync-note").as_deref(), Some(sentence), "{typed:?}");
        p.key(Key::Enter, PLAIN);
        p.click(p.at("sync-go"), PLAIN);
        assert!(p.rt.find("sync-field").is_some(), "{typed:?} keeps the sheet on the field");
        assert_eq!(p.rt.state.sync_status(), Status::Off);
        assert!(!dir.path().join("sync.json").exists());
    }

    p.key(Key::Escape, PLAIN);
    assert!(p.rt.find("sync").is_none());
    assert_eq!(p.rt.state.panel, Shown::Shown);
}

#[test]
fn stopping_sync_keeps_every_note_and_removes_sync_json() {
    let server = TestServer::start();
    let (dir, fresh) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (mut a, _) = on_folder(dir.path());
    a.submit("Synced before stopping");
    let code = a.start_vault(&server.address.to_string());
    frames_until(&mut a, "a synced", |s| s.sync_status() == Status::Synced);

    a.open_sync();
    assert!(a.reads("Stopping keeps every note on this machine."));
    a.click(a.at("sync-stop"), PLAIN);
    assert!(a.rt.find("sync-field").is_some(), "the sheet offers to sync again");
    assert_eq!(a.rt.state.sync_status(), Status::Off);
    assert!(!dir.path().join("sync.json").exists());
    a.key(Key::Escape, PLAIN);
    a.click(a.at("composer"), PLAIN);
    a.submit("Written after stopping");
    assert_eq!(a.texts(), ["Synced before stopping", "Written after stopping"]);
    a.click(a.at("more"), PLAIN);
    assert!(a.reads("\u{25CB}  Off"));

    let (mut c, _) = on_folder(fresh.path());
    c.join_vault(&code);
    frames_until(&mut c, "c synced", |s| s.sync_status() == Status::Synced);
    assert_eq!(c.texts(), ["Synced before stopping"]);
}

#[test]
fn an_unreachable_server_shows_the_problem_and_notes_still_save_here() {
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let dir = tempfile::tempdir().unwrap();
    let (mut a, notes) = on_folder(dir.path());
    a.start_vault(&format!("http://127.0.0.1:{port}"));
    frames_until(&mut a, "a offline", |s| matches!(s.sync_status(), Status::Offline(Offline::Unreachable(_))));

    a.open_sync();
    assert_eq!(a.text_at("sync-status"), Some(format!("Can't reach 127.0.0.1:{port}")));
    assert!(a.text_at("sync-note").is_some_and(|note| note.starts_with("New notes stay on this machine")));
    a.key(Key::Escape, PLAIN);
    a.click(a.at("composer"), PLAIN);
    a.submit("Written while the server is away");
    notes.flush();
    assert!(fs::read_to_string(dir.path().join("notes.json")).unwrap().contains("Written while the server is away"));
    a.click(a.at("more"), PLAIN);
    assert!(a.reads("\u{25CF}  Not synced"));
}

#[test]
fn a_saved_sync_json_resumes_syncing_at_the_next_launch() {
    let server = TestServer::start();
    let dir = tempfile::tempdir().unwrap();
    let (mut a, notes) = on_folder(dir.path());
    let code = a.start_vault(&server.address.to_string());
    frames_until(&mut a, "a synced", |s| s.sync_status() == Status::Synced);
    drop(a);
    notes.flush();
    let (mut again, _) = on_folder(dir.path());
    assert_eq!(again.rt.state.sync_code().map(|shown| shown.reveal()), Some(code));
    frames_until(&mut again, "the relaunch synced", |s| s.sync_status() == Status::Synced);
}

#[test]
fn without_the_keyboard_the_gestures_notice_points_to_the_shortcuts_and_stays_dismissed() {
    let mut p = Panel::with_access(Access::NoKeyboard);
    assert!(p.reads("Shift gestures are off because Bronze can't read the keyboard in this session."));
    p.click(p.at("show-shortcuts"), PLAIN);
    assert!(p.rt.find("shortcuts").is_some());
    assert!(!p.reads("Double-tap Left Shift"));
    assert!(p.reads("Capture selected text"));
    p.key(Key::Escape, PLAIN);
    settle_for(&mut p, 1.5);
    assert!(p.rt.find("notice").is_none());
}

#[test]
fn dismissing_the_gestures_notice_leaves_other_notices_working() {
    let mut p = Panel::with_access(Access::NoKeyboard);
    p.click(p.at("dismiss-notice"), PLAIN);
    assert!(p.rt.find("notice").is_none());
    p.platform.select(None);
    p.press(Global::Capture);
    assert!(p.reads("Nothing is selected to capture"));
}

#[test]
fn the_shortcuts_sheet_lists_the_shift_gestures_only_when_bronze_hears_them() {
    let mut p = Panel::new();
    p.click(p.at("more"), PLAIN);
    p.click(p.menu_item("Keyboard Shortcuts"), PLAIN);
    assert!(p.reads("Double-tap Left Shift"));
    assert!(p.reads("Right Shift + Left Shift"));
}

#[test]
fn links_capture_and_toggle_like_the_shortcuts_and_unknown_ones_do_nothing() {
    let mut p = Panel::new();
    p.platform.select(Some("picked by a bound key"));
    p.open_link("bronze://capture");
    assert_eq!(p.texts(), ["picked by a bound key"]);
    p.open_link("bronze://show-or-hide");
    assert_eq!(p.rt.state.panel, Shown::Hidden);
    p.open_link("bronze://show-or-hide/");
    assert_eq!(p.rt.state.panel, Shown::Shown);
    p.open_link("bronze://quit");
    p.open_link("https://example.com/capture");
    assert_eq!(p.texts(), ["picked by a bound key"]);
    assert_eq!(p.rt.state.panel, Shown::Shown);
}

#[test]
fn a_launch_by_the_show_or_hide_link_shows_bronze_and_later_ones_toggle() {
    let mut p = Panel::launched(Panel::blank(), Fake::default(), Some(Global::ShowOrHide));
    p.open_link("bronze://show-or-hide");
    assert_eq!(p.rt.state.panel, Shown::Shown);
    p.open_link("bronze://show-or-hide");
    assert_eq!(p.rt.state.panel, Shown::Hidden);
}

#[test]
fn a_bindable_command_reaches_this_data_folder_and_survives_spaces_and_quotes() {
    use super::commands::command;
    assert_eq!(command("bronze", None, Global::Capture), "bronze bronze://capture");
    assert_eq!(
        command("/opt/My Apps/bronze", Some(Path::new("/home/me/it's notes")), Global::ShowOrHide),
        r"'/opt/My Apps/bronze' --data '/home/me/it'\''s notes' bronze://show-or-hide"
    );
}
