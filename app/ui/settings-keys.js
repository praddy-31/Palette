// Settings from the keyboard (ROADMAP 14d-5, the owner's keys): numbers for the first ten tabs,
// letters for the desktop pages and help, arrows through the tab list and list rows, ⌘[ ⌘] for
// the previous/next tab, / or ⌘F to search, ? for every shortcut, ⌘W to close. A strip at the
// bottom names them (like Dispatch's). Plain keys never fire while you type in a box, move a
// slider or a menu, or record a shortcut.
"use strict";

const keys = {
  /** 1-9, 0: the first ten tabs, in the order they're listed. */
  numbers: ["apps", "mine", "themes", "wallpapers", "colours", "tools", "setup", "move", "general", "schedule"],
  /** Letters (owner): Windows, Bar, borders (J), Prompt, Dock & menu bar, Help. */
  letters: { w: "windows", b: "bar", j: "borders", p: "prompt", d: "macos", h: "guide" },

  /** The tab buttons you can reach (Linux mode hides the desktop pages). */
  tabs() {
    return [...document.querySelectorAll(".tab")].filter((b) => b.offsetParent !== null);
  },

  go(tab, focusTab = false) {
    const b = this.tabs().find((x) => x.dataset.tab === tab);
    if (!b) return;
    settings.show(tab);
    if (focusTab) b.focus();
    document.body.scrollTop = 0;
  },

  step(by) {
    const list = this.tabs();
    const at = list.findIndex((b) => b.dataset.tab === settings.tab);
    const next = list[(at + by + list.length) % list.length];
    this.go(next.dataset.tab, document.activeElement?.classList.contains("tab"));
  },

  /** Where a keystroke belongs to a control, not to Settings. */
  busy(e) {
    if (recording || e.defaultPrevented) return true;
    const t = e.target;
    return t.isContentEditable || t.tagName === "TEXTAREA" || t.tagName === "SELECT"
      || (t.tagName === "INPUT" && t.type !== "checkbox" && t.type !== "radio");
  },

  /** The search for this tab: a box, or the theme menu. */
  search() {
    const id = { mine: "#mine-search", themes: "#gt-search", colours: "#co-theme", wallpapers: "#wp-theme" }[settings.tab];
    const box = id && $(id);
    if (!box) return toast("This tab has no search");
    box.focus();
    box.select?.();
  },

  // ---- My themes: ↑↓ choose a row, ↩ use it, F star it.
  rows() { return settings.tab === "mine" ? [...document.querySelectorAll("#mine > .row")] : []; },
  picked: null,
  pick(by) {
    const rows = this.rows();
    if (!rows.length) return false;
    const at = rows.indexOf(this.picked);
    const next = rows[at < 0 ? (by > 0 ? 0 : rows.length - 1) : Math.max(0, Math.min(rows.length - 1, at + by))];
    rows.forEach((r) => r.classList.toggle("kb-sel", r === next));
    this.picked = next;
    next.scrollIntoView({ block: "nearest" });
    return true;
  },
  pressInRow(which) {
    const row = this.rows().includes(this.picked) ? this.picked : null;
    if (!row) return false;
    const btn = which === "star" ? row.querySelector(".mine-star") : row.querySelector(":scope > .btn");
    if (btn && !btn.disabled) btn.click();
    return true;
  },

  // ---- ↑↓ setting to setting: focus the next row's first control. Text boxes, sliders and menus
  // keep their own arrows (busy); Esc leaves them, and ↑↓ go on from that row.
  lastRow: null,
  control(row) {
    return row.querySelector("input:not([type=hidden]):not([disabled]), select:not([disabled]), button:not([disabled])");
  },
  moveRow(by) {
    const pane = document.querySelector(`[data-pane="${settings.tab}"]`);
    if (!pane) return false;
    const rows = [...pane.querySelectorAll(".row")].filter((r) => r.offsetParent !== null && this.control(r));
    if (!rows.length) return false;
    const from = document.activeElement?.closest?.(".row") || (pane.contains(this.lastRow) ? this.lastRow : null);
    const at = rows.indexOf(from);
    // Nothing chosen yet: start with the first setting on screen, not the top of the page.
    const firstSeen = rows.find((r) => r.getBoundingClientRect().bottom > 0) || rows[0];
    const next = at < 0 ? firstSeen : rows[at + by];
    if (!next) return false; // past the first or last setting: the page scrolls as usual
    this.lastRow = next;
    this.control(next).focus();
    next.scrollIntoView({ block: "nearest" });
    return true;
  },

  // ---- ? : every shortcut.
  sheet() {
    const open = $("#keys-sheet");
    if (open) return open.remove();
    const line = (k, what) => el("div", {}, el("kbd", {}, k), el("span", {}, what));
    const sheet = el("div", { id: "keys-sheet", class: "keys-sheet", role: "dialog", "aria-label": "Keyboard shortcuts" },
      el("div", { class: "keys-card" },
        el("h2", {}, "Keyboard"),
        line("1 … 9, 0", "Apps, My themes, Get themes, Wallpapers, Colours, Get the tools, Setup, Move to another Mac, General, Schedule"),
        line("W  B  J  P  D", "Windows, Bar, Borders, Prompt, Dock & menu bar"),
        line("H", "Where things are"),
        line("⌘[  ⌘]", "Previous / next tab"),
        line("↑ ↓", "Through the tabs (in the list); setting to setting on a page; through the themes (My themes)"),
        line("← →", "From the page to the tab list, and back"),
        line("↩  F", "My themes: use the chosen theme, star it"),
        line("/  ⌘F", "Search (My themes, Get themes), or the theme menu (Colours, Wallpapers)"),
        line("Tab  Space", "Move between controls, press one (as anywhere on the Mac)"),
        line("Esc", "Leave a box, menu or control, so letters and numbers are shortcuts again"),
        line("?", "This list (Esc closes it)"),
        line("⌘W", "Close Settings"),
        el("p", { class: "note" }, "Letters and numbers do nothing while you type in a box.")));
    sheet.onclick = (e) => { if (e.target === sheet) sheet.remove(); };
    document.body.append(sheet);
  },

  onKey(e) {
    const cmd = e.metaKey && !e.ctrlKey && !e.altKey;
    // ⌘ shortcuts work even inside a box.
    if (cmd && (e.key === "[" || e.key === "]")) { e.preventDefault(); return this.step(e.key === "]" ? 1 : -1); }
    if (cmd && e.key.toLowerCase() === "f") { e.preventDefault(); return this.search(); }
    if (cmd && e.key.toLowerCase() === "w") { e.preventDefault(); return invoke("hide"); }
    if (e.key === "Escape" && !recording) {
      if ($("#keys-sheet")) { e.preventDefault(); return $("#keys-sheet").remove(); }
      // Out of a box or control (owner): the letters and numbers are shortcuts again. A menu that's
      // open closes on its own first; the next Esc leaves it.
      const at = document.activeElement;
      if (at && at !== document.body) { this.lastRow = at.closest?.(".row") || this.lastRow; at.blur(); e.preventDefault(); }
      return;
    }
    if (e.metaKey || e.ctrlKey || e.altKey || this.busy(e)) return;

    const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    const onTab = document.activeElement?.classList.contains("tab");
    if (/^[0-9]$/.test(k)) { e.preventDefault(); return this.go(this.numbers[(Number(k) + 9) % 10]); }
    if (this.letters[k]) { e.preventDefault(); return this.go(this.letters[k]); }
    if (k === "?") { e.preventDefault(); return this.sheet(); }
    if (k === "/") { e.preventDefault(); return this.search(); }
    if (k === "f" && this.pressInRow("star")) return e.preventDefault();
    if (k === "Enter" && !onTab && this.pressInRow("use")) return e.preventDefault();
    if (k === "ArrowUp" || k === "ArrowDown") {
      const by = k === "ArrowDown" ? 1 : -1;
      if (onTab) { e.preventDefault(); return this.step(by); }
      // A list to walk (My themes); elsewhere setting to setting (the page scrolls with it).
      if (this.pick(by) || this.moveRow(by)) e.preventDefault();
      return;
    }
    if (k === "ArrowLeft" && !onTab) {
      e.preventDefault();
      return this.tabs().find((b) => b.dataset.tab === settings.tab)?.focus();
    }
    if (k === "ArrowRight" && onTab) {
      e.preventDefault();
      if (this.pick(1)) return document.activeElement.blur();
      const pane = document.querySelector(`[data-pane="${settings.tab}"]`);
      pane?.querySelector("button:not([disabled]), input, select, a[href]")?.focus();
    }
  },

  init() {
    // The key next to each tab name, faintly.
    const all = { ...Object.fromEntries(this.numbers.map((t, i) => [t, String((i + 1) % 10)])),
      ...Object.fromEntries(Object.entries(this.letters).map(([k, t]) => [t, k.toUpperCase()])) };
    document.querySelectorAll(".tab").forEach((b) => {
      const k = all[b.dataset.tab];
      if (k) b.append(el("span", { class: "tab-key", "aria-hidden": "true" }, k));
    });
    // A new tab or list starts with nothing chosen.
    const show = settings.show.bind(settings);
    settings.show = (tab) => { this.picked = null; show(tab); };
    document.addEventListener("keydown", (e) => this.onKey(e));
    // Sticky bars (Apply) and toasts sit above the strip, whatever its height (it wraps when narrow).
    const strip = document.querySelector(".keys-strip");
    if (strip) {
      const fit = () => document.documentElement.style.setProperty("--strip-h", `${strip.offsetHeight}px`);
      new ResizeObserver(fit).observe(strip);
      fit();
    }
  },
};

keys.init();
