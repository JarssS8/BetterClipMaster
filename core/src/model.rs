//! The clipboard item model and content hashing.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The kind of content held by a clipboard item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClipKind {
    /// Plain text.
    Text,
    /// Rich text stored as HTML.
    Rich,
    /// A raster image (PNG bytes in `blob`).
    Image,
    /// One or more file paths, newline-separated in `content`.
    Files,
}

impl ClipKind {
    /// Stable string used for DB storage and hashing.
    pub fn as_str(&self) -> &'static str {
        match self {
            ClipKind::Text => "text",
            ClipKind::Rich => "rich",
            ClipKind::Image => "image",
            ClipKind::Files => "files",
        }
    }

    /// Parse from the stored string. Unknown values fall back to `Text`.
    pub fn parse_str(s: &str) -> ClipKind {
        match s {
            "rich" => ClipKind::Rich,
            "image" => ClipKind::Image,
            "files" => ClipKind::Files,
            _ => ClipKind::Text,
        }
    }
}

/// A new item to be inserted (no id/timestamp yet).
#[derive(Debug, Clone)]
pub struct NewItem {
    pub kind: ClipKind,
    /// Text/HTML/paths. Empty for pure images.
    pub content: String,
    /// Raw bytes (image PNG). `None` for text-like items.
    pub blob: Option<Vec<u8>>,
}

impl NewItem {
    pub fn text(content: impl Into<String>) -> NewItem {
        NewItem {
            kind: ClipKind::Text,
            content: content.into(),
            blob: None,
        }
    }

    pub fn rich(html: impl Into<String>) -> NewItem {
        NewItem {
            kind: ClipKind::Rich,
            content: html.into(),
            blob: None,
        }
    }

    pub fn image(png: Vec<u8>, label: impl Into<String>) -> NewItem {
        NewItem {
            kind: ClipKind::Image,
            content: label.into(),
            blob: Some(png),
        }
    }

    pub fn files(paths: &[String]) -> NewItem {
        NewItem {
            kind: ClipKind::Files,
            content: paths.join("\n"),
            blob: None,
        }
    }

    /// Content hash used for deduplication.
    pub fn hash(&self) -> String {
        compute_hash(self.kind, &self.content, self.blob.as_deref())
    }

    /// Short single-line preview for the list view.
    pub fn preview(&self) -> String {
        match self.kind {
            ClipKind::Image => {
                if self.content.is_empty() {
                    "🖼 Imagen".to_string()
                } else {
                    self.content.clone()
                }
            }
            _ => make_preview(&self.content),
        }
    }
}

/// A persisted clipboard item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClipItem {
    pub id: i64,
    pub kind: ClipKind,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<Vec<u8>>,
    pub preview: String,
    pub pinned: bool,
    pub created_at: i64,
    pub hash: String,
}

/// SHA-256 over kind + content + optional blob, hex-encoded.
pub fn compute_hash(kind: ClipKind, content: &str, blob: Option<&[u8]>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(kind.as_str().as_bytes());
    hasher.update([0u8]);
    hasher.update(content.as_bytes());
    if let Some(b) = blob {
        hasher.update([0u8]);
        hasher.update(b);
    }
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{:02x}", byte));
    }
    out
}

/// Collapse whitespace to a single line and truncate to 120 chars.
pub fn make_preview(content: &str) -> String {
    let collapsed: String = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim();
    if trimmed.chars().count() > 120 {
        let truncated: String = trimmed.chars().take(120).collect();
        format!("{}…", truncated)
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_roundtrips_through_string() {
        for k in [
            ClipKind::Text,
            ClipKind::Rich,
            ClipKind::Image,
            ClipKind::Files,
        ] {
            assert_eq!(ClipKind::parse_str(k.as_str()), k);
        }
    }

    #[test]
    fn kind_serializes_lowercase() {
        let json = serde_json::to_string(&ClipKind::Rich).unwrap();
        assert_eq!(json, "\"rich\"");
    }

    #[test]
    fn hash_is_stable_for_same_content() {
        let a = compute_hash(ClipKind::Text, "hello", None);
        let b = compute_hash(ClipKind::Text, "hello", None);
        assert_eq!(a, b);
    }

    #[test]
    fn hash_differs_for_different_content() {
        let a = compute_hash(ClipKind::Text, "hello", None);
        let b = compute_hash(ClipKind::Text, "world", None);
        assert_ne!(a, b);
    }

    #[test]
    fn hash_differs_by_kind_and_blob() {
        assert_ne!(
            compute_hash(ClipKind::Text, "x", None),
            compute_hash(ClipKind::Rich, "x", None)
        );
        assert_ne!(
            compute_hash(ClipKind::Image, "", Some(&[1, 2, 3])),
            compute_hash(ClipKind::Image, "", Some(&[1, 2, 4]))
        );
    }

    #[test]
    fn preview_collapses_and_truncates() {
        assert_eq!(make_preview("  hola\n  mundo  "), "hola mundo");
        let long = "a".repeat(200);
        let p = make_preview(&long);
        assert_eq!(p.chars().count(), 121); // 120 + ellipsis
    }

    #[test]
    fn image_preview_has_label_fallback() {
        let empty = NewItem::image(vec![1, 2, 3], "");
        assert_eq!(empty.preview(), "🖼 Imagen");
        let labeled = NewItem::image(vec![1, 2, 3], "captura 800x600");
        assert_eq!(labeled.preview(), "captura 800x600");
    }
}
