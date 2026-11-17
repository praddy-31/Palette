//! Phase 21: herdr (terminal), Herdr (desktop app), micro and Rex, in a fake home:
//! `init` sets one value each (and copies the theme files apps load from their own
//! folders), a second `init` changes nothing, `--remove` gives the files back byte for
//! byte (micro: the setting out of the file `init` created).
#![cfg(target_os = "macos")]

mod common;

use common::Home;

const HERDR: &str = ".config/herdr/config.toml";
const HERDR_MINE: &str = "# mine\nonboarding = false\n";
const APP: &str = ".config/herdr-app/settings.json";
const APP_MINE: &str = "{\n  \"zoom\": 1,\n  \"appearance\": \"dark\"\n}\n";
const APP_THEME: &str = ".config/herdr-app/themes/palette.json";
const MICRO_DIR: &str = ".config/micro";
const MICRO: &str = ".config/micro/settings.json";
const MICRO_SCHEME: &str = ".config/micro/colorschemes/palette.micro";

fn home() -> Home {
    let home = Home::new();
    home.write(HERDR, HERDR_MINE);
    home.write(APP, APP_MINE);
    home.write(&format!("{MICRO_DIR}/bindings.json"), "{}");
    home.ok(&["set", "tokyo-night"]);
    home
}

#[test]
fn init_wires_each_with_one_value_and_remove_gives_it_back() {
    let home = home();
    assert!(!home.path(MICRO).exists());
    home.ok(&["init"]);

    let herdr = home.read(HERDR);
    assert!(herdr.starts_with(HERDR_MINE), "{herdr}");
    assert!(herdr.contains("[theme]\nname = \"terminal\""), "{herdr}");

    let app = home.read(APP);
    assert!(app.contains("\"theme\": \"palette\""), "{app}");
    assert!(app.contains("\"appearance\": \"dark\""), "{app}");
    assert_eq!(home.read(APP_THEME), home.current_file("herdr-app.json"));

    // micro had no settings.json: init made one with just the setting.
    assert_eq!(home.read(MICRO), "{\n    \"colorscheme\": \"palette\"\n}\n");
    assert_eq!(home.read(MICRO_SCHEME), home.current_file("micro.micro"));

    // Twice changes nothing.
    let again = home.ok(&["init"]);
    assert_eq!(home.read(HERDR), herdr);
    assert_eq!(home.read(APP), app);
    assert!(!again.contains("set herdr"), "{again}");

    // A switch updates the copies in the apps' folders from `reload`, which never runs
    // in a fake home (the modules' own tests call it). Here: the new colours are rendered.
    home.ok(&["set", "nord"]);
    assert!(
        home.current_file("herdr-app.json")
            .contains(&nord_background())
    );

    home.ok(&["init", "--remove"]);
    assert_eq!(home.read(HERDR), HERDR_MINE);
    assert_eq!(home.read(APP), APP_MINE);
    assert!(!home.path(APP_THEME).exists());
    assert!(!home.path(MICRO_SCHEME).exists());
    assert!(!home.read(MICRO).contains("colorscheme"));
}

fn nord_background() -> String {
    let colours = common::bundled("nord");
    let line = colours
        .lines()
        .find(|l| l.starts_with("background"))
        .unwrap();
    line.split('"').nth(1).unwrap().to_string()
}

#[test]
fn micro_is_left_alone_until_it_has_run_once() {
    let home = Home::new();
    home.ok(&["set", "tokyo-night"]);
    home.ok(&["init"]);
    assert!(!home.path(MICRO).exists());
    assert!(!home.path(MICRO_DIR).exists());
}

#[test]
fn rex_gets_its_colours_and_lists_as_applied_on_every_switch() {
    let home = home();
    let rex = home.current_file("rex.conf");
    assert!(rex.contains("palette = 15=#"), "{rex}");
    assert!(!rex.contains("opacity"), "colours only: {rex}");
    // Rex isn't installed in the fake home, so nothing is sent to it.
    let apps = home.ok(&["apps"]);
    assert!(apps.contains("rex"), "{apps}");
}

#[test]
fn a_theme_cannot_replace_rexs_file() {
    let home = Home::new();
    let dir = home.add_theme("odd", &common::bundled("tokyo-night"));
    std::fs::write(dir.join("rex.conf"), "command = /bin/sh\n").unwrap();
    let err = home.fails(&["set", "odd"]);
    assert!(err.contains("rex.conf"), "{err}");
}

const REX_THEMES: &str = "Applications/Rex Beta.app/Contents/Resources/\
SuperUI_SuperDesignTokens.bundle/Contents/Resources/themes.json";

/// Two themes in Rex's layout (the app's own file has ~40, with Display P3 copies).
fn rex_json() -> String {
    let ansi = |base: &str| {
        (0..16)
            .map(|i| format!("\"#{base}{i:02x}\""))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        r##"[{{"id": "deep-sea", "name": "Deep Sea", "appearance": "dark",
            "source": {{"background": "#06101a", "foreground": "#b8cdd9", "cursor": "#4fe3c1",
                        "selectionBackground": "#143349", "ansi": [{}], "paletteBase": {{}}}}}},
           {{"id": "paper-day", "name": "Paper Day", "appearance": "light",
            "source": {{"background": "#f8f4ec", "foreground": "#2b2b2b", "cursor": "#c0392b",
                        "selectionBackground": "#e3dccb", "ansi": [{}]}}}}]"##,
        ansi("8a40"),
        ansi("5a30")
    )
}

#[test]
fn rexs_own_themes_install_offline_from_the_app() {
    let home = Home::new();
    let out = home.ok(&["catalog"]);
    assert!(!out.contains("by Rex"), "no Rex, no Rex themes: {out}");
    assert!(
        home.fails(&["install", "rex"])
            .contains("Rex isn't installed")
    );

    home.write(REX_THEMES, &rex_json());
    let out = home.ok(&["catalog"]);
    assert!(out.contains("deep-sea") && out.contains("by Rex"), "{out}");

    home.ok(&["install", "deep-sea"]);
    let colours = home.read(".config/palette/themes/deep-sea/colors.toml");
    assert!(
        colours.contains("accent = \"#4fe3c1\""),
        "the cursor: {colours}"
    );
    assert!(colours.contains("selection = \"#143349\""), "{colours}");
    assert!(
        home.read(".config/palette/themes/deep-sea/.palette-source")
            .contains("rex:deep-sea")
    );

    // The rest at once; a second time installs nothing new.
    let out = home.ok(&["install", "rex"]);
    assert!(out.contains("1 of Rex's themes installed"), "{out}");
    assert!(
        home.path(".config/palette/themes/paper-day/light.mode")
            .is_file()
    );
    assert!(home.ok(&["install", "rex"]).contains("0 of Rex's themes"));

    // Update reads the app again; set works like any theme.
    home.ok(&["update", "deep-sea"]);
    home.ok(&["set", "paper-day"]);
}
