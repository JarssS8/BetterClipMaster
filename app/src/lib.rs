//! betterclipmaster — Tauri application shell.
//!
//! Wires the platform-independent `betterclipmaster-core` (store, watcher, search)
//! to the OS: a background clipboard-watcher thread, a configurable global
//! hotkey, a tray icon, a settings window, and the overlay UI served from `ui/`.

mod clipboard_source;
mod commands;
mod hotkey;
mod paste;
mod settings;
mod tray;

use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::Manager;

use betterclipmaster_core::watcher::Watcher;
use betterclipmaster_core::Store;

use clipboard_source::OsClipboard;
use commands::AppState;

/// How often the watcher polls the OS clipboard.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Path to the history database in the per-user data directory.
fn db_path() -> std::path::PathBuf {
    if let Some(dirs) = directories::ProjectDirs::from("com", "jars", "betterclipmaster") {
        let dir = dirs.data_dir();
        let _ = std::fs::create_dir_all(dir);
        dir.join("clips.db")
    } else {
        std::path::PathBuf::from("clips.db")
    }
}

/// Spawn the background watcher thread. It uses its own SQLite connection to the
/// same file (WAL mode permits concurrent readers/writers). The privacy flag and
/// history cap are read live from shared atomics so settings changes take effect
/// without restarting the thread.
fn spawn_watcher(
    path: std::path::PathBuf,
    paused: Arc<AtomicBool>,
    ignore_sensitive: Arc<AtomicBool>,
    max_items: Arc<AtomicUsize>,
) {
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
                let skip = ignore_sensitive.load(Ordering::SeqCst);
                match Watcher::poll_once_filtered(&mut source, &store, skip) {
                    Ok(Some(_)) => {
                        let _ = store.prune(max_items.load(Ordering::SeqCst));
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
    let cfg = settings::load();

    let paused = Arc::new(AtomicBool::new(false));
    let ignore_sensitive = Arc::new(AtomicBool::new(cfg.ignore_sensitive));
    let max_items = Arc::new(AtomicUsize::new(cfg.max_items));

    let store = Store::open(&path).expect("failed to open clipboard database");
    let app_state = AppState::new(
        store,
        paused.clone(),
        ignore_sensitive.clone(),
        max_items.clone(),
        cfg.clone(),
    );

    spawn_watcher(path, paused, ignore_sensitive, max_items);

    let initial_shortcut = cfg.shortcut.clone();
    let want_autostart = cfg.autostart;

    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
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
            commands::get_item_image,
            commands::pin,
            commands::remove,
            commands::toggle_pause,
            commands::hide_window,
            commands::paste_item,
            commands::get_settings,
            commands::set_settings,
            commands::clear_history,
            commands::open_settings,
            commands::app_version,
            commands::check_update,
            commands::install_update,
        ])
        .setup(move |app| {
            use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

            // Register the configured hotkey (fall back to the default if the
            // stored string fails to parse).
            let shortcut =
                Shortcut::from_str(&initial_shortcut).unwrap_or_else(|_| hotkey::toggle_shortcut());
            app.global_shortcut().register(shortcut)?;

            // Reconcile OS autostart state with the saved preference.
            {
                use tauri_plugin_autostart::ManagerExt;
                let al = app.autolaunch();
                let res = if want_autostart {
                    al.enable()
                } else {
                    al.disable()
                };
                if let Err(e) = res {
                    log::warn!("autostart reconcile failed: {e}");
                }
            }

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
