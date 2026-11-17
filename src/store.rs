#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io, thread};

use serde::{Deserialize, Serialize};

use crate::board::{Board, Note, Restored};

const VERSION: u32 = 1;
const FILE_NAME: &str = "notes.json";

#[derive(Clone)]
pub enum Store {
    Memory,
    File(Writer),
}

#[derive(Clone)]
pub struct Writer(Sender<Message>);

enum Message {
    Save(Board),
    Flush(Sender<()>),
}

#[derive(Serialize, Deserialize)]
struct NotesFile {
    version: u32,
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

pub fn open(dir: &Path) -> (Board, Store) {
    if let Err(error) = fs::create_dir_all(dir) {
        eprintln!("bronze: cannot create {}: {error}; notes stay in memory", dir.display());
        return (Board::default(), Store::Memory);
    }
    let path = dir.join(FILE_NAME);
    match load(&path) {
        Ok(board) => (board, Store::File(Writer::spawn(path))),
        Err(why) => match set_aside(&path) {
            Ok(aside) => {
                eprintln!("bronze: {} was unreadable ({why}); moved it to {}", path.display(), aside.display());
                (Board::default(), Store::File(Writer::spawn(path)))
            }
            Err(error) => {
                eprintln!("bronze: {} was unreadable ({why}) and could not be moved aside ({error}); notes stay in memory", path.display());
                (Board::default(), Store::Memory)
            }
        },
    }
}

impl Store {
    pub fn save(&self, board: &Board) {
        if let Store::File(Writer(tx)) = self {
            let _ = tx.send(Message::Save(board.clone()));
        }
    }

    pub fn flush(&self) {
        if let Store::File(Writer(tx)) = self {
            let (ack, done) = mpsc::channel();
            if tx.send(Message::Flush(ack)).is_ok() {
                let _ = done.recv();
            }
        }
    }
}

impl Writer {
    fn spawn(path: PathBuf) -> Writer {
        let (tx, rx) = mpsc::channel();
        thread::Builder::new().name("bronze-store".into()).spawn(move || write_loop(&path, &rx)).expect("spawn the store writer");
        Writer(tx)
    }
}

fn write_loop(path: &Path, rx: &Receiver<Message>) {
    while let Ok(first) = rx.recv() {
        let mut latest = None;
        for message in std::iter::once(first).chain(rx.try_iter()) {
            match message {
                Message::Save(board) => latest = Some(board),
                Message::Flush(ack) => {
                    if let Some(board) = latest.take() {
                        write(path, &board);
                    }
                    let _ = ack.send(());
                }
            }
        }
        if let Some(board) = latest {
            write(path, &board);
        }
    }
}

fn write(path: &Path, board: &Board) {
    let json = serde_json::to_string_pretty(&NotesFile::from(board)).expect("notes serialize");
    let temp = path.with_extension("json.tmp");
    let written = fs::write(&temp, json + "\n")
        .and_then(|()| fs::File::open(&temp)?.sync_all())
        .and_then(|()| fs::rename(&temp, path));
    if let Err(error) = written {
        eprintln!("bronze: cannot save {}: {error}", path.display());
    }
}

fn load(path: &Path) -> Result<Board, Unreadable> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Board::default()),
        Err(error) => return Err(Unreadable::Io(error)),
    };
    let header: Header = serde_json::from_slice(&bytes).map_err(Unreadable::Malformed)?;
    if header.version > VERSION {
        return Err(Unreadable::Newer(header.version));
    }
    let file: NotesFile = serde_json::from_slice(&bytes).map_err(Unreadable::Malformed)?;
    Ok(file.into())
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

impl From<&Board> for NotesFile {
    fn from(board: &Board) -> NotesFile {
        NotesFile {
            version: VERSION,
            active: board.active().map(|s| s.name.clone()),
            loose: board.loose().iter().map(NoteEntry::from).collect(),
            sections: board
                .sections()
                .iter()
                .map(|s| SectionEntry { name: s.name.clone(), notes: s.notes.iter().map(NoteEntry::from).collect() })
                .collect(),
        }
    }
}

impl From<NotesFile> for Board {
    fn from(file: NotesFile) -> Board {
        let notes = |entries: Vec<NoteEntry>| entries.into_iter().map(|n| (n.text, n.done)).collect();
        Board::restore(Restored {
            loose: notes(file.loose),
            sections: file.sections.into_iter().map(|s| (s.name, notes(s.notes))).collect(),
            active: file.active,
        })
    }
}
