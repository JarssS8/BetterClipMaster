# BetterClipMaster

Gestor de portapapeles nativo estilo **Alfred 5**, escrito en Rust + Tauri.
Corre en segundo plano, guarda tu historial del portapapeles y se abre con un
atajo de teclado global para buscar y pegar lo que copiaste antes.

> Estado: MVP. Target primario **Windows**; Linux en progreso (ver _Limitaciones_).

![icon](app/icons/icon.png)

## Características

- 📋 Historial del portapapeles persistente (SQLite, sobrevive a reiniciar).
- 🔎 Búsqueda **fuzzy** en vivo al escribir (estilo Alfred).
- ⭐ **Favoritos** fijados que nunca se borran por la limpieza automática.
- 🧩 Soporta texto, texto enriquecido, imágenes y listas de archivos en el modelo.
- 🔐 **Privacidad**: ignora contenido marcado como sensible por gestores de
  contraseñas (formato `Clipboard Viewer Ignore` en Windows).
- ⌨️ Totalmente manejable con teclado, ventana flotante que aparece sobre todo.
- 🖥️ Vive en la bandeja del sistema; pausar/reanudar captura desde ahí.

## Atajos

| Atajo            | Acción                          |
| ---------------- | ------------------------------- |
| `Ctrl+Shift+V`   | Abrir / cerrar el overlay       |
| `↑` / `↓`        | Navegar la lista                |
| `Enter`          | Pegar el item seleccionado      |
| `Ctrl+P`         | Fijar / desfijar favorito       |
| `Del`            | Borrar item                     |
| `Esc`            | Cerrar el overlay               |

## Estructura del proyecto

```
betterclipmaster/
├── core/   # librería Rust pura: modelo, store SQLite, búsqueda, privacidad, watcher
├── app/    # shell Tauri: comandos, hotkey, bandeja, pegar, UI (HTML/CSS/JS)
├── scripts/generate-icons.mjs   # genera los iconos sin dependencias externas
└── .github/workflows/           # CI y release automático
```

La lógica de negocio vive en `core` y está **probada de extremo a extremo** sin
depender de GUI ni del SO. `app` es una capa fina específica de plataforma.

## Desarrollo

### Probar el core (cualquier SO, incl. WSL/Linux)

No necesita librerías de GUI:

```bash
cargo test -p betterclipmaster-core
cargo clippy -p betterclipmaster-core -- -D warnings
cargo fmt -p betterclipmaster-core -- --check
```

### Compilar y ejecutar la app

**Windows** (recomendado, es el target primario):

1. Instala [Rust](https://rustup.rs) (toolchain MSVC).
2. Instala [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/)
   (ya viene en Windows 11; en 10 puede requerir instalarlo).
3. Instala la CLI de Tauri: `cargo install tauri-cli --version "^2"`
4. Desde la raíz:
   ```bash
   cargo tauri dev      # desarrollo
   cargo tauri build    # binario + instalador (.msi/.exe)
   ```

**Linux** (necesita las libs de Tauri):

```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev \
  libayatana-appindicator3-dev librsvg2-dev libxdo-dev \
  libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
cargo tauri build
```

> Nota de desarrollo: este repo se desarrolló en **WSL2**. El core se compila y
> prueba ahí; la app Tauri se compila/ejecuta en Windows nativo o en CI, no
> dentro de WSL (falta el entorno gráfico y el target es Windows).

### Regenerar iconos

```bash
node scripts/generate-icons.mjs
```

## Releases automáticos

Cada **push a `master`** dispara `.github/workflows/release.yml`, que:

1. Calcula la versión `‹major›.‹minor›.‹run_number›` (major.minor salen de
   `app/tauri.conf.json`), garantizando una versión nueva y creciente.
2. Compila los bundles nativos para **Windows** y **Linux** con `tauri-action`.
3. Publica un **GitHub Release** etiquetado `v‹version›` con los artefactos.

`ci.yml` corre en cada push/PR: tests + clippy + fmt del core, y build del `app`
en Windows y Linux.

## Privacidad

El historial se guarda **sin cifrar** en el directorio de datos del usuario
(`%APPDATA%/com.jars.betterclipmaster` en Windows). El contenido marcado como
sensible por gestores de contraseñas no se guarda. El cifrado de la base de
datos es una mejora futura.

## Limitaciones conocidas (siguientes iteraciones)

- La **captura desde el SO** cubre texto e imágenes. La captura de texto
  enriquecido (HTML) y listas de archivos desde el portapapeles real está
  pendiente (el modelo y el almacenamiento ya los soportan y están testeados).
- El **pegado** coloca el contenido como texto/imagen; el pegado nativo de texto
  enriquecido (formato HTML) es una mejora futura.
- **Linux/Wayland**: los atajos globales y la simulación de teclas están
  restringidos; soporte completo planificado para la fase 2 (X11 primero).

## Documentación

- Diseño: [`docs/superpowers/specs/2026-06-08-betterclipmaster-design.md`](docs/superpowers/specs/2026-06-08-betterclipmaster-design.md)
- Plan: [`docs/superpowers/plans/2026-06-08-betterclipmaster.md`](docs/superpowers/plans/2026-06-08-betterclipmaster.md)
- Arquitectura: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)

## Licencia

MIT
