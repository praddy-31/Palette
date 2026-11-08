// The picker: the theme grid under the menu bar icon (`picker.html`).
"use strict";

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
    for (const t of themes.filter((t) => findTheme(t, q))) grid.append(this.card(t));
    if (!q || "get more themes".includes(q)) {
      grid.append(el("button", {
        class: "card more", title: "Community themes for Omarchy, installable here",
        onclick: async () => { await invoke("open_settings_tab", { tab: "themes" }); invoke("hide"); },
      }, el("div", { class: "thumb" }, el("span", {}, "+")), el("div", { class: "meta" }, el("span", { class: "name" }, "Get more themes"))));
    }
    grid.querySelector(".grid > .card.current")?.scrollIntoView({ block: "nearest" });
    this.renderWall();
  },

  /** One theme card: the theme as a tiny desktop, ★ toggle, name and badges. */
  card(t) {
    // 12a: the theme drawn as a tiny desktop over its wallpaper (when it has one).
    const box = el("div", { class: "thumb" });
    if (!t.invalid) {
      const desk = tinyDesktop(t);
      box.append(desk);
      thumb(t.name).then((src) => desk.setWallpaper(src));
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
    // ★ = the wallpaper on screen is the one this theme starts with.
    const shown = at >= 0 ? w.images[at] : null;
    const star = $("#wp-star");
    star.disabled = !shown;
    star.textContent = shown?.default ? "★" : "☆";
    star.classList.toggle("on", !!shown?.default);
    star.title = !shown ? "No wallpaper of this theme on screen"
      : shown.default ? `${w.theme} starts with this wallpaper (click: back to the first)`
      : `Start ${w.theme} with this wallpaper`;
    star.onclick = () => shown && this.starWall(w.theme, shown);
  },

  async starWall(theme, img) {
    try {
      await invoke("set_default_wallpaper", { theme, name: img.default ? null : img.name });
      toast(img.default ? `${theme} starts with its first wallpaper again` : `★ ${theme} now starts with ${img.name}`);
    } catch (e) { toast(String(e), true); }
    this.renderWall();
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

// ---------------------------------------------------------------- wiring

listen("switched", (e) => {
  const s = e.payload;
  if (s.problems.length) toast(s.problems.join(" · "), true);
  else if (s.undo) toast(`Back to ${s.theme}`);
  else undoToast(s.theme);
});
$("#search").addEventListener("input", () => picker.render());
$("#search").after(chipRow($("#search"), () => picker.render()));
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

start(picker);
