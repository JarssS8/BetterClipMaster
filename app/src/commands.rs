//! Tauri commands bridging the UI (webview) to the core store.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine;
use serde::Serialize;
use tauri::{Manager, State};

use godclipboard_core::model::{ClipItem, ClipKind};
use godclipboard_core::{rank, Store};

/// Maximum items pulled from the store before fuzzy filtering.
const LIST_LIMIT: usize = 1000;

/// Shared application state.
pub struct AppState {
    pub store: Mutex<Store>,
    /// Capture pause flag, shared with the background watcher thread.
    pub paused: Arc<AtomicBool>,
}

impl AppState {
    pub fn new(store: Store, paused: Arc<AtomicBool>) -> AppState {
        AppState {
            store: Mutex::new(store),
            paused,
        }
    }
}

/// Data sent to the UI for a single item. Image blobs become data URLs; other
/// kinds carry their text/HTML/paths in `content`.
#[derive(Serialize)]
pub struct ItemDto {
    pub id: i64,
    pub kind: String,
    pub preview: String,
    pub content: String,
    pub pinned: bool,
    pub dataurl: Option<String>,
}

impl From<&ClipItem> for ItemDto {
    fn from(item: &ClipItem) -> ItemDto {
        let dataurl = match (item.kind, &item.blob) {
            (ClipKind::Image, Some(bytes)) => {
                let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
                Some(format!("data:image/png;base64,{b64}"))
            }
            _ => None,
        };
        ItemDto {
            id: item.id,
            kind: item.kind.as_str().to_string(),
            preview: item.preview.clone(),
            content: item.content.clone(),
            pinned: item.pinned,
            dataurl,
        }
    }
}

fn map_err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Return history items matching `query` (empty query = full history).
#[tauri::command]
pub fn list(state: State<AppState>, query: String) -> Result<Vec<ItemDto>, String> {
    let store = state.store.lock().map_err(map_err)?;
    let items = store.recent(LIST_LIMIT).map_err(map_err)?;
    let ranked = rank(&items, &query);
    Ok(ranked.iter().map(ItemDto::from).collect())
}

/// Pin or unpin an item.
#[tauri::command]
pub fn pin(state: State<AppState>, id: i64, pinned: bool) -> Result<(), String> {
    let store = state.store.lock().map_err(map_err)?;
    store.set_pinned(id, pinned).map_err(map_err)
}

/// Delete an item.
#[tauri::command]
pub fn remove(state: State<AppState>, id: i64) -> Result<(), String> {
    let store = state.store.lock().map_err(map_err)?;
    store.delete(id).map_err(map_err)
}

/// Toggle capture pause. Returns the new paused state.
#[tauri::command]
pub fn toggle_pause(state: State<AppState>) -> bool {
    let now = !state.paused.load(Ordering::SeqCst);
    state.paused.store(now, Ordering::SeqCst);
    now
}

/// Hide the overlay window.
#[tauri::command]
pub fn hide_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        win.hide().map_err(map_err)?;
    }
    Ok(())
}

/// Put the selected item on the clipboard, hide the overlay, and paste it into
/// the previously focused application.
#[tauri::command]
pub fn paste_item(app: tauri::AppHandle, state: State<AppState>, id: i64) -> Result<(), String> {
    let item = {
        let store = state.store.lock().map_err(map_err)?;
        store.get(id).map_err(map_err)?
    };
    let Some(item) = item else {
        return Err(format!("item {id} not found"));
    };

    crate::paste::set_clipboard(&item).map_err(map_err)?;

    if let Some(win) = app.get_webview_window("main") {
        win.hide().map_err(map_err)?;
    }

    // Best-effort paste; if it fails the item is still on the clipboard.
    crate::paste::send_paste();
    Ok(())
}
