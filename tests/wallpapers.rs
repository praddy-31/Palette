//! Wallpaper choice: a default per theme, previous/next, and starting a
//! switch on a chosen one. (Fake homes only record the choice.)

mod common;

use common::Home;

/// nord with three wallpapers, active.
fn nord() -> Home {
    let home = Home::new();
    let imgs: Vec<String> = ["a.png", "b.png", "c.png"]
        .iter()
        .map(|n| home.write(n, "png").to_string_lossy().to_string())
        .collect();
    home.ok(&["set", "tokyo-night"]);
    let mut args = vec!["bg", "add", "--theme", "nord"];
    args.extend(imgs.iter().map(String::as_str));
    home.ok(&args);
    home
}

fn current(home: &Home) -> String {
    home.ok(&["bg", "list"])
        .lines()
        .find(|l| l.starts_with('*'))
        .map(|l| l.rsplit('/').next().unwrap().to_string())
        .unwrap_or_default()
}

#[test]
fn a_theme_starts_with_its_default() {
    let home = nord();
    home.ok(&["bg", "default", "b.png", "--theme", "nord"]);
    assert!(
        home.read(".config/palette/settings.toml")
            .contains("[wallpapers]\nnord = \"b.png\"")
    );
    home.ok(&["set", "nord"]);
    assert_eq!(current(&home), "b.png");
    assert!(home.ok(&["bg", "list"]).contains("*d "));

    // Cleared: no default marked (the desktop keeps b.png, it's still nord's).
    home.ok(&["bg", "default", "--clear", "--theme", "nord"]);
    assert!(!home.ok(&["bg", "list"]).contains("d "));
    assert!(!home.read(".config/palette/settings.toml").contains("b.png"));
    assert!(
        home.fails(&["bg", "default", "zzz.png", "--theme", "nord"])
            .contains("no wallpaper named")
    );
}

#[test]
fn prev_and_next_wrap_around() {
    let home = nord();
    home.ok(&["set", "nord"]);
    assert_eq!(current(&home), "a.png");
    home.ok(&["bg", "prev"]);
    assert_eq!(current(&home), "c.png");
    home.ok(&["bg", "next"]);
    assert_eq!(current(&home), "a.png");
}

#[test]
fn set_can_start_on_a_chosen_wallpaper() {
    let home = nord();
    home.ok(&["set", "nord", "--wallpaper", "c.png"]);
    assert_eq!(current(&home), "c.png");
    assert!(
        home.fails(&["set", "nord", "--wallpaper", "nope.png"])
            .contains("no wallpaper named")
    );
}

#[test]
fn wallpaper_off_leaves_it_alone() {
    let home = nord();
    home.ok(&["apps", "off", "wallpaper"]);
    home.ok(&["set", "nord"]);
    assert_eq!(current(&home), "", "no wallpaper chosen while off");
    home.ok(&["apps", "on", "wallpaper"]);
    home.ok(&["set", "tokyo-night"]);
    home.ok(&["set", "nord"]);
    assert_eq!(current(&home), "a.png");
}
