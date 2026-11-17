// Settings → Bar (ROADMAP 18, the owner's pick B from docs/mockups/bar-18a.html): the bar at
// its real size with the empty desktop between the sides folded away, a library tray under it
// (ready pills first, then every item), the chosen item's and pill's settings between them,
// saved bars at the top, and the look settings in tabs. Loaded after desktop.js; adds the bar's
// functions to its `desktop` object. Changes wait for Apply, which also updates the saved bar
// in use (the core does that).
"use strict";

Object.assign(desktop, {
  barTab: "items", // items | bar | pills | text (S1)
  zoom: "100", // fit | 100 | 150
  trayTab: "presets",
  trayQuery: "",
  presets: null, // { presets, looks }
  live: { values: {}, needs: {} }, // S5, S6
  savedBars: [],
  hist: { past: [], future: [], last: null }, // S3
  dragPill: null,
  dragKind: null,
  liveTimer: null,

  /** Called by render() once the bar's settings are loaded. */
  async barStart() {
    if (!this.presets) this.presets = await invoke("bar_presets").catch(() => ({ presets: [], looks: [] }));
    this.savedBars = await invoke("bar_saved").catch(() => []);
    this.iconChoices = null; // apps open now: asked again each time the page opens
    // Undo survives leaving the tab, Apply and redraws (owner, 2026-11-11: a layout couldn't be
    // undone after the page was drawn again): only a bar changed elsewhere starts a new history.
    const now = JSON.stringify(this.model.bar);
    if (this.hist.last !== now) this.hist = { past: [], future: [], last: now };
    this.fetchLive();
    if (!this.liveTimer) {
      this.liveTimer = setInterval(() => {
        if (settings.tab === "bar" && !document.hidden && this.model.bar) this.fetchLive();
      }, 10000);
    }
  },

  /** The running bar's values and what can't work here; only the preview redraws. */
  async fetchLive() {
    try {
      this.live = await invoke("bar_live", { settings: this.model.bar });
      this.paintPreview();
    } catch { /* the samples stay */ }
  },

  kind(id) { return this.kinds.find((k) => k.id === id) || { id, name: id, what: "" }; },

  // ---------------- undo and redo (S3): every change is a step until Apply

  record() {
    const now = JSON.stringify(this.model.bar);
    if (now === this.hist.last) return;
    this.hist.past.push(this.hist.last);
    if (this.hist.past.length > 100) this.hist.past.shift();
    this.hist.last = now;
    this.hist.future = [];
  },

  step(from, to) {
    if (!from.length) return toast(from === this.hist.past ? "Nothing to undo" : "Nothing to redo");
    to.push(this.hist.last);
    this.hist.last = from.pop();
    this.model.bar = JSON.parse(this.hist.last);
    this.sel = null;
    this.draw("bar");
  },
  undo() { this.step(this.hist.past, this.hist.future); },
  redo() { this.step(this.hist.future, this.hist.past); },

  // ---------------- the page

  bar() {
    const t = "bar";
    const m = this.model.bar;
    const tab = (id, label) => el("button", { class: `btn${this.barTab === id ? " on" : ""}`, onclick: () => { this.barTab = id; this.draw(t); } }, label);
    const parts = [
      this.savedRow(),
      this.previewBlock(),
      el("p", { class: "warn dk-fit", hidden: "" }),
      el("div", { class: "dk-tabs" }, el("div", { class: "seg" }, tab("items", "Items"), tab("bar", "Bar"), tab("pills", "Pills"), tab("text", "Text and icons")),
        el("span", { class: "spacer" }),
        el("button", { class: "btn", title: "Undo (⌘Z)", onclick: () => this.undo() }, "↶"),
        el("button", { class: "btn", title: "Redo (⇧⌘Z)", onclick: () => this.redo() }, "↷")),
    ];
    if (this.barTab === "items") {
      const palettePill = m.pills.some((p) => p.items.some((i) => i.kind === "palette"));
      const addSide = (side, label) => this.btn(label, () => { m.pills.push({ side, items: [] }); this.sel = { p: m.pills.length - 1, i: null }; this.changed(t); });
      parts.push(
        el("div", { id: "dk-item" }),
        this.trayBlock(),
        el("div", { class: "actions" }, el("span", { class: "sub" }, "Empty pill:"), addSide("left", "Left"), addSide("center", "Centre"), addSide("right", "Right"),
          el("span", { class: "spacer" }),
          el("label", { class: "dk-inline" }, "Palette's theme pill ", this.paletteToggle(palettePill)),
          palettePill ? el("label", { class: "dk-inline", title: "Scrolling on Palette's pill steps to the next or previous theme. Off: the pill only opens Palette when clicked (also palette fav scroll on|off)" }, "Scroll on it to switch theme ", this.scrollToggle()) : null));
    }
    if (this.barTab === "bar") {
      parts.push(
        this.row("Ready looks", "Changes how the bar is drawn, never its items, font or colours (palette desktop bar look)", ...this.looks()),
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
        this.row("Blur", "Blurs what's behind the bar's background (0: none)", this.num(t, "blur_radius", 0, 100)));
    }
    if (this.barTab === "pills") {
      parts.push(
        el("p", { class: "note" }, "Every pill, unless it has its own colour (choose a pill on the Items tab)."),
        this.row("Colour", "A theme colour, so it changes with the theme", this.colour(t, "pill.colour")),
        this.row("Opacity", "How solid the pills are (100: no wallpaper shows through)", this.slider(t, "pill.opacity", 0, 100, 1, "%")),
        this.row("Height", "Pills keep this height whatever the text size", this.num(t, "pill.height", 6, 80)),
        this.row("Corner radius", "Half the pill height gives fully round ends", this.num(t, "pill.corner_radius", 0, 40)),
        this.row("Border", "Width (0: none) and colour", this.num(t, "pill.border_width", 0, 6), this.colour(t, "pill.border_colour")),
        this.row("Space between pills", "Points between two pills on the same side", this.num(t, "pill.gap", 0, 60)),
        this.row("Space inside the ends", "Points between a pill's edge and its first and last item", this.num(t, "pill.edge", 0, 40)));
    }
    if (this.barTab === "text") {
      parts.push(
        this.row("Font", "A Nerd Font shows the icons. Only the styles the font has are listed", ...this.font()),
        this.row("Sizes", "Icons and text, points", this.num(t, "icon_size", 6, 40, 0.5), this.num(t, "label_size", 6, 40, 0.5)),
        this.row("Icon colour", "A theme colour, so it changes with the theme", this.colour(t, "icon_colour")),
        this.row("Text colour", "A theme colour, so it changes with the theme", this.colour(t, "text_colour")),
        this.row("Stats refresh", "CPU, memory, network and the rest, every … seconds", this.num(t, "stats_every", 1, 600)),
        ...this.appIcons(t));
    }
    parts.push(this.applyBar(t));
    return parts.concat(this.paintSoon(() => this.paintFit()));
  },

  // ---------------- app icons (IDEAS 37b)

  // One row per app open now and per app you chose an icon for; every installed app behind
  // "Show all installed apps". A glyph name from the icon font's map (typed, with suggestions)
  // or up to 4 characters, previewed in that font. An empty box shows, dimmed, the icon the
  // map gives that app (its built-in icon). Saved with Apply, like the rest of the page.
  appIcons(t) {
    const m = this.model.bar;
    // Read only while drawing: the saved bar has no `icons` key when there are none, so adding
    // an empty one here made the page show "Unsaved changes" on every visit.
    const icons = () => m.icons || {};
    const box = el("div", {}, el("div", { class: "empty" }, "Looking for open apps…"));
    const choices = this.iconChoices ||= invoke("bar_icon_choices").catch(() => ({ glyphs: [], open: [], installed: [], map: {} }));
    choices.then(({ glyphs, open, installed = [], map = {} }) => {
      const byName = (a, b) => a.localeCompare(b, undefined, { sensitivity: "base" });
      const list = el("datalist", { id: "dk-glyphs" }, ...glyphs.map((g) => el("option", { value: g })));
      const row = (app) => {
        const builtIn = map[app] || "";
        const preview = el("span", { class: "dk-glyph", title: "As the bar draws it (needs the icon font)" });
        const show = () => {
          const v = icons()[app] || "";
          preview.textContent = v || builtIn;
          preview.classList.toggle("dim", !v);
        };
        show();
        const input = el("input", { class: "search dk-icon", list: "dk-glyphs", placeholder: builtIn || "built-in", value: icons()[app] || "", spellcheck: "false" });
        const sub = el("div", { class: "sub" });
        const label = () => { sub.textContent = icons()[app] ? "Yours" : builtIn && builtIn !== ":default:" ? "Built-in" : "Built-in: the font's catch-all"; };
        label();
        input.addEventListener("change", () => {
          const v = input.value.trim();
          const ok = !v || /^:[A-Za-z0-9_-]+:$/.test(v) || (![...v].some((c) => /[\s$`\\'"]/.test(c)) && [...v].length <= 4);
          if (!ok) { input.value = icons()[app] || ""; return toast("A glyph name like :ollama: or up to 4 characters", true); }
          if (v.startsWith(":") && glyphs.length && !glyphs.includes(v)) toast(`${v} isn't in the icon font's map: the bar shows nothing for it`, true);
          if (v) (m.icons ||= {})[app] = v;
          else if (m.icons) { delete m.icons[app]; if (!Object.keys(m.icons).length) delete m.icons; }
          show(); label();
          this.changed(t);
        });
        return el("div", { class: "row", "data-app": app.toLowerCase() },
          el("div", {}, el("div", { class: "title" }, app), sub), preview, input);
      };
      const shown = [...new Set([...Object.keys(icons()), ...open])].sort(byName);
      const rest = installed.filter((a) => !shown.includes(a)).sort(byName);
      const more = el("div", { hidden: "" });
      let toggle = null;
      if (rest.length) {
        const search = el("input", { class: "search dk-app-search", type: "search", placeholder: `Search ${rest.length} installed apps`, spellcheck: "false" });
        const rows = el("div", {});
        let built = false;
        search.addEventListener("input", () => {
          const q = search.value.trim().toLowerCase();
          for (const r of rows.children) r.hidden = !!q && !r.dataset.app.includes(q);
        });
        toggle = el("button", { class: "link dk-more-apps" }, `Show all installed apps (${rest.length})`);
        toggle.onclick = () => {
          // Built on first open: a hundred rows aren't drawn for nothing.
          if (!built) { rows.replaceChildren(...rest.map(row)); built = true; }
          more.hidden = !more.hidden;
          toggle.textContent = more.hidden ? `Show all installed apps (${rest.length})` : "Hide installed apps";
          if (!more.hidden) search.focus();
        };
        more.append(search, rows);
      }
      box.replaceChildren(list,
        el("p", { class: "note" }, glyphs.length
          ? `Each app gets a built-in icon from the icon font's map (icon_map.sh, shown dimmed). Type your own to replace it (${glyphs.length} glyphs: type to search, e.g. :ollama:). Empty = built-in.`
          : "The icon font's map isn't installed (~/.config/sketchybar/helpers/icon_map.sh), so the bar shows • for apps. You can still type up to 4 characters per app."),
        ...(shown.length ? shown.map(row) : [el("div", { class: "empty" }, "No open apps found (AeroSpace lists them)")]),
        ...(toggle ? [toggle, more] : []));
    });
    return [el("h3", {}, "App icons"), box];
  },

  // ---------------- saved bars (18c)

  savedRow() {
    const list = this.savedBars;
    const using = list.find((b) => b.in_use)?.name;
    const pick = el("select", { class: "select", "aria-label": "Saved bar" },
      ...(using ? [] : [el("option", { value: "" }, list.length ? "Not a saved bar" : "Not saved yet")]),
      ...list.map((b) => el("option", { value: b.name, selected: b.in_use }, b.name)));
    pick.onchange = async () => {
      if (this.dirty("bar")) { pick.value = using || ""; return toast("Apply or revert your changes first", true); }
      if (!pick.value) return;
      try {
        const r = await invoke("bar_use", { name: pick.value });
        this.report(r, `Now using the bar “${pick.value}”`);
      } catch (e) { toast(String(e), true); }
      this.render("bar");
    };
    const row = el("div", { class: "row dk-saved", id: "dk-saved" });
    const ask = (label, value, go) => {
      const input = el("input", { class: "search dk-text", type: "text", value, placeholder: "A name, e.g. Work", spellcheck: "false", maxlength: 40 });
      const ok = this.btn(label, async () => { try { await go(input.value); } catch (e) { toast(String(e), true); } }, "btn primary");
      input.addEventListener("keydown", (e) => { if (e.key === "Enter") ok.click(); if (e.key === "Escape") { e.stopPropagation(); this.paintSaved(); } });
      row.replaceChildren(el("div", {}, el("div", { class: "title" }, label === "Save" ? "Save this bar as" : `Rename “${using}”`)), input, ok, this.btn("Cancel", () => this.paintSaved()));
      setTimeout(() => input.focus());
    };
    const saveAs = this.btn("Save as…", () => ask("Save", "", async (name) => {
      // Unapplied changes go into the new bar only, never into the one in use before (core).
      const edited = this.dirty("bar") ? this.model.bar : null;
      const saved = await invoke("bar_save_as", { name, settings: edited });
      if (edited) { this.saved.bar = JSON.stringify(this.model.bar); this.changed("bar"); }
      toast(`Saved as “${saved}”. Apply now saves into it`);
      this.savedBars = await invoke("bar_saved");
      this.paintSaved();
    }));
    const rename = using ? this.btn("Rename…", () => ask("Rename", using, async (name) => {
      await invoke("bar_rename_saved", { name: using, new: name });
      this.savedBars = await invoke("bar_saved");
      this.paintSaved();
    })) : null;
    const del = using ? this.btn("Delete", async () => {
      if (!armed(del, "Click again: delete")) return;
      try {
        await invoke("bar_delete_saved", { name: using });
        toast(`Deleted “${using}”. The bar on screen stays`);
        this.savedBars = await invoke("bar_saved");
        this.paintSaved();
      } catch (e) { toast(String(e), true); }
    }, "btn danger") : null;
    row.append(el("div", {}, el("div", { class: "title" }, "Bar"),
      el("div", { class: "sub" }, using
        ? `Apply saves into “${using}”. Switch here, from the menu bar icon (Bars) or by Schedule`
        : "Save as… keeps this bar under a name, so you can switch between bars")),
    pick, saveAs, rename, del);
    return row;
  },

  paintSaved() { $("#dk-saved")?.replaceWith(this.savedRow()); },

  // ---------------- the preview

  previewBlock() {
    const z = (id, label, title) => el("button", { class: `btn${this.zoom === id ? " on" : ""}`, title, onclick: () => { this.zoom = id; this.draw("bar"); } }, label);
    return el("div", { class: "dk-preview" },
      el("div", { class: "dk-preview-h" },
        el("span", { class: "sub" }, "Drag items, whole pills (by ⋮⋮) or tiles from below. Click to choose; ← → move, ⌫ removes"),
        el("span", { class: "spacer" }),
        el("div", { class: "seg dk-zoom" },
          z("fit", "Fit", "The whole bar shrunk to fit, with its real gaps"),
          z("100", "100%", "Real size, the empty desktop between the sides folded"),
          z("150", "150%", "Larger, for fine dragging"))),
      el("div", { class: "dk-bar-wrap" }, el("div", { class: "dk-bar", id: "dk-bar-preview" })));
  },

  /** The whole bar drawn: the preview, the chosen item's settings and the tray's tiles. */
  paintBar() {
    this.paintPreview();
    this.paintItem();
    this.paintTiles();
  },

  paintPreview() {
    const box = $("#dk-bar-preview");
    const m = this.model.bar;
    if (!box || !m) return;
    const c = activeColours();
    const fit = this.zoom === "fit";
    box.classList.toggle("folded", !fit);
    box.style.background = m.background ? c[m.background] : "transparent";
    box.style.borderRadius = `${m.corner_radius}px`;
    box.style.minHeight = `${m.height}px`;
    box.style.gap = `${m.pill.gap}px`;
    box.style.padding = `0 ${m.padding}px`;
    const side = (s) => {
      const d = el("div", { class: `dk-side ${s}`, title: s === "center" ? "Centre: drop here (behind the notch on a MacBook screen)" : "" },
        ...m.pills.map((p, pi) => [p, pi]).filter(([p]) => p.side === s).map(([p, pi]) => this.pill(p, pi, c)));
      d.style.gap = `${m.pill.gap}px`;
      this.dropZone(d, { side: s });
      return d;
    };
    const gap = (label) => fit ? el("span", { class: "spacer" }) : el("span", { class: "dk-fold", "aria-hidden": "true" }, label);
    box.replaceChildren(side("left"), gap("⋯"), side("center"), gap("⋯"), side("right"));
    const wrap = box.parentElement;
    box.style.transform = "";
    box.style.zoom = this.zoom === "150" ? "1.5" : "";
    if (fit) {
      // A screen-wide bar in a narrower pane: shrink it to fit, like a miniature.
      const scale = Math.min(1, (wrap.clientWidth - 20) / box.scrollWidth);
      box.style.transform = `scale(${scale.toFixed(3)})`;
      wrap.style.height = `${Math.ceil(box.offsetHeight * scale) + 20}px`;
    } else {
      wrap.style.height = "";
    }
  },

  pill(p, pi, c) {
    const m = this.model.bar;
    const fixed = p.items.some((i) => i.kind === "palette");
    const chosen = this.sel && this.sel.p === pi;
    const colour = p.colour === "none" ? "transparent"
      : `color-mix(in srgb, ${c[p.colour || m.pill.colour]} ${p.opacity ?? m.pill.opacity}%, transparent)`;
    const h = m.pill.height;
    const pill = el("div", {
      class: `dk-pill${fixed ? " fixed" : ""}${chosen && this.sel.i == null ? " sel" : ""}`,
      title: fixed ? "Palette's theme pill: drag it by ⋮⋮, or click to choose it" : "Drag by ⋮⋮ to move the whole pill; click to choose it",
      style: {
        background: colour, height: `${h}px`, padding: `0 ${Math.max(4, m.pill.edge - 4)}px`,
        borderRadius: `${Math.min(m.pill.corner_radius, h / 2)}px`,
        border: m.pill.border_width ? `${m.pill.border_width}px solid ${c[m.pill.border_colour]}` : "none",
      },
      onclick: (e) => { e.stopPropagation(); this.sel = { p: pi, i: null }; this.paintBar(); },
    });
    // The grip moves the whole pill (pointer drag: WebKit's own drag of a pill never worked here).
    const grip = el("span", { class: "dk-grip", title: "Drag to move the whole pill", "aria-hidden": "true" }, "⋮⋮");
    this.pointerDrag(grip, () => { this.dragPill = pi; }, fixed ? "Palette's pill" : "Pill");
    pill.append(grip);
    this.dropZone(pill, { p: pi });
    for (const [i, item] of p.items.entries()) pill.append(this.chip(item, pi, i, c, fixed));
    if (!p.items.length) pill.append(el("span", { class: "sub dk-empty" }, "empty: drop items here"));
    return pill;
  },

  chip(item, pi, i, c, fixed) {
    const m = this.model.bar;
    const k = this.kind(item.kind);
    const sel = this.sel && this.sel.p === pi && this.sel.i === i;
    // Short, like the real bar: an icon (the dot) and the real value when the bar runs (S5).
    const SAMPLE = { cpu: "12%", gpu: "4%", temp: "41°", gputemp: "39°", pwr: "6W", mem: "58%", disk: "81%", vpn: "VPN", net: "2.1M",
      battery: "87%", volume: "40%", brightness: "70%", weather: "31°", habits: "3/5", dispatch: "3 read", front_app: "Ghostty",
      nowplaying: "Song title – Artist" };
    let face = this.live.values?.[item.kind] || SAMPLE[item.kind] || k.name;
    if (item.kind === "workspaces") face = Array.from({ length: this.workspaceCount() }, (_, n) => n + 1).join(" ");
    if (item.kind === "link" || item.kind === "script") face = item.label || item.icon || k.name;
    if (item.kind === "palette") face = themes.find((x) => x.current)?.name || "Palette";
    if (item.kind === "clock" && !this.live.values?.clock) face = new Date().toLocaleString("en-GB", { weekday: "short", day: "2-digit", hour: "2-digit", minute: "2-digit", hour12: false }).replace(",", "");
    const need = this.live.needs?.[item.kind];
    const notes = [k.what, need, item.hide_in_performance ? "Hidden in Performance mode" : ""].filter(Boolean);
    if (item.kind === "nowplaying") {
      // Cut to its width as the bar does (it scrolls there when Long titles says so).
      const w = item.width ?? 24;
      if ([...face].length > w) face = `${[...face].slice(0, w - 1).join("")}…`;
    }
    const chip = el("span", {
      class: `dk-it${sel ? " sel" : ""}${need ? " needs" : ""}${item.hide_in_performance ? " perf" : ""}`,
      draggable: fixed ? "false" : "true", title: `${k.name}: ${notes.join(" · ")}`,
      style: { color: c[m.text_colour], fontSize: `${m.label_size}px` },
      onclick: (e) => { e.stopPropagation(); this.sel = { p: pi, i }; this.paintBar(); },
    }, el("b", { style: { background: c[m.icon_colour], width: `${m.icon_size * 0.55}px`, height: `${m.icon_size * 0.55}px` } }), face);
    if (item.kind === "nowplaying" && item.beats !== false) {
      // The beat lines, drawn still (the bar moves them with the music).
      const [lo, hi] = [2, Math.max(6, m.pill.height - 12)];
      chip.prepend(el("span", { class: "dk-beats" }, ...[0.5, 0.9, 0.6, 1, 0.4].map((v) =>
        el("i", { style: { height: `${lo + v * (hi - lo)}px`, background: c[m.icon_colour] } }))));
    }
    if (!fixed) {
      chip.addEventListener("dragstart", (e) => { this.drag = { p: pi, i }; e.dataTransfer.effectAllowed = "move"; e.dataTransfer.setData("text/plain", "item"); });
      chip.addEventListener("dragend", () => { this.drag = null; });
    }
    this.dropZone(chip, { p: pi, i });
    return chip;
  },

  /** Drag by pointer, for pills (their grip) and the tray's tiles: WebKit won't start an HTML drag
   *  from a button, and a pill's own drag never worked on the owner's Mac (2026-10-25, 2026-11-11).
   *  A label follows the pointer; on release, the drop zone under it decides (`data-drop`). */
  pointerDrag(handle, start, label) {
    handle.addEventListener("pointerdown", (e) => {
      if (e.button !== 0) return;
      const [x0, y0] = [e.clientX, e.clientY];
      let ghost = null, over = null;
      const move = (ev) => {
        if (!ghost) {
          if (Math.hypot(ev.clientX - x0, ev.clientY - y0) < 5) return;
          start();
          ghost = el("div", { class: "dk-ghost" }, label);
          document.body.append(ghost);
        }
        ghost.style.left = `${ev.clientX + 12}px`;
        ghost.style.top = `${ev.clientY + 12}px`;
        const t = document.elementFromPoint(ev.clientX, ev.clientY)?.closest("[data-drop]") || null;
        if (t !== over) { over?.classList.remove("over"); over = t; over?.classList.add("over"); }
      };
      const up = () => {
        window.removeEventListener("pointermove", move);
        window.removeEventListener("pointerup", up);
        if (!ghost) return;
        ghost.remove();
        over?.classList.remove("over");
        this.dragged = true; // the click that follows isn't a click
        setTimeout(() => { this.dragged = false; });
        if (over) this.dropAt(JSON.parse(over.dataset.drop));
        else this.dragPill = this.dragKind = null;
      };
      window.addEventListener("pointermove", move);
      window.addEventListener("pointerup", up);
    });
    handle.addEventListener("click", (e) => { if (this.dragged) { e.stopImmediatePropagation(); e.preventDefault(); } }, true);
  },

  /** Somewhere a dragged item, pill or tile can land: a side, a pill, or before an item. */
  dropZone(node, at) {
    node.dataset.drop = JSON.stringify(at);
    node.addEventListener("dragover", (e) => { e.preventDefault(); e.stopPropagation(); node.classList.add("over"); });
    node.addEventListener("dragleave", () => node.classList.remove("over"));
    node.addEventListener("drop", (e) => { e.preventDefault(); e.stopPropagation(); node.classList.remove("over"); this.dropAt(at); });
  },

  dropAt(at) {
    const m = this.model.bar;
    const [d, pill, kind] = [this.drag, this.dragPill, this.dragKind];
    this.drag = this.dragPill = this.dragKind = null;
    // A whole pill (S2): before the pill it's dropped on, or at the end of a side.
    if (pill != null) {
      if (at.side) return this.placePill(pill, { to: at.side });
      if (at.p !== pill) this.placePill(pill, { before: at.p });
      return;
    }
    const target = at.p != null ? m.pills[at.p] : null;
    if (target?.items.some((i) => i.kind === "palette")) return toast("Palette's theme pill holds only the theme", true);
    if (kind) {
      if (at.side) return this.newPillWith(kind, at.side);
      return this.addItem(kind, at.p, at.i ?? target.items.length);
    }
    if (!d) return;
    if (at.side) {
      // An item dropped beside the pills: a new pill of its own on that side.
      const [item] = m.pills[d.p].items.splice(d.i, 1);
      m.pills.push({ side: at.side, items: [item] });
      this.sel = { p: m.pills.length - 1, i: 0 };
      return this.placeNew(at.side);
    }
    let i = at.i ?? target.items.length;
    const [item] = m.pills[d.p].items.splice(d.i, 1);
    if (d.p === at.p && d.i < i) i -= 1;
    m.pills[at.p].items.splice(i, 0, item);
    this.sel = { p: at.p, i };
    this.changed("bar");
  },

  /** The pill just pushed at the end of `pills` moved to the end of its side (the core's order). */
  placeNew(side) {
    return this.placePill(this.model.bar.pills.length - 1, { to: side });
  },

  newItem(kind) {
    const item = { kind };
    if (kind === "link") Object.assign(item, { label: "Link", click: "open https://example.com" });
    if (kind === "script") Object.assign(item, { icon: "", label: "--", path: "~/.config/sketchybar/plugins/my.sh" });
    return item;
  },

  canAdd(kind) {
    const k = this.kind(kind);
    if (!k.single) return true;
    return !this.model.bar.pills.some((p) => p.items.some((i) => i.kind === kind));
  },

  addItem(kind, p, i) {
    if (!this.canAdd(kind)) return toast(`${this.kind(kind).name} is on the bar already`, true);
    this.model.bar.pills[p].items.splice(i, 0, this.newItem(kind));
    this.sel = { p, i };
    this.changed("bar");
  },

  newPillWith(kind, side) {
    if (!this.canAdd(kind)) return toast(`${this.kind(kind).name} is on the bar already`, true);
    const m = this.model.bar;
    m.pills.push({ side, items: [this.newItem(kind)] });
    this.sel = { p: m.pills.length - 1, i: 0 };
    this.placeNew(side);
  },

  /** Where a tile click goes: the chosen pill, else a new pill on the right. */
  target() {
    const p = this.sel && this.model.bar.pills[this.sel.p];
    return p && !p.items.some((i) => i.kind === "palette") ? this.sel.p : null;
  },

  // ---------------- the tray (18b): ready pills, then every item

  trayBlock() {
    const tab = (id, label) => el("button", { class: `btn${this.trayTab === id && !this.trayQuery ? " on" : ""}`, onclick: () => { this.trayTab = id; this.trayQuery = ""; this.draw("bar"); } }, label);
    const q = el("input", { class: "search dk-tray-q", type: "search", placeholder: "Search items", value: this.trayQuery, spellcheck: "false" });
    q.addEventListener("input", () => { this.trayQuery = q.value.trim().toLowerCase(); this.paintTiles(); });
    return el("div", { class: "dk-tray" },
      el("div", { class: "dk-tray-h" },
        el("div", { class: "seg" }, tab("presets", "Ready pills"), tab("desktop", "Desktop"), tab("system", "Mac"), tab("stats", "Stats"), tab("yours", "Yours")),
        el("span", { class: "spacer" }), q),
      el("div", { id: "dk-tiles" }),
      el("p", { class: "sub", id: "dk-tray-hint" }));
  },

  paintTiles() {
    const box = $("#dk-tiles");
    if (!box) return;
    const m = this.model.bar;
    const c = activeColours();
    const to = this.target();
    const hint = $("#dk-tray-hint");
    if (hint) {
      hint.textContent = to != null
        ? `Click to add to the chosen pill (${m.pills[to].side === "center" ? "centre" : m.pills[to].side}), or drag onto the bar. A ready pill always makes a new pill on that side.`
        : "Choose a pill to add to it, or drag a tile onto the bar. Without one, a click makes a new pill on the right.";
    }
    if (this.trayTab === "presets" && !this.trayQuery) return box.replaceChildren(...this.presetCards(c));
    const q = this.trayQuery;
    const kinds = this.kinds.filter((k) => k.id !== "palette"
      && (q ? `${k.name} ${k.what}`.toLowerCase().includes(q) : k.group === this.trayTab));
    if (!kinds.length) return box.replaceChildren(el("p", { class: "note" }, "Nothing matches"));
    box.replaceChildren(el("div", { class: "dk-tiles" }, ...kinds.map((k) => {
      const on = !this.canAdd(k.id);
      const need = this.live.needs?.[k.id];
      const tile = el("button", {
        class: `dk-tile${on ? " used" : ""}`, disabled: on,
        title: on ? `${k.name} is on the bar already` : `Add ${k.name}`,
        onclick: () => {
          const p = this.target();
          if (p != null) this.addItem(k.id, p, m.pills[p].items.length);
          else this.newPillWith(k.id, "right");
        },
      },
      el("span", { class: "dk-tile-n" }, el("b", { style: { background: c[m.icon_colour] } }), k.name),
      el("span", { class: "dk-tile-w" }, k.what),
      on ? el("span", { class: "badge" }, "on the bar") : null,
      k.network ? el("span", { class: "badge" }, "network") : null,
      need ? el("span", { class: "badge warn" }, need) : null);
      this.pointerDrag(tile, () => { this.dragKind = k.id; }, k.name);
      return tile;
    })));
  },

  presetCards(c) {
    const m = this.model.bar;
    const side = () => (this.sel && m.pills[this.sel.p]?.side) || "right";
    const card = (p) => {
      const go = el("button", { class: `dk-preset${p.sort === "layout" ? " layout" : ""}` },
        el("span", { class: "dk-preset-t" }, p.name, el("span", { class: "sub" }, p.sort === "layout" ? "whole bar" : `${p.kinds.length} items`)),
        p.sort === "pill"
          ? el("span", { class: "dk-mini" }, ...p.kinds.map((k) => el("i", { class: this.canAdd(k) ? "" : "used", title: this.kind(k).name }, this.kind(k).name)))
          : el("span", { class: "dk-tile-w" }, p.what));
      go.title = p.what;
      go.onclick = async () => {
        if (p.sort === "layout" && !armed(go, "Click again: replaces every pill")) return;
        try {
          const [bar, added] = await invoke("bar_add_preset", { settings: m, id: p.id, side: side() });
          this.model.bar.pills = bar.pills;
          this.sel = added.pill != null ? { p: added.pill, i: null } : null;
          if (added.skipped.length) toast(`Left out: ${added.skipped.join(", ")}`);
          this.changed("bar");
        } catch (e) { toast(String(e), true); }
      };
      return go;
    };
    const ps = this.presets?.presets || [];
    return [
      el("div", { class: "dk-presets" }, ...ps.filter((p) => p.sort === "pill").map(card)),
      el("div", { class: "dk-presets" }, ...ps.filter((p) => p.sort === "layout").map(card)),
    ];
  },

  /** Ready looks (S8): buttons on the Bar tab. */
  looks() {
    return (this.presets?.looks || []).map((l) => {
      const b = this.btn(l.name, async () => {
        try {
          this.model.bar = await invoke("bar_look", { settings: this.model.bar, id: l.id });
          this.changed("bar");
          this.draw("bar");
        } catch (e) { toast(String(e), true); }
      });
      b.title = l.what;
      return b;
    });
  },

  // ---------------- moving pills

  /** The chosen pill one place left or right; at the end of its side it moves to the next
   *  side (left, centre, right). The core does the moving. */
  movePill(by) {
    this.placePill(this.sel.p, { to: by > 0 ? "next" : "prev" });
  },

  /** Move pill `from` with the core's `Bar::move_pill` (same as `palette desktop bar move`). */
  async placePill(from, where) {
    const keep = this.sel?.p === from ? this.sel.i : null;
    const chosen = this.sel?.p === from;
    try {
      const [bar, at] = await invoke("bar_move_pill", { settings: this.model.bar, from, to: where.to ?? null, before: where.before ?? null });
      // Only the pills change; keep the same model object, which the page's controls hold.
      this.model.bar.pills = bar.pills;
      this.sel = chosen ? { p: at, i: keep } : this.sel;
      this.changed("bar");
    } catch (e) { toast(String(e), true); }
  },

  /** Split, merge or copy a pill (S7), in the core. */
  async pillAction(action, at) {
    try {
      const [bar, p] = await invoke("bar_pill_action", { settings: this.model.bar, action, pill: this.sel.p, at: at ?? null });
      this.model.bar.pills = bar.pills;
      this.sel = { p, i: null };
      this.changed("bar");
    } catch (e) { toast(String(e), true); }
  },

  moveItem(by) {
    const items = this.model.bar.pills[this.sel.p].items;
    const j = this.sel.i + by;
    if (j < 0 || j >= items.length) return;
    [items[this.sel.i], items[j]] = [items[j], items[this.sel.i]];
    this.sel.i = j;
    this.changed("bar");
  },

  removeItem() {
    const m = this.model.bar;
    const items = m.pills[this.sel.p].items;
    if (items[this.sel.i]?.kind === "palette") return toast("Turn Palette's theme pill off below instead", true);
    items.splice(this.sel.i, 1);
    this.sel = items.length ? { p: this.sel.p, i: Math.min(this.sel.i, items.length - 1) } : { p: this.sel.p, i: null };
    this.changed("bar");
  },

  workspaceCount() {
    return this.workspaces || 6;
  },

  // ---------------- the chosen item and pill

  paintItem() {
    const box = $("#dk-item");
    if (!box) return;
    const m = this.model.bar;
    const pill = this.sel && m.pills[this.sel.p];
    if (!pill) return box.replaceChildren(el("p", { class: "note" }, "Click an item to change what it shows, how often it refreshes and what a click opens; click a pill's edge for its colour, or to split, merge or copy it."));
    const item = this.sel.i != null ? pill.items[this.sel.i] : null;
    box.replaceChildren(...[item ? this.itemBox(item) : null, this.pillBox(pill)].filter(Boolean));
  },

  itemBox(item) {
    const k = this.kind(item.kind);
    const set = (key, v) => { item[key] = v; this.changed("bar"); };
    const input = (key, placeholder, width = "") => {
      const x = el("input", { class: `search dk-text ${width}`, type: "text", value: item[key] ?? "", placeholder, spellcheck: "false" });
      x.addEventListener("change", () => set(key, x.value));
      return x;
    };
    const tg = (key, dflt = false) => { const x = el("input", { type: "checkbox", class: "switch" }); x.checked = !!(item[key] ?? dflt); x.onchange = () => set(key, x.checked); return x; };
    const rows = [];
    const need = this.live.needs?.[item.kind];
    if (need) rows.push(el("p", { class: "warn" }, `${k.name}: ${need}.`));
    if (item.kind === "nowplaying") {
      // Click: play/pause or open the player (owner), nothing, or your own command.
      const mode = item.click === "none" ? "none" : item.click ? "custom" : (item.action ?? "toggle");
      const how = el("select", { class: "select" },
        ...[["toggle", "Play / pause"], ["open", "Open the player"], ["none", "Nothing"], ["custom", "Open or run…"]].map(([v, l]) => el("option", { value: v, selected: v === mode }, l)));
      const cmd = input("click", "open -a Music");
      cmd.hidden = mode !== "custom";
      how.onchange = () => {
        cmd.hidden = how.value !== "custom";
        if (how.value === "toggle" || how.value === "open") { item.action = how.value; item.click = ""; }
        else item.click = how.value === "none" ? "none" : cmd.value || "open -a Music";
        cmd.value = item.click === "none" ? "" : item.click;
        this.changed("bar");
      };
      rows.push(this.row("Click", "Play / pause works for Music, Spotify, a browser…: whatever shows in Control Centre", how, cmd));
      rows.push(this.row("Beat lines", "Move with what the Mac plays. macOS asks once to let Palette hear it (System Audio Recording); never the microphone, nothing is recorded. Off in Performance mode's “Slower bar”", tg("beats", true)));
      const width = el("input", { class: "search dk-num", type: "number", min: 6, max: 80, value: item.width ?? 24 });
      width.onchange = () => { const v = Math.round(Number(width.value)); if (v >= 6 && v <= 80) set("width", v); else { width.value = item.width ?? 24; toast("From 6 to 80", true); } };
      const long = el("select", { class: "select" },
        ...[["scroll", "Scroll"], ["cut", "Cut short with …"]].map(([v, l]) => el("option", { value: v, selected: (item.scroll ?? true) === (v === "scroll") }, l)));
      long.onchange = () => set("scroll", long.value === "scroll");
      rows.push(this.row("Title", "Width in characters; a longer title and artist scroll slowly, or are cut short", width, long));
    }
    if (item.kind !== "palette" && item.kind !== "workspaces" && item.kind !== "nowplaying") {
      const mode = item.click === "none" ? "none" : item.click ? "custom" : "default";
      const how = el("select", { class: "select" },
        ...[["default", k.click ? "The usual" : "Nothing"], ["none", "Nothing"], ["custom", "Open or run…"]].map(([v, l]) => el("option", { value: v, selected: v === mode }, l)));
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
      const units = el("select", { class: "select" }, ...["C", "F"].map((u) => el("option", { value: u, selected: u === item.units }, `°${u}`)));
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
      rows.push(this.row("App icons", "Icons of the windows on each workspace", tg("show_apps", true)));
      rows.push(this.row("Hide empty ones", "Except the one you're on", tg("hide_empty")));
      rows.push(this.row("Every display shows all", "Off: each display's bar shows only the workspaces on that display", tg("all_displays")));
      rows.push(this.row("Which ones", "Every workspace AeroSpace has: the ones it keeps (Windows → How many), any with windows, and a new display's", this.btn("Windows…", () => settings.show("windows"))));
    }
    if (item.kind === "disk") {
      const at = el("input", { class: "search dk-num", type: "number", min: 1, max: 100, value: item.disk_at ?? 80 });
      at.onchange = () => set("disk_at", Number(at.value) || 80);
      rows.push(this.row("Show from", "% full", at));
    }
    if (item.kind === "gpu") {
      // 16b: macmon's busy share is steady; the driver's figure jumps between 0 and ~50 at rest.
      const pick = el("select", { class: "select" },
        ...[["busy", "Busy (steady)"], ["raw", "Raw (the driver's figure)"]].map(([v, label]) =>
          el("option", { value: v, selected: (item.gpu || "busy") === v }, label)));
      pick.onchange = () => set("gpu", pick.value);
      rows.push(this.row("Reading", "Busy: how much of the time the GPU works (needs macmon; without it, raw). Raw: the GPU driver's own figure, which jumps between 0 and about 50 when idle", pick));
    }
    if (item.kind === "link" || item.kind === "script") {
      rows.push(this.row("Icon and text", "A Nerd Font icon, text, or both", input("icon", "icon", "dk-short"), input("label", "text")));
    }
    if (item.kind === "script") rows.push(this.row("Script", "Runs with bash; it sets the item with sketchybar --set $NAME", input("path", "~/.config/sketchybar/plugins/my.sh")));
    if (item.kind !== "palette") {
      rows.push(this.row("Hide in Performance mode", "Gone while Performance mode's “Slower bar” is on, back when it's off (palette desktop bar hide)", tg("hide_in_performance")));
    }
    const head = el("div", { class: "dk-item-h" }, el("b", {}, k.name), el("span", { class: "sub" }, k.what), el("span", { class: "spacer" }));
    if (item.kind !== "palette") {
      head.append(el("span", { class: "sub" }, "Move"), this.btn("◀", () => this.moveItem(-1)), this.btn("▶", () => this.moveItem(1)),
        this.btn("Remove", () => this.removeItem(), "btn danger"));
    }
    return el("div", { class: "dk-item" }, head, ...rows);
  },

  pillBox(pill) {
    const m = this.model.bar;
    const fixed = pill.items.some((i) => i.kind === "palette");
    const order = m.pills.map((p, i) => [p, i]).filter(([p]) => p.side === pill.side).map(([, i]) => i);
    const n = order.indexOf(this.sel.p) + 1;
    const where = { left: "left", center: "centre", right: "right" }[pill.side];
    const c = activeColours();
    // Its own colour (S9): like the others, none, or a theme colour.
    const swatch = el("i", { class: "dk-swatch" });
    const colour = el("select", { class: "select", "aria-label": "Pill colour" },
      el("option", { value: "", selected: !pill.colour }, "Like the other pills"),
      el("option", { value: "none", selected: pill.colour === "none" }, "No background"),
      ...COLOUR_KEYS.map((k) => el("option", { value: k, selected: pill.colour === k }, LABEL[k] || k)));
    const paint = () => { swatch.style.background = pill.colour && pill.colour !== "none" ? c[pill.colour] : "transparent"; };
    colour.onchange = () => { pill.colour = colour.value; if (!pill.colour) delete pill.colour; paint(); this.changed("bar"); };
    paint();
    const own = el("input", { type: "checkbox", class: "switch", "aria-label": "Its own opacity" });
    own.checked = pill.opacity != null;
    const op = el("input", { type: "range", min: 0, max: 100, step: 1, value: pill.opacity ?? m.pill.opacity, disabled: pill.opacity == null });
    const out = el("span", { class: "dk-val" }, `${pill.opacity ?? m.pill.opacity}%`);
    own.onchange = () => { if (own.checked) pill.opacity = Number(op.value); else delete pill.opacity; this.changed("bar"); };
    op.addEventListener("input", () => { out.textContent = `${op.value}%`; pill.opacity = Number(op.value); this.changed("bar"); });
    const actions = [];
    if (!fixed) {
      if (this.sel.i > 0) actions.push(this.btn("Split before this item", () => this.pillAction("split", this.sel.i)));
      actions.push(this.btn("Merge with the next", () => this.pillAction("merge")));
      actions.push(this.btn("Copy", () => this.pillAction("duplicate")));
    }
    const remove = this.btn(fixed ? "Remove" : "Remove pill", () => {
      if (pill.items.length > 1 && !armed(remove, `Click again: remove ${pill.items.length} items`)) return;
      m.pills.splice(this.sel.p, 1);
      this.sel = null;
      this.changed("bar");
    }, "btn danger");
    return el("div", { class: "dk-item dk-pill-box" },
      el("div", { class: "dk-item-h" }, el("b", {}, fixed ? "Palette's theme pill" : `Pill ${n} on the ${where}`),
        el("span", { class: "sub" }, `${pill.items.length} item${pill.items.length === 1 ? "" : "s"}`), el("span", { class: "spacer" }),
        el("span", { class: "sub" }, "Move"), this.btn("◀", () => this.movePill(-1)), this.btn("▶", () => this.movePill(1)), remove),
      this.row("Colour", "Its own background, or like the others (Pills tab)", el("span", { class: "dk-colour" }, swatch, colour)),
      this.row("Its own opacity", "Off: the Pills tab's", own, el("span", { class: "dk-slider" }, op, out)),
      actions.length ? el("div", { class: "actions" }, ...actions) : null);
  },

  // ---------------- keyboard (S4), called first by settings-keys.js

  barKey(e) {
    if (settings.tab !== "bar" || !this.model.bar || $("#keys-sheet")) return false;
    const cmd = e.metaKey && !e.ctrlKey && !e.altKey;
    if (cmd && e.key.toLowerCase() === "z") {
      if (keys.busy(e)) return false; // a text box keeps its own undo
      e.preventDefault();
      if (e.shiftKey) this.redo(); else this.undo();
      return true;
    }
    if (e.metaKey || e.ctrlKey || keys.busy(e) || !this.sel || !this.model.bar.pills[this.sel.p]) return false;
    const item = this.sel.i != null;
    if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
      const by = e.key === "ArrowRight" ? 1 : -1;
      if (e.altKey || !item) this.movePill(by); else this.moveItem(by);
      e.preventDefault();
      return true;
    }
    if ((e.key === "Backspace" || e.key === "Delete") && item) {
      e.preventDefault();
      this.removeItem();
      return true;
    }
    if (e.key === "Escape" && (document.activeElement === document.body || !document.activeElement)) {
      e.preventDefault();
      this.sel = null;
      this.paintBar();
      return true;
    }
    return false;
  },

  // ---------------- look helpers (unchanged from before 18)

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
      msgs.push(`Pills (${m.pill.height} pt) are taller than the bar (${m.height} pt), so their tops and bottoms are cut off: raise Bar → Height.`);
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
      style.replaceChildren(...opts.map((v) => el("option", { value: v, selected: v === m.font_style }, v)));
      if (!have.length) style.title = `${m.font} isn't installed`;
      this.changed("bar");
    };
    invoke("font_families").then((all) => {
      const nerd = all.filter((f) => /nerd/i.test(f)), rest = all.filter((f) => !/nerd/i.test(f));
      const group = (label, fs) => el("optgroup", { label }, ...fs.map((f) => el("option", { value: f, selected: f === m.font }, f)));
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
      this.sel = null;
      this.changed("bar");
      this.draw("bar");
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
});

// A saved bar chosen from the menu bar icon (Bars) or by Schedule: show it.
listen("bar-changed", () => { if (settings.tab === "bar") desktop.render("bar"); });
