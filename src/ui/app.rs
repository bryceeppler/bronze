use rune::{Mods, PointerPress, TextEdit, Tx};

use crate::board::{Board, Note, NoteId, Owner, Place, Scope, SectionId, Submitted};
use crate::store::Store;

pub struct App {
    board: Board,
    store: Store,
    pub composer: TextEdit,
    pub search: TextEdit,
    pub overlay_field: TextEdit,
    selection: Selection,
    pub scope: Scope,
    pub overlay: Overlay,
    pub dragging: Option<Dragging>,
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
    pub fn new(board: Board, store: Store) -> App {
        App {
            board,
            store,
            composer: focused_field(""),
            search: TextEdit::new(""),
            overlay_field: TextEdit::new(""),
            selection: Selection::default(),
            scope: Scope::All,
            overlay: Overlay::None,
            dragging: None,
        }
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn edit<R>(&mut self, change: impl FnOnce(&mut Board) -> R) -> R {
        let result = change(&mut self.board);
        self.store.save(&self.board);
        let board = &self.board;
        self.selection.ids.retain(|id| board.notes().any(|n| n.id == *id));
        result
    }

    pub fn shown(&self) -> Vec<Shown<'_>> {
        let query = self.search.text.trim().to_lowercase();
        self.board
            .lists(self.scope)
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
        let scope = self.scope;
        self.edit(|board| board.clear_done(scope));
        self.overlay = Overlay::None;
    }

    pub fn pick(&mut self, pick: Pick, tx: &mut Tx) {
        match pick {
            Pick::All => self.scope = Scope::All,
            Pick::Section(id) => {
                self.edit(|board| board.activate(id));
                self.scope = Scope::Active;
            }
        }
        self.overlay = Overlay::None;
        self.keep_shown_selected();
        tx.scroll_to_instant(LIST_KEY, 0.0, 0.0);
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

    pub fn step_switcher(&mut self, by: isize) {
        let count = self.picks().len() as isize;
        if let Overlay::Switcher { picked } = &mut self.overlay {
            *picked = (*picked as isize + by).rem_euclid(count.max(1)) as usize;
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

fn focused_field(text: &str) -> TextEdit {
    let mut field = TextEdit::new(text);
    field.focused = true;
    field
}

pub const LIST_KEY: &str = "list";

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
