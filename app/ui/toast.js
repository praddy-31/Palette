// The "Switched to X · Undo" toast window: shown by app/src/main.rs
// (`announce`) after a switch from the tray, the bar pill or the quick
// switcher, without taking focus. Hides itself after 5 s (paused on hover).
"use strict";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const $ = (sel) => document.querySelector(sel);

const SHOW_MS = 5000;
let timer;
let hovered = false;

function later(ms) {
  clearTimeout(timer);
  timer = setTimeout(() => { if (!hovered) invoke("hide"); }, ms);
}

function paint(c) {
  if (!c || !c.background) return;
  const root = document.documentElement.style;
  root.setProperty("--bg", c.background);
  root.setProperty("--bg-dark", c.dark_background);
  root.setProperty("--surface", c.lighter_background);
  root.setProperty("--fg", c.foreground);
  root.setProperty("--dim", c.dark_foreground);
  root.setProperty("--muted", c.muted);
  root.setProperty("--accent", c.accent);
  root.setProperty("--red", c.red);
}

function show(s) {
  paint(s.colors);
  const msg = $("#msg");
  const problem = s.problems.length ? s.problems.join(" · ") : null;
  msg.replaceChildren(s.undo ? "Back to " : "Switched to ", Object.assign(document.createElement("b"), { textContent: s.theme }));
  if (problem) msg.append(Object.assign(document.createElement("span"), { className: "sub", textContent: problem }));
  $("#note").classList.toggle("error", !!problem);
  $("#undo-btn").hidden = s.undo;
  $("#note").classList.add("show");
  later(s.undo ? 2500 : SHOW_MS);
}

$("#undo-btn").onclick = async () => {
  $("#undo-btn").disabled = true;
  try { await invoke("step", { how: "undo" }); } // announces "Back to …" itself
  catch (e) { $("#msg").textContent = String(e); later(SHOW_MS); }
  $("#undo-btn").disabled = false;
};
$("#note").addEventListener("mouseenter", () => { hovered = true; clearTimeout(timer); });
$("#note").addEventListener("mouseleave", () => { hovered = false; later(1500); });

listen("switched", (e) => show(e.payload));
