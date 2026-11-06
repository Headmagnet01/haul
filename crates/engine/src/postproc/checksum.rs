//! Checksums: checksum files in a package, hashes the user pastes, server digests, and hashing a file.

use base64::Engine;
use reqwest::header::HeaderMap;
use std::io::Read;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algo {
    Crc32,
    Md5,
    Sha1,
    Sha256,
}

impl Algo {
    pub const ALL: [Algo; 4] = [Algo::Crc32, Algo::Md5, Algo::Sha1, Algo::Sha256];

    /// What the drawer shows.
    pub fn label(self) -> &'static str {
        match self {
            Algo::Crc32 => "CRC32",
            Algo::Md5 => "MD5",
            Algo::Sha1 => "SHA-1",
            Algo::Sha256 => "SHA-256",
        }
    }

    /// The stored tag, as in `"sha256:<hex>"`.
    pub fn key(self) -> &'static str {
        match self {
            Algo::Crc32 => "crc32",
            Algo::Md5 => "md5",
            Algo::Sha1 => "sha1",
            Algo::Sha256 => "sha256",
        }
    }

    fn hex_len(self) -> usize {
        match self {
            Algo::Crc32 => 8,
            Algo::Md5 => 32,
            Algo::Sha1 => 40,
            Algo::Sha256 => 64,
        }
    }

    fn from_hex_len(n: usize) -> Option<Algo> {
        Algo::ALL.into_iter().find(|a| a.hex_len() == n)
    }

    fn from_key(k: &str) -> Option<Algo> {
        Algo::ALL.into_iter().find(|a| a.key().eq_ignore_ascii_case(k))
    }
}

fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `"<key>:<hex>"`.
pub fn tagged(algo: Algo, hex: &str) -> String {
    format!("{}:{}", algo.key(), hex.to_ascii_lowercase())
}

/// Reads a stored `"<key>:<hex>"`.
pub fn parse_tagged(s: &str) -> Option<(Algo, String)> {
    let (k, h) = s.split_once(':')?;
    let algo = Algo::from_key(k)?;
    (is_hex(h) && h.len() == algo.hex_len()).then(|| (algo, h.to_ascii_lowercase()))
}

/// The algorithm a checksum file's name implies (`None` for a generic list).
fn algo_for_file(file_name: &str) -> Option<Algo> {
    let lower = file_name.to_ascii_lowercase();
    let base = lower.strip_suffix(".txt").unwrap_or(&lower);
    match base {
        "md5sums" => return Some(Algo::Md5),
        "sha1sums" => return Some(Algo::Sha1),
        "sha256sums" => return Some(Algo::Sha256),
        _ => {}
    }
    match lower.rsplit_once('.').map(|(_, e)| e) {
        Some("sfv") => Some(Algo::Crc32),
        Some("md5") => Some(Algo::Md5),
        Some("sha1") => Some(Algo::Sha1),
        Some("sha256") => Some(Algo::Sha256),
        _ => None,
    }
}

pub fn is_checksum_file(name: &str) -> bool {
    algo_for_file(name).is_some()
}

/// `SHA256 (name) = hex`.
fn parse_bsd(line: &str) -> Option<(String, Algo, String)> {
    let (tag, rest) = line.split_once(" (")?;
    let (name, h) = rest.rsplit_once(") = ")?;
    let algo = match tag.trim().to_ascii_uppercase().as_str() {
        "MD5" => Algo::Md5,
        "SHA1" => Algo::Sha1,
        "SHA256" => Algo::Sha256,
        "CRC32" => Algo::Crc32,
        _ => return None,
    };
    let h = h.trim();
    (is_hex(h) && h.len() == algo.hex_len() && !name.is_empty())
        .then(|| (name.to_string(), algo, h.to_ascii_lowercase()))
}

/// `<hex> <name>` or `<hex> *<name>` or `<hex>  <name>`.
fn parse_gnu(line: &str, algo: Option<Algo>) -> Option<(String, Algo, String)> {
    let (h, rest) = line.split_once(' ')?;
    let algo = algo.or_else(|| Algo::from_hex_len(h.len()))?;
    if !is_hex(h) || h.len() != algo.hex_len() {
        return None;
    }
    let name = rest.strip_prefix(['*', ' ']).unwrap_or(rest);
    (!name.trim().is_empty()).then(|| (name.to_string(), algo, h.to_ascii_lowercase()))
}

