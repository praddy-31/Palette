//! Zed loads every `*.json` theme family in `~/.config/zed/themes/` and
//! watches that folder. So `init` copies Palette's file there as
//! `palette.json` and sets `"theme": "Palette"` (or "Palette Glass" /
//! "Palette Clear", the glass setting) in settings.json; each switch copies the new file over it and Zed restyles
//! live. A copy, not a link: Zed's watcher sees changes inside its own folder,
//! not to the target of a link that points elsewhere.
//!
//! settings.json is JSON with comments and `theme` may be a nested object
//! (`{"mode", "light", "dark"}`), so the whole value is swapped: the user's
//! own text (comments included) is what `init --remove` puts back.
//!
//! Glass (`palette zed`): the file always holds three themes with the same
//! colours, one per window kind (opaque, blurred, transparent). Zed applies a
//! window's blur only when its *settings* change (or a window opens), never on
//! a theme file reload, so changing glass re-points `theme` ([`select`]).
//! Strength is only colour alpha, which a file reload does pick up.

use std::fs;
use std::path::PathBuf;

use super::{BASE, Note, Output, Platforms, Target};
use crate::paths::Paths;
use crate::settings::Glass;
use crate::system::Outcome;
use crate::wiring::{Setting, Wiring};

pub const TARGET: Target = Target {
    name: "zed",
    output: Some(Output {
        file: "zed.json",
        template: include_str!("template.json"),
        overridable: true,
        finish: None,
    }),
    wiring: Wiring::Setting(Setting {
        configs,
        per_file: false,
        value,
        read,
        write,
        prepare: Some(prepare),
    }),
    installed: |paths| {
        super::has_app(paths, "Zed.app")
            || super::has_app(paths, "Zed Preview.app")
            || super::has_program("zed")
    },
    reload,
    unload: Some(|_| Outcome::Done("switches back by itself".into())),
    doctor,
    platforms: Platforms::Mac,
    ..BASE
};

/// The `theme` value while wired, as JSON text: one theme in every mode, as
/// Palette's own light/dark decides the look.
fn value(paths: &Paths) -> String {
    let glass = crate::settings::load(paths)
        .unwrap_or_default()
        .zed_glass_now();
    quoted(glass.theme_name())
}

fn quoted(name: &str) -> String {
    format!("\"{name}\"")
}

/// Is `theme` (as JSON text) one of Palette's three?
fn is_palette_value(text: &str) -> bool {
    Glass::ALL.iter().any(|g| text == quoted(g.theme_name()))
}

