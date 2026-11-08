//! Phase 4 core pieces: the SketchyBar item, `prev`, `hotkey`, thumbnails.

mod common;

use std::process::Command;

use common::Home;

const RC: &str = "#!/bin/bash\nPILL=0xf0292e42\nBLUE=0xff7aa2f7\n\n\
                  sketchybar --add item clock right\n\nsketchybar --update\n";

fn bar() -> Home {
    let home = Home::new();
    home.write(".config/sketchybar/sketchybarrc", RC);
    home.ok(&["set", "tokyo-night"]);
    home
}

#[test]
fn bar_item_and_colours_share_sketchybarrc() {
    let home = bar();
    home.ok(&["init"]);
    let rc = home.read(".config/sketchybar/sketchybarrc");
    // Colours right after the colour block, the item just before --update.
    let colours = rc.find("current/sketchybar.sh").unwrap();
    let item = rc.find("current/sketchybar-item.sh").unwrap();
    let update = rc.find("sketchybar --update").unwrap();
    assert!(colours < item && item < update, "{rc}");
    assert!(
        home.current_file("sketchybar-item.sh")
            .contains("label=\"tokyo-night\"")
    );

    // Both count as wired; a second init changes nothing.
    let out = home.ok(&["init"]);
    for target in ["sketchybar ", "sketchybar-item "] {
        let line = out
            .lines()
            .find(|l| l.trim_start().starts_with(target))
            .unwrap();
        assert!(line.contains("already wired"), "{out}");
    }
    assert!(!out.contains("backed up"), "{out}");

    home.ok(&["init", "--remove"]);
    assert_eq!(home.read(".config/sketchybar/sketchybarrc"), RC);
}

#[test]
fn bar_item_script_is_valid_bash() {
    let home = bar();
    let out = Command::new("bash")
        .arg("-n")
        .arg(home.palette_dir().join("current/sketchybar-item.sh"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn prev_wraps_around() {
    let home = Home::new();
    home.ok(&["prev"]);
    assert_eq!(home.ok(&["current"]).trim(), "white");
    home.ok(&["prev"]);
    assert_eq!(home.ok(&["current"]).trim(), "vantablack");
    home.ok(&["set", "catppuccin-latte"]);
    home.ok(&["prev"]);
    assert_eq!(home.ok(&["current"]).trim(), "white");
}

#[test]
fn hotkey_setting() {
    let home = Home::new();
    assert_eq!(home.ok(&["hotkey"]).trim(), "alt+shift+t");
    assert!(
        home.ok(&["hotkey", "Option-Shift-P"])
            .contains("alt+shift+p")
    );
    assert_eq!(home.ok(&["hotkey"]).trim(), "alt+shift+p");
    assert!(home.fails(&["hotkey", "p"]).contains("modifier"));
    assert!(
        home.read(".config/palette/settings.toml")
            .contains("hotkey = \"alt+shift+p\"")
    );
}

#[test]
fn thumbnails_are_cached_small_jpegs() {
    let home = Home::new();
    let paths = palette::Paths::fake_home(home.dir.path());
    let theme = palette::theme::load(&paths, "nord").unwrap();
    assert_eq!(palette::thumbs::thumbnail(&paths, &theme).unwrap(), None);

    // A real 1200×600 PNG, written with Palette's own image code (`sips` on
    // macOS, the image crate on Linux).
    let big = home.path("big.png");
    let mut picture = palette::pixels::Image::new(1200, 600);
    picture.rgb.iter_mut().for_each(|p| *p = [40, 80, 160]);
    palette::pixels::write(&big, &picture).unwrap();
    home.ok(&["bg", "add", "--theme", "nord", big.to_str().unwrap()]);
    let theme = palette::theme::load(&paths, "nord").unwrap();
    let thumb = palette::thumbs::thumbnail(&paths, &theme).unwrap().unwrap();
    let head = std::fs::read(&thumb).unwrap();
    assert_eq!(&head[..2], b"\xff\xd8", "a JPEG");
    let small = palette::pixels::read(&thumb, None).unwrap();
    assert_eq!((small.width, small.height), (480, 240));
    // Second call hits the cache.
    assert_eq!(
        palette::thumbs::thumbnail(&paths, &theme).unwrap(),
        Some(thumb)
    );
}

#[test]
fn bg_set_and_remove_by_name() {
    let home = Home::new();
    home.ok(&["set", "nord"]);
    let a = home.write("Pictures/a.png", "png");
    let b = home.write("Pictures/b.png", "png");
    home.ok(&["bg", "add", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert!(home.ok(&["bg", "set", "b.png"]).contains("b.png"));
    assert!(home.ok(&["bg", "list"]).contains("* "));
    assert!(
        home.fails(&["bg", "set", "../../etc/passwd"])
            .contains("no wallpaper named")
    );
    home.ok(&["bg", "remove", "a.png"]);
    assert!(!home.ok(&["bg", "list"]).contains("a.png"));
    assert!(a.is_file(), "the original stays");
}