/// `<name> <crc32>`.
fn parse_sfv(line: &str) -> Option<(String, Algo, String)> {
    let (name, h) = line.rsplit_once(' ')?;
    let name = name.trim_end();
    (is_hex(h) && h.len() == 8 && !name.is_empty()).then(|| (name.to_string(), Algo::Crc32, h.to_ascii_lowercase()))
}

/// Every (listed file name, algorithm, lowercase hex) a checksum file holds. Lines it can't read are skipped.
pub fn parse_checksum_file(file_name: &str, text: &str) -> Vec<(String, Algo, String)> {
    let algo = algo_for_file(file_name);
    text.lines()
        .map(|l| l.trim_end_matches(['\r', '\n']).trim_start())
        .filter(|l| !l.is_empty() && !l.starts_with(';') && !l.starts_with('#'))
        .filter_map(|l| {
            if algo == Some(Algo::Crc32) {
                return parse_sfv(l);
            }
            parse_bsd(l).filter(|e| algo.is_none_or(|a| a == e.1)).or_else(|| parse_gnu(l, algo))
        })
        .collect()
}

const NOT_A_CHECKSUM: &str = "Not a checksum: use 8, 32, 40 or 64 hex characters";

/// A hash the user pasted, with an optional `md5:`, `sha1:`, `sha256:` or `crc32:` prefix.
pub fn parse_pasted(s: &str) -> Result<(Algo, String), String> {
    let s = s.trim();
    let (prefix, h) = match s.split_once(':') {
        Some((p, h)) => (Some(Algo::from_key(p.trim()).ok_or(NOT_A_CHECKSUM)?), h.trim()),
        None => (None, s),
    };
    let algo = Algo::from_hex_len(h.len()).filter(|_| is_hex(h)).ok_or(NOT_A_CHECKSUM)?;
    if prefix.is_some_and(|p| p != algo) {
        return Err(NOT_A_CHECKSUM.into());
    }
    Ok((algo, h.to_ascii_lowercase()))
}

fn from_b64(algo: Algo, b64: &str) -> Option<String> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok()?;
    (bytes.len() * 2 == algo.hex_len()).then(|| hex(&bytes))
}

/// `(algorithm name, base64)` pairs of a `Digest` or `Repr-Digest` header.
fn digest_entries(v: &str, structured: bool) -> Vec<(String, String)> {
    v.split(',')
        .filter_map(|e| {
            let (k, val) = e.split_once('=')?;
            let val = if structured { val.trim().strip_prefix(':')?.strip_suffix(':')? } else { val.trim() };
            Some((k.trim().to_ascii_lowercase(), val.to_string()))
        })
        .collect()
}

/// The server's digest of the whole file as `"<key>:<hex>"`. `Content-MD5` counts only when the
/// reply carried the whole body (`full_body`); on a 206 it describes just the range.
pub fn digest_from_headers(h: &HeaderMap, full_body: bool) -> Option<String> {
    let mut entries = Vec::new();
    for (name, structured) in [("repr-digest", true), ("digest", false)] {
        for v in h.get_all(name).iter().filter_map(|v| v.to_str().ok()) {
            entries.extend(digest_entries(v, structured));
        }
    }
    for (want, algo) in [("sha-256", Algo::Sha256), ("md5", Algo::Md5)] {
        if let Some(hex) = entries.iter().filter(|(k, _)| k == want).find_map(|(_, v)| from_b64(algo, v)) {
            return Some(tagged(algo, &hex));
        }
    }
    if full_body {
        let v = h.get("content-md5")?.to_str().ok()?;
        return from_b64(Algo::Md5, v).map(|hex| tagged(Algo::Md5, &hex));
    }
    None
}

enum Hasher {
    Crc32(crc32fast::Hasher),
    Md5(md5::Md5),
    Sha1(sha1::Sha1),
    Sha256(sha2::Sha256),
}

