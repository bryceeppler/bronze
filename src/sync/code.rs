use super::address::{AddressRefusal, ServerAddress};
use super::key::{KeyRefusal, SyncKey};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncCode {
    pub server: ServerAddress,
    pub key: SyncKey,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Setup {
    Start(ServerAddress),
    Join(SyncCode),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupRefusal {
    NotAddress,
    NotSecure,
    KeyMalformed,
    KeyCheckFails,
}

impl SyncCode {
    pub fn reveal(&self) -> String {
        format!("{}#{}", self.server, self.key.reveal())
    }
}

impl Setup {
    pub fn parse(text: &str) -> Result<Setup, SetupRefusal> {
        Ok(match text.split_once('#') {
            None => Setup::Start(ServerAddress::parse(text)?),
            Some((server, key)) => {
                Setup::Join(SyncCode { server: ServerAddress::parse(server)?, key: SyncKey::parse(key)? })
            }
        })
    }

    pub fn code(self) -> SyncCode {
        match self {
            Setup::Start(server) => SyncCode { server, key: SyncKey::generate() },
            Setup::Join(code) => code,
        }
    }
}

impl From<AddressRefusal> for SetupRefusal {
    fn from(refusal: AddressRefusal) -> SetupRefusal {
        match refusal {
            AddressRefusal::NotAddress => SetupRefusal::NotAddress,
            AddressRefusal::NotSecure => SetupRefusal::NotSecure,
        }
    }
}

impl From<KeyRefusal> for SetupRefusal {
    fn from(refusal: KeyRefusal) -> SetupRefusal {
        match refusal {
            KeyRefusal::Malformed => SetupRefusal::KeyMalformed,
            KeyRefusal::CheckFails => SetupRefusal::KeyCheckFails,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "bronze1-000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f-630dcd29";

    #[test]
    fn a_server_address_alone_starts_a_new_vault() {
        let setup = Setup::parse(" https://sync.example.com/ ").unwrap();
        assert_eq!(setup, Setup::Start(ServerAddress::parse("https://sync.example.com").unwrap()));
        let (one, two) = (setup.clone().code(), setup.code());
        assert_eq!(one.server, two.server);
        assert_ne!(one.key, two.key, "each start makes a new key");
    }

    #[test]
    fn a_sync_code_joins_the_vault_it_names_and_prints_back_the_same() {
        let text = format!("http://localhost:8787#{KEY}");
        let Setup::Join(code) = Setup::parse(&format!("  {text}\n")).unwrap() else { panic!("not a join") };
        assert_eq!(code.server.to_string(), "http://localhost:8787");
        assert_eq!(code.key.reveal(), KEY);
        assert_eq!(code.reveal(), text);
        assert_eq!(Setup::parse(&code.reveal()), Ok(Setup::Join(code.clone())));
        assert_eq!(Setup::Join(code.clone()).code(), code);
    }

    #[test]
    fn each_way_an_entry_can_be_wrong_has_its_own_refusal() {
        for (text, refusal) in [
            ("", SetupRefusal::NotAddress),
            ("sync.example.com", SetupRefusal::NotAddress),
            (KEY, SetupRefusal::NotAddress),
            (&format!("#{KEY}"), SetupRefusal::NotAddress),
            ("http://sync.example.com", SetupRefusal::NotSecure),
            (&format!("http://sync.example.com#{KEY}"), SetupRefusal::NotSecure),
            ("https://sync.example.com#", SetupRefusal::KeyMalformed),
            ("https://sync.example.com#bronze1-1234", SetupRefusal::KeyMalformed),
            (&format!("https://sync.example.com#{KEY}#"), SetupRefusal::KeyMalformed),
            (&format!("https://sync.example.com#{}", KEY.replace("630dcd29", "630dcd28")), SetupRefusal::KeyCheckFails),
        ] {
            assert_eq!(Setup::parse(text), Err(refusal), "{text:?}");
        }
    }

    #[test]
    fn debug_output_shows_the_server_but_not_the_key() {
        let Ok(Setup::Join(code)) = Setup::parse(&format!("https://sync.example.com#{KEY}")) else { panic!() };
        let printed = format!("{code:?}");
        assert!(printed.contains("sync.example.com") && !printed.contains("0001020304"), "{printed}");
    }
}
