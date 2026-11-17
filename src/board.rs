#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NoteId(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SectionId(u64);

impl std::fmt::Display for NoteId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Display for SectionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

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

#[derive(Clone, Debug, Default)]
pub struct Board {
    loose: Vec<Note>,
    sections: Vec<Section>,
    active: Option<SectionId>,
    next_id: u64,
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

impl Board {
    pub fn restore(restored: Restored) -> Board {
        let mut board = Board::default();
        board.loose = restored.loose.into_iter().map(|(text, done)| board.note(text, done)).collect();
        for (name, notes) in restored.sections {
            let id = board.open_section(&name);
            let notes: Vec<Note> = notes.into_iter().map(|(text, done)| board.note(text, done)).collect();
            if let Some(section) = board.section_mut(id) {
                section.notes.extend(notes);
            }
        }
        board.active = restored.active.and_then(|name| board.section_named(&name)).map(|s| s.id);
        board
    }

    pub fn loose(&self) -> &[Note] {
        &self.loose
    }

    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

    pub fn active(&self) -> Option<&Section> {
        self.active.and_then(|id| self.section(id))
    }

    pub fn section(&self, id: SectionId) -> Option<&Section> {
        self.sections.iter().find(|s| s.id == id)
    }

    pub fn notes(&self) -> impl Iterator<Item = &Note> {
        self.loose.iter().chain(self.sections.iter().flat_map(|s| &s.notes))
    }

    pub fn lists(&self, scope: Scope) -> Vec<List<'_>> {
        match (scope, self.active()) {
            (Scope::Active, Some(active)) => vec![active.list()],
            _ => std::iter::once(List { owner: Owner::Loose, name: None, notes: &self.loose })
                .chain(self.sections.iter().map(Section::list))
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
        let note = self.note(text.into(), false);
        let id = note.id;
        self.list_mut(self.active.map_or(Owner::Loose, Owner::Section)).push(note);
        id
    }

    pub fn toggle(&mut self, ids: &[NoteId]) {
        let all_done = self.notes().filter(|n| ids.contains(&n.id)).all(|n| n.done);
        for note in self.notes_mut().filter(|n| ids.contains(&n.id)) {
            note.done = !all_done;
        }
    }

    pub fn remove(&mut self, ids: &[NoteId]) {
        for list in self.lists_mut() {
            list.retain(|n| !ids.contains(&n.id));
        }
    }

    pub fn merge(&mut self, ids: &[NoteId]) -> Option<NoteId> {
        let picked: Vec<&Note> = self.notes().filter(|n| ids.contains(&n.id)).collect();
        let [first, rest @ ..] = picked.as_slice() else { return None };
        if rest.is_empty() {
            return None;
        }
        let into = first.id;
        let text = picked.iter().map(|n| n.text.as_str()).collect::<Vec<_>>().join("\n\n");
        let done = picked.iter().all(|n| n.done);
        let folded: Vec<NoteId> = rest.iter().map(|n| n.id).collect();
        self.remove(&folded);
        if let Some(note) = self.notes_mut().find(|n| n.id == into) {
            note.text = text;
            note.done = done;
        }
        Some(into)
    }

    pub fn move_note(&mut self, id: NoteId, to: Place) {
        let Some((from_owner, from)) = self.locate(id) else { return };
        let target = match to {
            Place::Before(other) if other != id => self.locate(other),
            Place::Before(_) => None,
            Place::End(owner) => {
                self.lists(Scope::All).iter().find(|l| l.owner == owner).map(|l| (owner, l.notes.len()))
            }
        };
        let Some((owner, mut at)) = target else { return };
        if owner == from_owner && from < at {
            at -= 1;
        }
        let note = self.list_mut(from_owner).remove(from);
        self.list_mut(owner).insert(at, note);
    }

    pub fn clear_done(&mut self, scope: Scope) {
        match (scope, self.active) {
            (Scope::Active, Some(id)) => self.list_mut(Owner::Section(id)).retain(|n| !n.done),
            _ => {
                for list in self.lists_mut() {
                    list.retain(|n| !n.done);
                }
            }
        }
    }

    pub fn activate(&mut self, id: SectionId) {
        if self.section(id).is_some() {
            self.active = Some(id);
        }
    }

    pub fn rename_section(&mut self, id: SectionId, name: &str) -> bool {
        let name = name.trim();
        let taken = self.section_named(name).is_some_and(|s| s.id != id);
        match self.section_mut(id) {
            Some(section) if !name.is_empty() && !taken => {
                section.name = name.to_owned();
                true
            }
            _ => false,
        }
    }

    pub fn remove_section(&mut self, id: SectionId) {
        let Some(at) = self.sections.iter().position(|s| s.id == id) else { return };
        let section = self.sections.remove(at);
        self.loose.extend(section.notes);
        if self.active == Some(id) {
            self.active = None;
        }
    }

    pub fn copy_text(&self, ids: &[NoteId]) -> String {
        self.notes().filter(|n| ids.contains(&n.id)).map(|n| n.text.as_str()).collect::<Vec<_>>().join("\n\n")
    }

    fn next(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    fn note(&mut self, text: String, done: bool) -> Note {
        Note { id: NoteId(self.next()), text, done }
    }

    fn open_section(&mut self, name: &str) -> SectionId {
        if let Some(found) = self.section_named(name) {
            return found.id;
        }
        let id = SectionId(self.next());
        self.sections.push(Section { id, name: name.to_owned(), notes: Vec::new() });
        id
    }

    fn section_named(&self, name: &str) -> Option<&Section> {
        let name = name.to_lowercase();
        self.sections.iter().find(|s| s.name.to_lowercase() == name)
    }

    fn section_mut(&mut self, id: SectionId) -> Option<&mut Section> {
        self.sections.iter_mut().find(|s| s.id == id)
    }

    fn locate(&self, id: NoteId) -> Option<(Owner, usize)> {
        self.lists(Scope::All).iter().find_map(|l| l.notes.iter().position(|n| n.id == id).map(|at| (l.owner, at)))
    }

    fn list_mut(&mut self, owner: Owner) -> &mut Vec<Note> {
        let at = match owner {
            Owner::Section(id) => self.sections.iter().position(|s| s.id == id),
            Owner::Loose => None,
        };
        match at {
            Some(i) => &mut self.sections[i].notes,
            None => &mut self.loose,
        }
    }

    fn lists_mut(&mut self) -> impl Iterator<Item = &mut Vec<Note>> {
        std::iter::once(&mut self.loose).chain(self.sections.iter_mut().map(|s| &mut s.notes))
    }

    fn notes_mut(&mut self) -> impl Iterator<Item = &mut Note> {
        self.lists_mut().flat_map(|l| l.iter_mut())
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
