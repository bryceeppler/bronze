use std::fmt;
use std::str::FromStr;
use std::time::Duration;

pub const HEALTH_ROUTE: &str = "/health";
pub const VAULT_ROUTE: &str = "/v1/vaults/{vault}";

pub fn vault_path(vault: &VaultId) -> String {
    format!("/v1/vaults/{vault}")
}

pub mod header {
    pub const PREFER: &str = "prefer";
    pub const SETUP_TOKEN: &str = "bronze-setup-token";
}

pub const MAX_WAIT: Duration = Duration::from_secs(55);
pub const DEFAULT_LISTEN: &str = "127.0.0.1:8787";
pub const DEFAULT_MAX_BYTES: u64 = 8 * 1024 * 1024;
pub const DEFAULT_MAX_VAULTS: u64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Malformed(pub &'static str);

impl fmt::Display for Malformed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "malformed {}", self.0)
    }
}

impl std::error::Error for Malformed {}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VaultId([u8; 32]);

impl VaultId {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl FromStr for VaultId {
    type Err = Malformed;

    fn from_str(text: &str) -> Result<Self, Malformed> {
        parse_hex(text).map(Self).ok_or(Malformed("vault id"))
    }
}

impl fmt::Display for VaultId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(f, &self.0)
    }
}

impl fmt::Debug for VaultId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VaultId({self})")
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct AuthToken([u8; 32]);

impl AuthToken {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn from_authorization(value: &str) -> Result<Self, Malformed> {
        let malformed = Malformed("bearer token");
        let (scheme, token) = value.split_once(' ').ok_or(malformed)?;
        if !scheme.eq_ignore_ascii_case("bearer") {
            return Err(malformed);
        }
        token.trim_start_matches(' ').parse().map_err(|_| malformed)
    }

    pub fn authorization(&self) -> String {
        let mut value = String::from("Bearer ");
        value.extend(self.0.iter().map(|byte| format!("{byte:02x}")));
        value
    }
}

impl FromStr for AuthToken {
    type Err = Malformed;

    fn from_str(text: &str) -> Result<Self, Malformed> {
        parse_hex(text).map(Self).ok_or(Malformed("token"))
    }
}

impl fmt::Debug for AuthToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AuthToken(..)")
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Version([u8; 16]);

impl Version {
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    pub fn from_etag(value: &str) -> Result<Self, Malformed> {
        value
            .trim()
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
            .and_then(|hex| hex.parse().ok())
            .ok_or(Malformed("entity tag"))
    }

    pub fn etag(&self) -> String {
        format!("\"{self}\"")
    }
}

impl FromStr for Version {
    type Err = Malformed;

    fn from_str(text: &str) -> Result<Self, Malformed> {
        parse_hex(text).map(Self).ok_or(Malformed("version"))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(f, &self.0)
    }
}

impl fmt::Debug for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Version({self})")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precondition {
    Create,
    Replace(Version),
}

impl Precondition {
    pub fn from_headers(if_match: Option<&str>, if_none_match: Option<&str>) -> Result<Option<Self>, Malformed> {
        match (if_match, if_none_match) {
            (None, None) => Ok(None),
            (Some(tag), None) => Version::from_etag(tag).map(|version| Some(Self::Replace(version))),
            (None, Some(any)) if any.trim() == "*" => Ok(Some(Self::Create)),
            _ => Err(Malformed("precondition")),
        }
    }

    pub fn header(&self) -> (&'static str, String) {
        match self {
            Self::Create => ("if-none-match", "*".to_owned()),
            Self::Replace(version) => ("if-match", version.etag()),
        }
    }
}

pub fn wait_preference(prefer: &str) -> Option<Duration> {
    prefer.split(',').find_map(|preference| {
        let (name, value) = preference.split(';').next()?.split_once('=')?;
        if !name.trim().eq_ignore_ascii_case("wait") {
            return None;
        }
        let seconds: u64 = value.trim().trim_matches('"').parse().ok()?;
        Some(Duration::from_secs(seconds).min(MAX_WAIT))
    })
}

