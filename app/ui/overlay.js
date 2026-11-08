// The quick switcher: the hotkey's overlay in the middle of the screen (`overlay.html`).
"use strict";

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
    return themes.filter((t) => !t.invalid && findTheme(t, this.filter));
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
        el("span", { class: "dot" }, tinyDesktop(t)),
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
        // Letters, and spaces between words ("warm dark": 12c).
        if ((/^[a-z0-9-]$/i.test(e.key) || (e.key === " " && this.filter && !this.filter.endsWith(" "))) && !e.metaKey && !e.ctrlKey) {
          this.filter += e.key.toLowerCase();
          this.index = 0;
          this.render();
        } else return;
    }
    e.preventDefault();
  },
};

// ---------------------------------------------------------------- wiring

document.addEventListener("keydown", (e) => overlay.key(e));

start(overlay);
