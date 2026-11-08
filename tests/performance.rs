//! `palette performance`: every glass window solid, a plainer bar and borders, and your own
//! settings unchanged, so `off` puts each look back. End to end in a fake home.
#![cfg(target_os = "macos")]

mod common;

use common::Home;

const ZED: &str = ".config/zed/settings.json";
const BORDERS: &str = "#!/bin/bash\noptions=(\n  style=round\n  width=6.0\n  hidpi=on\n)\nborders \"${options[@]}\"\n";

fn home() -> Home {
    let home = Home::new();
    home.write(ZED, "{\n    \"theme\": \"One Dark\",\n}\n");
    home.write(".config/borders/bordersrc", BORDERS);
    home.write(
        ".config/sketchybar/sketchybarrc",
        include_str!("fixtures/sketchybarrc-owner"),
    );
    home.ok(&["set", "tokyo-night"]);
    home.ok(&["init"]);
    home
}

#[test]
fn one_switch_makes_everything_solid_and_off_puts_it_back() {
    let home = home();
    home.ok(&[
        "glass",
        "dispatch",
        "blurred",
        "--strength",
        "40",
        "--blur",
        "30",
    ]);
    home.ok(&["desktop", "borders", "set", "blur_radius=12"]);
    home.ok(&["desktop", "bar", "set", "blur_radius=20", "stats_every=3"]);
    let glass_before = home.current_file("dispatch.json");
    assert!(
        glass_before.contains("\"kind\": \"blurred\""),
        "{glass_before}"
    );
    let zed_before = home.read(ZED);
    assert!(zed_before.contains("\"Palette Glass\""), "{zed_before}");
    let borders_before = home.read(".config/borders/bordersrc");
    assert!(
        borders_before.contains("blur_radius=12.0"),
        "{borders_before}"
    );
    let bar_before = home.read(".config/sketchybar/sketchybarrc");
    assert!(bar_before.contains("blur_radius=20"), "{bar_before}");
    let backups = home.ok(&["desktop", "borders", "backups"]);

    let out = home.ok(&["performance", "on"]);
    assert!(out.contains("Performance mode: on"), "{out}");
    let glass = home.current_file("dispatch.json");
    assert!(glass.contains("\"kind\": \"solid\""), "{glass}");
    assert!(home.read(ZED).contains("\"theme\": \"Palette\","));
    let borders = home.read(".config/borders/bordersrc");
    assert!(!borders.contains("blur_radius"), "{borders}");
    assert!(borders.contains("hidpi=off"), "{borders}");
    let bar = home.read(".config/sketchybar/sketchybarrc");
    assert!(bar.contains("blur_radius=0"), "{bar}");
    assert!(
        bar.contains("update_freq=30"),
        "the stats pill slows down: {bar}"
    );
    // Your choices stay as they were; the mode is its own table.
    let toml = home.read(".config/palette/settings.toml");
    assert!(
        toml.contains("[glass.dispatch]\nkind = \"blurred\""),
        "{toml}"
    );
    assert!(toml.contains("[performance]\non = true"), "{toml}");
    assert!(home.ok(&["glass", "dispatch"]).contains("blurred"));
    // No backup for a change you didn't make by hand.
    assert_eq!(home.ok(&["desktop", "borders", "backups"]), backups);

    home.ok(&["performance", "off"]);
    assert_eq!(home.current_file("dispatch.json"), glass_before);
    assert_eq!(home.read(ZED), zed_before);
    assert_eq!(home.read(".config/borders/bordersrc"), borders_before);
    assert_eq!(home.read(".config/sketchybar/sketchybarrc"), bar_before);
    assert!(
        home.read(".config/palette/settings.toml")
            .contains("on = false")
    );
}

#[test]
fn each_part_can_be_left_out() {
    let home = home();
    home.ok(&["glass", "kitty", "clear"]);
    home.ok(&["desktop", "borders", "set", "blur_radius=12"]);
    let out = home.ok(&["performance", "--borders", "off", "--stats", "off"]);
    assert!(
        out.contains("off (glass on, bar on, borders off, stats off)"),
        "{out}"
    );
    home.ok(&["performance", "toggle"]);
    // Borders keep their glow; glass still goes solid.
    assert!(
        home.read(".config/borders/bordersrc")
            .contains("blur_radius=12.0")
    );
    assert!(home.read(ZED).contains("\"theme\": \"Palette\","));
    assert!(home.ok(&["performance"]).contains("Performance mode: on"));
    assert!(
        home.fails(&["performance", "--sparkle", "on"])
            .contains("sparkle")
    );
}

#[test]
fn the_weather_pill_uses_schedules_place() {
    let home = home();
    home.ok(&["desktop", "bar", "manage"]);
    let rc = home.read(".config/sketchybar/sketchybarrc");
    assert!(
        !rc.contains("LOCATION=\"51.5"),
        "no place in Schedule yet: {rc}"
    );
    // Setting Schedule's place rewrites the bar by itself.
    home.ok(&[
        "schedule",
        "on",
        "--day",
        "tokyo-night",
        "--night",
        "tokyo-night",
        "--lat",
        "51.5",
        "--lon",
        "-0.12",
    ]);
    let rc = home.read(".config/sketchybar/sketchybarrc");
    assert!(rc.contains("LOCATION=\"51.5000,-0.1200\""), "{rc}");
}
