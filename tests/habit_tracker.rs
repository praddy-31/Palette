//! The Habit Tracker colours file: every bundled theme renders it with a plain
//! `#rrggbb` for every key the app reads (the app ignores anything else).

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

#[test]
fn every_bundled_theme_writes_a_valid_habit_tracker_file() {
    let home = Home::new();
    let mut themes: Vec<String> = fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/themes"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("colors.toml").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    themes.sort();
    assert!(themes.len() > 5);
    for theme in &themes {
        home.ok(&["set", theme]);
        let text = home.current_file("habit-tracker.json");
        assert!(
            text.trim_start().starts_with('{') && text.trim_end().ends_with('}'),
            "{theme}"
        );
        assert_eq!(value(&text, "theme"), Some(theme.as_str()), "{theme}");
        assert!(
            matches!(value(&text, "mode"), Some("dark") | Some("light")),
            "{theme}"
        );
        for key in KEYS {
            let v = value(&text, key).unwrap_or_else(|| panic!("{theme}: no {key}"));
            assert!(is_hex(v), "{theme}: {key} = {v:?}");
        }
    }
}

#[test]
fn themes_cannot_replace_the_habit_tracker_file() {
    let home = Home::new();
    let dir = home.add_theme("sneaky", &common::bundled("tokyo-night"));
    fs::write(
        dir.join("habit-tracker.json"),
        "{\"background\":\"url(x)\"}",
    )
    .unwrap();
    let err = home.fails(&["set", "sneaky"]);
    assert!(err.contains("themes cannot replace it"), "{err}");
}
