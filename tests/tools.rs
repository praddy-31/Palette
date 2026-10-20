//! `palette remove` for themes you made (Trash), and `palette tools`
//! (status, Homebrew commands, starter configs). All in a fake home: the
//! Trash is `<fake home>/.Trash`.

mod common;

use std::fs;

use common::{Home, bundled};

fn mine(home: &Home, name: &str) {
    home.add_theme(name, &bundled("nord"));
}

// ---------------------------------------------------------------- remove

#[test]
fn removing_a_theme_you_made_moves_it_to_the_trash_with_a_warning() {
    let home = Home::new();
    mine(&home, "test");
    home.write(".config/palette/themes/test/notes.txt", "keep me");
    let out = home.run(&["remove", "test"]);
    assert!(out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "test was made by you, not downloaded: moved to the Trash (restore it from there)"
        ),
        "{err}"
    );
    assert!(!home.palette_dir().join("themes/test").exists());
    assert_eq!(home.read(".Trash/test/notes.txt"), "keep me");

    // The name is taken in the Trash: a number is added, nothing is overwritten.
    mine(&home, "test");
    home.run(&["remove", "test"]);
    assert!(home.path(".Trash/test/colors.toml").is_file());
    assert!(home.path(".Trash/test 2/colors.toml").is_file());
}

#[test]
fn remove_refusals_say_what_is_wrong() {
    let home = Home::new();
    let err = home.fails(&["remove", "nope"]);
    assert!(err.contains("there is no theme called `nope`"), "{err}");
    assert!(!err.contains("os error"), "{err}");
    let err = home.fails(&["remove", "nord"]);
    assert!(err.contains("built into Palette"), "{err}");
    mine(&home, "mine");
    home.ok(&["set", "mine"]);
    let err = home.fails(&["remove", "mine"]);
    assert!(err.contains("active theme"), "{err}");
    assert!(home.palette_dir().join("themes/mine").is_dir());
    assert!(!home.path(".Trash").exists(), "nothing was moved");
}

// ---------------------------------------------------------------- tools

#[test]
fn tools_lists_the_homebrew_commands() {
    let home = Home::new();
    let out = home.ok(&["tools"]);
    for expected in [
        "The look",
        "Wallpaper recolourer",
        "brew install FelixKratz/formulae/sketchybar",
        "brew install FelixKratz/formulae/borders",
        "brew install --cask nikitabobko/tap/aerospace",
        "brew install gowall",
        "palette tools starter sketchybar",
        "Palette never runs Homebrew",
    ] {
        assert!(out.contains(expected), "missing `{expected}` in:\n{out}");
    }
}

#[test]
fn tools_status_knows_installed_from_not() {
    let home = Home::new();
    let status = palette::tools::status(&palette::Paths::fake_home(home.dir.path()));
    let ids: Vec<&str> = status.tools.iter().map(|t| t.id).collect();
    assert_eq!(ids, ["sketchybar", "borders", "aerospace", "gowall"]);
    let gowall = status.tools.iter().find(|t| t.id == "gowall").unwrap();
    assert_eq!(gowall.install, ["brew install gowall"]);
    assert_eq!(gowall.remove, ["brew uninstall gowall"]);
    assert_eq!(gowall.starter, palette::tools::Starter::None);
    let aero = status.tools.iter().find(|t| t.id == "aerospace").unwrap();
    assert_eq!(aero.starter, palette::tools::Starter::Available);
}

