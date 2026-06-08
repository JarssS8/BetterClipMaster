//! Core logic for godclipboard.
//!
//! This crate is platform-independent and contains no GUI, hotkey or OS clipboard
//! code. All business logic (model, persistence, search, privacy, and the watcher
//! loop) lives here so it can be tested end-to-end without a display server.
//!
//! The OS-specific shell (Tauri app, global hotkey, tray, paste) lives in the
//! `app` crate and depends on this one.

pub mod model;
pub mod privacy;
pub mod search;
pub mod store;
pub mod watcher;

pub use model::{ClipItem, ClipKind, NewItem};
pub use search::rank;
pub use store::Store;
pub use watcher::{Capture, ClipboardSource, Watcher};

/// Errors that can occur in the core layer.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("image error: {0}")]
    Image(#[from] image::ImageError),

    #[error("{0}")]
    Other(String),
}

/// Convenience result type for the core layer.
pub type Result<T> = std::result::Result<T, Error>;
