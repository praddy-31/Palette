//! 17b: display names (`palette rename`). The folder keeps its name; `set` takes either.

mod common;

use common::Home;

#[test]
fn a_theme_is_shown_and_found_by_its_new_name() {
    let home = Home::new();
    home.ok(&["set", "nord"]);
    home.ok(&["fav", "add", "nord"]);
    let out = home.ok(&["rename", "nord", "Arctic Blue"]);
    assert!(out.contains("nord is shown as \"Arctic Blue\""), "{out}");
    assert_eq!(home.ok(&["rename", "nord"]).trim(), "Arctic Blue");
    assert!(home.ok(&["list"]).contains("\"Arctic Blue\""));
    // The bar's theme pill shows it, written again at once (nord is active).
    let item = home.read(".config/palette/current/sketchybar-item.sh");
    assert!(item.contains("label=\"Arctic Blue\""), "{item}");
    home.ok(&["set", "tokyo-night"]);
    let out = home.ok(&["set", "arctic blue"]);
    assert!(out.contains("Switched to Arctic Blue (nord)"), "{out}");
    // Favourites keep the folder name.
    assert!(home.ok(&["fav"]).contains("nord"));
    home.ok(&["rename", "Arctic Blue", "--clear"]);
    assert!(!home.ok(&["list"]).contains("Arctic Blue"));
    let item = home.read(".config/palette/current/sketchybar-item.sh");
    assert!(item.contains("label=\"nord\""), "{item}");
}

#[test]
fn names_that_would_clash_or_break_the_bar_are_refused() {
    let home = Home::new();
    home.ok(&["rename", "nord", "Arctic"]);
    for bad in [
        "Tokyo-Night",
        "arctic",
        "a \"b\"",
        "$HOME",
        "a`b",
        "back\\slash",
    ] {
        home.fails(&["rename", "everforest", bad]);
    }
    home.fails(&["rename", "no-such-theme", "Name"]);
    home.fails(&["rename", "everforest", &"x".repeat(41)]);
    // Its own folder name gives the folder name back.
    home.ok(&["rename", "nord", "nord"]);
    assert!(
        !home
            .read(".config/palette/settings.toml")
            .contains("[names]")
    );
}
