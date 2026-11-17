//! Phase 18 from the command line, in a fake home: ready pills and layouts (18b), saved bars
//! (18c), looks (S8), split/merge/copy (S7), a pill's own colour (S9), hiding in Performance
//! mode (S11) and day/night bars on the schedule (S10).

mod common;

use common::Home;

const RC: &str = include_str!("fixtures/sketchybarrc-owner");

fn home() -> Home {
    let home = Home::new();
    home.write(".config/sketchybar/sketchybarrc", RC);
    home.ok(&["set", "tokyo-night"]);
    home.ok(&["desktop", "bar", "apply"]); // Palette manages the bar from here
    home
}

fn bar(home: &Home) -> String {
    home.read(".config/palette/desktop/bar.toml")
}

fn rc(home: &Home) -> String {
    home.read(".config/sketchybar/sketchybarrc")
}

#[test]
fn presets_add_a_pill_and_layouts_replace_them() {
    let home = home();
    let list = home.ok(&["desktop", "bar", "presets"]);
    assert!(
        list.contains("info") && list.contains("minimal") && list.contains("notch"),
        "{list}"
    );
    // The owner's bar already has every Info item: nothing to add.
    let err = home.fails(&["desktop", "bar", "add", "info"]);
    assert!(
        err.contains("nothing to add") && err.contains("CPU (already on the bar)"),
        "{err}"
    );
    home.ok(&["desktop", "bar", "add", "minimal"]);
    let pills = home.ok(&["desktop", "bar", "pills"]);
    assert_eq!(
        pills.lines().filter(|l| l.starts_with(' ')).count(),
        3,
        "{pills}"
    );
    assert!(pills.contains("palette"), "the theme pill stays: {pills}");
    // Now Info has room: a new pill on the left, at that side's end.
    let out = home.ok(&["desktop", "bar", "add", "info", "--side", "left"]);
    assert!(!out.contains("left out"), "{out}");
    let pills = home.ok(&["desktop", "bar", "pills"]);
    assert!(pills.contains(" 2  left    cpu gpu temp"), "{pills}");
}

#[test]
fn a_look_changes_the_look_only() {
    let home = home();
    let before = home.ok(&["desktop", "bar", "pills"]);
    home.ok(&["desktop", "bar", "look", "solid"]);
    let b = bar(&home);
    assert!(b.contains("background = \"background\""), "{b}");
    assert_eq!(home.ok(&["desktop", "bar", "pills"]), before);
    assert!(
        home.fails(&["desktop", "bar", "look", "sparkly"])
            .contains("no look")
    );
}

#[test]
fn pills_split_merge_and_copy() {
    let home = home();
    home.ok(&["desktop", "bar", "split", "cpu", "4"]);
    let pills = home.ok(&["desktop", "bar", "pills"]);
    assert!(pills.contains(" 2  left    cpu gpu temp pwr\n"), "{pills}");
    assert!(pills.contains(" 3  left    mem disk vpn net"), "{pills}");
    home.ok(&["desktop", "bar", "merge", "cpu"]);
    assert!(
        home.ok(&["desktop", "bar", "pills"])
            .contains("cpu gpu temp pwr mem disk vpn net")
    );
    // A copy without the items the bar holds once is empty: refused.
    assert!(
        home.fails(&["desktop", "bar", "duplicate", "cpu"])
            .contains("only once")
    );
    assert!(
        home.fails(&["desktop", "bar", "split", "palette", "1"])
            .contains("theme pill")
    );
}

#[test]
fn a_pill_has_its_own_colour() {
    let home = home();
    home.ok(&[
        "desktop",
        "bar",
        "colour",
        "habits",
        "red",
        "--opacity",
        "80",
    ]);
    assert!(bar(&home).contains("colour = \"red\""));
    let rc = rc(&home);
    assert!(
        rc.contains("--set pill.right.") && rc.contains("background.color=${C_red/#0xff/0xcc}"),
        "{rc}"
    );
    home.ok(&["desktop", "bar", "colour", "habits", "none"]);
    assert!(rc_has(&home, "background.drawing=off"));
    home.ok(&[
        "desktop",
        "bar",
        "colour",
        "habits",
        "bar",
        "--opacity",
        "bar",
    ]);
    let b = bar(&home);
    assert!(
        !b.contains("colour = \"none\"") && !b.contains("opacity = 80"),
        "back to the bar's look: {b}"
    );
    assert!(
        home.fails(&["desktop", "bar", "colour", "habits", "sparkle"])
            .contains("not a theme colour")
    );
}

