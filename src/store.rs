#[cfg(test)]
mod tests;

use std::cmp::Ordering;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io, thread};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::board::{
    Board, DeviceId, NoteId, NoteRecord, Position, Rank, Reg, Replica, Scope, SectionId, SectionRecord, Stamp,
};
use crate::prefs::{Frame, Prefs};

pub trait Document: Clone + Default + Send + 'static {
    const FILE_NAME: &'static str;
    const VERSION: u32;
    type File: Serialize + DeserializeOwned;

    fn to_file(&self) -> Self::File;
    fn from_file(file: Self::File) -> Self;

    fn from_older(bytes: &[u8]) -> serde_json::Result<Self> {
        body(bytes).map(Self::from_file)
    }
}

pub enum Store<T> {
    Memory,
    File(Writer<T>),
}

pub struct Writer<T> {
    tx: Sender<Message<T>>,
    path: PathBuf,
}

enum Message<T> {
    Save(T),
    Flush(Sender<()>),
}

#[derive(Serialize, Deserialize)]
struct Versioned<F> {
    version: u32,
    #[serde(flatten)]
    body: F,
}

#[derive(Serialize, Deserialize)]
pub struct NotesFile {
    device: DeviceId,
    active: Option<String>,
    loose: Vec<NoteEntry>,
    sections: Vec<SectionEntry>,
    removed: RemovedIds,
}

#[derive(Serialize, Deserialize)]
struct NoteEntry {
    id: NoteId,
    text: String,
    done: bool,
    rank: Rank,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    section: Option<SectionId>,
    done_stamp: Stamp,
    place_stamp: Stamp,
}

#[derive(Serialize, Deserialize)]
struct SectionEntry {
    id: SectionId,
    name: String,
    name_stamp: Stamp,
    created: Stamp,
    notes: Vec<NoteEntry>,
}

#[derive(Serialize, Deserialize)]
struct RemovedIds {
    notes: Vec<NoteId>,
    sections: Vec<SectionId>,
}

#[derive(Deserialize)]
struct V1Notes {
    active: Option<String>,
    loose: Vec<V1Note>,
    sections: Vec<V1Section>,
}

#[derive(Deserialize)]
struct V1Note {
    text: String,
    #[serde(default)]
    done: bool,
}

#[derive(Deserialize)]
struct V1Section {
    name: String,
    notes: Vec<V1Note>,
}

#[derive(Serialize, Deserialize)]
pub struct PrefsFile {
    keep_on_top: bool,
    scope: ScopeEntry,
    frame: Option<FrameEntry>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ScopeEntry {
    All,
    Active,
}

#[derive(Serialize, Deserialize)]
struct FrameEntry {
    x: i32,
    y: i32,
    width: f32,
    height: f32,
}

#[derive(Deserialize)]
struct Header {
    version: u32,
}

enum Unreadable {
    Io(io::Error),
    Malformed(serde_json::Error),
    Newer(u32),
}

pub fn data_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("ca", "bryce", "bronze").map(|dirs| dirs.data_dir().to_path_buf())
}

pub enum Unowned {
    Taken,
    Unlockable(io::Error),
}

impl std::fmt::Display for Unowned {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unowned::Taken => write!(f, "another Bronze already runs on it"),
            Unowned::Unlockable(error) => write!(f, "it cannot be locked: {error}"),
        }
    }
}

pub fn claim(dir: &Path) -> Result<fs::File, Unowned> {
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("bronze.lock"))
        .map_err(Unowned::Unlockable)?;
    match lock.try_lock() {
        Ok(()) => Ok(lock),
        Err(fs::TryLockError::WouldBlock) => Err(Unowned::Taken),
        Err(fs::TryLockError::Error(error)) => Err(Unowned::Unlockable(error)),
    }
}

pub fn open<T: Document>(dir: &Path) -> (T, Store<T>) {
    if let Err(error) = fs::create_dir_all(dir) {
        eprintln!("bronze: cannot create {}: {error}; {} stays in memory", dir.display(), T::FILE_NAME);
        return (T::default(), Store::Memory);
    }
    let path = dir.join(T::FILE_NAME);
    match load(&path) {
        Ok((document, version)) if version < T::VERSION => match keep_older(&path, version) {
            Ok(_) => (document, Store::File(Writer::spawn(path))),
            Err(error) => {
                eprintln!(
                    "bronze: cannot copy {} before upgrading it ({error}); it stays as it is and its contents stay in memory",
                    path.display()
                );
                (document, Store::Memory)
            }
        },
        Ok((document, _)) => (document, Store::File(Writer::spawn(path))),
        Err(why) => match set_aside(&path) {
            Ok(aside) => {
                eprintln!("bronze: {} was unreadable ({why}); moved it to {}", path.display(), aside.display());
                (T::default(), Store::File(Writer::spawn(path)))
            }
            Err(error) => {
                eprintln!(
                    "bronze: {} was unreadable ({why}) and could not be moved aside ({error}); it stays in memory",
                    path.display()
                );
                (T::default(), Store::Memory)
            }
        },
    }
}

