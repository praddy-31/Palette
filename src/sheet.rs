//! The theme inspection sheet (ROADMAP 12b): `palette catalog --sheet themes.html` writes one
//! offline page with every catalogue theme drawn as a tiny desktop, as its author made it
//! and as Palette uses it (Phase 11's fixes at your level), with what changed, how it works
//! in each kind of app, and search by look (12c). Self-contained: the drawing is
//! `app/ui/look.js` itself, embedded, and nothing is loaded from the network.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::normalise::Level;
use crate::paths::Paths;
use crate::ranking::Roles;
use crate::theme::Colors;

/// The drawing and the search rule, shared with the app.
const LOOK_JS: &str = include_str!("../app/ui/look.js");

/// A JSON string: quotes, backslashes and control characters escaped, and `</` broken up so
/// the data can't end the page's `<script>`.
fn js(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '/' => out.push_str("\\/"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn list(items: impl IntoIterator<Item = String>) -> String {
    format!("[{}]", items.into_iter().collect::<Vec<_>>().join(","))
}

fn colours(c: &Colors) -> String {
    format!(
        "{{{}}}",
        c.iter()
            .map(|(k, v)| format!("{}:{}", js(k), js(v)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// The page, for the catalogue at the level in your settings.
pub fn html(paths: &Paths) -> String {
    let settings = crate::settings::load(paths).unwrap_or_default();
    // Off would show nothing to compare: the sheet then shows Minimal's fixes.
    let level = match settings.readability {
        Level::Off => Level::Minimal,
        l => l,
    };
    let installed: BTreeSet<String> = crate::install::installed(paths)
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    let entries = crate::catalog::entries();
    let data = list(entries.iter().map(|e| {
        let mut after = e.colors.clone();
        let roles = if e.derived.is_some() {
            Roles::Guessed
        } else {
            Roles::Named
        };
        let changes = crate::normalise::apply_all(
            &mut after,
            level,
            settings.monochrome,
            &BTreeSet::new(),
            roles,
        );
        let apps = crate::check::app_verdicts(&after, level, &|_| None);
        format!(
            "{{name:{},title:{},author:{},url:{},installed:{},tags:{},before:{},after:{},changes:{},apps:{}}}",
            js(&e.name),
            js(&e.title),
            js(&e.author),
            js(&e.url),
            installed.contains(&e.name),
            list(crate::search::tags(&after).into_iter().map(js)),
            colours(&e.colors),
            colours(&after),
            list(changes.iter().map(|c| js(&c.summary()))),
            list(apps.iter().map(|a| format!(
                "{{app:{},grade:{},why:{}}}",
                js(a.app),
                js(a.grade),
                js(&a.why.join("; "))
            ))),
        )
    }));
    PAGE.replace("/*LOOK*/", LOOK_JS)
        .replace("/*DATA*/", &data)
        .replace("{{level}}", level.as_str())
        .replace("{{count}}", &entries.len().to_string())
}

/// `palette catalog --sheet <file>`: write the page; returns how many themes it shows.
pub fn write(paths: &Paths, out: &Path) -> Result<usize> {
    let page = html(paths);
    let tmp = out.with_extension("html.tmp");
    fs::write(&tmp, page).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, out).with_context(|| format!("writing {}", out.display()))?;
    Ok(crate::catalog::entries().len())
}

/// Settings → Get themes, "Inspection sheet": write it into Palette's cache folder and open
/// it in the browser. Returns where it is.
pub fn open(paths: &Paths) -> Result<std::path::PathBuf> {
    let dir = paths.root().join("cache");
    fs::create_dir_all(&dir)?;
    let file = dir.join("themes.html");
    write(paths, &file)?;
    crate::system::open_file(&file).map_err(anyhow::Error::msg)?;
    Ok(file)
}

const PAGE: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Palette themes</title>
<style>
  :root { --bg: #14151b; --card: #1c1d25; --fg: #e4e4ea; --dim: #9093a4; --line: #2c2e3a; --accent: #8fa8ff; }
  @media (prefers-color-scheme: light) { :root { --bg: #f4f4f6; --card: #fff; --fg: #1c1d25; --dim: #5d6070; --line: #dcdde3; --accent: #3b5bdb; } }
  * { box-sizing: border-box; }
  body { margin: 0; padding: 24px 16px 48px; background: var(--bg); color: var(--fg); font: 14px/1.45 -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
  header { max-width: 1200px; margin: 0 auto 16px; }
  h1 { font-size: 22px; margin: 0 0 4px; }
  .note { color: var(--dim); margin: 0 0 12px; }
  .bar { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
  input[type=search] { flex: 1; min-width: 200px; padding: 8px 10px; border-radius: 8px; border: 1px solid var(--line); background: var(--card); color: var(--fg); font: inherit; }
  label { color: var(--dim); font-size: 13px; display: flex; gap: 6px; align-items: center; }
  .chips { display: flex; flex-wrap: wrap; gap: 4px; margin: 8px 0 0; }
  .chip { border: 1px solid var(--line); background: none; color: var(--dim); border-radius: 999px; padding: 1px 10px; font-family: inherit; font-size: 12px; line-height: 20px; cursor: pointer; }
  .chip.on { background: var(--accent); color: var(--bg); border-color: var(--accent); }
  #count { color: var(--dim); font-size: 13px; }
  main { max-width: 1200px; margin: 0 auto; display: grid; grid-template-columns: repeat(auto-fill, minmax(340px, 1fr)); gap: 14px; }
  .t { background: var(--card); border: 1px solid var(--line); border-radius: 12px; padding: 12px; min-width: 0; }
  .t h2 { font-size: 15px; margin: 0; display: flex; gap: 8px; align-items: baseline; }
  .t h2 small { color: var(--dim); font-weight: 400; font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pair { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; margin: 10px 0 8px; }
  .pair figure { margin: 0; }
  .pair figcaption { color: var(--dim); font-size: 11.5px; margin-top: 3px; }
  .desk { position: relative; aspect-ratio: 16 / 10; border-radius: 8px; overflow: hidden; }
  .desk svg.tiny { position: absolute; inset: 0; width: 100%; height: 100%; display: block; }
  .tags { display: flex; flex-wrap: wrap; gap: 3px; margin: 0 0 6px; }
  .tags span { font-size: 11px; padding: 0 6px; border-radius: 4px; background: var(--line); color: var(--dim); }
  .apps { display: grid; gap: 2px; font-size: 12px; margin: 0 0 6px; }
  .apps b { display: inline-block; min-width: 38px; font-weight: 600; }
  .ok { color: #3fa55b; } .weak { color: #c99a22; } .bad { color: #e0555f; }
  details { font-size: 12px; color: var(--dim); }
  details li { margin: 2px 0; overflow-wrap: anywhere; }
  .same figure:last-child { opacity: .55; }
  @media (max-width: 420px) { main { grid-template-columns: 1fr; } }
</style>
</head>
<body>
<header>
  <h1>Palette themes</h1>
  <p class="note">{{count}} community themes, each as its author made it and as Palette uses it
    (readability fixes: {{level}}). Made offline by <code>palette catalog --sheet</code>.</p>
  <div class="bar">
    <input id="q" type="search" placeholder="Search: a name, or warm, dark, blue…" autocomplete="off" spellcheck="false">
    <label><input id="changed" type="checkbox"> only themes Palette changes</label>
    <label><input id="notok" type="checkbox"> only with a weak or bad app</label>
    <span id="count"></span>
  </div>
  <div class="chips" id="chips"></div>
</header>
<main id="grid"></main>
<script>
/*LOOK*/
const THEMES = /*DATA*/;
const $ = (s) => document.querySelector(s);
const h = (tag, attrs = {}, ...kids) => {
  const n = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) k === "class" ? (n.className = v) : n.setAttribute(k, v);
  for (const k of kids) if (k != null) n.append(k);
  return n;
};
// The same words under other names (search finds both; the cards show one).
const ALIASES = new Set(["colourful", "colorful", "muted", "monochrome", "cold", "gray", "contrast", "violet", "cyan", "soft"]);
const CHIPS = [["dark", "light"], ["warm", "cool"], ["calm", "vivid", "mono"],
  ["red", "orange", "yellow", "green", "teal", "blue", "purple", "pink"]];
function card(t) {
  const desk = (colors) => h("div", { class: "desk" }, tinyDesktop({ name: t.name, colors }));
  const same = !t.changes.length;
  return h("article", { class: `t${same ? " same" : ""}` },
    h("h2", {}, t.name, h("small", {}, `${t.title !== t.name ? t.title + " · " : ""}by ${t.author}${t.installed ? " · installed" : ""}`)),
    h("div", { class: "pair" },
      h("figure", {}, desk(t.before), h("figcaption", {}, "As made")),
      h("figure", {}, desk(t.after), h("figcaption", {}, same ? "Palette changes nothing" : `With Palette's fixes (${t.changes.length})`))),
    h("div", { class: "tags" }, ...t.tags.filter((x) => !ALIASES.has(x)).map((x) => h("span", {}, x))),
    h("div", { class: "apps" }, ...t.apps.map((a) => h("div", {}, h("b", { class: a.grade }, a.grade), ` ${a.app}${a.why ? ": " + a.why : ""}`))),
    same ? null : h("details", {}, h("summary", {}, "What Palette changes"), h("ul", {}, ...t.changes.map((c) => h("li", {}, c)))));
}
const cards = new Map(THEMES.map((t) => [t.name, card(t)]));
function render() {
  const q = $("#q").value;
  const shown = THEMES.filter((t) => findTheme(t, q, `${t.title} ${t.author}`) &&
    (!$("#changed").checked || t.changes.length) &&
    (!$("#notok").checked || t.apps.some((a) => a.grade !== "ok")));
  $("#grid").replaceChildren(...shown.map((t) => cards.get(t.name)));
  $("#count").textContent = `${shown.length} of ${THEMES.length}`;
  const words = q.toLowerCase().split(/\s+/);
  document.querySelectorAll(".chip").forEach((b) => b.classList.toggle("on", words.includes(b.textContent)));
}
for (const group of CHIPS) for (const w of group) {
  const b = h("button", { class: "chip", type: "button" }, w);
  b.onclick = () => {
    let now = $("#q").value.trim().toLowerCase().split(/\s+/).filter(Boolean);
    const on = now.includes(w);
    now = now.filter((x) => !(group.length < 8 ? group.includes(x) : x === w));
    if (!on) now.push(w);
    $("#q").value = now.join(" ");
    render();
  };
  $("#chips").append(b);
}
for (const id of ["#q", "#changed", "#notok"]) $(id).addEventListener("input", render);
render();
</script>
</body>
</html>
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_strings_cannot_break_the_page() {
        assert_eq!(js("a\"b\\c"), r#""a\"b\\c""#);
        assert_eq!(js("</script>"), r#""<\/script>""#);
        assert_eq!(js("x\ny"), r#""x\u000ay""#);
    }

    #[test]
    fn the_sheet_holds_every_catalogue_theme_and_the_drawing() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        let page = html(&paths);
        assert!(page.contains("function tinyDesktop"));
        assert!(page.contains("function findTheme"));
        assert!(!page.contains("/*DATA*/") && !page.contains("{{level}}"));
        for e in crate::catalog::entries() {
            assert!(
                page.contains(&format!("name:{}", js(&e.name))),
                "{}",
                e.name
            );
        }
        // Offline: no address to fetch anything from, other than the themes' own links.
        assert!(!page.contains("<link") && !page.contains("src=\"http"));
    }
}
