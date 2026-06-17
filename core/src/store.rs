//! SQLite-backed persistence for clipboard history.
//!
//! Ordering uses a strictly-increasing logical millisecond timestamp
//! (`created_at`) so that re-copying an existing item moves it to the top
//! deterministically, even within the same wall-clock millisecond.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

use crate::model::{ClipItem, ClipKind, NewItem};
use crate::Result;

/// Handle to the clipboard history database.
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open (creating if needed) a store at the given path.
    pub fn open(path: impl AsRef<Path>) -> Result<Store> {
        let conn = Connection::open(path)?;
        Self::from_conn(conn)
    }

    /// Open an ephemeral in-memory store (used in tests).
    pub fn open_in_memory() -> Result<Store> {
        let conn = Connection::open_in_memory()?;
        Self::from_conn(conn)
    }

    fn from_conn(conn: Connection) -> Result<Store> {
        conn.pragma_update(None, "journal_mode", "WAL").ok();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS clips (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                kind        TEXT    NOT NULL,
                content     TEXT    NOT NULL,
                blob        BLOB,
                preview     TEXT    NOT NULL,
                pinned      INTEGER NOT NULL DEFAULT 0,
                created_at  INTEGER NOT NULL,
                hash        TEXT    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_clips_created ON clips(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_clips_hash ON clips(hash);",
        )?;
        Ok(Store { conn })
    }

    /// Insert a captured item.
    ///
    /// - If it is identical to the current most-recent item, returns `None`
    ///   (ignored as a consecutive duplicate).
    /// - If an identical item exists elsewhere, that row is bumped to the top
    ///   (no duplicate row) and its id is returned.
    /// - Otherwise a new row is inserted and its id returned.
    ///
    /// All reads and the write run inside a single IMMEDIATE transaction to
    /// eliminate the race window between the duplicate check and the insert.
    pub fn insert(&self, item: &NewItem) -> Result<Option<i64>> {
        let hash = item.hash();

        let tx = self.conn.unchecked_transaction()?;

        // Consecutive duplicate of the most recent item -> ignore.
        let most_recent: Option<String> = tx
            .query_row(
                "SELECT hash FROM clips ORDER BY created_at DESC, id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if most_recent.as_deref() == Some(hash.as_str()) {
            tx.rollback()?;
            return Ok(None);
        }

        let ts = {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            let max: i64 = tx.query_row(
                "SELECT COALESCE(MAX(created_at), 0) FROM clips",
                [],
                |r| r.get(0),
            )?;
            now.max(max + 1)
        };

        // Existing-but-not-most-recent -> bump to top.
        let existing: Option<i64> = tx
            .query_row(
                "SELECT id FROM clips WHERE hash = ?1 LIMIT 1",
                params![hash],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            tx.execute(
                "UPDATE clips SET created_at = ?1 WHERE id = ?2",
                params![ts, id],
            )?;
            tx.commit()?;
            return Ok(Some(id));
        }

        tx.execute(
            "INSERT INTO clips (kind, content, blob, preview, pinned, created_at, hash)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)",
            params![
                item.kind.as_str(),
                item.content,
                item.blob,
                item.preview(),
                ts,
                hash
            ],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(Some(id))
    }

    /// Most recent items first (pinned float to the top), limited.
    pub fn recent(&self, limit: usize) -> Result<Vec<ClipItem>> {
        self.query(
            "SELECT id, kind, content, blob, preview, pinned, created_at, hash
             FROM clips ORDER BY pinned DESC, created_at DESC LIMIT ?1",
            params![limit as i64],
        )
    }

    /// All items, pinned first then most recent.
    pub fn all(&self) -> Result<Vec<ClipItem>> {
        self.query(
            "SELECT id, kind, content, blob, preview, pinned, created_at, hash
             FROM clips ORDER BY pinned DESC, created_at DESC",
            params![],
        )
    }

    /// Fetch a single item by id.
    pub fn get(&self, id: i64) -> Result<Option<ClipItem>> {
        let mut rows = self.query(
            "SELECT id, kind, content, blob, preview, pinned, created_at, hash
             FROM clips WHERE id = ?1",
            params![id],
        )?;
        Ok(rows.pop())
    }

    /// Pin or unpin an item.
    pub fn set_pinned(&self, id: i64, pinned: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE clips SET pinned = ?1 WHERE id = ?2",
            params![pinned as i64, id],
        )?;
        Ok(())
    }

    /// Delete a single item.
    pub fn delete(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM clips WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Remove everything.
    pub fn clear(&self) -> Result<()> {
        self.conn.execute("DELETE FROM clips", [])?;
        Ok(())
    }

    /// Keep at most `max_items` non-pinned items (most recent). Pinned items are
    /// never deleted.
    pub fn prune(&self, max_items: usize) -> Result<usize> {
        let deleted = self.conn.execute(
            "DELETE FROM clips
             WHERE pinned = 0
               AND id NOT IN (
                 SELECT id FROM clips WHERE pinned = 0
                 ORDER BY created_at DESC LIMIT ?1
               )",
            params![max_items as i64],
        )?;
        Ok(deleted)
    }

    /// Count of stored items.
    pub fn count(&self) -> Result<usize> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM clips", [], |r| r.get(0))?;
        Ok(n as usize)
    }

    fn query(&self, sql: &str, params: impl rusqlite::Params) -> Result<Vec<ClipItem>> {
        let mut stmt = self.conn.prepare_cached(sql)?;
        let rows = stmt.query_map(params, |row| {
            let kind_s: String = row.get(1)?;
            let pinned_i: i64 = row.get(5)?;
            Ok(ClipItem {
                id: row.get(0)?,
                kind: ClipKind::parse_str(&kind_s),
                content: row.get(2)?,
                blob: row.get(3)?,
                preview: row.get(4)?,
                pinned: pinned_i != 0,
                created_at: row.get(6)?,
                hash: row.get(7)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_recent() {
        let s = Store::open_in_memory().unwrap();
        let id = s.insert(&NewItem::text("hello")).unwrap();
        assert!(id.is_some());
        let recent = s.recent(10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].content, "hello");
    }

    #[test]
    fn consecutive_duplicate_is_ignored() {
        let s = Store::open_in_memory().unwrap();
        assert!(s.insert(&NewItem::text("dup")).unwrap().is_some());
        assert!(s.insert(&NewItem::text("dup")).unwrap().is_none());
        assert_eq!(s.count().unwrap(), 1);
    }

    #[test]
    fn reinsert_bumps_existing_to_top_without_duplicate() {
        let s = Store::open_in_memory().unwrap();
        s.insert(&NewItem::text("A")).unwrap();
        s.insert(&NewItem::text("B")).unwrap();
        let third = s.insert(&NewItem::text("A")).unwrap();
        assert!(third.is_some());
        assert_eq!(s.count().unwrap(), 2); // no duplicate A row
        let recent = s.recent(10).unwrap();
        assert_eq!(recent[0].content, "A"); // A bumped to top
    }

    #[test]
    fn pin_and_prune_keeps_pinned() {
        let s = Store::open_in_memory().unwrap();
        let a = s.insert(&NewItem::text("A")).unwrap().unwrap();
        s.insert(&NewItem::text("B")).unwrap();
        s.insert(&NewItem::text("C")).unwrap();
        s.insert(&NewItem::text("D")).unwrap();
        s.set_pinned(a, true).unwrap();

        let deleted = s.prune(2).unwrap();
        // 4 items, 1 pinned (A). Keep A + 2 most-recent non-pinned (D, C). Delete B.
        assert_eq!(deleted, 1);
        let all = s.all().unwrap();
        assert_eq!(all.len(), 3);
        assert!(all.iter().any(|i| i.content == "A" && i.pinned));
        assert!(all.iter().any(|i| i.content == "C"));
        assert!(all.iter().any(|i| i.content == "D"));
        assert!(!all.iter().any(|i| i.content == "B"));
    }

    #[test]
    fn delete_removes_item() {
        let s = Store::open_in_memory().unwrap();
        let id = s.insert(&NewItem::text("x")).unwrap().unwrap();
        s.delete(id).unwrap();
        assert_eq!(s.count().unwrap(), 0);
    }

    #[test]
    fn get_returns_item() {
        let s = Store::open_in_memory().unwrap();
        let id = s.insert(&NewItem::text("findme")).unwrap().unwrap();
        let got = s.get(id).unwrap().unwrap();
        assert_eq!(got.content, "findme");
        assert!(s.get(99999).unwrap().is_none());
    }
}
