// Settings → Schedule: day and night themes.
"use strict";

// ---------------------------------------------------------------- schedule

const schedule = {
  s: null,

  async render() {
    try { this.s = await invoke("schedule_get"); } catch (e) { toast(String(e), true); return; }
    this.fillThemes();
    this.paint();
    await this.search($("#sc-q").value);
  },

  fillThemes() {
    for (const [id, key] of [["#sc-day", "day"], ["#sc-night", "night"]]) {
      const names = themes.filter((t) => !t.invalid).map((t) => t.name);
      $(id).replaceChildren(el("option", { value: "" }, "Choose a theme…"),
        ...names.map((n) => { const o = el("option", { value: n }, n); if (n === this.s[key]) o.selected = true; return o; }));
    }
  },

  paint() {
    const s = this.s;
    $("#sc-on").checked = s.on;
    $("#sc-mode").value = s.mode;
    $("#sc-sun").hidden = s.mode !== "sun";
    $("#sc-fixed").hidden = s.mode !== "fixed";
    $("#sc-day-at").value = s.day_at;
    $("#sc-night-at").value = s.night_at;
    $("#sc-lat").value = s.lat ?? "";
    $("#sc-lon").value = s.lon ?? "";
    $("#sc-place").textContent = s.place || "Not chosen yet";
    const sug = $("#sc-suggest");
    sug.hidden = !s.suggestion || (s.city === s.suggestion && s.lat == null);
    sug.textContent = s.suggestion ? `Use ${s.suggestion}` : "";
    sug.title = "Suggested from this Mac's time zone";
    $("#sc-status").textContent = s.on ? `On: ${s.phase === "night" ? "night" : "day"} theme now` : "Off";
    $("#sc-today").textContent = s.today || s.problem || "-";
    $("#sc-next").textContent = s.next || (s.problem ? "Choose a place or times first" : "None within a year");
  },

  async search(q) {
    const rows = await invoke("schedule_cities", { query: q });
    $("#sc-cities").replaceChildren(...rows.map((c) => {
      const o = el("option", { value: c.name }, `${c.name}, ${c.country}`);
      if (c.name === this.s?.city && this.s.lat == null) o.selected = true;
      return o;
    }));
  },

  /** Save everything on the page; the library checks it and switches if due. */
  async save(extra = {}) {
    const input = {
      on: $("#sc-on").checked,
      day: $("#sc-day").value,
      night: $("#sc-night").value,
      mode: $("#sc-mode").value,
      city: this.s.city,
      lat: this.s.lat,
      lon: this.s.lon,
      day_at: $("#sc-day-at").value || "07:00",
      night_at: $("#sc-night-at").value || "19:00",
      ...extra,
    };
    try {
      this.s = await invoke("schedule_save", { input });
      toast(input.on ? "Schedule saved" : "Schedule off");
    } catch (e) {
      toast(String(e), true);
      try { this.s = await invoke("schedule_get"); } catch { /* keep the old view */ }
    }
    this.paint();
    await load();
  },
};