/// Hashes a file in 1 MiB chunks. Blocking: call it from `spawn_blocking`.
pub fn hash_file(path: &Path, algo: Algo) -> std::io::Result<String> {
    use sha2::Digest;
    let mut h = match algo {
        Algo::Crc32 => Hasher::Crc32(crc32fast::Hasher::new()),
        Algo::Md5 => Hasher::Md5(md5::Md5::new()),
        Algo::Sha1 => Hasher::Sha1(sha1::Sha1::new()),
        Algo::Sha256 => Hasher::Sha256(sha2::Sha256::new()),
    };
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        let chunk = &buf[..n];
        match &mut h {
            Hasher::Crc32(x) => x.update(chunk),
            Hasher::Md5(x) => x.update(chunk),
            Hasher::Sha1(x) => x.update(chunk),
            Hasher::Sha256(x) => x.update(chunk),
        }
    }
    Ok(match h {
        Hasher::Crc32(x) => format!("{:08x}", x.finalize()),
        Hasher::Md5(x) => hex(&x.finalize()),
        Hasher::Sha1(x) => hex(&x.finalize()),
        Hasher::Sha256(x) => hex(&x.finalize()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue};

    const SHA256_HH: &str = "d900ad473ce7307451f19c17c34dd17abbc8035842642932863b35104a411e9c";
    const MD5_HH: &str = "0dd8c03ef74686e11693dcffbb7a9417";
    const SHA1_HH: &str = "43654d1daf1f023a2260fb56512d12f000a5f57a";
    const CRC_HH: &str = "a0603c3f";

    fn entry(name: &str, algo: Algo, hex: &str) -> (String, Algo, String) {
        (name.into(), algo, hex.into())
    }

    #[test]
    fn parses_sfv() {
        let text = "; made by a tool\r\nmovie.part1.rar A0603C3F\r\nfile with spaces.bin 0000ffff\r\n";
        assert_eq!(
            parse_checksum_file("movie.sfv", text),
            vec![entry("movie.part1.rar", Algo::Crc32, CRC_HH), entry("file with spaces.bin", Algo::Crc32, "0000ffff")]
        );
    }

    #[test]
    fn parses_gnu_and_bsd() {
        let text = format!("{SHA256_HH} *ubuntu.iso\n{SHA256_HH}  two  spaces.bin\nSHA256 (b.bin) = {SHA256_HH}\n");
        assert_eq!(
            parse_checksum_file("SHA256SUMS", &text),
            vec![
                entry("ubuntu.iso", Algo::Sha256, SHA256_HH),
                entry("two  spaces.bin", Algo::Sha256, SHA256_HH),
                entry("b.bin", Algo::Sha256, SHA256_HH),
            ]
        );
        assert_eq!(parse_checksum_file("x.md5", &format!("{MD5_HH} a.bin")), vec![entry("a.bin", Algo::Md5, MD5_HH)]);
        assert_eq!(
            parse_checksum_file("x.sha1", &format!("SHA1 (a.bin) = {SHA1_HH}")),
            vec![entry("a.bin", Algo::Sha1, SHA1_HH)]
        );
    }

    #[test]
    fn algo_from_length_in_generic_file() {
        let text = format!("{MD5_HH}  a.bin\n{SHA1_HH}  b.bin\n");
        assert_eq!(
            parse_checksum_file("checksums.txt", &text),
            vec![entry("a.bin", Algo::Md5, MD5_HH), entry("b.bin", Algo::Sha1, SHA1_HH)]
        );
    }

    #[test]
    fn ignores_garbage_lines() {
        let text = format!("hello world\n\n# comment\n{MD5_HH}\nzz{} a.bin\n{SHA256_HH} *ok.bin\n", &MD5_HH[2..]);
        assert_eq!(parse_checksum_file("SHA256SUMS", &text), vec![entry("ok.bin", Algo::Sha256, SHA256_HH)]);
        // A SHA256SUMS line whose hash has the wrong length is ignored.
        assert!(parse_checksum_file("SHA256SUMS", &format!("{MD5_HH}  a.bin")).is_empty());
    }

    #[test]
    fn is_checksum_file_table() {
        for (name, want) in [
            ("x.sha256", true),
            ("X.SFV", true),
            ("a.md5", true),
            ("a.sha1", true),
            ("SHA256SUMS", true),
            ("sha256sums.txt", true),
            ("MD5SUMS", true),
            ("SHA1SUMS.TXT", true),
            ("x.sha256.html", false),
            ("SUMS", false),
            ("notes.txt", false),
            ("movie.mkv", false),
        ] {
            assert_eq!(is_checksum_file(name), want, "{name}");
        }
    }

    #[test]
    fn pasted_lengths() {
        assert_eq!(parse_pasted(CRC_HH), Ok((Algo::Crc32, CRC_HH.into())));
        assert_eq!(parse_pasted(&MD5_HH.to_uppercase()), Ok((Algo::Md5, MD5_HH.into())));
        assert_eq!(parse_pasted(SHA1_HH), Ok((Algo::Sha1, SHA1_HH.into())));
        assert_eq!(parse_pasted(&format!("  sha256:{SHA256_HH}\n")), Ok((Algo::Sha256, SHA256_HH.into())));
        let err = "Not a checksum: use 8, 32, 40 or 64 hex characters".to_string();
        assert_eq!(parse_pasted("0123456789"), Err(err.clone()));
        assert_eq!(parse_pasted(&"zz".repeat(16)), Err(err.clone()));
        assert_eq!(parse_pasted(""), Err(err.clone()));
        // A prefix that names the wrong algorithm for the length is rejected too.
        assert_eq!(parse_pasted(&format!("md5:{SHA256_HH}")), Err(err));
    }

    #[test]
    fn tagged_roundtrip() {
        assert_eq!(tagged(Algo::Sha1, SHA1_HH), format!("sha1:{SHA1_HH}"));
        assert_eq!(parse_tagged(&format!("sha1:{SHA1_HH}")), Some((Algo::Sha1, SHA1_HH.into())));
        assert_eq!(parse_tagged("nope"), None);
        assert_eq!(Algo::Sha256.label(), "SHA-256");
        assert_eq!(Algo::Crc32.label(), "CRC32");
    }

    #[test]
    fn hash_file_known_values() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("hh.txt");
        std::fs::write(&p, b"hello haul\n").unwrap();
        assert_eq!(hash_file(&p, Algo::Crc32).unwrap(), CRC_HH);
        assert_eq!(hash_file(&p, Algo::Md5).unwrap(), MD5_HH);
        assert_eq!(hash_file(&p, Algo::Sha1).unwrap(), SHA1_HH);
        // Checked against `certutil -hashfile hh.txt SHA256`.
        assert_eq!(hash_file(&p, Algo::Sha256).unwrap(), SHA256_HH);
    }

    #[test]
    fn hash_file_spans_chunks() {
        use sha2::Digest;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("big.bin");
        let data: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&p, &data).unwrap();
        let want = sha2::Sha256::digest(&data).iter().map(|b| format!("{b:02x}")).collect::<String>();
        assert_eq!(hash_file(&p, Algo::Sha256).unwrap(), want);
    }

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, HeaderValue::from_str(v).unwrap());
        }
        h
    }

    #[test]
    fn digest_headers() {
        let sha_b64 = "2QCtRzznMHRR8ZwXw03RervIA1hCZCkyhjs1EEpBHpw=";
        let md5_b64 = "DdjAPvdGhuEWk9z/u3qUFw==";
        let want_sha = Some(format!("sha256:{SHA256_HH}"));
        let want_md5 = Some(format!("md5:{MD5_HH}"));
        assert_eq!(digest_from_headers(&headers(&[("repr-digest", &format!("sha-256=:{sha_b64}:"))]), false), want_sha);
        assert_eq!(
            digest_from_headers(&headers(&[("repr-digest", &format!("sha-512=:AAAA:, sha-256=:{sha_b64}:"))]), false),
            want_sha
        );
        assert_eq!(digest_from_headers(&headers(&[("digest", &format!("MD5={md5_b64}"))]), false), want_md5);
        assert_eq!(digest_from_headers(&headers(&[("digest", &format!("SHA-256={sha_b64}"))]), false), want_sha);
        assert_eq!(digest_from_headers(&headers(&[("content-md5", md5_b64)]), true), want_md5);
        assert_eq!(digest_from_headers(&headers(&[("content-md5", md5_b64)]), false), None);
        assert_eq!(digest_from_headers(&headers(&[("digest", "sha-256=notbase64!")]), true), None);
        assert_eq!(digest_from_headers(&HeaderMap::new(), true), None);
    }
}
