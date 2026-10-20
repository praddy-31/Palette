// Palette's picker and overlay. No framework, no build step: Tauri serves
// these files and `window.__TAURI__` (withGlobalTauri) talks to app/src/main.rs.
"use strict";

const { invoke } = window.__TAURI__.core;
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
  root.setProperty("--fg", c.foreground);
  root.setProperty("--dim", c.dark_foreground);
  root.setProperty("--muted", c.muted);
  root.setProperty("--accent", c.accent);
  root.setProperty("--red", c.red);
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

const strip = (t) =>
  el("div", { class: "strip" },
    ...["background", "accent", "red", "yellow", "green", "cyan", "blue", "magenta"]
      .map((k) => el("i", { style: { background: t.colors[k] } })));

// ---------------------------------------------------------------- picker

const picker = {
  busy: false,
  recent: [],

  async open() {
    await load();
    this.recent = await invoke("recents").catch(() => []);
    $("#search").value = "";
    $("#search").focus();
    this.render();
    this.renderWall();
    const key = pretty(await invoke("get_hotkey"));
    $("#hint").textContent = key;
    $("#hint").title = `${key} opens the quick switcher`;
  },

  render() {
    const q = $("#search").value.trim().toLowerCase();
    const cur = themes.find((t) => t.current);
    $("#now").textContent = cur ? cur.name : "no theme yet";
    const grid = $("#grid");
    grid.replaceChildren();
    if (!q) {
      // Favourites and recent themes first, each as one scrolling row.
      const byName = new Map(themes.map((t) => [t.name, t]));
      const favs = themes.filter((t) => t.favourite && !t.invalid);
      const recent = this.recent.filter((n) => !cur || n !== cur.name).map((n) => byName.get(n)).filter((t) => t && !t.invalid);
      for (const [title, list] of [["Favourites", favs], ["Recent", recent]]) {
        if (!list.length) continue;
        grid.append(el("div", { class: "sect" }, title),
          el("div", { class: "shelf" }, ...list.map((t) => this.card(t))));
      }
      if (grid.childElementCount) grid.append(el("div", { class: "sect" }, "All themes"));
    }
    for (const t of themes.filter((t) => t.name.includes(q))) grid.append(this.card(t));
    if (!q || "get more themes".includes(q)) {
      grid.append(el("button", {
        class: "card more", title: "Community themes for Omarchy, installable here",
        onclick: async () => { await invoke("open_settings_tab", { tab: "themes" }); invoke("hide"); },
      }, el("div", { class: "thumb" }, el("span", {}, "+")), el("div", { class: "meta" }, el("span", { class: "name" }, "Get more themes"))));
    }
    grid.querySelector(".grid > .card.current")?.scrollIntoView({ block: "nearest" });
    this.renderWall();
  },

  /** One theme card: thumbnail, ★ toggle, name and badges. */
  card(t) {
    const img = el("img", { alt: "" });
    const box = el("div", { class: "thumb" }, img, t.invalid ? null : strip(t));
    if (!t.invalid) {
      const c = t.colors;
      box.style.background = `radial-gradient(circle at 75% 30%, ${c.accent}55, transparent 45%),
        linear-gradient(135deg, ${c.background}, ${c.darker_background})`;
      thumb(t.name).then((src) => {
        if (src) { img.src = src; img.onload = () => img.classList.add("ready"); }
      });
    }
    const card = el("button", {
      class: `card${t.current ? " current" : ""}${t.invalid ? " invalid" : ""}`,
      title: t.invalid || `${t.name} (${t.origin}${t.wallpapers ? `, ${t.wallpapers} wallpapers` : ""})`,
      onclick: () => {
        if (t.invalid) return;
        if (t.current) return toast(`${t.name} is already active`);
        this.apply(t.name, card);
      },
    },
      box,
      el("div", { class: "meta" },
        el("span", { class: "name" }, t.name),
        t.mode === "light" ? el("span", { class: "badge" }, "light") : null,
        t.origin === "user" ? el("span", { class: "badge" }, "yours") : null,
        contrastBadge(t)));
    if (!t.invalid) {
      const star = el("span", {
        class: `star${t.favourite ? " on" : ""}`, role: "button", tabindex: "0",
        "aria-label": t.favourite ? `Remove ${t.name} from favourites` : `Add ${t.name} to favourites`,
        title: t.favourite ? "Favourite (click to remove)" : "Add to favourites",
        onclick: (e) => { e.stopPropagation(); this.favourite(t); },
        onkeydown: (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); e.stopPropagation(); this.favourite(t); } },
      }, t.favourite ? "★" : "☆");
      box.append(star);
    }
    return card;
  },

  async favourite(t) {
    try {
      const favs = await invoke("set_favourite", { name: t.name, on: !t.favourite });
      for (const x of themes) x.favourite = favs.includes(x.name);
      if (!t.favourite) toast(`${t.name} isn't a favourite any more`);
      else toast(`★ ${t.name}: in Favourites (${favs.length}/10)`);
    } catch (e) { toast(String(e), true); }
    this.render();
  },

  async apply(name, card) {
    if (this.busy) return;
    this.busy = true;
    card?.classList.add("busy");
    try {
      await invoke("apply", { name }); // the "switched" event shows the toast
    } catch (e) {
      toast(String(e), true);
    } finally {
      this.busy = false;
      await load();
      this.render();
    }
  },

  /** Footer: ‹ current wallpaper (n/m) › for the active theme. */
  async renderWall() {
    let w = null;
    try { w = await invoke("wallpapers", {}); } catch { /* no theme yet */ }
    const n = w?.images.length ?? 0;
    const at = w ? w.images.findIndex((i) => i.current) : -1;
    $("#walls-n").textContent = n ? `${at >= 0 ? at + 1 : "–"}/${n}` : "none";
    $("#walls").title = n
      ? `${at >= 0 ? w.images[at].name : ""}\nClick: all of ${w.theme}'s wallpapers (Settings)`
      : "Add some in Settings → Wallpapers";
    $("#wp-prev").disabled = $("#wp-next").disabled = n < 2;
  },

  async stepWall(by) {
    try {
      const problems = await invoke("step_wallpaper", { by });
      if (problems.length) toast(problems.join(" · "), true);
    } catch (e) { toast(String(e), true); }
    this.renderWall();
  },

  async step(how) {
    try {
      await invoke("step", { how }); // the "switched" event shows the toast
    } catch (e) {
      toast(String(e), true);
    }
    await load();
    this.recent = await invoke("recents").catch(() => []);
    this.render();
  },
};

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

// ---------------------------------------------------------------- overlay

const overlay = {
  filter: "",
  index: 0,
  /** Theme name → { images, at, moved }: the wallpaper ←→ picked in the preview. */
  walls: new Map(),

  async open() {
    await load();
    this.filter = "";
    this.walls = new Map();
    const shown = this.shown();
    this.index = Math.max(0, shown.findIndex((t) => t.current));
    this.render();
  },

  shown() {
    return themes.filter((t) => !t.invalid && t.name.includes(this.filter));
  },

  render() {
    const shown = this.shown();
    this.index = Math.min(this.index, Math.max(0, shown.length - 1));
    $("#filter").textContent = this.filter;
    const list = $("#list");
    list.replaceChildren(...shown.map((t, i) =>
      el("li", {
        class: `${i === this.index ? "on" : ""}${t.current ? " current" : ""}`,
        onclick: () => { this.index = i; this.render(); },
        ondblclick: () => this.apply(),
      },
        el("span", { class: "dot", style: { background: `linear-gradient(135deg, ${t.colors.background} 50%, ${t.colors.accent} 50%)` } }),
        el("span", { class: "name" }, t.name),
        t.mode === "light" ? el("span", { class: "badge" }, "light") : null)));
    list.querySelector("li.on")?.scrollIntoView({ block: "nearest" });
    const t = shown[this.index];
    if (t) this.preview(t);
    else $("#preview").replaceChildren();
  },

  /** A mini desktop in `t`'s colours: wallpaper, bar pills, two windows. */
  preview(t) {
    const c = t.colors;
    const desk = $("#preview");
    const vars = {
      "--p-bg": c.background, "--p-bg-dark": c.dark_background, "--p-darker": c.darker_background,
      "--p-fg": c.foreground, "--p-grey": c.dark_foreground, "--p-muted": c.muted,
      "--p-accent": c.accent, "--p-pill": c.selection + "f0",
    };
    for (const [k, v] of Object.entries(vars)) desk.style.setProperty(k, v);
    desk.style.backgroundImage = "";
    const name = t.name;
    thumb(name).then((src) => {
      if (src && this.shown()[this.index]?.name === name) desk.style.backgroundImage = `url("${src}")`;
    });
    $("#mode").textContent = `${t.mode}${t.wallpapers ? ` · ${t.wallpapers} wallpapers` : " · no wallpapers"}`;
    if (this.walls.get(name)?.moved) this.showWall(t);

    const span = (color, text) => el("span", { style: { color } }, text);
    const term = el("div", { class: "term" },
      span(c.green, "❯ "), "palette set ", span(c.accent, name), "\n",
      span(c.yellow, "  ~ "), "sketchybar.sh  ", span(c.dark_foreground, "changed"), "\n",
      span(c.green, "  ok "), "reloaded\n",
      span(c.green, "❯ "), "ls\n",
      span(c.blue, "app/  docs/  src/  "), "Cargo.toml  README.md\n",
      span(c.green, "❯ "), "git status --short\n",
      span(c.red, " M "), "src/main.rs\n",
      span(c.green, "?? "), "app/ui/app.js",
      el("div", { class: "ansi" }, ...t.ansi.map((a) => el("i", { style: { background: a } }))));
    const code = el("div", { class: "code" },
      span(c.dark_foreground, "// theme.rs"), "\n",
      span(c.magenta, "pub fn "), span(c.blue, "load"), "(name: ", span(c.yellow, "&str"), ") {\n",
      "  ", span(c.magenta, "let "), "n = ", span(c.orange, "23"), ";\n",
      "  println!(", span(c.green, "\"hi\""), ");\n}");
    desk.replaceChildren(
      el("div", { class: "bar" },
        el("div", { class: "pill" }, ...[1, 2, 3, 4].map((n) => el("span", { class: `ws${n === 1 ? " on" : ""}` }, String(n)))),
        el("div", { class: "pill" }, el("span", { class: "ic" }, "\uf2db"), "12%  ", el("span", { class: "ic" }, "\uefc5"), "8.1G"),
        el("span", { class: "spacer" }),
        el("div", { class: "pill" }, el("span", { class: "ic" }, "\u{f03d8}"), name),
        el("div", { class: "pill" }, el("span", { class: "ic" }, "\uf017"), "Mon 12 Oct  22:34")),
      el("div", { class: "win back" }, el("div", { class: "title" }, el("i"), el("i"), el("i")), code),
      el("div", { class: "win front" }, el("div", { class: "title" }, el("i"), el("i"), el("i")), term));
  },

  /** ←→: browse the highlighted theme's wallpapers; ⏎ then starts on that one. */
  async moveWall(by) {
    const t = this.shown()[this.index];
    if (!t?.wallpapers) return toast(t ? `${t.name} has no wallpapers` : "");
    let w = this.walls.get(t.name);
    if (!w) {
      const list = await invoke("wallpapers", { theme: t.name });
      const images = list.images;
      let at = images.findIndex((i) => i.current);
      if (at < 0) at = Math.max(0, images.findIndex((i) => i.default));
      w = { images, at, moved: false };
      this.walls.set(t.name, w);
    }
    const n = w.images.length;
    w.at = (w.at + by + n) % n;
    w.moved = true;
    this.showWall(t);
  },

  showWall(t) {
    const w = this.walls.get(t.name);
    const img = w.images[w.at];
    $("#mode").textContent = `${t.mode} · wallpaper ${w.at + 1}/${w.images.length} · ${img.name}`;
    invoke("wallpaper_thumb", { theme: t.name, name: img.name }).then((src) => {
      if (this.shown()[this.index]?.name === t.name && w.images[w.at] === img) {
        $("#preview").style.backgroundImage = `url("${src}")`;
      }
    }).catch(() => {});
  },

  move(by) {
    const n = this.shown().length;
    if (!n) return;
    this.index = (this.index + by + n) % n;
    this.render();
  },

  /** Hide first so the switch feels instant; the desktop changes a moment later. */
  async apply() {
    const t = this.shown()[this.index];
    if (!t) return;
    const w = this.walls.get(t.name);
    const wallpaper = w?.moved ? w.images[w.at].name : null;
    await invoke("hide");
    try {
      await invoke("apply", { name: t.name, wallpaper });
    } catch (e) {
      console.error(e);
    }
  },

  key(e) {
    switch (e.key) {
      case "ArrowDown": this.move(1); break;
      case "ArrowUp": this.move(-1); break;
      case "ArrowRight": this.moveWall(1); break;
      case "ArrowLeft": this.moveWall(-1); break;
      case "Enter": this.apply(); break;
      case "Escape":
        if (this.filter) { this.filter = ""; this.render(); } else invoke("hide");
        break;
      case "Backspace": this.filter = this.filter.slice(0, -1); this.index = 0; this.render(); break;
      default:
        if (/^[a-z0-9-]$/i.test(e.key) && !e.metaKey && !e.ctrlKey) {
          this.filter += e.key.toLowerCase();
          this.index = 0;
          this.render();
        } else return;
    }
    e.preventDefault();
  },
};

