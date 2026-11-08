// Settings → Bar, Windows, Borders: Palette's pages for SketchyBar, AeroSpace
// and JankyBorders (`palette desktop` in the terminal). Loaded after shared.js and settings.js
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
    if (tool === "bar") { this.paintBar(); this.paintFit(); }
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
    const revert = this.btn("Undo changes", () => { this.model[tool] = JSON.parse(this.saved[tool]); this.sel = null; this.draw(tool); });
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
    if (!this.apps) this.apps = await invoke("running_apps").catch(() => []);
    return this.apps;
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
    const pick = el("select", { class: "select" }, el("option", { value: "" }, "Add a running app…"));
    this.runningApps().then((apps) => pick.append(...apps.filter(([, n]) => !list.includes(n)).map(([, n]) => el("option", { value: n }, n))));
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
          el("button", { class: "link", onclick: () => { list.push({ mode, key: "", run: [""] }); this.changed(t); paint(); body.querySelector(".dk-keys-row:last-of-type .dk-key")?.focus(); } }, "+ Add"))];
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
    return el("div", { class: "dk-keys-row" }, key, rec, cmd, remove);
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
    const app = el("select", { class: "select" }, el("option", { value: r.app_id }, r.app_name || r.app_id));
    this.runningApps().then((apps) => app.append(...apps.filter(([id]) => id !== r.app_id).map(([id, n]) => el("option", { value: id }, n))));
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

  // ---------------- bar

  kind(id) { return this.kinds.find((k) => k.id === id) || { id, name: id, what: "" }; },

  bar() {
    const t = "bar";
    const m = this.model.bar;
    const palettePill = m.pills.some((p) => p.items.some((i) => i.kind === "palette"));
    const addSide = (side, label) => this.btn(label, () => {
      m.pills.push({ side, items: [] });
      this.changed(t);
    });
    return [
      el("div", { class: "dk-bar-wrap" }, el("div", { class: "dk-bar", id: "dk-bar-preview" })),
      el("p", { class: "note" }, "To move a whole pill, click one of its items and use Pill ◀ ▶: it walks left → centre → right. Centre pills sit in the middle of the screen: behind the notch on a MacBook screen that has one."),
      el("div", { class: "actions" }, addSide("left", "New pill on the left"), addSide("center", "New pill in the centre"), addSide("right", "New pill on the right"),
        el("span", { class: "spacer" }),
        el("label", { class: "dk-inline" }, "Palette's theme pill ", this.paletteToggle(palettePill)),
        palettePill ? el("label", { class: "dk-inline", title: "Scrolling on Palette's pill steps to the next or previous theme. Off: the pill only opens Palette when clicked (also palette fav scroll on|off)" }, "Scroll on it to switch theme ", this.scrollToggle()) : null),
      el("div", { id: "dk-item" }),
      el("h2", {}, "The bar"),
      this.row("Position", "Top or bottom of every display", this.select(t, "position", [["top", "Top"], ["bottom", "Bottom"]])),
      this.row("Height", "Points. Match the menu bar: as tall as the Mac's menu bar strip, pills and text to suit", this.num(t, "height", 10, 80),
        this.btn("Match the menu bar", async () => {
          try {
            this.model.bar = await invoke("bar_match_menu_bar", { settings: this.model.bar });
            this.changed(t);
            this.draw(t);
          } catch (e) { toast(String(e), true); }
        })),
      this.row("Distance from the edge", "Points between the screen's edge and the bar. Windows don't move with it: leave room in Windows → Top gap", this.num(t, "y_offset", -40, 200)),
      this.row("Side margins", "Space left and right of the bar", this.num(t, "margin", 0, 400)),
      this.row("Padding", "Space inside the bar's ends", this.num(t, "padding", 0, 100)),
      this.row("Background", "Behind the whole bar; none shows only the pills", this.colour(t, "background", "None")),
      this.row("Corner radius", "Rounds the bar's background (0: square)", this.num(t, "corner_radius", 0, 40)),
      this.row("Blur", "Blurs what's behind the bar's background (0: none)", this.num(t, "blur_radius", 0, 100)),
      el("h2", {}, "Pills"),
      this.row("Colour", "A theme colour, so it changes with the theme", this.colour(t, "pill.colour")),
      this.row("Opacity", "How solid the pills are (100: no wallpaper shows through)", this.slider(t, "pill.opacity", 0, 100, 1, "%")),
      this.row("Height", "Pills keep this height whatever the text size", this.num(t, "pill.height", 6, 80)),
      el("p", { class: "warn dk-fit", hidden: "" }),
      this.row("Corner radius", "Half the pill height gives fully round ends", this.num(t, "pill.corner_radius", 0, 40)),
      this.row("Border", "Width (0: none) and colour", this.num(t, "pill.border_width", 0, 6), this.colour(t, "pill.border_colour")),
      this.row("Space between pills", "Points between two pills on the same side", this.num(t, "pill.gap", 0, 60)),
      this.row("Space inside the ends", "Points between a pill's edge and its first and last item", this.num(t, "pill.edge", 0, 40)),
      el("h2", {}, "Text and icons"),
      this.row("Font", "A Nerd Font shows the icons. Only the styles the font has are listed", ...this.font()),
      this.row("Sizes", "Icons and text, points", this.num(t, "icon_size", 6, 40, 0.5), this.num(t, "label_size", 6, 40, 0.5)),
      el("p", { class: "warn dk-fit", hidden: "" }),
      this.row("Icon colour", "A theme colour, so it changes with the theme", this.colour(t, "icon_colour")),
      this.row("Text colour", "A theme colour, so it changes with the theme", this.colour(t, "text_colour")),
      this.row("Stats refresh", "CPU, memory, network and the rest, every … seconds", this.num(t, "stats_every", 1, 600)),
      this.applyBar(t),
    ].concat(this.paintSoon(() => this.paintFit()));
  },

  /** Pills never grow by themselves (owner): say so when the text won't fit them, or a pill is
   *  taller than the bar (SketchyBar cuts it off). Text needs about 1.3 × its size, plus room. */
  paintFit() {
    const m = this.model.bar;
    if (!m) return;
    const size = Math.max(m.icon_size, m.label_size);
    const need = Math.ceil(size * 1.3) + 2;
    const msgs = [];
    if (m.pill.height < need) {
      msgs.push(`Text at ${size} pt needs pills about ${need} pt tall (now ${m.pill.height}): raise Pills → Height, or make the text smaller.`);
    }
    if (m.pill.height > m.height) {
      msgs.push(`Pills (${m.pill.height} pt) are taller than the bar (${m.height} pt), so their tops and bottoms are cut off: raise The bar → Height.`);
    }
    document.querySelectorAll(".dk-fit").forEach((p) => {
      p.textContent = msgs.join(" ");
      p.hidden = !msgs.length;
    });
  },

  /** Installed families (Nerd Fonts first) and the chosen one's real styles. */
  font() {
    const m = this.model.bar;
    const family = el("select", { class: "select dk-font" }, el("option", { value: m.font }, m.font));
    const style = el("select", { class: "select" }, el("option", { value: m.font_style }, m.font_style));
    const styles = async () => {
      const have = await invoke("font_styles", { family: m.font }).catch(() => []);
      if (have.length && !have.includes(m.font_style)) m.font_style = have.includes("Bold") ? "Bold" : have.includes("Regular") ? "Regular" : have[0];
      const opts = have.length ? have : [m.font_style];
      style.replaceChildren(...opts.map((v) => { const o = el("option", { value: v }, v); if (v === m.font_style) o.selected = true; return o; }));
      if (!have.length) style.title = `${m.font} isn't installed`;
      this.changed("bar");
    };
    invoke("font_families").then((all) => {
      const nerd = all.filter((f) => /nerd/i.test(f)), rest = all.filter((f) => !/nerd/i.test(f));
      const group = (label, fs) => el("optgroup", { label }, ...fs.map((f) => { const o = el("option", { value: f }, f); if (f === m.font) o.selected = true; return o; }));
      const missing = all.includes(m.font) ? [] : [el("option", { value: m.font, selected: "" }, `${m.font} (not installed)`)];
      family.replaceChildren(...missing, ...(nerd.length ? [group("Nerd Fonts (with icons)", nerd)] : []), group("Other fonts", rest));
    }).catch(() => {});
    family.onchange = () => { m.font = family.value; styles(); };
    style.onchange = () => { m.font_style = style.value; this.changed("bar"); };
    styles();
    return [family, style];
  },

  text(tool, path) {
    const input = el("input", { class: "search dk-text", type: "text", value: this.get(tool, path), spellcheck: "false" });
    input.addEventListener("change", () => this.put(tool, path, input.value.trim()));
    return input;
  },

  paletteToggle(on) {
    const c = el("input", { type: "checkbox", class: "switch" });
    c.checked = on;
    c.onchange = () => {
      const m = this.model.bar;
      const first = m.pills.findIndex((p) => p.side === "right");
      const pill = { side: "right", items: [{ kind: "palette" }] };
      if (c.checked) first < 0 ? m.pills.push(pill) : m.pills.splice(first, 0, pill);
      else m.pills = m.pills.filter((p) => !p.items.some((i) => i.kind === "palette"));
      this.changed("bar");
    };
    return c;
  },

  /** The pill's scroll switch: a Palette setting (`settings.toml`), not part of the bar's
   *  model, so it saves at once instead of with the bar. */
  scrollToggle() {
    const c = el("input", { type: "checkbox", class: "switch", "aria-label": "Scroll on Palette's pill to switch theme" });
    invoke("prefs").then((p) => { c.checked = p.bar_scroll; }).catch(() => {});
    c.onchange = async () => {
      try {
        c.checked = await invoke("set_bar_scroll", { on: c.checked });
        toast(c.checked ? "Scroll on the bar pill switches theme" : "The bar pill only opens Palette now");
      } catch (e) { toast(String(e), true); }
    };
    return c;
  },

  /** The bar drawn in the active theme: pills with draggable items. */
  paintBar() {
    const box = $("#dk-bar-preview");
    const m = this.model.bar;
    if (!box || !m) return;
    const c = activeColours();
    box.style.background = m.background ? c[m.background] : "transparent";
    box.style.borderRadius = `${m.corner_radius}px`;
    const side = (s) => el("div", { class: `dk-side ${s}`, title: s === "center" ? "Centre" : "" },
      ...m.pills.map((p, pi) => [p, pi]).filter(([p]) => p.side === s).map(([p, pi]) => this.pill(p, pi, c)));
    box.replaceChildren(side("left"), el("span", { class: "spacer" }), side("center"), el("span", { class: "spacer" }), side("right"));
    // A screen-wide bar in a narrower pane: shrink it to fit, like a miniature.
    const wrap = box.parentElement;
    const scale = Math.min(1, (wrap.clientWidth - 20) / box.scrollWidth);
    box.style.transform = `scale(${scale.toFixed(3)})`;
    wrap.style.height = `${Math.ceil(box.offsetHeight * scale) + 20}px`;
    this.paintItem();
  },

  pill(p, pi, c) {
    const m = this.model.bar;
    const fixed = p.items.some((i) => i.kind === "palette");
    const pill = el("div", {
      class: `dk-pill${fixed ? " fixed" : ""}`,
      style: { background: `color-mix(in srgb, ${c[m.pill.colour]} ${m.pill.opacity}%, transparent)`, borderRadius: `${Math.min(m.pill.corner_radius, 14)}px`, border: m.pill.border_width ? `${m.pill.border_width}px solid ${c[m.pill.border_colour]}` : "none" },
    });
    // An item dropped here goes to the end of this pill. Whole pills move with Pill ◀ ▶.
    if (!fixed) {
      pill.addEventListener("dragover", (e) => { e.preventDefault(); pill.classList.add("over"); });
      pill.addEventListener("dragleave", () => pill.classList.remove("over"));
      pill.addEventListener("drop", (e) => { e.preventDefault(); this.dropAt(pi, p.items.length); });
    }
    for (const [i, item] of p.items.entries()) pill.append(this.chip(item, pi, i, c, fixed));
    if (!fixed) {
      pill.append(el("button", { class: "dk-add", title: "Add to this pill", onclick: (e) => { e.stopPropagation(); this.addMenu(pi, e.currentTarget); } }, "+"));
      if (!p.items.length) pill.append(el("button", { class: "dk-add", title: "Remove this empty pill", onclick: () => { m.pills.splice(pi, 1); this.changed("bar"); } }, "×"));
    }
    return pill;
  },

  chip(item, pi, i, c, fixed) {
    const m = this.model.bar;
    const k = this.kind(item.kind);
    const sel = this.sel && this.sel.p === pi && this.sel.i === i;
    // Short, like the real bar: an icon (the dot) and a sample value.
    const SAMPLE = { cpu: "12", gpu: "4", temp: "41°", pwr: "6W", mem: "58", disk: "81", vpn: "VPN", net: "2.1M",
      battery: "87", volume: "40", brightness: "70", weather: "31°", habits: "3/5", dispatch: "3 read", front_app: "Ghostty" };
    let face = SAMPLE[item.kind] || k.name;
    if (item.kind === "workspaces") face = Array.from({ length: this.workspaceCount() }, (_, n) => n + 1).join(" ");
    if (item.kind === "link" || item.kind === "script") face = item.label || item.icon || k.name;
    if (item.kind === "palette") face = themes.find((x) => x.current)?.name || "Palette";
    if (item.kind === "clock") face = new Date().toLocaleString("en-GB", { weekday: "short", day: "2-digit", hour: "2-digit", minute: "2-digit", hour12: false }).replace(",", "");
    const chip = el("span", {
      class: `dk-it${sel ? " sel" : ""}`, draggable: fixed ? "false" : "true", title: k.what,
      style: { color: c[m.text_colour] },
      onclick: (e) => { e.stopPropagation(); this.sel = { p: pi, i }; this.paintBar(); },
    }, el("b", { style: { background: c[m.icon_colour] } }), face);
    if (!fixed) {
      chip.addEventListener("dragstart", (e) => { this.drag = { p: pi, i }; e.dataTransfer.effectAllowed = "move"; e.dataTransfer.setData("text/plain", "item"); });
      chip.addEventListener("dragover", (e) => { e.preventDefault(); e.stopPropagation(); chip.classList.add("over"); });
      chip.addEventListener("dragleave", () => chip.classList.remove("over"));
      chip.addEventListener("drop", (e) => { e.preventDefault(); e.stopPropagation(); this.dropAt(pi, i); });
    }
    return chip;
  },

  /** The selected pill one place left or right; at the end of its side it moves
   *  to the next side (left, centre, right). The core does the moving. */
  movePill(by) {
    this.placePill(this.sel.p, { to: by > 0 ? "next" : "prev" });
  },

  /** Move pill `from` with the core's `Bar::move_pill` (same as `palette desktop bar move`). */
  async placePill(from, where) {
    const keep = this.sel?.p === from ? this.sel.i : null;
    try {
      const [bar, at] = await invoke("bar_move_pill", { settings: this.model.bar, from, to: where.to ?? null, before: where.before ?? null });
      // Only the pills change; keep the same model object, which the page's controls hold.
      this.model.bar.pills = bar.pills;
      this.sel = keep != null ? { p: at, i: keep } : null;
      this.changed("bar");
    } catch (e) { toast(String(e), true); }
  },

  workspaceCount() {
    return this.workspaces || 6;
  },

  dropAt(p, i) {
    const d = this.drag;
    this.drag = null;
    if (!d) return;
    const pills = this.model.bar.pills;
    const [item] = pills[d.p].items.splice(d.i, 1);
    if (d.p === p && d.i < i) i -= 1;
    pills[p].items.splice(i, 0, item);
    this.sel = { p, i };
    this.changed("bar");
  },

  addMenu(pi, anchor) {
    const m = this.model.bar;
    const used = new Set(m.pills.flatMap((p) => p.items.map((i) => i.kind)));
    const groups = [["desktop", "Desktop"], ["system", "Mac"], ["stats", "Stats"], ["yours", "Yours"]];
    const menu = el("div", { class: "dk-menu" }, ...groups.flatMap(([g, title]) => {
      const ks = this.kinds.filter((k) => k.group === g && k.id !== "palette" && !(k.single && used.has(k.id)));
      if (!ks.length) return [];
      return [el("div", { class: "dk-menu-h" }, title), ...ks.map((k) => el("button", {
        class: "dk-menu-i",
        onclick: () => {
          const item = { kind: k.id };
          if (k.id === "link") Object.assign(item, { label: "Link", click: "open https://example.com" });
          if (k.id === "script") Object.assign(item, { icon: "", label: "--", path: "~/.config/sketchybar/plugins/my.sh" });
          m.pills[pi].items.push(item);
          this.sel = { p: pi, i: m.pills[pi].items.length - 1 };
          menu.remove();
          this.changed("bar");
        },
      }, el("span", { class: "dk-menu-t" }, el("span", {}, k.name, k.network ? el("span", { class: "badge" }, "network") : null),
        k.what ? el("span", { class: "dk-menu-w" }, k.what) : null)))];
    }));
    document.querySelector(".dk-menu")?.remove();
    // On <body> with fixed position: the bar preview clips anything inside it.
    document.body.append(menu);
    const r = anchor.getBoundingClientRect();
    const below = window.innerHeight - r.bottom - 12, above = r.top - 12;
    const down = below >= 240 || below >= above;
    menu.style.maxHeight = `${Math.max(160, down ? below : above)}px`;
    menu.style.left = `${Math.max(8, Math.min(r.left, window.innerWidth - menu.offsetWidth - 8))}px`;
    if (down) menu.style.top = `${r.bottom + 4}px`; else menu.style.bottom = `${window.innerHeight - r.top + 4}px`;
    const close = (e) => { if (e.type === "keydown" && e.key !== "Escape") return; if (e.type === "click" && menu.contains(e.target)) return; menu.remove(); off(); };
    const off = () => { document.removeEventListener("click", close, true); document.removeEventListener("keydown", close, true); window.removeEventListener("scroll", onScroll, true); };
    const onScroll = (e) => { if (!menu.contains(e.target)) { menu.remove(); off(); } };
    setTimeout(() => { document.addEventListener("click", close, true); document.addEventListener("keydown", close, true); window.addEventListener("scroll", onScroll, true); });
  },

  /** The selected item's settings. */
  paintItem() {
    const box = $("#dk-item");
    if (!box) return;
    const m = this.model.bar;
    const item = this.sel && m.pills[this.sel.p]?.items[this.sel.i];
    if (!item) return box.replaceChildren(el("p", { class: "note" }, "Click an item to change what it shows, how often it refreshes and what a click opens."));
    const k = this.kind(item.kind);
    const set = (key, v) => { item[key] = v; this.changed("bar"); };
    const input = (key, placeholder, width = "") => {
      const x = el("input", { class: `search dk-text ${width}`, type: "text", value: item[key] ?? "", placeholder, spellcheck: "false" });
      x.addEventListener("change", () => set(key, x.value));
      return x;
    };
    const rows = [];
    if (item.kind !== "palette" && item.kind !== "workspaces") {
      const mode = item.click === "none" ? "none" : item.click ? "custom" : "default";
      const how = el("select", { class: "select" },
        ...[["default", k.click ? "The usual" : "Nothing"], ["none", "Nothing"], ["custom", "Open or run…"]].map(([v, l]) => {
          const o = el("option", { value: v }, l);
          if (v === mode) o.selected = true;
          return o;
        }));
      const cmd = input("click", "open -a Calendar   or   open https://…");
      cmd.hidden = mode !== "custom";
      how.onchange = () => { cmd.hidden = how.value !== "custom"; set("click", how.value === "none" ? "none" : how.value === "default" ? "" : cmd.value || k.click); cmd.value = item.click; };
      rows.push(this.row("Click", k.click && mode === "default" ? `Opens: ${k.click}` : "An app: open -a Name · a website or settings pane: open URL", how, cmd));
    }
    if (k.every > 0) {
      const every = el("input", { class: "search dk-num", type: "number", min: 1, max: 86400, value: item.every || k.every });
      every.onchange = () => set("every", Number(every.value) || 0);
      rows.push(this.row("Refresh", "Every … seconds", every));
    }
    if (item.kind === "clock") rows.push(this.row("Format", "As for date: %a weekday · %d day · %b month · %H:%M time", input("format", "%a %d %H:%M")));
    if (item.kind === "weather") {
      const units = el("select", { class: "select" }, ...["C", "F"].map((u) => { const o = el("option", { value: u }, `°${u}`); if (u === item.units) o.selected = true; return o; }));
      units.onchange = () => set("units", units.value);
      // The place is Schedule's (one place for the weather and sunrise/sunset).
      const where = el("span", { class: "sub" }, "…");
      invoke("schedule_get").then((sc) => {
        where.textContent = sc.place
          ? `${sc.place}, from Schedule`
          : `Not set in Schedule yet: using "${item.location || "your internet address"}" until you set one there`;
      }).catch(() => { where.textContent = "Set in Schedule"; });
      rows.push(this.row("Place", "Set in Schedule, which sunrise and sunset use too: change it there", where,
        this.btn("Schedule…", () => settings.show("schedule")), units));
    }
    if (item.kind === "workspaces") {
      const tg = (key) => { const x = el("input", { type: "checkbox", class: "switch" }); x.checked = !!(item[key] ?? (key === "show_apps")); x.onchange = () => set(key, x.checked); return x; };
      rows.push(this.row("App icons", "Icons of the windows on each workspace", tg("show_apps")));
      rows.push(this.row("Hide empty ones", "Except the one you're on", tg("hide_empty")));
      rows.push(this.row("Every display shows all", "Off: each display's bar shows only the workspaces on that display", tg("all_displays")));
      rows.push(this.row("Which ones", "Every workspace AeroSpace has: the ones it keeps (Windows → How many), any with windows, and a new display's", this.btn("Windows…", () => settings.show("windows"))));
    }
    if (item.kind === "disk") {
      const at = el("input", { class: "search dk-num", type: "number", min: 1, max: 100, value: item.disk_at ?? 80 });
      at.onchange = () => set("disk_at", Number(at.value) || 80);
      rows.push(this.row("Show from", "% full", at));
    }
    if (item.kind === "link" || item.kind === "script") {
      rows.push(this.row("Icon and text", "A Nerd Font icon, text, or both", input("icon", "icon", "dk-short"), input("label", "text")));
    }
    if (item.kind === "script") rows.push(this.row("Script", "Runs with bash; it sets the item with sketchybar --set $NAME", input("path", "~/.config/sketchybar/plugins/my.sh")));
    const move = (by) => {
      const p = m.pills[this.sel.p].items;
      const j = this.sel.i + by;
      if (j < 0 || j >= p.length) return;
      [p[this.sel.i], p[j]] = [p[j], p[this.sel.i]];
      this.sel.i = j;
      this.changed("bar");
    };
    const remove = this.btn("Remove", () => {
      m.pills[this.sel.p].items.splice(this.sel.i, 1);
      this.sel = null;
      this.changed("bar");
    }, "btn danger");
    box.replaceChildren(el("div", { class: "dk-item" },
      el("div", { class: "dk-item-h" }, el("b", {}, k.name), el("span", { class: "sub" }, k.what), el("span", { class: "spacer" }),
        item.kind === "palette" ? null : el("span", { class: "sub" }, "Item"),
        item.kind === "palette" ? null : this.btn("◀", () => move(-1)),
        item.kind === "palette" ? null : this.btn("▶", () => move(1)),
        el("span", { class: "sub" }, "Pill"),
        this.btn("◀", () => this.movePill(-1)),
        this.btn("▶", () => this.movePill(1)),
        item.kind === "palette" ? null : remove),
      ...rows));
  },
};

/** The active theme's colours, by key. */
function activeColours() {
  return (themes.find((t) => t.current && !t.invalid) || themes.find((t) => !t.invalid))?.colors || {};
}
