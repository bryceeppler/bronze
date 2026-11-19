use std::fmt;

use bronze_wire::{AuthToken, VaultId};
use hkdf::Hkdf;
use sha2::{Digest, Sha256};

const PREFIX: &str = "bronze1-";
const SALT: &[u8] = b"bronze/v1";

#[derive(Clone, PartialEq, Eq)]
pub struct SyncKey([u8; 32]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyRefusal {
    Malformed,
    CheckFails,
}

pub struct Derived {
    pub vault: VaultId,
    pub token: AuthToken,
    pub cipher: CipherKey,
}

pub struct CipherKey([u8; 32]);

impl SyncKey {
    pub fn generate() -> SyncKey {
        SyncKey(random())
    }

    pub fn parse(text: &str) -> Result<SyncKey, KeyRefusal> {
        let text = text.trim().to_ascii_lowercase();
        let (body, check) =
            text.strip_prefix(PREFIX).and_then(|rest| rest.split_once('-')).ok_or(KeyRefusal::Malformed)?;
        let secret: [u8; 32] = unhex(body).ok_or(KeyRefusal::Malformed)?;
        let check: [u8; 4] = unhex(check).ok_or(KeyRefusal::Malformed)?;
        if check != checksum(&secret) {
            return Err(KeyRefusal::CheckFails);
        }
        Ok(SyncKey(secret))
    }

    pub fn reveal(&self) -> String {
        format!("{PREFIX}{}-{}", hex(&self.0), hex(&checksum(&self.0)))
    }

    pub fn derive(&self) -> Derived {
        let hkdf = Hkdf::<Sha256>::new(Some(SALT), &self.0);
        let expand = |info: &str| {
            let mut okm = [0; 32];
            hkdf.expand(info.as_bytes(), &mut okm).expect("HKDF-SHA256 expands to 32 bytes");
            okm
        };
        Derived {
            vault: VaultId::from_bytes(expand("vault")),
            token: AuthToken::from_bytes(expand("authentication")),
            cipher: CipherKey(expand("encryption")),
        }
    }
}

impl CipherKey {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for SyncKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SyncKey(..)")
    }
}

impl fmt::Debug for CipherKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CipherKey(..)")
    }
}

pub fn random<const N: usize>() -> [u8; N] {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).expect("the system random source");
    bytes
}

fn checksum(secret: &[u8; 32]) -> [u8; 4] {
    let digest = Sha256::digest(secret);
    [digest[0], digest[1], digest[2], digest[3]]
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex<const N: usize>(text: &str) -> Option<[u8; N]> {
    if text.len() != N * 2 || !text.bytes().all(|digit| digit.is_ascii_hexdigit()) {
        return None;
    }
    let mut bytes = [0; N];
    for (at, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[at * 2..at * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: [u8; 32] = [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29,
        30, 31,
    ];
    const TEXT: &str = "bronze1-000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f-630dcd29";

    #[test]
    fn a_key_prints_and_parses_as_its_pinned_text() {
        let key = SyncKey(SECRET);
        assert_eq!(key.reveal(), TEXT);
        assert_eq!(SyncKey::parse(TEXT), Ok(key.clone()));
        assert_eq!(SyncKey::parse(&format!("  {}\n", TEXT.to_uppercase())), Ok(key));
    }

    #[test]
    fn a_key_derives_its_pinned_vault_token_and_cipher_key() {
        let derived = SyncKey(SECRET).derive();
        assert_eq!(derived.vault.to_string(), "1c16bf32158822719b77555734507767941b1f37167bf3e12835e499d3e9ffd6");
        assert_eq!(
            derived.token.authorization(),
            "Bearer addff0b3ba6f1643c9527b56bdbd5ff6e17f0aed80bf7fde286fb8d0f8e9207f"
        );
        assert_eq!(hex(derived.cipher.as_bytes()), "f4e99d73ce1a0c27c32479e1dd0c8ae3d94b3814687c46e4573e36cd084ebaef");
    }

    #[test]
    fn a_wrong_key_is_refused_for_its_shape_or_its_check() {
        let body = &TEXT[PREFIX.len()..PREFIX.len() + 64];
        for malformed in [
            "",
            "bronze1-",
            &TEXT[..TEXT.len() - 1],
            &format!("{TEXT}0"),
            &TEXT.replacen("bronze1-", "bronze2-", 1),
            &TEXT.replacen('-', "_", 2),
            &format!("bronze1-{}-630dcd29", &body[..62]),
            &TEXT.replacen('a', "g", 1),
            &TEXT.replacen("-630dcd29", "630dcd29", 1),
        ] {
            assert_eq!(SyncKey::parse(malformed), Err(KeyRefusal::Malformed), "{malformed:?}");
        }
        assert_eq!(SyncKey::parse(&TEXT.replacen("630dcd29", "630dcd2a", 1)), Err(KeyRefusal::CheckFails));
        assert_eq!(SyncKey::parse(&TEXT.replacen("00010203", "00010204", 1)), Err(KeyRefusal::CheckFails));
    }

    #[test]
    fn debug_output_hides_every_secret() {
        let key = SyncKey(SECRET);
        let derived = key.derive();
        let printed = format!("{key:?} {:?} {:?}", derived.token, derived.cipher);
        assert_eq!(printed, "SyncKey(..) AuthToken(..) CipherKey(..)");
    }

    #[test]
    fn generated_keys_differ_and_round_trip() {
        let (one, two) = (SyncKey::generate(), SyncKey::generate());
        assert_ne!(one, two);
        assert_eq!(SyncKey::parse(&one.reveal()), Ok(one));
    }
}
