//! `palette desktop`: the bar, windows and borders pages, end to end with
//! the real binary in a fake home (nothing is reloaded there).

mod common;

use common::Home;

const RC: &str = include_str!("fixtures/sketchybarrc-owner");
const AEROSPACE: &str = include_str!("fixtures/aerospace-owner.toml");
const BORDERS: &str = "#!/bin/bash\noptions=(\n  style=round\n  width=6.0\n  hidpi=on\n)\nborders \"${options[@]}\"\n";

/// The overview line for one tool, minus the install part: "yours" or
/// "not installed" depends on what the machine running the tests has.
fn overview(home: &Home, tool: &str) -> String {
    let out = home.ok(&["desktop"]);
    let line = out
        .lines()
        .find(|l| l.split_whitespace().next() == Some(tool))
        .unwrap_or_else(|| panic!("no {tool} line in:\n{out}"));
    match line.contains("managed by Palette") {
        true => "managed".into(),
        false => "not managed".into(),
    }
}

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
    assert_eq!(overview(&home, "windows"), "not managed");
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
    assert_eq!(overview(&home, "borders"), "managed");
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
    assert_eq!(overview(&home, "borders"), "not managed");
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

#[test]
fn pills_move_from_the_command_line() {
    let home = home();
    home.ok(&["desktop", "bar", "manage"]);
    let pills = home.ok(&["desktop", "bar", "pills"]);
    assert!(pills.contains(" 1  left    workspaces"), "{pills}");
    home.ok(&["desktop", "bar", "move", "habits", "center"]);
    home.ok(&["desktop", "bar", "move", "palette", "--before", "habits"]);
    let pills = home.ok(&["desktop", "bar", "pills"]);
    let center: Vec<&str> = pills.lines().filter(|l| l.contains(" center ")).collect();
    assert_eq!(center.len(), 2, "{pills}");
    assert!(
        center[0].ends_with("palette") && center[1].ends_with("habits"),
        "{pills}"
    );
    // Written to the bar, and kept in the saved settings.
    let rc = home.read(".config/sketchybar/sketchybarrc");
    assert!(rc.contains("--add item habits center"), "{rc}");
    assert!(
        home.read(".config/palette/desktop/bar.toml")
            .contains("side = \"center\"")
    );
    assert!(
        home.fails(&["desktop", "bar", "move", "nope", "left"])
            .contains("no pill holds")
    );
    assert!(
        home.fails(&["desktop", "windows", "pills"])
            .contains("the bar's")
    );
}

/// Removing Palette from every config and setting it up again leaves a managed bar as the
/// Bar page writes it: the theme pill's line before the block that colours and places it
/// (owner, 2026-11-08: appended at the end, the pill came out grey and out of place).
#[test]
fn setting_up_again_keeps_the_managed_bar_in_order() {
    let home = home();
    home.ok(&["desktop", "bar", "manage"]);
    let built = home.read(".config/sketchybar/sketchybarrc");
    home.ok(&["init", "--remove"]);
    assert!(
        !home
            .read(".config/sketchybar/sketchybarrc")
            .contains("sketchybar-item.sh")
    );
    home.ok(&["init"]);
    let again = home.read(".config/sketchybar/sketchybarrc");
    let item = again
        .find("sketchybar-item.sh")
        .expect("the pill is wired again");
    let pill = again
        .find("--set palette_pill")
        .expect("the pill's own look");
    assert!(item < pill, "the pill's file must come first:\n{again}");
    assert_eq!(again, built, "the same file as the Bar page wrote");

    // One app at a time (Settings → Apps) too.
    home.ok(&["apps", "off", "sketchybar-item"]);
    home.ok(&["apps", "on", "sketchybar-item"]);
    assert_eq!(home.read(".config/sketchybar/sketchybarrc"), built);
}

