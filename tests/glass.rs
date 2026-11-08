//! Glass lift (ROADMAP 11g): text in see-through apps is lifted to read over the wallpaper,
//! only in those apps' files, and follows a wallpaper change.

mod common;

use common::Home;

/// A one-colour wallpaper in the theme's own folder.
fn wallpaper(home: &Home, theme: &str, name: &str, rgb: [u8; 3]) {
    let mut img = palette::pixels::Image::new(16, 16);
    img.rgb.iter_mut().for_each(|p| *p = rgb);
    let path = home.path(&format!(
        ".config/palette/themes/{theme}/backgrounds/{name}"
    ));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    palette::pixels::write(&path, &img).unwrap();
}

fn ansi(conf: &str, prefix: &str) -> String {
    conf.lines()
        .find_map(|l| l.strip_prefix(prefix))
        .unwrap_or_default()
        .trim()
        .to_string()
}

#[test]
fn see_through_text_is_lifted_over_a_bright_wallpaper_and_follows_it() {
    let home = Home::new();
    home.add_theme("night", &common::bundled("tokyo-night"));
    wallpaper(&home, "night", "a-white.png", [245, 245, 245]);
    wallpaper(&home, "night", "b-black.png", [5, 5, 5]);
    home.ok(&["glass", "ghostty", "blurred", "--strength", "40"]);
    home.ok(&["set", "night"]);

    // Over white at 60 % opacity the faint colours are lifted in Ghostty only.
    let out = home.ok(&["check"]);
    assert!(out.contains("Glass lift (Auto) in ghostty"), "{out}");
    assert!(!out.contains("kitty"), "kitty is solid: {out}");
    let lifted = ansi(&home.current_file("ghostty.conf"), "foreground = ");
    let solid = ansi(&home.current_file("kitty.conf"), "foreground ");
    assert_ne!(lifted, solid, "ghostty {lifted}, kitty {solid}");

    // A darker wallpaper: Ghostty's file follows without a theme switch.
    home.ok(&["bg", "next"]);
    let after = ansi(&home.current_file("ghostty.conf"), "foreground = ");
    assert_ne!(after, lifted, "the lift follows the wallpaper");

    // By hand (F2): off lifts nothing, 100 % lifts further than Auto.
    home.ok(&["bg", "next"]);
    let auto = home.current_file("ghostty.conf");
    let out = home.ok(&["check", "--glass-lift", "0"]);
    assert!(out.contains("Glass lift for night: off"), "{out}");
    assert!(!out.contains("Glass lift (off) in"), "{out}");
    assert_eq!(
        ansi(&home.current_file("ghostty.conf"), "foreground = "),
        solid,
        "off: Ghostty gets the solid theme's colours"
    );
    let out = home.ok(&["check", "--glass-lift", "100"]);
    assert!(out.contains("Glass lift (100 %) in ghostty"), "{out}");
    assert_ne!(
        home.current_file("ghostty.conf"),
        auto,
        "100 % differs from Auto"
    );
    home.ok(&["check", "--glass-lift", "auto"]);
    assert_eq!(home.current_file("ghostty.conf"), auto, "back to Auto");

    // Solid again: nothing lifted.
    home.ok(&["glass", "ghostty", "solid"]);
    let out = home.ok(&["check"]);
    assert!(!out.contains("ghostty"), "{out}");
}
