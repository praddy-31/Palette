//! Pin a theme to one app (ROADMAP 24): the app keeps its theme through switches, Undo
//! and removal of other themes; light/dark stays the main theme's.

mod common;

use common::{Home, bundled};

/// Each app's file for `theme` on its own, to compare a pinned app with.
fn files_for(theme: &str) -> (String, String) {
    let home = Home::new();
    home.ok(&["set", theme]);
    (
        home.current_file("ghostty.conf"),
        home.current_file("kitty.conf"),
    )
}

#[test]
fn a_pinned_app_keeps_its_theme_through_switches_and_undo() {
    let (ghostty_latte, _) = files_for("catppuccin-latte");
    let (_, kitty_nord) = files_for("nord");
    let (ghostty_tn, kitty_tn) = files_for("tokyo-night");

    let home = Home::new();
    home.ok(&["set", "tokyo-night"]);
    let out = home.ok(&["pin", "ghostty", "catppuccin-latte"]);
    assert!(out.contains("ghostty keeps"), "{out}");
    assert_eq!(home.current_file("ghostty.conf"), ghostty_latte);
    assert_eq!(home.current_file("kitty.conf"), kitty_tn);

    // A switch: kitty follows, Ghostty stays.
    home.ok(&["set", "nord"]);
    assert_eq!(home.current_file("ghostty.conf"), ghostty_latte);
    assert_eq!(home.current_file("kitty.conf"), kitty_nord);

    // Undo goes back a theme; the pin is a setting and stays.
    home.ok(&["undo"]);
    assert_eq!(home.current_file("ghostty.conf"), ghostty_latte);
    assert_eq!(home.current_file("kitty.conf"), kitty_tn);

    // Listed, and kept in settings.toml.
    let out = home.ok(&["pin"]);
    assert!(
        out.contains("ghostty") && out.contains("catppuccin-latte"),
        "{out}"
    );
    assert!(
        home.read(".config/palette/settings.toml")
            .contains("[pins]\nghostty = \"catppuccin-latte\"")
    );

    // Cleared: follows the main theme again, in place.
    let out = home.ok(&["pin", "ghostty", "--clear"]);
    assert!(out.contains("follows the main theme"), "{out}");
    assert_eq!(home.current_file("ghostty.conf"), ghostty_tn);
}

#[test]
fn removing_a_pinned_theme_frees_the_app_with_a_warning() {
    let home = Home::new();
    home.add_theme("mine", &bundled("everforest"));
    home.ok(&["set", "tokyo-night"]);
    let (ghostty_tn, _) = files_for("tokyo-night");
    home.ok(&["pin", "sketchybar", "mine"]);
    home.ok(&["pin", "ghostty", "mine"]);
    assert_ne!(home.current_file("ghostty.conf"), ghostty_tn);

    let out = home.run(&["remove", "mine"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("ghostty, sketchybar pinned to mine"), "{err}");
    assert_eq!(home.current_file("ghostty.conf"), ghostty_tn);
    assert!(home.ok(&["pin"]).contains("No pinned apps"));
}

#[test]
fn unknown_apps_and_themes_are_refused() {
    let home = Home::new();
    home.ok(&["set", "tokyo-night"]);
    assert!(
        home.fails(&["pin", "nosuchapp", "nord"])
            .contains("no app called nosuchapp")
    );
    assert!(
        home.fails(&["pin", "ghostty", "no-such-theme"])
            .contains("no-such-theme")
    );
    assert!(
        home.ok(&["pin", "ghostty"])
            .contains("follows the main theme")
    );
}
