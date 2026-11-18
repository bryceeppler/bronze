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
        let scheme = uri.scheme_str().unwrap_or_default().to_ascii_lowercase();
        match scheme.as_str() {
            "https" => {}
            "http" if matches!(host.as_str(), "localhost" | "127.0.0.1") => {}
            "http" => return Err(AddressRefusal::NotSecure),
            _ => return Err(AddressRefusal::NotAddress),
        }
        let port = authority.port_u16().map(|port| format!(":{port}")).unwrap_or_default();
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
    fn routes_follow_any_path_the_server_lives_under() {
        let address = ServerAddress::parse("https://example.com/bronze/").unwrap();
        assert_eq!(address.route("/health"), "https://example.com/bronze/health");
        let address = ServerAddress::parse("http://localhost:8787").unwrap();
        assert_eq!(address.route("/v1/vaults/ab"), "http://localhost:8787/v1/vaults/ab");
    }
}