impl<T> Clone for Store<T> {
    fn clone(&self) -> Store<T> {
        match self {
            Store::Memory => Store::Memory,
            Store::File(Writer { tx, path }) => Store::File(Writer { tx: tx.clone(), path: path.clone() }),
        }
    }
}

impl<T: Document> Store<T> {
    pub fn save(&self, document: &T) {
        if let Store::File(Writer { tx, .. }) = self {
            let _ = tx.send(Message::Save(document.clone()));
        }
    }

    pub fn path(&self) -> Option<&Path> {
        match self {
            Store::Memory => None,
            Store::File(writer) => Some(&writer.path),
        }
    }

    pub fn flush(&self) {
        if let Store::File(Writer { tx, .. }) = self {
            let (ack, done) = mpsc::channel();
            if tx.send(Message::Flush(ack)).is_ok() {
                let _ = done.recv();
            }
        }
    }
}

impl<T: Document> Writer<T> {
    fn spawn(path: PathBuf) -> Writer<T> {
        let (tx, rx) = mpsc::channel();
        let file = path.clone();
        thread::Builder::new()
            .name("bronze-store".into())
            .spawn(move || write_loop(&file, &rx))
            .expect("spawn the store writer");
        Writer { tx, path }
    }
}

fn write_loop<T: Document>(path: &Path, rx: &Receiver<Message<T>>) {
    while let Ok(first) = rx.recv() {
        let mut latest = None;
        for message in std::iter::once(first).chain(rx.try_iter()) {
            match message {
                Message::Save(document) => latest = Some(document),
                Message::Flush(ack) => {
                    if let Some(document) = latest.take() {
                        write(path, &document);
                    }
                    let _ = ack.send(());
                }
            }
        }
        if let Some(document) = latest {
            write(path, &document);
        }
    }
}

fn write<T: Document>(path: &Path, document: &T) {
    let versioned = Versioned { version: T::VERSION, body: document.to_file() };
    let json = serde_json::to_string_pretty(&versioned).expect("documents serialize");
    let temp = path.with_extension("json.tmp");
    let written = fs::File::create(&temp)
        .and_then(|mut file| {
            file.write_all((json + "\n").as_bytes())?;
            file.sync_all()
        })
        .and_then(|()| fs::rename(&temp, path));
    if let Err(error) = written {
        eprintln!("bronze: cannot save {}: {error}", path.display());
    }
}

fn load<T: Document>(path: &Path) -> Result<(T, u32), Unreadable> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok((T::default(), T::VERSION)),
        Err(error) => return Err(Unreadable::Io(error)),
    };
    let header: Header = serde_json::from_slice(&bytes).map_err(Unreadable::Malformed)?;
    let document = match header.version.cmp(&T::VERSION) {
        Ordering::Greater => return Err(Unreadable::Newer(header.version)),
        Ordering::Equal => body(&bytes).map(T::from_file),
        Ordering::Less => T::from_older(&bytes),
    };
    Ok((document.map_err(Unreadable::Malformed)?, header.version))
}

fn body<F: DeserializeOwned>(bytes: &[u8]) -> serde_json::Result<F> {
    serde_json::from_slice::<Versioned<F>>(bytes).map(|file| file.body)
}

fn keep_older(path: &Path, version: u32) -> io::Result<PathBuf> {
    let kept = path.with_extension(format!("v{version}.json"));
    fs::copy(path, &kept)?;
    Ok(kept)
}

fn set_aside(path: &Path) -> io::Result<PathBuf> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let aside = path.with_extension(format!("unreadable-{stamp}.json"));
    fs::rename(path, &aside)?;
    Ok(aside)
}

impl std::fmt::Display for Unreadable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unreadable::Io(error) => write!(f, "{error}"),
            Unreadable::Malformed(error) => write!(f, "{error}"),
            Unreadable::Newer(version) => write!(f, "version {version} is newer than this build reads"),
        }
    }
}

impl Document for Board {
    const FILE_NAME: &'static str = "notes.json";
    const VERSION: u32 = 2;
    type File = NotesFile;

