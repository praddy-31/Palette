// Settings → Get themes: the community catalogue, install, update, remove.
"use strict";

Object.assign(settings, {
  // ---------------- get themes
  catalog: null,

  async renderThemes() {
    if (!this.catalog) {
      try { this.catalog = await invoke("catalog"); } catch (e) { toast(String(e), true); return; }
    }
    const q = $("#gt-search").value.trim().toLowerCase();
    const f = $("#gt-filter").value;
    const shown = this.catalog.filter((t) => findTheme(t, q, `${t.title} ${t.author}`) &&
      (!f || t.mood === f || (f === "light" && t.mode === "light") || (f === "installed" && t.installed)));
    $("#gt-grid").replaceChildren(...shown.map((t) => this.themeCard(t)));
    if (!shown.length) $("#gt-grid").replaceChildren(el("div", { class: "empty" }, "No theme matches."));
    // Installed from a link (not in the list).
    const known = new Set(this.catalog.map((t) => t.name));
    const others = (await invoke("installed_themes")).filter(([n]) => !known.has(n));
    $("#gt-others").replaceChildren(...others.map(([name, url]) => el("div", { class: "row" },
      el("div", {}, el("div", { class: "title" }, name), el("div", { class: "sub" }, url)),
      this.btn("Update", (b) => this.installTheme(name, b, true)),
      this.btn("Remove…", (b) => this.removeTheme(name, b)))));
  },

  btn(label, onclick, cls = "btn") {
    const b = el("button", { class: cls }, label);
    b.onclick = () => onclick(b);
    return b;
  },

  themeCard(t) {
    const c = t.colors;
    // 12a: drawn from the theme's colours, so nothing is downloaded to show it.
    const sw = el("div", { class: "sw" }, tinyDesktop(t));
    const card = el("div", { class: "gt", title: `${t.title} by ${t.author}\n${t.url}` }, sw);
    const acts = el("div", { class: "acts" });
    if (t.installed) {
      acts.append(
        this.btn("Use", async () => { try { report(await invoke("apply", { name: t.name })); toast(`Now ${t.name}`); } catch (e) { toast(String(e), true); } }, "btn primary"),
        this.btn("Update", (b) => this.installTheme(t.name, b, true, card)));
      const x = this.btn("×", (b) => this.removeTheme(t.name, b), "btn icon");
      x.title = "Remove this theme (click twice)";
      acts.append(x);
    } else {
      acts.append(this.btn("Install", (b) => this.installTheme(t.name, b, false, card), "btn primary"));
    }
    if (!t.installed) acts.append(el("span", { class: "badge" }, t.mode === "light" ? "light" : t.mood));
    card.append(el("div", { class: "meta" }, el("div", { class: "ttl" }, t.title || t.name), el("div", { class: "by" }, `by ${t.author}`), contrastBadge(t, true), acts));
    return card;
  },

  async installTheme(source, btn, update = false, card = null) {
    const label = btn.textContent;
    btn.textContent = update ? "Updating…" : "Downloading…";
    card?.classList.add("busy");
    let failed = null;
    try {
      const r = await invoke("install_theme", { source, update });
      toast(`${update ? "Updated" : "Installed"} ${r.name}${r.wallpapers ? ` with ${r.wallpapers} wallpaper${r.wallpapers === 1 ? "" : "s"}` : ""}`);
    } catch (e) { failed = e; toast(String(e), true); btn.textContent = label; }
    card?.classList.remove("busy");
    this.catalog = null;
    await load();
    this.renderThemes();
    return failed;
  },

  /** A link that isn't a theme: offer to browse it as a wallpaper collection. */
  offerWallpapers(url, why) {
    const box = $("#gt-offer");
    box.hidden = false;
    box.replaceChildren(
      el("div", {}, el("b", {}, "This looks like a wallpaper collection"), ` — ${why}`),
      el("div", { class: "actions" },
        el("button", {
          class: "btn primary",
          onclick: () => {
            box.hidden = true;
            this.show("wallpapers");
            $("#gl-url").value = url;
            this.linkLook();
          },
        }, "Browse its images"),
        el("button", { class: "btn", onclick: () => (box.hidden = true) }, "Not now")));
  },

  async removeTheme(name, btn) {
    if (!armed(btn, "Click again")) return toast(`Click again to delete ${name} and its wallpapers`);
    try { const r = await invoke("remove_theme", { name }); toast(r.message, r.warning); } catch (e) { toast(String(e), true); }
    this.catalog = null;
    await load();
    this.renderThemes();
  },
});
