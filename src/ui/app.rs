use std::io;
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};

use rune::{Mods, PointerPress, ShortcutError, TextEdit, Tx};

use super::commands::{self, Global};
use super::window::Window;
use crate::board::{Board, Note, NoteId, Owner, Place, Scope, SectionId, Submitted};
use crate::platform::{Access, Platform};
use crate::prefs::Prefs;
use crate::store::Store;
use crate::sync::{Delivery, Setup, Status, SyncCode, Syncing};

const NOTICE_SECONDS: f64 = 2.5;
const ACCESS_RECHECK_SECONDS: f64 = 1.0;

pub struct App {
    board: Board,
    notes: Store<Board>,
    sync: Syncing,
    prefs: Prefs,
    preferences: Store<Prefs>,
    platform: Rc<dyn Platform>,
    access: Access,
    recheck_access_at: f64,
    window: Window,
    selections: (Sender<Option<String>>, Receiver<Option<String>>),
    pub panel: Panel,
    nothing_selected_until: Option<f64>,
    unavailable: Vec<(Global, ShortcutError)>,
    pub composer: TextEdit,
    pub search: TextEdit,
    pub overlay_field: TextEdit,
    selection: Selection,
    pub overlay: Overlay,
    pub dragging: Option<Dragging>,
}

pub struct Saved {
    pub board: Board,
    pub notes: Store<Board>,
    pub prefs: Prefs,
    pub preferences: Store<Prefs>,
    pub sync: Syncing,
}

pub enum Notice<'a> {
    NothingSelected,
    NeedsAccess,
    Unavailable(Global, &'a ShortcutError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Shown,
    Hidden,
}

#[derive(Default)]
pub struct Selection {
    ids: Vec<NoteId>,
    anchor: Option<NoteId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    Only,
    Toggle,
    Extend,
}

pub enum Overlay {
    None,
    Menu,
    NoteMenu(PointerPress),
    SectionMenu(SectionId, PointerPress),
    Rename(SectionId),
    Switcher { picked: usize },
    Shortcuts,
}

#[derive(Clone, Copy)]
pub struct Dragging {
    pub id: NoteId,
    pub dy: f32,
    pub target: Option<Place>,
}

pub struct Shown<'a> {
    pub owner: Owner,
    pub name: Option<&'a str>,
    pub notes: Vec<&'a Note>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    All,
    Section(SectionId),
}

impl App {
    pub fn new(saved: Saved, platform: Rc<dyn Platform>, window: Window) -> App {
        let mut sync = saved.sync;
        sync.resume(saved.board.replica(), window.waker());
        App {
            board: saved.board,
            notes: saved.notes,
            sync,
            prefs: saved.prefs,
            preferences: saved.preferences,
            access: platform.access(),
            recheck_access_at: 0.0,
            platform,
            window,
            selections: mpsc::channel(),
            panel: Panel::Shown,
            nothing_selected_until: None,
            unavailable: Vec::new(),
            composer: focused_field(""),
            search: TextEdit::new(""),
            overlay_field: TextEdit::new(""),
            selection: Selection::default(),
            overlay: Overlay::None,
            dragging: None,
        }
    }

    pub fn scope(&self) -> Scope {
        self.prefs.scope
    }

    pub fn keeps_on_top(&self) -> bool {
        self.prefs.keep_on_top
    }

    pub fn notes_path(&self) -> Option<&Path> {
        self.notes.path()
    }

    fn set_prefs(&mut self, change: impl FnOnce(&mut Prefs)) {
        change(&mut self.prefs);
        self.preferences.save(&self.prefs);
    }

    pub fn use_platform(&mut self, platform: Rc<dyn Platform>) {
        self.access = platform.access();
        self.platform = platform;
    }

    pub fn ask_for_access(&self) {
        self.platform.ask_for_access();
    }

    pub fn register_shortcuts(&mut self) {
        for global in Global::ALL {
            if let Err(error) = self.window.register(global) {
                self.shortcut_unavailable(global, error);
            }
        }
    }

    pub fn shortcut_unavailable(&mut self, global: Global, error: ShortcutError) {
        eprintln!("bronze: cannot register {}: {error}", global.chord());
        self.unavailable.push((global, error));
    }

