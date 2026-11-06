//! Pure sorting logic: categories by extension and the user's ordered rules.

use crate::grabber::grouping::archive_part;
use crate::naming::sanitize_filename;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A default destination by extension. `folder` is absolute, or relative to the download folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub name: String,
    pub exts: Vec<String>,
    pub folder: String,
}

const COMPRESSED: &str = "Compressed";

pub fn default_categories() -> Vec<Category> {
    let cat = |name: &str, exts: &str| Category {
        name: name.into(),
        exts: exts.split_whitespace().map(String::from).collect(),
        folder: name.into(),
    };
    vec![
        cat("Video", "mp4 mkv avi mov wmv flv webm m4v mpg mpeg ts 3gp"),
        cat("Music", "mp3 flac wav aac ogg opus m4a wma aiff"),
        cat("Documents", "pdf doc docx xls xlsx ppt pptx odt ods odp txt rtf csv epub md"),
        cat("Programs", "exe msi msix appx bat cmd ps1 iso img apk"),
        cat(COMPRESSED, "zip rar 7z tar gz tgz bz2 xz zst cab"),
        cat("Other", ""),
    ]
}

/// What a rule looks at. An unset field matches everything.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RuleMatch {
    /// `example.com`, or `*.example.com` for it and its subdomains.
    pub host: Option<String>,
    /// A glob over the whole file name, or a regex written `/…/`. Case-insensitive.
    pub name: Option<String>,
    /// Lowercase, no dot.
    pub exts: Vec<String>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RuleAction {
    pub folder: Option<String>,
    /// Tokens: `{name}` `{ext}` `{package}` `{host}` `{date}`.
    pub rename: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    #[serde(default)]
    pub id: i64,
    pub name: String,
    pub enabled: bool,
    #[serde(rename = "match")]
    pub matcher: RuleMatch,
    pub action: RuleAction,
}

/// What the rules know about one finished file.
#[derive(Clone, Copy, Debug)]
pub struct Facts<'a> {
    pub name: &'a str,
    pub host: &'a str,
    pub size: Option<u64>,
    pub package: &'a str,
    pub date: chrono::NaiveDate,
}

/// Where a file goes. `folder: None` means it stays where it is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Destination {
    pub folder: Option<PathBuf>,
    pub name: String,
    /// `Rule “<name>”`, a category name, or empty when nothing matched.
    pub by: String,
}

fn ext_of(name: &str) -> String {
    name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
}

fn stem_of(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(s, _)| s)
}

