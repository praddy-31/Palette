//! Zed: theme file copied into its folder, one `theme` value swapped in
//! settings.json, glass on/off from Palette, and a clean way back.
#![cfg(target_os = "macos")]

mod common;

use common::Home;

const SETTINGS: &str = ".config/zed/settings.json";
const THEME: &str = ".config/zed/themes/palette.json";
const MINE: &str = "{\n    // Zed settings\n    \"theme\": {\n        \"mode\": \"dark\", // keep\n        \"light\": \"One Light\",\n        \"dark\": \"One Dark\",\n    },\n    \"buffer_font_size\": 14,\n}\n";

fn zed() -> Home {
    let home = Home::new();
    // Zed counts as installed in the fake home too (`~/Applications`), so `doctor` checks it on
    // a Mac without Zed (GitHub's runner) as on the owner's.
    home.write("Applications/Zed.app/Contents/Info.plist", "");
    home.write(SETTINGS, MINE);
    home.ok(&["set", "tokyo-night"]);
    home
}

#[test]
fn init_copies_the_theme_and_sets_one_value_then_gives_it_back() {
    let home = zed();
    let out = home.ok(&["init"]);
    assert!(out.contains("zed"), "{out}");
    let wired = home.read(SETTINGS);
    // Glass is blurred by default, so it points at that theme.
    assert!(
        wired.contains("    \"theme\": \"Palette Glass\",\n    \"buffer_font_size\": 14,"),
        "{wired}"
    );
    assert!(wired.starts_with("{\n    // Zed settings\n"), "{wired}");
    assert_eq!(home.read(THEME), home.current_file("zed.json"));

    // Another kind of glass re-points `theme` (Zed re-applies a window's blur
    // only when settings change); one line, nothing else moves.
    let out = home.ok(&["zed", "--glass", "clear"]);
    assert!(out.contains("clear, strength 50"), "{out}");
    assert_eq!(
        home.read(SETTINGS),
        wired.replace("Palette Glass", "Palette Clear")
    );
    home.ok(&["zed", "--glass", "off"]);
    assert_eq!(
        home.read(SETTINGS),
        wired.replace("\"Palette Glass\"", "\"Palette\"")
    );

    // Strength is only colour alpha: settings.json stays as it is.
    let before = home.read(SETTINGS);
    home.ok(&["zed", "--glass", "blurred", "--strength", "100"]);
    assert_eq!(home.read(SETTINGS), wired);
    home.ok(&["zed", "--strength", "20"]);
    assert_eq!(home.read(SETTINGS), wired);
    assert!(
        home.current_file("zed.json")
            .contains("\"background\": \"#13141ccc\"")
    );
    assert!(home.ok(&["zed"]).contains("blurred, strength 20"));
    let toml = home.read(".config/palette/settings.toml");
    assert!(
        toml.contains("[zed]\nglass = \"blurred\"\nstrength = 20"),
        "{toml}"
    );
    assert_ne!(before, wired);
    home.fails(&["zed", "--glass", "frosted"]);
    home.fails(&["zed", "--strength", "101"]);

    home.ok(&["init", "--remove"]);
    assert_eq!(home.read(SETTINGS), MINE);
    assert!(!home.path(THEME).exists());
}

/// The owner's bug (2026-11-01): Zed's folder still held round 1's file (one
/// theme, "Palette", blurred); picking Clear pointed Zed at "Palette Clear",
/// which that file lacked ("theme not found"), and Solid changed nothing.
#[test]
fn every_glass_kind_names_a_theme_that_is_in_zeds_copy() {
    let home = zed();
    home.ok(&["init"]);
    let round_one = "{\n  \"name\": \"Palette\",\n  \"author\": \"Palette: tokyo-night (generated)\",\n  \"themes\": [{\"name\": \"Palette\", \"appearance\": \"dark\", \"style\": {\"background.appearance\": \"blurred\"}}]\n}\n";
    home.write(THEME, round_one);
    for (kind, name, window) in [
        ("clear", "Palette Clear", "transparent"),
        ("off", "Palette", "opaque"),
        ("blurred", "Palette Glass", "blurred"),
    ] {
        home.ok(&["zed", "--glass", kind]);
        let settings = home.read(SETTINGS);
        assert!(
            settings.contains(&format!("\"theme\": \"{name}\"")),
            "{kind}: {settings}"
        );
        let copy: serde_json::Value = serde_json::from_str(&home.read(THEME)).unwrap();
        let theme = copy["themes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("{kind}: no {name} in Zed's copy"));
        assert_eq!(theme["style"]["background.appearance"], window, "{kind}");
    }
}

#[test]
fn doctor_names_a_kind_picked_in_zed_itself() {
    let home = zed();
    home.ok(&["init"]);
    let wired = home.read(SETTINGS);
    home.write(SETTINGS, &wired.replace("Palette Glass", "Palette Clear"));
    let out = String::from_utf8_lossy(&home.run(&["doctor"]).stdout).to_string();
    assert!(out.contains("palette zed blurred"), "{out}");
}

#[test]
fn glass_changes_leave_a_users_own_theme_alone() {
    let home = zed();
    // Not wired (no init): settings.json is never touched by `palette zed`.
    home.ok(&["zed", "--glass", "clear"]);
    assert_eq!(home.read(SETTINGS), MINE);
    // And init then picks the chosen kind.
    home.ok(&["init"]);
    assert!(home.read(SETTINGS).contains("\"theme\": \"Palette Clear\""));
}

#[test]
fn the_first_on_off_setting_still_reads() {
    let home = zed();
    home.write(".config/palette/settings.toml", "[zed]\nglass = false\n");
    assert!(home.ok(&["zed"]).contains("Zed glass: solid"));
}

#[test]
fn a_users_own_palette_json_is_never_replaced() {
    let home = zed();
    let own = "{ \"name\": \"My Palette\", \"author\": \"me\", \"themes\": [] }\n";
    home.write(THEME, own);
    let out = home.ok(&["init"]);
    assert!(out.contains("isn't Palette's"), "{out}");
    assert_eq!(home.read(THEME), own);
    assert_eq!(home.read(SETTINGS), MINE);
}

#[test]
fn the_old_stopgap_file_is_replaced() {
    let home = zed();
    home.write(
        THEME,
        "{\n  \"name\": \"Palette\",\n  \"author\": \"Palette (generated by port_palette_to_zed.py)\",\n  \"themes\": []\n}\n",
    );
    home.ok(&["init"]);
    assert_eq!(home.read(THEME), home.current_file("zed.json"));
}

/// `palette zed clear` reads like `palette glass ghostty clear` (14b); `--glass` still works,
/// and "solid" is a word for off.
#[test]
fn zed_takes_the_glass_kind_as_a_word_like_palette_glass() {
    let home = Home::new();
    let out = home.ok(&["zed", "clear", "--strength", "40"]);
    assert!(out.contains("clear, strength 40"), "{out}");
    assert!(home.ok(&["zed", "solid"]).contains("Zed glass: solid"));
    assert!(home.ok(&["zed", "--glass", "blurred"]).contains("blurred"));
    assert!(
        home.fails(&["zed", "clear", "--glass", "blurred"])
            .contains("cannot be used")
    );
}
