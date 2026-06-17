# Performance Overhaul Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate resource accumulation bugs and optimize hot paths so BetterClipMaster stays lightweight after days of use.

**Architecture:** Six independent fix sets across the JS renderer, Tauri commands, clipboard watcher, and SQLite store. The clipboard capture bug (macOS changeCount) was already fixed in a prior commit. Each task can be reviewed and committed independently.

**Tech Stack:** Rust 2021 + Tauri 2.x, Vanilla JS (no bundler), SQLite via `rusqlite`, `nucleo-matcher` for fuzzy search, `arboard` for clipboard, `image` crate for PNG encoding.

## Global Constraints

- No new runtime JS dependencies — plain DOM APIs only.
- Rust: no new crates beyond what's already in `app/Cargo.toml` / `core/Cargo.toml`.
- All Rust tests live in `#[cfg(test)]` modules in the same file as the code under test.
- Build command (run from workspace root): `cargo build -p betterclipmaster` (app) or `cargo test -p betterclipmaster-core` (core tests).
- `LIST_LIMIT` constant lives in `app/src/commands.rs:16`.
- `POLL_INTERVAL` constant lives in `app/src/lib.rs:28`.

---

### Task 1: JS — Debounce search + event delegation

**Files:**
- Modify: `app/ui/app.js`

**Interfaces:**
- Produces: `listEl` has exactly 2 event listeners (one click, one dblclick) at all times regardless of render count. Search IPC fires at most once per 150ms.

**What and why:** Every keystroke currently fires a full IPC round-trip to Rust AND rebuilds the DOM with 2 new listeners per row. After hours of typing, thousands of orphaned listeners accumulate in the renderer. Fix: debounce the input + move listeners to the parent element (event delegation).

- [ ] **Step 1: Add debounce and replace per-item listeners with delegation in `app/ui/app.js`**

Replace the entire file content from line 1 to end with the following. The only behavioral change visible to the user: search responds after 150ms idle rather than instantly (imperceptible).

Key changes:
1. Add `debounce()` helper at the top.
2. Wrap the `queryEl` input handler with `debounce(refresh, 150)`.
3. Remove the two `li.addEventListener` calls inside `render()`.
4. Add two delegated listeners on `listEl` after the DOM is defined.

```javascript
// Overlay logic: query the core, render the Alfred-style list + preview, and
// handle keyboard navigation. Uses the global Tauri API (withGlobalTauri).

const invoke = window.__TAURI__.core.invoke;
const listen = window.__TAURI__.event.listen;

const queryEl = document.getElementById("query");
const listEl = document.getElementById("list");
const previewEl = document.getElementById("preview");

let items = [];
let selected = 0;

function debounce(fn, ms) {
  let t;
  return (...args) => {
    clearTimeout(t);
    t = setTimeout(() => fn(...args), ms);
  };
}

function escapeHtml(s) {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

async function refresh() {
  try {
    items = await invoke("list", { query: queryEl.value });
  } catch (e) {
    items = [];
    console.error("list failed", e);
  }
  if (selected >= items.length) selected = Math.max(0, items.length - 1);
  render();
}

function render() {
  listEl.innerHTML = "";
  if (items.length === 0) {
    listEl.innerHTML = '<li class="empty">Sin resultados</li>';
    previewEl.innerHTML = "";
    return;
  }
  items.forEach((item, i) => {
    const li = document.createElement("li");
    li.className = "list-row" + (i === selected ? " selected" : "");
    li.dataset.index = i;
    const pin = item.pinned ? '<span class="pin">★</span>' : "";
    li.innerHTML =
      `<span class="kind-dot ${item.kind}"></span>` +
      `<span class="row-text">${escapeHtml(item.preview)}</span>` +
      pin +
      `<span class="tag">${item.kind}</span>`;
    listEl.appendChild(li);
  });
  renderPreview(items[selected]);
  const sel = listEl.querySelector(".selected");
  if (sel) sel.scrollIntoView({ block: "nearest" });
}

async function renderPreview(item) {
  if (!item) {
    previewEl.innerHTML = "";
    return;
  }
  if (item.kind === "image") {
    try {
      const dataurl = await invoke("get_item_image", { id: item.id });
      if (dataurl) {
        previewEl.innerHTML = `<img src="${dataurl}" alt="imagen" />`;
      } else {
        previewEl.innerHTML = "";
      }
    } catch (e) {
      previewEl.innerHTML = "";
    }
    return;
  }
  if (item.kind === "files") {
    const rows = item.content
      .split("\n")
      .filter((p) => p.length)
      .map((p) => `<li>${escapeHtml(p)}</li>`)
      .join("");
    previewEl.innerHTML = `<ul class="files">${rows}</ul>`;
  } else {
    previewEl.innerHTML = `<div class="ptext">${escapeHtml(item.content)}</div>`;
  }
}

async function paste(id) {
  try {
    await invoke("paste_item", { id });
  } catch (e) {
    console.error("paste failed", e);
  }
}

async function togglePin(item) {
  if (!item) return;
  try {
    await invoke("pin", { id: item.id, pinned: !item.pinned });
    await refresh();
  } catch (e) {
    console.error("pin failed", e);
  }
}

async function remove(item) {
  if (!item) return;
  try {
    await invoke("remove", { id: item.id });
    await refresh();
  } catch (e) {
    console.error("remove failed", e);
  }
}

function move(delta) {
  if (items.length === 0) return;
  selected = (selected + delta + items.length) % items.length;
  render();
}

// Single delegated click listener — no per-row listeners.
listEl.addEventListener("click", (e) => {
  const li = e.target.closest("li[data-index]");
  if (!li) return;
  selected = Number(li.dataset.index);
  render();
});

listEl.addEventListener("dblclick", (e) => {
  const li = e.target.closest("li[data-index]");
  if (!li) return;
  paste(items[Number(li.dataset.index)].id);
});

queryEl.addEventListener("input", debounce(() => {
  selected = 0;
  refresh();
}, 150));

document.addEventListener("keydown", (e) => {
  switch (e.key) {
    case "ArrowDown":
      e.preventDefault();
      move(1);
      break;
    case "ArrowUp":
      e.preventDefault();
      move(-1);
      break;
    case "Enter":
      e.preventDefault();
      if (items[selected]) paste(items[selected].id);
      break;
    case "Escape":
      e.preventDefault();
      invoke("hide_window");
      break;
    case "Delete":
      e.preventDefault();
      remove(items[selected]);
      break;
    case "p":
    case "P":
      if (e.ctrlKey || e.metaKey) {
        e.preventDefault();
        togglePin(items[selected]);
      }
      break;
  }
});

// Reset and focus whenever the overlay is opened via the hotkey.
listen("open", () => {
  queryEl.value = "";
  selected = 0;
  queryEl.focus();
  queryEl.select();
  refresh();
});

// Initial load (also covers the first show).
queryEl.focus();
refresh();
```

