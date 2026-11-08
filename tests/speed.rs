//! Phase 14a: change one thing, reload one thing. A setting renders the active theme again in
//! place (only the app it is for), never uses up an Undo step, and Undo brings the earlier
//! theme back with today's settings.

mod common;

use std::collections::BTreeMap;

use common::Home;

/// Every file in `current/`, by name.
fn current_files(home: &Home) -> BTreeMap<String, Vec<u8>> {
    let dir = home.palette_dir().join("current");
    std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            (name, std::fs::read(e.path()).unwrap())
        })
        .collect()
}

/// The saved generations Undo steps through.
fn generations(home: &Home) -> usize {
    std::fs::read_dir(home.palette_dir().join("generations"))
        .unwrap()
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .count()
}

fn files_that_differ(
    before: &BTreeMap<String, Vec<u8>>,
    after: &BTreeMap<String, Vec<u8>>,
) -> Vec<String> {
    after
        .iter()
        .filter(|(name, bytes)| before.get(*name) != Some(*bytes))
        .map(|(name, _)| name.clone())
        .collect()
}

fn opacity(conf: &str) -> String {
    conf.lines()
        .find(|l| l.starts_with("background-opacity"))
        .unwrap_or_default()
        .to_string()
}

#[test]
fn a_glass_change_touches_only_that_apps_file_and_no_undo_step() {
    let home = Home::new();
    home.ok(&["set", "nord"]);
    home.ok(&["set", "tokyo-night"]);
    assert_eq!(generations(&home), 2);
    let before = current_files(&home);

    home.ok(&["glass", "ghostty", "clear", "--strength", "30"]);
    let after = current_files(&home);
    assert_eq!(
        files_that_differ(&before, &after),
        ["ghostty.conf"],
        "only Ghostty's file is written again"
    );
    assert_eq!(generations(&home), 2, "a setting is not an Undo step");

    // Undo: the earlier theme, with today's glass (owner, 2026-11-08), not nord's old file.
    let now = opacity(&home.current_file("ghostty.conf"));
    assert!(home.ok(&["undo"]).contains("nord"));
    assert_eq!(home.ok(&["current"]).trim(), "nord");
    assert_eq!(opacity(&home.current_file("ghostty.conf")), now);
}

#[test]
fn colour_edits_readability_and_refresh_dont_use_up_undo() {
    let home = Home::new();
    home.ok(&["set", "nord"]);
    home.ok(&["set", "tokyo-night"]);
    let before = current_files(&home);

    home.ok(&["colours", "--set", "accent=#ff8800"]);
    let edited = current_files(&home);
    assert!(
        files_that_differ(&before, &edited).len() > 5,
        "a colour reaches every app"
    );
    home.ok(&["check", "--level", "strong"]);
    home.ok(&["check", "--greys", "off"]);
    home.ok(&["obsidian", "--ring", "off"]);
    let out = home.ok(&["refresh"]);
    assert!(out.contains("unchanged"), "{out}");
    assert_eq!(generations(&home), 2);

    // Undo still reaches the theme before, two steps of settings later.
    home.ok(&["undo"]);
    assert_eq!(home.ok(&["current"]).trim(), "nord");
}

#[test]
fn switching_to_the_active_theme_rewrites_nothing() {
    let home = Home::new();
    home.ok(&["set", "nord"]);
    let before = current_files(&home);
    home.ok(&["refresh"]);
    assert!(files_that_differ(&before, &current_files(&home)).is_empty());
}

#[test]
fn only_apps_whose_file_changed_reload() {
    use palette::effects::needs_reload;
    let targets: Vec<_> = palette::targets::all().collect();
    let by = |name: &str| targets.iter().find(|t| t.name == name).unwrap();
    let changed = ["ghostty.conf"];
    assert!(needs_reload(by("ghostty"), Some(&changed)));
    assert!(!needs_reload(by("sketchybar"), Some(&changed)));
    assert!(
        !needs_reload(by("bat"), Some(&changed)),
        "no file of its own"
    );
    assert!(
        needs_reload(by("sketchybar"), None),
        "refresh reloads everything"
    );
    // The bar and its pill share one reload, but each follows its own file.
    assert!(needs_reload(
        by("sketchybar-item"),
        Some(&["sketchybar-item.sh"])
    ));
}

#[test]
fn every_glass_app_is_a_target_so_its_file_can_be_rendered_alone() {
    let names: Vec<&str> = palette::targets::all().map(|t| t.name).collect();
    for app in palette::settings::GLASS_APPS {
        assert!(names.contains(&app), "{app} is not a target name");
    }
}

/// Settings redraws on focus only when this fingerprint changed (14b): the same when nothing
/// happened, different after a setting, a colour edit, a switch or a new library picture.
#[test]
fn the_stamp_changes_only_when_palettes_files_do() {
    let home = Home::new();
    home.ok(&["set", "nord"]);
    let paths = palette::Paths::fake_home(home.dir.path());
    let stamp = || palette::state::stamp(&paths);
    let first = stamp();
    assert_eq!(stamp(), first, "nothing happened");

    let mut seen = vec![first];
    for args in [
        vec!["glass", "ghostty", "clear"],
        vec!["colours", "--set", "accent=#ff8800"],
        vec!["set", "tokyo-night"],
        vec!["fav", "add", "nord"],
    ] {
        home.ok(&args);
        let now = stamp();
        assert!(!seen.contains(&now), "{args:?} left the stamp as it was");
        seen.push(now);
    }
    let pic = home.write("pic.png", "png");
    home.ok(&["bg", "library", "add", pic.to_str().unwrap()]);
    assert!(!seen.contains(&stamp()), "a library picture");
}
