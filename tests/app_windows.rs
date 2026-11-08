//! `palette ui --windows/--click/--status`: how the app keeps its windows and what a menu
//! bar click opens (Phase 9a/9b). The app itself is checked by hand; this covers the setting.

mod common;

use common::Home;

#[test]
fn windows_and_click_are_saved_and_shown() {
    let home = Home::new();
    let out = home.ok(&["ui", "--status"]);
    assert!(out.contains("Windows: ready"), "{out}");
    assert!(out.contains("Menu bar click: picker"), "{out}");

    let out = home.ok(&["ui", "--windows", "on-demand"]);
    assert!(out.contains("Windows: on-demand"), "{out}");
    assert!(out.contains("Menu bar click: picker"), "{out}");
    let out = home.ok(&["ui", "--click", "quick-switcher"]);
    assert!(out.contains("Windows: on-demand"), "{out}");
    assert!(out.contains("Menu bar click: quick-switcher"), "{out}");

    let file = home.read(".config/palette/settings.toml");
    assert!(file.contains("windows = \"on-demand\""), "{file}");
    assert!(file.contains("menu_click = \"quick-switcher\""), "{file}");
    // Other settings survive the change.
    home.ok(&["hotkey", "ctrl+cmd+p"]);
    let out = home.ok(&["ui", "--status"]);
    assert!(out.contains("Menu bar click: quick-switcher"), "{out}");
}

#[test]
fn everything_on_demand_and_closing_idle_settings() {
    let home = Home::new();
    let out = home.ok(&["ui", "--status"]);
    assert!(out.contains("Close idle Settings: off"), "{out}");
    let out = home.ok(&[
        "ui",
        "--windows",
        "all-on-demand",
        "--close-idle-settings",
        "on",
    ]);
    assert!(out.contains("Windows: all-on-demand"), "{out}");
    assert!(out.contains("Close idle Settings: on"), "{out}");
    let file = home.read(".config/palette/settings.toml");
    assert!(file.contains("windows = \"all-on-demand\""), "{file}");
    assert!(file.contains("close_idle_settings = true"), "{file}");
    let out = home.ok(&["ui", "--close-idle-settings", "off"]);
    assert!(out.contains("Windows: all-on-demand"), "{out}");
    assert!(out.contains("Close idle Settings: off"), "{out}");
    home.fails(&["ui", "--close-idle-settings", "maybe"]);
}

#[test]
fn unknown_values_are_refused() {
    let home = Home::new();
    home.fails(&["ui", "--windows", "sometimes"]);
    home.fails(&["ui", "--click", "dock"]);
    // A hand-edited file with a wrong value says what's allowed.
    home.write(".config/palette/settings.toml", "windows = \"lazy-ish\"\n");
    let err = home.fails(&["ui", "--status"]);
    assert!(err.contains("ready, on-demand or all-on-demand"), "{err}");
}

#[cfg(target_os = "macos")]
#[test]
fn the_bar_pill_opens_what_the_menu_bar_click_opens() {
    let home = Home::new();
    home.write(
        ".config/sketchybar/sketchybarrc",
        "sketchybar --bar height=32\n",
    );
    home.ok(&["set", "tokyo-night"]);
    let item = home.current_file("sketchybar-item.sh");
    assert!(item.contains("open -g palette://click"), "{item}");
}