pub fn prefer_wait(wait: Duration) -> String {
    format!("wait={}", wait.min(MAX_WAIT).as_secs())
}

fn parse_hex<const N: usize>(text: &str) -> Option<[u8; N]> {
    if text.len() != N * 2 {
        return None;
    }
    let mut bytes = [0; N];
    for (byte, [high, low]) in bytes.iter_mut().zip(text.as_bytes().as_chunks::<2>().0) {
        *byte = (nibble(*high)? << 4) | nibble(*low)?;
    }
    Some(bytes)
}

fn nibble(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        _ => None,
    }
}

fn write_hex(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    bytes.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEX: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    #[test]
    fn ids_are_exactly_64_lowercase_hex() {
        assert_eq!(HEX.parse::<VaultId>().unwrap().to_string(), HEX);
        assert_eq!(HEX.parse::<AuthToken>().unwrap().as_bytes()[1], 0x11);
        for bad in [&HEX[..63], &format!("{HEX}0"), &HEX.to_uppercase(), &HEX.replacen('0', "g", 1), ""] {
            assert_eq!(bad.parse::<VaultId>(), Err(Malformed("vault id")), "{bad:?}");
            assert!(bad.parse::<AuthToken>().is_err(), "{bad:?}");
        }
    }

    #[test]
    fn bearer_round_trips_and_rejects_other_schemes() {
        let token: AuthToken = HEX.parse().unwrap();
        assert_eq!(token.authorization(), format!("Bearer {HEX}"));
        assert_eq!(AuthToken::from_authorization(&token.authorization()), Ok(token.clone()));
        assert_eq!(AuthToken::from_authorization(&format!("bearer  {HEX}")), Ok(token));
        for bad in [format!("Basic {HEX}"), HEX.to_owned(), format!("Bearer {}", &HEX[1..])] {
            assert!(AuthToken::from_authorization(&bad).is_err(), "{bad:?}");
        }
        assert_eq!(format!("{:?}", HEX.parse::<AuthToken>().unwrap()), "AuthToken(..)");
    }

    #[test]
    fn versions_travel_as_strong_entity_tags() {
        let version = Version::from_bytes([0xab; 16]);
        assert_eq!(version.etag(), format!("\"{}\"", "ab".repeat(16)));
        assert_eq!(Version::from_etag(&version.etag()), Ok(version));
        for bad in ["ab".repeat(16), format!("W/{}", version.etag()), "\"*\"".to_owned()] {
            assert!(Version::from_etag(&bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn preconditions_parse_from_one_header() {
        let version = Version::from_bytes([7; 16]);
        assert_eq!(Precondition::from_headers(None, None), Ok(None));
        assert_eq!(Precondition::from_headers(None, Some("*")), Ok(Some(Precondition::Create)));
        assert_eq!(Precondition::from_headers(Some(&version.etag()), None), Ok(Some(Precondition::Replace(version))));
        assert!(Precondition::from_headers(Some(&version.etag()), Some("*")).is_err());
        assert!(Precondition::from_headers(None, Some(&version.etag())).is_err());
        assert!(Precondition::from_headers(Some("*"), None).is_err());
        for precondition in [Precondition::Create, Precondition::Replace(version)] {
            let (name, value) = precondition.header();
            let parsed = match name {
                "if-match" => Precondition::from_headers(Some(&value), None),
                _ => Precondition::from_headers(None, Some(&value)),
            };
            assert_eq!(parsed, Ok(Some(precondition)));
        }
    }

    #[test]
    fn wait_is_read_from_prefer_and_capped() {
        assert_eq!(wait_preference("wait=10"), Some(Duration::from_secs(10)));
        assert_eq!(wait_preference("respond-async, Wait = 3"), Some(Duration::from_secs(3)));
        assert_eq!(wait_preference("wait=600"), Some(MAX_WAIT));
        assert_eq!(wait_preference(&prefer_wait(Duration::from_secs(600))), Some(MAX_WAIT));
        assert_eq!(wait_preference("wait=soon"), None);
        assert_eq!(wait_preference("return=minimal"), None);
    }
}
