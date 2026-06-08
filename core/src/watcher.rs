//! The clipboard watcher loop, decoupled from any real OS clipboard via the
//! [`ClipboardSource`] trait so it can be driven deterministically in tests.

use crate::model::{ClipKind, NewItem};
use crate::privacy::is_sensitive;
use crate::store::Store;
use crate::Result;

/// A single clipboard reading.
#[derive(Debug, Clone)]
pub struct Capture {
    pub kind: ClipKind,
    pub content: String,
    pub blob: Option<Vec<u8>>,
    /// Names of the formats present on the clipboard (for privacy detection).
    pub formats: Vec<String>,
}

impl Capture {
    pub fn text(content: impl Into<String>) -> Capture {
        Capture {
            kind: ClipKind::Text,
            content: content.into(),
            blob: None,
            formats: vec!["CF_UNICODETEXT".to_string()],
        }
    }

    pub fn image(png: Vec<u8>, label: impl Into<String>) -> Capture {
        Capture {
            kind: ClipKind::Image,
            content: label.into(),
            blob: Some(png),
            formats: vec!["CF_DIB".to_string()],
        }
    }

    /// A capture flagged as sensitive (e.g. from a password manager).
    pub fn sensitive(content: impl Into<String>) -> Capture {
        Capture {
            kind: ClipKind::Text,
            content: content.into(),
            blob: None,
            formats: vec![
                "CF_UNICODETEXT".to_string(),
                "Clipboard Viewer Ignore".to_string(),
            ],
        }
    }

    fn into_new_item(self) -> NewItem {
        NewItem {
            kind: self.kind,
            content: self.content,
            blob: self.blob,
        }
    }
}

/// Source of clipboard changes. Implementations return `Some` only when the
/// clipboard has changed since the previous read.
pub trait ClipboardSource {
    fn read(&mut self) -> Option<Capture>;
}

/// Drives clipboard captures into the store, honouring privacy markers and
/// deduplication.
pub struct Watcher;

impl Watcher {
    /// Read once from `source`, skipping privacy-marked content.
    ///
    /// Returns the inserted/bumped row id, or `None` if nothing was stored
    /// (no change, sensitive content, or a consecutive duplicate).
    pub fn poll_once(source: &mut dyn ClipboardSource, store: &Store) -> Result<Option<i64>> {
        Self::poll_once_filtered(source, store, true)
    }

    /// Like [`poll_once`](Self::poll_once) but with explicit control over
    /// whether sensitive (privacy-marked) content is skipped.
    pub fn poll_once_filtered(
        source: &mut dyn ClipboardSource,
        store: &Store,
        skip_sensitive: bool,
    ) -> Result<Option<i64>> {
        let Some(capture) = source.read() else {
            return Ok(None);
        };
        if skip_sensitive && is_sensitive(&capture.formats) {
            return Ok(None);
        }
        store.insert(&capture.into_new_item())
    }
}

/// In-memory programmable clipboard source for tests.
pub struct MockSource {
    queue: std::collections::VecDeque<Capture>,
}

impl MockSource {
    pub fn new(captures: Vec<Capture>) -> MockSource {
        MockSource {
            queue: captures.into(),
        }
    }

    pub fn push(&mut self, capture: Capture) {
        self.queue.push_back(capture);
    }
}

impl ClipboardSource for MockSource {
    fn read(&mut self) -> Option<Capture> {
        self.queue.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_skips_sensitive_and_dedupes() {
        let store = Store::open_in_memory().unwrap();
        let mut source = MockSource::new(vec![
            Capture::text("a"),
            Capture::text("a"), // consecutive duplicate
            Capture::sensitive("hunter2"),
            Capture::image(vec![1, 2, 3], "img"),
        ]);

        // Drain all four reads.
        Watcher::poll_once(&mut source, &store).unwrap(); // a -> stored
        Watcher::poll_once(&mut source, &store).unwrap(); // a -> dup, ignored
        Watcher::poll_once(&mut source, &store).unwrap(); // sensitive -> skipped
        Watcher::poll_once(&mut source, &store).unwrap(); // image -> stored

        assert_eq!(store.count().unwrap(), 2);
        let all = store.all().unwrap();
        assert!(all.iter().any(|i| i.content == "a"));
        assert!(all.iter().any(|i| i.kind == ClipKind::Image));
        assert!(!all.iter().any(|i| i.content == "hunter2"));
    }

    #[test]
    fn poll_returns_none_when_no_change() {
        let store = Store::open_in_memory().unwrap();
        let mut source = MockSource::new(vec![]);
        assert!(Watcher::poll_once(&mut source, &store).unwrap().is_none());
    }

    #[test]
    fn poll_filtered_can_keep_sensitive() {
        let store = Store::open_in_memory().unwrap();
        let mut source = MockSource::new(vec![Capture::sensitive("kept")]);
        // skip_sensitive = false stores even privacy-marked content.
        Watcher::poll_once_filtered(&mut source, &store, false).unwrap();
        assert_eq!(store.count().unwrap(), 1);
    }
}
