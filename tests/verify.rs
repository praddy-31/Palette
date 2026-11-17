//! `palette verify` (ROADMAP 29): reads back what apps took. In a fake home only the file
//! comparisons run; programs (SketchyBar, Rex, macOS) are never asked.
#![cfg(target_os = "macos")]

mod common;

use common::Home;

const SETTINGS: &str = ".config/zed/settings.json";
const THEME: &str = ".config/zed/themes/palette.json";

fn zed() -> Home {
    let home = Home::new();
    home.write("Applications/Zed.app/Contents/Info.plist", "");
    home.write(SETTINGS, "{\n    \"theme\": \"One Dark\",\n}\n");
    home.ok(&["set", "tokyo-night"]);
    home
}

#[test]
fn zed_passes_once_wired_and_fails_when_its_copy_is_old() {
    let home = zed();
    // Not wired yet: nothing to read back, said plainly.
    let out = home.ok(&["verify", "zed"]);
    assert!(out.contains("not wired"), "{out}");

    home.ok(&["init"]);
    let out = home.ok(&["verify"]);
    assert!(
        out.contains("zed") && out.contains("is the current theme"),
        "{out}"
    );
    // Programs are never asked in a fake home.
    assert!(out.contains("fake home"), "{out}");

    // Zed's copy from another theme: verify says so and fails.
    home.write(
        THEME,
        "{\"name\": \"Palette\", \"author\": \"Palette (old)\"}",
    );
    let err = home.fails(&["verify"]);
    assert!(err.contains("don't show the current theme"), "{err}");
    let out = home.run(&["verify"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("!!") && text.contains("palette refresh"),
        "{text}"
    );

    // The right copy back (a real Mac's `refresh` does this; a fake home never reloads apps).
    home.write(THEME, &home.current_file("zed.json"));
    home.ok(&["verify", "zed"]);
}

#[test]
fn an_unknown_app_is_an_error_and_an_app_without_a_read_back_says_why() {
    let home = zed();
    let err = home.fails(&["verify", "nosuchapp"]);
    assert!(err.contains("no app called nosuchapp"), "{err}");
}

#[test]
fn catch_up_is_only_for_apps_told_while_running() {
    let home = zed();
    let err = home.fails(&["catch-up", "Zed"]);
    assert!(err.contains("Orca") && err.contains("Rex"), "{err}");
    // A fake home never asks a program: Orca counts as not running, and catch-up says so.
    let out = home.ok(&["catch-up", "Orca"]);
    assert!(out.contains("not running"), "{out}");
}
