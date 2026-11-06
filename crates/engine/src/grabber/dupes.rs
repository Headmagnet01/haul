//! Pure duplicate detection for Collector links.

use serde::Serialize;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DupKind {
    /// A finished download has the same link, or the same name and size.
    Downloaded,
    /// A download that has not finished does.
    Queued,
    /// An earlier Collector link does.
    Collector,
}

/// What a link or download is compared by.
#[derive(Clone, Debug)]
pub struct DupKey<'a> {
    /// The original URL and, once known, the final one.
    pub urls: Vec<&'a str>,
    pub name: &'a str,
    pub size: Option<u64>,
}

#[derive(Default)]
struct Seen<'a> {
    urls: HashSet<&'a str>,
    /// Lowercased name with a known size; Windows file names ignore case.
    named: HashSet<(String, u64)>,
}

impl<'a> Seen<'a> {
    fn of(keys: &[DupKey<'a>]) -> Self {
        let mut s = Seen::default();
        keys.iter().for_each(|k| s.add(k));
        s
    }

    fn named_key(k: &DupKey) -> Option<(String, u64)> {
        Some((k.name.to_lowercase(), k.size?)).filter(|(n, _)| !n.is_empty())
    }

    fn add(&mut self, k: &DupKey<'a>) {
        self.urls.extend(k.urls.iter().copied());
        if let Some(n) = Self::named_key(k) {
            self.named.insert(n);
        }
    }

    fn has(&self, k: &DupKey) -> bool {
        k.urls.iter().any(|u| self.urls.contains(u)) || Self::named_key(k).is_some_and(|n| self.named.contains(&n))
    }
}

/// For each link: whether it repeats a finished download, a queued one, or an earlier link.
pub fn find_duplicates(links: &[DupKey], queued: &[DupKey], finished: &[DupKey]) -> Vec<Option<DupKind>> {
    let (fin, que) = (Seen::of(finished), Seen::of(queued));
    let mut earlier = Seen::default();
    links
        .iter()
        .map(|k| {
            let kind = if fin.has(k) {
                Some(DupKind::Downloaded)
            } else if que.has(k) {
                Some(DupKind::Queued)
            } else if earlier.has(k) {
                Some(DupKind::Collector)
            } else {
                None
            };
            earlier.add(k);
            kind
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k<'a>(urls: &[&'a str], name: &'a str, size: Option<u64>) -> DupKey<'a> {
        DupKey { urls: urls.to_vec(), name, size }
    }

    #[test]
    fn same_final_url() {
        let links = [k(&["https://a/x?sig=1", "https://cdn/x"], "x", None)];
        let queued = [k(&["https://cdn/x"], "other", None)];
        assert_eq!(find_duplicates(&links, &queued, &[]), vec![Some(DupKind::Queued)]);
    }

    #[test]
    fn same_name_and_size_case_insensitive() {
        let links = [k(&["https://a/1"], "X.ZIP", Some(10))];
        let finished = [k(&["https://b/2"], "x.zip", Some(10))];
        assert_eq!(find_duplicates(&links, &[], &finished), vec![Some(DupKind::Downloaded)]);
        let other_size = [k(&["https://b/2"], "x.zip", Some(11))];
        assert_eq!(find_duplicates(&links, &[], &other_size), vec![None]);
    }

    #[test]
    fn unknown_size_never_matches_by_name() {
        let links = [k(&["https://a/1"], "x.zip", None)];
        let queued = [k(&["https://b/2"], "x.zip", None)];
        assert_eq!(find_duplicates(&links, &queued, &[]), vec![None]);
    }

    #[test]
    fn finished_beats_queued() {
        let links = [k(&["https://a/1"], "x", None)];
        let both = [k(&["https://a/1"], "x", None)];
        assert_eq!(find_duplicates(&links, &both, &both), vec![Some(DupKind::Downloaded)]);
    }

    #[test]
    fn earlier_collector_link() {
        let links = [k(&["https://a/1"], "same.bin", Some(5)), k(&["https://b/2"], "same.bin", Some(5))];
        assert_eq!(find_duplicates(&links, &[], &[]), vec![None, Some(DupKind::Collector)]);
    }
}
