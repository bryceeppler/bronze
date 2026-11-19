mod config;
mod http;
mod store;

pub use config::{Config, Limits};
pub use http::serve;
pub use store::Store;

use bronze_wire::{Malformed, Version};

#[derive(Debug)]
pub enum Reject {
    NoVault,
    Unauthorized,
    Malformed(Malformed),
    PreconditionRequired,
    PreconditionFailed(Option<Version>),
    TooLarge,
    VaultLimitReached,
    Internal(Box<dyn std::error::Error + Send + Sync>),
}

impl From<rusqlite::Error> for Reject {
    fn from(error: rusqlite::Error) -> Self {
        Self::Internal(error.into())
    }
}
