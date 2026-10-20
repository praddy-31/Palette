//! `palette desktop`: the bar, windows and borders pages, end to end with
//! the real binary in a fake home (nothing is reloaded there).

mod common;

use common::Home;

const RC: &str = include_str!("fixtures/sketchybarrc-owner");
const AEROSPACE: &str = include_str!("fixtures/aerospace-owner.toml");
const BORDERS: &str = "#!/bin/bash\noptions=(\n  style=round\n  width=6.0\n  hidpi=on\n)\nborders \"${options[@]}\"\n";

fn home() -> Home {
    let home = Home::new();
    home.write(".config/sketchybar/sketchybarrc", RC);
    home.write(".aerospace.toml", AEROSPACE);
    home.write(".config/borders/bordersrc", BORDERS);
    home.ok(&["set", "tokyo-night"]);
    home
}

#[test]
fn shows_without_writing_anything() {
    let home = home();
    let out = home.ok(&["desktop", "windows"]);
    assert!(out.contains("workspaces = 6"), "{out}");
    assert!(out.contains("doesn't manage AeroSpace yet"));
    assert_eq!(home.read(".aerospace.toml"), AEROSPACE);
    assert!(
        home.ok(&["desktop"])
            .contains("windows  AeroSpace     yours")
    );
}

#[test]
fn manage_change_and_give_back() {
    let home = home();
    // Set manages first: the original is kept.
    home.ok(&["desktop", "borders", "set", "width=3", "style=square"]);
    let rc = home.read(".config/borders/bordersrc");
    assert!(
        rc.contains("width=3.0") && rc.contains("style=square"),
        "{rc}"
    );
    assert!(
        home.ok(&["desktop"])
            .contains("borders  JankyBorders  managed by Palette")
    );
    assert!(
        home.fails(&["desktop", "borders", "set", "width=99"])
            .contains("width is 1 to 20")
    );
    assert!(
        home.fails(&["desktop", "borders", "set", "sparkle=1"])
            .contains("not a borders setting")
    );
    // The failed sets wrote nothing.
    assert_eq!(home.read(".config/borders/bordersrc"), rc);

    home.ok(&["desktop", "borders", "release"]);
    assert_eq!(home.read(".config/borders/bordersrc"), BORDERS);
    assert!(
        home.ok(&["desktop"])
            .contains("borders  JankyBorders  yours")
    );
}

#[test]
fn the_bar_is_built_and_counts_as_wired() {
    let home = home();
    home.ok(&["desktop", "bar", "manage"]);
    let rc = home.read(".config/sketchybar/sketchybarrc");
    assert!(rc.starts_with("#!/bin/bash\n# SketchyBar, built by Palette"));
    assert!(home.path(".config/palette/bar/stats.sh").is_file());
    // `palette doctor` still sees both of Palette's sketchybar lines.
    let out = home.run(&["doctor"]);
    let doctor = String::from_utf8_lossy(&out.stdout);
    for target in ["sketchybar ", "sketchybar-item "] {
        assert!(
            doctor
                .lines()
                .any(|l| l.trim_start().starts_with(target) && l.contains("wired (")),
            "{doctor}"
        );
    }
    // Workspace slots, filled at run time with whatever AeroSpace has, so a
    // change in Windows needs no new bar.
    assert!(
        rc.contains("for slot in 1 2 3 4 5 6 7 8 9 10 11 12; do"),
        "{rc}"
    );
    assert!(rc.contains("SLOTS=12"), "{rc}");
    home.ok(&["desktop", "bar", "release"]);
    assert_eq!(home.read(".config/sketchybar/sketchybarrc"), RC);
}

#[test]
fn backups_are_kept_and_trimmed() {
    let home = home();
    for width in 1..=13 {
        home.ok(&["desktop", "borders", "set", &format!("width={width}")]);
    }
    let out = home.ok(&["desktop", "borders", "backups"]);
    assert!(out.starts_with("original"), "{out}");
    assert_eq!(out.lines().count(), 1 + 5, "{out}");
    // Going back: 1 is how it was before the last change, and going back
    // is itself undone the same way.
    home.ok(&["desktop", "borders", "restore", "1"]);
    assert!(
        home.read(".config/borders/bordersrc")
            .contains("width=12.0")
    );
    home.ok(&["desktop", "borders", "restore", "1"]);
    assert!(
        home.read(".config/borders/bordersrc")
            .contains("width=13.0")
    );
    assert!(
        !home
            .run(&["desktop", "borders", "restore", "9"])
            .status
            .success()
    );
    // The oldest of the five: the backup made before restoring trims it,
    // so it must be read first (it used to fail with "No such file").
    let list = home.ok(&["desktop", "borders", "backups"]);
    let oldest = list
        .lines()
        .last()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_string();
    let saved = home.read(&format!(
        ".config/palette/desktop/backups/borders/{oldest}/.config/borders/bordersrc"
    ));
    let width = saved
        .lines()
        .find(|l| l.contains("width="))
        .unwrap()
        .trim()
        .to_string();
    home.ok(&["desktop", "borders", "restore", &oldest]);
    assert!(
        home.read(".config/borders/bordersrc").contains(&width),
        "{width}"
    );
}

#[test]
fn the_prompt_changes_key_by_key_and_comes_back() {
    const STARSHIP: &str = include_str!("fixtures/starship-owner.toml");
    let home = home();
    home.write(".config/starship.toml", STARSHIP);
    home.ok(&[
        "desktop",
        "prompt",
        "set",
        "shape=\"arrow\"",
        "versions=false",
    ]);
    let text = home.read(".config/starship.toml");
    assert!(text.contains("[\u{e0b0}](fg:foreground bg:blue)"), "{text}");
    assert!(text.contains("format = \"[ $symbol ]($style)\""), "{text}");
    assert!(text.contains("\"Documents\" = \"󰈙 \""), "the rest stays");
    home.ok(&["desktop", "prompt", "release"]);
    assert_eq!(home.read(".config/starship.toml"), STARSHIP);
}
