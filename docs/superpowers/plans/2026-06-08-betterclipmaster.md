# godclipboard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clipboard manager nativo estilo Alfred 5 en Rust/Tauri, Windows primero, con historial persistente, búsqueda fuzzy, favoritos y atajo global.

**Architecture:** Cargo workspace de dos crates. `core` es una librería Rust pura (modelo, store SQLite, búsqueda fuzzy, privacidad, watcher con trait mockeable) — totalmente testeable en cualquier SO sin GUI. `app` es la shell Tauri (comandos, hotkey, bandeja, pegar, UI HTML) que cablea el core a la plataforma. Toda la lógica de negocio vive en `core` y se testea E2E; `app` es una capa fina específica de plataforma.

**Tech Stack:** Rust, Tauri v2, rusqlite (SQLite bundled), nucleo (fuzzy), serde, sha2, image, thiserror. UI en HTML/CSS/JS plano (sin bundler). GitHub Actions para CI y releases.

---

## File Structure

```
godclipboard/
├── Cargo.toml                  # workspace
├── .gitignore
├── README.md
├── core/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs              # re-exports + errores
│   │   ├── model.rs           # ClipItem, ClipKind, hashing
│   │   ├── privacy.rs         # detección de contenido sensible
│   │   ├── store.rs           # SQLite: insert/dedup/prune/pin/query/delete
│   │   ├── search.rs          # ranking fuzzy
│   │   └── watcher.rs         # ClipboardSource trait + Watcher + Mock
│   └── tests/
│       └── e2e.rs             # integración: todas las casuísticas
├── app/
│   ├── Cargo.toml
│   ├── build.rs
│   ├── tauri.conf.json
│   ├── src/
│   │   ├── main.rs            # arranque, estado, wiring
│   │   ├── commands.rs        # comandos Tauri (UI ↔ core)
│   │   ├── hotkey.rs          # atajo global
│   │   ├── tray.rs            # icono bandeja
│   │   └── paste.rs           # simular Ctrl+V
│   ├── icons/                 # iconos app/tray
│   └── ui/
│       ├── index.html         # overlay layout A
│       ├── style.css
│       └── app.js
├── .github/workflows/
│   ├── ci.yml                 # cargo test en push/PR
│   └── release.yml            # push a master → versión + build + GH Release
└── docs/
    ├── superpowers/specs/2026-06-08-godclipboard-design.md
    ├── superpowers/plans/2026-06-08-godclipboard.md
    └── ARCHITECTURE.md
```

Comandos de test del core (ejecutables en Linux/WSL):
`cargo test -p godclipboard-core`

---

## Task 0: Workspace scaffolding

**Files:**
- Create: `Cargo.toml`, `core/Cargo.toml`, `core/src/lib.rs`, `.gitignore` (ya existe, ampliar)

- [ ] **Step 1:** Crear `Cargo.toml` workspace con members `core` y `app`, resolver "2".
- [ ] **Step 2:** Crear `core/Cargo.toml` con deps: rusqlite (bundled), nucleo, serde+derive, serde_json, sha2, image, thiserror.
- [ ] **Step 3:** Crear `core/src/lib.rs` con módulos y un `Error` enum (thiserror).
- [ ] **Step 4:** `cargo build -p godclipboard-core` → compila.
- [ ] **Step 5:** Commit `chore: workspace scaffolding`.

## Task 1: Modelo `ClipItem` + hashing

**Files:** Create `core/src/model.rs`; Test en mismo archivo (`#[cfg(test)]`).

- [ ] **Step 1 (test):** `kind` serializa a string; `compute_hash` igual para mismo contenido, distinto para distinto.
- [ ] **Step 2:** Run `cargo test -p godclipboard-core model` → FAIL.
- [ ] **Step 3:** Implementar `ClipKind {Text,Rich,Image,Files}`, `ClipItem` (id, kind, content, blob, preview, pinned, created_at, hash), `compute_hash` (sha256 de kind+content+blob), `make_preview` (trunca a 120 chars, 1 línea).
- [ ] **Step 4:** Run test → PASS.
- [ ] **Step 5:** Commit `feat(core): ClipItem model + hashing`.

## Task 2: Privacidad

**Files:** Create `core/src/privacy.rs`.

