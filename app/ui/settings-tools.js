// Settings → Get the tools: install commands to copy, starter configs.
"use strict";

Object.assign(settings, {
  // ---------------- get the tools
  async renderTools() {
    const box = $("#tools");
    let s;
    try { s = await invoke("tools_status"); } catch (e) { return box.replaceChildren(el("div", { class: "empty" }, String(e))); }
    const parts = [];
    if (!s.homebrew) {
      parts.push(el("div", { class: "brew-missing" },
        el("span", {}, "Homebrew isn't installed, and the commands below need it. Get it from brew.sh first; Palette never installs it for you."),
        this.btn("Open brew.sh", async () => { try { await invoke("open_homebrew_site"); } catch (e) { toast(String(e), true); } })));
    }
    for (const [group, title] of [["look", "The look"], ["wallpaper", "Wallpaper recolourer"]]) {
      parts.push(el("h2", {}, title));
      for (const t of s.tools.filter((x) => x.group === group)) parts.push(this.toolCard(t));
    }
    box.replaceChildren(...parts);
  },

  cmdBlock(lines) {
    return el("div", { class: "cmd" }, lines.join("\n"));
  },

  toolCard(t) {
    const copy = (lines) => async () => {
      try { await invoke("copy_text", { text: lines.join(" && ") }); toast("Copied: paste it into Terminal"); }
      catch (e) { toast(String(e), true); }
    };
    const terminal = () => this.btn("Open Terminal", async () => { try { await invoke("open_terminal"); } catch (e) { toast(String(e), true); } });
    const card = el("div", { class: "tool" },
      el("div", { class: "head" },
        el("div", {}, el("div", { class: "title" }, t.name), el("div", { class: "sub" }, t.what)),
        el("span", { class: `state${t.installed ? " on" : ""}` }, t.installed ? "✓ Installed" : "Not installed")));
    // The command for what is missing; the other one tucked away.
    const main = t.installed ? ["Remove", t.remove] : ["Install", t.install];
    const other = t.installed ? ["Install", t.install] : ["Remove", t.remove];
    card.append(
      el("div", { class: "sub" }, `${main[0]} with Homebrew, in Terminal:`),
      this.cmdBlock(main[1]),
      el("div", { class: "actions" }, this.btn("Copy", copy(main[1])), terminal()),
      el("details", {}, el("summary", {}, `${other[0]} command`), this.cmdBlock(other[1]),
        el("div", { class: "actions" }, this.btn("Copy", copy(other[1])))));
    const note = (text, ...btns) => card.append(el("div", { class: "starter" }, el("span", {}, text), ...btns));
    if (t.starter === "available") {
      note("No config yet. Palette can add a small starter, themed with your colours; it never overwrites a config.",
        this.btn("Add a starter config", (b) => this.starter(t, b, "add_starter", "Adding…")));
    } else if (t.starter === "has_config") {
      note(`You already have a config (${t.config}), so Palette adds nothing. It only wires its colours in with Set up.`);
    } else if (t.starter === "added") {
      note("Palette's starter config is in place, unchanged. Removing it moves the files to the Trash.",
        this.btn("Remove starter config", (b) => this.starter(t, b, "remove_starter", "Moving…", true)));
    } else if (t.starter === "added_edited") {
      note(`Palette added a starter here, and you have edited it since, so it's yours now (${t.config}).`);
    }
    return card;
  },

  async starter(t, btn, command, busy, confirm = false) {
    if (confirm && !armed(btn, "Click again")) return toast(`Click again to move ${t.name}'s starter config to the Trash`);
    const label = btn.textContent;
    btn.disabled = true;
    btn.textContent = busy;
    try { toast(await invoke(command, { tool: t.id })); } catch (e) { toast(String(e), true); }
    btn.textContent = label;
    btn.disabled = false;
    this.renderTools();
  },
});
