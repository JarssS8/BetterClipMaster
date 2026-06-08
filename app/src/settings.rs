//! User-configurable settings, persisted as JSON in the per-user config dir.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Default global toggle shortcut.
pub const DEFAULT_SHORTCUT: &str = "Ctrl+Shift+V";
/// Default history cap (non-pinned items).
pub const DEFAULT_MAX_ITEMS: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Global shortcut string, e.g. "Ctrl+Shift+V" (parsed by global-shortcut).
    pub shortcut: String,
    /// Start the app automatically at login.
    pub autostart: bool,
    /// Maximum non-pinned items kept in history.
    pub max_items: usize,
    /// Skip clipboard content marked sensitive by password managers.
    pub ignore_sensitive: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            shortcut: DEFAULT_SHORTCUT.to_string(),
            autostart: false,
            max_items: DEFAULT_MAX_ITEMS,
            ignore_sensitive: true,
        }
    }
}

impl Settings {
    /// Clamp/repair invalid values to safe defaults.
    pub fn sanitized(mut self) -> Settings {
        if self.shortcut.trim().is_empty() {
            self.shortcut = DEFAULT_SHORTCUT.to_string();
        }
        self.max_items = self.max_items.clamp(10, 100_000);
        self
    }
}

/// Path to `settings.json` in the per-user config directory.
pub fn settings_path() -> PathBuf {
    if let Some(dirs) = directories::ProjectDirs::from("com", "jars", "betterclipmaster") {
        let dir = dirs.config_dir();
        let _ = std::fs::create_dir_all(dir);
        dir.join("settings.json")
    } else {
        PathBuf::from("settings.json")
    }
}

/// Load settings, falling back to defaults if missing or unreadable.
pub fn load() -> Settings {
    match std::fs::read_to_string(settings_path()) {
        Ok(text) => serde_json::from_str::<Settings>(&text)
            .map(Settings::sanitized)
            .unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

/// Persist settings to disk.
pub fn save(settings: &Settings) -> Result<(), String> {
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(settings_path(), json).map_err(|e| e.to_string())
}
