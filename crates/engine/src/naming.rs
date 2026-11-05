//! Safe Windows file names, names from headers and URLs, and collision-free paths.

use crate::types::DownloadId;
use percent_encoding::percent_decode_str;
use std::path::{Path, PathBuf};

const MAX_NAME_CHARS: usize = 200;
const FALLBACK: &str = "download";
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2",
    "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

fn trim_end_dots_spaces(s: &str) -> &str {
    s.trim_end_matches(['.', ' '])
}

/// Turns any string into a name Windows accepts, keeping the extension.
pub fn sanitize_filename(raw: &str) -> String {
    let replaced: String =
        raw.chars().map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { '_' } else { c }).collect();
    let mut name = trim_end_dots_spaces(replaced.trim_start()).to_string();
    if name.chars().all(|c| c == '.') {
        return FALLBACK.to_string();
    }

    if name.chars().count() > MAX_NAME_CHARS {
        let ext = name
            .rfind('.')
            .filter(|&i| i > 0 && name[i..].chars().count() <= 20)
            .map(|i| name[i..].to_string())
            .unwrap_or_default();
        let stem_chars = MAX_NAME_CHARS - ext.chars().count();
        let stem: String = name.chars().take(stem_chars).collect();
        let stem = trim_end_dots_spaces(&stem);
        name = if stem.is_empty() { format!("{FALLBACK}{ext}") } else { format!("{stem}{ext}") };
    }

    let stem = name.split('.').next().unwrap_or("");
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem)) {
        name.insert(0, '_');
    }
    name
}

