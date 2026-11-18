use std::fmt;

use ureq::http::Uri;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerAddress(String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressRefusal {
    NotAddress,
    NotSecure,
}

impl ServerAddress {
    pub fn parse(text: &str) -> Result<ServerAddress, AddressRefusal> {
        let text = text.trim();
        if text.contains('#') {
            return Err(AddressRefusal::NotAddress);
        }
        let uri: Uri = text.parse().map_err(|_| AddressRefusal::NotAddress)?;
        let authority = uri.authority().ok_or(AddressRefusal::NotAddress)?;
        let host = authority.host().to_ascii_lowercase();
        if host.is_empty() || authority.as_str().contains('@') || uri.query().is_some() {
            return Err(AddressRefusal::NotAddress);
        }
        let scheme = uri.scheme_str().ok_or(AddressRefusal::NotAddress)?.to_ascii_lowercase();
        match scheme.as_str() {
            "https" => {}
            "http" if matches!(host.as_str(), "localhost" | "127.0.0.1") => {}
            "http" => return Err(AddressRefusal::NotSecure),
            _ => return Err(AddressRefusal::NotAddress),
        }
        let port = match &authority.as_str()[authority.host().len()..] {
            "" => String::new(),
            typed => typed
                .strip_prefix(':')
                .filter(|digits| digits.bytes().all(|digit| digit.is_ascii_digit()))
                .and_then(|digits| digits.parse::<u16>().ok())
                .filter(|port| *port > 0)
                .map(|port| format!(":{port}"))
                .ok_or(AddressRefusal::NotAddress)?,
        };
        Ok(ServerAddress(format!("{scheme}://{host}{port}{}", uri.path().trim_end_matches('/'))))
    }

    pub fn route(&self, path: &str) -> String {
        format!("{}{path}", self.0)
    }
}

impl fmt::Display for ServerAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::Setup;
    use crate::sync::code::SetupRefusal;

    fn parsed(text: &str) -> Result<String, AddressRefusal> {
        ServerAddress::parse(text).map(|address| address.to_string())
    }

    #[test]
    fn secure_and_local_addresses_read_in_one_canonical_form() {
        for (text, address) in [
            ("https://sync.example.com", "https://sync.example.com"),
            ("  HTTPS://Sync.Example.COM/  ", "https://sync.example.com"),
            ("https://sync.example.com:8443/bronze/", "https://sync.example.com:8443/bronze"),
            ("https://[2001:db8::1]:8787", "https://[2001:db8::1]:8787"),
            ("http://localhost:8787", "http://localhost:8787"),
            ("http://127.0.0.1:8787/", "http://127.0.0.1:8787"),
            ("http://LOCALHOST", "http://localhost"),
        ] {
            assert_eq!(parsed(text).as_deref(), Ok(address), "{text:?}");
        }
    }

    #[test]
    fn plain_http_is_refused_beyond_this_machine() {
        for text in ["http://sync.example.com", "http://192.168.1.4:8787", "http://[::1]:8787", "http://127.0.0.2"] {
            assert_eq!(parsed(text), Err(AddressRefusal::NotSecure), "{text:?}");
        }
    }

    #[test]
    fn anything_but_a_bare_server_is_not_an_address() {
        for text in [
            "",
            "   ",
            "https://",
            "ftp://sync.example.com",
            "ws://localhost:8787",
            "https://me@sync.example.com",
            "https://me:secret@sync.example.com",
            "https://sync.example.com/?vault=1",
            "https://sync.example.com/?",
            "https://sync.example.com/#key",
            "https://sync example.com",
            "sync.example.com",
            "localhost:8787",
            "bronze1-000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f-630dcd29",
        ] {
            assert_eq!(parsed(text), Err(AddressRefusal::NotAddress), "{text:?}");
        }
    }

    #[test]
    fn a_port_that_is_not_1_to_65535_is_refused_never_dropped() {
        const KEY: &str = "bronze1-000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f-630dcd29";
        for text in [
            "https://sync.example.com:65536",
            "https://sync.example.com:abc",
            "https://sync.example.com:",
            "https://sync.example.com:0",
            "https://sync.example.com:-1",
            "https://sync.example.com:443x/bronze",
            "https://sync.example.com:+443",
            "https://[2001:db8::1]:99999",
            "https://[2001:db8::1]:",
            "http://localhost:65536",
        ] {
            assert_eq!(parsed(text), Err(AddressRefusal::NotAddress), "{text:?}");
            let code = format!("{text}#{KEY}");
            assert_eq!(Setup::parse(&code), Err(SetupRefusal::NotAddress), "{code:?}");
        }
        for (text, address) in [
            ("https://sync.example.com:1", "https://sync.example.com:1"),
            ("https://sync.example.com:65535/bronze", "https://sync.example.com:65535/bronze"),
            ("https://[2001:db8::1]:443", "https://[2001:db8::1]:443"),
        ] {
            assert_eq!(parsed(text).as_deref(), Ok(address), "{text:?}");
        }
    }

    #[test]
    fn routes_follow_any_path_the_server_lives_under() {
        let address = ServerAddress::parse("https://example.com/bronze/").unwrap();
        assert_eq!(address.route("/health"), "https://example.com/bronze/health");
        let address = ServerAddress::parse("http://localhost:8787").unwrap();
        assert_eq!(address.route("/v1/vaults/ab"), "http://localhost:8787/v1/vaults/ab");
    }
}
