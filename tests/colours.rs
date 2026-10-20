//! `palette colours`: your colour edits on top of a theme, end to end with
//! the real binary in a fake home.

mod common;

use common::Home;

#[test]
fn edits_re_render_the_active_theme_and_reset() {
    let home = Home::new();
    home.ok(&["set", "tokyo-night"]);
    let out = home.ok(&["colours", "--set", "red=#FF0001"]);
    assert!(out.contains("tokyo-night: red = #ff0001"), "{out}");
    assert!(home.current_file("alacritty.toml").contains("ff0001"));
    let sheet = home.ok(&["colours"]);
    assert!(sheet.contains("* yours (was #f7768e)"), "{sheet}");
    // The built-in theme is still built in, with a folder holding the edits.
    assert!(
        home.palette_dir()
            .join("themes/tokyo-night/edits.toml")
            .is_file()
    );

    home.ok(&["colours", "--reset"]);
    assert!(!home.current_file("alacritty.toml").contains("ff0001"));
    assert!(!home.palette_dir().join("themes/tokyo-night").exists());
    assert!(
        home.ok(&["colours", "--reset"])
            .contains("nothing to change")
    );
}

#[test]
fn check_fix_applies_every_suggestion() {
    let home = Home::new();
    home.ok(&["set", "white"]);
    assert!(!home.run(&["check"]).status.success());
    let out = home.ok(&["check", "--fix"]);
    assert!(out.contains("dark_foreground"), "{out}");
    assert!(home.ok(&["check"]).contains("readable"));
    // Edits to a theme that isn't active are saved, not applied.
    home.ok(&["colours", "nord", "--set", "accent=#123456"]);
    assert!(home.ok(&["colours", "nord"]).contains("#123456  * yours"));
    assert!(
        home.fails(&["colours", "nord", "--set", "sparkle=#123456"])
            .contains("not a theme colour")
    );
    assert!(
        home.fails(&["colours", "nord", "--set", "accent"])
            .contains("KEY=#RRGGBB")
    );
}