- [ ] **Step 2: Verify visually**

Open the app. Type in the search box rapidly. Open DevTools console → Elements → inspect `#list`. Confirm `#list` itself has 2 listeners (via `getEventListeners(document.getElementById('list'))` in DevTools) and individual `<li>` elements have 0. Typing multiple characters quickly should still trigger only one `list` IPC call after 150ms of idle.

- [ ] **Step 3: Commit**

```bash
git add app/ui/app.js
git commit -m "perf: debounce search (150ms) + event delegation on list"
```

---

### Task 2: Rust — Add `get_item_image` command + remove `dataurl` from `ItemDto`

**Files:**
- Modify: `app/src/commands.rs`

**Interfaces:**
- Consumes: `Store::get(id)` → `Option<ClipItem>`, already available.
- Produces:
  - `get_item_image(state, id: i64) -> Result<Option<String>, String>` — returns `Some("data:image/png;base64,...")` or `None`.
  - `ItemDto.dataurl` field removed. Task 1 JS already calls `get_item_image` for preview.
  - `LIST_LIMIT` changed from `1000` to `300`.

**What and why:** Currently `list` base64-encodes every image blob in history and sends it over IPC on every keystroke. With 1000 items this can be 100 MB+ per IPC call. Fix: strip images from the list payload entirely and add a dedicated command that fetches one image by id on demand (called only when rendering the preview for a selected item).

- [ ] **Step 1: Add `get_item_image` command and remove `dataurl` from `ItemDto` in `app/src/commands.rs`**

Change `LIST_LIMIT` at line 16:
```rust
const LIST_LIMIT: usize = 300;
```

Remove `dataurl` from `ItemDto`:
```rust
#[derive(Serialize)]
pub struct ItemDto {
    pub id: i64,
    pub kind: String,
    pub preview: String,
    pub content: String,
    pub pinned: bool,
}
```

Update `From<&ClipItem> for ItemDto` (remove the `dataurl` logic):
```rust
impl From<&ClipItem> for ItemDto {
    fn from(item: &ClipItem) -> ItemDto {
        ItemDto {
            id: item.id,
            kind: item.kind.as_str().to_string(),
            preview: item.preview.clone(),
            content: item.content.clone(),
            pinned: item.pinned,
        }
    }
}
```

