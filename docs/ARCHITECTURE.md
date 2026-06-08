# Arquitectura de godclipboard

## Visión general

Una sola app nativa (Tauri) con dos partes en el mismo binario: un **backend
siempre activo** que captura el portapapeles y una **ventana overlay oculta**
que se muestra con un atajo global.

```
┌─────────────────────────────────────────────┐
│  godclipboard (Tauri app, corre en bandeja)  │
│                                               │
│  ┌─────────────┐      ┌──────────────────┐  │
│  │ Backend Rust │◄────►│ UI WebView (HTML) │  │
│  │ (siempre on) │      │ (overlay, oculta) │  │
│  └──────┬───────┘      └──────────────────┘  │
│         │                                     │
│  watcher clipboard → SQLite ← fuzzy search    │
│  global hotkey → mostrar/ocultar overlay      │
│  tray icon → salir / pausar / ajustes         │
└───────────────────────────────────────────────┘
```

## Separación en dos crates

El proyecto es un **workspace Cargo** con dos crates:

### `core` (`godclipboard-core`) — librería Rust pura

Sin GUI, sin atajos, sin código de portapapeles del SO. Toda la lógica de
negocio vive aquí y se puede testear de extremo a extremo sin display.

| Módulo        | Responsabilidad                                                        | Depende de        |
| ------------- | ---------------------------------------------------------------------- | ----------------- |
| `model`       | `ClipItem`, `ClipKind`, `NewItem`, hashing (sha256) y preview.         | serde, sha2       |
| `store`       | Persistencia SQLite: insert con dedup, prune, pin, query, delete.      | rusqlite          |
| `search`      | Ranking fuzzy de items contra una query.                              | nucleo-matcher    |
| `privacy`     | Detección de contenido sensible por marcadores de formato.            | —                 |
| `watcher`     | Trait `ClipboardSource` + `Watcher::poll_once` + `MockSource` (test). | model, privacy, store |

### `app` (`godclipboard`) — shell Tauri

Capa fina específica de plataforma que cablea el core al SO.

| Módulo               | Responsabilidad                                                       |
| -------------------- | -------------------------------------------------------------------- |
| `lib.rs`             | Arranque: abre el store, lanza el hilo watcher, configura Tauri.      |
| `clipboard_source`   | `OsClipboard`: lee el portapapeles real (arboard) → `Capture`.       |
| `commands`           | Comandos Tauri (`list`, `pin`, `remove`, `paste_item`, …) y estado.  |
| `hotkey`             | Atajo global `Ctrl+Shift+V` que muestra/oculta el overlay.           |
| `tray`               | Icono de bandeja con menú Abrir / Pausar / Salir.                    |
| `paste`              | Pone el item en el portapapeles y simula `Ctrl+V` (enigo).           |
| `ui/`                | Overlay HTML/CSS/JS: búsqueda + lista + preview, navegación teclado. |

## Modelo de datos

Tabla `clips` (SQLite):

| Columna      | Tipo    | Notas                                            |
| ------------ | ------- | ------------------------------------------------ |
| `id`         | INTEGER | PK autoincremental.                              |
| `kind`       | TEXT    | `text` \| `rich` \| `image` \| `files`.          |
| `content`    | TEXT    | Texto, HTML, o rutas (separadas por `\n`).       |
| `blob`       | BLOB    | Bytes PNG de la imagen, `NULL` si no aplica.     |
| `preview`    | TEXT    | Línea corta para la lista.                       |
| `pinned`     | INTEGER | Favorito (0/1). Nunca se borra por prune.        |
| `created_at` | INTEGER | Timestamp lógico en ms, estrictamente creciente. |
| `hash`       | TEXT    | sha256(kind+content+blob) para deduplicar.       |

## Flujos

**Captura (hilo de fondo, cada 500 ms):**

```
OsClipboard.read() → ¿cambió? → Watcher.poll_once →
  ¿sensible? sí→descartar / no→ Store.insert (dedup) → prune(MAX_ITEMS)
```

El watcher usa su **propia conexión SQLite** al mismo archivo (modo WAL permite
lectores/escritores concurrentes), separada de la conexión de los comandos.

**Uso (overlay con hotkey):**

```
Ctrl+Shift+V → mostrar overlay + evento `open` → UI enfoca búsqueda →
  input → command `list(query)` → rank fuzzy → render lista + preview →
  Enter → command `paste_item(id)` → set portapapeles + ocultar + simular Ctrl+V
```

## Decisiones de diseño

- **Tauri sobre egui**: el overlay estilo Alfred es HTML casi directo; el webview
  renderiza imágenes (data URL) de forma nativa; binario pequeño (WebView2).
- **Lógica en `core`, no en `app`**: permite testear todas las casuísticas sin
  GUI/SO. La cobertura E2E vive en `core/tests/e2e.rs`.
- **Dedup con bump**: re-copiar un item existente no crea fila duplicada; mueve la
  existente al tope (`created_at` lógico creciente garantiza orden determinista).
- **Privacidad por marcador**: en Windows se detecta `Clipboard Viewer Ignore`
  para no guardar contraseñas; barato y efectivo.
- **Pegado best-effort**: si la simulación de teclas falla, el item ya está en el
  portapapeles y el usuario puede pegar con `Ctrl+V` manual.

## Fases

- **Fase 1 (actual)**: Windows. Captura texto+imagen, store, hotkey, overlay,
  fuzzy, favoritos, privacidad, pegado, bandeja.
- **Fase 2**: captura SO de HTML enriquecido y listas de archivos; pegado nativo
  de rich text; Linux (X11 → Wayland); cifrado opcional de la BD; ajustes por GUI.