    pub fn global(&mut self, global: Global, tx: &mut Tx) {
        match (global, self.panel) {
            (Global::Capture, _) => self.capture(),
            (Global::ShowOrHide, Panel::Shown) => self.hide_panel(),
            (Global::ShowOrHide, Panel::Hidden) => self.show_panel(tx),
        }
    }

    fn capture(&self) {
        let (send, wake) = (self.selections.0.clone(), self.window.waker());
        self.platform.read_selection(Box::new(move |text| {
            if send.send(text).is_ok() {
                wake();
            }
        }));
    }

    pub fn poll(&mut self, clock: f64, tx: &mut Tx) -> bool {
        let mut changed = false;
        while let Ok(text) = self.selections.1.try_recv() {
            self.captured(text, tx);
            changed = true;
        }
        for arrival in self.sync.arrived() {
            if let Delivery::Remote(replica) = arrival {
                self.edit(|board| board.join(&replica));
            }
            changed = true;
        }
        if self.nothing_selected_until.is_some_and(|until| clock >= until) {
            self.nothing_selected_until = None;
            changed = true;
        }
        if clock >= self.recheck_access_at {
            let access = self.platform.access();
            changed |= access != self.access;
            self.access = access;
            self.recheck_access_at = clock + ACCESS_RECHECK_SECONDS;
        }
        if self.access == Access::Missing {
            tx.wake_at(self.recheck_access_at);
        }
        changed
    }

    fn captured(&mut self, text: Option<String>, tx: &mut Tx) {
        match text.filter(|text| !text.trim().is_empty()) {
            Some(text) => {
                let id = self.edit(|board| board.capture(text));
                self.nothing_selected_until = None;
                tx.scroll_into_view(card_key(id));
            }
            None => {
                let until = tx.clock() + NOTICE_SECONDS;
                self.nothing_selected_until = Some(until);
                tx.wake_at(until);
            }
        }
    }