// ---------------------------------------------------------------- settings

/** Two-click confirm: first click arms the button for 4 s, second runs it.
 *  (Web views in apps don't reliably show `confirm()` dialogs.) */
function armed(btn, label) {
  if (btn.dataset.armed) return true;
  const original = btn.textContent;
  btn.dataset.armed = "1";
  btn.textContent = label;
  setTimeout(() => { delete btn.dataset.armed; btn.textContent = original; }, 4000);
  return false;
}

/** `1.2 GB`, `340 MB`, `12 KB` (as the CLI prints sizes). */
function size(bytes) {
  const units = ["bytes", "KB", "MB", "GB"];
  let v = bytes, u = 0;
  while (v >= 1000 && u < units.length - 1) { v /= 1000; u++; }
  return u === 0 ? `${bytes} bytes` : `${v < 10 ? v.toFixed(1) : Math.round(v)} ${units[u]}`;
}

/** Theme colour keys in words (Settings → Colours). */
const LABEL = {
  background: "Background", dark_background: "Terminal black", darker_background: "Darkest background",
  lighter_background: "Panels and tabs", selection: "Selection", foreground: "Text",
  dark_foreground: "Secondary text", light_foreground: "Light text", bright_foreground: "Brightest text",
  muted: "Comments", accent: "Accent", red: "Red", yellow: "Yellow", green: "Green", cyan: "Cyan",
  blue: "Blue", magenta: "Magenta", orange: "Orange", brown: "Brown", bright_red: "Bright red",
  bright_yellow: "Bright yellow", bright_green: "Bright green", bright_cyan: "Bright cyan",
  bright_blue: "Bright blue", bright_magenta: "Bright magenta",
};

const LABELS = {
  sketchybar: ["SketchyBar", "Bar colours"],
  "sketchybar-item": ["SketchyBar theme pill", "The Palette pill in your bar"],
  borders: ["JankyBorders", "Window borders"],
  wallpaper: ["Wallpaper", "The theme's wallpaper"],
  appearance: ["Light / dark mode", "macOS appearance follows the theme"],
  ghostty: ["Ghostty"], kitty: ["kitty"], alacritty: ["Alacritty"], tmux: ["tmux"],
  wezterm: ["WezTerm", "Colours from one line before `return config` in wezterm.lua"],
  vscodium: ["VSCodium"], neovim: ["Neovim"], btop: ["btop"],
  obsidian: ["Obsidian", "A colour snippet in every vault, on top of your theme"],
  shell: ["Shell", "Loads fzf, bat, starship, lazygit colours in new terminals"],
  starship: ["Starship"], fzf: ["fzf"], bat: ["bat"], lazygit: ["lazygit"],
  "habit-tracker": ["Habit Tracker", "Colours file the app follows (turn on in its Settings → Integrations)"],
};
const GROUPS = [
  ["Desktop", ["sketchybar", "sketchybar-item", "borders", "wallpaper", "appearance"]],
  ["Terminals", ["ghostty", "kitty", "alacritty", "wezterm", "tmux"]],
  ["Editors", ["vscodium", "neovim", "obsidian"]],
  ["Command-line tools", ["shell", "starship", "fzf", "bat", "lazygit", "btop"]],
  ["Apps", ["habit-tracker"]],
];

// ---------------------------------------------------------------- welcome tour

/** Five steps on first launch (and General → Welcome tour). Each step only
 *  calls the same commands the rest of Settings uses. */
const welcome = {
  step: 0,
  STEPS: ["hello", "theme", "apps", "walls", "ready"],

  async render() {
    $("#wl-dots").replaceChildren(...this.STEPS.map((_, i) => el("i", { class: i <= this.step ? "on" : "" })));
    $("#wl-back").hidden = this.step === 0;
    $("#wl-next").textContent = this.step === this.STEPS.length - 1 ? "Done" : "Next";
    $("#wl-body").replaceChildren(...await this[this.STEPS[this.step]]());
  },

  go(by) {
    if (this.step + by >= this.STEPS.length) return this.finish();
    this.step = Math.max(0, this.step + by);
    this.render();
  },

  async finish() {
    try { await invoke("set_welcomed", { done: true }); } catch (e) { toast(String(e), true); }
    this.step = 0;
    settings.show("apps");
  },

  async hello() {
    const key = pretty((await invoke("prefs")).hotkey);
    return [
      el("h1", {}, "Welcome to Palette"),
      el("p", { class: "lead" }, "One theme restyles your Mac: the wallpaper, light or dark mode, and every app Palette knows how to colour. This takes a minute."),
      el("ul", { class: "facts" },
        el("li", {}, el("b", {}, "Menu bar"), el("span", {}, "Palette lives in the menu bar (the palette icon; it can hide behind the notch). Click it for the theme picker.")),
        el("li", {}, el("b", {}, key), el("span", {}, "Quick switch from anywhere: ↑↓ previews themes, ←→ wallpapers, ⏎ applies.")),
        el("li", {}, el("b", {}, "Safe"), el("span", {}, "Palette only adds one line to an app's settings, backs the file up first, and can take it out again. Nothing leaves your Mac."))),
    ];
  },

  async theme() {
    await load();
    const grid = el("div", { class: "mini-grid" }, ...themes.filter((t) => !t.invalid).map((t) => {
      const im = el("div", { class: "im", style: { background: `linear-gradient(135deg, ${t.colors.background} 55%, ${t.colors.accent} 55%)` } });
      thumb(t.name).then((src) => { if (src) im.style.backgroundImage = `url("${src}")`; });
      return el("button", {
        class: `mini${t.current ? " current" : ""}`, title: t.name,
        onclick: async () => {
          toast(`Applying ${t.name}…`);
          try { report(await invoke("apply", { name: t.name })); } catch (e) { toast(String(e), true); }
          this.render();
        },
      }, im, el("div", { class: "nm" }, t.name));
    }));
    return [
      el("h1", {}, "Pick a theme"),
      el("p", { class: "note" }, "Click one to try it now; you can change it any time. Your wallpaper and light/dark mode follow straight away. Over a hundred more (with wallpapers) are in Settings → Get themes."),
      grid,
    ];
  },

  async apps() {
    const rows = (await invoke("apps")).filter((r) => r.installed && !["wallpaper", "appearance", "shell", "sketchybar-item"].includes(r.name));
    const label = (n) => (LABELS[n] || [n])[0];
    const todo = rows.filter((r) => r.on && !r.wired);
    if (!rows.length) {
      return [
        el("h1", {}, "Your apps"),
        el("p", { class: "lead" }, "Palette didn't find any of the apps it themes yet, so for now it changes your wallpaper and light/dark mode."),
        el("p", { class: "note" }, "Install one later (Ghostty, kitty, VSCodium, Neovim, Obsidian, btop, starship…) and it shows up in Settings → Apps with a Set up button."),
      ];
    }
    const btn = el("button", { class: "btn primary" }, todo.length ? `Set up ${todo.length} app${todo.length === 1 ? "" : "s"}` : "All set up");
    btn.disabled = !todo.length;
    btn.onclick = async () => {
      btn.disabled = true;
      btn.textContent = "Setting up…";
      try {
        const r = await invoke("setup_all", { remove: false });
        const manual = r.lines.filter((l) => l.what.startsWith("needs you"));
        toast(manual.length ? manual.map((l) => `${l.app}: ${l.what}`).join(" · ") : "Done; originals backed up", manual.length > 0);
      } catch (e) { toast(String(e), true); }
      this.render();
    };
    return [
      el("h1", {}, "Your apps"),
      el("p", { class: "lead" }, `Palette found ${rows.length} app${rows.length === 1 ? "" : "s"} it can theme${todo.length ? `; ${todo.length} still need${todo.length === 1 ? "s" : ""} a line added to its settings` : ", all hooked up"}.`),
      el("div", { class: "found" }, ...rows.map((r) => el("span", { class: r.wired ? "wired" : "", title: r.wired ? "themed" : "not set up yet" }, label(r.name)))),
      btn,
      el("p", { class: "note", style: { marginTop: "10px" } }, "Each app's file is backed up first. To keep an app on its own look, switch it off in Settings → Apps."),
    ];
  },

  async walls() {
    let w = null;
    try { w = await invoke("wallpapers", {}); } catch { /* no theme yet */ }
    const n = w?.images.length ?? 0;
    const add = el("button", { class: "btn" }, "Add images…");
    add.onclick = async () => {
      try {
        const k = await invoke("add_wallpapers", { theme: w.theme });
        if (k) toast(`Added ${k} to ${w.theme}`);
      } catch (e) { toast(String(e), true); }
      this.render();
    };
    return [
      el("h1", {}, "Wallpapers"),
      el("p", { class: "lead" }, n
        ? `${w.theme} has ${n} wallpaper${n === 1 ? "" : "s"}. Each theme keeps its own, and switching theme switches wallpaper.`
        : `${w ? w.theme : "This theme"} has no wallpapers yet. Each theme keeps its own; add some and switching theme switches wallpaper too.`),
      el("ul", { class: "facts" },
        el("li", {}, el("b", {}, "Picker"), el("span", {}, "‹ Wallpaper › at the bottom flips through the theme's wallpapers.")),
        el("li", {}, el("b", {}, "Quick switch"), el("span", {}, "←→ picks one before you apply a theme.")),
        el("li", {}, el("b", {}, "Settings"), el("span", {}, "Wallpapers tab: add or remove images, ★ the one a theme starts with."))),
      w ? el("div", { class: "actions", style: { marginTop: "14px" } }, add) : null,
    ];
  },

  async ready() {
    const p = await invoke("prefs");
    const login = el("input", { type: "checkbox", class: "switch" });
    login.checked = p.login;
    login.onchange = async () => {
      try { login.checked = await invoke("set_login", { on: login.checked }); } catch (e) { toast(String(e), true); }
    };
    return [
      el("h1", {}, "You're set"),
      el("p", { class: "lead" }, `Press ${pretty(p.hotkey)} any time to switch. Everything else is in Settings: apps, wallpapers, moving to another Mac.`),
      el("label", { class: "row" }, el("div", {}, el("div", { class: "title" }, "Open at login"), el("div", { class: "sub" }, "Starts quietly in the menu bar")), login),
    ];
  },
};

