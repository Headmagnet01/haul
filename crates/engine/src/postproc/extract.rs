//! Extracting archives with the bundled 7-Zip.

use crate::grabber::grouping::{ArchiveKind, archive_part};
use crate::postproc::checksum::is_checksum_file;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, PartialEq, Eq)]
pub enum ExtractOutcome {
    Ok,
    WrongPassword,
    /// Damaged, unsupported, or 7-Zip couldn't run. The text is shown to the user.
    Failed(String),
}

/// Extracts one archive set (given by its first volume) into `out`. Blocking.
pub trait Extractor: Send + Sync {
    fn extract(&self, first_volume: &Path, out: &Path, password: &str) -> ExtractOutcome;
}

/// The bundled `7z.exe` (with `7z.dll` beside it).
pub struct SevenZip {
    pub exe: PathBuf,
}

/// `CREATE_NO_WINDOW`: no console flashes up while 7-Zip runs.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

impl Extractor for SevenZip {
    fn extract(&self, first_volume: &Path, out: &Path, password: &str) -> ExtractOutcome {
        use std::os::windows::process::CommandExt;
        let mut o = std::ffi::OsString::from("-o");
        o.push(out);
        let output = Command::new(&self.exe)
            .args(["x", "-y", "-bso0", "-bsp0"])
            .arg(format!("-p{password}"))
            .arg(o)
            .arg(first_volume)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let output = match output {
            Ok(o) => o,
            Err(e) => return ExtractOutcome::Failed(format!("Couldn't run 7-Zip: {e}")),
        };
        if output.status.success() {
            return ExtractOutcome::Ok;
        }
        let text = format!("{}\n{}", String::from_utf8_lossy(&output.stderr), String::from_utf8_lossy(&output.stdout));
        if text.contains("Wrong password") {
            return ExtractOutcome::WrongPassword;
        }
        let msg = text
            .lines()
            .rev()
            .find_map(|l| l.split_once("ERROR:").map(|(_, m)| m.trim().to_string()).filter(|m| !m.is_empty()));
        ExtractOutcome::Failed(msg.unwrap_or_else(|| format!("7-Zip exit {}", output.status.code().unwrap_or(-1))))
    }
}

/// The bundled 7-Zip in the source tree, for tests.
#[doc(hidden)]
pub fn seven_zip_for_tests() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/src-tauri/resources/7zip/7z.exe")
}

/// Single-file archive types 7-Zip opens on their own.
const LONE_EXTS: [&str; 6] = ["zip", "rar", "7z", "tar", "gz", "tgz"];

fn lone_archive(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, e)| LONE_EXTS.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

/// Whether a file name is an archive or a volume of one.
pub fn is_archive_name(name: &str) -> bool {
    !is_checksum_file(name) && (archive_part(name).is_some() || lone_archive(name))
}

/// (kind, lowercase base) of an archive set.
type SetKey = (ArchiveKind, String);
/// (part number, input index, explicit).
type Member = (u32, usize, bool);

/// The archive sets in a list of names, in order of first appearance. Each set lists indexes into
/// `names`, the volume to pass to 7-Zip first: the lowest part, except a split zip's `.zip`.
pub fn first_volumes(names: &[&str]) -> Vec<Vec<usize>> {
    let mut order: Vec<SetKey> = Vec::new();
    let mut sets: HashMap<SetKey, Vec<Member>> = HashMap::new();
    for (i, n) in names.iter().enumerate() {
        if is_checksum_file(n) {
            continue;
        }
        match archive_part(n) {
            Some(p) => {
                let key = (p.kind, p.base.to_lowercase());
                if !sets.contains_key(&key) {
                    order.push(key.clone());
                }
                sets.entry(key).or_default().push((p.part, i, p.explicit));
            }
            None if lone_archive(n) => {
                order.push((ArchiveKind::Zip, format!("\0lone{i}")));
            }
            None => {}
        }
    }
    let mut out = Vec::new();
    for key in &order {
        if let Some(i) = key.1.strip_prefix("\0lone").and_then(|s| s.parse::<usize>().ok()) {
            out.push(vec![i]);
            continue;
        }
        let mut members = sets[key].clone();
        members.sort_by_key(|&(part, i, _)| (part, i));
        if !members.iter().any(|m| m.2) {
            // `name.rar` / `name.zip` with no numbered sibling: each is a one-volume archive.
            out.extend(members.iter().map(|m| vec![m.1]));
            continue;
        }
        if key.0 == ArchiveKind::Zip
            && let Some(pos) = members.iter().position(|m| m.0 == u32::MAX)
        {
            let zip = members.remove(pos);
            members.insert(0, zip);
        }
        out.push(members.into_iter().map(|m| m.1).collect());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sets(names: &[&str]) -> Vec<Vec<&'static str>> {
        let owned: Vec<&'static str> = names.iter().map(|n| &*Box::leak(n.to_string().into_boxed_str())).collect();
        first_volumes(&owned).into_iter().map(|s| s.into_iter().map(|i| owned[i]).collect()).collect()
    }

    #[test]
    fn first_volumes_table() {
        let names = [
            "notes.txt",
            "SHA256SUMS",
            "split.7z.002",
            "split.7z.001",
            "split.7z.003",
            "multi.part2.rar",
            "multi.part1.rar",
            "oldstyle.r00",
            "oldstyle.rar",
            "plain.zip",
            "plain.7z",
            "secret.rar",
            "pack.tar.gz",
            "x.tgz",
            "movie.mkv",
            "plain.sfv",
        ];
        assert_eq!(
            sets(&names),
            vec![
                vec!["split.7z.001", "split.7z.002", "split.7z.003"],
                vec!["multi.part1.rar", "multi.part2.rar"],
                vec!["oldstyle.rar", "oldstyle.r00"],
                vec!["plain.zip"],
                vec!["plain.7z"],
                vec!["secret.rar"],
                vec!["pack.tar.gz"],
                vec!["x.tgz"],
            ]
        );
    }

    #[test]
    fn split_zip_passes_the_zip_first() {
        assert_eq!(sets(&["a.z01", "a.zip", "a.z02"]), vec![vec!["a.zip", "a.z01", "a.z02"]]);
    }

    #[test]
    fn is_archive_name_table() {
        for (n, want) in [
            ("a.zip", true),
            ("a.r00", true),
            ("a.7z.002", true),
            ("a.TAR", true),
            ("a.txt", false),
            ("SHA256SUMS", false),
        ] {
            assert_eq!(is_archive_name(n), want, "{n}");
        }
    }
}