/// Splits a Content-Disposition header into (key, value) params, honouring quotes.
fn disposition_params(h: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut parts = Vec::new();
    let (mut cur, mut quoted, mut escaped) = (String::new(), false, false);
    for c in h.chars() {
        if escaped {
            cur.push(c);
            escaped = false;
        } else if quoted && c == '\\' {
            escaped = true;
        } else if c == '"' {
            quoted = !quoted;
            cur.push(c);
        } else if c == ';' && !quoted {
            parts.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    parts.push(cur);
    for p in parts {
        if let Some((k, v)) = p.split_once('=') {
            let v = v.trim();
            let v = v.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or(v);
            out.push((k.trim().to_ascii_lowercase(), v.to_string()));
        }
    }
    out
}

/// Decodes an RFC 5987 value: `charset'lang'percent-encoded`.
fn decode_ext_value(v: &str) -> Option<String> {
    let mut it = v.splitn(3, '\'');
    let charset = it.next()?.to_ascii_lowercase();
    let _lang = it.next()?;
    let bytes: Vec<u8> = percent_decode_str(it.next()?).collect();
    match charset.as_str() {
        "utf-8" => String::from_utf8(bytes).ok(),
        "iso-8859-1" => Some(bytes.iter().map(|&b| b as char).collect()),
        _ => None,
    }
}

/// Picks a file name: `filename*` beats `filename`, then the URL's last path segment.
pub fn filename_from(content_disposition: Option<&str>, url: &url::Url) -> String {
    if let Some(h) = content_disposition {
        let params = disposition_params(h);
        let ext = params.iter().find(|(k, _)| k == "filename*").and_then(|(_, v)| decode_ext_value(v));
        let plain = params.iter().find(|(k, _)| k == "filename").map(|(_, v)| v.clone());
        if let Some(n) = ext.or(plain).filter(|n| !n.trim().is_empty()) {
            return sanitize_filename(&n);
        }
    }
    let seg = url
        .path_segments()
        .and_then(|mut s| s.rfind(|p| !p.is_empty()))
        .map(|s| percent_decode_str(s).decode_utf8_lossy().into_owned());
    match seg {
        Some(s) if !s.trim().is_empty() => sanitize_filename(&s),
        _ => FALLBACK.to_string(),
    }
}

fn numbered(file_name: &str, n: u32) -> String {
    match file_name.rfind('.').filter(|&i| i > 0) {
        Some(i) => format!("{} ({n}){}", &file_name[..i], &file_name[i..]),
        None => format!("{file_name} ({n})"),
    }
}

/// The first of `name`, `name (1)`, `name (2)`… that does not exist in `dir`.
pub fn unique_path(dir: &Path, file_name: &str) -> PathBuf {
    let first = dir.join(file_name);
    if !first.exists() {
        return first;
    }
    (1..).map(|n| dir.join(numbered(file_name, n))).find(|p| !p.exists()).unwrap()
}

/// Like `unique_path`, but atomically creates an empty placeholder there so two
/// downloads finishing at the same moment can never pick the same name.
pub fn reserve_unique_path(dir: &Path, file_name: &str) -> std::io::Result<PathBuf> {
    for n in 0u32.. {
        let p = if n == 0 { dir.join(file_name) } else { dir.join(numbered(file_name, n)) };
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&p) {
            Ok(_) => return Ok(p),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}

/// `<dir>/<name>.<id>.haulpart`
pub fn part_path(dir: &Path, file_name: &str, id: DownloadId) -> PathBuf {
    dir.join(format!("{file_name}.{id}.haulpart"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;

    #[test]
    fn sanitize_replaces_illegal() {
        assert_eq!(sanitize_filename("a<b>:c\"d/e\\f|g?h*.txt"), "a_b__c_d_e_f_g_h_.txt");
        assert_eq!(sanitize_filename("tab\there\u{1}.txt"), "tab_here_.txt");
    }

    #[test]
    fn sanitize_strips_trailing_dots_spaces() {
        assert_eq!(sanitize_filename("report. . "), "report");
    }

    #[test]
    fn sanitize_reserved_names() {
        assert_eq!(sanitize_filename("CON"), "_CON");
        assert_eq!(sanitize_filename("com1.txt"), "_com1.txt");
        assert_eq!(sanitize_filename("console.txt"), "console.txt");
    }

    #[test]
    fn sanitize_long_keeps_extension() {
        let raw = format!("{}.mkv", "a".repeat(300));
        let s = sanitize_filename(&raw);
        assert!(s.chars().count() <= 200, "len {}", s.chars().count());
        assert!(s.ends_with(".mkv"));
    }

    #[test]
    fn sanitize_empty() {
        assert_eq!(sanitize_filename(""), "download");
        assert_eq!(sanitize_filename("..."), "download");
    }

    #[test]
    fn filename_rfc5987() {
        let u = Url::parse("https://x.org/ignored.bin").unwrap();
        let h = "attachment; filename*=UTF-8''na%C3%AFve%20file.zip";
        assert_eq!(filename_from(Some(h), &u), "naïve file.zip");
        let both = "attachment; filename=\"plain.zip\"; filename*=UTF-8''fancy.zip";
        assert_eq!(filename_from(Some(both), &u), "fancy.zip");
    }

    #[test]
    fn filename_quoted() {
        let u = Url::parse("https://x.org/ignored.bin").unwrap();
        assert_eq!(filename_from(Some("attachment; filename=\"my file.iso\""), &u), "my file.iso");
    }

    #[test]
    fn filename_from_url_path() {
        let u = Url::parse("https://x.org/a/b/ubuntu%2024.iso?x=1").unwrap();
        assert_eq!(filename_from(None, &u), "ubuntu 24.iso");
    }

    #[test]
    fn filename_fallback() {
        let u = Url::parse("https://x.org/").unwrap();
        assert_eq!(filename_from(None, &u), "download");
    }

    #[test]
    fn unique_path_counts_up() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.bin"), b"").unwrap();
        std::fs::write(dir.path().join("a (1).bin"), b"").unwrap();
        assert_eq!(unique_path(dir.path(), "a.bin"), dir.path().join("a (2).bin"));
        std::fs::write(dir.path().join("README"), b"").unwrap();
        assert_eq!(unique_path(dir.path(), "README"), dir.path().join("README (1)"));
        assert_eq!(unique_path(dir.path(), "new.txt"), dir.path().join("new.txt"));
    }

    #[test]
    fn reserve_unique_path_creates_placeholder() {
        let dir = tempfile::tempdir().unwrap();
        let a = reserve_unique_path(dir.path(), "x.zip").unwrap();
        let b = reserve_unique_path(dir.path(), "x.zip").unwrap();
        assert_eq!(a, dir.path().join("x.zip"));
        assert_eq!(b, dir.path().join("x (1).zip"));
        assert!(a.exists() && b.exists());
    }

    #[test]
    fn part_path_has_id() {
        let d = std::path::Path::new("C:\\dl");
        assert_eq!(part_path(d, "a.bin", 7), d.join("a.bin.7.haulpart"));
    }
}