// ---------------------------------------------------------------- schedule

const schedule = {
  s: null,

  async render() {
    try { this.s = await invoke("schedule_get"); } catch (e) { toast(String(e), true); return; }
    this.fillThemes();
    this.paint();
    await this.search($("#sc-q").value);
  },

  fillThemes() {
    for (const [id, key] of [["#sc-day", "day"], ["#sc-night", "night"]]) {
      const names = themes.filter((t) => !t.invalid).map((t) => t.name);
      $(id).replaceChildren(el("option", { value: "" }, "Choose a theme…"),
        ...names.map((n) => { const o = el("option", { value: n }, n); if (n === this.s[key]) o.selected = true; return o; }));
    }
  },

  paint() {
    const s = this.s;
    $("#sc-on").checked = s.on;
    $("#sc-mode").value = s.mode;
    $("#sc-sun").hidden = s.mode !== "sun";
    $("#sc-fixed").hidden = s.mode !== "fixed";
    $("#sc-day-at").value = s.day_at;
    $("#sc-night-at").value = s.night_at;
    $("#sc-lat").value = s.lat ?? "";
    $("#sc-lon").value = s.lon ?? "";
    $("#sc-place").textContent = s.place || "Not chosen yet";
    const sug = $("#sc-suggest");
    sug.hidden = !s.suggestion || (s.city === s.suggestion && s.lat == null);
    sug.textContent = s.suggestion ? `Use ${s.suggestion}` : "";
    sug.title = "Suggested from this Mac's time zone";
    $("#sc-status").textContent = s.on ? `On: ${s.phase === "night" ? "night" : "day"} theme now` : "Off";
    $("#sc-today").textContent = s.today || s.problem || "-";
    $("#sc-next").textContent = s.next || (s.problem ? "Choose a place or times first" : "None within a year");
  },

  async search(q) {
    const rows = await invoke("schedule_cities", { query: q });
    $("#sc-cities").replaceChildren(...rows.map((c) => {
      const o = el("option", { value: c.name }, `${c.name}, ${c.country}`);
      if (c.name === this.s?.city && this.s.lat == null) o.selected = true;
      return o;
    }));
  },

  /** Save everything on the page; the library checks it and switches if due. */
  async save(extra = {}) {
    const input = {
      on: $("#sc-on").checked,
      day: $("#sc-day").value,
      night: $("#sc-night").value,
      mode: $("#sc-mode").value,
      city: this.s.city,
      lat: this.s.lat,
      lon: this.s.lon,
      day_at: $("#sc-day-at").value || "07:00",
      night_at: $("#sc-night-at").value || "19:00",
      ...extra,
    };
    try {
      this.s = await invoke("schedule_save", { input });
      toast(input.on ? "Schedule saved" : "Schedule off");
    } catch (e) {
      toast(String(e), true);
      try { this.s = await invoke("schedule_get"); } catch { /* keep the old view */ }
    }
    this.paint();
    await load();
  },
};

