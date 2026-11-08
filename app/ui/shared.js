// Shared by every window (loaded first): the command bridge (`invoke`, `Problem`), theme
// list, toasts, small DOM helpers, the hotkey recorder, and `start(view)`. No framework, no
// build step: Tauri serves these files and `window.__TAURI__` (withGlobalTauri) talks to
// app/src/*.rs. Each window then loads its own script: picker.js, overlay.js, or
// settings.js with settings-*.js, welcome.js, schedule.js and desktop.js.
"use strict";

const tauriInvoke = window.__TAURI__.core.invoke;

/** A command that failed. Prints as its message (so `toast(String(e))` reads as before);
 *  `code` and `subject` name the cause when the app can act on it (`src/problem.rs`:
 *  not-installed, no-such-theme, theme-active, name-taken, no-theme-yet, not-a-theme,
 *  too-big, needs-permission). */
class Problem extends Error {
  constructor(e) {
    super(typeof e === "string" ? e : e?.message ?? String(e));
    this.code = e?.code ?? null;
    this.subject = e?.subject ?? null;
  }
  toString() { return this.message; }
}
const invoke = (cmd, args) => tauriInvoke(cmd, args).catch((e) => { throw new Problem(e); });
const { listen } = window.__TAURI__.event;
const VIEW = document.body.dataset.view; // "picker" | "overlay" | "settings"
const $ = (sel) => document.querySelector(sel);

let themes = [];
let thumbs = new Map(); // name -> Promise<data URL | null>

const thumb = (name) => {
  if (!thumbs.has(name)) thumbs.set(name, invoke("thumbnail", { name }).catch(() => null));
  return thumbs.get(name);
};

function el(tag, props = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (k === "class") node.className = v;
    else if (k === "style") Object.assign(node.style, v);
    else if (k.startsWith("on")) node.addEventListener(k.slice(2), v);
    else if (v === false || v == null) continue; // a boolean attribute that's off (disabled)
    else node.setAttribute(k, v);
  }
  for (const c of children) if (c != null) node.append(c);
  return node;
}

// ---------------------------------------------------------------- shared

/** Paint the window itself in the active theme. */
function paintChrome() {
  const cur = themes.find((t) => t.current && !t.invalid) || themes.find((t) => !t.invalid);
  if (!cur) return;
  const c = cur.colors;
  const root = document.documentElement.style;
  root.setProperty("--bg", c.background);
  root.setProperty("--bg-dark", c.dark_background);
  root.setProperty("--surface", c.lighter_background);
  root.setProperty("--sel", c.selection);
  // Text on the selection: the brightest text, the one the readability fixes keep readable
  // there (11c; normal and secondary text aren't checked on it).
  root.setProperty("--on-sel", c.bright_foreground);
  root.setProperty("--fg", c.foreground);
  root.setProperty("--dim", c.dark_foreground);
  root.setProperty("--muted", c.muted);
  root.setProperty("--accent", c.accent);
  root.setProperty("--red", c.red);
  // Native controls (a dropdown's open list) follow the theme's light or dark.
  root.setProperty("color-scheme", cur.mode === "light" ? "light" : "dark");
}

let toastTimer;
function toast(text, error = false, ms = error ? 5000 : 2200) {
  const t = $("#toast");
  t.textContent = text;
  t.classList.remove("action", "report");
  t.classList.toggle("error", error);
  t.classList.add("show");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => t.classList.remove("show"), ms);
}

/** "Switched to X · Undo" in the picker (5 s; the button undoes it). */
function undoToast(theme) {
  const t = $("#toast");
  const undo = el("button", { class: "btn" }, "Undo");
  undo.onclick = () => picker.step("undo");
  t.replaceChildren(el("span", {}, "Switched to ", el("b", {}, theme)), undo);
  t.classList.remove("error");
  t.classList.add("show", "action");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => t.classList.remove("show", "action"), 5000);
}

/** "2 low-contrast pairs" and the report, for a badge (`palette check`).
 *  `fixable`: a theme you have (the picker), so the report offers Fix and Edit;
 *  a Get themes card isn't installed yet, so it only shows the report. */
function contrastBadge(t, long = false, fixable = !long) {
  const n = t.contrast?.length || 0;
  if (!n) return null;
  const what = `${n} low-contrast pair${n === 1 ? "" : "s"}`;
  const text = `${what} in ${t.name}:\n• ${t.contrast.join("\n• ")}`;
  // Click (not just hover) shows the report: tooltips are slow to appear in
  // the menu bar window. Settings → Colours lists it with an Apply per pair.
  return el("span", {
    class: "badge warn", role: "button", title: text,
    onclick: (e) => { e.stopPropagation(); fixable ? reportToast(t, text) : toast(text, false, 8000); },
  }, long ? what : `◐ ${n}`);
}

/** The contrast report with "Fix all" (every suggested colour) and "Edit
 *  colours…" (Settings → Colours). Stays 20 s, long enough to read. */