/// After the glass setting changed: point settings.json at the matching
/// theme, if it is set to one of Palette's (backed up first, like `init`).
/// Returns whether it changed the file.
pub fn select(paths: &Paths) -> anyhow::Result<bool> {
    let Some(path) = configs(paths).into_iter().find(|p| p.is_file()) else {
        return Ok(false);
    };
    let text = fs::read_to_string(&path)?;
    let want = value(paths);
    match read(&text) {
        Some(now) if is_palette_value(&now) && now != want => {
            // The file first: the switch before this skipped the copy (settings.json
            // still named the old kind, so Zed didn't count as wired), and pointing Zed
            // at a theme its copy lacks makes it fall back to its default.
            install(paths).map_err(anyhow::Error::msg)?;
            crate::wiring::change_in_place(paths, &path, &write(&text, Some(&want)))?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Zed's folder on macOS (`~/.config/zed`, not `$XDG_CONFIG_HOME`).
fn dir(paths: &Paths) -> PathBuf {
    paths.home().join(".config/zed")
}

fn configs(paths: &Paths) -> Vec<PathBuf> {
    vec![dir(paths).join("settings.json")]
}

/// `~/.config/zed/themes/palette.json`, the copy Zed loads.
fn theme_file(paths: &Paths) -> PathBuf {
    dir(paths).join("themes/palette.json")
}

/// A theme file Palette wrote (or the earlier `port_palette_to_zed.py`
/// stopgap, which used the same name and author), so safe to replace.
fn is_ours(text: &str) -> bool {
    text.contains("\"name\": \"Palette\"") && text.contains("\"author\": \"Palette")
}

/// Copy the generated theme into Zed's folder: temp file + rename inside the
/// folder, so Zed never reads half a file. Refuses to replace a user's own
/// `palette.json`.
fn install(paths: &Paths) -> Result<(), String> {
    let theme = fs::read(paths.current().join("zed.json"))
        .map_err(|e| format!("set a theme first ({e})"))?;
    let file = theme_file(paths);
    if let Ok(existing) = fs::read_to_string(&file)
        && !is_ours(&existing)
    {
        return Err(format!(
            "{} already exists and isn't Palette's; rename it, then run `palette init` again",
            file.display()
        ));
    }
    let folder = file.parent().ok_or("no themes folder")?;
    fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let temp = folder.join(".palette.json.partial");
    fs::write(&temp, theme).map_err(|e| e.to_string())?;
    fs::rename(&temp, &file).map_err(|e| e.to_string())
}

/// Before `theme` is set: put the theme file in place. After it is restored
/// (`remove`): delete it, if it is still Palette's.
fn prepare(paths: &Paths, remove: bool) -> Result<String, String> {
    let file = theme_file(paths);
    if remove {
        if fs::read_to_string(&file).is_ok_and(|t| is_ours(&t)) {
            fs::remove_file(&file).map_err(|e| e.to_string())?;
        }
        return Ok("removed themes/palette.json".into());
    }
    install(paths)?;
    Ok("copied Palette's theme to themes/palette.json".into())
}

fn reload(paths: &Paths) -> Outcome {
    match install(paths) {
        Ok(()) => Outcome::Done("updated (Zed restyles by itself)".into()),
        Err(e) => Outcome::Failed(e),
    }
}

fn doctor(paths: &Paths) -> Vec<Note> {
    // Picked in Zed's own theme picker: another of Palette's three. Palette then
    // doesn't count Zed as wired, so switches would stop reaching it.
    let named = configs(paths)
        .first()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|t| read(&t));
    if let Some(named) = named.filter(|n| is_palette_value(n) && *n != value(paths)) {
        let glass = crate::settings::load(paths)
            .unwrap_or_default()
            .zed_glass_now();
        return vec![Note::fix(format!(
            "Zed is set to {named} but Palette's glass is {}: run `palette zed {}` \
             (or pick that kind in Settings → Apps → Zed)",
            glass.as_str(),
            glass.as_str()
        ))];
    }
    let wired = crate::wiring::is_wired(paths, &TARGET);
    if wired && !fs::read_to_string(theme_file(paths)).is_ok_and(|t| is_ours(&t)) {
        return vec![Note::fix(
            "theme is set to Palette but themes/palette.json is missing: run `palette refresh`",
        )];
    }
    Vec::new()
}

// ---------------------------------------------------------------- settings.json

/// A small reader for JSON with comments (Zed's settings.json): just enough
/// to find a top-level key's value without touching anything else.
struct Scan<'a> {
    b: &'a [u8],
}

impl Scan<'_> {
    /// Skip spaces, `// …` and `/* … */`.
    fn blank(&self, mut i: usize) -> usize {
        loop {
            while i < self.b.len() && self.b[i].is_ascii_whitespace() {
                i += 1;
            }
            match self.b.get(i..i + 2) {
                Some(b"//") => {
                    while i < self.b.len() && self.b[i] != b'\n' {
                        i += 1;
                    }
                }
                Some(b"/*") => {
                    i += 2;
                    while i < self.b.len() && self.b.get(i..i + 2) != Some(b"*/") {
                        i += 1;
                    }
                    i = (i + 2).min(self.b.len());
                }
                _ => return i,
            }
        }
    }

    /// `i` at an opening quote → just past the closing one.
    fn string(&self, mut i: usize) -> Option<usize> {
        i += 1;
        while i < self.b.len() {
            match self.b[i] {
                b'\\' => i += 2,
                b'"' => return Some(i + 1),
                _ => i += 1,
            }
        }
        None
    }

    /// `i` at the start of a value → just past its end.
    fn value(&self, i: usize) -> Option<usize> {
        match *self.b.get(i)? {
            b'"' => self.string(i),
            b'{' | b'[' => {
                let mut depth = 0usize;
                let mut j = i;
                while j < self.b.len() {
                    match self.b[j] {
                        b'"' => {
                            j = self.string(j)?;
                            continue;
                        }
                        b'/' if matches!(self.b.get(j + 1), Some(b'/' | b'*')) => {
                            j = self.blank(j);
                            continue;
                        }
                        b'{' | b'[' => depth += 1,
                        b'}' | b']' => {
                            depth -= 1;
                            if depth == 0 {
                                return Some(j + 1);
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
                None
            }
            _ => {
                let mut j = i;
                while j < self.b.len() && !b",}]\n/".contains(&self.b[j]) {
                    j += 1;
                }
                let end = self.b[i..j]
                    .iter()
                    .rposition(|c| !c.is_ascii_whitespace())?;
                Some(i + end + 1)
            }
        }
    }
}

/// The top-level `"theme"` entry: (where its key starts, value start, value end).
fn theme_span(text: &str) -> Option<(usize, usize, usize)> {
    let s = Scan { b: text.as_bytes() };
    let mut i = s.blank(0);
    if s.b.get(i) != Some(&b'{') {
        return None;
    }
    i += 1;
    loop {
        i = s.blank(i);
        if s.b.get(i)? != &b'"' {
            return None;
        }
        let key_start = i;
        let key_end = s.string(i)?;
        i = s.blank(key_end);
        if s.b.get(i)? != &b':' {
            return None;
        }
        let start = s.blank(i + 1);
        let end = s.value(start)?;
        if &text[key_start..key_end] == "\"theme\"" {
            return Some((key_start, start, end));
        }
        i = s.blank(end);
        match s.b.get(i)? {
            b',' => i += 1,
            _ => return None,
        }
    }
}

fn read(text: &str) -> Option<String> {
    let (_, start, end) = theme_span(text)?;
    Some(text[start..end].to_string())
}

/// Swap the value (`Some`, JSON text), add the setting at the top, or
/// (`None`) delete the line Palette added.
fn write(text: &str, value: Option<&str>) -> String {
    match (value, theme_span(text)) {
        (Some(v), Some((_, start, end))) => format!("{}{v}{}", &text[..start], &text[end..]),
        (Some(v), None) => match text.find('{') {
            Some(brace) => {
                let rest = &text[brace + 1..];
                let next = &rest[Scan { b: rest.as_bytes() }.blank(0)..];
                let comma = if next.is_empty() || next.starts_with('}') {
                    ""
                } else {
                    ","
                };
                format!("{}\n    \"theme\": {v}{comma}{rest}", &text[..=brace])
            }
            None => format!("{{\n    \"theme\": {v}\n}}\n"),
        },
        (None, Some((key, _, end))) => {
            let line_start = text[..key].rfind('\n').map_or(0, |i| i + 1);
            let line_end = text[end..].find('\n').map_or(text.len(), |i| end + i + 1);
            format!("{}{}", &text[..line_start], &text[line_end..])
        }
        (None, None) => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETTINGS: &str = r#"{
    // Theme ---------- (a "theme": in a comment)
    "theme": {
        "mode": "dark", // follows macOS
        "light": "One Light",
        "dark": "Palette Glass",
    },
    "languages": { "Python": { "theme": "nested, not ours" } },
    "url": "http://x//y",
    "buffer_font_size": 14,
}
"#;

    #[test]
    fn swaps_a_nested_theme_and_puts_it_back_with_its_comments() {
        let before = read(SETTINGS).unwrap();
        assert!(before.starts_with('{') && before.contains("// follows macOS"));
        let wired = write(SETTINGS, Some("\"Palette\""));
        assert!(
            wired.contains("    \"theme\": \"Palette\",\n    \"languages\""),
            "{wired}"
        );
        assert_eq!(read(&wired).as_deref(), Some("\"Palette\""));
        assert_eq!(write(&wired, Some(&before)), SETTINGS);
    }

    #[test]
    fn plain_string_theme() {
        let text = "{\n  \"theme\": \"One Dark\" /* mine */,\n  \"x\": 1\n}";
        assert_eq!(read(text).as_deref(), Some("\"One Dark\""));
        let wired = write(text, Some("\"Palette\""));
        assert_eq!(
            wired,
            "{\n  \"theme\": \"Palette\" /* mine */,\n  \"x\": 1\n}"
        );
    }

    #[test]
    fn adds_and_deletes_a_missing_setting() {
        let src = "{\n    // mine\n    \"vim_mode\": false\n}\n";
        assert_eq!(read(src), None);
        let wired = write(src, Some("\"Palette\""));
        assert_eq!(read(&wired).as_deref(), Some("\"Palette\""));
        assert_eq!(write(&wired, None), src);

        let empty = write("{}", Some("\"Palette\""));
        assert_eq!(empty, "{\n    \"theme\": \"Palette\"}");
        assert_eq!(read(&empty).as_deref(), Some("\"Palette\""));
        assert_eq!(
            read(&write("{\n  // only a comment\n}", Some("\"Palette\""))).as_deref(),
            Some("\"Palette\"")
        );
    }

    #[test]
    fn a_nested_theme_key_is_not_the_setting() {
        let text = "{ \"languages\": { \"theme\": \"x\" }, \"a\": [1, {\"theme\": 2}] }";
        assert_eq!(read(text), None);
    }

    #[test]
    fn install_copies_replaces_its_own_and_refuses_others() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(tmp.path());
        assert!(install(&paths).unwrap_err().contains("set a theme first"));
        let gen_file = paths.current().join("zed.json");
        fs::create_dir_all(gen_file.parent().unwrap()).unwrap();
        let theme =
            |t: &str| format!("{{\n  \"name\": \"Palette\",\n  \"author\": \"Palette: {t}\"\n}}\n");
        fs::write(&gen_file, theme("one")).unwrap();
        install(&paths).unwrap();
        fs::write(&gen_file, theme("two")).unwrap();
        assert!(matches!(reload(&paths), Outcome::Done(_)));
        assert_eq!(
            fs::read_to_string(theme_file(&paths)).unwrap(),
            theme("two")
        );
        assert!(
            !theme_file(&paths)
                .with_file_name(".palette.json.partial")
                .exists()
        );

        fs::write(theme_file(&paths), "{\"name\": \"Mine\"}").unwrap();
        assert!(matches!(reload(&paths), Outcome::Failed(_)));
        assert_eq!(
            fs::read_to_string(theme_file(&paths)).unwrap(),
            "{\"name\": \"Mine\"}"
        );
        prepare(&paths, true).unwrap();
        assert!(
            theme_file(&paths).exists(),
            "a user's file is never deleted"
        );
    }

    #[test]
    fn recognises_its_own_file_and_the_stopgap() {
        let ours =
            "{\n  \"name\": \"Palette\",\n  \"author\": \"Palette: tokyo-night (generated)\"";
        let stopgap = "{\n  \"name\": \"Palette\",\n  \"author\": \"Palette (generated by port_palette_to_zed.py)\"";
        assert!(is_ours(ours) && is_ours(stopgap));
        assert!(!is_ours(
            "{\n  \"name\": \"My Theme\",\n  \"author\": \"me\""
        ));
    }
}
