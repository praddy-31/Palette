// Settings → Setup: wire every app, and the doctor's checks.
"use strict";

Object.assign(settings, {
  // ---------------- setup
  async runSetup(remove) {
    const btn = remove ? $("#remove-all") : $("#setup");
    if (remove && !armed(btn, "Click again to remove")) return;
    btn.disabled = true;
    try {
      const r = await invoke("setup_all", { remove });
      const changed = r.lines.length;
      const manual = r.lines.filter((l) => l.what.startsWith("needs you"));
      let msg = changed ? `${remove ? "Restored" : "Set up"} ${changed} config${changed === 1 ? "" : "s"}` : "Nothing to change";
      if (r.backup) msg += "; originals backed up";
      if (manual.length) msg += `. ${manual.map((l) => `${l.app}: ${l.what}`).join(" · ")}`;
      toast(msg + (r.problems.length ? ` · ${r.problems.join(" · ")}` : ""), manual.length > 0 || r.problems.length > 0);
    } catch (e) { toast(String(e), true); }
    btn.disabled = false;
    await this.renderApps();
    this.check();
  },

  // The same apps, groups and order as the Apps tab; apps that aren't installed
  // stay in place but muted, so the two lists always match.
  async check() {
    const out = $("#check-out");
    out.replaceChildren(el("div", { class: "empty" }, "Checking…"));
    try {
      const [rows, apps] = await Promise.all([invoke("check"), invoke("apps")]);
      const sign = { ok: "✓", fix: "!", off: "–" };
      const byName = new Map(rows.map((r) => [r.name, r]));
      // Wallpaper and light/dark aren't apps for the doctor: say whether they are on.
      for (const a of apps) {
        if ((a.name === "wallpaper" || a.name === "appearance") && !byName.has(a.name)) {
          byName.set(a.name, { name: a.name, status: a.on ? "ok" : "off", detail: a.on ? "on" : "off", notes: [] });
        }
      }
      const known = new Set(GROUPS.flatMap(([, names]) => names));
      const own = rows.filter((r) => ["theme", "command", "backups"].includes(r.name));
      const other = rows.filter((r) => !known.has(r.name) && !own.includes(r));
      const groups = [["Palette", own.map((r) => r.name)], ...GROUPS, ["Other", other.map((r) => r.name)]];
      const line = (r) => [
        el("div", { class: `c${r.status === "off" ? " absent" : ""}` },
          el("span", { class: `s ${r.status}` }, sign[r.status]),
          el("span", {}, (LABELS[r.name] || [r.name])[0]),
          el("span", { class: "d" }, r.detail)),
        ...r.notes.map(([kind, text]) => el("div", { class: "c" }, el("span"), el("span"),
          el("span", { class: `n${kind === "fix" ? " fix" : kind === "info" ? " info" : ""}` }, kind === "fix" ? `! ${text}` : text))),
      ];
      out.replaceChildren(...groups.flatMap(([title, names]) => {
        const items = names.map((n) => byName.get(n)).filter(Boolean);
        return items.length ? [el("h3", {}, title), ...items.flatMap(line)] : [];
      }));
    } catch (e) {
      out.replaceChildren(el("div", { class: "empty" }, String(e)));
    }
  },
});
