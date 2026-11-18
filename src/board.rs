mod rank;
mod replica;
#[cfg(test)]
mod simulation;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

pub use rank::Rank;
pub use replica::{DeviceId, NoteId, NoteRecord, Position, Reg, Replica, SectionId, SectionRecord, Stamp};

#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub id: NoteId,
    pub text: String,
    pub done: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub id: SectionId,
    pub name: String,
    pub notes: Vec<Note>,
}

#[derive(Clone, Debug)]
pub struct Board {
    replica: Replica,
    device: DeviceId,
    clock: Clock,
    rng: fastrand::Rng,
    active: Option<SectionId>,
    view: View,
}

#[derive(Clone, Copy, Debug)]
struct Clock {
    wall: fn() -> u64,
    latest: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct View {
    loose: Vec<Note>,
    sections: Vec<Section>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Loose,
    Section(SectionId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Before(NoteId),
    End(Owner),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    All,
    Active,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Submitted {
    Empty,
    Activated(SectionId),
    Added(NoteId),
}

pub struct List<'a> {
    pub owner: Owner,
    pub name: Option<&'a str>,
    pub notes: &'a [Note],
}

pub struct Restored {
    pub loose: Vec<(String, bool)>,
    pub sections: Vec<(String, Vec<(String, bool)>)>,
    pub active: Option<String>,
}

impl Default for Board {
    fn default() -> Board {
        Board::new(fastrand::Rng::new(), wall_clock)
    }
}

impl Board {
    pub fn seeded(seed: u64) -> Board {
        Board::new(fastrand::Rng::with_seed(seed), || 0)
    }

    fn new(mut rng: fastrand::Rng, wall: fn() -> u64) -> Board {
        Board {
            replica: Replica::default(),
            device: DeviceId::random(&mut rng),
            clock: Clock { wall, latest: 0 },
            rng,
            active: None,
            view: View::default(),
        }
    }

    pub fn load(device: DeviceId, replica: &Replica, active: Option<&str>) -> Board {
        let mut board = Board { device, ..Board::default() };
        board.join(replica);
        board.active = active.and_then(|name| board.section_named(name)).map(|s| s.id);
        board
    }

    pub fn restore(restored: Restored) -> Board {
        let mut board = Board::default();
        let mut lists = vec![(None, restored.loose)];
        for (name, notes) in restored.sections {
            lists.push((Some(board.open_section(&name)), notes));
        }
        let at = board.tick();
        let mut last: HashMap<Option<SectionId>, Rank> = HashMap::new();
        for (section, notes) in lists {
            for (text, done) in notes {
                let rank = Rank::between(last.get(&section), None, &mut board.rng);
                board.add(text, done, Position { section, rank: rank.clone() }, at);
                last.insert(section, rank);
            }
        }
        board.project();
        board.active = restored.active.and_then(|name| board.section_named(&name)).map(|s| s.id);
        board
    }

    pub fn device(&self) -> DeviceId {
        self.device
    }

    pub fn replica(&self) -> &Replica {
        &self.replica
    }

    pub fn join(&mut self, other: &Replica) -> bool {
        self.replica.join(other);
        self.clock.latest = self.clock.latest.max(other.latest());
        let before = std::mem::take(&mut self.view);
        self.project();
        self.view != before
    }

    #[cfg(test)]
    pub fn loose(&self) -> &[Note] {
        &self.view.loose
    }

    pub fn sections(&self) -> &[Section] {
        &self.view.sections
    }

    pub fn active(&self) -> Option<&Section> {
        self.active.and_then(|id| self.section(id))
    }

    pub fn section(&self, id: SectionId) -> Option<&Section> {
        self.view.sections.iter().find(|s| s.id == id)
    }

    pub fn notes(&self) -> impl Iterator<Item = &Note> {
        self.view.loose.iter().chain(self.view.sections.iter().flat_map(|s| &s.notes))
    }

    pub fn lists(&self, scope: Scope) -> Vec<List<'_>> {
        match (scope, self.active()) {
            (Scope::Active, Some(active)) => vec![active.list()],
            _ => std::iter::once(List { owner: Owner::Loose, name: None, notes: &self.view.loose })
                .chain(self.view.sections.iter().map(Section::list))
                .collect(),
        }
    }

    pub fn submit(&mut self, input: &str) -> Submitted {
        let text = input.trim();
        if text.is_empty() {
            return Submitted::Empty;
        }
        match section_command(text) {
            Some(name) => {
                let id = self.open_section(name);
                self.active = Some(id);
                Submitted::Activated(id)
            }
            None => Submitted::Added(self.capture(text)),
        }
    }

    pub fn capture(&mut self, text: impl Into<String>) -> NoteId {
        let position = self.position(self.active.map_or(Owner::Loose, Owner::Section), None, None);
        let at = self.tick();
        let id = self.add(text.into(), false, position, at);
        self.project();
        id
    }

    pub fn toggle(&mut self, ids: &[NoteId]) {
        let done = !self.notes().filter(|n| ids.contains(&n.id)).all(|n| n.done);
        let at = self.tick();
        for id in ids {
            if let Some(NoteRecord::Live { done: flag, .. }) = self.replica.notes.get_mut(id) {
                *flag = Reg { value: done, at };
            }
        }
        self.project();
    }

    pub fn remove(&mut self, ids: &[NoteId]) {
        for id in ids {
            if let Some(record) = self.replica.notes.get_mut(id) {
                *record = NoteRecord::Removed;
            }
        }
        self.project();
    }

    pub fn merge(&mut self, ids: &[NoteId]) -> Option<NoteId> {
        let picked: Vec<&Note> = self.notes().filter(|n| ids.contains(&n.id)).collect();
        let [first, rest @ ..] = picked.as_slice() else { return None };
        if rest.is_empty() {
            return None;
        }
        let text = picked.iter().map(|n| n.text.as_str()).collect::<Vec<_>>().join("\n\n");
        let done = picked.iter().all(|n| n.done);
        let position = self.position_of(first.id)?.clone();
        let folded: Vec<NoteId> = picked.iter().map(|n| n.id).collect();
        let at = self.tick();
        let merged = self.add(text, done, position, at);
        self.remove(&folded);
        Some(merged)
    }

    pub fn move_note(&mut self, id: NoteId, to: Place) {
        if self.locate(id).is_none() {
            return;
        }
        let (owner, before) = match to {
            Place::Before(other) if other != id => match self.locate(other) {
                Some(owner) => (owner, Some(other)),
                None => return,
            },
            Place::Before(_) => return,
            Place::End(owner) if self.lists(Scope::All).iter().any(|l| l.owner == owner) => (owner, None),
            Place::End(_) => return,
        };
        let position = self.position(owner, before, Some(id));
        let at = self.tick();
        self.place(id, position, at);
        self.project();
    }

    pub fn clear_done(&mut self, scope: Scope) {
        let done: Vec<NoteId> = match (scope, self.active()) {
            (Scope::Active, Some(active)) => active.notes.iter().filter(|n| n.done).map(|n| n.id).collect(),
            _ => self.notes().filter(|n| n.done).map(|n| n.id).collect(),
        };
        self.remove(&done);
    }

    pub fn activate(&mut self, id: SectionId) {
        if self.section(id).is_some() {
            self.active = Some(id);
        }
    }

    pub fn deactivate(&mut self) {
        self.active = None;
    }

    pub fn rename_section(&mut self, id: SectionId, name: &str) -> bool {
        let name = name.trim();
        let taken = self.section_named(name).is_some_and(|s| s.id != id);
        if self.section(id).is_none() || name.is_empty() || taken {
            return false;
        }
        let at = self.tick();
        for member in self.members(id) {
            if let Some(SectionRecord::Live { name: current, .. }) = self.replica.sections.get_mut(&member) {
                *current = Reg { value: name.to_owned(), at };
            }
        }
        self.project();
        true
    }

    pub fn remove_section(&mut self, id: SectionId) {
        let Some(section) = self.section(id) else { return };
        let notes: Vec<NoteId> = section.notes.iter().map(|n| n.id).collect();
        for member in self.members(id) {
            self.replica.sections.insert(member, SectionRecord::Removed);
        }
        let at = self.tick();
        let mut last = self.view.loose.last().and_then(|n| self.position_of(n.id)).map(|p| p.rank.clone());
        for note in notes {
            let rank = Rank::between(last.as_ref(), None, &mut self.rng);
            self.place(note, Position { section: None, rank: rank.clone() }, at);
            last = Some(rank);
        }
        if self.active == Some(id) {
            self.active = None;
        }
        self.project();
    }

    pub fn copy_text(&self, ids: &[NoteId]) -> String {
        self.notes().filter(|n| ids.contains(&n.id)).map(|n| n.text.as_str()).collect::<Vec<_>>().join("\n\n")
    }

    fn tick(&mut self) -> Stamp {
        self.clock.latest = (self.clock.wall)().max(self.clock.latest + 1);
        Stamp { time: self.clock.latest, device: self.device }
    }

    fn add(&mut self, text: String, done: bool, position: Position, at: Stamp) -> NoteId {
        let id = NoteId::random(&mut self.rng);
        let record = NoteRecord::Live { text, done: Reg { value: done, at }, place: Reg { value: position, at } };
        self.replica.notes.insert(id, record);
        id
    }

    fn place(&mut self, id: NoteId, position: Position, at: Stamp) {
        if let Some(NoteRecord::Live { place, .. }) = self.replica.notes.get_mut(&id) {
            *place = Reg { value: position, at };
        }
    }

    fn position_of(&self, id: NoteId) -> Option<&Position> {
        match self.replica.notes.get(&id) {
            Some(NoteRecord::Live { place, .. }) => Some(&place.value),
            _ => None,
        }
    }

    fn position(&mut self, owner: Owner, before: Option<NoteId>, moving: Option<NoteId>) -> Position {
        let list: Vec<NoteId> = self
            .lists(Scope::All)
            .iter()
            .find(|l| l.owner == owner)
            .map(|l| l.notes.iter().map(|n| n.id).filter(|id| Some(*id) != moving).collect())
            .unwrap_or_default();
        let at = before.and_then(|b| list.iter().position(|id| *id == b)).unwrap_or(list.len());
        let rank = |i: usize| list.get(i).and_then(|id| self.position_of(*id)).map(|p| p.rank.clone());
        let (lo, hi) = (at.checked_sub(1).and_then(rank), rank(at));
        let section = match owner {
            Owner::Loose => None,
            Owner::Section(id) => Some(id),
        };
        Position { section, rank: Rank::between(lo.as_ref(), hi.as_ref(), &mut self.rng) }
    }

    fn open_section(&mut self, name: &str) -> SectionId {
        if let Some(found) = self.section_named(name) {
            return found.id;
        }
        let at = self.tick();
        let id = SectionId::random(&mut self.rng);
        self.replica.sections.insert(id, SectionRecord::Live { name: Reg { value: name.to_owned(), at }, created: at });
        self.project();
        id
    }

    fn section_named(&self, name: &str) -> Option<&Section> {
        let name = name.to_lowercase();
        self.view.sections.iter().find(|s| s.name.to_lowercase() == name)
    }

    fn members(&self, id: SectionId) -> Vec<SectionId> {
        let Some(shown) = self.section(id).map(|s| s.name.to_lowercase()) else { return Vec::new() };
        self.replica
            .sections
            .iter()
            .filter_map(|(member, record)| match record {
                SectionRecord::Live { name, .. } if name.value.to_lowercase() == shown => Some(*member),
                _ => None,
            })
            .collect()
    }

    fn locate(&self, id: NoteId) -> Option<Owner> {
        self.lists(Scope::All).iter().find(|l| l.notes.iter().any(|n| n.id == id)).map(|l| l.owner)
    }

    fn project(&mut self) {
        let mut live: Vec<(Stamp, SectionId, &str)> = self
            .replica
            .sections
            .iter()
            .filter_map(|(id, record)| match record {
                SectionRecord::Live { name, created } => Some((*created, *id, name.value.as_str())),
                SectionRecord::Removed => None,
            })
            .collect();
        live.sort_unstable();
        let mut sections: Vec<Section> = Vec::new();
        let mut named: HashMap<String, usize> = HashMap::new();
        let mut shown_in: HashMap<SectionId, usize> = HashMap::new();
        for (_, id, name) in live {
            let at = *named.entry(name.to_lowercase()).or_insert_with(|| {
                sections.push(Section { id, name: name.to_owned(), notes: Vec::new() });
                sections.len() - 1
            });
            shown_in.insert(id, at);
        }
        let mut placed: Vec<(Option<usize>, &Rank, NoteId, &String, bool)> = self
            .replica
            .notes
            .iter()
            .filter_map(|(id, record)| match record {
                NoteRecord::Live { text, done, place } => {
                    let list = place.value.section.and_then(|s| shown_in.get(&s).copied());
                    Some((list, &place.value.rank, *id, text, done.value))
                }
                NoteRecord::Removed => None,
            })
            .collect();
        placed.sort_unstable_by(|a, b| (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)));
        let mut loose = Vec::new();
        for (list, _, id, text, done) in placed {
            let note = Note { id, text: text.clone(), done };
            match list {
                Some(at) => sections[at].notes.push(note),
                None => loose.push(note),
            }
        }
        self.active = self.active.and_then(|id| shown_in.get(&id)).map(|&at| sections[at].id);
        self.view = View { loose, sections };
    }
}

impl Section {
    fn list(&self) -> List<'_> {
        List { owner: Owner::Section(self.id), name: Some(&self.name), notes: &self.notes }
    }
}

fn section_command(text: &str) -> Option<&str> {
    let name = text.strip_prefix("# ")?.trim();
    (!name.is_empty() && !name.contains('\n')).then_some(name)
}

fn wall_clock() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}
