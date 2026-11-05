//! Finding HTTP(S) links in pasted text.

/// Every HTTP(S) link in `text`, normalised, without duplicates, in first-seen order.
pub fn extract_links(text: &str) -> Vec<String> {
    let lower = text.to_ascii_lowercase();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let rest = &lower[i..];
        let Some(off) = rest.find("http") else { break };
        let at = i + off;
        let tail = &lower[at..];
        let scheme_len = if tail.starts_with("https://") {
            8
        } else if tail.starts_with("http://") {
            7
        } else {
            i = at + 4;
            continue;
        };
        let end = text[at..]
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | '`'))
            .map_or(text.len(), |n| at + n);
        let candidate = trim_trailing(&text[at..end]);
        if candidate.len() > scheme_len
            && let Ok(u) = url::Url::parse(candidate)
            && u.host_str().is_some_and(|h| !h.is_empty())
        {
            let s = u.to_string();
            if !out.contains(&s) {
                out.push(s);
            }
        }
        i = end.max(at + scheme_len);
    }
    out
}

fn trim_trailing(mut s: &str) -> &str {
    loop {
        let Some(last) = s.chars().last() else { return s };
        let strip = match last {
            '.' | ',' | ';' | ':' | '!' | '?' => true,
            ')' => s.matches('(').count() < s.matches(')').count(),
            ']' => s.matches('[').count() < s.matches(']').count(),
            '}' => s.matches('{').count() < s.matches('}').count(),
            _ => false,
        };
        if !strip {
            return s;
        }
        s = &s[..s.len() - 1];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_plain_and_trailing_punctuation() {
        assert_eq!(
            extract_links("get https://a.org/x.zip and http://b.net/y.iso."),
            vec!["https://a.org/x.zip", "http://b.net/y.iso"]
        );
    }

    #[test]
    fn extract_wrapped() {
        assert_eq!(
            extract_links("<https://a.org/x> \"https://a.org/q?x=1&y=2\", [f](https://a.org/f.bin)"),
            vec!["https://a.org/x", "https://a.org/q?x=1&y=2", "https://a.org/f.bin"]
        );
    }

    #[test]
    fn extract_keeps_balanced_parens() {
        assert_eq!(
            extract_links("(see https://en.wikipedia.org/wiki/Foo_(bar))"),
            vec!["https://en.wikipedia.org/wiki/Foo_(bar)"]
        );
    }

    #[test]
    fn extract_dedupes_and_normalises() {
        assert_eq!(extract_links("HTTPS://A.ORG/Up https://a.org/Up\nhttps://a.org/Up"), vec!["https://a.org/Up"]);
    }

    #[test]
    fn extract_ignores_other_schemes_and_junk() {
        assert!(extract_links("ftp://a.org/x mailto:x@y.z www.a.org https:// http://").is_empty());
        assert!(extract_links("").is_empty());
    }

    #[test]
    fn extract_many() {
        let text: String = (0..500).map(|i| format!("https://a.org/{i}.bin\r\n")).collect();
        assert_eq!(extract_links(&text).len(), 500);
    }
}
