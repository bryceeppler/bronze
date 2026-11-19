use std::fmt;

use super::seal::Unopenable;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Off,
    Connecting,
    Synced,
    Offline(Offline),
    Refused(Refusal),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Offline {
    Unreachable(String),
    ServerError(u16),
    Unexpected(u16),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    WrongKey,
    Declined(String),
    TooLarge,
    NewerFormat,
    Damaged,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    Offline(Offline),
    Refused(Refusal),
}

impl From<Failure> for Status {
    fn from(failure: Failure) -> Status {
        match failure {
            Failure::Offline(offline) => Status::Offline(offline),
            Failure::Refused(refusal) => Status::Refused(refusal),
        }
    }
}

impl From<Unopenable> for Refusal {
    fn from(why: Unopenable) -> Refusal {
        match why {
            Unopenable::Newer => Refusal::NewerFormat,
            Unopenable::Damaged => Refusal::Damaged,
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Status::Off => f.write_str("off"),
            Status::Connecting => f.write_str("connecting"),
            Status::Synced => f.write_str("synced"),
            Status::Offline(Offline::Unreachable(why)) => write!(f, "offline: {why}"),
            Status::Offline(Offline::ServerError(code)) => write!(f, "offline: the server failed with {code}"),
            Status::Offline(Offline::Unexpected(code)) => write!(f, "offline: the server answered {code}"),
            Status::Refused(Refusal::WrongKey) => f.write_str("refused: the server does not accept this key"),
            Status::Refused(Refusal::Declined(why)) => write!(f, "refused: {why}"),
            Status::Refused(Refusal::TooLarge) => f.write_str("refused: the board is larger than the server allows"),
            Status::Refused(Refusal::NewerFormat) => {
                f.write_str("refused: a newer Bronze wrote the server's copy, so update Bronze")
            }
            Status::Refused(Refusal::Damaged) => f.write_str("refused: the server's copy is damaged"),
        }
    }
}
