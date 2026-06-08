// Settings window logic. Loads current settings, lets the user edit them
// (including recording a global shortcut), and saves via the backend.

const invoke = window.__TAURI__.core.invoke;

const shortcutEl = document.getElementById("shortcut");
const autostartEl = document.getElementById("autostart");
const maxItemsEl = document.getElementById("maxItems");
const ignoreEl = document.getElementById("ignoreSensitive");
const saveEl = document.getElementById("save");
const clearEl = document.getElementById("clear");
const statusEl = document.getElementById("status");
const versionEl = document.getElementById("version");
const checkUpdateEl = document.getElementById("checkUpdate");
const updateStatusEl = document.getElementById("updateStatus");

let shortcut = "Ctrl+Shift+V";
let recording = false;

const MODS = ["Control", "Shift", "Alt", "Meta"];

function setStatus(msg, kind) {
  statusEl.textContent = msg;
  statusEl.className = kind || "";
}

function prettyKey(e) {
  const k = e.key;
  if (k.length === 1) return k.toUpperCase(); // letters, digits, symbols
  if (/^F\d{1,2}$/.test(k)) return k; // F1..F12
  const map = {
    ArrowUp: "Up", ArrowDown: "Down", ArrowLeft: "Left", ArrowRight: "Right",
    " ": "Space", Escape: "Escape", Enter: "Enter", Tab: "Tab",
  };
  return map[k] || null;
}

function buildShortcut(e) {
  const parts = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.shiftKey) parts.push("Shift");
  if (e.altKey) parts.push("Alt");
  if (e.metaKey) parts.push("Super");
  const key = prettyKey(e);
  if (!key) return null; // only a modifier pressed so far
  parts.push(key);
  return parts.join("+");
}

shortcutEl.addEventListener("click", () => {
  recording = true;
  shortcutEl.classList.add("recording");
  shortcutEl.textContent = "Pulsa la combinación…";
});

document.addEventListener("keydown", (e) => {
  if (!recording) return;
  e.preventDefault();
  if (MODS.includes(e.key)) return; // wait for a real key
  const combo = buildShortcut(e);
  if (!combo) return;
  shortcut = combo;
  shortcutEl.textContent = combo;
  shortcutEl.classList.remove("recording");
  recording = false;
});

async function load() {
  try {
    const s = await invoke("get_settings");
    shortcut = s.shortcut;
    shortcutEl.textContent = s.shortcut;
    autostartEl.checked = s.autostart;
    maxItemsEl.value = s.max_items;
    ignoreEl.checked = s.ignore_sensitive;
  } catch (e) {
    setStatus("No se pudieron cargar los ajustes: " + e, "err");
  }
}

saveEl.addEventListener("click", async () => {
  const payload = {
    shortcut,
    autostart: autostartEl.checked,
    max_items: parseInt(maxItemsEl.value, 10) || 1000,
    ignore_sensitive: ignoreEl.checked,
  };
  try {
    await invoke("set_settings", { new: payload });
    setStatus("Guardado ✓", "ok");
  } catch (e) {
    setStatus("Error: " + e, "err");
  }
});

clearEl.addEventListener("click", async () => {
  try {
    await invoke("clear_history");
    setStatus("Historial borrado", "ok");
  } catch (e) {
    setStatus("Error: " + e, "err");
  }
});

async function loadVersion() {
  try {
    const v = await invoke("app_version");
    versionEl.textContent = "Versión " + v;
  } catch (e) {
    versionEl.textContent = "Versión —";
  }
}

checkUpdateEl.addEventListener("click", async () => {
  updateStatusEl.textContent = "Buscando…";
  updateStatusEl.style.color = "var(--dim)";
  try {
    const upd = await invoke("check_update");
    if (!upd) {
      updateStatusEl.textContent = "Ya tienes la última versión ✓";
      updateStatusEl.style.color = "#6ec07a";
      return;
    }
    updateStatusEl.textContent = `Nueva versión ${upd.version} disponible.`;
    updateStatusEl.style.color = "var(--accent)";
    checkUpdateEl.textContent = "Instalar y reiniciar";
    checkUpdateEl.onclick = async () => {
      updateStatusEl.textContent = "Descargando e instalando… la app se reiniciará.";
      try {
        await invoke("install_update");
      } catch (e) {
        updateStatusEl.textContent = "Error al actualizar: " + e;
        updateStatusEl.style.color = "#e57373";
      }
    };
  } catch (e) {
    updateStatusEl.textContent = "Error al comprobar: " + e;
    updateStatusEl.style.color = "#e57373";
  }
});

load();
loadVersion();
