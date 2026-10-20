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

    // A real 1200×600 image: a 24-bit BMP is a 54-byte header plus pixels.
    let raw = home.path("big.bmp");
    std::fs::write(&raw, bmp(1200, 600)).unwrap();
    let big = home.path("big.png");
    let made = Command::new("sips")
        .args(["-s", "format", "png"])
        .arg(&raw)
        .arg("--out")
        .arg(&big)
        .output()
        .unwrap();
    assert!(made.status.success());
    home.ok(&["bg", "add", "--theme", "nord", big.to_str().unwrap()]);
    let theme = palette::theme::load(&paths, "nord").unwrap();
    let thumb = palette::thumbs::thumbnail(&paths, &theme).unwrap().unwrap();
    let info = Command::new("sips")
        .args(["-g", "pixelWidth", "-g", "format"])
        .arg(&thumb)
        .output()
        .unwrap();
    let info = String::from_utf8_lossy(&info.stdout);
    assert!(
        info.contains("pixelWidth: 480") && info.contains("jpeg"),
        "{info}"
    );
    // Second call hits the cache.
    assert_eq!(
        palette::thumbs::thumbnail(&paths, &theme).unwrap(),
        Some(thumb)
    );
}

/// A solid-colour 24-bit BMP (width × 3 must be a multiple of 4).
fn bmp(width: u32, height: u32) -> Vec<u8> {
    let pixels = width * height * 3;
    let mut out = Vec::new();
    out.extend(b"BM");
    out.extend((54 + pixels).to_le_bytes());
    out.extend([0u8; 4]);
    out.extend(54u32.to_le_bytes());
    out.extend(40u32.to_le_bytes());
    out.extend(width.to_le_bytes());
    out.extend(height.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(24u16.to_le_bytes());
    out.extend([0u8; 24]);
    out.extend(std::iter::repeat_n([0x46u8, 0x34, 0x2e], (width * height) as usize).flatten());
    out
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
