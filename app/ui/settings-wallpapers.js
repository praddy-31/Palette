// Settings → Wallpapers: a theme's wallpapers, the studio, the library, wallpapers from a
// link, and a new theme from a wallpaper.
"use strict";

Object.assign(settings, {
  // ---------------- wallpapers
  wpTheme: null,

  async renderWallpapers() {
    const select = $("#wp-theme");
    const cur = themes.find((t) => t.current);
    if (!this.wpTheme) this.wpTheme = cur?.name;
    select.replaceChildren(...themes.filter((t) => !t.invalid).map((t) => {
      const o = el("option", { value: t.name }, `${t.name}${t.current ? " (active)" : ""}`);
      if (t.name === this.wpTheme) o.selected = true;
      return o;
    }));
    let w;
    try {
      w = await invoke("wallpapers", { theme: this.wpTheme });
    } catch (e) {
      $("#wp-grid").replaceChildren(el("div", { class: "empty" }, String(e)));
      return;
    }
    $("#wp-sub").textContent = `${w.images.length} wallpaper${w.images.length === 1 ? "" : "s"}${w.active ? " · click one to use it now" : " · switch to this theme to see them on the desktop"}`;
    $("#wp-next").disabled = !w.active || w.images.length < 2;
    $("#wp-dir").textContent = w.dir;
    $("#wp-size").textContent = `· ${size(w.bytes)}`;
    $("#wp-dir").title = w.dir;
    this.renderLibrary();
    const grid = $("#wp-grid");
    if (!w.images.length) {
      grid.replaceChildren(el("div", { class: "empty" }, "No wallpapers yet: Add images…, or add some from your library below"));
      return;
    }
    grid.replaceChildren(...w.images.map((img) => {
      const tile = el("div", {
        class: `wp${img.current ? " current" : ""}`,
        title: img.name,
        onclick: () => this.useWallpaper(w.theme, img.name),
      },
        el("div", { class: "label" }, img.name),
        el("button", {
          class: `star${img.default ? " on" : ""}`,
          title: img.default ? `${w.theme} starts with this one (click: back to the first)` : `Start ${w.theme} with this wallpaper`,
          onclick: async (e) => {
            e.stopPropagation();
            try { await invoke("set_default_wallpaper", { theme: w.theme, name: img.default ? null : img.name }); }
            catch (err) { toast(String(err), true); }
            this.renderWallpapers();
          },
        }, img.default ? "★" : "☆"),
        el("button", {
          class: "x", title: "Remove from this theme (your original file stays)",
          onclick: (e) => { e.stopPropagation(); this.removeWallpaper(w.theme, img.name, e.target); },
        }, "×"));
      invoke("wallpaper_thumb", { theme: w.theme, name: img.name })
        .then((src) => (tile.style.backgroundImage = `url("${src}")`))
        .catch(() => {});
      return tile;
    }));
  },

  async useWallpaper(theme, name) {
    try {
      const problems = await invoke("set_wallpaper", { theme, name });
      if (problems.length) toast(problems.join(" · "), true);
    } catch (e) { toast(String(e), true); }
    this.renderWallpapers();
  },

  async removeWallpaper(theme, name, btn) {
    if (!armed(btn, "?")) return toast(`Click × again to remove ${name} from ${theme} (your original file stays)`);
    try { await invoke("remove_wallpaper", { theme, name }); } catch (e) { toast(String(e), true); }
    this.renderWallpapers();
  },

  /** Run `work` with `btn` disabled and showing `busy`. */
  async busy(btn, busy, work) {
    const label = btn.textContent;
    btn.disabled = true;
    btn.textContent = busy;
    try { await work(); } catch (e) { toast(String(e), true); }
    btn.disabled = false;
    btn.textContent = label;
  },

  async recolour() {
    const theme = this.wpTheme;
    const engine = $("#wp-engine").hidden ? "palette" : $("#wp-engine").value;
    const strength = $("#wp-strength").value;
    await this.busy($("#wp-recolour"), "Recolouring…", async () => {
      const made = await invoke("recolour_wallpapers", { theme, engine, strength });
      if (made.length) toast(`Added ${made.length === 1 ? made[0] : `${made.length} recoloured wallpapers`} to ${theme}; click one to use it`);
    });
    this.renderWallpapers();
  },

  // `variant`: paint a new random variant (palette-<style>-<seed>.jpg).
  async generate(variant = false) {
    const theme = this.wpTheme;
    const seed = variant ? Math.floor(Math.random() * 100000) : null;
    await this.busy($(variant ? "#wp-variant" : "#wp-generate"), "Painting…", async () => {
      const name = await invoke("generate_wallpaper", { theme, style: $("#wp-style").value, seed });
      toast(`Added ${name} to ${theme}; click it to use it`);
    });
    this.renderWallpapers();
  },

  // ---------------- wallpaper library
  libPicked: null,
  libOpen: new Set([""]), // folders shown open ("" = the top level)

  async renderLibrary() {
    let lib;
    try { lib = await invoke("library"); } catch (e) { $("#lib-grid").replaceChildren(el("div", { class: "empty" }, String(e))); return; }
    const n = lib.items.length;
    $("#lib-sub").textContent = `· ${n} picture${n === 1 ? "" : "s"}, ${size(lib.bytes)}`;
    $("#lib-dir").textContent = lib.dir;
    $("#lib-dir").title = lib.dir;
    if (!n) {
      $("#lib-grid").replaceChildren(el("div", { class: "empty" }, "Empty. Add images…, or get some from a link below."));
      $("#lib-act").hidden = true;
      return;
    }
    // One section per folder; thumbnails are made only for open ones.
    const byFolder = new Map();
    for (const item of lib.items) {
      if (!byFolder.has(item.folder)) byFolder.set(item.folder, []);
      byFolder.get(item.folder).push(item);
    }
    const sections = [...byFolder.keys()].sort().map((folder) => {
      const items = byFolder.get(folder);
      const grid = el("div", { class: "wp-grid" });
      const fill = () => {
        if (grid.childElementCount) return;
        grid.append(...items.map((item) => this.libTile(item)));
      };
      const bytes = items.reduce((s, i) => s + i.bytes, 0);
      const d = el("details", { class: "lib-folder" },
        el("summary", {}, el("span", {}, folder || "Top level"), el("span", { class: "sub" }, `${items.length} · ${size(bytes)}`)),
        grid);
      if (this.libOpen.has(folder)) { d.open = true; fill(); }
      d.addEventListener("toggle", () => {
        if (d.open) { this.libOpen.add(folder); fill(); } else this.libOpen.delete(folder);
      });
      return d;
    });
    $("#lib-grid").replaceChildren(...sections);
    if (this.libPicked && !lib.items.some((i) => i.name === this.libPicked)) { this.libPicked = null; $("#lib-act").hidden = true; }
  },

  libTile(item) {
    const label = item.name.split("/").pop();
    const tile = el("div", {
      class: `wp${this.libPicked === item.name ? " current" : ""}`,
      title: `${item.name} · ${size(item.bytes)}`,
      "data-name": item.name,
      onclick: () => this.libPick(item),
    }, el("div", { class: "label" }, label));
    invoke("library_thumb", { name: item.name })
      .then((src) => (tile.style.backgroundImage = `url("${src}")`))
      .catch(() => {});
    return tile;
  },

  /** Actions for one library picture, under the grid. */
  libPick(item) {
    this.libPicked = item.name;
    document.querySelectorAll("#lib-grid .wp").forEach((t) => t.classList.toggle("current", t.dataset.name === item.name));
    const box = $("#lib-act");
    box.hidden = false;
    const target = el("select", { class: "select", title: "Theme to add it to" },
      ...themes.filter((t) => !t.invalid).map((t) => {
        const o = el("option", { value: t.name }, `${t.name}${t.current ? " (active)" : ""}`);
        if (t.name === this.wpTheme) o.selected = true;
        return o;
      }));
    const recolour = el("input", { type: "checkbox" });
    const add = el("button", { class: "btn" }, "Add to theme");
    add.onclick = () => this.busy(add, recolour.checked ? "Recolouring…" : "Adding…", async () => {
      const made = await invoke("library_to_theme", { name: item.name, theme: target.value, recolour: recolour.checked });
      toast(`Added ${made} to ${target.value}`);
      await load();
      this.renderWallpapers();
    });
    const newTheme = el("button", { class: "btn" }, "New theme from this");
    newTheme.onclick = () => this.busy(newTheme, "Reading colours…", async () => {
      const d = await invoke("draft_theme_from_library", { name: item.name, light: null });
      if (!d) return;
      this.draft = d;
      this.ntRender();
      $("#nt-draft").scrollIntoView({ behavior: "smooth", block: "nearest" });
    });
    const remove = el("button", { class: "btn danger" }, "Remove");
    remove.onclick = async () => {
      if (!armed(remove, "Click again")) return toast(`Click again to delete ${item.name} from your library (themes keep their copies)`);
      try { await invoke("library_remove", { name: item.name }); toast(`Removed ${item.name.split("/").pop()}`); } catch (e) { toast(String(e), true); }
      this.libPicked = null;
      box.hidden = true;
      this.renderLibrary();
    };
    box.replaceChildren(
      el("div", { class: "lib-name" }, el("b", {}, item.name.split("/").pop()), el("span", { class: "sub" }, ` ${item.folder ? `${item.folder} · ` : ""}${size(item.bytes)}`)),
      el("div", { class: "lib-row" },
        el("button", {
          class: "btn primary",
          onclick: async () => {
            try {
              const problems = await invoke("library_use", { name: item.name });
              toast(problems.length ? problems.join(" · ") : "Using it until the next theme switch", problems.length > 0);
            } catch (e) { toast(String(e), true); }
          },
        }, "Use now"),
        newTheme,
        el("span", { class: "spacer" }),
        remove),
      el("div", { class: "lib-row" },
        target,
        el("label", { class: "check" }, recolour, "recolour to it"),
        add));
  },

  async libAdd() {
    try {
      const n = await invoke("library_add");
      if (n) toast(`Added ${n} picture${n === 1 ? "" : "s"} to your library`);
    } catch (e) { toast(String(e), true); }
    this.renderLibrary();
  },

  // ---------------- get wallpapers from a link
  link: null, // { url, listing, chosen: Set of paths }

  async linkLook() {
    const url = $("#gl-url").value.trim();
    if (!url) return toast("Paste a link first: a GitHub repo or folder, or a picture");
    const box = $("#gl-out");
    await this.busy($("#gl-go"), "Looking…", async () => {
      const listing = await invoke("wallpaper_link", { url });
      this.link = { url, listing, chosen: new Set(), open: new Set() };
      this.linkRender();
      box.scrollIntoView({ behavior: "smooth", block: "nearest" });
    });
  },

  linkRender() {
    const { listing: l, chosen } = this.link;
    const box = $("#gl-out");
    box.hidden = false;
    const files = l.files;
    const head = [];
    if (l.kind === "image") {
      head.push(el("div", {}, "One picture: ", el("b", {}, files[0].path)));
      files.forEach((f) => chosen.add(f.path));
    } else {
      const total = files.reduce((s, f) => s + f.bytes, 0);
      head.push(el("div", {}, el("b", {}, l.title), ` · ${files.length} picture${files.length === 1 ? "" : "s"}, ${size(total)} in ${l.folders.length} folder${l.folders.length === 1 ? "" : "s"}`));
      if (l.truncated) head.push(el("div", { class: "warn" }, "GitHub listed only part of this repo (it's very big). Paste a link to one folder (open it on GitHub and copy the address) to see all of it."));
      if (!files.length) head.push(el("div", { class: "empty" }, "No pictures there (png, jpg, webp, heic, tiff)."));
    }
    const list = el("div", { class: "gl-list" });
    if (l.kind === "github") {
      for (const folder of l.folders) {
        const inside = files.filter((f) => (f.path.includes("/") ? f.path.slice(0, f.path.lastIndexOf("/")) : "") === folder.path);
        const all = inside.every((f) => chosen.has(f.path));
        const some = inside.some((f) => chosen.has(f.path));
        const box2 = el("input", { type: "checkbox", title: "Choose the whole folder" });
        box2.checked = all;
        box2.indeterminate = some && !all;
        box2.onclick = (e) => {
          e.stopPropagation();
          inside.forEach((f) => (box2.checked ? chosen.add(f.path) : chosen.delete(f.path)));
          this.linkRender();
        };
        const d = el("details", { class: "gl-folder" },
          el("summary", {}, box2, el("span", { class: "name" }, folder.path || "Top level"), el("span", { class: "sub" }, `${folder.count} · ${size(folder.bytes)}`)));
        const fill = () => {
          if (d.childElementCount > 1) return;
          d.append(...inside.map((f) => {
            const c = el("input", { type: "checkbox" });
            c.checked = chosen.has(f.path);
            c.onchange = () => { c.checked ? chosen.add(f.path) : chosen.delete(f.path); this.linkRender(); };
            return el("label", { class: "gl-file" }, c, el("span", { class: "name" }, f.path.split("/").pop()), el("span", { class: "sub" }, size(f.bytes)));
          }));
        };
        if (this.link.open.has(folder.path)) { d.open = true; fill(); }
        d.addEventListener("toggle", () => {
          if (d.open) { this.link.open.add(folder.path); fill(); } else this.link.open.delete(folder.path);
        });
        list.append(d);
      }
    }
    const picked = files.filter((f) => chosen.has(f.path));
    const bytes = picked.reduce((s, f) => s + f.bytes, 0);
    const over = picked.length > l.max_files || bytes > l.max_bytes;
    const theme = el("select", { class: "select", id: "gl-theme", title: "Also add them to a theme (optional)" },
      el("option", { value: "" }, "Library only"),
      ...themes.filter((t) => !t.invalid).map((t) => el("option", { value: t.name }, `+ ${t.name}`)));
    if (this.link.theme) theme.value = this.link.theme;
    theme.onchange = () => (this.link.theme = theme.value);
    const recolour = el("label", { class: "check" }, el("input", { type: "checkbox", id: "gl-recolour" }), "recolour");
    const go = el("button", { class: "btn primary", id: "gl-download" }, l.kind === "image" ? "Download" : "Download selected");
    go.disabled = !picked.length || over;
    go.onclick = () => this.linkDownload(go);
    const summary = l.kind === "image" ? `Saves to ${l.into}/ in your library`
      : over ? `${picked.length} chosen (${size(bytes)}): at most ${l.max_files} pictures / ${size(l.max_bytes)} at a time`
        : `${picked.length} chosen${picked.length ? ` (${size(bytes)})` : ""} · saves to ${l.into}/ in your library`;
    box.replaceChildren(...head, list,
      el("div", { class: `gl-foot${over ? " over" : ""}` }, el("span", { class: "sub" }, summary)),
      el("div", { class: "lib-row" }, theme, recolour, el("span", { class: "spacer" }), go));
  },

  async linkDownload(btn) {
    const { url, listing, chosen } = this.link;
    const files = listing.files.filter((f) => chosen.has(f.path));
    const theme = $("#gl-theme").value || null;
    const recolour = $("#gl-recolour").checked;
    await this.busy(btn, `Downloading ${files.length}…`, async () => {
      const r = await invoke("wallpaper_link_download", { url, files, theme, recolour });
      const extra = theme && r.added ? `, ${r.added} added to ${theme}` : "";
      const skipped = r.skipped.length ? ` · ${r.skipped.length} skipped: ${r.skipped.slice(0, 2).map(([p, why]) => `${p.split("/").pop()} (${why})`).join(", ")}` : "";
      toast(`Saved ${r.saved} picture${r.saved === 1 ? "" : "s"} (${size(r.bytes)})${extra}${skipped}`, r.saved === 0);
      if (r.saved) { chosen.clear(); this.libOpen.add(r.dir.split("/wallpapers/").pop() || ""); }
      if (theme) await load();
      this.renderWallpapers();
    });
    if (this.link) this.linkRender();
  },

  // ---------------- new theme from a wallpaper
  draft: null,

  async ntDraft(light = null) {
    const btn = $("#nt-pick");
    await this.busy(btn, "Reading colours…", async () => {
      const d = await invoke("draft_theme_from_wallpaper", { path: this.draft?.path ?? null, light });
      if (!d) return;
      // Keep a name the user typed when only light/dark changed.
      if (this.draft && light !== null) d.name = $("#nt-name").value.trim() || d.name;
      this.draft = d;
      this.ntRender();
    });
  },

  ntRender() {
    const d = this.draft;
    const box = $("#nt-draft");
    box.hidden = false;
    const c = d.colors;
    const mode = (light) => el("button", {
      class: `btn${d.light === light ? " on" : ""}`,
      onclick: () => d.light !== light && this.ntDraft(light),
    }, light ? "Light" : "Dark");
    const name = el("input", { id: "nt-name", class: "search", type: "text", value: d.name, spellcheck: "false", autocomplete: "off", placeholder: "theme-name" });
    name.addEventListener("keydown", (e) => { if (e.key === "Enter") this.ntSave(); });
    box.replaceChildren(
      el("div", { class: "nt-top" },
        el("div", { class: "pic", style: { backgroundImage: d.thumb ? `url("${d.thumb}")` : "none" } }),
        el("div", { class: "nt-info" },
          el("div", { class: "nt-label" }, "Picture's colours"),
          el("div", { class: "sws" }, ...d.swatches.map((s) => el("i", { title: s, style: { background: s } }))),
          el("div", { class: "nt-label" }, `Theme · ${d.light ? "light" : "dark"}, ${d.mood}`),
          el("div", { class: "nt-prev", style: { background: c.background, color: c.foreground } },
            el("div", {}, el("span", { style: { color: c.accent } }, "❯ "), "palette set ", el("span", { style: { color: c.accent } }, d.name)),
            el("div", { class: "sws" }, ...["accent", "red", "yellow", "green", "cyan", "blue", "magenta"].map((k) => el("i", { title: `${k} ${c[k]}`, style: { background: c[k] } })))))),
      el("div", { class: "nt-form" },
        name,
        el("div", { class: "seg" }, mode(false), mode(true)),
        el("span", { class: "spacer" }),
        el("button", { class: "btn primary", id: "nt-save", onclick: () => this.ntSave() }, "Save"),
        el("button", { class: "btn", onclick: () => { box.hidden = true; this.draft = null; } }, "Cancel")));
  },

  async ntSave() {
    const d = this.draft;
    const name = $("#nt-name").value.trim();
    if (!name) return toast("Give the theme a name first", true);
    await this.busy($("#nt-save"), "Saving…", async () => {
      await invoke("save_theme_from_wallpaper", { name, path: d.path, light: d.light });
      await load();
      this.draft = null;
      const box = $("#nt-draft");
      box.replaceChildren(
        el("div", {}, el("b", {}, name), " is saved with the picture as its wallpaper. Edit its colors.toml any time."),
        el("div", { class: "actions" },
          el("button", {
            class: "btn primary",
            onclick: async () => {
              try { report(await invoke("apply", { name })); toast(`Now ${name}`); } catch (e) { toast(String(e), true); }
              box.hidden = true;
              await load();
              this.wpTheme = name;
              this.renderWallpapers();
            },
          }, "Use it"),
          el("button", { class: "btn", onclick: () => (box.hidden = true) }, "Done")));
    });
  },
});
