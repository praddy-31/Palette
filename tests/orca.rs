//! Phase 27b: Orca's plugin files, rendered for every bundled theme. Orca refuses an app
//! theme without `--background` and `--foreground` (`orca plugin validate`, 2026-11-16),
//! so each half of `orca.json` must be JSON with the keys Orca reads.
#![cfg(target_os = "macos")]

mod common;

use common::Home;
use serde_json::Value;

const ANSI: [&str; 16] = [
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
    "brightBlack",
    "brightRed",
    "brightGreen",
    "brightYellow",
    "brightBlue",
    "brightMagenta",
    "brightCyan",
    "brightWhite",
];

fn is_colour(v: &Value) -> bool {
    v.as_str()
        .is_some_and(|s| s.starts_with('#') && matches!(s.len(), 7 | 9))
}

#[test]
fn every_bundled_theme_gives_orca_both_halves() {
    let home = Home::new();
    let list = home.ok(&["list"]);
    let themes: Vec<&str> = list
        .lines()
        .filter(|l| l.contains("bundled"))
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert!(themes.len() > 5, "{list}");
    for theme in themes {
        home.ok(&["set", theme]);
        let text = home.read(".config/palette/current/orca.json");
        let (app, terminal) = text
            .split_once("\n@@terminal@@\n")
            .unwrap_or_else(|| panic!("{theme}: no marker"));
        let app: Value = serde_json::from_str(&app.replace("@MODE@", "dark"))
            .unwrap_or_else(|e| panic!("{theme} app theme: {e}"));
        let terminal: Value = serde_json::from_str(&terminal.replace("@MODE@", "light"))
            .unwrap_or_else(|e| panic!("{theme} terminal theme: {e}"));

        assert_eq!(app["base"], "dark");
        for key in ["--background", "--foreground", "--primary", "--border"] {
            assert!(is_colour(&app["tokens"][key]), "{theme} {key}");
        }
        assert!(app["tokenColors"].as_array().is_some_and(|a| !a.is_empty()));
        assert_eq!(terminal["mode"], "light");
        for key in ANSI.iter().chain(&["foreground", "background", "cursor"]) {
            assert!(is_colour(&terminal["terminal"][key]), "{theme} {key}");
        }
    }
}
