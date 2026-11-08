//! Colour files for apps that read Palette's colours themselves (Dispatch, EasyPlots):
//! each has its own file, never another app's, with a plain `#rrggbb` for every key.

mod common;

use std::fs;

use common::Home;

const KEYS: &[&str] = &[
    "background",
    "dark_background",
    "lighter_background",
    "selection",
    "muted",
    "foreground",
    "dark_foreground",
    "light_foreground",
    "bright_foreground",
    "accent",
    "red",
    "orange",
    "yellow",
    "green",
    "cyan",
    "blue",
    "magenta",
];

fn value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let line = text
        .lines()
        .find(|l| l.trim_start().starts_with(&format!("\"{key}\":")))?;
    let v = line.split(':').nth(1)?.trim().trim_end_matches(',');
    Some(v.trim_matches('"'))
}

fn is_hex(v: &str) -> bool {
    v.len() == 7 && v.starts_with('#') && v[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Dispatch is Mac-only for now; EasyPlots runs on both systems.
fn files() -> Vec<&'static str> {
    if cfg!(target_os = "macos") {
        vec!["dispatch.json", "easyplots.json"]
    } else {
        vec!["easyplots.json"]
    }
}

#[test]
fn each_app_gets_its_own_valid_file() {
    let home = Home::new();
    for theme in ["tokyo-night", "catppuccin-latte"] {
        home.ok(&["set", theme]);
        for file in files() {
            let text = home.current_file(file);
            assert_eq!(value(&text, "theme"), Some(theme), "{file}");
            assert!(
                matches!(value(&text, "mode"), Some("dark" | "light")),
                "{file}"
            );
            for key in KEYS {
                let v = value(&text, key).unwrap_or_else(|| panic!("{file}: no {key}"));
                assert!(is_hex(v), "{file} {theme}: {key} = {v:?}");
            }
        }
    }
    // The apps' files don't depend on the Habit Tracker's being written.
    home.ok(&["apps", "off", "habit-tracker"]);
    home.ok(&["set", "tokyo-night"]);
    for file in files() {
        assert!(
            home.current_file(file)
                .contains("\"theme\": \"tokyo-night\"")
        );
    }
}

#[test]
fn themes_cannot_replace_the_app_files() {
    for file in files() {
        let home = Home::new();
        let dir = home.add_theme("sneaky", &common::bundled("tokyo-night"));
        fs::write(dir.join(file), "{\"background\":\"url(x)\"}").unwrap();
        let err = home.fails(&["set", "sneaky"]);
        assert!(err.contains("themes cannot replace it"), "{file}: {err}");
    }
}