#[test]
fn a_sketchybar_starter_is_written_and_wired_into_an_empty_slot() {
    let home = Home::new();
    home.ok(&["set", "tokyo-night"]);
    let out = home.ok(&["tools", "starter", "sketchybar"]);
    assert!(out.contains("Added the sketchybar starter config"), "{out}");

    let rc = home.read(".config/sketchybar/sketchybarrc");
    assert!(
        rc.contains("sketchybar.sh"),
        "wired to Palette's colours:\n{rc}"
    );
    assert!(rc.contains("Added by palette init"), "{rc}");
    for plugin in ["space", "front_app", "clock", "battery"] {
        assert!(
            home.path(&format!(".config/sketchybar/plugins/{plugin}.sh"))
                .is_file()
        );
    }
    // The plugins that set colours are wired too (they run as separate scripts).
    assert!(
        home.read(".config/sketchybar/plugins/space.sh")
            .contains("sketchybar.sh")
    );
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(home.path(".config/sketchybar/sketchybarrc"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o111, 0o111, "executable");

    // A second time: already added.
    assert!(
        home.fails(&["tools", "starter", "sketchybar"])
            .contains("already added")
    );
}

#[test]
fn starters_refuse_when_any_config_exists() {
    let home = Home::new();
    home.write(".config/sketchybar/sketchybarrc", "mine\n");
    let err = home.fails(&["tools", "starter", "sketchybar"]);
    assert!(err.contains("already has a config"), "{err}");
    assert_eq!(home.read(".config/sketchybar/sketchybarrc"), "mine\n");
    assert!(!home.path(".config/sketchybar/plugins").exists());

    home.write(".aerospace.toml", "# mine\n");
    let err = home.fails(&["tools", "starter", "aerospace"]);
    assert!(err.contains("already has a config"), "{err}");
    assert!(!home.path(".config/aerospace").exists());

    home.write(".config/borders/anything", "x");
    assert!(
        home.fails(&["tools", "starter", "borders"])
            .contains("already has a config")
    );

    let err = home.fails(&["tools", "starter", "gowall"]);
    assert!(err.contains("no starter"), "{err}");
    let err = home.fails(&["tools", "starter", "nothing"]);
    assert!(err.contains("no tool named"), "{err}");
}

#[test]
fn the_borders_starter_is_wired_and_the_aerospace_one_is_plain() {
    let home = Home::new();
    home.ok(&["set", "nord"]);
    home.ok(&["tools", "starter", "borders"]);
    let rc = home.read(".config/borders/bordersrc");
    assert!(
        rc.contains("borders.sh") && rc.contains("ACTIVE_COLOR"),
        "{rc}"
    );
    home.ok(&["tools", "starter", "aerospace"]);
    let toml: toml::Table = home
        .read(".config/aerospace/aerospace.toml")
        .parse()
        .unwrap();
    assert!(
        toml["gaps"]["outer"]["top"].as_integer().unwrap() > 20,
        "room for the bar"
    );
    assert!(toml["mode"]["main"]["binding"]["alt-1"].is_str());
}

#[test]
fn a_starter_can_be_removed_to_the_trash_only_while_unchanged() {
    let home = Home::new();
    home.ok(&["set", "nord"]);
    home.ok(&["tools", "starter", "borders"]);
    // Wiring lines are Palette's own, so they don't count as a change.
    let out = home.ok(&["tools", "starter", "borders", "--remove"]);
    assert!(out.contains("Trash"), "{out}");
    assert!(!home.path(".config/borders").exists());
    assert!(home.path(".Trash/borders/bordersrc").is_file());
    // The slot is empty again.
    home.ok(&["tools", "starter", "borders"]);

    // Edited by the owner: now it is theirs.
    let rc = home.path(".config/borders/bordersrc");
    let text = fs::read_to_string(&rc).unwrap();
    fs::write(&rc, text.replace("width=6.0", "width=2.0")).unwrap();
    let err = home.fails(&["tools", "starter", "borders", "--remove"]);
    assert!(err.contains("has changed"), "{err}");
    assert!(rc.is_file());
    assert!(home.ok(&["tools"]).contains("since edited by you"));

    // An extra file in the folder counts too.
    let home = Home::new();
    home.ok(&["tools", "starter", "aerospace"]);
    home.write(".config/aerospace/extra.toml", "x");
    assert!(
        home.fails(&["tools", "starter", "aerospace", "--remove"])
            .contains("has changed")
    );
}