/// A managed bar keeps its colours when Palette's lines are taken out (owner, 2026-11-08: the
/// workspace numbers, stats and pills turned invisible): the theme's colours are written in
/// the file before Palette's line, which overrides them while it's there.
#[test]
fn the_managed_bar_keeps_its_colours_without_palettes_line() {
    let home = home();
    home.ok(&["desktop", "bar", "manage"]);
    let rc = home.read(".config/sketchybar/sketchybarrc");
    let theme = home.current_file("sketchybar.sh");
    let fallback = rc.find("C_accent=").expect("a fallback for the accent");
    let palette_line = rc.find("current/sketchybar.sh").expect("Palette's line");
    assert!(
        fallback < palette_line,
        "Palette's colours must win while wired:\n{rc}"
    );
    let ours = rc.lines().find_map(|l| l.strip_prefix("C_accent="));
    let theirs = theme
        .lines()
        .find_map(|l| l.strip_prefix("export C_accent="));
    assert_eq!(ours, theirs, "the active theme's accent");
    // Every colour the file uses has a fallback.
    for (i, _) in rc.match_indices("$C_") {
        let name: String = rc[i + 1..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        assert!(rc.contains(&format!("\n{name}=")), "{name} has no fallback");
    }
    home.ok(&["init", "--remove"]);
    assert!(
        home.read(".config/sketchybar/sketchybarrc")
            .contains("C_accent=")
    );
}

/// "Match the menu bar": the bar as tall as the notch strip from the very top, pills the same,
/// text to suit; a Mac without a notch gets the standard 24 pt bar.
#[test]
fn matching_the_menu_bar_sizes_bar_pills_and_text() {
    let bar = palette::desktop::bar::Bar::default();
    let notch = palette::desktop::bar::matched_to_menu_bar(&bar, 32);
    assert_eq!(
        (notch.height, notch.y_offset, notch.pill.height),
        (32, 0, 32)
    );
    assert_eq!((notch.icon_size, notch.label_size), (13.5, 13.5));
    let plain = palette::desktop::bar::matched_to_menu_bar(&bar, 24);
    assert_eq!(
        (plain.height, plain.label_size),
        (24, 11.0),
        "text never below 11"
    );
}

/// The other displays' top gap follows the bar Palette manages (its reach plus the gap between
/// windows), also after the bar changes; with Follow off it's the number you typed.
#[test]
fn the_other_displays_gap_follows_the_bar() {
    let home = home();
    home.ok(&["desktop", "bar", "manage"]);
    home.ok(&[
        "desktop",
        "bar",
        "set",
        "position=top",
        "y_offset=4",
        "height=26",
    ]);
    home.ok(&["desktop", "windows", "manage"]);
    home.ok(&[
        "desktop",
        "windows",
        "set",
        "gaps.outer_top=4",
        "gaps.inner_vertical=8",
        "gaps.outer_top_other=99",
    ]);
    let top = || {
        home.read(".aerospace.toml")
            .lines()
            .find(|l| l.starts_with("gaps.outer.top"))
            .unwrap_or_default()
            .to_string()
    };
    assert!(
        top().contains("[{ monitor.\"built-in\" = 4 }, 34]"),
        "bar 4 + 26, then Top 4: {}",
        top()
    );
    home.ok(&["desktop", "bar", "set", "height=32", "y_offset=0"]);
    assert!(top().contains("}, 36]"), "follows a new bar: {}", top());
    // A new Top moves the other displays too (32 + 10).
    home.ok(&["desktop", "windows", "set", "gaps.outer_top=10"]);
    assert!(top().contains("= 10 }, 42]"), "follows Top: {}", top());
    home.ok(&["desktop", "windows", "set", "gaps.outer_top=4"]);
    home.ok(&["desktop", "windows", "set", "gaps.follow_bar=false"]);
    assert!(top().contains("}, 99]"), "your own number: {}", top());
    home.ok(&["desktop", "windows", "set", "gaps.outer_top_other=off"]);
    assert!(
        top().ends_with("= 4"),
        "one gap for every display: {}",
        top()
    );
    // Small bars are allowed (the page warns); below the floor is still an error.
    home.ok(&["desktop", "bar", "set", "height=10", "pill.height=6"]);
    assert!(
        home.fails(&["desktop", "bar", "set", "height=9"])
            .contains("height is 10 to 80")
    );
}
