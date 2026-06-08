// Overlay logic: query the core, render the Alfred-style list + preview, and
// handle keyboard navigation. Uses the global Tauri API (withGlobalTauri).

const invoke = window.__TAURI__.core.invoke;
const listen = window.__TAURI__.event.listen;

const queryEl = document.getElementById("query");
const listEl = document.getElementById("list");
const previewEl = document.getElementById("preview");

let items = [];
let selected = 0;

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
    li.addEventListener("click", () => {
      selected = i;
      render();
    });
    li.addEventListener("dblclick", () => paste(item.id));
    listEl.appendChild(li);
  });
  renderPreview(items[selected]);
  const sel = listEl.querySelector(".selected");
  if (sel) sel.scrollIntoView({ block: "nearest" });
}

function renderPreview(item) {
  if (!item) {
    previewEl.innerHTML = "";
    return;
  }
  if (item.kind === "image" && item.dataurl) {
    previewEl.innerHTML = `<img src="${item.dataurl}" alt="imagen" />`;
  } else if (item.kind === "files") {
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

queryEl.addEventListener("input", () => {
  selected = 0;
  refresh();
});

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
