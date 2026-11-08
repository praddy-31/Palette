// How themes look, drawn and searched (Phase 12). No dependencies: loaded before shared.js by
// every window, and embedded as-is in the inspection sheet (`palette catalog --sheet`,
// `src/sheet.rs`), so the app and the sheet draw the same desktops.
"use strict";

/** Search by look (12c): every word typed starts one of the theme's words (`tags`, from
 *  `palette::search::tags`: dark, warm, calm, blue…) or is part of its name (or `extra`,
 *  e.g. a catalogue title). The same rule as `palette list <words>`. */
function findTheme(t, q, extra = "") {
  const name = `${t.name} ${extra}`.toLowerCase();
  return q.toLowerCase().split(/\s+/).filter(Boolean)
    .every((w) => name.includes(w) || (t.tags || []).some((x) => x.startsWith(w)));
}

/** A theme drawn as a tiny desktop (12a): its bar, an editor and a terminal (with the
 *  window border and a selected line), from its colours alone, so it needs no image. An SVG
 *  in a 160×100 box that scales to any size. `wallpaper` (a data URL) goes behind it when
 *  given; otherwise the theme's own background, lit by its accent. */
function tinyDesktop(t, wallpaper = null) {
  const c = t.colors;
  const NS = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(NS, "svg");
  svg.setAttribute("viewBox", "0 0 160 100");
  svg.setAttribute("preserveAspectRatio", "xMidYMin slice"); // the bar always shows
  svg.setAttribute("class", "tiny");
  svg.setAttribute("aria-hidden", "true");
  const add = (tag, attrs, parent = svg) => {
    const n = document.createElementNS(NS, tag);
    for (const [k, v] of Object.entries(attrs)) n.setAttribute(k, v);
    parent.append(n);
    return n;
  };
  const id = `g${Math.random().toString(36).slice(2, 8)}`;
  const defs = add("defs", {});
  const lin = add("linearGradient", { id: `${id}l`, x1: 0, y1: 0, x2: 1, y2: 1 }, defs);
  add("stop", { offset: 0, "stop-color": c.background }, lin);
  add("stop", { offset: 1, "stop-color": c.darker_background || c.background }, lin);
  const rad = add("radialGradient", { id: `${id}r`, cx: 0.78, cy: 0.25, r: 0.55 }, defs);
  add("stop", { offset: 0, "stop-color": c.accent, "stop-opacity": 0.45 }, rad);
  add("stop", { offset: 1, "stop-color": c.accent, "stop-opacity": 0 }, rad);
  add("rect", { width: 160, height: 100, fill: `url(#${id}l)` });
  add("rect", { width: 160, height: 100, fill: `url(#${id}r)` });
  const wall = add("image", { width: 160, height: 100, preserveAspectRatio: "xMidYMid slice" });
  svg.setWallpaper = (src) => { if (src) wall.setAttribute("href", src); };
  svg.setWallpaper(wallpaper);
  const line = (x, y, w, fill, h = 2.6) => add("rect", { x, y, width: w, height: h, rx: h / 2, fill });

  // The bar: workspaces, a stats pill, the theme pill.
  const pill = (x, w) => add("rect", { x, y: 3, width: w, height: 8, rx: 4, fill: c.selection || c.dark_background, "fill-opacity": 0.92 });
  pill(4, 26);
  [0, 1, 2, 3].forEach((i) => add("circle", { cx: 9.5 + i * 5.2, cy: 7, r: 1.5, fill: i === 0 ? c.accent : c.dark_foreground }));
  pill(106, 50);
  add("circle", { cx: 112, cy: 7, r: 1.8, fill: c.accent });
  line(116, 5.7, 34, c.foreground);

  // The editor, behind: code in the theme's syntax colours.
  add("rect", { x: 8, y: 16, width: 88, height: 62, rx: 4, fill: c.background, stroke: c.dark_background || c.background, "stroke-width": 0.8 });
  add("rect", { x: 8, y: 16, width: 88, height: 7, rx: 4, fill: c.dark_background || c.background });
  const code = [[c.dark_foreground, 30], [[c.magenta, 12], [c.blue, 16], [c.foreground, 18]], [[c.magenta, 8], [c.foreground, 10], [c.orange, 6]],
    [[c.foreground, 18], [c.green, 22]], [[c.cyan, 14], [c.yellow, 20]], [c.foreground, 6]];
  code.forEach((row, i) => {
    let x = 13;
    for (const [fill, w] of Array.isArray(row[0]) ? row : [row]) { line(x, 27.5 + i * 6.5, w, fill); x += w + 2.5; }
  });

  // The terminal, in front, focused: the border's accent, a prompt, a selected line, the ANSI colours.
  add("rect", { x: 66, y: 38, width: 88, height: 56, rx: 4, fill: c.background, stroke: c.accent, "stroke-width": 1.2 });
  line(72, 45, 4, c.green); line(78, 45, 34, c.foreground);
  line(72, 51.5, 22, c.blue); line(97, 51.5, 26, c.foreground);
  add("rect", { x: 69.5, y: 56, width: 81, height: 6, rx: 1.5, fill: c.selection || c.dark_background });
  line(72, 57.7, 40, c.bright_foreground || c.foreground);
  line(72, 65, 4, c.red); line(78, 65, 28, c.foreground);
  line(72, 71.5, 4, c.green); line(78, 71.5, 4, c.muted || c.dark_foreground);
  ["red", "green", "yellow", "blue", "magenta", "cyan", "orange", "accent"].forEach((k, i) =>
    add("rect", { x: 72 + i * 9.5, y: 80, width: 8, height: 7, rx: 1.5, fill: c[k] || c.foreground }));
  return svg;
}
