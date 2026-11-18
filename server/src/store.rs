use std::path::Path;
use std::sync::Mutex;

use bronze_wire::{AuthToken, Precondition, VaultId, Version};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::{Limits, Reject};

const DATABASE: &str = "bronze-sync.sqlite3";

const SCHEMA: &str = "
    PRAGMA journal_mode = WAL;
    PRAGMA synchronous = FULL;
    CREATE TABLE IF NOT EXISTS vaults (
        id BLOB PRIMARY KEY,
        token_sha256 BLOB NOT NULL,
        version BLOB NOT NULL,
        bytes BLOB NOT NULL
    ) STRICT;
";

pub struct Store(Mutex<Connection>);

pub enum Read {
    Changed { version: Version, bytes: Vec<u8> },
    Unchanged(Version),
}

pub enum Written {
    Created(Version),
    Replaced(Version),
}

pub struct Write {
    pub vault: VaultId,
    pub token: AuthToken,
    pub precondition: Precondition,
    pub bytes: Vec<u8>,
}

impl Store {
    pub fn open(folder: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        std::fs::create_dir_all(folder)?;
        let connection = Connection::open(folder.join(DATABASE))?;
        connection.execute_batch(SCHEMA)?;
        Ok(Self(Mutex::new(connection)))
    }

    pub(crate) fn read(&self, vault: &VaultId, token: &AuthToken, seen: Option<Version>) -> Result<Read, Reject> {
        let connection = self.0.lock().map_err(|_| Reject::Internal("store lock poisoned".into()))?;
        let (token_sha256, version, bytes) = connection
            .query_row(
                "SELECT token_sha256, version, CASE WHEN version = ?2 THEN NULL ELSE bytes END FROM vaults WHERE id = ?1",
                params![vault.as_bytes(), seen.as_ref().map(Version::as_bytes)],
                |row| Ok((row.get(0)?, Version::from_bytes(row.get(1)?), row.get::<_, Option<Vec<u8>>>(2)?)),
            )
            .optional()?
            .ok_or(Reject::NoVault)?;
        authorize(&token_sha256, token)?;
        Ok(match bytes {
            Some(bytes) => Read::Changed { version, bytes },
            None => Read::Unchanged(version),
        })
    }

    pub(crate) fn write(&self, write: Write, limits: &Limits) -> Result<Written, Reject> {
        let mut connection = self.0.lock().map_err(|_| Reject::Internal("store lock poisoned".into()))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = transaction
            .query_row("SELECT token_sha256, version FROM vaults WHERE id = ?1", [write.vault.as_bytes()], |row| {
                Ok((row.get(0)?, Version::from_bytes(row.get(1)?)))
            })
            .optional()?;
        let version = fresh_version()?;
        let written = match (current, write.precondition) {
            (Some((token_sha256, current)), precondition) => {
                authorize(&token_sha256, &write.token)?;
                if precondition != Precondition::Replace(current) {
                    return Err(Reject::PreconditionFailed(Some(current)));
                }
                transaction.execute(
                    "UPDATE vaults SET version = ?2, bytes = ?3 WHERE id = ?1",
                    params![write.vault.as_bytes(), version.as_bytes(), write.bytes],
                )?;
                Written::Replaced(version)
            }
            (None, Precondition::Replace(_)) => return Err(Reject::PreconditionFailed(None)),
            (None, Precondition::Create) => {
                let vaults: i64 = transaction.query_row("SELECT COUNT(*) FROM vaults", [], |row| row.get(0))?;
                if vaults.unsigned_abs() >= limits.max_vaults {
                    return Err(Reject::VaultLimitReached);
                }
                transaction.execute(
                    "INSERT INTO vaults (id, token_sha256, version, bytes) VALUES (?1, ?2, ?3, ?4)",
                    params![write.vault.as_bytes(), token_sha256(&write.token), version.as_bytes(), write.bytes],
                )?;
                Written::Created(version)
            }
        };
        transaction.commit()?;
        Ok(written)
    }
}

fn token_sha256(token: &AuthToken) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

fn authorize(stored: &[u8; 32], token: &AuthToken) -> Result<(), Reject> {
    if bool::from(stored.ct_eq(&token_sha256(token))) { Ok(()) } else { Err(Reject::Unauthorized) }
}

fn fresh_version() -> Result<Version, Reject> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|error| Reject::Internal(error.to_string().into()))?;
    Ok(Version::from_bytes(bytes))
}
