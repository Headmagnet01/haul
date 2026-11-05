//! Pure grouping: which file names are parts of one multi-part archive, and what to call a package.

use serde::Serialize;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum ArchiveKind {
    #[serde(rename = "rar")]
    Rar,
    #[serde(rename = "7z")]
    SevenZ,
    #[serde(rename = "zip")]
    Zip,
}

impl ArchiveKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ArchiveKind::Rar => "rar",
            ArchiveKind::SevenZ => "7z",
            ArchiveKind::Zip => "zip",
        }
    }
}

/// A file name recognised as one volume of a multi-part archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartOf {
    /// The name before the archive suffix, in its original case.
    pub base: String,
    pub kind: ArchiveKind,
    /// Sort order inside the set.
    pub part: u32,
    /// False for `name.rar` and `name.zip`, which are only a set together with a numbered sibling.
    pub explicit: bool,
}

/// `digits` as a number when it is at least `min` ASCII digits long.
fn number(digits: &str, min: usize) -> Option<u32> {
    (digits.len() >= min && digits.bytes().all(|b| b.is_ascii_digit())).then(|| digits.parse().ok()).flatten()
}

/// Splits `s` at its last dot: (`before`, `after`).
fn split_ext(s: &str) -> Option<(&str, &str)> {
    s.rsplit_once('.')
}

pub fn archive_part(name: &str) -> Option<PartOf> {
    let (stem, ext) = split_ext(name)?;
    let ext_l = ext.to_ascii_lowercase();
    let part = |base: &str, kind, part, explicit| {
        (!base.is_empty()).then(|| PartOf { base: base.to_string(), kind, part, explicit })
    };
    match ext_l.as_str() {
        "rar" => {
            // `name.partN.rar`, or the first volume `name.rar` of an old-style set.
            let (base, last) = split_ext(stem).unwrap_or(("", stem));
            let last_l = last.to_ascii_lowercase();
            if let Some(n) = last_l.strip_prefix("part").and_then(|d| number(d, 1)) {
                return part(base, ArchiveKind::Rar, n, true);
            }
            part(stem, ArchiveKind::Rar, 0, false)
        }
        "zip" => part(stem, ArchiveKind::Zip, u32::MAX, false),
        _ => {
            if let Some(n) = ext_l.strip_prefix('r').and_then(|d| number(d, 2)) {
                return part(stem, ArchiveKind::Rar, n.checked_add(1)?, true);
            }
            if let Some(n) = ext_l.strip_prefix('z').and_then(|d| number(d, 2)) {
                return part(stem, ArchiveKind::Zip, n, true);
            }
            let n = number(&ext_l, 3)?;
            let (base, inner) = split_ext(stem)?;
            let kind = match inner.to_ascii_lowercase().as_str() {
                "7z" => ArchiveKind::SevenZ,
                "zip" => ArchiveKind::Zip,
                _ => return None,
            };
            part(base, kind, n, true)
        }
    }
}

/// One package's worth of names from a batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    /// `"<kind>:<lowercase base>"` for an archive set, `"rest"` for everything else.
    pub key: String,
    pub archive: Option<ArchiveKind>,
    pub base: Option<String>,
    /// Indexes into the input, archive parts in part order.
    pub members: Vec<usize>,
}

/// Splits one batch's names into archive sets (in order of first appearance) and the rest.
pub fn group_names(names: &[&str]) -> Vec<Group> {
    let parts: Vec<Option<PartOf>> = names.iter().map(|n| archive_part(n)).collect();
    let mut order: Vec<(ArchiveKind, String)> = Vec::new();
    let mut sets: HashMap<(ArchiveKind, String), Vec<usize>> = HashMap::new();
    for (i, p) in parts.iter().enumerate() {
        if let Some(p) = p {
            let k = (p.kind, p.base.to_lowercase());
            if !sets.contains_key(&k) {
                order.push(k.clone());
            }
            sets.entry(k).or_default().push(i);
        }
    }
    let mut out = Vec::new();
    let mut grouped = vec![false; names.len()];
    for k in order {
        let mut members = sets.remove(&k).unwrap_or_default();
        if !members.iter().any(|&i| parts[i].as_ref().is_some_and(|p| p.explicit)) {
            continue;
        }
        members.sort_by_key(|&i| parts[i].as_ref().map_or(0, |p| p.part));
        members.iter().for_each(|&i| grouped[i] = true);
        let first = members.iter().min().copied().unwrap_or(0);
        out.push(Group {
            key: format!("{}:{}", k.0.as_str(), k.1),
            archive: Some(k.0),
            base: parts[first].as_ref().map(|p| p.base.clone()),
            members,
        });
    }
    let rest: Vec<usize> = (0..names.len()).filter(|&i| !grouped[i]).collect();
    if !rest.is_empty() {
        out.push(Group { key: "rest".into(), archive: None, base: None, members: rest });
    }
    out
}

