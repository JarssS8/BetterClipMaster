//! Real OS clipboard reader implementing the core [`ClipboardSource`] trait.
//!
//! Text and images are read cross-platform via `arboard`. Image bytes are
//! re-encoded to PNG so the stored blob is directly renderable as a data URL.
//! On Windows we additionally detect the "Clipboard Viewer Ignore" marker so
//! password-manager payloads are never captured.
//!
//! On macOS we use NSPasteboard.changeCount for reliable change detection:
//! it increments on every write regardless of format or source (keyboard,
//! context menu, programmatic). This prevents misses that the hash-only
//! approach could produce when arboard can't read an intermediate clipboard
//! state during a write.

use betterclipmaster_core::model::{compute_hash, ClipKind};
use betterclipmaster_core::watcher::{Capture, ClipboardSource};

/// Polling clipboard reader with change detection.
pub struct OsClipboard {
    clip: arboard::Clipboard,
    last_hash: Option<String>,
    /// macOS: last observed NSPasteboard.changeCount.
    #[cfg(target_os = "macos")]
    last_change_count: isize,
    /// macOS: changeCount advanced but read_current() returned None (transient
    /// empty state during write) — retry on the next poll.
    #[cfg(target_os = "macos")]
    pending_read: bool,
}

impl OsClipboard {
    pub fn new() -> Result<OsClipboard, arboard::Error> {
        Ok(OsClipboard {
            clip: arboard::Clipboard::new()?,
            last_hash: None,
            #[cfg(target_os = "macos")]
            last_change_count: macos_change_count(),
            #[cfg(target_os = "macos")]
            pending_read: false,
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
        // Image: hash raw RGBA bytes before the expensive PNG encode.
        // If the image hasn't changed since last capture, skip encoding entirely.
        if let Ok(img) = self.clip.get_image() {
            use betterclipmaster_core::model::compute_hash;
            let raw_hash = compute_hash(ClipKind::Image, "", Some(img.bytes.as_ref()));
            if self.last_hash.as_deref() == Some(raw_hash.as_str()) {
                return None;
            }
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

/// Query NSPasteboard.generalPasteboard.changeCount.
/// Increments on every clipboard write regardless of format or source.
#[cfg(target_os = "macos")]
fn macos_change_count() -> isize {
    use objc2::{class, msg_send};
    use objc2::runtime::AnyObject;
    unsafe {
        let pb: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
        msg_send![pb, changeCount]
    }
}

impl ClipboardSource for OsClipboard {
    fn read(&mut self) -> Option<Capture> {
        #[cfg(target_os = "macos")]
        {
            let current = macos_change_count();
            if current != self.last_change_count {
                // A write happened — reset retry state and record new count.
                self.last_change_count = current;
                self.pending_read = false;
            } else if !self.pending_read {
                // Nothing changed and no pending retry.
                return None;
            }
        }

        let Some(capture) = self.read_current() else {
            // Clipboard changed but content not yet readable (transient empty
            // state during a write). Retry on the next poll.
            #[cfg(target_os = "macos")]
            {
                self.pending_read = true;
            }
            return None;
        };

        #[cfg(target_os = "macos")]
        {
            self.pending_read = false;
        }

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
