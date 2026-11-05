//! Watch clipboard: copied text with links goes to the Collector while the switch is on.

use crate::core::AppCore;
use clipboard_master::{CallbackResult, ClipboardHandler, Master};
use haul_engine::extract_links;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{Duration, Instant};

/// Larger clipboard text is ignored: it is a copied log or document, not a list of links.
pub const MAX_CLIPBOARD_BYTES: usize = 1_000_000;

/// How long to wait after a change before reading. Many apps (anything using the OLE clipboard,
/// such as .NET and Office) finish a copy in more than one step; opening the clipboard in between
/// makes their copy fail.
const READ_DELAY: Duration = Duration::from_millis(150);

fn digest(text: &str) -> u64 {
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// Decides which clipboard texts become Collector intake.
#[derive(Default)]
pub struct ClipboardFilter {
    /// The last text seen and when; the same text again soon after is one copy, not two.
    last_seen: Option<(u64, Instant)>,
    own: Option<u64>,
}

/// Windows often reports one copy as several changes; the same text within this window is ignored.
const REPEAT_WINDOW: Duration = Duration::from_secs(2);

impl ClipboardFilter {
    pub fn new() -> Self {
        Self::default()
    }

    /// The text to send to the Collector, or `None` if it is too big, repeats the last text within
    /// a moment, was written by Haul itself, or holds no links.
    pub fn accept(&mut self, text: &str) -> Option<String> {
        self.accept_at(text, Instant::now())
    }

    pub fn accept_at(&mut self, text: &str, now: Instant) -> Option<String> {
        if text.len() > MAX_CLIPBOARD_BYTES {
            self.last_seen = None;
            return None;
        }
        let d = digest(text);
        let repeat = self
            .last_seen
            .replace((d, now))
            .is_some_and(|(prev, at)| prev == d && now.saturating_duration_since(at) < REPEAT_WINDOW);
        if repeat || self.own == Some(d) || extract_links(text).is_empty() {
            return None;
        }
        Some(text.to_string())
    }

    /// Haul is about to put `text` on the clipboard; the change that follows is not intake.
    pub fn note_own_write(&mut self, text: &str) {
        self.own = Some(digest(text));
    }
}

struct Watcher {
    core: AppCore,
    filter: ClipboardFilter,
    clipboard: Option<arboard::Clipboard>,
}

impl Watcher {
    fn read_text(&mut self) -> Option<String> {
        // Right after a change the copying app may still hold the clipboard open: try twice.
        for attempt in 0..2 {
            if self.clipboard.is_none() {
                self.clipboard = arboard::Clipboard::new().ok();
            }
            match self.clipboard.as_mut()?.get_text() {
                Ok(t) => return Some(t),
                Err(arboard::Error::ContentNotAvailable) => return None,
                Err(e) => {
                    tracing::debug!(error = %e, attempt, "clipboard read failed");
                    self.clipboard = None;
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
        None
    }
}

impl ClipboardHandler for Watcher {
    fn on_clipboard_change(&mut self) -> CallbackResult {
        if !self.core.watch_clipboard() {
            return CallbackResult::Next;
        }
        std::thread::sleep(READ_DELAY);
        if let Some(text) = self.read_text().and_then(|t| self.filter.accept(&t))
            && let Err(e) = self.core.add_links(&text, "clipboard")
        {
            tracing::warn!(error = %e, "clipboard links not added");
        }
        CallbackResult::Next
    }

    fn on_clipboard_error(&mut self, error: std::io::Error) -> CallbackResult {
        tracing::warn!(error = %error, "clipboard watcher error");
        CallbackResult::Next
    }
}

/// Starts the `haul-clipboard` thread. It runs for the life of the app; while the switch is off
/// it ignores changes, so turning it on never grabs what was already on the clipboard.
pub fn spawn_watcher(core: AppCore) -> std::io::Result<()> {
    std::thread::Builder::new().name("haul-clipboard".into()).spawn(move || {
        let handler = Watcher { core, filter: ClipboardFilter::new(), clipboard: None };
        match Master::new(handler) {
            Ok(mut m) => {
                if let Err(e) = m.run() {
                    tracing::warn!(error = %e, "clipboard watcher stopped");
                }
            }
            Err(e) => tracing::warn!(error = %e, "clipboard watcher could not start"),
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_accepts_links_once() {
        let mut f = ClipboardFilter::new();
        assert_eq!(f.accept("see https://a.org/x").as_deref(), Some("see https://a.org/x"));
        assert_eq!(f.accept("see https://a.org/x"), None);
    }

    #[test]
    fn filter_accepts_same_text_again_later() {
        let mut f = ClipboardFilter::new();
        let t0 = std::time::Instant::now();
        assert!(f.accept_at("https://a.org/x", t0).is_some());
        assert!(f.accept_at("https://a.org/x", t0 + Duration::from_millis(500)).is_none(), "duplicate change event");
        assert!(f.accept_at("https://a.org/x", t0 + Duration::from_secs(5)).is_some(), "copied again later");
    }

    #[test]
    fn filter_rejects_text_without_links() {
        let mut f = ClipboardFilter::new();
        assert_eq!(f.accept("just some words"), None);
        assert_eq!(f.accept(""), None);
    }

    #[test]
    fn filter_rejects_own_write() {
        let mut f = ClipboardFilter::new();
        f.note_own_write("https://a.org/mine");
        assert_eq!(f.accept("https://a.org/mine"), None);
        assert!(f.accept("https://a.org/theirs").is_some());
    }

    #[test]
    fn filter_rejects_huge_text() {
        let mut f = ClipboardFilter::new();
        let mut big = String::from("https://a.org/x ");
        big.push_str(&"a".repeat(MAX_CLIPBOARD_BYTES + 1 - big.len()));
        assert_eq!(big.len(), MAX_CLIPBOARD_BYTES + 1);
        assert_eq!(f.accept(&big), None);
    }

    #[test]
    fn filter_accepts_again_after_other_text() {
        let mut f = ClipboardFilter::new();
        assert!(f.accept("https://a.org/A").is_some());
        assert!(f.accept("https://a.org/B").is_some());
        assert!(f.accept("https://a.org/A").is_some());
    }
}
