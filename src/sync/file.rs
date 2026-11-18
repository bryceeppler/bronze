use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::address::{AddressRefusal, ServerAddress};
use super::code::SyncCode;
use super::key::{KeyRefusal, SyncKey};

pub const FILE_NAME: &str = "sync.json";
const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncFile(PathBuf);

#[derive(Serialize, Deserialize)]
struct Fields {
    version: u32,
    server: String,
    key: String,
}

#[derive(Debug)]
pub enum Unreadable {
    Io(io::Error),
    NotJson { line: usize, column: usize },
    Newer(u32),
    Server(AddressRefusal),
    Key(KeyRefusal),
}

impl SyncFile {
    pub fn in_folder(folder: &Path) -> SyncFile {
        SyncFile(folder.join(FILE_NAME))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn read(&self) -> Result<Option<SyncCode>, Unreadable> {
        let bytes = match fs::read(&self.0) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(Unreadable::Io(error)),
        };
        let fields: Fields = serde_json::from_slice(&bytes)
            .map_err(|error| Unreadable::NotJson { line: error.line(), column: error.column() })?;
        if fields.version > VERSION {
            return Err(Unreadable::Newer(fields.version));
        }
        Ok(Some(SyncCode {
            server: ServerAddress::parse(&fields.server).map_err(Unreadable::Server)?,
            key: SyncKey::parse(&fields.key).map_err(Unreadable::Key)?,
        }))
    }

    pub fn write(&self, code: &SyncCode) -> io::Result<()> {
        let fields = Fields { version: VERSION, server: code.server.to_string(), key: code.key.reveal() };
        let json = serde_json::to_string_pretty(&fields).map_err(io::Error::other)? + "\n";
        let temp = self.0.with_extension("json.tmp");
        match fs::remove_file(&temp) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&temp)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temp, &self.0)
    }

    pub fn remove(&self) -> io::Result<()> {
        match fs::remove_file(&self.0) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }
}

impl fmt::Display for Unreadable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Unreadable::Io(error) => write!(f, "{error}"),
            Unreadable::NotJson { line, column } => {
                write!(f, "it is not the expected JSON (line {line}, column {column})")
            }
            Unreadable::Newer(version) => write!(f, "version {version} is newer than this build reads"),
            Unreadable::Server(AddressRefusal::NotAddress) => f.write_str("its server is not a server address"),
            Unreadable::Server(AddressRefusal::NotSecure) => {
                f.write_str("its server is not https, and plain http only reaches this machine")
            }
            Unreadable::Key(KeyRefusal::Malformed) => f.write_str("its key is not a Sync Key"),
            Unreadable::Key(KeyRefusal::CheckFails) => f.write_str("its key fails its check, so it was mistyped"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::Setup;

    const KEY: &str = "bronze1-000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f-630dcd29";

    fn code() -> SyncCode {
        Setup::parse(&format!("https://sync.example.com#{KEY}")).unwrap().code()
    }

    #[test]
    fn no_file_means_sync_is_off() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(SyncFile::in_folder(dir.path()).read().unwrap(), None);
    }

    #[test]
    fn a_written_file_reads_back_the_same_code() {
        let dir = tempfile::tempdir().unwrap();
        let file = SyncFile::in_folder(dir.path());
        file.write(&code()).unwrap();
        assert_eq!(file.read().unwrap(), Some(code()));
        let json: serde_json::Value = serde_json::from_slice(&fs::read(file.path()).unwrap()).unwrap();
        assert_eq!(json, serde_json::json!({ "version": 1, "server": "https://sync.example.com", "key": KEY }));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1, "no temporary file is left behind");
    }

    #[cfg(unix)]
    #[test]
    fn only_its_owner_can_read_the_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let file = SyncFile::in_folder(dir.path());
        fs::write(file.path().with_extension("json.tmp"), "left by a crash").unwrap();
        file.write(&code()).unwrap();
        assert_eq!(fs::metadata(file.path()).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn removing_the_file_turns_sync_off_and_is_safe_to_repeat() {
        let dir = tempfile::tempdir().unwrap();
        let file = SyncFile::in_folder(dir.path());
        file.write(&code()).unwrap();
        file.remove().unwrap();
        file.remove().unwrap();
        assert_eq!(file.read().unwrap(), None);
    }

    #[test]
    fn an_unreadable_file_says_why_without_showing_the_key() {
        let dir = tempfile::tempdir().unwrap();
        let file = SyncFile::in_folder(dir.path());
        let mistyped = KEY.replace("630dcd29", "630dcd28");
        for (json, why) in [
            ("{".to_owned(), "it is not the expected JSON (line 1, column 1)"),
            (
                format!(r#"{{"version":1,"server":"https://sync.example.com","key":{{"k":"{KEY}"}}}}"#),
                "it is not the expected JSON (line 1, column 55)",
            ),
            (
                format!(r#"{{"version":2,"server":"https://sync.example.com","key":"{KEY}"}}"#),
                "version 2 is newer than this build reads",
            ),
            (
                format!(r#"{{"version":1,"server":"http://sync.example.com","key":"{KEY}"}}"#),
                "its server is not https, and plain http only reaches this machine",
            ),
            (
                format!(r#"{{"version":1,"server":"https://sync.example.com","key":"{mistyped}"}}"#),
                "its key fails its check, so it was mistyped",
            ),
        ] {
            fs::write(file.path(), &json).unwrap();
            let unreadable = file.read().unwrap_err();
            assert_eq!(unreadable.to_string(), why);
            assert!(!format!("{unreadable:?}").contains("0001020304"));
        }
    }
}