- [ ] **Step 1 (test):** `is_sensitive` true cuando el set de formatos contiene "Clipboard Viewer Ignore" o "ExcludeClipboardContentFromMonitorProcessing"; false si no.
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Implementar `is_sensitive(formats: &[String]) -> bool` (case-insensitive, marcadores conocidos).
- [ ] **Step 4:** Run → PASS.
- [ ] **Step 5:** Commit `feat(core): sensitive content detection`.

## Task 3: Store SQLite

**Files:** Create `core/src/store.rs`.

API: `Store::open(path)`, `open_in_memory()`, `insert(NewItem) -> Option<i64>` (None si dup del último), `recent(limit)`, `all()`, `set_pinned(id,bool)`, `delete(id)`, `prune(max_items)`, `clear()`.

- [ ] **Step 1 (test):** open_in_memory + insert + recent devuelve 1 item.
- [ ] **Step 2:** dedup: insertar 2 veces el mismo hash consecutivo → segunda devuelve None, recent=1.
- [ ] **Step 3:** dedup no consecutivo: A,B,A → 3 inserts ok (re-inserta A actualizando created_at) — recent[0]=A.
- [ ] **Step 4:** set_pinned + prune: con max=2 y 4 items (1 pinned) → quedan 2 no-pinned recientes + el pinned nunca se borra.
- [ ] **Step 5:** delete por id.
- [ ] **Step 6:** Run tras cada uno → FAIL→impl→PASS. Implementar schema (tabla clips, índices created_at y hash), migraciones idempotentes, todas las funciones.
- [ ] **Step 7:** Commit `feat(core): SQLite store with dedup/prune/pin`.

## Task 4: Búsqueda fuzzy

**Files:** Create `core/src/search.rs`.

- [ ] **Step 1 (test):** `rank(items, "")` devuelve todos en orden original. `rank(items,"cfg")` pone "config.env" antes que "readme". Query sin match → vacío.
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Implementar `rank(&[ClipItem], query) -> Vec<ClipItem>` usando nucleo sobre `preview` (+content para text). Query vacía = passthrough.
- [ ] **Step 4:** Run → PASS.
- [ ] **Step 5:** Commit `feat(core): fuzzy search ranking`.

## Task 5: Watcher

**Files:** Create `core/src/watcher.rs`.

- [ ] **Step 1 (test):** `ClipboardSource` trait con `read() -> Option<Capture>` y `formats()`. `MockSource` con cola programable. `Watcher::poll_once(&store)` lee la fuente, ignora sensibles, inserta no-dups.
- [ ] **Step 2:** test: secuencia [text "a", text "a"(dup), sensitive, image] → store tiene a + image (2).
- [ ] **Step 3:** Run → FAIL.
- [ ] **Step 4:** Implementar trait, `Capture {kind,content,blob,formats}`, `MockSource`, `Watcher::poll_once` (usa privacy + store).
- [ ] **Step 5:** Run → PASS.
- [ ] **Step 6:** Commit `feat(core): clipboard watcher with mockable source`.

## Task 6: E2E del core

**Files:** Create `core/tests/e2e.rs`.

- [ ] **Step 1:** Test E2E que recorre el ciclo completo con MockSource: copiar texto/rich/imagen/archivo → poll → recent → buscar → fijar → prune → borrar → verificar invariantes (favorito sobrevive, dedup, orden por reciente, búsqueda filtra). DB en archivo temporal (tempfile) para probar persistencia: cerrar Store, reabrir, datos siguen.
- [ ] **Step 2:** Run `cargo test -p godclipboard-core` → todos PASS.
- [ ] **Step 3:** Commit `test(core): full e2e covering all casuistics`.

## Task 7: Tauri app — comandos y arranque

**Files:** Create `app/Cargo.toml`, `app/build.rs`, `app/tauri.conf.json`, `app/src/main.rs`, `app/src/commands.rs`, `app/icons/*`.

- [ ] **Step 1:** `app/Cargo.toml`: tauri v2, godclipboard-core (path), serde_json, global-hotkey, tray via tauri, enigo, arboard.
- [ ] **Step 2:** `tauri.conf.json`: ventana oculta, sin decoración, always-on-top, skipTaskbar, frontendDist `../ui`, sin beforeBuildCommand.
- [ ] **Step 3:** `commands.rs`: comandos `list(query)`, `pin(id,bool)`, `remove(id)`, `paste_item(id)` que llaman al core (Store en estado compartido `Mutex`).
- [ ] **Step 4:** `main.rs`: abrir Store en appdata, lanzar hilo watcher (arboard real como ClipboardSource), registrar comandos, crear ventana oculta, montar hotkey+tray.
- [ ] **Step 5:** Commit `feat(app): tauri shell with core commands`.

