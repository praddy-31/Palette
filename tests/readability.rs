//! Readability fixes (Phase 11a): a theme's colours are fixed when it's used, never in its
//! file. ash as Omarchy ships it has a selection the shade of its background, so Ghostty
//! showed no selection at all.

mod common;

use common::Home;

const ASH: &str = r##"accent = "#626262"
selection = "#0d0d0d"
muted = "#5a5a5a"
background = "#121212"
dark_background = "#0d0d0d"
darker_background = "#080808"
lighter_background = "#212121"
foreground = "#e0e0e0"
dark_foreground = "#adadad"
light_foreground = "#eaeaea"
bright_foreground = "#fafafa"
red = "#8a8a8a"
yellow = "#9e9e9e"
green = "#767676"
cyan = "#767676"
blue = "#626262"
magenta = "#8a8a8a"
bright_red = "#9e9e9e"
bright_yellow = "#b2b2b2"
bright_green = "#767676"
bright_cyan = "#9e9e9e"
bright_blue = "#626262"
bright_magenta = "#8a8a8a"
"##;

fn home_with_ash() -> Home {
    let home = Home::new();
    home.write(".config/palette/themes/ash/colors.toml", ASH);
    home
}

fn ghostty_selection(home: &Home) -> String {
    home.current_file("ghostty.conf")
        .lines()
        .find_map(|l| l.strip_prefix("selection-background = "))
        .unwrap_or_default()
        .to_string()
}

#[test]
fn ghostty_gets_a_visible_selection_and_the_theme_file_is_untouched() {
    let home = home_with_ash();
    home.ok(&["set", "ash"]);
    assert_eq!(ghostty_selection(&home), "#2a2a2a");
    assert!(
        home.read(".config/palette/themes/ash/colors.toml")
            .contains("#0d0d0d")
    );
    let out = home.ok(&["check", "ash"]);
    assert!(out.contains("selection #0d0d0d → #2a2a2a"), "{out}");
}

#[test]
fn original_colours_and_level_off_turn_it_off_and_re_render() {
    let home = home_with_ash();
    home.ok(&["set", "ash"]);
    let out = home.ok(&["check", "ash", "--original", "on"]);
    assert!(out.contains("original colours"), "{out}");
    assert_eq!(ghostty_selection(&home), "#0d0d0d");
    home.ok(&["check", "ash", "--original", "off"]);
    assert_eq!(ghostty_selection(&home), "#2a2a2a");
    home.ok(&["check", "--level", "off"]);
    assert_eq!(ghostty_selection(&home), "#0d0d0d");
    let file = home.read(".config/palette/settings.toml");
    assert!(file.contains("readability = \"off\""), "{file}");
}

#[test]
fn your_own_edit_wins_over_the_fix() {
    let home = home_with_ash();
    home.ok(&["colours", "ash", "--set", "selection=#101010"]);
    home.ok(&["set", "ash"]);
    assert_eq!(ghostty_selection(&home), "#101010");
}

/// Ghostty's terminal colour `n` (`palette = n=#rrggbb`).
fn ghostty_ansi(home: &Home, n: u8) -> String {
    let want = format!("palette = {n}=");
    home.current_file("ghostty.conf")
        .lines()
        .find_map(|l| l.strip_prefix(&want))
        .unwrap_or_default()
        .to_string()
}

#[test]
fn monochrome_greys_by_default_and_the_tint_on_request() {
    // ash's green and cyan are the same grey (#767676): spaced apart by default.
    let home = home_with_ash();
    home.ok(&["set", "ash"]);
    let (green, cyan) = (ghostty_ansi(&home, 2), ghostty_ansi(&home, 6));
    assert!(!green.is_empty() && green != cyan, "{green} {cyan}");
    let out = home.ok(&["check", "ash"]);
    assert!(out.contains("monochrome: spaced"), "{out}");

    // The tint re-renders the active theme and is saved; greys off puts them back.
    let out = home.ok(&["check", "--tint", "on"]);
    assert!(out.contains("a faint tint"), "{out}");
    assert_ne!(ghostty_ansi(&home, 2), green);
    let file = home.read(".config/palette/settings.toml");
    assert!(file.contains("monochrome_tint = true"), "{file}");
    home.ok(&["check", "--tint", "off", "--greys", "off"]);
    assert_eq!(ghostty_ansi(&home, 2), ghostty_ansi(&home, 6));
}
