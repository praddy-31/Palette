//! End-to-end: run the real `palette` binary against a temporary fake home.

mod common;

use std::fs;

use common::{Home, bundled};

#[test]
fn fresh_install_lists_bundled_themes_and_has_no_current() {
    let home = Home::new();
    let list = home.ok(&["list"]);
    for name in ["catppuccin-mocha", "gruvbox-dark", "tokyo-night"] {
        assert!(list.contains(name), "{list}");
    }
    assert!(!list.contains('*'));
    assert!(home.ok(&["current"]).contains("none"));
}

#[test]
fn list_and_catalog_find_themes_by_look() {
    // 12c: words narrow the list; every word must match.
    let home = Home::new();
    let warm = home.ok(&["list", "warm", "dark"]);
    assert!(warm.contains("gruvbox-dark"), "{warm}");
    assert!(!warm.contains("tokyo-night"), "{warm}");
    assert!(!warm.contains("catppuccin-latte"), "{warm}");
    let light = home.ok(&["list", "light"]);
    assert!(
        light.contains("catppuccin-latte") && !light.contains("gruvbox-dark"),
        "{light}"
    );
    assert!(home.ok(&["list", "tokyo"]).contains("tokyo-night"));
    let cat = home.ok(&["catalog", "purple"]);
    assert!(
        cat.lines().filter(|l| l.contains(" by ")).count() > 3,
        "{cat}"
    );

    // 12b: the inspection sheet, one offline page.
    let file = home.path("themes.html");
    let out = home.ok(&["catalog", "--sheet", file.to_str().unwrap()]);
    assert!(out.contains("112 themes"), "{out}");
    let page = fs::read_to_string(&file).unwrap();
    assert!(page.contains("function tinyDesktop") && page.contains("purple-moon"));
}

#[test]
fn set_writes_files_and_marks_current() {
    let home = Home::new();
    let out = home.ok(&["set", "tokyo-night"]);
    assert!(out.contains("Switched to tokyo-night"), "{out}");
    assert!(out.contains("+ ghostty.conf"), "{out}");

    assert_eq!(home.ok(&["current"]).trim(), "tokyo-night");
    assert!(home.ok(&["list"]).contains("* tokyo-night"));
    assert!(
        home.current_file("borders.sh")
            .contains("ACTIVE_COLOR=0xff7aa2f7")
    );
}

#[test]
fn dry_run_writes_nothing_but_reports_changes() {
    let home = Home::new();
    let out = home.ok(&["set", "gruvbox-dark", "--dry-run"]);
    assert!(out.contains("dry run"), "{out}");
    assert!(!home.palette_dir().exists(), "dry run created files");

    home.ok(&["set", "gruvbox-dark"]);
    let out = home.ok(&["set", "gruvbox-dark", "--dry-run"]);
    assert!(out.contains("= kitty.conf"), "{out}");
    let out = home.ok(&["set", "tokyo-night", "--dry-run"]);
    assert!(out.contains("~ kitty.conf"), "{out}");
    assert_eq!(home.ok(&["current"]).trim(), "gruvbox-dark");
}

#[test]
fn next_cycles_alphabetically_and_wraps() {
    let home = Home::new();
    home.ok(&["next"]);
    assert_eq!(home.ok(&["current"]).trim(), "catppuccin-latte");
    home.ok(&["set", "vantablack"]);
    let seen: Vec<String> = (0..3)
        .map(|_| {
            home.ok(&["next"]);
            home.ok(&["current"]).trim().to_string()
        })
        .collect();
    assert_eq!(seen, ["white", "catppuccin-latte", "catppuccin-mocha"]);
}

#[test]
fn user_theme_shadows_bundled_and_light_mode_file_works() {
    let home = Home::new();
    let dir = home.add_theme(
        "tokyo-night",
        &bundled("tokyo-night").replace("#7aa2f7", "#8ab2f9"),
    );
    fs::write(dir.join("light.mode"), "").unwrap();
    home.ok(&["set", "tokyo-night"]);
    assert!(home.ok(&["list"]).contains("tokyo-night              user"));
    let sb = home.current_file("sketchybar.sh");
    assert!(sb.contains("ACCENT=0xff8ab2f9"), "{sb}");
    assert!(sb.contains("PALETTE_MODE=\"light\""), "{sb}");
}