    pub fn notice(&self) -> Option<Notice<'_>> {
        if self.nothing_selected_until.is_some() {
            return Some(Notice::NothingSelected);
        }
        if self.access == Access::Missing {
            return Some(Notice::NeedsAccess);
        }
        self.unavailable.first().map(|(global, error)| Notice::Unavailable(*global, error))
    }

    pub fn hide_panel(&mut self) {
        self.remember_frame();
        self.window.hide();
        self.overlay = Overlay::None;
        self.panel = Panel::Hidden;
    }

    pub fn show_panel(&mut self, tx: &mut Tx) {
        self.window.open();
        self.panel = Panel::Shown;
        tx.focus_when_shown("composer", false);
    }

    pub fn escape(&mut self) {
        if self.deselect() {
            return;
        }
        match self.overlay {
            Overlay::None => self.hide_panel(),
            _ => self.overlay = Overlay::None,
        }
    }

    pub fn toggle_keep_on_top(&mut self) {
        let on = !self.prefs.keep_on_top;
        self.set_prefs(|prefs| prefs.keep_on_top = on);
        self.window.keep_on_top(on);
        self.overlay = Overlay::None;
    }

    pub fn remember_frame(&mut self) {
        if let Some(frame) = self.window.frame().filter(|frame| self.prefs.frame != Some(*frame)) {
            self.set_prefs(|prefs| prefs.frame = Some(frame));
        }
    }

    pub fn reveal_notes(&mut self) {
        if let Some(path) = self.notes.path() {
            self.platform.reveal(path);
        }
        self.overlay = Overlay::None;
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn edit<R>(&mut self, change: impl FnOnce(&mut Board) -> R) -> R {
        let result = change(&mut self.board);
        self.notes.save(&self.board);
        self.sync.edited(self.board.replica());
        let board = &self.board;
        self.selection.ids.retain(|id| board.notes().any(|n| n.id == *id));
        result
    }

    pub fn shown(&self) -> Vec<Shown<'_>> {
        let query = self.search.text.trim().to_lowercase();
        self.board
            .lists(self.prefs.scope)
            .into_iter()
            .map(|list| Shown {
                owner: list.owner,
                name: list.name,
                notes: list.notes.iter().filter(|n| n.text.to_lowercase().contains(&query)).collect(),
            })
            .filter(|shown| query.is_empty() || !shown.notes.is_empty())
            .collect()
    }

    pub fn shown_ids(&self) -> Vec<NoteId> {
        self.shown().iter().flat_map(|s| s.notes.iter().map(|n| n.id)).collect()
    }

    pub fn selected(&self) -> &[NoteId] {
        &self.selection.ids
    }

    pub fn is_selected(&self, id: NoteId) -> bool {
        self.selection.ids.contains(&id)
    }

    pub fn submit(&mut self, tx: &mut Tx) {
        let input = std::mem::take(&mut self.composer.text);
        self.composer.clear();
        if let Submitted::Added(id) = self.edit(|board| board.submit(&input)) {
            tx.scroll_into_view(card_key(id));
        }
    }

    pub fn click(&mut self, id: NoteId, click: Click) {
        let order = self.shown_ids();
        let selection = &mut self.selection;
        match (click, selection.anchor.and_then(|a| order.iter().position(|n| *n == a))) {
            (Click::Extend, Some(from)) => {
                let to = order.iter().position(|n| *n == id).unwrap_or(from);
                selection.ids = order[from.min(to)..=from.max(to)].to_vec();
            }
            (Click::Toggle, _) => {
                match selection.ids.iter().position(|n| *n == id) {
                    Some(at) => _ = selection.ids.remove(at),
                    None => selection.ids.push(id),
                }
                selection.anchor = Some(id);
            }
            _ => {
                selection.ids = vec![id];
                selection.anchor = Some(id);
            }
        }
    }

    pub fn deselect(&mut self) -> bool {
        let had = !self.selection.ids.is_empty();
        self.selection = Selection::default();
        had
    }

    pub fn keep_shown_selected(&mut self) {
        let shown = self.shown_ids();
        self.selection.ids.retain(|id| shown.contains(id));
    }

    pub fn copy(&self, tx: &mut Tx) -> bool {
        if self.selection.ids.is_empty() {
            return false;
        }
        tx.copy(self.board.copy_text(&self.selection.ids));
        true
    }

    pub fn merge(&mut self) -> bool {
        let ids = self.selection.ids.clone();
        let Some(into) = self.edit(|board| board.merge(&ids)) else { return false };
        self.selection = Selection { ids: vec![into], anchor: Some(into) };
        true
    }

    pub fn toggle(&mut self, ids: &[NoteId]) -> bool {
        self.edit(|board| board.toggle(ids));
        !ids.is_empty()
    }

    pub fn toggle_selected(&mut self) -> bool {
        let ids = self.selection.ids.clone();
        self.toggle(&ids)
    }

    pub fn delete_selected(&mut self) -> bool {
        let ids = self.selection.ids.clone();
        self.edit(|board| board.remove(&ids));
        !ids.is_empty()
    }

    pub fn clear_done(&mut self) {
        let scope = self.prefs.scope;
        self.edit(|board| board.clear_done(scope));
        self.overlay = Overlay::None;
    }

    pub fn pick(&mut self, pick: Pick, tx: &mut Tx) {
        let scope = match pick {
            Pick::All => Scope::All,
            Pick::Section(id) => {
                self.edit(|board| board.activate(id));
                Scope::Active
            }
        };
        self.set_prefs(|prefs| prefs.scope = scope);
        self.overlay = Overlay::None;
        self.keep_shown_selected();
        tx.scroll_to_instant(LIST_KEY, 0.0, 0.0);
    }

    pub fn set_scope(&mut self, scope: Scope, tx: &mut Tx) {
        self.set_prefs(|prefs| prefs.scope = scope);
        self.overlay = Overlay::None;
        self.keep_shown_selected();
        tx.scroll_to_instant(LIST_KEY, 0.0, 0.0);
    }

    pub fn send_to(&mut self, target: Option<SectionId>) {
        self.edit(|board| match target {
            Some(id) => board.activate(id),
            None => board.deactivate(),
        });
        self.overlay = Overlay::None;
        self.keep_shown_selected();
    }

    pub fn picks(&self) -> Vec<(Pick, String)> {
        let query = self.overlay_field.text.trim().to_lowercase();
        std::iter::once((Pick::All, "Show All".to_owned()))
            .chain(self.board.sections().iter().map(|s| (Pick::Section(s.id), s.name.clone())))
            .filter(|(_, name)| name.to_lowercase().contains(&query))
            .collect()
    }

    pub fn open_switcher(&mut self) {
        self.overlay = match self.overlay {
            Overlay::Switcher { .. } => Overlay::None,
            _ => {
                self.overlay_field = focused_field("");
                Overlay::Switcher { picked: 0 }
            }
        };
    }

    pub fn step_switcher(&mut self, by: isize, tx: &mut Tx) {
        let count = self.picks().len() as isize;
        if let Overlay::Switcher { picked } = &mut self.overlay {
            *picked = (*picked as isize + by).rem_euclid(count.max(1)) as usize;
            tx.scroll_into_view(pick_key(*picked));
        }
    }

    pub fn pick_switcher(&mut self, tx: &mut Tx) {
        let Overlay::Switcher { picked } = self.overlay else { return };
        if let Some(&(pick, _)) = self.picks().get(picked) {
            self.pick(pick, tx);
        }
    }

    pub fn start_rename(&mut self, id: SectionId) {
        let Some(section) = self.board.section(id) else { return };
        self.overlay_field = focused_field(&section.name);
        self.overlay_field.set_selection(0, section.name.len());
        self.overlay = Overlay::Rename(id);
    }

    pub fn finish_rename(&mut self) {
        if let Overlay::Rename(id) = std::mem::replace(&mut self.overlay, Overlay::None) {
            let name = self.overlay_field.text.clone();
            self.edit(|board| board.rename_section(id, &name));
        }
    }

    pub fn remove_section(&mut self, id: SectionId) {
        self.edit(|board| board.remove_section(id));
        self.overlay = Overlay::None;
    }

    pub fn open_note_menu(&mut self, id: NoteId, at: PointerPress) {
        if !self.is_selected(id) {
            self.click(id, Click::Only);
        }
        self.overlay = Overlay::NoteMenu(at);
    }

    pub fn drag(&mut self, id: NoteId, dy: f32, target: Option<Place>) {
        self.dragging = Some(Dragging { id, dy, target });
    }

    pub fn drop(&mut self) {
        if let Some(Dragging { id, target: Some(place), .. }) = self.dragging.take() {
            self.edit(|board| board.move_note(id, place));
        }
    }
}