fn rc_has(home: &Home, s: &str) -> bool {
    rc(home).contains(s)
}

#[test]
fn hidden_items_go_only_in_performance_mode() {
    let home = home();
    home.ok(&["desktop", "bar", "hide", "weather", "on"]);
    assert!(
        home.ok(&["desktop", "bar", "pills"])
            .contains("weather (hidden in Performance mode)")
    );
    assert!(rc_has(&home, "--add item weather"));
    home.ok(&["performance", "on"]);
    assert!(
        !rc_has(&home, "--add item weather"),
        "gone while Slower bar is on"
    );
    assert!(rc_has(&home, "--add item clock"));
    home.ok(&["performance", "--stats", "off"]);
    assert!(
        rc_has(&home, "--add item weather"),
        "only the Slower bar part hides it"
    );
}

#[test]
fn saved_bars_switch_and_keep_their_changes() {
    let home = home();
    assert!(
        home.ok(&["desktop", "bar", "saved"])
            .contains("No saved bars yet")
    );
    home.ok(&["desktop", "bar", "save", "Work"]);
    assert!(
        home.fails(&["desktop", "bar", "save", "work"])
            .contains("already")
    );
    home.ok(&["desktop", "bar", "add", "minimal"]);
    // Apply (here: any change) goes into the bar in use.
    let work = home.read(".config/palette/desktop/bars/Work.toml");
    assert_eq!(work, bar(&home));
    home.ok(&["desktop", "bar", "save", "Minimal one"]);
    home.ok(&["desktop", "bar", "use", "work"]); // any case
    assert_eq!(
        home.ok(&["desktop", "bar", "saved"]),
        "  Minimal one\n* Work\n"
    );
    // Work was changed to the minimal layout before Minimal one was saved: they match.
    home.ok(&["desktop", "bar", "set", "height=33"]);
    assert!(
        home.read(".config/palette/desktop/bars/Work.toml")
            .contains("height = 33")
    );
    assert!(
        !home
            .read(".config/palette/desktop/bars/Minimal one.toml")
            .contains("height = 33")
    );
    home.ok(&["desktop", "bar", "rename-saved", "Work", "Desk"]);
    assert_eq!(
        home.ok(&["desktop", "bar", "saved"]),
        "* Desk\n  Minimal one\n"
    );
    home.ok(&["desktop", "bar", "delete-saved", "desk"]);
    assert_eq!(home.ok(&["desktop", "bar", "saved"]), "  Minimal one\n");
    assert!(
        home.fails(&["desktop", "bar", "save", "a/b"])
            .contains("letters, digits")
    );
    assert!(
        home.fails(&["desktop", "bar", "use", "nope"])
            .contains("no saved bar")
    );
}

#[test]
fn the_schedule_switches_the_bar_with_the_theme() {
    let home = home();
    home.ok(&["desktop", "bar", "save", "Day"]);
    home.ok(&["desktop", "bar", "add", "minimal"]);
    home.ok(&["desktop", "bar", "save", "Night"]);
    // 2026-06-01 12:00 at UTC+8 is day; 22:00 is night.
    let noon = "1780286400";
    let late = "1780322400";
    let env = |now: &str| {
        [
            ("PALETTE_NOW", now.to_string()),
            ("PALETTE_UTC_OFFSET", "480".into()),
        ]
    };
    let run = |args: &[&str], now: &str| {
        let e = env(now);
        let e: Vec<(&str, &str)> = e.iter().map(|(k, v)| (*k, v.as_str())).collect();
        home.ok_env(args, &e)
    };
    assert!(
        home.fails(&[
            "schedule",
            "on",
            "--day",
            "nord",
            "--night",
            "tokyo-night",
            "--at",
            "07:00,19:00",
            "--day-bar",
            "Nope"
        ])
        .contains("isn't a saved bar")
    );
    run(
        &[
            "schedule",
            "on",
            "--day",
            "nord",
            "--night",
            "tokyo-night",
            "--at",
            "07:00,19:00",
            "--day-bar",
            "Day",
            "--night-bar",
            "Night",
        ],
        noon,
    );
    assert!(home.ok(&["desktop", "bar", "saved"]).contains("* Day"));
    run(&["schedule", "run"], late);
    assert!(home.ok(&["desktop", "bar", "saved"]).contains("* Night"));
    // Chosen by hand: kept until the next boundary.
    home.ok(&["desktop", "bar", "use", "Day"]);
    run(&["schedule", "run"], late);
    assert!(home.ok(&["desktop", "bar", "saved"]).contains("* Day"));
    // Renaming a saved bar keeps the schedule pointing at it; deleting clears it.
    home.ok(&["desktop", "bar", "rename-saved", "Night", "Late"]);
    assert!(
        home.read(".config/palette/settings.toml")
            .contains("night_bar = \"Late\"")
    );
    home.ok(&["desktop", "bar", "delete-saved", "Late"]);
    assert!(
        home.read(".config/palette/settings.toml")
            .contains("night_bar = \"\"")
    );
}

