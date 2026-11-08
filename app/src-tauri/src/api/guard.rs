//! The local API's three locks, as plain functions.

use sha2::{Digest, Sha256};

const EXTENSION_SCHEME: &str = "chrome-extension://";
const FIREFOX_SCHEME: &str = "moz-extension://";

/// The `Host` header must name exactly `127.0.0.1:<port>`, which blocks DNS rebinding.
pub fn check_host(host: Option<&str>, port: u16) -> bool {
    host == Some(format!("127.0.0.1:{port}").as_str())
}

/// The request's origin, when it is a browser extension's: `chrome-extension://<id>` (Chrome, Edge)
/// or `moz-extension://<uuid>` (Firefox; a lowercase 8-4-4-4-12 uuid, random per profile and install).
pub fn check_origin(origin: Option<&str>) -> Option<&str> {
    let o = origin?;
    if let Some(uuid) = o.strip_prefix(FIREFOX_SCHEME) {
        return is_lower_uuid(uuid).then_some(o);
    }
    let id = o.strip_prefix(EXTENSION_SCHEME)?;
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric())).then_some(o)
}

fn is_lower_uuid(s: &str) -> bool {
    s.len() == 36
        && s.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_digit() || ('a'..='f').contains(&c),
        })
}

/// The token in `Authorization: Bearer <token>`.
pub fn bearer(auth: Option<&str>) -> Option<&str> {
    auth?.strip_prefix("Bearer ").map(str::trim).filter(|t| !t.is_empty())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// What is stored for a token: its SHA-256, as lowercase hex.
pub fn hash_token(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

/// 32 random bytes as 64 lowercase hex characters.
pub fn new_token() -> String {
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).expect("the OS random number generator");
    hex(&b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_must_match_exactly() {
        assert!(check_host(Some("127.0.0.1:17654"), 17654));
        for bad in ["localhost:17654", "127.0.0.1:17655", "evil.test:17654", "127.0.0.1", "127.0.0.1:17654.evil.test"] {
            assert!(!check_host(Some(bad), 17654), "{bad}");
        }
        assert!(!check_host(None, 17654));
    }

    #[test]
    fn origin_must_be_extension() {
        assert_eq!(check_origin(Some("chrome-extension://abcdef")), Some("chrome-extension://abcdef"));
        for bad in ["https://a.org", "null", "chrome-extension://", "chrome-extension:///x", ""] {
            assert_eq!(check_origin(Some(bad)), None, "{bad}");
        }
        assert_eq!(check_origin(None), None);
    }

    #[test]
    fn origin_accepts_firefox_uuid() {
        let uuid = "6c1d9f2e-1b7a-4e3c-9a55-0d2f7e8b4c31";
        let ok = format!("moz-extension://{uuid}");
        assert_eq!(check_origin(Some(&ok)), Some(ok.as_str()));
        let bad = [
            format!("moz-extension://{}", uuid.to_uppercase()),
            format!("moz-extension://{}", &uuid[1..]),
            format!("moz-extension://{uuid}0"),
            format!("moz-extension://{}", uuid.replace('-', "")),
            "moz-extension://6c1d9f2e1-b7a-4e3c-9a55-0d2f7e8b4c31".to_string(),
            format!("moz-extension://{uuid}/"),
            format!("moz-extension://{uuid}/popup.html"),
            "moz-extension://".to_string(),
            format!("moz-extension://{}", uuid.replacen('6', "g", 1)),
            format!("chrome-extension://{uuid}"),
        ];
        for b in &bad {
            assert_eq!(check_origin(Some(b)), None, "{b}");
        }
    }

    #[test]
    fn bearer_parses() {
        assert_eq!(bearer(Some("Bearer abc123")), Some("abc123"));
        assert_eq!(bearer(Some("Bearer ")), None);
        assert_eq!(bearer(Some("Basic abc")), None);
        assert_eq!(bearer(None), None);
    }

    #[test]
    fn hash_is_sha256_hex() {
        assert_eq!(hash_token("abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn new_token_is_64_hex_and_unique() {
        let a = new_token();
        let b = new_token();
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_ne!(a, b);
    }
}