const settings = {
  tab: "apps",

  show(tab) {
    this.tab = tab;
    document.querySelectorAll(".tab").forEach((b) => b.classList.toggle("on", b.dataset.tab === tab));
    document.querySelectorAll("[data-pane]").forEach((p) => (p.hidden = p.dataset.pane !== tab));
    if (tab === "wallpapers") this.renderWallpapers();
    // Every time, not just the first: wallpapers, apps or the theme may have
    // changed in another tab since the last check.
    if (tab === "setup") this.check();
    if (tab === "welcome") welcome.render();
    if (tab === "themes") this.renderThemes();
    if (tab === "schedule") schedule.render();
    if (tab === "tools") this.renderTools();
    if (tab.startsWith("colours")) this.renderColours(tab.split(":")[1]);
    if (["bar", "windows", "borders", "prompt"].includes(tab)) desktop.render(tab);
    if (tab === "macos") desktop.macos();
  },

  // ---------------- colours
  coTheme: null,
  co: null, // the sheet: colours, verdict, issues
  coTimer: null,

  async renderColours(name) {
    // `colours:<name>` (from a picker badge) or the tab: keep the last choice.
    this.tab = "colours";
    document.querySelectorAll(".tab").forEach((b) => b.classList.toggle("on", b.dataset.tab === "colours"));
    document.querySelectorAll("[data-pane]").forEach((p) => (p.hidden = p.dataset.pane !== "colours"));
    if (!themes.length) await load();
    if (name) this.coTheme = name;
    if (!this.coTheme || !themes.some((t) => t.name === this.coTheme && !t.invalid)) {
      this.coTheme = themes.find((t) => t.current)?.name || themes.find((t) => !t.invalid)?.name;
    }
    $("#co-theme").replaceChildren(...themes.filter((t) => !t.invalid).map((t) => {
      const o = el("option", { value: t.name }, `${t.name}${t.current ? " (active)" : ""}`);
      if (t.name === this.coTheme) o.selected = true;
      return o;
    }));
    await this.coFetch();
    this.coGrid();
  },

  async coFetch() {
    try { this.co = await invoke("colour_sheet", { name: this.coTheme }); }
    catch (e) { $("#co-check").replaceChildren(el("div", { class: "empty" }, String(e))); return; }
    this.coPaint();
  },

  /** A colour by key, as shown now (with unsaved picks). */
  coGet(key) { return this.co.colours.find((c) => c.key === key)?.value; },

  /** Everything but the grid (so an open colour panel keeps its swatch). */
  coPaint() {
    const s = this.co;
    const t = themes.find((x) => x.name === s.theme);
    const edited = s.colours.filter((c) => c.value !== c.original).length;
    $("#co-sub").textContent = `${s.mode} · ${edited ? `${edited} colour${edited === 1 ? "" : "s"} changed by you` : "as its author made it"}${t?.current ? " · active, apps update as you edit" : ""}`;
    $("#co-reset").disabled = !edited;
    this.coPreview();
    const chip = (fg, on, label = "Aa") => el("span", { class: "co-chip", style: { color: fg, background: on } }, label);
    const box = $("#co-check");
    if (!s.issues.length) {
      return box.replaceChildren(el("div", { class: "row" },
        el("div", {}, el("div", { class: "title" }, "✓ Everything is readable"),
          el("div", { class: "sub" }, s.verdict.replace(`${s.theme}: `, "")))));
    }
    const all = this.btn("Apply all suggestions", () => this.coEdit({ how: "fix" }), "btn primary");
    const rows = s.issues.map((i) => el("div", { class: "row co-issue" },
      chip(i.fg_hex, i.bg_hex),
      el("div", {},
        el("div", { class: "title" }, `${LABEL[i.fg] || i.fg} on ${(LABEL[i.bg] || i.bg).toLowerCase()}: ${i.ratio.toFixed(1)}:1, needs ${i.min}`),
        el("div", { class: "sub" }, `${i.used_for}${i.applied ? ". Palette already lifts it where it can" : ""}`)),
      el("span", { class: "co-arrow" }, "→"),
      chip(i.suggested, i.bg_hex),
      el("code", {}, i.suggested),
      this.btn("Apply", () => this.coEdit({ how: "set", colours: [[i.fg, i.suggested]] }))));
    box.replaceChildren(
      el("div", { class: "row" },
        el("div", {}, el("div", { class: "title" }, s.verdict.replace(`${s.theme}: `, "")),
          el("div", { class: "sub" }, "Each suggestion keeps the colour's hue and only makes it lighter or darker until it reads well")),
        all),
      ...rows);
  },

  /** A small editor and terminal, painted with the colours as they stand. */
  coPreview() {
    const c = Object.fromEntries(this.co.colours.map((x) => [x.key, x.value]));
    const span = (key, text, extra = {}) => el("span", { style: { color: c[key], ...extra } }, text);
    const line = (n, ...parts) => el("div", { class: "ln" }, el("i", { style: { color: c.dark_foreground } }, n), ...parts);
    $("#co-preview").replaceChildren(
      el("div", { class: "co-tabs", style: { background: c.darker_background } },
        el("span", { style: { background: c.background, color: c.foreground, borderTop: `2px solid ${c.accent}` } }, "main.rs"),
        el("span", { style: { background: c.lighter_background, color: c.dark_foreground } }, "notes.md")),
      el("div", { class: "co-code", style: { background: c.background, color: c.foreground } },
        line("1", span("muted", "// colours from your theme")),
        line("2", span("magenta", "fn "), span("blue", "main"), "() {"),
        line("3", "    ", span("magenta", "let "), "name = ", span("green", "\"palette\""), ";"),
        el("div", { class: "ln", style: { background: c.selection } },
          el("i", { style: { color: c.dark_foreground } }, "4"),
          span("bright_foreground", "    selected line, "), span("yellow", "42"), span("bright_foreground", " items")),
        line("5", "    ", span("cyan", "println!"), "(", span("orange", "\"{name}\""), ");"),
        line("6", "}"),
        el("div", { class: "co-term", style: { background: c.dark_background } },
          span("accent", "~/palette "), span("magenta", "main "), span("green", "❯ "), "cargo test",
          el("br"), span("red", "error"), ": one failed  ", span("yellow", "warning"), ": unused  ", span("green", "ok"), span("muted", "  (2 ignored)"),
          el("br"), ...["red", "yellow", "green", "cyan", "blue", "magenta"].map((k) => span(k, "■ ")),
          ...["bright_red", "bright_yellow", "bright_green", "bright_cyan", "bright_blue", "bright_magenta"].map((k) => span(k, "■ ")))));
  },

  coGrid() {
    const groups = [
      ["Backgrounds", ["background", "dark_background", "darker_background", "lighter_background", "selection"]],
      ["Text", ["foreground", "dark_foreground", "light_foreground", "bright_foreground", "muted"]],
      ["Accent and colours", ["accent", "red", "yellow", "green", "cyan", "blue", "magenta", "orange", "brown"]],
      ["Bright colours", ["bright_red", "bright_yellow", "bright_green", "bright_cyan", "bright_blue", "bright_magenta"]],
    ];
    const parts = [];
    for (const [title, keys] of groups) {
      parts.push(el("h3", {}, title));
      parts.push(el("div", { class: "co-grid" }, ...keys.filter((k) => this.coGet(k)).map((k) => this.coSwatch(k))));
    }
    $("#co-grid").replaceChildren(...parts);
  },

  coSwatch(key) {
    // Looked up each time: every save fetches a fresh sheet.
    const now = () => this.co.colours.find((c) => c.key === key);
    const sw = now();
    const pick = el("input", { type: "color", value: sw.value, title: "Open the colour wheel" });
    const hex = el("input", { class: "search co-hex", type: "text", value: sw.value, spellcheck: "false", maxlength: "7" });
    const reset = el("button", { class: "btn co-undo", title: `Back to the original, ${sw.original}` }, "↺");
    const item = el("div", { class: `co-sw${sw.value !== sw.original ? " edited" : ""}`, "data-key": key },
      pick,
      el("div", { class: "co-name" }, el("div", { class: "title" }, LABEL[key] || key), el("div", { class: "sub" }, key)),
      hex, reset);
    const local = (v) => {
      now().value = v;
      pick.value = v;
      hex.value = v;
      item.classList.toggle("edited", v !== sw.original);
      this.coPreview();
    };
    // While the colour panel is open, `input` fires as you drag: paint the
    // preview at once and save when you pause, so apps reload once, not per pixel.
    pick.addEventListener("input", () => {
      local(pick.value);
      clearTimeout(this.coTimer);
      this.coTimer = setTimeout(() => this.coEdit({ how: "set", colours: [[key, pick.value]] }, false), 600);
    });
    hex.addEventListener("change", () => {
      let v = hex.value.trim().toLowerCase();
      if (/^[0-9a-f]{6}$/.test(v)) v = `#${v}`;
      if (!/^#[0-9a-f]{6}$/.test(v)) { hex.value = now().value; return toast("Type a colour as #rrggbb, like #7aa2f7", true); }
      local(v);
      this.coEdit({ how: "set", colours: [[key, v]] }, false);
    });
    reset.onclick = () => { local(sw.original); this.coEdit({ how: "reset", key }, false); };
    return item;
  },

  /** Save a change, then refresh the report (and the grid, unless the
   *  change came from it: rebuilding would close an open colour panel). */
  async coEdit(args, regrid = true) {
    clearTimeout(this.coTimer);
    try {
      const [changed, problems] = await invoke("edit_colours", { name: this.coTheme, ...args });
      if (problems.length) toast(problems.join(" · "), true);
      else if (args.how === "fix") toast(`${changed.length} colour${changed.length === 1 ? "" : "s"} adjusted`);
      else if (args.how === "reset" && !args.key) toast(`${this.coTheme} is back to its original colours`);
    } catch (e) { toast(String(e), true); }
    await load(); // the picker's badges and this window's own colours
    await this.coFetch();
    // A change from a swatch only touched that key, and the swatch already
    // shows it (maybe a newer pick, mid-drag): leave the grid alone.
    if (regrid) this.coGrid();
  },

  // ---------------- get the tools
  async renderTools() {
    const box = $("#tools");
    let s;
    try { s = await invoke("tools_status"); } catch (e) { return box.replaceChildren(el("div", { class: "empty" }, String(e))); }
    const parts = [];
    if (!s.homebrew) {
      parts.push(el("div", { class: "brew-missing" },
        el("span", {}, "Homebrew isn't installed, and the commands below need it. Get it from brew.sh first; Palette never installs it for you."),
        this.btn("Open brew.sh", async () => { try { await invoke("open_homebrew_site"); } catch (e) { toast(String(e), true); } })));
    }
    for (const [group, title] of [["look", "The look"], ["wallpaper", "Wallpaper recolourer"]]) {
      parts.push(el("h2", {}, title));
      for (const t of s.tools.filter((x) => x.group === group)) parts.push(this.toolCard(t));
    }
    box.replaceChildren(...parts);
  },

  cmdBlock(lines) {
    return el("div", { class: "cmd" }, lines.join("\n"));
  },

  toolCard(t) {
    const copy = (lines) => async () => {
      try { await invoke("copy_text", { text: lines.join(" && ") }); toast("Copied: paste it into Terminal"); }
      catch (e) { toast(String(e), true); }
    };
    const terminal = () => this.btn("Open Terminal", async () => { try { await invoke("open_terminal"); } catch (e) { toast(String(e), true); } });
    const card = el("div", { class: "tool" },
      el("div", { class: "head" },
        el("div", {}, el("div", { class: "title" }, t.name), el("div", { class: "sub" }, t.what)),
        el("span", { class: `state${t.installed ? " on" : ""}` }, t.installed ? "✓ Installed" : "Not installed")));
    // The command for what is missing; the other one tucked away.
    const main = t.installed ? ["Remove", t.remove] : ["Install", t.install];
    const other = t.installed ? ["Install", t.install] : ["Remove", t.remove];
    card.append(
      el("div", { class: "sub" }, `${main[0]} with Homebrew, in Terminal:`),
      this.cmdBlock(main[1]),
      el("div", { class: "actions" }, this.btn("Copy", copy(main[1])), terminal()),
      el("details", {}, el("summary", {}, `${other[0]} command`), this.cmdBlock(other[1]),
        el("div", { class: "actions" }, this.btn("Copy", copy(other[1])))));
    const note = (text, ...btns) => card.append(el("div", { class: "starter" }, el("span", {}, text), ...btns));
    if (t.starter === "available") {
      note("No config yet. Palette can add a small starter, themed with your colours; it never overwrites a config.",
        this.btn("Add a starter config", (b) => this.starter(t, b, "add_starter", "Adding…")));
    } else if (t.starter === "has_config") {
      note(`You already have a config (${t.config}), so Palette adds nothing. It only wires its colours in with Set up.`);
    } else if (t.starter === "added") {
      note("Palette's starter config is in place, unchanged. Removing it moves the files to the Trash.",
        this.btn("Remove starter config", (b) => this.starter(t, b, "remove_starter", "Moving…", true)));
    } else if (t.starter === "added_edited") {
      note(`Palette added a starter here, and you have edited it since, so it's yours now (${t.config}).`);
    }
    return card;
  },

  async starter(t, btn, command, busy, confirm = false) {
    if (confirm && !armed(btn, "Click again")) return toast(`Click again to move ${t.name}'s starter config to the Trash`);
    const label = btn.textContent;
    btn.disabled = true;
    btn.textContent = busy;
    try { toast(await invoke(command, { tool: t.id })); } catch (e) { toast(String(e), true); }
    btn.textContent = label;
    btn.disabled = false;
    this.renderTools();
  },

  // ---------------- get themes
  catalog: null,

  async renderThemes() {
    if (!this.catalog) {
      try { this.catalog = await invoke("catalog"); } catch (e) { toast(String(e), true); return; }
    }
    const q = $("#gt-search").value.trim().toLowerCase();
    const f = $("#gt-filter").value;
    const shown = this.catalog.filter((t) =>
      (t.name.includes(q) || t.title.toLowerCase().includes(q) || t.author.toLowerCase().includes(q)) &&
      (!f || t.mood === f || (f === "light" && t.mode === "light") || (f === "installed" && t.installed)));
    $("#gt-grid").replaceChildren(...shown.map((t) => this.themeCard(t)));
    if (!shown.length) $("#gt-grid").replaceChildren(el("div", { class: "empty" }, "No theme matches."));
    // Installed from a link (not in the list).
    const known = new Set(this.catalog.map((t) => t.name));
    const others = (await invoke("installed_themes")).filter(([n]) => !known.has(n));
    $("#gt-others").replaceChildren(...others.map(([name, url]) => el("div", { class: "row" },
      el("div", {}, el("div", { class: "title" }, name), el("div", { class: "sub" }, url)),
      this.btn("Update", (b) => this.installTheme(name, b, true)),
      this.btn("Remove…", (b) => this.removeTheme(name, b)))));
  },

  btn(label, onclick, cls = "btn") {
    const b = el("button", { class: cls }, label);
    b.onclick = () => onclick(b);
    return b;
  },

  themeCard(t) {
    const c = t.colors;
    const sw = el("div", { class: "sw", style: { background: `radial-gradient(circle at 80% 20%, ${c.accent}66, transparent 55%), linear-gradient(135deg, ${c.background}, ${c.darker_background})` } },
      el("span", { class: "txt" }, el("span", { style: { color: c.accent } }, "❯ "), el("span", { style: { color: c.foreground } }, t.name)),
      el("div", { class: "strip" }, ...["accent", "red", "yellow", "green", "cyan", "blue", "magenta"].map((k) => el("i", { style: { background: c[k] } }))));
    const card = el("div", { class: "gt", title: `${t.title} by ${t.author}\n${t.url}` }, sw);
    const acts = el("div", { class: "acts" });
    if (t.installed) {
      acts.append(
        this.btn("Use", async () => { try { report(await invoke("apply", { name: t.name })); toast(`Now ${t.name}`); } catch (e) { toast(String(e), true); } }, "btn primary"),
        this.btn("Update", (b) => this.installTheme(t.name, b, true, card)));
      const x = this.btn("×", (b) => this.removeTheme(t.name, b), "btn icon");
      x.title = "Remove this theme (click twice)";
      acts.append(x);
    } else {
      acts.append(this.btn("Install", (b) => this.installTheme(t.name, b, false, card), "btn primary"));
    }
    if (!t.installed) acts.append(el("span", { class: "badge" }, t.mode === "light" ? "light" : t.mood));
    card.append(el("div", { class: "meta" }, el("div", { class: "ttl" }, t.title || t.name), el("div", { class: "by" }, `by ${t.author}`), contrastBadge(t, true), acts));
    return card;
  },

  async installTheme(source, btn, update = false, card = null) {
    const label = btn.textContent;
    btn.textContent = update ? "Updating…" : "Downloading…";
    card?.classList.add("busy");
    let failed = null;
    try {
      const r = await invoke("install_theme", { source, update });
      toast(`${update ? "Updated" : "Installed"} ${r.name}${r.wallpapers ? ` with ${r.wallpapers} wallpaper${r.wallpapers === 1 ? "" : "s"}` : ""}`);
    } catch (e) { failed = String(e); toast(failed, true); btn.textContent = label; }
    card?.classList.remove("busy");
    this.catalog = null;
    await load();
    this.renderThemes();
    return failed;
  },

  /** A link that isn't a theme: offer to browse it as a wallpaper collection. */
  offerWallpapers(url, why) {
    const box = $("#gt-offer");
    box.hidden = false;
    box.replaceChildren(
      el("div", {}, el("b", {}, "This looks like a wallpaper collection"), ` — ${why}`),
      el("div", { class: "actions" },
        el("button", {
          class: "btn primary",
          onclick: () => {
            box.hidden = true;
            this.show("wallpapers");
            $("#gl-url").value = url;
            this.linkLook();
          },
        }, "Browse its images"),
        el("button", { class: "btn", onclick: () => (box.hidden = true) }, "Not now")));
  },

  async removeTheme(name, btn) {
    if (!armed(btn, "Click again")) return toast(`Click again to delete ${name} and its wallpapers`);
    try { const r = await invoke("remove_theme", { name }); toast(r.message, r.warning); } catch (e) { toast(String(e), true); }
    this.catalog = null;
    await load();
    this.renderThemes();
  },

  // ---------------- wallpapers
  wpTheme: null,

  async renderWallpapers() {
    const select = $("#wp-theme");
    const cur = themes.find((t) => t.current);
    if (!this.wpTheme) this.wpTheme = cur?.name;
    select.replaceChildren(...themes.filter((t) => !t.invalid).map((t) => {
      const o = el("option", { value: t.name }, `${t.name}${t.current ? " (active)" : ""}`);
      if (t.name === this.wpTheme) o.selected = true;
      return o;
    }));
    let w;
    try {
      w = await invoke("wallpapers", { theme: this.wpTheme });
    } catch (e) {
      $("#wp-grid").replaceChildren(el("div", { class: "empty" }, String(e)));
      return;
    }
    $("#wp-sub").textContent = `${w.images.length} wallpaper${w.images.length === 1 ? "" : "s"}${w.active ? " · click one to use it now" : " · switch to this theme to see them on the desktop"}`;
    $("#wp-next").disabled = !w.active || w.images.length < 2;
    $("#wp-dir").textContent = w.dir;
    $("#wp-size").textContent = `· ${size(w.bytes)}`;
    $("#wp-dir").title = w.dir;
    this.renderLibrary();
    const grid = $("#wp-grid");
    if (!w.images.length) {
      grid.replaceChildren(el("div", { class: "empty" }, "No wallpapers yet: Add images…, or add some from your library below"));
      return;
    }
    grid.replaceChildren(...w.images.map((img) => {
      const tile = el("div", {
        class: `wp${img.current ? " current" : ""}`,
        title: img.name,
        onclick: () => this.useWallpaper(w.theme, img.name),
      },
        el("div", { class: "label" }, img.name),
        el("button", {
          class: `star${img.default ? " on" : ""}`,
          title: img.default ? `${w.theme} starts with this one (click: back to the first)` : `Start ${w.theme} with this wallpaper`,
          onclick: async (e) => {
            e.stopPropagation();
            try { await invoke("set_default_wallpaper", { theme: w.theme, name: img.default ? null : img.name }); }
            catch (err) { toast(String(err), true); }
            this.renderWallpapers();
          },
        }, img.default ? "★" : "☆"),
        el("button", {
          class: "x", title: "Remove from this theme (your original file stays)",
          onclick: (e) => { e.stopPropagation(); this.removeWallpaper(w.theme, img.name, e.target); },
        }, "×"));
      invoke("wallpaper_thumb", { theme: w.theme, name: img.name })
        .then((src) => (tile.style.backgroundImage = `url("${src}")`))
        .catch(() => {});
      return tile;
    }));
  },

  async useWallpaper(theme, name) {
    try {
      const problems = await invoke("set_wallpaper", { theme, name });
      if (problems.length) toast(problems.join(" · "), true);
    } catch (e) { toast(String(e), true); }
    this.renderWallpapers();
  },

  async removeWallpaper(theme, name, btn) {
    if (!armed(btn, "?")) return toast(`Click × again to remove ${name} from ${theme} (your original file stays)`);
    try { await invoke("remove_wallpaper", { theme, name }); } catch (e) { toast(String(e), true); }
    this.renderWallpapers();
  },

  /** Run `work` with `btn` disabled and showing `busy`. */
  async busy(btn, busy, work) {
    const label = btn.textContent;
    btn.disabled = true;
    btn.textContent = busy;
    try { await work(); } catch (e) { toast(String(e), true); }
    btn.disabled = false;
    btn.textContent = label;
  },

  async recolour() {
    const theme = this.wpTheme;
    const engine = $("#wp-engine").hidden ? "palette" : $("#wp-engine").value;
    const strength = $("#wp-strength").value;
    await this.busy($("#wp-recolour"), "Recolouring…", async () => {
      const made = await invoke("recolour_wallpapers", { theme, engine, strength });
      if (made.length) toast(`Added ${made.length === 1 ? made[0] : `${made.length} recoloured wallpapers`} to ${theme}; click one to use it`);
    });
    this.renderWallpapers();
  },

  // `variant`: paint a new random variant (palette-<style>-<seed>.jpg).
  async generate(variant = false) {
    const theme = this.wpTheme;
    const seed = variant ? Math.floor(Math.random() * 100000) : null;
    await this.busy($(variant ? "#wp-variant" : "#wp-generate"), "Painting…", async () => {
      const name = await invoke("generate_wallpaper", { theme, style: $("#wp-style").value, seed });
      toast(`Added ${name} to ${theme}; click it to use it`);
    });
    this.renderWallpapers();
  },

  // ---------------- wallpaper library
  libPicked: null,
  libOpen: new Set([""]), // folders shown open ("" = the top level)

  async renderLibrary() {
    let lib;
    try { lib = await invoke("library"); } catch (e) { $("#lib-grid").replaceChildren(el("div", { class: "empty" }, String(e))); return; }
    const n = lib.items.length;
    $("#lib-sub").textContent = `· ${n} picture${n === 1 ? "" : "s"}, ${size(lib.bytes)}`;
    $("#lib-dir").textContent = lib.dir;
    $("#lib-dir").title = lib.dir;
    if (!n) {
      $("#lib-grid").replaceChildren(el("div", { class: "empty" }, "Empty. Add images…, or get some from a link below."));
      $("#lib-act").hidden = true;
      return;
    }
    // One section per folder; thumbnails are made only for open ones.
    const byFolder = new Map();
    for (const item of lib.items) {
      if (!byFolder.has(item.folder)) byFolder.set(item.folder, []);
      byFolder.get(item.folder).push(item);
    }
    const sections = [...byFolder.keys()].sort().map((folder) => {
      const items = byFolder.get(folder);
      const grid = el("div", { class: "wp-grid" });
      const fill = () => {
        if (grid.childElementCount) return;
        grid.append(...items.map((item) => this.libTile(item)));
      };
      const bytes = items.reduce((s, i) => s + i.bytes, 0);
      const d = el("details", { class: "lib-folder" },
        el("summary", {}, el("span", {}, folder || "Top level"), el("span", { class: "sub" }, `${items.length} · ${size(bytes)}`)),
        grid);
      if (this.libOpen.has(folder)) { d.open = true; fill(); }
      d.addEventListener("toggle", () => {
        if (d.open) { this.libOpen.add(folder); fill(); } else this.libOpen.delete(folder);
      });
      return d;
    });
    $("#lib-grid").replaceChildren(...sections);
    if (this.libPicked && !lib.items.some((i) => i.name === this.libPicked)) { this.libPicked = null; $("#lib-act").hidden = true; }
  },

  libTile(item) {
    const label = item.name.split("/").pop();
    const tile = el("div", {
      class: `wp${this.libPicked === item.name ? " current" : ""}`,
      title: `${item.name} · ${size(item.bytes)}`,
      "data-name": item.name,
      onclick: () => this.libPick(item),
    }, el("div", { class: "label" }, label));
    invoke("library_thumb", { name: item.name })
      .then((src) => (tile.style.backgroundImage = `url("${src}")`))
      .catch(() => {});
    return tile;
  },

  /** Actions for one library picture, under the grid. */
  libPick(item) {
    this.libPicked = item.name;
    document.querySelectorAll("#lib-grid .wp").forEach((t) => t.classList.toggle("current", t.dataset.name === item.name));
    const box = $("#lib-act");
    box.hidden = false;
    const target = el("select", { class: "select", title: "Theme to add it to" },
      ...themes.filter((t) => !t.invalid).map((t) => {
        const o = el("option", { value: t.name }, `${t.name}${t.current ? " (active)" : ""}`);
        if (t.name === this.wpTheme) o.selected = true;
        return o;
      }));
    const recolour = el("input", { type: "checkbox" });
    const add = el("button", { class: "btn" }, "Add to theme");
    add.onclick = () => this.busy(add, recolour.checked ? "Recolouring…" : "Adding…", async () => {
      const made = await invoke("library_to_theme", { name: item.name, theme: target.value, recolour: recolour.checked });
      toast(`Added ${made} to ${target.value}`);
      await load();
      this.renderWallpapers();
    });
    const newTheme = el("button", { class: "btn" }, "New theme from this");
    newTheme.onclick = () => this.busy(newTheme, "Reading colours…", async () => {
      const d = await invoke("draft_theme_from_library", { name: item.name, light: null });
      if (!d) return;
      this.draft = d;
      this.ntRender();
      $("#nt-draft").scrollIntoView({ behavior: "smooth", block: "nearest" });
    });
    const remove = el("button", { class: "btn danger" }, "Remove");
    remove.onclick = async () => {
      if (!armed(remove, "Click again")) return toast(`Click again to delete ${item.name} from your library (themes keep their copies)`);
      try { await invoke("library_remove", { name: item.name }); toast(`Removed ${item.name.split("/").pop()}`); } catch (e) { toast(String(e), true); }
      this.libPicked = null;
      box.hidden = true;
      this.renderLibrary();
    };
    box.replaceChildren(
      el("div", { class: "lib-name" }, el("b", {}, item.name.split("/").pop()), el("span", { class: "sub" }, ` ${item.folder ? `${item.folder} · ` : ""}${size(item.bytes)}`)),
      el("div", { class: "lib-row" },
        el("button", {
          class: "btn primary",
          onclick: async () => {
            try {
              const problems = await invoke("library_use", { name: item.name });
              toast(problems.length ? problems.join(" · ") : "Using it until the next theme switch", problems.length > 0);
            } catch (e) { toast(String(e), true); }
          },
        }, "Use now"),
        newTheme,
        el("span", { class: "spacer" }),
        remove),
      el("div", { class: "lib-row" },
        target,
        el("label", { class: "check" }, recolour, "recolour to it"),
        add));
  },

  async libAdd() {
    try {
      const n = await invoke("library_add");
      if (n) toast(`Added ${n} picture${n === 1 ? "" : "s"} to your library`);
    } catch (e) { toast(String(e), true); }
    this.renderLibrary();
  },

  // ---------------- get wallpapers from a link
  link: null, // { url, listing, chosen: Set of paths }

  async linkLook() {
    const url = $("#gl-url").value.trim();
    if (!url) return toast("Paste a link first: a GitHub repo or folder, or a picture");
    const box = $("#gl-out");
    await this.busy($("#gl-go"), "Looking…", async () => {
      const listing = await invoke("wallpaper_link", { url });
      this.link = { url, listing, chosen: new Set(), open: new Set() };
      this.linkRender();
      box.scrollIntoView({ behavior: "smooth", block: "nearest" });
    });
  },

  linkRender() {
    const { listing: l, chosen } = this.link;
    const box = $("#gl-out");
    box.hidden = false;
    const files = l.files;
    const head = [];
    if (l.kind === "image") {
      head.push(el("div", {}, "One picture: ", el("b", {}, files[0].path)));
      files.forEach((f) => chosen.add(f.path));
    } else {
      const total = files.reduce((s, f) => s + f.bytes, 0);
      head.push(el("div", {}, el("b", {}, l.title), ` · ${files.length} picture${files.length === 1 ? "" : "s"}, ${size(total)} in ${l.folders.length} folder${l.folders.length === 1 ? "" : "s"}`));
      if (l.truncated) head.push(el("div", { class: "warn" }, "GitHub listed only part of this repo (it's very big). Paste a link to one folder (open it on GitHub and copy the address) to see all of it."));
      if (!files.length) head.push(el("div", { class: "empty" }, "No pictures there (png, jpg, webp, heic, tiff)."));
    }
    const list = el("div", { class: "gl-list" });
    if (l.kind === "github") {
      for (const folder of l.folders) {
        const inside = files.filter((f) => (f.path.includes("/") ? f.path.slice(0, f.path.lastIndexOf("/")) : "") === folder.path);
        const all = inside.every((f) => chosen.has(f.path));
        const some = inside.some((f) => chosen.has(f.path));
        const box2 = el("input", { type: "checkbox", title: "Choose the whole folder" });
        box2.checked = all;
        box2.indeterminate = some && !all;
        box2.onclick = (e) => {
          e.stopPropagation();
          inside.forEach((f) => (box2.checked ? chosen.add(f.path) : chosen.delete(f.path)));
          this.linkRender();
        };
        const d = el("details", { class: "gl-folder" },
          el("summary", {}, box2, el("span", { class: "name" }, folder.path || "Top level"), el("span", { class: "sub" }, `${folder.count} · ${size(folder.bytes)}`)));
        const fill = () => {
          if (d.childElementCount > 1) return;
          d.append(...inside.map((f) => {
            const c = el("input", { type: "checkbox" });
            c.checked = chosen.has(f.path);
            c.onchange = () => { c.checked ? chosen.add(f.path) : chosen.delete(f.path); this.linkRender(); };
            return el("label", { class: "gl-file" }, c, el("span", { class: "name" }, f.path.split("/").pop()), el("span", { class: "sub" }, size(f.bytes)));
          }));
        };
        if (this.link.open.has(folder.path)) { d.open = true; fill(); }
        d.addEventListener("toggle", () => {
          if (d.open) { this.link.open.add(folder.path); fill(); } else this.link.open.delete(folder.path);
        });
        list.append(d);
      }
    }
    const picked = files.filter((f) => chosen.has(f.path));
    const bytes = picked.reduce((s, f) => s + f.bytes, 0);
    const over = picked.length > l.max_files || bytes > l.max_bytes;
    const theme = el("select", { class: "select", id: "gl-theme", title: "Also add them to a theme (optional)" },
      el("option", { value: "" }, "Library only"),
      ...themes.filter((t) => !t.invalid).map((t) => el("option", { value: t.name }, `+ ${t.name}`)));
    if (this.link.theme) theme.value = this.link.theme;
    theme.onchange = () => (this.link.theme = theme.value);
    const recolour = el("label", { class: "check" }, el("input", { type: "checkbox", id: "gl-recolour" }), "recolour");
    const go = el("button", { class: "btn primary", id: "gl-download" }, l.kind === "image" ? "Download" : "Download selected");
    go.disabled = !picked.length || over;
    go.onclick = () => this.linkDownload(go);
    const summary = l.kind === "image" ? `Saves to ${l.into}/ in your library`
      : over ? `${picked.length} chosen (${size(bytes)}): at most ${l.max_files} pictures / ${size(l.max_bytes)} at a time`
        : `${picked.length} chosen${picked.length ? ` (${size(bytes)})` : ""} · saves to ${l.into}/ in your library`;
    box.replaceChildren(...head, list,
      el("div", { class: `gl-foot${over ? " over" : ""}` }, el("span", { class: "sub" }, summary)),
      el("div", { class: "lib-row" }, theme, recolour, el("span", { class: "spacer" }), go));
  },

  async linkDownload(btn) {
    const { url, listing, chosen } = this.link;
    const files = listing.files.filter((f) => chosen.has(f.path));
    const theme = $("#gl-theme").value || null;
    const recolour = $("#gl-recolour").checked;
    await this.busy(btn, `Downloading ${files.length}…`, async () => {
      const r = await invoke("wallpaper_link_download", { url, files, theme, recolour });
      const extra = theme && r.added ? `, ${r.added} added to ${theme}` : "";
      const skipped = r.skipped.length ? ` · ${r.skipped.length} skipped: ${r.skipped.slice(0, 2).map(([p, why]) => `${p.split("/").pop()} (${why})`).join(", ")}` : "";
      toast(`Saved ${r.saved} picture${r.saved === 1 ? "" : "s"} (${size(r.bytes)})${extra}${skipped}`, r.saved === 0);
      if (r.saved) { chosen.clear(); this.libOpen.add(r.dir.split("/wallpapers/").pop() || ""); }
      if (theme) await load();
      this.renderWallpapers();
    });
    if (this.link) this.linkRender();
  },

  // ---------------- new theme from a wallpaper
  draft: null,

  async ntDraft(light = null) {
    const btn = $("#nt-pick");
    await this.busy(btn, "Reading colours…", async () => {
      const d = await invoke("draft_theme_from_wallpaper", { path: this.draft?.path ?? null, light });
      if (!d) return;
      // Keep a name the user typed when only light/dark changed.
      if (this.draft && light !== null) d.name = $("#nt-name").value.trim() || d.name;
      this.draft = d;
      this.ntRender();
    });
  },

  ntRender() {
    const d = this.draft;
    const box = $("#nt-draft");
    box.hidden = false;
    const c = d.colors;
    const mode = (light) => el("button", {
      class: `btn${d.light === light ? " on" : ""}`,
      onclick: () => d.light !== light && this.ntDraft(light),
    }, light ? "Light" : "Dark");
    const name = el("input", { id: "nt-name", class: "search", type: "text", value: d.name, spellcheck: "false", autocomplete: "off", placeholder: "theme-name" });
    name.addEventListener("keydown", (e) => { if (e.key === "Enter") this.ntSave(); });
    box.replaceChildren(
      el("div", { class: "nt-top" },
        el("div", { class: "pic", style: { backgroundImage: d.thumb ? `url("${d.thumb}")` : "none" } }),
        el("div", { class: "nt-info" },
          el("div", { class: "nt-label" }, "Picture's colours"),
          el("div", { class: "sws" }, ...d.swatches.map((s) => el("i", { title: s, style: { background: s } }))),
          el("div", { class: "nt-label" }, `Theme · ${d.light ? "light" : "dark"}, ${d.mood}`),
          el("div", { class: "nt-prev", style: { background: c.background, color: c.foreground } },
            el("div", {}, el("span", { style: { color: c.accent } }, "❯ "), "palette set ", el("span", { style: { color: c.accent } }, d.name)),
            el("div", { class: "sws" }, ...["accent", "red", "yellow", "green", "cyan", "blue", "magenta"].map((k) => el("i", { title: `${k} ${c[k]}`, style: { background: c[k] } })))))),
      el("div", { class: "nt-form" },
        name,
        el("div", { class: "seg" }, mode(false), mode(true)),
        el("span", { class: "spacer" }),
        el("button", { class: "btn primary", id: "nt-save", onclick: () => this.ntSave() }, "Save"),
        el("button", { class: "btn", onclick: () => { box.hidden = true; this.draft = null; } }, "Cancel")));
  },

  async ntSave() {
    const d = this.draft;
    const name = $("#nt-name").value.trim();
    if (!name) return toast("Give the theme a name first", true);
    await this.busy($("#nt-save"), "Saving…", async () => {
      await invoke("save_theme_from_wallpaper", { name, path: d.path, light: d.light });
      await load();
      this.draft = null;
      const box = $("#nt-draft");
      box.replaceChildren(
        el("div", {}, el("b", {}, name), " is saved with the picture as its wallpaper. Edit its colors.toml any time."),
        el("div", { class: "actions" },
          el("button", {
            class: "btn primary",
            onclick: async () => {
              try { report(await invoke("apply", { name })); toast(`Now ${name}`); } catch (e) { toast(String(e), true); }
              box.hidden = true;
              await load();
              this.wpTheme = name;
              this.renderWallpapers();
            },
          }, "Use it"),
          el("button", { class: "btn", onclick: () => (box.hidden = true) }, "Done")));
    });
  },

  // ---------------- setup
  async runSetup(remove) {
    const btn = remove ? $("#remove-all") : $("#setup");
    if (remove && !armed(btn, "Click again to remove")) return;
    btn.disabled = true;
    try {
      const r = await invoke("setup_all", { remove });
      const changed = r.lines.length;
      const manual = r.lines.filter((l) => l.what.startsWith("needs you"));
      let msg = changed ? `${remove ? "Restored" : "Set up"} ${changed} config${changed === 1 ? "" : "s"}` : "Nothing to change";
      if (r.backup) msg += "; originals backed up";
      if (manual.length) msg += `. ${manual.map((l) => `${l.app}: ${l.what}`).join(" · ")}`;
      toast(msg + (r.problems.length ? ` · ${r.problems.join(" · ")}` : ""), manual.length > 0 || r.problems.length > 0);
    } catch (e) { toast(String(e), true); }
    btn.disabled = false;
    await this.renderApps();
    this.check();
  },

  // The same apps, groups and order as the Apps tab; apps that aren't installed
  // stay in place but muted, so the two lists always match.
  async check() {
    const out = $("#check-out");
    out.replaceChildren(el("div", { class: "empty" }, "Checking…"));
    try {
      const [rows, apps] = await Promise.all([invoke("check"), invoke("apps")]);
      const sign = { ok: "✓", fix: "!", off: "–" };
      const byName = new Map(rows.map((r) => [r.name, r]));
      // Wallpaper and light/dark aren't apps for the doctor: say whether they are on.
      for (const a of apps) {
        if ((a.name === "wallpaper" || a.name === "appearance") && !byName.has(a.name)) {
          byName.set(a.name, { name: a.name, status: a.on ? "ok" : "off", detail: a.on ? "on" : "off", notes: [] });
        }
      }
      const known = new Set(GROUPS.flatMap(([, names]) => names));
      const own = rows.filter((r) => ["theme", "command", "backups"].includes(r.name));
      const other = rows.filter((r) => !known.has(r.name) && !own.includes(r));
      const groups = [["Palette", own.map((r) => r.name)], ...GROUPS, ["Other", other.map((r) => r.name)]];
      const line = (r) => [
        el("div", { class: `c${r.status === "off" ? " absent" : ""}` },
          el("span", { class: `s ${r.status}` }, sign[r.status]),
          el("span", {}, (LABELS[r.name] || [r.name])[0]),
          el("span", { class: "d" }, r.detail)),
        ...r.notes.map(([kind, text]) => el("div", { class: "c" }, el("span"), el("span"),
          el("span", { class: `n${kind === "fix" ? " fix" : kind === "info" ? " info" : ""}` }, kind === "fix" ? `! ${text}` : text))),
      ];
      out.replaceChildren(...groups.flatMap(([title, names]) => {
        const items = names.map((n) => byName.get(n)).filter(Boolean);
        return items.length ? [el("h3", {}, title), ...items.flatMap(line)] : [];
      }));
    } catch (e) {
      out.replaceChildren(el("div", { class: "empty" }, String(e)));
    }
  },

  // ---------------- import
  async importPick() {
    const box = $("#import-plan");
    try {
      const plan = await invoke("import_dialog");
      if (!plan) return;
      box.hidden = false;
      box.replaceChildren(
        el("div", {}, el("b", {}, `${plan.files} files`), ` from ${plan.dir}`),
        el("div", {}, plan.theme ? `Theme: ${plan.theme}` : "No active theme in this export"),
        plan.apps.length ? el("div", {}, `Configs for: ${plan.apps.join(", ")}`) : null,
        plan.replaced.length
          ? el("div", {}, `Replaces ${plan.replaced.length} of your files (backed up first):`, el("ul", {}, ...plan.replaced.map((p) => el("li", {}, p))))
          : el("div", { class: "note" }, "Replaces none of your files."),
        el("div", { class: "actions" },
          el("button", { class: "btn", onclick: () => this.importApply(plan.dir) }, "Import"),
          el("button", { class: "btn", onclick: () => (box.hidden = true) }, "Cancel")));
    } catch (e) { toast(String(e), true); }
  },

  async importApply(dir) {
    $("#import-plan").hidden = true;
    toast("Importing…");
    try { toast(await invoke("import_apply", { dir })); } catch (e) { toast(String(e), true); }
    await this.open();
  },

  async open() {
    await load();
    const p = await invoke("prefs");
    $("#hotkey").textContent = pretty(p.hotkey);
    $("#login").checked = p.login;
    $("#favs-only").checked = p.next_cycles === "favourites";
    await this.renderApps();
    await this.renderExport();
    await this.renderCli();
    if (this.tab === "wallpapers") this.renderWallpapers();
    // First launch: the tour, once per window load.
    if (!p.welcomed && !this.toured) {
      this.toured = true;
      this.show("welcome");
    }
  },

  render() {},

  async renderApps() {
    const rows = await invoke("apps");
    const byName = new Map(rows.map((r) => [r.name, r]));
    const known = new Set(GROUPS.flatMap(([, names]) => names));
    const groups = [...GROUPS, ["Other", rows.map((r) => r.name).filter((n) => !known.has(n))]];
    const box = $("#apps");
    box.replaceChildren();
    for (const [title, names] of groups) {
      const items = names.map((n) => byName.get(n)).filter(Boolean);
      if (!items.length) continue;
      box.append(el("h3", {}, title));
      for (const a of items) {
        const [label, what] = LABELS[a.name] || [a.name];
        const sw = el("input", { type: "checkbox", class: "switch" });
        sw.checked = a.on;
        sw.onchange = () => this.toggle(a.name, sw);
        // Installed but never wired (new in this version, or added after init).
        const setup = a.can_wire
          ? el("button", { class: "btn", onclick: (e) => { e.preventDefault(); this.setUp(a.name, e.target); } }, "Set up")
          : null;
        const extra = a.name === "wallpaper" || a.name === "appearance";
        const state = !a.on ? "off: keeps its own look" : extra ? null : a.detail;
        const sub = [what, state].filter(Boolean).join(" · ");
        box.append(el("label", { class: `row${a.installed ? "" : " absent"}` },
          el("div", {}, el("div", { class: "title" }, label), el("div", { class: "sub" }, sub)),
          setup, sw));
        if (a.name === "obsidian" && a.on && a.installed) await this.renderObsidianLook(box);
      }
    }
  },

  // Obsidian's active pane: two options under its row (when the editor is split).
  async renderObsidianLook(box) {
    const p = await invoke("prefs");
    const option = (key, title, sub, on) => {
      const sw = el("input", { type: "checkbox", class: "switch" });
      sw.checked = on;
      sw.onchange = async () => {
        sw.disabled = true;
        try {
          await invoke("set_obsidian_look", { [key]: sw.checked });
          toast(`Obsidian: ${title.toLowerCase()} ${sw.checked ? "on" : "off"}`);
        } catch (e) {
          sw.checked = !sw.checked;
          toast(String(e), true);
        }
        sw.disabled = false;
      };
      box.append(el("label", { class: "row nested" },
        el("div", {}, el("div", { class: "title" }, title), el("div", { class: "sub" }, sub)), sw));
    };
    option("ring", "Ring around the active pane", "In the theme's accent, when the editor is split", p.obsidian_ring);
    option("dim", "Fade the other panes", "A little, so the one you're in stands out", p.obsidian_dim);
  },

  async toggle(name, sw) {
    sw.disabled = true;
    try {
      const problems = await invoke("set_app", { name, on: sw.checked });
      const label = (LABELS[name] || [name])[0];
      if (problems.length) toast(problems.join(" · "), true);
      else toast(sw.checked ? `${label}: themed by Palette` : `${label}: back to its own look`);
    } catch (e) {
      sw.checked = !sw.checked;
      toast(String(e), true);
    }
    sw.disabled = false;
    await this.renderApps();
  },

  async setUp(name, btn) {
    btn.disabled = true;
    btn.textContent = "Setting up…";
    try {
      const problems = await invoke("set_app", { name, on: true });
      const label = (LABELS[name] || [name])[0];
      toast(problems.length ? problems.join(" · ") : `${label} is now themed by Palette`, problems.length > 0);
    } catch (e) {
      toast(String(e), true);
    }
    await this.renderApps();
  },

  async renderExport() {
    const s = await invoke("sync_prefs");
    $("#exp-walls").checked = s.wallpapers;
    $("#exp-configs").checked = true;
    $("#configs-sub").textContent = s.configs.length
      ? `${s.configs.length} from sync.toml: ${s.configs.slice(0, 4).join(", ")}${s.configs.length > 4 ? ", …" : ""}`
      : "None listed in ~/.config/palette/sync.toml";
  },

  async exportNow() {
    const btn = $("#export");
    btn.disabled = true;
    btn.textContent = "Exporting…";
    try {
      const msg = await invoke("export_dialog", {
        wallpapers: $("#exp-walls").checked,
        configs: $("#exp-configs").checked,
      });
      if (msg) toast(msg);
    } catch (e) {
      toast(String(e), true);
    }
    btn.disabled = false;
    btn.textContent = "Export…";
  },

  async renderCli() {
    const s = await invoke("cli_status");
    const btn = $("#cli");
    const sub = $("#cli-sub");
    btn.disabled = !s.bundled || s.linked;
    btn.textContent = s.linked ? "Installed" : "Install";
    sub.textContent = !s.bundled ? "Not included in this build"
      : s.linked && s.other ? `Installed. An older copy at ${s.other} may run instead: remove it (cargo uninstall palette), then open a new terminal`
      : s.linked ? "`palette` in your terminal runs the app's copy (~/.local/bin)"
      : s.other ? `You already have one at ${s.other}; Install adds ~/.local/bin/palette`
      : "Adds `palette` to ~/.local/bin so you can use it in a terminal";
  },
};

// ---------------------------------------------------------------- wiring

const view = { overlay, picker, settings }[VIEW];

// Windows are created hidden and only shown; refresh every time they get focus.
window.addEventListener("focus", () => view.open());
listen("theme-changed", async () => { await load(); view.render(); });

if (VIEW === "picker") {
  listen("switched", (e) => {
    const s = e.payload;
    if (s.problems.length) toast(s.problems.join(" · "), true);
    else if (s.undo) toast(`Back to ${s.theme}`);
    else undoToast(s.theme);
  });
  $("#search").addEventListener("input", () => picker.render());
  $("#prev").onclick = () => picker.step("prev");
  $("#next").onclick = () => picker.step("next");
  $("#undo").onclick = () => picker.step("undo");
  $("#settings").onclick = async () => { await invoke("open_settings"); invoke("hide"); };
  $("#walls").onclick = async () => { await invoke("open_settings_tab", { tab: "wallpapers" }); invoke("hide"); };
  $("#wp-prev").onclick = () => picker.stepWall(-1);
  $("#wp-next").onclick = () => picker.stepWall(1);
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") invoke("hide");
    if (e.key === "Enter") {
      const first = $("#grid .card:not(.invalid)");
      first?.click();
    }
  });
} else if (VIEW === "settings") {
  $("#record").onclick = () => {
    recording = true;
    $("#hotkey").classList.add("recording");
    $("#hotkey").textContent = "Press keys…";
    $("#record").textContent = "Esc to cancel";
  };
  document.addEventListener("keydown", (e) => { if (recording) recordKey(e); });
  $("#login").onchange = async (e) => {
    try { e.target.checked = await invoke("set_login", { on: e.target.checked }); }
    catch (err) { toast(String(err), true); }
  };
  $("#favs-only").onchange = (e) =>
    invoke("set_next_cycles", { cycle: e.target.checked ? "favourites" : "all" }).catch((err) => toast(String(err), true));
  $("#export").onclick = () => settings.exportNow();
  $("#sc-on").onchange = () => schedule.save();
  for (const id of ["#sc-day", "#sc-night", "#sc-mode", "#sc-day-at", "#sc-night-at"]) $(id).onchange = () => schedule.save();
  $("#sc-q").addEventListener("input", (e) => schedule.search(e.target.value));
  $("#sc-cities").onchange = (e) => schedule.save({ city: e.target.value, lat: null, lon: null });
  $("#sc-suggest").onclick = () => schedule.save({ city: schedule.s.suggestion, lat: null, lon: null });
  const typed = () => {
    const lat = parseFloat($("#sc-lat").value), lon = parseFloat($("#sc-lon").value);
    if (Number.isFinite(lat) && Number.isFinite(lon)) schedule.save({ city: null, lat, lon });
  };
  $("#sc-lat").onchange = typed;
  $("#sc-lon").onchange = typed;
  $("#tour").onclick = () => settings.show("welcome");
  $("#gt-search").addEventListener("input", () => settings.renderThemes());
  $("#gt-filter").onchange = () => settings.renderThemes();
  $("#gt-install-url").onclick = async () => {
    const url = $("#gt-url").value.trim();
    if (!url) return toast("Paste a GitHub link to an Omarchy theme first");
    $("#gt-offer").hidden = true;
    // A folder, file or picture link can't be a theme: offer the wallpaper browser straight away.
    if (/github\.com\/[^/]+\/[^/]+\/(tree|blob|raw)\/|raw\.githubusercontent\.com|\.(png|jpe?g|webp|heic|tiff)$/i.test(url)) {
      return settings.offerWallpapers(url, "a link to a folder or a picture, not a theme.");
    }
    const failed = await settings.installTheme(url, $("#gt-install-url"));
    $("#gt-install-url").textContent = "Install";
    if (failed && /colors\.toml|alacritty\.toml/i.test(failed)) {
      settings.offerWallpapers(url, "it has no theme colours (colors.toml).");
    } else if (failed && /maximum file size|exceeded/i.test(failed)) {
      settings.offerWallpapers(url, "it's too big to be a theme; Palette can list its pictures without downloading it all.");
    }
  };
  $("#wp-reveal").onclick = () => invoke("reveal_folder", { what: "theme", theme: settings.wpTheme }).catch((e) => toast(String(e), true));
  $("#lib-reveal").onclick = () => invoke("reveal_folder", { what: "library" }).catch((e) => toast(String(e), true));
  $("#lib-add").onclick = () => settings.libAdd();
  $("#gl-go").onclick = () => settings.linkLook();
  $("#gl-url").addEventListener("keydown", (e) => { if (e.key === "Enter") settings.linkLook(); });
  $("#wl-next").onclick = () => welcome.go(1);
  $("#wl-back").onclick = () => welcome.go(-1);
  $("#wl-skip").onclick = () => welcome.finish();
  document.querySelectorAll(".tab").forEach((b) => (b.onclick = () => settings.show(b.dataset.tab)));
  listen("open-tab", (e) => settings.show(e.payload));
  $("#wp-theme").onchange = (e) => { settings.wpTheme = e.target.value; settings.renderWallpapers(); };
  $("#wp-add").onclick = async () => {
    try {
      const n = await invoke("add_wallpapers", { theme: settings.wpTheme });
      if (n) toast(`Added ${n} wallpaper${n === 1 ? "" : "s"} to ${settings.wpTheme}`);
    } catch (e) { toast(String(e), true); }
    settings.renderWallpapers();
  };
  $("#wp-recolour").onclick = () => settings.recolour();
  $("#wp-generate").onclick = () => settings.generate();
  $("#wp-variant").onclick = () => settings.generate(true);
  // Strength is Palette's own; gowall has none.
  $("#wp-engine").onchange = () => ($("#wp-strength").hidden = $("#wp-engine").value === "gowall");
  $("#nt-pick").onclick = () => { settings.draft = null; settings.ntDraft(); };
  // gowall is offered only when it's installed.
  invoke("has_gowall").then((yes) => ($("#wp-engine").hidden = !yes)).catch(() => {});
  $("#wp-next").onclick = async () => {
    try { await invoke("step_wallpaper"); } catch (e) { toast(String(e), true); }
    settings.renderWallpapers();
  };
  $("#setup").onclick = () => settings.runSetup(false);
  $("#remove-all").onclick = () => settings.runSetup(true);
  $("#refresh").onclick = async () => {
    try { const r = await invoke("refresh"); report(r); toast(`Re-applied ${r.theme}`); } catch (e) { toast(String(e), true); }
  };
  $("#check").onclick = () => settings.check();
  $("#co-theme").onchange = (e) => settings.renderColours(e.target.value);
  $("#co-reset").onclick = () => settings.coEdit({ how: "reset" });
  $("#copy-help").onclick = async () => {
    try { await invoke("copy_text", { text: "palette help" }); toast("Copied: palette help"); } catch (e) { toast(String(e), true); }
  };
  $("#import").onclick = () => settings.importPick();
  $("#cli").onclick = async () => {
    try { toast(await invoke("install_cli")); } catch (err) { toast(String(err), true); }
    settings.renderCli();
  };
} else {
  document.addEventListener("keydown", (e) => overlay.key(e));
}

view.open();
