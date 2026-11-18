use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;

use bronze_wire::{DEFAULT_LISTEN, DEFAULT_MAX_BYTES, DEFAULT_MAX_VAULTS};

pub struct Config {
    pub listen: SocketAddr,
    pub data: PathBuf,
    pub limits: Limits,
}

#[derive(Debug)]
pub struct Limits {
    pub max_bytes: usize,
    pub max_vaults: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self { max_bytes: DEFAULT_MAX_BYTES as usize, max_vaults: DEFAULT_MAX_VAULTS }
    }
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let max_bytes: u64 = var("BRONZE_SYNC_MAX_BYTES", &DEFAULT_MAX_BYTES.to_string())?;
        Ok(Self {
            listen: var("BRONZE_SYNC_LISTEN", DEFAULT_LISTEN)?,
            data: std::env::var_os("BRONZE_SYNC_DATA")
                .filter(|data| !data.is_empty())
                .map(PathBuf::from)
                .ok_or("BRONZE_SYNC_DATA must name the folder that holds the database")?,
            limits: Limits {
                max_bytes: usize::try_from(max_bytes).map_err(|_| "BRONZE_SYNC_MAX_BYTES is too large")?,
                max_vaults: var("BRONZE_SYNC_MAX_VAULTS", &DEFAULT_MAX_VAULTS.to_string())?,
            },
        })
    }
}

fn var<T: FromStr>(name: &str, default: &str) -> Result<T, String> {
    let value = std::env::var(name).ok().filter(|value| !value.is_empty()).unwrap_or_else(|| default.to_owned());
    value.parse().map_err(|_| format!("{name} cannot be {value:?}"))
}