> Nota: `app` no compila en este entorno WSL (faltan webkit2gtk + es target Windows). Se compila en CI (Windows/Linux). El código se escribe completo y se valida en el workflow.

## Task 8: Hotkey global

**Files:** Create `app/src/hotkey.rs`.

- [ ] **Step 1:** Registrar `Ctrl+Shift+V` con `global-hotkey`; al dispararse, mostrar+enfocar ventana, emitir evento `open` a la UI. Esc/blur ocultan (manejado en UI/JS + comando hide).
- [ ] **Step 2:** Commit `feat(app): global hotkey toggle overlay`.

## Task 9: Bandeja

**Files:** Create `app/src/tray.rs`.

- [ ] **Step 1:** Tray icon con menú: Abrir, Pausar captura (toggle), Salir.
- [ ] **Step 2:** Commit `feat(app): system tray menu`.

## Task 10: Pegar

**Files:** Create `app/src/paste.rs`.

- [ ] **Step 1:** `paste_item`: poner item en portapapeles (arboard, según kind), ocultar overlay, simular Ctrl+V (enigo). Fallback documentado: si enigo falla, solo copia.
- [ ] **Step 2:** Commit `feat(app): paste selected item`.

## Task 11: UI overlay (layout A)

**Files:** Create `app/ui/index.html`, `app/ui/style.css`, `app/ui/app.js`.

- [ ] **Step 1:** HTML: barra búsqueda + lista (izq) + preview (der), tema oscuro (según mockup aprobado).
- [ ] **Step 2:** JS: al abrir, foco en buscador; input → invoke `list(query)`; render lista; ↑↓ navegar; Enter → `paste_item`; Esc → hide; Ctrl+P → `pin`; Del → `remove`. Render preview por kind (texto, HTML rich en iframe sandbox, imagen desde dataurl, lista de archivos).
- [ ] **Step 3:** Commit `feat(ui): alfred-style overlay`.

## Task 12: CI workflow

**Files:** Create `.github/workflows/ci.yml`.

- [ ] **Step 1:** En push/PR: job Linux instala deps tauri + Rust, `cargo test -p godclipboard-core`, `cargo fmt --check`, `cargo clippy`. Job que compila `app` en Windows.
- [ ] **Step 2:** Commit `ci: test workflow`.

## Task 13: Release workflow

**Files:** Create `.github/workflows/release.yml`.

- [ ] **Step 1:** En push a `master`: leer versión de `app/tauri.conf.json`/`Cargo.toml`, calcular tag `v{version}+{run}` o auto-bump patch, crear tag si no existe.
- [ ] **Step 2:** Matriz build Windows (.msi/.exe) + Linux (.deb/AppImage) con `tauri-action`, subir artefactos a un GitHub Release nuevo por cada push a master.
- [ ] **Step 3:** Commit `ci: release on master with versioned GitHub Release`.

## Task 14: Documentación

**Files:** Create `README.md`, `docs/ARCHITECTURE.md`.

- [ ] **Step 1:** README: qué es, features, atajos, build en Windows (rustup + WebView2 + `cargo tauri build`), build/test del core en Linux, cómo funciona el release.
- [ ] **Step 2:** ARCHITECTURE.md: diagrama, responsabilidades por módulo, modelo de datos, decisiones (Tauri, privacidad, fallback de pegar), X11/Wayland fase 2.
- [ ] **Step 3:** Commit `docs: README + architecture`.

---

## Self-Review

- **Spec coverage:** texto/rich/imagen/archivo (model+watcher), persistencia (store+e2e), fuzzy (search), favoritos (store pin + UI), privacidad (privacy+watcher), hotkey/tray/paste/overlay (app), riesgos WSL/Windows (docs+CI), release on master (workflow). ✔
- **Placeholder scan:** sin TBD; código real en core; app/UI con código completo en ejecución. ✔
- **Type consistency:** `ClipItem`/`ClipKind`/`Capture`/`Store`/`rank` nombrados consistentes entre tareas. ✔
- **Testabilidad honesta:** core testeado E2E en este entorno; `app` validado por CI (no buildable en WSL sin webkit2gtk + target Windows). Documentado. ✔
```
