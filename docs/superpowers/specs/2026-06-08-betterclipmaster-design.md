# godclipboard — Design Spec

**Fecha:** 2026-06-08
**Estado:** Aprobado para planificación

## Resumen

Clipboard manager nativo estilo Alfred 5, hecho en Rust con Tauri. Corre en
segundo plano, captura el historial del portapapeles, y se abre con un atajo
de teclado global mostrando un overlay con búsqueda fuzzy para encontrar y
pegar items anteriores.

**Target primario:** Windows. **Fase 2:** Linux (X11 primero).

## Objetivos

- App nativa de un solo binario, vive en la bandeja del sistema.
- Captura texto plano, texto enriquecido (HTML/RTF), imágenes y rutas de archivos.
- Atajo global abre un overlay flotante centrado.
- Búsqueda fuzzy en vivo sobre el historial.
- Favoritos (items fijados que no expiran).
- Persistencia en SQLite (sobrevive a reiniciar).
- No guardar contenido marcado como sensible (gestores de contraseñas).

## No objetivos (por ahora)

- Sincronización en la nube.
- Cifrado de la base de datos (posible fase futura).
- Soporte Wayland completo (fase 2, asumido como trabajo extra).
- Ajustes avanzados configurables por GUI (fase 2).

## Stack

- **UI:** Tauri (backend Rust + WebView del SO, UI en HTML/CSS/JS).
- **Persistencia:** SQLite vía `rusqlite`.
- **Portapapeles:** `arboard` (+ listener de cambios).
- **Atajo global:** `global-hotkey`.
- **Bandeja:** `tray-icon`.
- **Fuzzy search:** `nucleo`.
- **Simular pegar:** `enigo` (o API del SO).

Razón de Tauri sobre egui: rich text e imágenes se renderizan nativo en el
webview; el layout elegido (estilo Alfred) es HTML casi directo; binario pequeño
usando WebView2 del SO.

## Arquitectura

App única (Tauri) con backend siempre activo + ventana overlay oculta.

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

- Arranca al inicio de sesión, vive en la bandeja, ventana oculta.
- Hotkey global muestra el overlay centrado; Esc o perder foco lo oculta.

## Componentes (unidades aisladas)

| Unidad | Qué hace | Depende de |
|---|---|---|
| `clipboard_watcher` | Detecta cambios del portapapeles (texto/rich/imagen/archivo), normaliza a `ClipItem`. | `arboard`, listener de eventos |
| `store` | Guarda/lee/borra items en SQLite. Dedup, límite de historial, favoritos. | `rusqlite` |
| `search` | Filtro fuzzy sobre el historial. | `nucleo` |
| `hotkey` | Registra atajo global, emite evento "abrir". | `global-hotkey` |
| `tray` | Icono bandeja, menú salir / pausar captura / ajustes. | `tray-icon` |
| `paste` | Inserta el item elegido en la app activa (copia + simula Ctrl+V). | `enigo` / API SO |
| UI (webview) | Lista + preview lateral + búsqueda. Habla con backend vía comandos Tauri. | HTML/CSS/JS |

`store` y `search` son Rust puro sin dependencia de UI ni SO → testeables solos.
`clipboard_watcher` se testea con un trait mockeable del portapapeles.

## Modelo de datos: `ClipItem`

```
id          INTEGER
kind        TEXT      -- 'text' | 'rich' | 'image' | 'files'
content     TEXT      -- texto, o HTML (rich), o rutas (files)
blob        BLOB      -- bytes de imagen (NULL si no aplica)
preview     TEXT      -- texto corto para la lista
pinned      INTEGER   -- favorito 0/1
created_at  INTEGER   -- timestamp unix
hash        TEXT      -- para deduplicar copias repetidas
```

- Imágenes: miniatura para la lista, blob completo para pegar.
- Límite de historial configurable (ej. 1000 items o 90 días).
- Favoritos (pinned) nunca expiran.

## UI — layout elegido

Estilo Alfred clásico: barra de búsqueda arriba, lista a la izquierda, vista
previa del item seleccionado a la derecha. Tema oscuro.

Atajos dentro del overlay:

- `Enter` — pegar item seleccionado
- `Esc` — cerrar overlay
- `↑` / `↓` — navegar lista
- `Ctrl+P` — fijar / desfijar favorito
- `Del` — borrar item

Atajo global por defecto (configurable): `Ctrl+Shift+V`.

## Flujo de datos

**Captura (background, siempre):**

```
copias algo → watcher detecta → normaliza ClipItem → calcula hash →
  ¿duplicado del último? sí→ignora / no→ store.insert → poda si excede límite
```

**Uso (con hotkey):**

```
pulsas atajo → overlay aparece, foco en buscador →
  escribes → fuzzy filtra en vivo → ↑↓ navegas, preview a la derecha →
  Enter → paste copia al portapapeles + simula Ctrl+V en app activa →
  overlay se oculta
```

## Privacidad

- Detectar y **no guardar** contenido marcado como sensible por gestores de
  contraseñas. En Windows: respetar el formato `Clipboard Viewer Ignore` /
  `ExcludeClipboardContentFromMonitorProcessing`.
- Pausar captura manualmente desde el menú de la bandeja.
- BD local sin cifrar por defecto. Cifrado = posible fase futura.

## Manejo de errores

- El watcher nunca tumba la app: errores logueados, la captura continúa.
- BD corrupta → backup + recrear esquema.
- Atajo global ya tomado por otra app → avisar y permitir reasignar.
- Si simular paste falla en una app/plataforma → fallback: solo copiar al
  portapapeles y que el usuario pegue con Ctrl+V manual.

## Testing

- `store`: tests unitarios Rust puros (insert, dedup, poda, favoritos, query).
- `search`: tests unitarios del fuzzy ranking.
- `clipboard_watcher`: test con trait mockeable del portapapeles.
- UI: sin lógica de negocio (toda en backend), pruebas manuales.

## Riesgos y consideraciones

1. **Entorno dev = WSL2, target = Windows nativo.** Tauri en Windows requiere
   WebView2 + toolchain MSVC. Compilar/probar el binario final en Windows, no en
   WSL. WSL sirve para editar.
2. **Linux (fase 2): X11 vs Wayland.** Atajos globales y simular teclas están
   restringidos en Wayland (portales/permisos). X11 más fácil. Trabajo extra.
3. **Simular "pegar" (Ctrl+V)** es lo más frágil cross-platform. Plan B definido
   (solo copiar).
4. **Autoarranque + bandeja:** registrar inicio con la sesión (registro Windows).
5. **Tamaño de imágenes** en SQLite: miniatura + blob, poda agresiva.

## Fases

**Fase 1 (MVP, Windows):**
watcher texto+rich+imagen+archivo, SQLite, hotkey global, overlay (layout
Alfred), fuzzy search, favoritos, ignorar contraseñas, simular pegar, bandeja,
autoarranque.

**Fase 2:**
portar a Linux (X11 primero), cifrado opcional de BD, ajustes configurables por
GUI, soporte Wayland.
