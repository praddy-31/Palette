// Settings → Bar, Windows, Borders: Palette's pages for SketchyBar, AeroSpace
// and JankyBorders (`palette desktop` in the terminal). The Bar page's own code is in
// desktop-bar.js. Loaded after shared.js and settings.js
// and uses its helpers (el, $, toast, invoke, themes, armed).
"use strict";

const COLOUR_KEYS = [
  "accent", "blue", "cyan", "green", "yellow", "orange", "red", "magenta", "brown",
  "foreground", "bright_foreground", "light_foreground", "dark_foreground", "muted",
  "background", "dark_background", "darker_background", "lighter_background", "selection",
  "bright_blue", "bright_cyan", "bright_green", "bright_yellow", "bright_red", "bright_magenta",
];

const desktop = {
  status: {}, // tool -> DesktopStatus
  model: {}, // tool -> settings being edited
  saved: {}, // tool -> JSON of the saved settings
  kinds: [],
  apps: null, // running apps, fetched once per page
  sel: null, // bar: { p, i } of the selected item
  drag: null,
  timer: null,

  async render(tool) {
    const box = $(`#dk-${tool}`);
    try {
      this.status[tool] = await invoke("desktop_status", { name: tool });
      this.model[tool] = await invoke("desktop_get", { name: tool });
      this.saved[tool] = JSON.stringify(this.model[tool]);
      if (tool === "bar" && !this.kinds.length) this.kinds = await invoke("bar_kinds");
      if (tool === "prompt" && !this.promptMeta) this.promptMeta = await invoke("prompt_meta");
      // The bar shows AeroSpace's workspaces (set in Windows).
      if (tool === "bar") this.workspaces = (await invoke("desktop_get", { name: "windows" }).catch(() => null))?.workspaces || 6;
      if (tool === "bar") await this.barStart(); // desktop-bar.js
    } catch (e) {
      return box.replaceChildren(el("div", { class: "empty" }, String(e)));
    }
    this.sel = null;
    this.draw(tool);
  },

  draw(tool) {
    const parts = [this.header(tool)];
    if (this.status[tool].installed || this.status[tool].managed) {
      parts.push(...{ bar: () => this.bar(), windows: () => this.windows(), borders: () => this.borders(), prompt: () => this.prompt() }[tool]());
    }
    const box = $(`#dk-${tool}`);
    box.classList.toggle("dk-locked", !this.status[tool].managed);
    box.replaceChildren(...parts);
  },

  // ---------------- shared

  header(tool) {
    const s = this.status[tool];
    const file = s.files[0]?.replace(/^\/Users\/[^/]+/, "~") || "";
    if (!s.installed && !s.managed) {
      return el("div", { class: "row dk-head" },
        el("div", {}, el("div", { class: "title" }, `${s.app} isn't installed`),
          el("div", { class: "sub" }, "Get it in Get the tools; this page works once it's there.")),
        this.btn("Get the tools", () => settings.show("tools")));
    }
    if (!s.managed) {
      const go = this.btn(`Let Palette manage ${s.app}`, async () => {
        go.disabled = true;
        try {
          const r = await invoke("desktop_own", { name: tool, manage: true });
          this.report(r, `Palette now manages ${s.app}. Your file is kept as the original.`);
        } catch (e) { toast(String(e), true); }
        this.render(tool);
      }, "btn primary");
      return el("div", { class: "row dk-head callout" },
        el("div", {},
          el("div", { class: "title" }, "These are your settings, as Palette read them"),
          el("div", { class: "sub" }, `To change them here, Palette takes over ${file}: it keeps your file as the original first, and Give it back restores it.`)),
        go);
    }
    const back = this.btn("Give it back", async () => {
      if (!armed(back, "Click again: restore your file")) return;
      try {
        const r = await invoke("desktop_own", { name: tool, manage: false });
        this.report(r, `${s.app} is back on your own file`);
      } catch (e) { toast(String(e), true); }
      this.render(tool);
    }, "btn danger");
    return el("div", { class: "row dk-head" },
      el("div", {},
        el("div", { class: "title" }, `Palette manages ${file}`),
        el("div", { class: "sub" }, "A backup is made before every change: the last 5, plus your original (Give it back).")),
      this.history(tool), back);
  },

  /** Go back to the settings before one of the last changes. */
  history(tool) {
    const list = this.status[tool].history || [];
    if (!list.length) return null;
    // 2026-10-17T051246Z(-1) → 17 Oct 13:12 (local time)
    const when = (name) => {
      const m = name.match(/^(\d{4})-(\d\d)-(\d\d)T(\d\d)(\d\d)(\d\d)Z/);
      if (!m) return name;
      const d = new Date(Date.UTC(+m[1], m[2] - 1, +m[3], +m[4], +m[5], +m[6]));
      return d.toLocaleString("en-GB", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit", second: "2-digit" });
    };
    const pick = el("select", { class: "select", title: "Each is how it was before a change" },
      el("option", { value: "" }, "Go back to…"),
      ...list.map((name, i) => el("option", { value: name }, `${i === 0 ? "Before the last change" : "Before the change at"} · ${when(name)}`)));
    pick.onchange = async () => {
      if (!pick.value) return;
      try {
        const r = await invoke("desktop_restore", { name: tool, backup: pick.value });
        this.report(r, "Back to how it was. This can be undone the same way.");
      } catch (e) { toast(String(e), true); }
      this.render(tool);
    };
    return pick;
  },

  btn(label, onclick, cls = "btn") {
    return el("button", { class: cls, onclick }, label);
  },

  report(r, ok) {
    if (r.warnings.length) toast(r.warnings.join("\n"), false, 9000);
    else if (r.failed) toast(`Saved, but ${r.reload}`, true);
    else if (ok) toast(ok); // borders apply as you go: the borders are the message
  },

  /** Read and write `a.b.c` in a model. */
  get(tool, path) { return path.split(".").reduce((o, k) => o?.[k], this.model[tool]); },
  put(tool, path, value) {
    const keys = path.split(".");
    const last = keys.pop();
    keys.reduce((o, k) => o[k], this.model[tool])[last] = value;
    this.changed(tool);
  },

  dirty(tool) { return JSON.stringify(this.model[tool]) !== this.saved[tool]; },

  /** Borders apply as you go; the bar and windows wait for Apply. */
  changed(tool) {
    if (tool === "borders") {
      this.paintBorders();
      clearTimeout(this.timer);
      this.timer = setTimeout(() => this.apply("borders"), 450);
      return;
    }
    if (tool === "bar") { this.record(); this.paintBar(); this.paintFit(); }
    if (tool === "prompt") this.previewSoon();
    const bar = document.querySelector(`#dk-${tool} .dk-apply`);
    if (bar) bar.hidden = !this.dirty(tool);
  },

  async apply(tool) {
    if (!this.dirty(tool)) return;
    const btn = document.querySelector(`#dk-${tool} .dk-apply .primary`);
    if (btn) btn.disabled = true;
    try {
      const r = await invoke("desktop_set", { name: tool, settings: this.model[tool] });
      this.saved[tool] = JSON.stringify(this.model[tool]);
      if (tool === "bar") this.paintSaved?.();
      this.report(r, tool === "borders" ? "" : "Applied");
      this.status[tool] = await invoke("desktop_status", { name: tool });
      // The header lists the backups: a change made one and may have trimmed one.
      document.querySelector(`#dk-${tool} .dk-head`)?.replaceWith(this.header(tool));
    } catch (e) { toast(String(e), true); }
    if (btn) btn.disabled = false;
    const bar = document.querySelector(`#dk-${tool} .dk-apply`);
    if (bar) bar.hidden = !this.dirty(tool);
  },

  applyBar(tool) {
    const revert = this.btn(tool === "bar" ? "Revert" : "Undo changes", () => { this.model[tool] = JSON.parse(this.saved[tool]); this.sel = null; if (tool === "bar") this.record(); this.draw(tool); });
    // A redraw (block colour, module chips) builds a new bar: it must still show unsaved changes.
    return el("div", { class: "dk-apply", ...(this.dirty(tool) ? {} : { hidden: "" }) },
      el("span", {}, "Unsaved changes"), revert, this.btn("Apply", () => this.apply(tool), "btn primary"));
  },

  row(title, sub, ...controls) {
    return el("div", { class: "row" }, el("div", {}, el("div", { class: "title" }, title), sub ? el("div", { class: "sub" }, sub) : null), ...controls);
  },

  num(tool, path, min, max, step = 1) {
    const input = el("input", { class: "search dk-num", type: "number", min, max, step, value: this.get(tool, path) });
    input.addEventListener("change", () => {
      const v = Number(input.value);
      if (!Number.isFinite(v) || v < min || v > max) { input.value = this.get(tool, path); return toast(`From ${min} to ${max}`, true); }
      this.put(tool, path, v);
    });
    return input;
  },

  slider(tool, path, min, max, step = 1, unit = "") {
    const out = el("span", { class: "dk-val" }, `${this.get(tool, path)}${unit}`);
    const input = el("input", { type: "range", min, max, step, value: this.get(tool, path) });
    input.addEventListener("input", () => { out.textContent = `${input.value}${unit}`; this.put(tool, path, Number(input.value)); });
    return el("span", { class: "dk-slider" }, input, out);
  },

  select(tool, path, options) {
    const s = el("select", { class: "select" }, ...options.map(([v, label]) => {
      const o = el("option", { value: v }, label);
      if (v === this.get(tool, path)) o.selected = true;
      return o;
    }));
    s.onchange = () => this.put(tool, path, s.value);
    return s;
  },

  toggle(tool, path) {
    const c = el("input", { type: "checkbox", class: "switch" });
    c.checked = !!this.get(tool, path);
    c.onchange = () => this.put(tool, path, c.checked);
    return c;
  },

  /** A theme colour by name, with a swatch of the active theme's. */
  colour(tool, path, none) {
    const c = activeColours();
    const swatch = el("i", { class: "dk-swatch" });
    const paint = () => (swatch.style.background = c[this.get(tool, path)] || "transparent");
    const opts = COLOUR_KEYS.map((k) => [k, LABEL[k] || k]);
    const s = this.select(tool, path, none ? [["", none], ...opts] : opts);
    s.addEventListener("change", paint);
    paint();
    return el("span", { class: "dk-colour" }, swatch, s);
  },

  async runningApps() {
    this.apps ||= invoke("running_apps").catch(() => []);
    return this.apps;
  },

  /** A menu of running apps, filled now and again each time Settings comes back to the front
   *  (apps start and quit while it stays open; owner, 2026-11-17). `options(apps)` gives
   *  [value, label] pairs; the first is the one selected. */
  appMenu(select, options) {
    select.dataset.apps = "";
    select.refill = async () => {
      const apps = await this.runningApps();
      select.replaceChildren(...options(apps).map(([v, l], i) => el("option", { value: v, selected: i === 0 }, l)));
    };
    select.refill();
    return select;
  },

  /** Settings is in front again: ask for the running apps afresh and refill the menus on show. */
  freshApps() {
    this.apps = null;
    for (const s of document.querySelectorAll("select[data-apps]")) s.refill?.();
  },

  // ---------------- borders

  borders() {
    const t = "borders";
    return [
      el("div", { class: "dk-bd-preview" },
        el("div", { class: "dk-win", id: "bd-inactive" }, el("span", {}, "Other window")),
        el("div", { class: "dk-win", id: "bd-active" }, el("span", {}, "Focused window"))),
      el("h2", {}, "Shape"),
      this.row("Width", "Points", this.slider(t, "width", 1, 20, 0.5)),
      this.row("Corners", "Round matches macOS windows; square suits square-cornered apps", this.select(t, "style", [["round", "Round"], ["square", "Square"]])),
      this.row("Sharper on Retina", "Draws at full resolution (hidpi)", this.toggle(t, "hidpi")),
      this.row("Above or below the window", "Below hides the inner half", this.select(t, "order", [["above", "Above"], ["below", "Below"]])),
      this.row("Blur", "Blurs what's behind the border (0: none)", this.slider(t, "blur_radius", 0, 40, 1)),
      el("h2", {}, "Colours"),
      this.row("Focused window", "A theme colour, so it changes with the theme", this.colour(t, "active")),
      this.row("Fade into", "A second colour makes a gradient", this.colour(t, "active_to", "No gradient")),
      this.row("Other windows", "The border of every window you're not in", this.colour(t, "inactive")),
      this.row("How visible", "0 hides the other windows' borders", this.slider(t, "inactive_opacity", 0, 100, 5, "%")),
      el("h2", {}, "No border for"),
      this.appList(t, "ignore"),
    ].concat(this.paintSoon());
  },

  /** Paint previews after the elements are in the page. */
  paintSoon(also) {
    setTimeout(() => { this.paintBorders(); this.paintBar(); also?.(); });
    return [];
  },

  paintBorders() {
    const m = this.model.borders;
    const a = $("#bd-active"), b = $("#bd-inactive");
    if (!m || !a) return;
    const c = activeColours();
    const radius = m.style === "round" ? "12px" : "2px";
    const w = `${Math.max(1, m.width * 0.6)}px`;
    const fill = m.active_to ? `linear-gradient(135deg, ${c[m.active]}, ${c[m.active_to]})` : c[m.active];
    Object.assign(a.style, { borderRadius: radius, padding: w, background: fill });
    const op = m.inactive_opacity / 100;
    Object.assign(b.style, { borderRadius: radius, padding: w, background: `color-mix(in srgb, ${c[m.inactive]} ${op * 100}%, transparent)` });
  },

  /** App names (borders) as chips, with an Add from running apps. */
  appList(tool, path) {
    const list = this.get(tool, path);
    const chips = el("div", { class: "dk-chips" }, ...list.map((name, i) => el("span", { class: "dk-chip" }, name,
      el("button", { class: "link", title: "Remove", onclick: () => { list.splice(i, 1); this.put(tool, path, list); this.draw(tool); } }, "×"))));
    const pick = this.appMenu(el("select", { class: "select" }), (apps) =>
      [["", "Add a running app…"], ...apps.filter(([, n]) => !list.includes(n)).map(([, n]) => [n, n])]);
    pick.onchange = () => {
      if (!pick.value) return;
      list.push(pick.value);
      this.put(tool, path, list);
      this.draw(tool);
    };
    return el("div", {}, chips, el("div", { class: "row" }, el("div", {}, el("div", { class: "sub" }, list.length ? "" : "Every app has a border")), pick));
  },

  // ---------------- windows

  windows() {
    const t = "windows";
    const m = this.model.windows;
    const gap = (path, label) => el("label", { class: `dk-gap dk-gap-${path.split("_")[1] || path}` }, el("span", path === "outer_top" ? { id: "dk-top-label" } : {}, label), this.num(t, `gaps.${path}`, 0, 400));
    // With a separate gap for other displays, Top is the MacBook screen's only: say so (owner
    // changed Top while looking at another display, and nothing moved there).
    const topLabel = () => (m.gaps.outer_top_other != null ? "Top (MacBook screen)" : "Top");
    const topGap = gap("outer_top", topLabel());
    topGap.querySelector("input").addEventListener("change", () => this.paintTopOther?.());
    const ws = el("div", { class: "dk-ws" });
    const boxes = () => ws.replaceChildren(...Array.from({ length: m.workspaces }, (_, i) => el("i", {}, String(i + 1))));
    boxes();
    const count = this.num(t, "workspaces", 1, 9);
    count.addEventListener("change", boxes);
    const rules = el("div", { class: "dk-rules" }, ...m.rules.map((r, i) => this.rule(r, i)));
    const add = this.btn("Add a rule", async () => {
      const apps = await this.runningApps();
      const [id, name] = apps.find(([id]) => !m.rules.some((r) => r.app_id === id)) || ["com.apple.finder", "Finder"];
      m.rules.push({ app_id: id, app_name: name, action: "float", workspace: 1 });
      this.changed(t);
      this.draw(t);
    });
    // What the bar takes up, for the note (Palette never sets the gaps itself).
    if (this.barRoom === undefined) {
      this.barRoom = null;
      invoke("desktop_get", { name: "bar" }).then((b) => { this.barRoom = b.position === "top" ? Math.max(0, b.y_offset) + b.height : null; this.draw(t); }).catch(() => {});
    }
    return [
      el("h2", {}, "Gaps"),
      el("p", { class: "note" }, "Gaps are the empty space AeroSpace leaves around tiled windows. Top, Left, Right and Bottom are the space between the windows and the edges of the screen; the two \"Between\" gaps are the space between windows that sit next to each other."),
      el("p", { class: "note" }, "AeroSpace counts the edge gaps from the area macOS lets windows use, not from the glass: below the menu bar and beside the Dock while they're always shown, from the screen's edges while they hide. A MacBook screen with a notch is the exception: macOS always keeps the menu bar's strip there, even with the menu bar hidden, so on the MacBook Top counts from below that strip, and your bar sits inside it."),
      el("p", { class: "note" }, "macOS doesn't know about your bar (SketchyBar), so wherever windows can reach the top of the screen the top gap has to leave room for it",
        this.barRoom ? ` (your bar reaches ${this.barRoom} pt down)` : "",
        ". Palette doesn't change the gaps by itself when you change the bar, Dock or menu bar, except with Match the MacBook screen below. AeroSpace re-lays out a workspace when you next visit it. While a hidden Dock is showing, macOS gives windows less room, so they shrink until it hides again (macOS, not Palette; AeroSpace takes a moment to grow them back)."),
      el("div", { class: "dk-gaps" },
        topGap, gap("outer_left", "Left"), gap("outer_right", "Right"), gap("outer_bottom", "Bottom"),
        el("div", { class: "dk-gaps-inner" }, el("i"), el("i"), gap("inner_horizontal", "Between, across"), gap("inner_vertical", "Between, down"))),
      this.topOther(),
      el("h2", {}, "Layout"),
      this.row("A new workspace starts as", "Tiles: side by side. Accordion: stacked, one in front", this.select(t, "layout", [["tiles", "Tiles"], ["accordion", "Accordion"]])),
      this.row("Direction", "Auto: across on wide screens, down on tall ones", this.select(t, "orientation", [["auto", "Auto"], ["horizontal", "Across"], ["vertical", "Down"]])),
      this.accordion(),
      el("h2", {}, "Workspaces"),
      this.row("How many", `Kept even when empty. The bar shows these plus any other workspace with windows. Shortcuts: ${m.modifier}-1… to go, ${m.modifier}-shift-1… to move a window. Spare shortcuts you have stay.`,
        ws, count),
      el("h2", {}, "App rules"),
      el("p", { class: "note" }, "What happens to an app's new windows. Rules you wrote with more conditions stay in your file and aren't listed here."),
      rules, el("div", { class: "actions" }, add),
      el("h2", {}, "Start"),
      this.row("Start AeroSpace at login", null, this.toggle(t, "start_at_login")),
      el("h2", {}, "Keyboard shortcuts"),
      this.shortcuts(),
      this.applyBar(t),
    ];
  },

  /** A different top gap on displays other than the built-in screen. */
  topOther() {
    const g = this.model.windows.gaps;
    if (g.follow_bar === undefined) g.follow_bar = true;
    const on = el("input", { type: "checkbox", class: "switch" });
    on.checked = g.outer_top_other != null;
    // Matching the MacBook screen: the bar's reach (offset + height, a top bar Palette manages)
    // plus Top, so windows start as far below the bar everywhere. Worked out again whenever the
    // bar or this page is applied (`follow_bar` in the saved settings).
    const follow = () => (this.barRoom ? this.barRoom + (Number(g.outer_top) || 0) : null);
    const n = el("input", { class: "search dk-num", type: "number", min: 0, max: 400 });
    const fol = el("input", { type: "checkbox", class: "switch" });
    fol.checked = g.follow_bar;
    const folSub = el("div", { class: "sub" });
    const followRow = el("div", { class: "row nested" },
      el("div", {}, el("div", { class: "title" }, "Match the MacBook screen"), folSub), fol);
    const paint = () => {
      const f = follow();
      const following = g.follow_bar && f != null;
      if (following) g.outer_top_other = on.checked ? f : g.outer_top_other;
      n.disabled = following;
      n.value = following ? f : (g.outer_top_other ?? g.outer_top);
      n.hidden = !on.checked;
      followRow.hidden = !on.checked;
      folSub.textContent = f != null
        ? `Other displays get your bar (${this.barRoom} pt) plus Top (${g.outer_top}): ${f} pt from the top of the screen, so windows start ${g.outer_top} pt below the bar there too, the same as on the MacBook. `
          + "Palette works it out again whenever you change the bar or Top. Turn this off to type your own number."
        : "Needs a bar at the top that Palette manages (Bar page) to know how tall it is. Until then, type the number yourself: your bar's height (plus its offset from the top) plus the space you want under it.";
      const l = $("#dk-top-label");
      if (l) l.textContent = on.checked ? "Top (MacBook screen)" : "Top";
    };
    this.paintTopOther = paint;
    on.onchange = () => { g.outer_top_other = on.checked ? Number(n.value) : null; paint(); this.changed("windows"); };
    fol.onchange = () => { g.follow_bar = fol.checked; paint(); this.changed("windows"); };
    n.onchange = () => { const v = Number(n.value); if (v >= 0 && v <= 400) { g.outer_top_other = v; this.changed("windows"); } };
    paint();
    const main = this.row("Different top gap on other displays",
      "Top above is measured on the MacBook screen from below the notch strip, where your bar already sits. "
      + "An external display or an iPad (Sidecar) with the menu bar hidden has no such strip: windows can reach the very top of the screen and end up behind your bar. "
      + "Turn this on to give those displays their own top gap, counted from the top edge of the screen, so it has to be at least as tall as your bar. Off: every display uses Top.",
      n, on);
    return el("div", {}, main, followRow);
  },

  /** Accordion padding, explained: it only shows on a workspace in accordion layout. */
  accordion() {
    const m = this.model.windows;
    const key = (cmd) => (m.shortcuts || []).find((s) => s.mode === "main" && s.run[0]?.startsWith(cmd))?.key;
    const acc = key("layout accordion"), tiles = key("layout tiles");
    const how = acc ? `Press ${acc} on a workspace to stack it${tiles ? ` (${tiles}: tiles again)` : ""}.` : "Add a shortcut for “layout accordion” below to switch a workspace to it.";
    return this.row("Accordion padding",
      `In accordion layout the windows lie on top of each other, and the ones behind peek out by this much at the sides. Nothing changes in tiles layout. ${how}`,
      this.num("windows", "accordion_padding", 0, 200));
  },

  /** Every AeroSpace shortcut, by binding mode. */
  shortcuts() {
    const t = "windows";
    const m = this.model.windows;
    const list = m.shortcuts || (m.shortcuts = []);
    if (!this.commands) invoke("aerospace_commands").then((c) => { this.commands = c; this.draw(t); }).catch(() => {});
    const ids = "dk-cmds";
    const datalist = el("datalist", { id: ids }, ...(this.commands || []).map(([c, what]) => el("option", { value: c }, what)));
    // Workspace shortcuts (modifier-N, modifier-shift-N up to N) follow Workspaces above.
    const auto = (s) => {
      const n = Number(s.key.split("-").pop());
      return s.mode === "main" && n >= 1 && n <= m.workspaces && s.run.length === 1
        && ((s.key === `${m.modifier}-${n}` && s.run[0] === `workspace ${n}`)
          || (s.key === `${m.modifier}-shift-${n}` && s.run[0] === `move-node-to-workspace ${n}`));
    };
    const filter = el("input", { class: "search", type: "search", placeholder: "Find a shortcut or command", value: this.find || "" });
    const counts = {};
    for (const s of list) counts[`${s.mode} ${s.key}`] = (counts[`${s.mode} ${s.key}`] || 0) + 1;
    const modes = [...new Set(["main", ...list.map((s) => s.mode)])];
    const entered = (mode) => list.find((s) => s.run.includes(`mode ${mode}`) && s.mode !== mode)?.key;
    const body = el("div", { class: "dk-keys" });
    const paint = () => {
      const q = (this.find || "").toLowerCase();
      body.replaceChildren(...modes.flatMap((mode) => {
        const rows = list.map((s, i) => [s, i]).filter(([s]) => s.mode === mode && !auto(s)
          && (!q || s.key.toLowerCase().includes(q) || s.run.join(" ").toLowerCase().includes(q)));
        const head = mode === "main" ? "Always on" : `${mode} mode${entered(mode) ? ` · entered with ${entered(mode)}` : ""}`;
        const out = [el("div", { class: "dk-keys-h" }, head,
          el("button", { class: "link", onclick: () => { list.push({ mode, key: "", run: [""] }); this.changed(t); paint(); body.querySelector(`.dk-keys-row[data-i="${list.length - 1}"] .dk-key`)?.focus(); } }, "+ Add"))];
        if (mode === "main" && !q) {
          out.push(el("div", { class: "dk-keys-row locked" },
            el("span", { class: "dk-key" }, `${m.modifier}-1…${m.workspaces}`), el("span", { class: "sub" }, `Go to workspace 1…${m.workspaces}; with shift: move the window there (set by How many, above)`)));
        }
        return out.concat(rows.map(([s, i]) => this.shortcut(s, i, counts[`${s.mode} ${s.key}`] > 1, ids, paint)));
      }));
    };
    filter.addEventListener("input", () => { this.find = filter.value; paint(); });
    paint();
    return el("div", {},
      el("p", { class: "note" }, "Type a shortcut (alt-shift-h) or press Record and then the keys. A shortcut AeroSpace already uses goes to AeroSpace first, so type those. Several commands: separate them with ;"),
      filter, datalist, body);
  },

  shortcut(s, i, dup, ids, repaint) {
    const t = "windows";
    const list = this.model.windows.shortcuts;
    const key = el("input", { class: `search dk-key${dup ? " bad" : ""}`, type: "text", value: s.key, placeholder: "alt-h", spellcheck: "false", title: dup ? "Used twice in this mode" : "" });
    key.addEventListener("change", () => { s.key = key.value.trim(); this.changed(t); repaint(); });
    const rec = el("button", { class: "btn", title: "Record: press the keys" }, "⏺");
    rec.onclick = () => this.record(rec, (combo) => { s.key = combo; this.changed(t); repaint(); });
    const cmd = el("input", { class: "search dk-text", type: "text", value: s.run.join("; "), list: ids, placeholder: "focus left", spellcheck: "false" });
    cmd.addEventListener("change", () => { s.run = cmd.value.split(";").map((c) => c.trim()).filter(Boolean); this.changed(t); });
    const what = (this.commands || []).find(([c]) => c === s.run[0])?.[1];
    if (what) cmd.title = what;
    const remove = el("button", { class: "btn", title: "Remove this shortcut", onclick: () => { list.splice(i, 1); this.changed(t); repaint(); } }, "×");
    return el("div", { class: "dk-keys-row", "data-i": i }, key, rec, cmd, remove);
  },

  /** The next key pressed (with modifiers) as AeroSpace writes it. */
  record(btn, done) {
    const NAMES = { Minus: "minus", Equal: "equal", Period: "period", Comma: "comma", Slash: "slash", Backslash: "backslash",
      Quote: "quote", Semicolon: "semicolon", Backquote: "backtick", BracketLeft: "leftSquareBracket", BracketRight: "rightSquareBracket",
      Space: "space", Enter: "enter", Escape: "esc", Backspace: "backspace", Tab: "tab", PageUp: "pageUp", PageDown: "pageDown",
      Home: "home", End: "end", Delete: "forwardDelete", IntlBackslash: "sectionSign", ArrowLeft: "left", ArrowRight: "right",
      ArrowUp: "up", ArrowDown: "down", NumpadEnter: "keypadEnter", NumpadAdd: "keypadPlus", NumpadSubtract: "keypadMinus",
      NumpadMultiply: "keypadMultiply", NumpadDivide: "keypadDivide", NumpadDecimal: "keypadDecimalMark", NumpadEqual: "keypadEqual", NumLock: "keypadClear" };
    const name = (code) => NAMES[code] || code.match(/^Key([A-Z])$/)?.[1].toLowerCase() || code.match(/^Digit(\d)$/)?.[1]
      || (code.match(/^Numpad(\d)$/) && `keypad${code.slice(6)}`) || (code.match(/^F(\d+)$/) && code.toLowerCase());
    const stop = () => { window.removeEventListener("keydown", on, true); btn.classList.remove("on"); btn.textContent = "⏺"; clearTimeout(timer); };
    const on = (e) => {
      if (["Shift", "Control", "Alt", "Meta"].includes(e.key)) return;
      e.preventDefault(); e.stopPropagation();
      const k = name(e.code);
      stop();
      if (!k) return toast(`${e.code} isn't a key AeroSpace can use`, true);
      done([e.ctrlKey && "ctrl", e.metaKey && "cmd", e.altKey && "alt", e.shiftKey && "shift", k].filter(Boolean).join("-"));
    };
    btn.classList.add("on");
    btn.textContent = "…";
    window.addEventListener("keydown", on, true);
    const timer = setTimeout(stop, 6000);
  },

  // ---------------- prompt (Starship)

  prompt() {
    const t = "prompt";
    const m = this.model.prompt;
    const presets = el("select", { class: "select" }, el("option", { value: "" }, "Start from a preset…"));
    invoke("prompt_presets").then((list) => presets.append(...list.map((p) => el("option", { value: p }, p))));
    const use = this.btn("Use it", async () => {
      if (!presets.value) return toast("Pick a preset first", true);
      if (!armed(use, "Click again: it replaces your starship.toml (kept as a backup)")) return;
      try {
        const r = await invoke("prompt_preset", { name: presets.value });
        this.report(r, `Your prompt is now ${presets.value}, in the theme's colours`);
      } catch (e) { toast(String(e), true); }
      this.render(t);
    });
    const blocks = el("div", { class: "pr-blocks" }, ...m.blocks.map((b, i) => this.promptBlock(b, i)));
    const add = this.btn("New block", () => { m.blocks.push({ colour: "lighter_background", text: "foreground", modules: [] }); this.changed(t); this.draw(t); });
    return [
      el("div", { class: "pr-preview", id: "pr-preview" }, el("span", { class: "sub" }, "Drawing the prompt…")),
      el("p", { class: "note" }, "The real prompt in your home folder, after a 4 s command. Git and languages show inside a project."),
      el("div", { class: "actions" }, presets, use),
      el("h2", {}, "Look"),
      this.row("Between blocks", null, this.select(t, "shape", [["arrow", "Arrows"], ["round", "Rounded"], ["slant", "Slants"], ["flat", "Touching"], ["plain", "Plain text, no blocks"]])),
      this.row("Left end", null, this.select(t, "start", [["fade", "Fade in ░▒▓"], ["round", "Rounded"], ["arrow", "Arrow"], ["none", "Square"]])),
      this.row("Type on a new line", "The ❯ goes on a line of its own", this.toggle(t, "second_line")),
      this.row("Blank line between prompts", null, this.toggle(t, "add_newline")),
      this.row("Language versions", "rust 1.90 rather than just the logo", this.toggle(t, "versions")),
      el("h2", {}, "Blocks"),
      el("p", { class: "note" }, "Drag modules between blocks; click one for its options. A block's colour is its background; the text colour is fixed per theme if it would be unreadable."),
      blocks, el("div", { class: "actions" }, add),
      el("div", { id: "pr-opts" }),
      el("h2", {}, "Prompt sign"),
      this.promptSign(),
      this.applyBar(t),
    ].concat(this.paintSoon(() => { this.previewSoon(0); this.paintPromptOpts(); }));
  },

  promptBlock(b, bi) {
    const t = "prompt";
    const m = this.model.prompt;
    const c = activeColours();
    const pick = (key) => {
      const s = el("select", { class: "select", title: key === "colour" ? "Background" : "Text" },
        ...(COLOUR_KEYS.includes(b[key]) ? [] : [el("option", { value: b[key] }, b[key])]),
        ...COLOUR_KEYS.map((k) => { const o = el("option", { value: k }, LABEL[k] || k); if (k === b[key]) o.selected = true; return o; }));
      s.onchange = () => { b[key] = s.value; this.changed(t); this.draw(t); };
      return s;
    };
    const name = (id) => this.promptMeta?.modules.find((x) => x.id === id)?.name || id;
    const chips = b.modules.map((id, i) => {
      const sel = this.promptSel === id;
      const chip = el("span", { class: `dk-it${sel ? " sel" : ""}`, draggable: "true", style: { color: c[b.text] || b.text },
        onclick: () => { this.promptSel = id; this.draw(t); } }, name(id),
        el("button", { class: "link", title: "Take out", onclick: (e) => { e.stopPropagation(); b.modules.splice(i, 1); this.changed(t); this.draw(t); } }, "×"));
      chip.addEventListener("dragstart", (e) => { this.drag = { b: bi, i }; e.dataTransfer.setData("text/plain", id); });
      chip.addEventListener("dragover", (e) => { e.preventDefault(); e.stopPropagation(); });
      chip.addEventListener("drop", (e) => { e.preventDefault(); e.stopPropagation(); this.promptDrop(bi, i); });
      return chip;
    });
    const box = el("div", { class: "pr-block", style: { background: c[b.colour] || b.colour } }, ...chips,
      el("button", { class: "dk-add", title: "Add a module", onclick: (e) => { e.stopPropagation(); this.promptMenu(bi, e.currentTarget); } }, "+"));
    box.addEventListener("dragover", (e) => e.preventDefault());
    box.addEventListener("drop", (e) => { e.preventDefault(); this.promptDrop(bi, b.modules.length); });
    const move = (by) => {
      const j = bi + by;
      if (j < 0 || j >= m.blocks.length) return;
      [m.blocks[bi], m.blocks[j]] = [m.blocks[j], m.blocks[bi]];
      this.changed(t); this.draw(t);
    };
    return el("div", { class: "pr-row" }, box,
      el("div", { class: "pr-tools" }, pick("colour"), pick("text"),
        this.btn("▲", () => move(-1)), this.btn("▼", () => move(1)),
        this.btn("×", () => { m.blocks.splice(bi, 1); this.changed(t); this.draw(t); })));
  },

  promptDrop(bi, i) {
    const d = this.drag;
    this.drag = null;
    if (!d || d.b === undefined) return;
    const blocks = this.model.prompt.blocks;
    const [id] = blocks[d.b].modules.splice(d.i, 1);
    if (d.b === bi && d.i < i) i -= 1;
    blocks[bi].modules.splice(i, 0, id);
    this.changed("prompt");
    this.draw("prompt");
  },

  promptMenu(bi, anchor) {
    const m = this.model.prompt;
    const used = new Set(m.blocks.flatMap((b) => b.modules));
    const groups = [["Where you are", (x) => !x.language && !["time", "cmd_duration", "jobs", "memory_usage", "battery", "status"].includes(x.id)],
      ["Languages", (x) => x.language], ["Status", (x) => ["time", "cmd_duration", "jobs", "memory_usage", "battery", "status"].includes(x.id)]];
    const menu = el("div", { class: "dk-menu" }, ...groups.flatMap(([title, f]) => {
      const ms = this.promptMeta.modules.filter((x) => f(x) && !used.has(x.id));
      if (!ms.length) return [];
      return [el("div", { class: "dk-menu-h" }, title), ...ms.map((x) => el("button", {
        class: "dk-menu-i", title: x.what,
        onclick: () => { m.blocks[bi].modules.push(x.id); this.promptSel = x.id; menu.remove(); this.changed("prompt"); this.draw("prompt"); },
      }, x.name))];
    }));
    document.querySelector(".dk-menu")?.remove();
    anchor.after(menu);
    setTimeout(() => document.addEventListener("click", () => menu.remove(), { once: true }));
  },

  /** Rows for a module's options (the ones the page knows). */
  /** The sign you type after (14d-4): click one, or type your own. Green after a command
   *  worked, red after one failed: the theme's own green and red (Palette's Starship palette).
   *  Written as Starship's `[sign](bold green)`; its special characters are escaped. */
  promptSign() {
    const m = this.model.prompt;
    const SIGNS = ["❯", "➜", "λ", "›", "$", "▶", "→", "%", "#"];
    const opts = () => m.options.character || {};
    // `[❯](bold green)` → ❯ (backslash escapes undone); anything else is shown as typed.
    const shown = (v) => {
      const hit = /^\[(.*)\]\([^)]*\)$/.exec(v || "");
      return hit ? hit[1].replace(/\\(.)/g, "$1") : (v || "");
    };
    const now = shown(opts().success_symbol);
    const set = (sign) => {
      m.options.character = m.options.character || {};
      if (!sign) {
        delete m.options.character.success_symbol;
        delete m.options.character.error_symbol;
      } else {
        const text = sign.replace(/[\\[\]()$]/g, (c) => `\\${c}`);
        m.options.character.success_symbol = `[${text}](bold green)`;
        m.options.character.error_symbol = `[${text}](bold red)`;
      }
      this.changed("prompt");
      this.draw("prompt");
    };
    const pick = (sign, label = sign) => el("button", {
      class: `btn${(sign || "") === now ? " on" : ""}`, title: sign ? `Use ${sign}` : "Starship's own sign (❯)",
      onclick: () => set(sign),
    }, label);
    const own = el("input", { class: "search dk-text", type: "text", value: SIGNS.includes(now) ? "" : now, placeholder: "Your own, e.g. >>", spellcheck: "false" });
    own.onchange = () => set(own.value.trim());
    return el("div", {},
      this.row("Sign", "Green after a command worked, red after one failed: the theme's own green and red",
        el("div", { class: "seg" }, ...SIGNS.map((x) => pick(x)), pick("", "Default"))),
      this.row("Your own", "Any text instead; the colours still follow the theme", own));
  },

  promptOptions(module) {
    const m = this.model.prompt;
    const opts = (this.promptMeta?.options || []).filter((o) => o.module === module);
    return opts.map((o) => {
      const cur = m.options[module]?.[o.key];
      const set = (v) => {
        m.options[module] = m.options[module] || {};
        if (v === "" || v === null) delete m.options[module][o.key];
        else m.options[module][o.key] = v;
        this.changed("prompt");
      };
      let input;
      if (o.kind === "bool") {
        input = el("input", { type: "checkbox", class: "switch" });
        input.checked = !!cur;
        input.onchange = () => set(input.checked);
      } else {
        input = el("input", { class: `search ${o.kind === "int" ? "dk-num" : "dk-text"}`, type: o.kind === "int" ? "number" : "text", value: cur ?? "", placeholder: "Starship's default", spellcheck: "false" });
        input.onchange = () => set(o.kind === "int" ? (input.value === "" ? "" : Number(input.value)) : input.value);
      }
      return this.row(o.label, null, input);
    });
  },

  paintPromptOpts() {
    const box = $("#pr-opts");
    if (!box) return;
    const id = this.promptSel;
    const meta = this.promptMeta?.modules.find((x) => x.id === id);
    if (!id || !this.model.prompt.blocks.some((b) => b.modules.includes(id))) {
      return box.replaceChildren(el("p", { class: "note" }, "Click a module for its options."));
    }
    const rows = this.promptOptions(id);
    box.replaceChildren(el("div", { class: "dk-item" },
      el("div", { class: "dk-item-h" }, el("b", {}, meta?.name || id), el("span", { class: "sub" }, meta?.what || "Yours: Palette places it but keeps its own style")),
      ...(rows.length ? rows : [el("p", { class: "note" }, "Nothing to set here: its colours come from its block.")])));
  },

  /** Draw the real prompt (starship) for the settings being edited. */
  previewSoon(ms = 400) {
    clearTimeout(this.prTimer);
    this.prTimer = setTimeout(async () => {
      const box = $("#pr-preview");
      if (!box) return;
      try {
        const spans = await invoke("prompt_preview", { settings: this.model.prompt });
        const c = activeColours();
        const col = (v) => (!v ? "" : v.startsWith("#") ? v : c[v] || c[{ white: "foreground", black: "dark_background" }[v.replace("bright_", "")]] || v);
        box.style.background = c.background;
        box.replaceChildren(...spans.map((s) => el("span", { style: { color: col(s.fg) || c.foreground, background: col(s.bg) || "transparent", fontWeight: s.bold ? "700" : "400" } }, s.text)));
      } catch (e) {
        box.replaceChildren(el("span", { class: "sub" }, String(e)));
      }
    }, ms);
  },

  // ---------------- Dock & menu bar (applies at once)

  async macos() {
    const box = $("#dk-macos");
    let m;
    try { m = await invoke("macos_get"); } catch (e) { return box.replaceChildren(el("div", { class: "empty" }, String(e))); }
    const save = async () => {
      try {
        await invoke("macos_set", { settings: m });
      } catch (e) {
        toast(`${e}\nmacOS may ask to let Palette control System Events (as for dark mode): allow it in System Settings → Privacy & Security → Automation.`, true, 9000);
      }
      this.macos();
    };
    const tog = (k) => { const c = el("input", { type: "checkbox", class: "switch" }); c.checked = m[k]; c.onchange = () => { m[k] = c.checked; save(); }; return c; };
    const edge = el("select", { class: "select" }, ...[["bottom", "Bottom"], ["left", "Left"], ["right", "Right"]].map(([v, l]) => { const o = el("option", { value: v }, l); if (v === m.dock_position) o.selected = true; return o; }));
    edge.onchange = () => { m.dock_position = edge.value; save(); };
    const size = el("input", { type: "range", min: 16, max: 128, value: m.dock_size });
    const out = el("span", { class: "dk-val" }, `${m.dock_size} pt`);
    size.oninput = () => (out.textContent = `${size.value} pt`);
    size.onchange = () => { m.dock_size = Number(size.value); save(); };
    box.replaceChildren(
      el("h2", {}, "Menu bar"),
      this.row("Hide the menu bar", "It slides down when the pointer reaches the top. Your bar can sit there instead. On a MacBook screen (the notch) windows still stay below it", tog("menu_bar_autohide")),
      el("h2", {}, "Dock"),
      this.row("Hide the Dock", "It appears when the pointer reaches its edge", tog("dock_autohide")),
      this.row("Position", null, edge),
      this.row("Size", null, el("span", { class: "dk-slider" }, size, out)),
      this.row("Magnify under the pointer", null, tog("dock_magnify")),
    );
  },

  rule(r, i) {
    const t = "windows";
    const m = this.model.windows;
    // Built from the rule each time, so a refill keeps the app picked (saved or not).
    const app = this.appMenu(el("select", { class: "select" }, el("option", { value: r.app_id }, r.app_name || r.app_id)), (apps) =>
      [[r.app_id, r.app_name || r.app_id], ...apps.filter(([id]) => id !== r.app_id)]);
    app.onchange = () => { r.app_id = app.value; r.app_name = app.selectedOptions[0].textContent; this.changed(t); };
    const action = el("select", { class: "select" },
      ...[["float", "Floats"], ["tile", "Tiles"], ["workspace", "Opens on workspace"]].map(([v, l]) => {
        const o = el("option", { value: v }, l);
        if (v === r.action) o.selected = true;
        return o;
      }));
    const where = el("input", { class: "search dk-num", type: "number", min: 1, max: m.workspaces, value: r.workspace });
    where.hidden = r.action !== "workspace";
    action.onchange = () => { r.action = action.value; where.hidden = r.action !== "workspace"; this.changed(t); };
    where.onchange = () => { r.workspace = Math.min(m.workspaces, Math.max(1, Number(where.value) || 1)); this.changed(t); };
    const remove = el("button", { class: "btn", title: "Remove this rule", onclick: () => { m.rules.splice(i, 1); this.changed(t); this.draw(t); } }, "×");
    return el("div", { class: "row dk-rule" }, app, action, where, remove);
  },

};

/** The active theme's colours, by key. */
function activeColours() {
  return (themes.find((t) => t.current && !t.invalid) || themes.find((t) => !t.invalid))?.colors || {};
}