#[cfg_attr(not(test), expect(dead_code, reason = "the setup screen calls these"))]
impl App {
    pub fn sync_with(&mut self, setup: Setup) -> io::Result<SyncCode> {
        let code = setup.code();
        self.sync.start(code.clone(), self.board.replica(), self.window.waker())?;
        Ok(code)
    }

    pub fn stop_syncing(&mut self) -> io::Result<()> {
        self.sync.stop()
    }

    pub fn sync_status(&self) -> Status {
        self.sync.status()
    }

    pub fn sync_code(&self) -> Option<&SyncCode> {
        self.sync.code()
    }
}

impl Notice<'_> {
    pub fn text(&self) -> String {
        match self {
            Notice::NothingSelected => "Nothing is selected to capture".to_owned(),
            Notice::NeedsAccess => {
                "Bronze needs Accessibility access to capture selected text and to respond to double-tap Shift."
                    .to_owned()
            }
            Notice::Unavailable(global, error) => {
                let (name, chord) = (global.name(), commands::label(global.chord()));
                match error {
                    ShortcutError::Conflict => format!("{name} {chord} is taken by another app"),
                    _ => format!("{name} {chord} is unavailable here"),
                }
            }
        }
    }
}

fn focused_field(text: &str) -> TextEdit {
    let mut field = TextEdit::new(text);
    field.focused = true;
    field
}

pub const LIST_KEY: &str = "list";

pub fn pick_key(index: usize) -> String {
    format!("pick-{index}")
}

pub fn card_key(id: NoteId) -> String {
    format!("note-{id}")
}

impl Click {
    pub fn from_mods(mods: Mods) -> Click {
        if mods.shift {
            Click::Extend
        } else if mods.cmd {
            Click::Toggle
        } else {
            Click::Only
        }
    }
}
