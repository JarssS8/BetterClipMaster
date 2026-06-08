//! Global hotkey handling via the Tauri global-shortcut plugin.
//!
//! Default binding: Ctrl+Shift+V toggles the overlay. When shown, the window is
//! focused and an `open` event is emitted so the UI can reset and focus search.

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};

/// The default toggle shortcut.
pub fn toggle_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyV)
}

/// Handler invoked by the plugin on every registered shortcut event.
pub fn handle(app: &AppHandle, shortcut: &Shortcut, state: ShortcutState) {
    if shortcut != &toggle_shortcut() || state != ShortcutState::Pressed {
        return;
    }
    toggle_overlay(app);
}

/// Show+focus the overlay if hidden, hide it if already visible.
pub fn toggle_overlay(app: &AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    let visible = win.is_visible().unwrap_or(false);
    if visible {
        let _ = win.hide();
    } else {
        let _ = win.show();
        let _ = win.set_focus();
        let _ = app.emit("open", ());
    }
}
