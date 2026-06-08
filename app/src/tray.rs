//! System tray icon and menu: Open, Pause/Resume capture, Quit.

use std::sync::atomic::Ordering;

use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{App, Manager};

use crate::commands::AppState;
use crate::hotkey;

/// Build and attach the tray icon. Called once during setup.
pub fn build(app: &App) -> tauri::Result<()> {
    let open = MenuItemBuilder::with_id("open", "Abrir").build(app)?;
    let pause = MenuItemBuilder::with_id("pause", "Pausar captura").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Salir").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&open, &pause, &quit])
        .build()?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("default window icon".into()))?;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("BetterClipMaster")
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open" => hotkey::toggle_overlay(app),
            "pause" => {
                let state = app.state::<AppState>();
                let now = !state.paused.load(Ordering::SeqCst);
                state.paused.store(now, Ordering::SeqCst);
                let label = if now {
                    "Reanudar captura"
                } else {
                    "Pausar captura"
                };
                let _ = pause.set_text(label);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}