#[test]
fn saved_bars_travel_with_export_but_not_which_is_in_use() {
    let home = home();
    home.ok(&["desktop", "bar", "save", "Work"]);
    let dir = home.path("export");
    home.ok(&["export", dir.to_str().unwrap()]);
    assert!(dir.join("palette/desktop/bars/Work.toml").is_file());
    assert!(!dir.join("palette/desktop/bars/.in-use").exists());
    let other = Home::new();
    other.ok(&["import", dir.to_str().unwrap()]);
    assert_eq!(other.ok(&["desktop", "bar", "saved"]), "  Work\n");
}

/// Save as with the page's unapplied changes puts them into the new bar only (owner, 2026-11-11:
/// both saved bars ended up with the same look).
#[test]
fn save_as_keeps_the_bar_in_use_as_it_was() {
    use palette::desktop::{
        self, Tool,
        bar::{Bar, presets, saved},
    };
    let home = home();
    home.ok(&["desktop", "bar", "save", "Work"]);
    let work = home.read(".config/palette/desktop/bars/Work.toml");
    let paths = palette::paths::Paths::fake_home(home.dir.path());
    let bar: Bar = desktop::get(&paths, Tool::Bar).unwrap().try_into().unwrap();
    let solid = presets::with_look(&bar, "solid", None).unwrap();
    saved::save_as(&paths, "Solid", Some(solid)).unwrap();
    assert_eq!(
        home.read(".config/palette/desktop/bars/Work.toml"),
        work,
        "Work untouched"
    );
    let solid = home.read(".config/palette/desktop/bars/Solid.toml");
    assert!(solid.contains("background = \"background\""), "{solid}");
    assert_eq!(
        solid,
        home.read(".config/palette/desktop/bar.toml"),
        "and applied"
    );
    assert_eq!(home.ok(&["desktop", "bar", "saved"]), "* Solid\n  Work\n");
}

#[test]
fn your_app_icons_go_in_bar_toml_and_icons_sh() {
    let home = home();
    let icons = ".config/palette/bar/icons.sh";
    assert!(!home.read(icons).contains(") icon_result"));

    home.ok(&["desktop", "bar", "icon", "Ollama", ":ollama:"]);
    home.ok(&["desktop", "bar", "icon", "Bob's App", "B"]);
    assert!(bar(&home).contains("[icons]"), "{}", bar(&home));
    let sh = home.read(icons);
    assert!(sh.contains("'Ollama') icon_result=':ollama:' ;;"), "{sh}");
    assert!(sh.contains(r"'Bob'\''s App') icon_result='B' ;;"), "{sh}");

    // Anything that could run in the shell is refused.
    let err = home.fails(&["desktop", "bar", "icon", "Ollama", "$(x)"]);
    assert!(err.contains("glyph name like"), "{err}");

    // Cleared: back to the map's icon.
    home.ok(&["desktop", "bar", "icon", "Ollama", "--clear"]);
    assert!(!home.read(icons).contains("Ollama"));
    assert!(home.read(icons).contains("Bob"));
}

#[test]
fn app_icons_need_a_managed_bar() {
    let home = Home::new();
    home.ok(&["set", "tokyo-night"]);
    let err = home.fails(&["desktop", "bar", "icon", "Ollama", ":ollama:"]);
    assert!(err.contains("manage"), "{err}");
}
