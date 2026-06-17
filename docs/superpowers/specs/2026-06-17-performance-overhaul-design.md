# Performance Overhaul — Design Spec
**Date:** 2026-06-17  
**Status:** Approved

## Problem

After a few days of use BetterClipMaster becomes slow and consumes excessive RAM/CPU. Root causes identified via static analysis:

| # | Location | Issue | Severity |
|---|----------|-------|----------|
| 1 | `app/ui/app.js` | No debounce: every keystroke fires a full IPC round-trip to Rust | CRITICAL |
| 2 | `app/ui/app.js` | Event listeners accumulate: 2 new listeners per list item per render, never cleaned up | CRITICAL |
| 3 | `app/src/commands.rs` | `list` base64-encodes ALL image blobs on every keystroke (~100 MB+ IPC payload) | HIGH |
| 4 | `app/src/clipboard_source.rs` | PNG encode runs before hash check: large images re-encoded 2×/sec even when unchanged | HIGH |
| 5 | `core/src/search.rs` | `rank()` allocates 3 vectors + 1 String per item per search call | HIGH |
| 6 | `core/src/store.rs` | `prepare()` recompiles SQL statement on every query, never cached | MEDIUM |
| 7 | `core/src/store.rs` | `insert()` runs 3 separate queries with no transaction | MEDIUM |
| 8 | `app/src/lib.rs` | `prune()` only runs on successful insert; DB can grow unbounded during idle periods | MEDIUM |
| 9 | `app/src/lib.rs` | Poll interval 500ms — unnecessary CPU load; 1000ms is perceptually identical | LOW |
| 10 | `app/src/commands.rs` | `LIST_LIMIT = 1000` — list is unusable at that size, causes excess alloc and IPC | LOW |

## Approach

Full performance overhaul (Approach B): surgical fixes to every identified issue. No architectural rewrites, no behavior changes visible to the user.

---

## Section 1 — JavaScript (`app/ui/app.js`)

### 1a. Debounce search input
- Add 150ms debounce on the `input` event before calling `refresh()`.
- Eliminates IPC round-trips for intermediate keystrokes (e.g. typing "hello" fires 1 call instead of 5).

### 1b. Event delegation
- Remove per-`<li>` `click` and `dblclick` listeners.
- Add a single `click` and `dblclick` listener on `listEl`.
- Each handler reads `e.target.closest("li")?.dataset.index` to identify the item.
- Eliminates listener accumulation entirely regardless of how many renders occur.

### 1c. Lazy image loading
- Remove `dataurl` from `ItemDto` (Rust side).
- Add new Tauri command: `get_item_image(id: i64) -> Result<Option<String>, String>` that returns the base64 data URL for a single item.
- In `renderPreview()`: when `item.kind === "image"`, call `invoke("get_item_image", { id: item.id })` and set the `<img src>` asynchronously.
- Result: list IPC payload drops from ~100 MB to kilobytes.

---

## Section 2 — Clipboard polling (`app/src/clipboard_source.rs`, `app/src/lib.rs`)

### 2a. Hash before PNG encode
- In `OsClipboard::read_current()`: when an image is detected, compute a hash over the **raw RGBA bytes** before calling `encode_png`.
- Compare against `self.last_hash`. If unchanged, return `None` immediately.
- `encode_png` only runs when the image is actually new.
- Eliminates ~172,000 unnecessary PNG encodes per day when an image sits on the clipboard.

### 2b. Poll interval 500ms → 1000ms
- Change `POLL_INTERVAL` constant in `app/src/lib.rs` from 500ms to 1000ms.
- 1 second is perceptually instantaneous for clipboard capture. Halves background CPU.

---

## Section 3 — Commands + Search (`app/src/commands.rs`, `core/src/search.rs`)

### 3a. New command `get_item_image`
- `pub fn get_item_image(state: State<AppState>, id: i64) -> Result<Option<String>, String>`
- Fetches item by id, encodes blob to base64 data URL only for that item.
- Called only from JS `renderPreview()` for image items.

### 3b. `LIST_LIMIT` 1000 → 300
- 1000 items is not usable in the overlay UI. 300 covers all practical use.
- Reduces memory allocated in `recent()`, `rank()`, and the IPC serialization payload.

### 3c. `rank()` haystack allocation
- In `haystack(item)`: if `content` is empty, return `Cow::Borrowed(&item.preview)` instead of `format!()`.
- If content is non-empty, only allocate when the two strings differ — use `format!()` only then.
- Result: saves one `String` allocation per item on the hot path when content equals preview.

---

## Section 4 — SQLite (`core/src/store.rs`)

### 4a. `prepare_cached` 
- Replace all `self.conn.prepare(sql)?` with `self.conn.prepare_cached(sql)?`.
- `rusqlite`'s `prepare_cached` caches the compiled statement keyed by SQL string. Zero behavior change.

### 4b. Transaction in `insert()`
- Wrap the 3-query sequence (check recent hash, check existing, insert/update) in a single `IMMEDIATE` transaction.
- Eliminates the race window between read and write. Faster: one fsync instead of up to three.

### 4c. Periodic `prune()` independent of inserts
- In the watcher loop in `app/src/lib.rs`: add a counter; call `store.prune(max_items)` every 60 iterations (~60 seconds at 1000ms interval).
- Ensures DB size stays bounded even when clipboard is unchanged for long periods.

---

## Files Changed

| File | Changes |
|------|---------|
| `app/ui/app.js` | Debounce, event delegation, lazy image fetch |
| `app/src/commands.rs` | Add `get_item_image`, remove `dataurl` from `ItemDto`, `LIST_LIMIT` → 300 |
| `app/src/clipboard_source.rs` | Hash before PNG encode |
| `app/src/lib.rs` | `POLL_INTERVAL` → 1000ms, periodic prune counter |
| `core/src/search.rs` | Haystack avoids unnecessary allocation |
| `core/src/store.rs` | `prepare_cached`, transaction in `insert()` |

## Success Criteria

- No listener accumulation: `listEl` has exactly 2 listeners at all times regardless of how many renders have occurred.
- IPC payload for `list`: no image data transmitted, only text previews.
- PNG encode: only runs when a new image is copied, not on every poll.
- After 7 days of normal use: RAM and CPU usage remain at initial levels (no growth).

## Out of Scope

- List virtualization (rendering only visible rows): not needed after lazy images + debounce fix.
- Adaptive poll backoff: not needed at 1000ms.
- Statement preparation for `insert()`'s individual queries: covered by `prepare_cached`.