Add the new command after `remove`:
```rust
/// Return the base64 data URL for a single image item, or None if the item
/// is not found or has no image blob.
#[tauri::command]
pub fn get_item_image(state: State<AppState>, id: i64) -> Result<Option<String>, String> {
    let store = state.store.lock().map_err(map_err)?;
    let item = store.get(id).map_err(map_err)?;
    let dataurl = item.and_then(|it| {
        it.blob.map(|bytes| {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            format!("data:image/png;base64,{b64}")
        })
    });
    Ok(dataurl)
}
```

Register it in `app/src/lib.rs` inside `tauri::generate_handler![]` — add `commands::get_item_image` to the list:
```rust
.invoke_handler(tauri::generate_handler![
    commands::list,
    commands::get_item_image,   // <-- add this line
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
```

- [ ] **Step 2: Build**

```bash
cargo build -p betterclipmaster
```
Expected: compiles with no errors. The `base64` import is already in scope.

- [ ] **Step 3: Verify visually**

Open app. Navigate to an image item. Preview panel should still show the image (fetched on demand). List load should be noticeably faster/lighter. Check Network in DevTools — IPC `list` responses should be tiny.

- [ ] **Step 4: Commit**

```bash
git add app/src/commands.rs app/src/lib.rs
git commit -m "perf: lazy image loading — get_item_image command, LIST_LIMIT 300"
```

---

### Task 3: Rust — Hash before PNG encode in clipboard watcher

**Files:**
- Modify: `app/src/clipboard_source.rs`

**Interfaces:**
- No interface change. `OsClipboard` implements `ClipboardSource` unchanged.

**What and why:** Currently `read_current()` encodes raw RGBA to PNG unconditionally, then `read()` checks the hash. When a screenshot sits on the clipboard, this encodes potentially megabytes of image data on every poll (2×/sec) even though nothing changed. Fix: hash the raw RGBA bytes first; only encode if the image is new.

- [ ] **Step 1: Restructure `read_current` to hash before encode**

In `app/src/clipboard_source.rs`, replace the `read_current` method:

```rust
fn read_current(&mut self) -> Option<Capture> {
    // Text first: it is the common case and cheapest.
    if let Ok(text) = self.clip.get_text() {
        if !text.is_empty() {
            return Some(Capture {
                kind: ClipKind::Text,
                content: text,
                blob: None,
                formats: current_formats(),
            });
        }
    }
    // Image: hash raw bytes first, only encode PNG if the image is new.
    if let Ok(img) = self.clip.get_image() {
        use betterclipmaster_core::model::compute_hash;
        let raw_hash = compute_hash(ClipKind::Image, "", Some(img.bytes.as_ref()));
        if self.last_hash.as_deref() == Some(raw_hash.as_str()) {
            return None; // same image, skip expensive PNG encode
        }
        let label = format!("Imagen {}x{}", img.width, img.height);
        if let Some(png) = Self::encode_png(&img) {
            return Some(Capture {
                kind: ClipKind::Image,
                content: label,
                blob: Some(png),
                formats: current_formats(),
            });
        }
    }
    None
}
```

Note: the outer `read()` will still compute the final hash over the encoded PNG bytes and update `last_hash`. The raw-bytes pre-check is purely an early-exit optimization — it uses the same `compute_hash` function so the types are consistent.

- [ ] **Step 2: Build**

```bash
cargo build -p betterclipmaster
```
Expected: compiles with no errors.

- [ ] **Step 3: Commit**

```bash
git add app/src/clipboard_source.rs
git commit -m "perf: hash raw image bytes before PNG encode to skip redundant work"
```

---

### Task 4: Rust — Poll interval 1000ms + periodic prune

**Files:**
- Modify: `app/src/lib.rs`

**Interfaces:**
- No interface change.

**What and why:** Poll at 500ms burns unnecessary CPU. 1000ms is perceptually instantaneous for clipboard capture. Separately, `prune()` only runs when a new item is inserted — if the clipboard is unchanged for a long time, old items accumulate. Fix: run prune every 60 iterations (~1 minute) unconditionally.

- [ ] **Step 1: Change poll interval and add periodic prune counter**

In `app/src/lib.rs`, change the constant at line 28:
```rust
const POLL_INTERVAL: Duration = Duration::from_millis(1000);
```

In `spawn_watcher`, add a counter to the loop:

```rust
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
        let mut poll_count: u32 = 0;
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
                poll_count += 1;
                if poll_count % 60 == 0 {
                    let _ = store.prune(max_items.load(Ordering::SeqCst));
                }
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    });
}
```

- [ ] **Step 2: Build**

```bash
cargo build -p betterclipmaster
```
Expected: compiles with no errors.

- [ ] **Step 3: Commit**

```bash
git add app/src/lib.rs
git commit -m "perf: poll interval 1000ms, periodic prune every 60 polls"
```

