// The welcome tour: five steps on first launch (General → Welcome tour).
"use strict";

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
    const p = await invoke("prefs");
    const key = pretty(p.hotkey);
    // What a click opens is a setting (General → "Menu bar click opens"), so say the one in use.
    const click = {
      menu: "Click it for a menu of your themes, favourites and more",
      picker: "Click it for the theme picker",
      "quick-switcher": "Click it for the quick switcher",
    }[p.menu_click] || "Click it to switch theme";
    return [
      el("h1", {}, "Welcome to Palette"),
      el("p", { class: "lead" }, "One theme restyles your Mac: the wallpaper, light or dark mode, and every app Palette knows how to colour. This takes a minute."),
      el("ul", { class: "facts" },
        el("li", {}, el("b", {}, "Menu bar"), el("span", {}, `Palette lives in the menu bar (the palette icon; it can hide behind the notch). ${click}${p.menu_click === "menu" ? "" : "; right-click it for the menu"} (General → "Menu bar click opens" changes this).`)),
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
        el("p", { class: "note" }, "Install one later (Ghostty, kitty, VSCodium, Zed, Neovim, Obsidian, btop, starship…) and it shows up in Settings → Apps with a Set up button."),
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