function reportToast(t, text) {
  const box = $("#toast");
  const close = () => box.classList.remove("show", "action", "report");
  const fix = el("button", { class: "btn primary" }, "Fix all");
  fix.onclick = async (e) => {
    e.stopPropagation();
    fix.disabled = true;
    try {
      const [changed, problems] = await invoke("edit_colours", { name: t.name, how: "fix" });
      await load();
      view.render();
      if (problems.length) toast(problems.join(" · "), true);
      else toast(`${t.name}: ${changed.length} colour${changed.length === 1 ? "" : "s"} adjusted (Settings → Colours to undo)`, false, 4000);
    } catch (err) { toast(String(err), true); }
  };
  const edit = el("button", { class: "btn" }, "Edit colours…");
  edit.onclick = async (e) => {
    e.stopPropagation();
    close();
    await invoke("open_settings_tab", { tab: `colours:${t.name}` });
    if (VIEW !== "settings") invoke("hide");
  };
  const later = el("button", { class: "btn" }, "Close");
  later.onclick = (e) => { e.stopPropagation(); close(); };
  box.replaceChildren(el("div", { class: "rep" }, text), el("div", { class: "acts" }, fix, edit, later));
  box.classList.remove("error");
  box.classList.add("show", "action", "report");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(close, 20000);
}

function report(result) {
  if (result.problems.length) toast(result.problems.join(" · "), true);
}

async function load() {
  themes = await invoke("themes");
  thumbs = new Map(); // wallpapers may have changed since last time
  paintChrome();
}

/** The words the chips offer; words in one group exclude each other (dark or light). */
const CHIP_GROUPS = [["dark", "light"], ["warm", "cool"], ["calm", "vivid", "mono"],
  ["red", "orange", "yellow", "green", "teal", "blue", "purple", "pink"]];

/** A row of word chips for a search box (12c): a click adds the word to the box, or takes
 *  it out; another word of the same kind replaces it (light replaces dark). */
function chipRow(input, onchange) {
  const words = () => input.value.trim().toLowerCase().split(/\s+/).filter(Boolean);
  const row = el("div", { class: "chips", role: "group", "aria-label": "Search by look" });
  const paint = () => {
    const now = words();
    row.querySelectorAll(".chip").forEach((b) => b.classList.toggle("on", now.includes(b.dataset.w)));
  };
  for (const group of CHIP_GROUPS) {
    for (const w of group) {
      row.append(el("button", { class: "chip", type: "button", "data-w": w, onclick: () => {
        let now = words();
        const on = now.includes(w);
        now = now.filter((x) => !(group.length < 8 ? group.includes(x) : x === w));
        if (!on) now.push(w);
        input.value = now.join(" ");
        paint();
        onchange();
      } }, w));
    }
  }
  input.addEventListener("input", paint);
  paint();
  return row;
}



// ---------------------------------------------------------------- hotkey recorder

const SYMBOLS = { ctrl: "⌃", alt: "⌥", shift: "⇧", cmd: "⌘" };
const pretty = (combo) =>
  combo.split("+").map((p) => SYMBOLS[p] || (p.length === 1 ? p.toUpperCase() : p[0].toUpperCase() + p.slice(1))).join("");

/** `KeyT` → `t`, `Digit5` → `5`, `ArrowUp` → `up`, `F5` → `f5`. */
function keyName(code) {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3).toLowerCase();
  if (/^Digit\d$/.test(code)) return code.slice(5);
  if (/^F\d{1,2}$/.test(code)) return code.toLowerCase();
  const named = { Space: "space", ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right", Tab: "tab", Enter: "enter" };
  return named[code] || null;
}

let recording = false;
async function recordKey(e) {
  e.preventDefault();
  const btn = $("#hotkey");
  if (e.key === "Escape") {
    recording = false;
    btn.classList.remove("recording");
    $("#record").textContent = "Change…";
    btn.textContent = pretty(await invoke("get_hotkey"));
    return;
  }
  const key = keyName(e.code);
  if (!key) return; // a modifier on its own: keep waiting
  const combo = [e.ctrlKey && "ctrl", e.altKey && "alt", e.shiftKey && "shift", e.metaKey && "cmd", key]
    .filter(Boolean).join("+");
  recording = false;
  btn.classList.remove("recording");
  $("#record").textContent = "Change…";
  try {
    btn.textContent = pretty(await invoke("set_hotkey", { combo }));
    toast(`Quick switch is now ${btn.textContent}`);
  } catch (err) {
    btn.textContent = pretty(await invoke("get_hotkey"));
    toast(String(err), true);
  }
}

/** Run a window's view: draw it once every script of the page has loaded, again on each
 *  focus (windows are hidden, not closed, until idle), and after any switch. A view with
 *  its own `onFocus` (Settings) decides for itself what a focus redraws. */
function start(view) {
  window.addEventListener("focus", () => (view.onFocus ? view.onFocus() : view.open()));
  listen("theme-changed", async () => { await load(); view.render(); });
  document.addEventListener("DOMContentLoaded", () => view.open());
}