    fn to_file(&self) -> NotesFile {
        let replica = self.replica();
        let mut sections: Vec<SectionEntry> = replica
            .sections
            .iter()
            .filter_map(|(&id, record)| match record {
                SectionRecord::Live { name, created } => Some(SectionEntry {
                    id,
                    name: name.value.clone(),
                    name_stamp: name.at,
                    created: *created,
                    notes: Vec::new(),
                }),
                SectionRecord::Removed => None,
            })
            .collect();
        sections.sort_unstable_by_key(|s| (s.created, s.id));
        let nested: HashMap<SectionId, usize> = sections.iter().enumerate().map(|(at, s)| (s.id, at)).collect();
        let mut notes: Vec<NoteEntry> = replica
            .notes
            .iter()
            .filter_map(|(&id, record)| match record {
                NoteRecord::Live { text, done, place } => Some(NoteEntry {
                    id,
                    text: text.clone(),
                    done: done.value,
                    rank: place.value.rank.clone(),
                    section: place.value.section,
                    done_stamp: done.at,
                    place_stamp: place.at,
                }),
                NoteRecord::Removed => None,
            })
            .collect();
        notes.sort_unstable_by(|a, b| (&a.rank, a.id).cmp(&(&b.rank, b.id)));
        let mut loose = Vec::new();
        for mut note in notes {
            match note.section.and_then(|id| nested.get(&id)) {
                Some(&at) => {
                    note.section = None;
                    sections[at].notes.push(note);
                }
                None => loose.push(note),
            }
        }
        NotesFile {
            device: self.device(),
            active: self.active().map(|s| s.name.clone()),
            loose,
            sections,
            removed: RemovedIds {
                notes: replica
                    .notes
                    .iter()
                    .filter(|(_, r)| matches!(r, NoteRecord::Removed))
                    .map(|(&id, _)| id)
                    .collect(),
                sections: replica
                    .sections
                    .iter()
                    .filter(|(_, r)| matches!(r, SectionRecord::Removed))
                    .map(|(&id, _)| id)
                    .collect(),
            },
        }
    }

    fn from_file(file: NotesFile) -> Board {
        let mut replica = Replica::default();
        for note in file.loose {
            replica.notes.insert(note.id, note.into_record(None));
        }
        for section in file.sections {
            for note in section.notes {
                replica.notes.insert(note.id, note.into_record(Some(section.id)));
            }
            let name = Reg { value: section.name, at: section.name_stamp };
            replica.sections.insert(section.id, SectionRecord::Live { name, created: section.created });
        }
        replica.notes.extend(file.removed.notes.into_iter().map(|id| (id, NoteRecord::Removed)));
        replica.sections.extend(file.removed.sections.into_iter().map(|id| (id, SectionRecord::Removed)));
        Board::load(file.device, &replica, file.active.as_deref())
    }

    fn from_older(bytes: &[u8]) -> serde_json::Result<Board> {
        let file: V1Notes = body(bytes)?;
        let device = DeviceId::random(&mut fastrand::Rng::new());
        let migrated = Stamp { time: 0, device };
        let mut replica = Replica::default();
        let mut lists = vec![(None, file.loose)];
        for (order, section) in (0..).zip(file.sections) {
            let id = SectionId::named(&section.name);
            let name = Reg { value: section.name, at: migrated };
            let created = Stamp { time: order, device };
            replica.sections.entry(id).or_insert(SectionRecord::Live { name, created });
            lists.push((Some(id), section.notes));
        }
        let mut placed: HashMap<Option<SectionId>, u32> = HashMap::new();
        let mut copies: HashMap<(Option<SectionId>, String), u64> = HashMap::new();
        for (section, notes) in lists {
            for V1Note { text, done } in notes {
                let occurrence = copies.entry((section, text.clone())).or_default();
                let id = NoteId::listed(section, &text, *occurrence);
                *occurrence += 1;
                let order = placed.entry(section).or_default();
                let place = Position { section, rank: Rank::nth(*order) };
                *order += 1;
                let record = NoteRecord::Live {
                    text,
                    done: Reg { value: done, at: migrated },
                    place: Reg { value: place, at: migrated },
                };
                replica.notes.insert(id, record);
            }
        }
        Ok(Board::load(device, &replica, file.active.as_deref()))
    }
}

impl NoteEntry {
    fn into_record(self, nested_in: Option<SectionId>) -> NoteRecord {
        NoteRecord::Live {
            text: self.text,
            done: Reg { value: self.done, at: self.done_stamp },
            place: Reg {
                value: Position { section: self.section.or(nested_in), rank: self.rank },
                at: self.place_stamp,
            },
        }
    }
}

impl Document for Prefs {
    const FILE_NAME: &'static str = "preferences.json";
    const VERSION: u32 = 1;
    type File = PrefsFile;

    fn to_file(&self) -> PrefsFile {
        PrefsFile {
            keep_on_top: self.keep_on_top,
            scope: match self.scope {
                Scope::All => ScopeEntry::All,
                Scope::Active => ScopeEntry::Active,
            },
            frame: self.frame.map(|f| FrameEntry {
                x: f.position.0,
                y: f.position.1,
                width: f.size.0,
                height: f.size.1,
            }),
        }
    }

    fn from_file(file: PrefsFile) -> Prefs {
        Prefs {
            keep_on_top: file.keep_on_top,
            scope: match file.scope {
                ScopeEntry::All => Scope::All,
                ScopeEntry::Active => Scope::Active,
            },
            frame: file.frame.map(|f| Frame { position: (f.x, f.y), size: (f.width, f.height) }),
        }
    }
}
