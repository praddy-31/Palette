// Settings (`settings.html`): shared helpers, the `settings` object with the Apps, General,
// Move and command-line parts, and the page's wiring. Each other tab adds its methods from
// its own file (settings-*.js), loaded after this one; Bar/Windows/… are in desktop.js.
"use strict";

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
  appearance: ["Light / dark mode", "The system's appearance follows the theme"],
  gnome: ["GNOME", "Accent colour, nearest of GNOME's nine"],
  kde: ["KDE Plasma", "The whole desktop's colour scheme"],
  noctalia: ["Noctalia", "Bar, panels and launcher colours"],
  niri: ["niri", "Focus ring and window border colours"],
  foot: ["foot"],
  konsole: ["Konsole", "Its own Palette profile, made from your default"],
  "gnome-terminal": ["GNOME Terminal", "Its own Palette profile, made from your default"],
  fuzzel: ["fuzzel", "The launcher's colours"],
  ghostty: ["Ghostty"], kitty: ["kitty"], alacritty: ["Alacritty"], tmux: ["tmux"],
  wezterm: ["WezTerm", "Colours from one line before `return config` in wezterm.lua"],
  vscodium: ["VSCodium"], zed: ["Zed", "Its own Palette theme, opaque or glass"], neovim: ["Neovim"], btop: ["btop"],
  obsidian: ["Obsidian", "A colour snippet in every vault, on top of your theme"],
  shell: ["Shell", "Loads fzf, bat, starship, lazygit colours in new terminals"],
  starship: ["Starship"], fzf: ["fzf"], bat: ["bat"], lazygit: ["lazygit"],
  "habit-tracker": ["Habit Tracker", "Colours file the app follows (turn on in its Settings → Integrations)"],
  "chess-review": ["Chess Review", "Colours file the app follows (turn on in its Settings → Appearance)"],
  dispatch: ["Dispatch", "Colours file the app follows by itself"],
  easyplots: ["EasyPlots", "Colours file the launcher follows (its theme setting \"palette\", the default)"],
};
// Terminals with a Glass control (`palette glass`), and when each one shows a change.
const GLASS_NOTES = {
  ghostty: "Opacity changes show after quitting and reopening Ghostty.",
  kitty: "The first time, quit and reopen kitty; after that it updates live.",
  alacritty: "Your alacritty.toml wins if it sets opacity or blur too.",
  wezterm: "Updates live.",
  dispatch: "Updates live (within 2 s). Dispatch stops at 60 % see-through so text stays readable.",
  easyplots: "The launcher's background. Updates when it opens and live while it's open. No limit: above 60 % see-through text gets hard to read, and EasyPlots' Settings warns.",
  "habit-tracker": "The Mac app's window. Updates within 2 s while it follows Palette's colours, otherwise when the window comes to the front. Stops at 60 % see-through, like Dispatch.",
};

const GROUPS = [
  ["Desktop", ["sketchybar", "sketchybar-item", "borders", "gnome", "kde", "noctalia", "niri", "fuzzel", "wallpaper", "appearance"]],
  ["Terminals", ["ghostty", "kitty", "alacritty", "foot", "konsole", "gnome-terminal", "wezterm", "tmux"]],
  ["Editors", ["vscodium", "zed", "neovim", "obsidian"]],
  ["Command-line tools", ["shell", "starship", "fzf", "bat", "lazygit", "btop"]],
  ["Apps", ["habit-tracker", "chess-review", "dispatch", "easyplots"]],
];

/** What each heavy tab draws every time it's shown (emptied when you leave it, see `show`). */
const DRAWN = {
  mine: ["#mine"],
  themes: ["#gt-grid", "#gt-others"],
  wallpapers: ["#wp-grid", "#lib-grid"],
};