const SEPARATORS: [char; 7] = [' ', '-', '_', '.', '(', '[', ','];

/// The shared start of the file names (extensions dropped), cut at a word boundary,
/// when it is long enough to name a package.
pub fn common_name_prefix(names: &[&str]) -> Option<String> {
    if names.len() < 2 {
        return None;
    }
    let stems: Vec<Vec<char>> = names
        .iter()
        .map(|n| match split_ext(n) {
            Some((s, _)) if !s.is_empty() => s.chars().collect(),
            _ => n.chars().collect(),
        })
        .collect();
    let mut len = 0;
    while stems.iter().all(|s| s.len() > len && s[len] == stems[0][len]) {
        len += 1;
    }
    let mut prefix: Vec<char> = stems[0][..len].to_vec();
    let mid_token = prefix.last().is_some_and(|c| c.is_alphanumeric())
        && stems.iter().any(|s| s.get(len).is_some_and(|c| c.is_alphanumeric()));
    if mid_token {
        let cut = prefix.iter().rposition(|c| SEPARATORS.contains(c)).unwrap_or(0);
        prefix.truncate(cut);
    }
    while prefix.last().is_some_and(|c| SEPARATORS.contains(c)) {
        prefix.pop();
    }
    (prefix.len() >= 4).then(|| prefix.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ArchiveKind::*;
    const M: u32 = u32::MAX;
    type Expected = Option<(&'static str, ArchiveKind, u32, bool)>;

    #[test]
    fn archive_part_table() {
        let table: &[(&str, Expected)] = &[
            ("movie.part1.rar", Some(("movie", Rar, 1, true))),
            ("movie.part01.rar", Some(("movie", Rar, 1, true))),
            ("movie.part002.rar", Some(("movie", Rar, 2, true))),
            ("Movie.Part10.RAR", Some(("Movie", Rar, 10, true))),
            ("my.show.s01.part3.rar", Some(("my.show.s01", Rar, 3, true))),
            ("backup 2024.part1.rar", Some(("backup 2024", Rar, 1, true))),
            ("x.part0.rar", Some(("x", Rar, 0, true))),
            ("set.rar", Some(("set", Rar, 0, false))),
            ("set.r00", Some(("set", Rar, 1, true))),
            ("set.r01", Some(("set", Rar, 2, true))),
            ("set.r99", Some(("set", Rar, 100, true))),
            ("SET.R12", Some(("SET", Rar, 13, true))),
            ("set.r100", Some(("set", Rar, 101, true))),
            ("caminandes-3-4k.7z.001", Some(("caminandes-3-4k", SevenZ, 1, true))),
            ("caminandes-3-4k.7z.004", Some(("caminandes-3-4k", SevenZ, 4, true))),
            ("data.7z.0001", Some(("data", SevenZ, 1, true))),
            ("Data.7Z.010", Some(("Data", SevenZ, 10, true))),
            ("pack.zip.001", Some(("pack", Zip, 1, true))),
            ("pack.zip.012", Some(("pack", Zip, 12, true))),
            ("pack.zip", Some(("pack", Zip, M, false))),
            ("godot-4.4-stable-win64.zip", Some(("godot-4.4-stable-win64", Zip, M, false))),
            ("pack.z01", Some(("pack", Zip, 1, true))),
            ("pack.z09", Some(("pack", Zip, 9, true))),
            ("PACK.Z10", Some(("PACK", Zip, 10, true))),
            ("movie.mkv", None),
            ("movie.2024.mkv", None),
            ("report.part.pdf", None),
            ("report.part1.pdf", None),
            ("notes.part1.txt", None),
            ("x.part1.7z", None),
            ("set.r", None),
            ("set.r1", None),
            ("set.rx1", None),
            ("photo.r2d2.png", None),
            ("data.7z", None),
            ("data.7z.1", None),
            ("data.7z.01", None),
            ("dump.7z.001.tmp", None),
            ("pack.zip.1", None),
            ("pack.z1", None),
            ("pack.zipx", None),
            ("archive.tar.gz", None),
            ("archive.tgz", None),
            ("file.001", None),
            ("readme.txt", None),
            ("v1.2.3.exe", None),
            ("setup.rar.exe", None),
            ("movie.part1.rar.html", None),
            ("part1.rar", None),
            (".rar", None),
            ("song.mp3", None),
            ("linux.iso", None),
            ("setup.msi", None),
        ];
        for (name, want) in table {
            let got = archive_part(name).map(|p| (p.base, p.kind, p.part, p.explicit));
            assert_eq!(got, want.map(|(b, k, n, e)| (b.to_string(), k, n, e)), "{name}");
        }
    }

    fn shape(gs: &[Group]) -> Vec<(&str, Vec<usize>)> {
        gs.iter().map(|g| (g.key.as_str(), g.members.clone())).collect()
    }

    #[test]
    fn groups_mixed_batch() {
        let g = group_names(&["a.part2.rar", "readme.txt", "a.part1.rar", "b.7z.001", "b.7z.002", "c.bin"]);
        assert_eq!(shape(&g), vec![("rar:a", vec![2, 0]), ("7z:b", vec![3, 4]), ("rest", vec![1, 5])]);
        assert_eq!(g[0].base.as_deref(), Some("a"));
    }

    #[test]
    fn lone_rar_and_zip_stay_in_rest() {
        assert_eq!(shape(&group_names(&["x.rar", "y.zip", "z.bin"])), vec![("rest", vec![0, 1, 2])]);
    }

    #[test]
    fn old_style_rar_set() {
        assert_eq!(shape(&group_names(&["s.r01", "s.rar", "s.r00"])), vec![("rar:s", vec![1, 2, 0])]);
    }

    #[test]
    fn split_zip_last_volume() {
        assert_eq!(shape(&group_names(&["p.zip", "p.z01", "p.z02"])), vec![("zip:p", vec![1, 2, 0])]);
    }

    #[test]
    fn case_insensitive_base() {
        let g = group_names(&["X.PART1.RAR", "x.part2.rar"]);
        assert_eq!(shape(&g), vec![("rar:x", vec![0, 1])]);
        assert_eq!(g[0].base.as_deref(), Some("X"));
    }

    #[test]
    fn different_kinds_same_base_separate() {
        assert_eq!(shape(&group_names(&["a.7z.001", "a.zip.001"])), vec![("7z:a", vec![0]), ("zip:a", vec![1])]);
    }

    #[test]
    fn prefix_names() {
        let p = |n: &[&str]| common_name_prefix(n);
        assert_eq!(
            p(&["godot-4.4-stable-win64.zip", "godot-4.4-stable-linux.zip"]).as_deref(),
            Some("godot-4.4-stable")
        );
        assert_eq!(p(&["Movie.Name.S01E01.mkv", "Movie.Name.S01E02.mkv"]).as_deref(), Some("Movie.Name"));
        assert_eq!(p(&["report-final.pdf", "report-draft.pdf"]).as_deref(), Some("report"));
        assert_eq!(p(&["data_2024_01.csv", "data_2024_02.csv"]).as_deref(), Some("data_2024"));
        assert_eq!(p(&["same.bin", "same.bin"]).as_deref(), Some("same"));
        assert_eq!(p(&["abc1.bin", "abc2.bin"]), None);
        assert_eq!(p(&["a.bin", "b.bin"]), None);
        assert_eq!(p(&["one.bin"]), None);
    }
}