#[test]
fn static_override_is_copied_verbatim() {
    let home = Home::new();
    let dir = home.add_theme("custom", &bundled("tokyo-night"));
    fs::write(dir.join("kitty.conf"), "# hand tuned\n").unwrap();
    let out = home.ok(&["set", "custom"]);
    assert!(
        out.contains("kitty.conf") && out.contains("from theme folder"),
        "{out}"
    );
    assert_eq!(home.current_file("kitty.conf"), "# hand tuned\n");
    assert!(home.current_file("ghostty.conf").contains("palette = 0="));
}

#[test]
fn invalid_theme_lists_every_problem_and_changes_nothing() {
    let home = Home::new();
    home.ok(&["set", "tokyo-night"]);
    let broken = bundled("tokyo-night")
        .replace("accent = \"#7aa2f7\"", "accent = \"blue\"")
        .replace("magenta = \"#ad8ee6\"\n", "");
    home.add_theme("broken", &broken);

    let err = home.fails(&["set", "broken"]);
    assert!(err.contains("theme `broken` is invalid"), "{err}");
    assert!(err.contains("`accent` = \"blue\" is not #RRGGBB"), "{err}");
    assert!(err.contains("missing key `magenta`"), "{err}");
    assert_eq!(home.ok(&["current"]).trim(), "tokyo-night");
}

#[test]
fn unknown_theme_and_unsafe_names_are_rejected() {
    let home = Home::new();
    assert!(
        home.fails(&["set", "nope"])
            .contains("no theme named `nope`")
    );
    assert!(
        home.fails(&["set", "../../etc"])
            .contains("invalid theme name")
    );
}

#[test]
fn unknown_keys_only_warn() {
    let home = Home::new();
    home.add_theme(
        "extra",
        &format!("{}\nsparkle = \"#ffffff\"\n", bundled("tokyo-night")),
    );
    let out = home.run(&["set", "extra"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("warning: unknown key `sparkle`"));
}

/// Every command shows up once in the "Commands by job" list under `--help`, so a new command
/// can't be left out of it (glass, performance and log were, until 2026-11-08).
#[test]
fn help_lists_every_command_by_job() {
    let home = common::Home::new();
    let help = home.ok(&["--help"]);
    let (commands, rest) = help.split_once("Options:").unwrap();
    let names: Vec<&str> = commands
        .lines()
        .skip_while(|l| !l.starts_with("Commands:"))
        .skip(1)
        .filter_map(|l| l.split_whitespace().next())
        .filter(|n| *n != "help")
        .collect();
    let by_job = rest.split_once("Commands by job:").unwrap().1;
    let listed: Vec<&str> = by_job
        .lines()
        .skip(1) // the rest of the heading's line
        .take_while(|l| !l.trim().is_empty())
        .filter_map(|l| l.get(19..))
        .flat_map(|l| l.split(", "))
        .map(str::trim)
        .collect();
    for name in &names {
        let times = listed.iter().filter(|n| *n == name).count();
        assert_eq!(
            times, 1,
            "`{name}` is listed {times} times under Commands by job"
        );
    }
    assert!(names.len() > 30, "{names:?}");
}

/// A search that finds nothing says so; piped output carries no colour codes (`catalog`
/// printed them anyway until 2026-11-08).
#[test]
fn empty_searches_say_so_and_pipes_get_plain_text() {
    let home = Home::new();
    let out = home.run(&["list", "purpel"]);
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("No theme matches `purpel`"));
    let out = home.run(&["catalog", "zzzz"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("No theme matches `zzzz`"));
    let catalog = home.ok(&["catalog"]);
    assert!(catalog.contains("aetheria"), "{catalog}");
    assert!(!catalog.contains('\x1b'), "colour codes in a pipe");
    assert!(!home.ok(&["list"]).contains('\x1b'));
}