const settings = {
  tab: "apps",

  show(tab) {
    // Leaving a heavy tab: empty what it drew, so the page gives that memory back (measured
    // 2026-11-08: My themes' drawn desktops ~45 MB, Get themes' cards ~26 MB, kept while
    // hidden). Each one is drawn again from scratch when you come back.
    if (this.tab !== tab) for (const box of DRAWN[this.tab] || []) $(box)?.replaceChildren();
    this.tab = tab;
    document.querySelectorAll(".tab").forEach((b) => b.classList.toggle("on", b.dataset.tab === tab));
    document.querySelectorAll("[data-pane]").forEach((p) => (p.hidden = p.dataset.pane !== tab));
    if (tab === "wallpapers") this.renderWallpapers();
    // Every time, not just the first: wallpapers, apps or the theme may have
    // changed in another tab since the last check.
    if (tab === "setup") this.check();
    if (tab === "welcome") welcome.render();
    if (tab === "themes") this.renderThemes();
    if (tab === "mine") this.renderMine();
    if (tab === "schedule") schedule.render();
    if (tab === "tools") this.renderTools();
    if (tab.startsWith("colours")) this.renderColours(tab.split(":")[1]);
    if (["bar", "windows", "borders", "prompt"].includes(tab)) desktop.render(tab);
    if (tab === "macos") desktop.macos();
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

  /** Linux: say so, hide the Mac-only pages, show how to bind the quick switcher. */
  async applySystem() {
    if (this.system) return;
    this.system = await invoke("system_info");
    const sys = this.system;
    if (sys.platform !== "linux") return;
    document.body.classList.add("linux");
    const line = $("#mode-line");
    line.textContent = `Linux mode · ${sys.label}`;
    line.hidden = false;
    for (const tab of ["bar", "windows", "borders", "macos", "tools"]) {
      const b = document.querySelector(`.tab[data-tab="${tab}"]`);
      if (b) b.hidden = true;
    }
    const move = document.querySelector('.tab[data-tab="move"]');
    if (move) move.textContent = "Move to another computer";
    $("#move-title").textContent = "Move to another computer";
    $("#move-how").textContent = "scp, ";
    $("#move-other").textContent = "computer";
    if (sys.quick_switch_hint) {
      $("#hotkey").closest(".row").hidden = true;
      $("#hotkey-linux").hidden = false;
      $("#hotkey-hint").textContent = sys.quick_switch_hint;
      $("#hotkey-copy").onclick = async () => {
        const text = sys.quick_switch_hint.split("\n").pop();
        try { await invoke("copy_text", { text }); toast("Copied"); } catch (e) { toast(String(e), true); }
      };
    }
  },

  // Focus (every AeroSpace hop, every click back from another app): redraw only the tab you're
  // on, and only when something changed since the last look (14b). `stamp` is a fingerprint
  // of Palette's files from the core; the first draw is `open`.
  stamp: null,
  async onFocus() {
    // Opened on a tab (the picker's Wallpapers button) while this window was hidden.
    const tab = await invoke("pending_tab");
    if (tab) this.show(tab);
    const now = await invoke("stamp");
    if (now === this.stamp) return;
    this.stamp = now;
    await load();
    await this.redrawTab();
  },

  /** The visible tab, drawn again with fresh data. */
  async redrawTab() {
    if (this.tab === "apps") return this.renderApps();
    if (this.tab === "general") {
      this.fillGeneral(await invoke("prefs"));
      return this.renderCli();
    }
    if (this.tab === "move") return this.renderExport();
    this.show(this.tab);
  },

  async open() {
    this.stamp = await invoke("stamp");
    await load();
    await this.applySystem();
    const p = await invoke("prefs");
    this.fillGeneral(p);
    await this.renderApps();
    await this.renderExport();
    await this.renderCli();
    if (this.tab === "wallpapers") this.renderWallpapers();
    // First launch: the tour, once per window load.
    if (!p.welcomed && !this.toured) {
      this.toured = true;
      this.show("welcome");
    }
    // Opened on a tab (the picker's Wallpapers button) while this window was still loading.
    const tab = await invoke("pending_tab");
    if (tab) this.show(tab);
  },

  /** General's switches and menus, from `prefs`. */
  fillGeneral(p) {
    $("#hotkey").textContent = pretty(p.hotkey);
    $("#login").checked = p.login;
    $("#favs-only").checked = p.next_cycles === "favourites";
    $("#app-windows").value = p.windows;
    $("#idle-settings").checked = p.close_idle_settings;
    $("#idle-settings-row").hidden = p.windows === "ready";
    $("#menu-click").value = p.menu_click;
    $("#log-on").checked = p.log;
    $("#mono-greys").checked = p.monochrome.greys;
    $("#mono-tint").checked = p.monochrome.tint;
  },

  render() {},

  // Built off-screen and swapped in at once: emptying the list first (and waiting on the
  // core meanwhile) made the page short for a moment, which sent the scroll to the top.
  async renderApps() {
    const [rows, p] = await Promise.all([invoke("apps"), invoke("prefs")]);
    const byName = new Map(rows.map((r) => [r.name, r]));
    const known = new Set(GROUPS.flatMap(([, names]) => names));
    const groups = [...GROUPS, ["Other", rows.map((r) => r.name).filter((n) => !known.has(n))]];
    const box = document.createDocumentFragment();
    this.renderPerformance(box, p);
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
        if (a.name === "obsidian" && a.on && a.installed) this.renderObsidianLook(box, p);
        if (a.name === "zed" && a.on && a.installed) this.renderZedLook(box, p);
        if (GLASS_NOTES[a.name] && a.on && a.installed) this.renderGlass(box, a.name, label, p);
      }
    }
    $("#apps").replaceChildren(box);
  },

  // Performance mode: one switch, and which parts it covers (each can be left out).
  renderPerformance(box, prefs) {
    const p = prefs.performance;
    const save = async (args, what) => {
      try {
        await invoke("set_performance", args);
        toast(what);
      } catch (e) { toast(String(e), true); }
      await this.renderApps();
    };
    const sw = (checked, onchange) => {
      const input = el("input", { type: "checkbox", class: "switch" });
      input.checked = checked;
      input.onchange = () => { input.disabled = true; onchange(input.checked); };
      return input;
    };
    if (p.on) {
      box.append(el("div", { class: "perf-banner" },
        el("b", {}, "Performance mode is on. "),
        p.glass ? "Every glass window is solid, so the Glass controls below are locked; " : "",
        "your own looks come back when you turn it off (here or in the menu bar)."));
    }
    box.append(el("h3", {}, "Performance"));
    box.append(el("label", { class: "row" },
      el("div", {}, el("div", { class: "title" }, "Performance mode"),
        el("div", { class: "sub" }, "Easier on the GPU and battery. Your own looks come back when you turn it off. Also in the menu bar")),
      sw(p.on, (v) => save({ on: v }, v ? "Performance mode on" : "Performance mode off: your looks are back"))));
    const PARTS = [
      ["glass", "Solid windows", "Ghostty, kitty, Alacritty, WezTerm, Dispatch, EasyPlots, Habit Tracker and Zed"],
      ["bar", "Opaque bar", "SketchyBar's pills and background fully opaque, no blur, when Palette manages it (Bar page)"],
      ["borders", "Plain window borders", "JankyBorders without glow and HiDPI, when Palette manages it"],
      ["stats", "Slower stats pill", "CPU, GPU, temperature… every 30 s instead of every few seconds"],
    ];
    for (const [part, title, sub] of PARTS) {
      box.append(el("label", { class: "row nested" },
        el("div", {}, el("div", { class: "title" }, title), el("div", { class: "sub" }, sub)),
        sw(p[part], (v) => save({ part, value: v }, `Performance mode: ${title.toLowerCase()} ${v ? "included" : "left out"}`))));
    }
  },

  // Obsidian's active pane: two options under its row (when the editor is split).
  renderObsidianLook(box, p) {
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

  // Zed's window under its row: Solid / Blurred / Clear, and how see-through.
  renderZedLook(box, p) {
    const locked = p.performance.on && p.performance.glass;
    const KINDS = [["off", "Solid"], ["blurred", "Blurred"], ["clear", "Clear"]];
    // A slider only saves (its number already shows the value); the buttons add or remove
    // rows, so they redraw.
    const save = async (args, what, redraw = true) => {
      try {
        await invoke("set_zed_look", args);
        toast(`Zed: ${what}`);
      } catch (e) { toast(String(e), true); }
      if (redraw) await this.renderApps();
    };
    const seg = el("div", { class: "seg" }, ...KINDS.map(([k, label]) => el("button", {
      class: `btn${p.zed_glass === k ? " on" : ""}`, disabled: locked,
      onclick: () => p.zed_glass !== k && save({ glass: k }, label === "Solid" ? "solid window" : `${label.toLowerCase()} glass`),
    }, label)));
    box.append(el("div", { class: "row nested wrap" },
      el("div", {}, el("div", { class: "title" }, "Glass"),
        el("div", { class: "sub" }, "Blurred: frosted, see-through window. Clear: see-through, no blur")), seg));
    if (p.zed_glass === "off") return;
    const value = el("span", { class: "sub" }, `${p.zed_strength}`);
    const slider = el("input", { type: "range", min: "0", max: "100", step: "5", value: String(p.zed_strength), disabled: locked });
    slider.oninput = () => { value.textContent = slider.value; };
    slider.onchange = () => save({ strength: Number(slider.value) }, `glass strength ${slider.value}`, false);
    box.append(el("div", { class: "row nested" },
      el("div", {}, el("div", { class: "title" }, "Strength"),
        el("div", { class: "sub" }, "How see-through: 0 solid, 100 fully clear")), el("div", {}, slider, " ", value)));
  },

  // A terminal's window under its row: Yours / Solid / Blurred / Clear (Ghostty:
  // also Liquid), how see-through, and how much blur.
  renderGlass(box, app, label, prefs) {
    const g = prefs.glass[app];
    const NAMES = { own: "Yours", solid: "Solid", blurred: "Blurred", clear: "Clear",
      "liquid-glass": "Liquid", "liquid-clear": "Liquid clear" };
    // Sliders only save (see renderZedLook); the kind buttons redraw.
    const save = async (args, what, redraw = true) => {
      try {
        await invoke("set_glass", { app, ...args });
        toast(`${label}: ${what}`);
      } catch (e) { toast(String(e), true); }
      if (redraw) await this.renderApps();
    };
    const perf = prefs.performance;
    const locked = perf.on && perf.glass;
    const seg = el("div", { class: "seg" }, ...g.kinds.map((k) => el("button", {
      class: `btn${g.kind === k ? " on" : ""}`, disabled: locked,
      onclick: () => g.kind !== k && save({ kind: k },
        k === "own" ? "your own window setting" : k === "solid" ? "solid window" : `${NAMES[k].toLowerCase()} glass`),
    }, NAMES[k])));
    const about = (perf.on && perf.glass ? "Solid while Performance mode is on; this is the look it comes back to. " : "")
      + (g.kind === "own"
        ? `Yours: Palette leaves it to your ${label} config. ${GLASS_NOTES[app]}`
        : GLASS_NOTES[app]);
    // `wrap`: the buttons go under the text when both don't fit (one word per line before).
    box.append(el("div", { class: "row nested wrap" },
      el("div", {}, el("div", { class: "title" }, "Glass"), el("div", { class: "sub" }, about)), seg));
    if (g.kind === "own" || g.kind === "solid") return;
    const slider = (key, title, sub, max) => {
      const value = el("span", { class: "sub" }, `${g[key]}`);
      const input = el("input", { type: "range", min: "0", max: String(max), step: "5", value: String(g[key]), disabled: locked });
      input.oninput = () => { value.textContent = input.value; };
      input.onchange = () => save({ [key]: Number(input.value) }, `glass ${key} ${input.value}`, false);
      box.append(el("div", { class: "row nested" },
        el("div", {}, el("div", { class: "title" }, title), el("div", { class: "sub" }, sub)), el("div", {}, input, " ", value)));
    };
    slider("strength", "Strength", "How see-through: 0 solid, 100 fully clear", 100);
    if (g.kind === "blurred") {
      const BLUR = {
        ghostty: "How frosted: 20 is Ghostty's default",
        kitty: "How frosted (kitty stops at 64)",
        alacritty: "Alacritty only has blur on (above 0) or off",
        wezterm: "How frosted",
        dispatch: "How frosted (like Ghostty's: 20 is light)",
        easyplots: "How frosted (like Ghostty's: 20 is light)",
        "habit-tracker": "How frosted (like Ghostty's: 20 is light)",
      };
      slider("blur", "Blur", BLUR[app], 100);
    }
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
    $("#exp-library").checked = s.library;
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
        library: $("#exp-library").checked,
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
$("#app-windows").onchange = (e) => {
  $("#idle-settings-row").hidden = e.target.value === "ready";
  invoke("set_app_prefs", { windows: e.target.value }).catch((err) => toast(String(err), true));
};
$("#idle-settings").onchange = async (e) => {
  try { e.target.checked = await invoke("set_close_idle_settings", { on: e.target.checked }); }
  catch (err) { toast(String(err), true); }
};
/** Monochrome themes (11f): both switches go through one command; the theme re-renders. */
for (const [id, key] of [["#mono-greys", "greys"], ["#mono-tint", "tint"]]) {
  $(id).onchange = async (e) => {
    try {
      const m = await invoke("set_monochrome", { [key]: e.target.checked });
      $("#mono-greys").checked = m.greys;
      $("#mono-tint").checked = m.tint;
      await load();
    } catch (err) { toast(String(err), true); }
  };
}
$("#menu-click").onchange = (e) =>
  invoke("set_app_prefs", { click: e.target.value }).catch((err) => toast(String(err), true));
$("#log-on").onchange = async (e) => {
  try { e.target.checked = await invoke("set_log", { on: e.target.checked }); }
  catch (err) { toast(String(err), true); }
};
$("#log-show").onclick = () => invoke("reveal_folder", { what: "log" }).catch((e) => toast(String(e), true));
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
$("#gt-sheet").onclick = async () => {
  try { await invoke("open_sheet"); toast("Opened the inspection sheet in your browser"); }
  catch (e) { toast(String(e), true); }
};
$("#gt-search").parentElement.after(chipRow($("#gt-search"), () => settings.renderThemes()));
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
  if (failed?.code === "not-a-theme") {
    settings.offerWallpapers(url, "it has no theme colours (colors.toml).");
  } else if (failed?.code === "too-big") {
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
// The menu bar's Performance Mode tick: keep the Apps page in step if it's showing.
listen("performance", () => { if ($("#apps")?.childElementCount) settings.renderApps(); });
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

start(settings);

// Where things are (F4): each place is a link to its tab.
document.querySelectorAll("[data-go]").forEach((b) => { b.onclick = () => settings.show(b.dataset.go); });
