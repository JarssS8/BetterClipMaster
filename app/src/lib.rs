//! godclipboard — Tauri application shell.
//!
//! Wires the platform-independent `godclipboard-core` (store, watcher, search)
//! to the OS: a background clipboard-watcher thread, a global hotkey, a tray
//! icon, and the overlay UI served from `ui/`.

mod clipboard_source;
mod commands;
mod hotkey;
mod paste;
mod tray;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::Manager;

use godclipboard_core::watcher::Watcher;
use godclipboard_core::Store;

use clipboard_source::OsClipboard;
use commands::AppState;

/// Maximum non-pinned items retained in history.
const MAX_ITEMS: usize = 1000;
/// How often the watcher polls the OS clipboard.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Path to the history database in the per-user data directory.
fn db_path() -> std::path::PathBuf {
    if let Some(dirs) = directories::ProjectDirs::from("com", "jars", "godclipboard") {
        let dir = dirs.data_dir();
        let _ = std::fs::create_dir_all(dir);
        dir.join("clips.db")
    } else {
        std::path::PathBuf::from("clips.db")
    }
}

/// Spawn the background watcher thread. It uses its own SQLite connection to the
/// same file (WAL mode permits concurrent readers/writers).
fn spawn_watcher(path: std::path::PathBuf, paused: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let store = match Store::open(&path) {
            Ok(s) => s,
            Err(e) => {
                log::error!("watcher: open store failed: {e}");
                return;
            }
        };
        let mut source = match OsClipboard::new() {
            Ok(s) => s,
            Err(e) => {
                log::error!("watcher: clipboard init failed: {e}");
                return;
            }
        };
        loop {
            if !paused.load(Ordering::SeqCst) {
                match Watcher::poll_once(&mut source, &store) {
                    Ok(Some(_)) => {
                        let _ = store.prune(MAX_ITEMS);
                    }
                    Ok(None) => {}
                    Err(e) => log::warn!("watcher: {e}"),
                }
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let path = db_path();
    let paused = Arc::new(AtomicBool::new(false));

    let store = Store::open(&path).expect("failed to open clipboard database");
    let app_state = AppState::new(store, paused.clone());

    spawn_watcher(path, paused);

    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    hotkey::handle(app, shortcut, event.state());
                })
                .build(),
        )
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::list,
            commands::pin,
            commands::remove,
            commands::toggle_pause,
            commands::hide_window,
            commands::paste_item,
        ])
        .setup(|app| {
            use tauri_plugin_global_shortcut::GlobalShortcutExt;
            app.global_shortcut().register(hotkey::toggle_shortcut())?;
            tray::build(app)?;

            // Alfred-style: hide the overlay when it loses focus.
            if let Some(win) = app.get_webview_window("main") {
                let w = win.clone();
                win.on_window_event(move |event| {
                    if let tauri::WindowEvent::Focused(false) = event {
                        let _ = w.hide();
                    }
                });
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, _event| {});
}
