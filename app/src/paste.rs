//! Placing an item back on the clipboard and simulating a paste keystroke.

use std::borrow::Cow;

use betterclipmaster_core::model::{ClipItem, ClipKind};

/// Put an item onto the OS clipboard.
///
/// Text, rich text and file lists are written as text (rich text falls back to
/// its HTML source). Images are decoded back to RGBA and written as a bitmap.
pub fn set_clipboard(item: &ClipItem) -> Result<(), String> {
    let mut clip = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    match item.kind {
        ClipKind::Image => {
            let bytes = item
                .blob
                .as_ref()
                .ok_or_else(|| "image item has no blob".to_string())?;
            let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            clip.set_image(arboard::ImageData {
                width: w as usize,
                height: h as usize,
                bytes: Cow::Owned(rgba.into_raw()),
            })
            .map_err(|e| e.to_string())
        }
        _ => clip
            .set_text(item.content.clone())
            .map_err(|e| e.to_string()),
    }
}

/// Simulate the paste shortcut in the currently focused application.
///
/// Best-effort: failures are logged, not propagated, because the item is
/// already on the clipboard and the user can paste manually (Ctrl+V).
pub fn send_paste() {
    // Give the OS a moment to return focus to the previous window.
    std::thread::sleep(std::time::Duration::from_millis(120));

    use enigo::{
        Direction::{Click, Press, Release},
        Enigo, Key, Keyboard, Settings,
    };

    let mut enigo = match Enigo::new(&Settings::default()) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("paste: could not init input simulation: {e}");
            return;
        }
    };

    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    let result = (|| -> Result<(), enigo::InputError> {
        enigo.key(modifier, Press)?;
        enigo.key(Key::Unicode('v'), Click)?;
        enigo.key(modifier, Release)?;
        Ok(())
    })();

    if let Err(e) = result {
        log::warn!("paste: input simulation failed: {e}");
    }
}