---

### Task 5: Rust — SQLite `prepare_cached` + transaction in `insert()`

**Files:**
- Modify: `core/src/store.rs`

**Interfaces:**
- No interface change. All `Store` public methods have the same signatures.

**What and why:** `query()` calls `conn.prepare(sql)` on every call, recompiling the SQL statement each time. `rusqlite` provides `prepare_cached` which caches the compiled statement keyed by SQL string. Also, `insert()` runs 3 separate queries without a transaction, creating a race window and multiple fsyncs. Wrapping in a transaction closes the race and collapses fsyncs to one.

- [ ] **Step 1: Replace `prepare` with `prepare_cached` in `query()`**

In `core/src/store.rs`, change the `query` method at line 196:

```rust
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
```

- [ ] **Step 2: Wrap `insert()` in a transaction**

Replace the `insert` method body (lines 73–121) in `core/src/store.rs`:

```rust
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
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
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
```

Note: `next_ts()` is inlined into the transaction body so the timestamp query runs inside the same transaction. The private `next_ts` method can be removed or left unused — leaving it is fine, removing it avoids a dead_code warning.

If removing `next_ts`, delete lines 53–64 in `core/src/store.rs` (the `fn next_ts` block).

- [ ] **Step 3: Run core tests**

```bash
cargo test -p betterclipmaster-core
```
Expected: all tests in `store.rs` pass — `insert_and_recent`, `consecutive_duplicate_is_ignored`, `reinsert_bumps_existing_to_top_without_duplicate`, `pin_and_prune_keeps_pinned`, `delete_removes_item`, `get_returns_item`.

- [ ] **Step 4: Build**

```bash
cargo build -p betterclipmaster
```
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add core/src/store.rs
git commit -m "perf: prepare_cached for SQLite statements, transaction in insert()"
```

---

### Task 6: Rust — `rank()` haystack allocation reduction

**Files:**
- Modify: `core/src/search.rs`

**Interfaces:**
- `pub fn rank(items: &[ClipItem], query: &str) -> Vec<ClipItem>` — signature unchanged.

**What and why:** `haystack()` allocates a new `String` via `format!()` for every item even when `content` equals `preview`. Using `Cow<str>` lets us borrow when only one field is needed and allocate only when both fields must be joined.

- [ ] **Step 1: Replace `haystack` with a `Cow`-based version**

In `core/src/search.rs`, replace the `haystack` function and its use in `rank`:

```rust
use std::borrow::Cow;

fn haystack(item: &ClipItem) -> Cow<str> {
    if item.content.is_empty() || item.content == item.preview {
        Cow::Borrowed(&item.preview)
    } else {
        Cow::Owned(format!("{} {}", item.preview, item.content))
    }
}
```

The rest of `rank()` uses `haystack(item)` — `Utf32Str::new` accepts `&str`, and `Cow<str>` derefs to `&str`, so no other change is needed.

- [ ] **Step 2: Run core tests**

```bash
cargo test -p betterclipmaster-core
```
Expected: all tests in `search.rs` pass — `empty_query_returns_all_in_order`, `ranks_better_match_first`, `no_match_returns_empty`, `matches_are_case_insensitive`.

- [ ] **Step 3: Commit**

```bash
git add core/src/search.rs
git commit -m "perf: haystack uses Cow to avoid allocation when content == preview"
```

---

## Self-Review

**Spec coverage check:**

| Spec item | Task |
|-----------|------|
| JS debounce 150ms | Task 1 |
| Event delegation | Task 1 |
| Lazy image loading | Task 1 (JS) + Task 2 (Rust) |
| `get_item_image` command | Task 2 |
| `LIST_LIMIT` 1000→300 | Task 2 |
| Hash before PNG encode | Task 3 |
| Poll interval 1000ms | Task 4 |
| Periodic prune | Task 4 |
| `prepare_cached` | Task 5 |
| Transaction in `insert()` | Task 5 |
| `rank()` haystack alloc | Task 6 |
| macOS changeCount capture bug | Done (prior commit) |

All spec items covered. No gaps.

**Placeholder scan:** No TBDs, all code blocks complete.

**Type consistency:**
- `get_item_image` defined in Task 2 (`app/src/commands.rs`) and called in Task 1 JS (`invoke("get_item_image", { id: item.id })`). ✓
- `ItemDto` loses `dataurl` in Task 2; Task 1 JS never accesses `item.dataurl`. ✓
- `Cow<str>` used in Task 6; `Utf32Str::new` takes `&str`, `Cow` derefs correctly. ✓
- `unchecked_transaction()` available in `rusqlite` (already a dependency). ✓
