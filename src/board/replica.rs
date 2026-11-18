#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::rank::Rank;

macro_rules! hex_id {
    ($name:ident, $int:ident, $width:literal) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(into = "String", try_from = "String")]
        pub struct $name($int);

        impl $name {
            pub fn random(rng: &mut fastrand::Rng) -> $name {
                $name(rng.$int(..))
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{:0width$x}", self.0, width = $width)
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> String {
                id.to_string()
            }
        }

        impl TryFrom<String> for $name {
            type Error = std::num::ParseIntError;

            fn try_from(text: String) -> Result<$name, Self::Error> {
                $int::from_str_radix(&text, 16).map($name)
            }
        }
    };
}

hex_id!(DeviceId, u64, 16);
hex_id!(NoteId, u128, 32);
hex_id!(SectionId, u128, 32);

impl SectionId {
    pub fn named(name: &str) -> SectionId {
        SectionId(fnv1a(&[b"section", name.to_lowercase().as_bytes()]))
    }
}

impl NoteId {
    pub fn listed(section: Option<SectionId>, text: &str, occurrence: u64) -> NoteId {
        let list = section.map(|id| id.0.to_le_bytes());
        NoteId(fnv1a(&[b"note", list.as_ref().map_or(&[], |id| id), text.as_bytes(), &occurrence.to_le_bytes()]))
    }
}

fn fnv1a(parts: &[&[u8]]) -> u128 {
    const OFFSET: u128 = 0x6c62272e07bb014262b821756295c58d;
    const PRIME: u128 = 0x0000000001000000000000000000013b;
    parts
        .iter()
        .flat_map(|part| (part.len() as u64).to_le_bytes().into_iter().chain(part.iter().copied()))
        .fold(OFFSET, |hash, byte| (hash ^ u128::from(byte)).wrapping_mul(PRIME))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Stamp {
    pub time: u64,
    pub device: DeviceId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reg<T> {
    pub value: T,
    pub at: Stamp,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    pub section: Option<SectionId>,
    pub rank: Rank,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoteRecord {
    Live { text: String, done: Reg<bool>, place: Reg<Position> },
    Removed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SectionRecord {
    Live { name: Reg<String>, created: Stamp },
    Removed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Replica {
    pub notes: BTreeMap<NoteId, NoteRecord>,
    pub sections: BTreeMap<SectionId, SectionRecord>,
}

impl Replica {
    pub fn join(&mut self, other: &Replica) {
        join_maps(&mut self.notes, &other.notes, NoteRecord::join);
        join_maps(&mut self.sections, &other.sections, SectionRecord::join);
    }

    pub fn latest(&self) -> u64 {
        let notes = self.notes.values().filter_map(|record| match record {
            NoteRecord::Live { done, place, .. } => Some(done.at.max(place.at)),
            NoteRecord::Removed => None,
        });
        let sections = self.sections.values().filter_map(|record| match record {
            SectionRecord::Live { name, created } => Some(name.at.max(*created)),
            SectionRecord::Removed => None,
        });
        notes.chain(sections).map(|stamp| stamp.time).max().unwrap_or(0)
    }
}

fn join_maps<K: Ord + Copy, V: Clone>(into: &mut BTreeMap<K, V>, from: &BTreeMap<K, V>, join: fn(&mut V, &V)) {
    for (key, value) in from {
        into.entry(*key).and_modify(|mine| join(mine, value)).or_insert_with(|| value.clone());
    }
}

impl<T: Ord + Clone> Reg<T> {
    fn join(&mut self, other: &Reg<T>) {
        if (other.at, &other.value) > (self.at, &self.value) {
            *self = other.clone();
        }
    }
}

impl NoteRecord {
    fn join(&mut self, other: &NoteRecord) {
        match (self, other) {
            (
                NoteRecord::Live { text, done, place },
                NoteRecord::Live { text: their_text, done: their_done, place: their_place },
            ) => {
                if their_text > text {
                    text.clone_from(their_text);
                }
                done.join(their_done);
                place.join(their_place);
            }
            (mine, NoteRecord::Removed) => *mine = NoteRecord::Removed,
            (NoteRecord::Removed, NoteRecord::Live { .. }) => {}
        }
    }
}

impl SectionRecord {
    fn join(&mut self, other: &SectionRecord) {
        match (self, other) {
            (
                SectionRecord::Live { name, created },
                SectionRecord::Live { name: their_name, created: their_created },
            ) => {
                name.join(their_name);
                *created = (*created).max(*their_created);
            }
            (mine, SectionRecord::Removed) => *mine = SectionRecord::Removed,
            (SectionRecord::Removed, SectionRecord::Live { .. }) => {}
        }
    }
}
