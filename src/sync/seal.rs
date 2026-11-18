use std::collections::BTreeMap;

use bronze_wire::VaultId;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};

use super::key::{CipherKey, random};
use crate::board::{NoteId, NoteRecord, Position, Rank, Reg, Replica, SectionId, SectionRecord, Stamp};

const FORMAT: u8 = 1;
const NONCE_BYTES: usize = 24;

pub struct Seal {
    cipher: XChaCha20Poly1305,
    associated: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unopenable {
    Newer,
    Damaged,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    notes: BTreeMap<NoteId, Option<LiveNote>>,
    sections: BTreeMap<SectionId, Option<LiveSection>>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LiveNote {
    text: String,
    done: bool,
    done_at: Stamp,
    section: Option<SectionId>,
    rank: Rank,
    placed_at: Stamp,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LiveSection {
    name: String,
    named_at: Stamp,
    created: Stamp,
}

impl Seal {
    pub fn new(vault: &VaultId, key: &CipherKey) -> Seal {
        Seal {
            cipher: XChaCha20Poly1305::new(&Key::from(*key.as_bytes())),
            associated: format!("bronze/snapshot/v1:{vault}").into_bytes(),
        }
    }

    pub fn seal(&self, replica: &Replica) -> Vec<u8> {
        self.seal_with(replica, random())
    }

    fn seal_with(&self, replica: &Replica, nonce: [u8; NONCE_BYTES]) -> Vec<u8> {
        let plain = serde_json::to_vec(&Snapshot::from(replica)).expect("a snapshot serializes");
        let sealed = self
            .cipher
            .encrypt(&XNonce::from(nonce), Payload { msg: &plain, aad: &self.associated })
            .expect("XChaCha20-Poly1305 seals any board");
        [&[FORMAT][..], &nonce, &sealed].concat()
    }

    pub fn open(&self, sealed: &[u8]) -> Result<Replica, Unopenable> {
        match sealed.first() {
            Some(&FORMAT) => {}
            Some(&format) if format > FORMAT => return Err(Unopenable::Newer),
            _ => return Err(Unopenable::Damaged),
        }
        let (nonce, ciphertext) = sealed[1..].split_first_chunk::<NONCE_BYTES>().ok_or(Unopenable::Damaged)?;
        let plain = self
            .cipher
            .decrypt(&XNonce::from(*nonce), Payload { msg: ciphertext, aad: &self.associated })
            .map_err(|_| Unopenable::Damaged)?;
        serde_json::from_slice::<Snapshot>(&plain).map(Replica::from).map_err(|_| Unopenable::Damaged)
    }
}

impl From<&Replica> for Snapshot {
    fn from(replica: &Replica) -> Snapshot {
        let notes = replica.notes.iter().map(|(&id, record)| {
            let live = match record {
                NoteRecord::Live { text, done, place } => Some(LiveNote {
                    text: text.clone(),
                    done: done.value,
                    done_at: done.at,
                    section: place.value.section,
                    rank: place.value.rank.clone(),
                    placed_at: place.at,
                }),
                NoteRecord::Removed => None,
            };
            (id, live)
        });
        let sections = replica.sections.iter().map(|(&id, record)| {
            let live = match record {
                SectionRecord::Live { name, created } => {
                    Some(LiveSection { name: name.value.clone(), named_at: name.at, created: *created })
                }
                SectionRecord::Removed => None,
            };
            (id, live)
        });
        Snapshot { notes: notes.collect(), sections: sections.collect() }
    }
}

impl From<Snapshot> for Replica {
    fn from(snapshot: Snapshot) -> Replica {
        let notes = snapshot.notes.into_iter().map(|(id, live)| {
            let record = match live {
                Some(note) => NoteRecord::Live {
                    text: note.text,
                    done: Reg { value: note.done, at: note.done_at },
                    place: Reg { value: Position { section: note.section, rank: note.rank }, at: note.placed_at },
                },
                None => NoteRecord::Removed,
            };
            (id, record)
        });
        let sections = snapshot.sections.into_iter().map(|(id, live)| {
            let record = match live {
                Some(section) => SectionRecord::Live {
                    name: Reg { value: section.name, at: section.named_at },
                    created: section.created,
                },
                None => SectionRecord::Removed,
            };
            (id, record)
        });
        Replica { notes: notes.collect(), sections: sections.collect() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{Board, DeviceId};
    use crate::sync::SyncKey;

    const KEY: &str = "bronze1-000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f-630dcd29";
    const FORMAT_1: &[u8] = include_bytes!("fixtures/sealed-v1.bin");

    fn seal_for(key: &str) -> Seal {
        let derived = SyncKey::parse(key).unwrap().derive();
        Seal::new(&derived.vault, &derived.cipher)
    }

    fn sample() -> Replica {
        let stamp = |time| Stamp { time, device: DeviceId::try_from("00000000000000aa".to_owned()).unwrap() };
        let id = |hex: &str| hex.repeat(32);
        let rank = |hex: &str| Rank::try_from(hex.repeat(16)).unwrap();
        let section = SectionId::try_from(id("5")).unwrap();
        let mut replica = Replica::default();
        replica.sections.insert(
            section,
            SectionRecord::Live { name: Reg { value: "Inbox".into(), at: stamp(20) }, created: stamp(10) },
        );
        replica.sections.insert(SectionId::try_from(id("6")).unwrap(), SectionRecord::Removed);
        replica.notes.insert(
            NoteId::try_from(id("1")).unwrap(),
            NoteRecord::Live {
                text: "Ask for a rollout plan".into(),
                done: Reg { value: true, at: stamp(30) },
                place: Reg { value: Position { section: None, rank: rank("8") }, at: stamp(11) },
            },
        );
        replica.notes.insert(
            NoteId::try_from(id("2")).unwrap(),
            NoteRecord::Live {
                text: "Compare the two cache designs\n\nwith **notes**".into(),
                done: Reg { value: false, at: stamp(12) },
                place: Reg { value: Position { section: Some(section), rank: rank("4") }, at: stamp(40) },
            },
        );
        replica.notes.insert(NoteId::try_from(id("3")).unwrap(), NoteRecord::Removed);
        replica
    }

    #[test]
    fn a_format_1_snapshot_keeps_opening_in_every_future_build() {
        assert_eq!(seal_for(KEY).open(FORMAT_1), Ok(sample()));
    }

    #[test]
    fn a_sealed_board_opens_to_the_same_replica_and_hides_its_text() {
        let mut board = Board::default();
        board.submit("# Later");
        let id = board.capture("Summarize the failing test output");
        board.toggle(&[id]);
        let gone = board.capture("Gone");
        board.remove(&[gone]);
        let seal = seal_for(KEY);
        let sealed = seal.seal(board.replica());
        assert_eq!(sealed[0], 1);
        assert_eq!(seal.open(&sealed).as_ref(), Ok(board.replica()));
        assert!(!sealed.windows(9).any(|w| w == b"Summarize"), "the text travels encrypted");
        assert_ne!(seal.seal(board.replica()), sealed, "each seal takes a fresh nonce");
    }

    #[test]
    fn a_newer_format_is_told_apart_from_a_damaged_copy() {
        let seal = seal_for(KEY);
        let mut newer = FORMAT_1.to_vec();
        newer[0] = 2;
        assert_eq!(seal.open(&newer), Err(Unopenable::Newer));
        let mut flipped = FORMAT_1.to_vec();
        *flipped.last_mut().unwrap() ^= 1;
        for damaged in [&[][..], &[0], &[1], &[1; 80], &FORMAT_1[..30], &flipped] {
            assert_eq!(seal.open(damaged), Err(Unopenable::Damaged), "{damaged:?}");
        }
    }

    #[test]
    fn another_vaults_key_cannot_open_the_snapshot() {
        let other = SyncKey::generate().reveal();
        assert_eq!(seal_for(&other).open(FORMAT_1), Err(Unopenable::Damaged));
    }

    #[test]
    fn an_authentic_snapshot_with_fields_this_build_does_not_know_is_damaged() {
        let seal = seal_for(KEY);
        let plain = br#"{"notes":{},"sections":{},"later":true}"#;
        let nonce = [7; NONCE_BYTES];
        let sealed = seal.cipher.encrypt(&XNonce::from(nonce), Payload { msg: plain, aad: &seal.associated }).unwrap();
        assert_eq!(seal.open(&[&[FORMAT][..], &nonce, &sealed].concat()), Err(Unopenable::Damaged));
    }
}
