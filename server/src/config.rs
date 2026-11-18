use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;

use bronze_wire::{DEFAULT_LISTEN, DEFAULT_MAX_BYTES, DEFAULT_MAX_VAULTS};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

pub struct Config {
    pub listen: SocketAddr,
    pub data: PathBuf,
    pub limits: Limits,
}

#[derive(Debug)]
pub struct Limits {
    pub max_bytes: usize,
    pub max_vaults: u64,
    pub setup_token: Option<SetupToken>,
}

impl Default for Limits {
    fn default() -> Self {
        Self { max_bytes: DEFAULT_MAX_BYTES as usize, max_vaults: DEFAULT_MAX_VAULTS, setup_token: None }
    }
}

pub struct SetupToken([u8; 32]);

impl SetupToken {
    pub fn new(secret: &str) -> Self {
        Self(Sha256::digest(secret.as_bytes()).into())
    }

    pub fn admits(&self, presented: Option<&[u8]>) -> bool {
        presented.is_some_and(|presented| {
            let presented: [u8; 32] = Sha256::digest(presented).into();
            self.0.ct_eq(&presented).into()
        })
    }
}

impl fmt::Debug for SetupToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SetupToken(..)")
    }
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        Self::from_vars(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
    }

    fn from_vars(var: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let parse = |name: &str, default: &str| -> Result<u64, String> { parse_var(var(name).as_deref(), name, default) };
        let max_bytes = parse("BRONZE_SYNC_MAX_BYTES", &DEFAULT_MAX_BYTES.to_string())?;
        Ok(Self {
            listen: parse_var(var("BRONZE_SYNC_LISTEN").as_deref(), "BRONZE_SYNC_LISTEN", DEFAULT_LISTEN)?,
            data: var("BRONZE_SYNC_DATA")
                .map(PathBuf::from)
                .ok_or("BRONZE_SYNC_DATA must name the folder that holds the database")?,
            limits: Limits {
                max_bytes: usize::try_from(max_bytes).map_err(|_| "BRONZE_SYNC_MAX_BYTES is too large")?,
                max_vaults: parse("BRONZE_SYNC_MAX_VAULTS", &DEFAULT_MAX_VAULTS.to_string())?,
                setup_token: var("BRONZE_SYNC_SETUP_TOKEN").as_deref().map(SetupToken::new),
            },
        })
    }
}

fn parse_var<T: FromStr>(value: Option<&str>, name: &str, default: &str) -> Result<T, String> {
    let value = value.unwrap_or(default);
    value.parse().map_err(|_| format!("{name} cannot be {value:?}"))
}
