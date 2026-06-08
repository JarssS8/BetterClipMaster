//! Detection of clipboard content that should not be stored.
//!
//! Password managers and security-sensitive apps mark their clipboard payloads
//! with well-known formats so that clipboard history tools skip them. We honour
//! those markers and never persist matching captures.

/// Marker clipboard format names (case-insensitive) that indicate the content
/// must be excluded from history.
const SENSITIVE_MARKERS: &[&str] = &[
    "clipboard viewer ignore",
    "excludeclipboardcontentfrommonitorprocessing",
    "cf_clipboard_viewer_ignore",
    "org.nspasteboard.concealedtype",
];

/// Returns true if any of the provided clipboard format names marks the content
/// as sensitive and therefore not to be stored.
pub fn is_sensitive(formats: &[String]) -> bool {
    formats.iter().any(|f| {
        let lower = f.trim().to_ascii_lowercase();
        SENSITIVE_MARKERS.contains(&lower.as_str())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_known_marker_case_insensitive() {
        let formats = vec!["CF_TEXT".to_string(), "Clipboard Viewer Ignore".to_string()];
        assert!(is_sensitive(&formats));
    }

    #[test]
    fn detects_exclude_marker() {
        let formats = vec!["ExcludeClipboardContentFromMonitorProcessing".to_string()];
        assert!(is_sensitive(&formats));
    }

    #[test]
    fn ignores_normal_formats() {
        let formats = vec!["CF_TEXT".to_string(), "CF_UNICODETEXT".to_string()];
        assert!(!is_sensitive(&formats));
    }

    #[test]
    fn empty_is_not_sensitive() {
        assert!(!is_sensitive(&[]));
    }
}
