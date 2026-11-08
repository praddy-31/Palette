// Settings → My themes: every installed theme, ★ favourites (also `palette fav`), click to
// switch. Drawn desktops (12a), no wallpaper thumbnails, so the tab stays light.
"use strict";

Object.assign(settings, {
  async renderMine() {
    await load();
    const q = $("#mine-search").value.trim().toLowerCase();
    const favs = themes.filter((t) => t.favourite).length;
    $("#mine-count").textContent = `★ ${favs}/10 favourites · ${themes.length} themes`;
    // Favourites first, then the rest, each by name (sort keeps the order otherwise).
    const list = themes.filter((t) => findTheme(t, q));
    list.sort((a, b) => (b.favourite - a.favourite) || 0);
    $("#mine").replaceChildren(...list.map((t) => this.mineRow(t)));
  },

  mineRow(t) {
    const sw = el("div", { class: "mine-sw" }, t.invalid ? null : tinyDesktop(t));
    const star = el("button", {
      class: `mine-star${t.favourite ? " on" : ""}`,
      disabled: !!t.invalid,
      "aria-label": t.favourite ? `Remove ${t.name} from favourites` : `Add ${t.name} to favourites`,
      title: t.favourite ? "Favourite (click to remove)" : "Add to favourites",
      onclick: (e) => { e.stopPropagation(); this.mineFavourite(t); },
    }, t.favourite ? "★" : "☆");
    const use = el("button", {
      class: "btn", disabled: !!t.invalid || t.current,
      onclick: () => this.mineApply(t.name, use),
    }, t.current ? "Active" : "Use");
    return el("div", { class: `row${t.current ? " mine-current" : ""}`, title: t.invalid || "" },
      sw,
      el("div", {},
        el("div", { class: "title" }, t.name, " ",
          t.mode === "light" ? el("span", { class: "badge" }, "light") : null, " ",
          t.origin === "user" ? el("span", { class: "badge" }, "yours") : null, " ",
          t.invalid ? el("span", { class: "badge warn" }, "can't load") : contrastBadge(t)),
        el("div", { class: "sub" }, t.wallpapers ? `${t.wallpapers} wallpaper${t.wallpapers === 1 ? "" : "s"}` : "no wallpapers"),
        t.glass_wallpaper && !t.invalid ? this.mineGlass(t) : null),
      star, use);
  },

  /** The glass slider (F2): how hard this theme's text is lifted on see-through windows.
   *  Auto (11g) reaches the minimums over the wallpaper's average; by hand, 0 is off and
   *  1-100 % aims higher (up to 2×) against brighter parts of the wallpaper. The swatch
   *  shows the theme's text on the most see-through app's glass, live while dragging. */
  mineGlass(t) {
    const label = (v) => v == null ? "Auto" : v === 0 ? "Off" : `${v} %`;
    const val = el("span", { class: "mine-glass-val" }, label(t.glass_lift));
    const sw = el("span", { class: "mine-glass-sw", hidden: true });
    const range = el("input", {
      type: "range", min: 0, max: 100, step: 5, value: t.glass_lift ?? 0,
      class: t.glass_lift == null ? "auto" : "",
      "aria-label": `Glass lift for ${t.name}`,
    });
    const auto = el("button", {
      class: `btn mine-lift${t.glass_lift == null ? " on" : ""}`,
      "aria-pressed": t.glass_lift == null ? "true" : "false",
      title: "Auto: lift text just enough to read over the wallpaper's average",
      onclick: (e) => { e.stopPropagation(); this.mineLiftSave(t, null); },
    }, "Auto");
    let timer = 0;
    const show = async (v) => {
      try {
        const p = await invoke("glass_preview", { name: t.name, value: v });
        if (!p) { sw.hidden = true; return; }
        sw.style.background = p.glass;
        sw.replaceChildren(...p.colours.map(([k, hex]) => el("b", { style: { color: hex }, title: k }, "a")));
        sw.title = `${t.name}'s text on ${p.app}'s glass over ${t.glass_wallpaper}: weakest ${p.worst.toFixed(1)}:1` +
          (p.changed ? `, ${p.changed} colour${p.changed === 1 ? "" : "s"} lifted` : ", nothing lifted") +
          (p.faint.length ? `; can't read at any lightness: ${p.faint.join(", ")} (raise the opacity)` : "");
        sw.hidden = false;
      } catch { sw.hidden = true; }
    };
    range.addEventListener("click", (e) => e.stopPropagation());
    range.addEventListener("input", () => {
      range.classList.remove("auto");
      val.textContent = label(+range.value);
      clearTimeout(timer);
      timer = setTimeout(() => show(+range.value), 80);
    });
    range.addEventListener("change", () => this.mineLiftSave(t, +range.value));
    const box = el("div", { class: "mine-glass", title: `Glass lift with ${t.glass_wallpaper}: how hard text on see-through windows is lifted. Auto reads over the wallpaper's average; slide for more (up to twice the contrast, against its brightest parts) or to 0 for none. Also palette check ${t.name} --glass-lift auto|0-100` },
      el("span", { class: "mine-glass-l" }, "Glass lift"), auto, range, val, sw);
    // The swatch appears when you reach for the slider (and for the active theme at once).
    box.addEventListener("pointerenter", () => { if (sw.hidden) show(t.glass_lift); }, { once: true });
    if (t.current) show(t.glass_lift);
    return box;
  },

  async mineFavourite(t) {
    try {
      const favs = await invoke("set_favourite", { name: t.name, on: !t.favourite });
      toast(t.favourite ? `${t.name} isn't a favourite any more` : `★ ${t.name}: in Favourites (${favs.length}/10)`);
    } catch (e) { toast(String(e), true); }
    this.renderMine();
  },

  async mineLiftSave(t, value) {
    try {
      const now = await invoke("set_glass_lift", { name: t.name, value });
      toast(`${t.name}: glass lift ${now}${t.current ? "" : " (when you switch to it)"}`);
    } catch (e) { toast(String(e), true); }
    this.renderMine();
  },

  async mineApply(name, button) {
    button.disabled = true;
    try { await invoke("apply", { name }); } // the "switched" event shows the toast
    catch (e) { toast(String(e), true); }
    this.renderMine();
  },
});

$("#mine-search").addEventListener("input", () => settings.renderMine());
$("#mine-search").parentElement.after(chipRow($("#mine-search"), () => settings.renderMine()));
