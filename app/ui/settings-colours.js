// Settings → Colours: a theme's colours, edited on top of its own (`edits.toml`).
"use strict";

Object.assign(settings, {
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
    const fixes = this.coFixes(chip);
    if (!s.issues.length) {
      return box.replaceChildren(...fixes, el("div", { class: "row" },
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
    const covered = new Set(s.fixed.map((f) => f.key));
    for (const [n, i] of s.issues.entries()) {
      if (covered.has(i.fg) || covered.has(i.bg)) {
        rows[n].querySelector(".sub").append(" · Palette's fix covers this when the theme is used");
      }
    }
    box.replaceChildren(
      ...fixes,
      el("div", { class: "row" },
        el("div", {}, el("div", { class: "title" }, `In the theme's own colours: ${s.verdict.replace(`${s.theme}: `, "")}`),
          el("div", { class: "sub" }, "Each suggestion keeps the colour's hue and only makes it lighter or darker until it reads well")),
        all),
      ...rows);
  },

  /** Palette's readability fixes: level, this theme left original, what they change. */
  coFixes(chip) {
    const s = this.co;
    const level = el("select", { class: "select", onchange: (e) => this.coReadability({ level: e.target.value }) },
      ...[["off", "Off"], ["minimal", "Minimal"], ["strong", "Strong"]].map(([v, l]) =>
        el("option", { value: v, selected: v === s.level }, l)));
    const original = el("input", { type: "checkbox", class: "switch", checked: s.original,
      onchange: (e) => this.coReadability({ original: e.target.checked }) });
    const out = [
      el("div", { class: "row" },
        el("div", {}, el("div", { class: "title" }, "Readability fixes"),
          el("div", { class: "sub" }, "When a theme is used, Palette fixes colours that are hard to read or see, lighter or darker only, never one you changed. Minimal fixes what's broken; Strong aims for WCAG AA. For every theme")),
        level),
      el("label", { class: "row" },
        el("div", {}, el("div", { class: "title" }, `Original colours for ${s.theme}`),
          el("div", { class: "sub" }, "Use this theme exactly as its author made it, without Palette's fixes")),
        original),
    ];
    if (s.fixed.length) {
      out.push(el("div", { class: "sub co-fixed-head" }, `What Palette changes when ${s.theme} is used:`),
        ...s.fixed.map((f) => {
          // A surface shows text on it; a text colour shows itself on the background.
          const surface = f.key.includes("background") || f.key === "selection";
          const sample = (hex) => surface ? chip(this.coGet("bright_foreground"), hex) : chip(hex, this.coGet("background"));
          return el("div", { class: "row co-issue" },
          sample(f.from),
          el("span", { class: "co-arrow" }, "→"),
          sample(f.to),
          el("div", {}, el("div", { class: "title" }, LABEL[f.key] || f.key),
            el("div", { class: "sub" }, f.why)));
        }));
    } else if (s.level !== "off" && !s.original) {
      out.push(el("div", { class: "sub co-fixed-head" }, `Nothing to fix in ${s.theme}.`));
    }
    // Glass lift (11g): see-through apps' text, lifted over the wallpaper this theme shows.
    for (const [app, lines] of s.glass || []) {
      out.push(el("div", { class: "sub co-fixed-head" }, `On glass in ${app} (over this theme's wallpaper):`),
        ...lines.map((l) => el("div", { class: "sub co-glass" }, l)));
    }
    // Per app (11i): what each kind of app draws with these colours.
    if (s.apps?.length) {
      out.push(el("div", { class: "sub co-fixed-head" }, `How ${s.theme} works in each kind of app:`),
        ...s.apps.map((a) => el("div", { class: "row co-app" },
          el("span", { class: `co-grade ${a.grade}` }, a.grade),
          el("div", {}, el("div", { class: "title" }, a.app),
            el("div", { class: "sub" }, a.why.length ? a.why.join("; ") : "everything reads")))));
    }
    return out;
  },

  async coReadability(args) {
    try {
      await invoke("set_readability", { name: this.coTheme, ...args });
      await load();
      await this.coFetch();
    } catch (e) { toast(String(e), true); }
  },

  /** A small editor and terminal, painted with the colours as they stand. */
  coPreview() {
    const c = Object.fromEntries(this.co.colours.map((x) => [x.key, x.value]));
    // What apps get: Palette's fixes on top (they never touch a colour you edited).
    if ($("#co-withfix").checked) for (const f of this.co.fixed) c[f.key] = f.to;
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
});

$("#co-withfix").onchange = () => settings.co && settings.coPreview();