fn set(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// A compiled file-name pattern.
enum NameMatcher {
    Glob(globset::GlobMatcher),
    Regex(regex::Regex),
}

fn name_matcher(pattern: &str) -> Result<NameMatcher, String> {
    if pattern.len() >= 2 && pattern.starts_with('/') && pattern.ends_with('/') {
        let re = regex::RegexBuilder::new(&pattern[1..pattern.len() - 1])
            .case_insensitive(true)
            .build()
            .map_err(|e| e.to_string())?;
        return Ok(NameMatcher::Regex(re));
    }
    let g = globset::GlobBuilder::new(pattern)
        .case_insensitive(true)
        .literal_separator(false)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(NameMatcher::Glob(g.compile_matcher()))
}

/// Checks a rule before it is saved. The messages are shown to the user.
pub fn validate_rule(r: &Rule) -> Result<(), String> {
    let name = r.name.trim();
    if name.is_empty() {
        return Err("Rule needs a name".into());
    }
    if let Some(p) = set(&r.matcher.name) {
        name_matcher(p).map_err(|e| format!("Bad pattern in “{name}”: {e}"))?;
    }
    if let (Some(lo), Some(hi)) = (r.matcher.min_size, r.matcher.max_size)
        && lo > hi
    {
        return Err(format!("Min size is bigger than max size in “{name}”"));
    }
    if set(&r.action.folder).is_none() && set(&r.action.rename).is_none() {
        return Err(format!("Rule “{name}” has no folder or rename"));
    }
    Ok(())
}

fn host_matches(pattern: &str, host: &str) -> bool {
    let (p, h) = (pattern.to_ascii_lowercase(), host.to_ascii_lowercase());
    match p.strip_prefix("*.") {
        Some(root) => h == root || h.ends_with(&format!(".{root}")),
        None => h == p,
    }
}

/// Whether a valid rule matches. A rule whose pattern doesn't compile matches nothing.
fn matches(m: &RuleMatch, f: &Facts) -> bool {
    if let Some(h) = set(&m.host)
        && !host_matches(h, f.host)
    {
        return false;
    }
    if let Some(p) = set(&m.name) {
        let hit = match name_matcher(p) {
            Ok(NameMatcher::Glob(g)) => g.is_match(f.name),
            Ok(NameMatcher::Regex(re)) => re.is_match(f.name),
            Err(_) => false,
        };
        if !hit {
            return false;
        }
    }
    if !m.exts.is_empty() {
        let ext = ext_of(f.name);
        if !m.exts.iter().any(|e| e.trim_start_matches('.').eq_ignore_ascii_case(&ext)) {
            return false;
        }
    }
    if m.min_size.is_some() || m.max_size.is_some() {
        let Some(size) = f.size else { return false };
        if m.min_size.is_some_and(|lo| size < lo) || m.max_size.is_some_and(|hi| size > hi) {
            return false;
        }
    }
    true
}

/// The category a file name belongs to, as an index into `cats` (which must not be empty).
pub fn category_of(name: &str, cats: &[Category]) -> usize {
    let ext = ext_of(name);
    if !ext.is_empty()
        && let Some(i) = cats.iter().position(|c| c.exts.iter().any(|e| e.eq_ignore_ascii_case(&ext)))
    {
        return i;
    }
    if archive_part(name).is_some()
        && let Some(i) = cats.iter().position(|c| c.name == COMPRESSED)
    {
        return i;
    }
    cats.iter().position(|c| c.exts.is_empty()).unwrap_or(cats.len().saturating_sub(1))
}

/// Fills a rename pattern and makes the result a safe Windows name.
pub fn render(pattern: &str, f: &Facts) -> String {
    let out = pattern
        .replace("{name}", stem_of(f.name))
        .replace("{ext}", &ext_of(f.name))
        .replace("{package}", f.package)
        .replace("{host}", f.host)
        .replace("{date}", &f.date.format("%Y-%m-%d").to_string());
    sanitize_filename(&out)
}

fn resolve(base: &Path, folder: &str) -> PathBuf {
    base.join(folder)
}

/// Where a file goes: the first enabled, valid rule that matches; else its category when
/// sorting by category is on; else it stays.
pub fn decide(f: &Facts, rules: &[Rule], cats: &[Category], sort_categories: bool, base: &Path) -> Destination {
    for r in rules.iter().filter(|r| r.enabled && validate_rule(r).is_ok()) {
        if matches(&r.matcher, f) {
            return Destination {
                folder: set(&r.action.folder).map(|d| resolve(base, d)),
                name: set(&r.action.rename).map_or_else(|| f.name.to_string(), |p| render(p, f)),
                by: format!("Rule “{}”", r.name.trim()),
            };
        }
    }
    if sort_categories && !cats.is_empty() {
        let c = &cats[category_of(f.name, cats)];
        return Destination { folder: Some(resolve(base, &c.folder)), name: f.name.to_string(), by: c.name.clone() };
    }
    Destination { folder: None, name: f.name.to_string(), by: String::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::path::{Path, PathBuf};

    fn facts(name: &str) -> Facts<'_> {
        Facts {
            name,
            host: "releases.ubuntu.com",
            size: Some(5_000_000),
            package: "P",
            date: NaiveDate::from_ymd_opt(2026, 11, 6).unwrap(),
        }
    }

    fn rule(name: &str, m: RuleMatch, folder: &str) -> Rule {
        Rule {
            id: 0,
            name: name.into(),
            enabled: true,
            matcher: m,
            action: RuleAction { folder: Some(folder.into()), rename: None },
        }
    }

    fn by_name(name: &str) -> RuleMatch {
        RuleMatch { name: Some(name.into()), ..Default::default() }
    }

    #[test]
    fn category_table() {
        let cats = default_categories();
        let name = |f: &str| cats[category_of(f, &cats)].name.clone();
        assert_eq!(name("movie.MKV"), "Video");
        assert_eq!(name("a.tar.gz"), "Compressed");
        assert_eq!(name("x.part2.rar"), "Compressed");
        assert_eq!(name("set.r01"), "Compressed");
        assert_eq!(name("split.7z.002"), "Compressed");
        assert_eq!(name("ubuntu.iso"), "Programs");
        assert_eq!(name("notes"), "Other");
        assert_eq!(name("song.flac"), "Music");
        assert_eq!(name("report.pdf"), "Documents");
        assert_eq!(name("weird.xyz"), "Other");
    }

    #[test]
    fn default_categories_shape() {
        let cats = default_categories();
        let names: Vec<_> = cats.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Video", "Music", "Documents", "Programs", "Compressed", "Other"]);
        assert!(cats.iter().all(|c| c.folder == c.name));
        assert!(cats[5].exts.is_empty());
    }

    #[test]
    fn host_patterns() {
        let m = RuleMatch { host: Some("*.ubuntu.com".into()), ..Default::default() };
        let r = rule("U", m, "U");
        let base = Path::new(r"D:\dl");
        let mut f = facts("a.iso");
        for (host, hit) in
            [("releases.ubuntu.com", true), ("ubuntu.com", true), ("UBUNTU.COM", true), ("notubuntu.com", false)]
        {
            f.host = host;
            assert_eq!(decide(&f, std::slice::from_ref(&r), &[], false, base).by == "Rule “U”", hit, "{host}");
        }
        let exact = rule("E", RuleMatch { host: Some("ubuntu.com".into()), ..Default::default() }, "E");
        f.host = "releases.ubuntu.com";
        assert_eq!(decide(&f, &[exact], &[], false, base).by, "");
    }

    #[test]
    fn glob_and_regex_names() {
        let base = Path::new(r"D:\dl");
        let g = rule("G", by_name("*.iso"), "G");
        assert_eq!(decide(&facts("A.ISO"), std::slice::from_ref(&g), &[], false, base).by, "Rule “G”");
        assert_eq!(decide(&facts("A.ISO.txt"), &[g], &[], false, base).by, "");
        let r = rule("R", by_name(r"/^ubuntu-\d+/"), "R");
        assert_eq!(decide(&facts("Ubuntu-24.04.iso"), std::slice::from_ref(&r), &[], false, base).by, "Rule “R”");
        assert_eq!(decide(&facts("my-ubuntu-24.iso"), &[r], &[], false, base).by, "");
    }

    #[test]
    fn exts_match() {
        let base = Path::new(r"D:\dl");
        let r = rule("X", RuleMatch { exts: vec!["iso".into(), "img".into()], ..Default::default() }, "X");
        assert_eq!(decide(&facts("a.IMG"), std::slice::from_ref(&r), &[], false, base).by, "Rule “X”");
        assert_eq!(decide(&facts("a.zip"), &[r], &[], false, base).by, "");
    }

    #[test]
    fn size_range_inclusive() {
        let base = Path::new(r"D:\dl");
        let r = rule("S", RuleMatch { min_size: Some(10), max_size: Some(20), ..Default::default() }, "S");
        let mut f = facts("a.bin");
        for (size, hit) in [(Some(9), false), (Some(10), true), (Some(20), true), (Some(21), false), (None, false)] {
            f.size = size;
            assert_eq!(decide(&f, std::slice::from_ref(&r), &[], false, base).by == "Rule “S”", hit, "{size:?}");
        }
    }

    #[test]
    fn first_match_wins_and_disabled_skipped() {
        let base = Path::new(r"D:\dl");
        let mut off = rule("Off", by_name("*.iso"), "Off");
        off.enabled = false;
        let rules = [off, rule("First", by_name("*.iso"), "First"), rule("Second", by_name("*"), "Second")];
        let d = decide(&facts("a.iso"), &rules, &[], false, base);
        assert_eq!(d.by, "Rule “First”");
        assert_eq!(d.folder, Some(PathBuf::from(r"D:\dl\First")));
    }

    #[test]
    fn empty_match_matches_everything() {
        let r = rule("All", RuleMatch::default(), r"E:\all");
        let d = decide(&facts("x.bin"), &[r], &[], false, Path::new(r"D:\dl"));
        assert_eq!(d.folder, Some(PathBuf::from(r"E:\all")));
    }

    #[test]
    fn render_tokens() {
        let f = Facts { name: "a b.txt", ..facts("") };
        assert_eq!(render("{package}/{name}-{date}.{ext}", &f), "P_a b-2026-11-06.txt");
        assert_eq!(render("{host}-{name}", &Facts { name: "noext", ..facts("") }), "releases.ubuntu.com-noext");
    }

    #[test]
    fn rename_applies_without_folder() {
        let mut r = rule("Ren", RuleMatch::default(), "");
        r.action = RuleAction { folder: None, rename: Some("{package} - {name}.{ext}".into()) };
        let d = decide(&facts("a.iso"), &[r], &default_categories(), true, Path::new(r"D:\dl"));
        assert_eq!(d.folder, None);
        assert_eq!(d.name, "P - a.iso");
    }

    #[test]
    fn relative_folder_resolves_against_base() {
        let base = Path::new(r"D:\dl");
        let d = decide(&facts("a.iso"), &[rule("R", RuleMatch::default(), r"Programs\ISO")], &[], false, base);
        assert_eq!(d.folder, Some(PathBuf::from(r"D:\dl\Programs\ISO")));
        let cats = default_categories();
        let d = decide(&facts("a.mkv"), &[], &cats, true, base);
        assert_eq!((d.folder, d.by.as_str(), d.name.as_str()), (Some(PathBuf::from(r"D:\dl\Video")), "Video", "a.mkv"));
    }

    #[test]
    fn no_sort_no_rule_stays() {
        let d = decide(&facts("a.mkv"), &[], &default_categories(), false, Path::new(r"D:\dl"));
        assert_eq!((d.folder, d.name.as_str(), d.by.as_str()), (None, "a.mkv", ""));
    }

    #[test]
    fn validate_rejects_bad_regex() {
        assert_eq!(
            validate_rule(&rule("Bad", by_name("/(/"), "x")).unwrap_err().split(':').next(),
            Some("Bad pattern in “Bad”")
        );
        assert!(validate_rule(&rule("Glob", by_name("[a"), "x")).is_err());
        assert!(validate_rule(&rule("Ok", by_name("*.iso"), "x")).is_ok());
    }

    #[test]
    fn validate_rejects_min_over_max() {
        let r = rule("M", RuleMatch { min_size: Some(5), max_size: Some(4), ..Default::default() }, "x");
        assert_eq!(validate_rule(&r).unwrap_err(), "Min size is bigger than max size in “M”");
    }

    #[test]
    fn validate_needs_name_and_action() {
        assert_eq!(validate_rule(&rule("  ", RuleMatch::default(), "x")).unwrap_err(), "Rule needs a name");
        let mut r = rule("N", RuleMatch::default(), "x");
        r.action = RuleAction { folder: Some(" ".into()), rename: None };
        assert_eq!(validate_rule(&r).unwrap_err(), "Rule “N” has no folder or rename");
    }

    #[test]
    fn broken_stored_rule_is_skipped() {
        let rules = [rule("Broken", by_name("/(/"), "B"), rule("Good", by_name("*.iso"), "G")];
        assert_eq!(decide(&facts("a.iso"), &rules, &[], false, Path::new(r"D:\dl")).by, "Rule “Good”");
    }

    #[test]
    fn rule_json_shape() {
        let r = rule("R", by_name("*.iso"), "x");
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["match"]["name"], "*.iso");
        assert_eq!(v["match"]["minSize"], serde_json::Value::Null);
        let back: Rule = serde_json::from_value(v).unwrap();
        assert_eq!(back, r);
    }
}
