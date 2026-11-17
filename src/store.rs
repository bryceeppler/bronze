#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io, thread};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::board::{Board, Note, Restored, Scope};
use crate::prefs::{Frame, Prefs};

pub trait Document: Clone + Default + Send + 'static {
    const FILE_NAME: &'static str;
    const VERSION: u32;
    type File: Serialize + DeserializeOwned;

    fn to_file(&self) -> Self::File;
    fn from_file(file: Self::File) -> Self;
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
    active: Option<String>,
    loose: Vec<NoteEntry>,
    sections: Vec<SectionEntry>,
}

#[derive(Serialize, Deserialize)]
struct NoteEntry {
    text: String,
    #[serde(default)]
    done: bool,
}

#[derive(Serialize, Deserialize)]
struct SectionEntry {
    name: String,
    notes: Vec<NoteEntry>,
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
        Ok(document) => (document, Store::File(Writer::spawn(path))),
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
    let written = fs::write(&temp, json + "\n")
        .and_then(|()| fs::File::open(&temp)?.sync_all())
        .and_then(|()| fs::rename(&temp, path));
    if let Err(error) = written {
        eprintln!("bronze: cannot save {}: {error}", path.display());
    }
}

fn load<T: Document>(path: &Path) -> Result<T, Unreadable> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(T::default()),
        Err(error) => return Err(Unreadable::Io(error)),
    };
    let header: Header = serde_json::from_slice(&bytes).map_err(Unreadable::Malformed)?;
    if header.version > T::VERSION {
        return Err(Unreadable::Newer(header.version));
    }
    let file: Versioned<T::File> = serde_json::from_slice(&bytes).map_err(Unreadable::Malformed)?;
    Ok(T::from_file(file.body))
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

impl From<&Note> for NoteEntry {
    fn from(note: &Note) -> NoteEntry {
        NoteEntry { text: note.text.clone(), done: note.done }
    }
}

impl Document for Board {
    const FILE_NAME: &'static str = "notes.json";
    const VERSION: u32 = 1;
    type File = NotesFile;

    fn to_file(&self) -> NotesFile {
        NotesFile {
            active: self.active().map(|s| s.name.clone()),
            loose: self.loose().iter().map(NoteEntry::from).collect(),
            sections: self
                .sections()
                .iter()
                .map(|s| SectionEntry { name: s.name.clone(), notes: s.notes.iter().map(NoteEntry::from).collect() })
                .collect(),
        }
    }

    fn from_file(file: NotesFile) -> Board {
        let notes = |entries: Vec<NoteEntry>| entries.into_iter().map(|n| (n.text, n.done)).collect();
        Board::restore(Restored {
            loose: notes(file.loose),
            sections: file.sections.into_iter().map(|s| (s.name, notes(s.notes))).collect(),
            active: file.active,
        })
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
