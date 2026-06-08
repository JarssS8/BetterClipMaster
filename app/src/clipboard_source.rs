//! Real OS clipboard reader implementing the core [`ClipboardSource`] trait.
//!
//! Text and images are read cross-platform via `arboard`. Image bytes are
//! re-encoded to PNG so the stored blob is directly renderable as a data URL.
//! On Windows we additionally detect the "Clipboard Viewer Ignore" marker so
//! password-manager payloads are never captured.
//!
//! Capture of rich-text (HTML) and file lists from the OS clipboard is a
//! follow-up (the core model already supports both kinds); see README.

use betterclipmaster_core::model::{compute_hash, ClipKind};
use betterclipmaster_core::watcher::{Capture, ClipboardSource};

/// Polling clipboard reader with change detection.
pub struct OsClipboard {
    clip: arboard::Clipboard,
    last_hash: Option<String>,
}

impl OsClipboard {
    pub fn new() -> Result<OsClipboard, arboard::Error> {
        Ok(OsClipboard {
            clip: arboard::Clipboard::new()?,
            last_hash: None,
        })
    }

    /// Encode raw RGBA pixels to PNG bytes.
    fn encode_png(img: &arboard::ImageData) -> Option<Vec<u8>> {
        let buf = image::RgbaImage::from_raw(
            img.width as u32,
            img.height as u32,
            img.bytes.clone().into_owned(),
        )?;
        let mut out = std::io::Cursor::new(Vec::new());
        buf.write_to(&mut out, image::ImageFormat::Png).ok()?;
        Some(out.into_inner())
    }

    /// Read the current clipboard as a Capture, regardless of change state.
    fn read_current(&mut self) -> Option<Capture> {
        // Text first: it is the common case and cheapest.
        if let Ok(text) = self.clip.get_text() {
            if !text.is_empty() {
                return Some(Capture {
                    kind: ClipKind::Text,
                    content: text,
                    blob: None,
                    formats: current_formats(),
                });
            }
        }
        // Then image.
        if let Ok(img) = self.clip.get_image() {
            let label = format!("Imagen {}x{}", img.width, img.height);
            if let Some(png) = Self::encode_png(&img) {
                return Some(Capture {
                    kind: ClipKind::Image,
                    content: label,
                    blob: Some(png),
                    formats: current_formats(),
                });
            }
        }
        None
    }
}

impl ClipboardSource for OsClipboard {
    fn read(&mut self) -> Option<Capture> {
        let capture = self.read_current()?;
        let hash = compute_hash(capture.kind, &capture.content, capture.blob.as_deref());
        if self.last_hash.as_deref() == Some(hash.as_str()) {
            return None; // unchanged since last poll
        }
        self.last_hash = Some(hash);
        Some(capture)
    }
}

/// Format names currently on the clipboard, used for privacy detection.
#[cfg(windows)]
fn current_formats() -> Vec<String> {
    // Detect the well-known "Clipboard Viewer Ignore" marker set by password
    // managers. If present we surface its name so the core skips the capture.
    if let Some(fmt) = clipboard_win::register_format("Clipboard Viewer Ignore") {
        if clipboard_win::is_format_avail(fmt.get()) {
            return vec!["Clipboard Viewer Ignore".to_string()];
        }
    }
    Vec::new()
}

/// On non-Windows platforms we cannot cheaply enumerate marker formats; privacy
/// markers are honoured on Windows (the primary target).
#[cfg(not(windows))]
fn current_formats() -> Vec<String> {
    Vec::new()
}
