//! WezTerm: every bundled theme renders a colour table with valid values,
//! a theme can't replace the Lua file, and `init` wires a fake wezterm.lua
//! once, before `return config`, and `--remove` restores it byte for byte.

mod common;

use std::fs;

use common::Home;

fn is_hex(v: &str) -> bool {
    v.len() == 7 && v.starts_with('#') && v[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Every `"#…"` string in the file.
fn colours(text: &str) -> Vec<&str> {
    text.split('"')
        .skip(1)
        .step_by(2)
        .filter(|s| s.starts_with('#'))
        .collect()
}

#[test]
fn every_bundled_theme_writes_a_valid_wezterm_file() {
    let home = Home::new();
    let mut themes: Vec<String> = fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/themes"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("colors.toml").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    themes.sort();
    for theme in &themes {
        home.ok(&["set", theme]);
        let text = home.current_file("wezterm.lua");
        let body: Vec<&str> = text.lines().filter(|l| !l.starts_with("--")).collect();
        assert_eq!(body.first(), Some(&"return {"), "{theme}");
        assert_eq!(body.last(), Some(&"}"), "{theme}");
        for key in [
            "foreground",
            "background",
            "selection_bg",
            "cursor_bg",
            "ansi",
            "brights",
            "tab_bar",
        ] {
            assert!(text.contains(&format!("  {key} = ")), "{theme}: no {key}");
        }
        let ansi = text
            .lines()
            .find(|l| l.trim_start().starts_with("ansi ="))
            .unwrap();
        assert_eq!(colours(ansi).len(), 8, "{theme}: {ansi}");
        let brights = text
            .lines()
            .find(|l| l.trim_start().starts_with("brights ="))
            .unwrap();
        assert_eq!(colours(brights).len(), 8, "{theme}: {brights}");
        for c in colours(&text) {
            assert!(is_hex(c), "{theme}: {c:?}");
        }
        assert_eq!(
            text.matches('{').count(),
            text.matches('}').count(),
            "{theme}"
        );
    }
}

#[test]
fn themes_cannot_replace_the_wezterm_file() {
    let home = Home::new();
    let dir = home.add_theme("sneaky", &common::bundled("tokyo-night"));
    fs::write(
        dir.join("wezterm.lua"),
        "os.execute('touch /tmp/x') return {}",
    )
    .unwrap();
    let err = home.fails(&["set", "sneaky"]);
    assert!(err.contains("themes cannot replace it"), "{err}");
}

const CONFIG: &str = "local wezterm = require 'wezterm'\n\
local config = wezterm.config_builder()\n\
config.color_scheme = 'Batman'\n\
config.font_size = 13\n\
\n\
return config\n";

#[test]
fn init_wires_wezterm_once_and_removes_cleanly() {
    let home = Home::new();
    let rc = home.write(".config/wezterm/wezterm.lua", CONFIG);
    home.ok(&["set", "tokyo-night"]);
    let out = home.ok(&["init"]);
    assert!(out.contains("wezterm"), "{out}");
    let wired = fs::read_to_string(&rc).unwrap();
    assert!(wired.contains("-- Added by palette init"), "{wired}");
    let line = wired
        .lines()
        .find(|l| l.starts_with("pcall(function() config.colors = dofile("))
        .expect("import line");
    assert!(line.contains("current/wezterm.lua"), "{line}");
    assert!(line.contains("add_to_config_reload_watch_list"), "{line}");
    // Before the return, after the user's colour scheme (so Palette wins).
    let at = |s: &str| wired.find(s).unwrap();
    assert!(at("color_scheme") < at("pcall(") && at("pcall(") < at("return config"));
    // A backup was taken.
    let backups = home.palette_dir().join("backup");
    assert!(fs::read_dir(&backups).unwrap().count() > 0);

    // Twice changes nothing.
    home.ok(&["init"]);
    assert_eq!(fs::read_to_string(&rc).unwrap(), wired);

    home.ok(&["init", "--remove"]);
    assert_eq!(fs::read_to_string(&rc).unwrap(), CONFIG);
}

#[test]
fn a_config_returning_a_table_is_left_to_the_user() {
    let home = Home::new();
    let text = "return {\n  font_size = 12,\n}\n";
    let rc = home.write(".wezterm.lua", text);
    home.ok(&["set", "tokyo-night"]);
    let out = home.ok(&["init"]);
    assert!(out.contains("do by hand"), "{out}");
    assert_eq!(fs::read_to_string(&rc).unwrap(), text);
}
