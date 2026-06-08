//! End-to-end tests covering the full clipboard lifecycle through the public
//! core API, exercising every casuistic the spec calls out:
//! capture of all four kinds, dedup, privacy filtering, fuzzy search, pinning,
//! pruning (favorites survive), deletion, and on-disk persistence.

use betterclipmaster_core::model::{ClipKind, NewItem};
use betterclipmaster_core::watcher::{Capture, MockSource, Watcher};
use betterclipmaster_core::{rank, Store};

#[test]
fn full_lifecycle_all_kinds() {
    let store = Store::open_in_memory().unwrap();

    // Capture every supported kind plus a sensitive item and a duplicate.
    let mut source = MockSource::new(vec![
        Capture::text("https://github.com/jarsbinksjar/betterclipmaster"),
        Capture::text("config.env DB_URL=postgres://localhost"),
        Capture {
            kind: ClipKind::Rich,
            content: "<b>negrita</b> y <i>cursiva</i>".to_string(),
            blob: None,
            formats: vec!["CF_HTML".to_string()],
        },
        Capture::image(vec![0x89, 0x50, 0x4e, 0x47, 1, 2, 3], "captura 800x600"),
        Capture {
            kind: ClipKind::Files,
            content: "C:\\proyecto\\config.env\nC:\\proyecto\\main.rs".to_string(),
            blob: None,
            formats: vec!["CF_HDROP".to_string()],
        },
        Capture::sensitive("super-secret-password"),
        Capture::text("https://github.com/jarsbinksjar/betterclipmaster"), // re-copy -> bump
    ]);

    // Drain every capture.
    for _ in 0..7 {
        Watcher::poll_once(&mut source, &store).unwrap();
    }

    // Sensitive content was never stored; re-copy did not create a duplicate.
    let all = store.all().unwrap();
    assert_eq!(all.len(), 5, "5 distinct non-sensitive items expected");
    assert!(!all
        .iter()
        .any(|i| i.content.contains("super-secret-password")));

    // The re-copied URL was bumped to the most-recent position.
    let recent = store.recent(10).unwrap();
    assert_eq!(
        recent[0].content,
        "https://github.com/jarsbinksjar/betterclipmaster"
    );

    // All four kinds are represented.
    for kind in [
        ClipKind::Text,
        ClipKind::Rich,
        ClipKind::Image,
        ClipKind::Files,
    ] {
        assert!(all.iter().any(|i| i.kind == kind), "missing kind {kind:?}");
    }

    // The image kept its blob.
    let image = all.iter().find(|i| i.kind == ClipKind::Image).unwrap();
    assert_eq!(
        image.blob.as_deref(),
        Some([0x89, 0x50, 0x4e, 0x47, 1, 2, 3].as_slice())
    );

    // Fuzzy search finds the config items (text + files both mention config).
    let hits = rank(&all, "config");
    assert!(hits.iter().any(|i| i.content.contains("config.env")));
    assert!(rank(&all, "zzzzznotfound").is_empty());
}

#[test]
fn pinned_item_survives_prune() {
    let store = Store::open_in_memory().unwrap();
    let keep = store.insert(&NewItem::text("KEEP ME")).unwrap().unwrap();
    store.insert(&NewItem::text("junk 1")).unwrap();
    store.insert(&NewItem::text("junk 2")).unwrap();
    store.insert(&NewItem::text("junk 3")).unwrap();
    store.set_pinned(keep, true).unwrap();

    store.prune(1).unwrap(); // keep only 1 non-pinned

    let all = store.all().unwrap();
    assert!(all.iter().any(|i| i.id == keep && i.pinned));
    assert_eq!(all.iter().filter(|i| !i.pinned).count(), 1);
    // Pinned items float to the top.
    assert_eq!(all[0].id, keep);
}

#[test]
fn delete_removes_only_target() {
    let store = Store::open_in_memory().unwrap();
    let a = store.insert(&NewItem::text("a")).unwrap().unwrap();
    let b = store.insert(&NewItem::text("b")).unwrap().unwrap();
    store.delete(a).unwrap();
    assert!(store.get(a).unwrap().is_none());
    assert!(store.get(b).unwrap().is_some());
}

#[test]
fn data_persists_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("clips.db");

    {
        let store = Store::open(&path).unwrap();
        store.insert(&NewItem::text("persisted line")).unwrap();
        let id = store.insert(&NewItem::text("pin this")).unwrap().unwrap();
        store.set_pinned(id, true).unwrap();
    } // store dropped, connection closed

    // Reopen the same file: data and pin state survive.
    let store = Store::open(&path).unwrap();
    let all = store.all().unwrap();
    assert_eq!(all.len(), 2);
    assert!(all.iter().any(|i| i.content == "persisted line"));
    assert!(all.iter().any(|i| i.content == "pin this" && i.pinned));
}
