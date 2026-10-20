//! Wallpaper studio (phase 5d): recolour, generate, and themes from a picture.
//! Images go through macOS `sips`, so these run on a Mac only.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::Home;
use palette::pixels::{self, Image};

fn gowall_installed() -> bool {
    Command::new("which")
        .arg("gowall")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// A small picture: `top` over `bottom`, with a `sun` patch in the corner.
fn picture(path: &Path, top: [u8; 3], bottom: [u8; 3], sun: [u8; 3]) {
    let (w, h) = (40, 30);
    let mut img = Image::new(w, h);
    for y in 0..h {
        for x in 0..w {
            img.rgb[y * w + x] = if x > 28 && y < 10 {
                sun
            } else if y < h / 2 {
                top
            } else {
                bottom
            };
        }
    }
    pixels::write(path, &img).unwrap();
}

fn dusk(home: &Home, file: &str) -> PathBuf {
    let path = home.path(file);
    picture(&path, [30, 40, 90], [20, 25, 60], [240, 120, 160]);
    path
}

fn s(p: &Path) -> String {
    p.to_string_lossy().to_string()
}

fn bgs(home: &Home, theme: &str) -> PathBuf {
    home.palette_dir()
        .join("themes")
        .join(theme)
        .join("backgrounds")
}

#[test]
fn recolour_makes_a_copy_in_the_theme() {
    let home = Home::new();
    home.ok(&["set", "nord"]);
    home.ok(&["set", "nord"]);
    let png = dusk(&home, "sky.png");
    let before = std::fs::read(&png).unwrap();
    let out = home.ok(&["bg", "recolor", &s(&png), "--theme", "nord"]);
    assert!(out.contains("sky-nord.png"), "{out}");
    let made = bgs(&home, "nord").join("sky-nord.png");
    assert!(made.is_file());
    assert!(pixels::read(&made, None).is_ok(), "output is a real image");
    assert!(home.ok(&["bg", "list"]).contains("sky-nord.png"));
    assert_eq!(std::fs::read(&png).unwrap(), before, "original untouched");

    let err = home.fails(&["bg", "recolor", &s(&png), "--theme", "nord"]);
    assert!(err.contains("already exists"), "{err}");
}

#[test]
fn recolour_keeps_jpeg_as_jpeg() {
    let home = Home::new();
    let jpg = home.path("photo.jpg");
    picture(&jpg, [200, 180, 150], [90, 120, 60], [255, 250, 200]);
    home.ok(&["bg", "recolor", &s(&jpg), "--theme", "nord"]);
    assert!(bgs(&home, "nord").join("photo-nord.jpg").is_file());
}

#[test]
fn recolour_refuses_non_images() {
    let home = Home::new();
    let txt = home.write("notes.txt", "hello");
    let err = home.fails(&["bg", "recolor", &s(&txt), "--theme", "nord"]);
    assert!(err.contains("not an image"), "{err}");
    let err = home.fails(&[
        "bg",
        "recolor",
        &s(&home.path("missing.png")),
        "--theme",
        "nord",
    ]);
    assert!(err.contains("not an image"), "{err}");
    assert!(!bgs(&home, "nord").join("notes-nord.png").exists());
}

#[test]
fn recolour_apply_records_the_wallpaper() {
    let home = Home::new();
    let png = dusk(&home, "sky.png");
    let out = home.ok(&["bg", "recolor", &s(&png), "--theme", "nord", "--apply"]);
    assert!(out.contains("not applied: fake home"), "{out}");
    let state = home.read(".config/palette/wallpaper");
    assert!(state.contains("sky-nord.png"), "{state}");
}

#[test]
fn gowall_engine_needs_gowall() {
    if gowall_installed() {
        return;
    }
    let home = Home::new();
    let png = dusk(&home, "sky.png");
    let err = home.fails(&[
        "bg",
        "recolor",
        &s(&png),
        "--theme",
        "nord",
        "--engine",
        "gowall",
    ]);
    assert!(err.contains("gowall is not installed"), "{err}");
    let err = home.fails(&[
        "bg",
        "recolor",
        &s(&png),
        "--theme",
        "nord",
        "--engine",
        "zzz",
    ]);
    assert!(!err.is_empty());
}

#[test]
fn generate_writes_and_replaces() {
    let home = Home::new();
    home.ok(&["set", "nord"]);
    home.ok(&["set", "nord"]);
    for style in palette::studio::STYLES {
        let out = home.ok(&["bg", "generate", style, "--theme", "nord"]);
        let name = format!("palette-{style}.jpg");
        assert!(out.contains(&name), "{out}");
        let file = bgs(&home, "nord").join(&name);
        assert!(file.is_file());
        std::fs::write(&file, "stale").unwrap();
        home.ok(&["bg", "generate", style, "--theme", "nord"]);
        assert!(std::fs::metadata(&file).unwrap().len() > 1000, "replaced");
        assert!(home.ok(&["bg", "list"]).contains(&name));
    }
}

#[test]
fn generate_rejects_unknown_style() {
    let home = Home::new();
    let err = home.fails(&["bg", "generate", "zigzag", "--theme", "nord"]);
    assert!(!err.is_empty());
    assert!(!bgs(&home, "nord").join("palette-zigzag.jpg").exists());
}

#[test]
fn generate_seed_names_a_variant() {
    let home = Home::new();
    let out = home.ok(&["bg", "generate", "topo", "--theme", "nord", "--seed", "42"]);
    assert!(out.contains("palette-topo-42.jpg"), "{out}");
    home.ok(&["bg", "generate", "topo", "--theme", "nord"]);
    let dir = bgs(&home, "nord");
    assert!(dir.join("palette-topo-42.jpg").is_file());
    assert!(dir.join("palette-topo.jpg").is_file());
    assert_ne!(
        std::fs::read(dir.join("palette-topo-42.jpg")).unwrap(),
        std::fs::read(dir.join("palette-topo.jpg")).unwrap()
    );
    home.fails(&["bg", "generate", "topo", "--theme", "nord", "--seed", "x"]);
}

#[test]
fn recolour_strengths_sit_side_by_side() {
    let home = Home::new();
    let png = dusk(&home, "sky.png");
    for (strength, name) in [
        ("balanced", "sky-nord.png"),
        ("subtle", "sky-nord-subtle.png"),
        ("strong", "sky-nord-strong.png"),
    ] {
        let out = home.ok(&[
            "bg",
            "recolor",
            &s(&png),
            "--theme",
            "nord",
            "--strength",
            strength,
        ]);
        assert!(out.contains(name), "{out}");
    }
    let err = home.fails(&[
        "bg",
        "recolor",
        &s(&png),
        "--theme",
        "nord",
        "--strength",
        "loud",
    ]);
    assert!(err.contains("strength"), "{err}");
}

#[test]
fn gowall_after_palette_does_not_clash() {
    if !gowall_installed() {
        return;
    }
    let home = Home::new();
    let png = dusk(&home, "sky.png");
    home.ok(&["bg", "recolor", &s(&png), "--theme", "nord"]);
    let out = home.ok(&[
        "bg",
        "recolor",
        &s(&png),
        "--theme",
        "nord",
        "--engine",
        "gowall",
    ]);
    assert!(out.contains("sky-nord-gowall.png"), "{out}");
    let made = bgs(&home, "nord").join("sky-nord-gowall.png");
    assert!(
        pixels::read(&made, None).is_ok(),
        "gowall wrote a real image"
    );
    assert!(bgs(&home, "nord").join("sky-nord.png").is_file());
}

#[test]
fn generate_apply_works_even_with_wallpaper_off() {
    let home = Home::new();
    home.ok(&["set", "nord"]);
    home.ok(&["apps", "off", "wallpaper"]);
    home.ok(&["bg", "generate", "gradient", "--theme", "nord", "--apply"]);
    assert!(
        home.read(".config/palette/wallpaper")
            .contains("palette-gradient.jpg"),
        "an explicit request still applies"
    );
}

fn hex_value(toml: &str, key: &str) -> Option<String> {
    toml.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
    })
}

#[test]
fn new_from_wallpaper_makes_a_working_theme() {
    let home = Home::new();
    let png = dusk(&home, "dusk.png");
    let out = home.ok(&["new", "dusky", "--from-wallpaper", &s(&png)]);
    assert!(out.contains("Saved"), "{out}");
    let dir = home.palette_dir().join("themes/dusky");
    assert!(dir.join("colors.toml").is_file());
    assert!(dir.join("backgrounds/dusk.png").is_file());
    assert!(!dir.join("light.mode").exists(), "dark picture, dark theme");

    home.ok(&["set", "dusky"]);

    let toml = std::fs::read_to_string(dir.join("colors.toml")).unwrap();
    let bg = hex_value(&toml, "background").expect("background");
    for key in [
        "foreground",
        "accent",
        "red",
        "orange",
        "yellow",
        "green",
        "cyan",
        "blue",
        "magenta",
        "bright_red",
        "bright_yellow",
        "bright_green",
        "bright_cyan",
        "bright_blue",
        "bright_magenta",
    ] {
        if let Some(c) = hex_value(&toml, key) {
            let ratio = palette::colour::contrast(&c, &bg);
            assert!(ratio >= 3.0, "{key} {c} on {bg}: contrast {ratio}");
        }
    }
    assert!(hex_value(&toml, "foreground").is_some());
}

#[test]
fn light_picture_or_flag_gives_a_light_theme() {
    let home = Home::new();
    let pale = home.path("pale.png");
    picture(&pale, [235, 228, 210], [225, 218, 200], [230, 150, 120]);
    home.ok(&["new", "pale", "--from-wallpaper", &s(&pale)]);
    assert!(home.palette_dir().join("themes/pale/light.mode").exists());
    home.ok(&["set", "pale"]);

    let dark = dusk(&home, "dusk.png");
    home.ok(&["new", "forced", "--from-wallpaper", &s(&dark), "--light"]);
    let dir = home.palette_dir().join("themes/forced");
    assert!(dir.join("light.mode").exists());
    let toml = std::fs::read_to_string(dir.join("colors.toml")).unwrap();
    let bg = hex_value(&toml, "background").unwrap();
    let fg = hex_value(&toml, "foreground").unwrap();
    assert!(palette::colour::contrast(&fg, &bg) >= 3.0);

    home.ok(&[
        "new",
        "forced-dark",
        "--from-wallpaper",
        &s(&pale),
        "--dark",
    ]);
    assert!(
        !home
            .palette_dir()
            .join("themes/forced-dark/light.mode")
            .exists()
    );
}

#[test]
fn new_refuses_existing_and_invalid_names() {
    let home = Home::new();
    let png = dusk(&home, "dusk.png");
    let err = home.fails(&["new", "nord", "--from-wallpaper", &s(&png)]);
    assert!(err.contains("already exists"), "{err}");
    home.ok(&["new", "mine", "--from-wallpaper", &s(&png)]);
    let err = home.fails(&["new", "mine", "--from-wallpaper", &s(&png)]);
    assert!(err.contains("already exists"), "{err}");
    for bad in ["Bad Name", "../x", "UPPER", ""] {
        let err = home.fails(&["new", bad, "--from-wallpaper", &s(&png)]);
        assert!(!err.is_empty(), "{bad:?}");
    }
    assert!(!home.palette_dir().join("themes/Bad Name").exists());
    let txt = home.write("n.txt", "x");
    let err = home.fails(&["new", "fromtext", "--from-wallpaper", &s(&txt)]);
    assert!(err.contains("not an image"), "{err}");
    assert!(!home.palette_dir().join("themes/fromtext").exists());
}

#[test]
fn new_dry_run_saves_nothing() {
    let home = Home::new();
    let png = dusk(&home, "dusk.png");
    let out = home.ok(&["new", "ghost", "--from-wallpaper", &s(&png), "--dry-run"]);
    assert!(out.contains("background"), "prints colours: {out}");
    assert!(out.contains('#'));
    assert!(!home.palette_dir().join("themes/ghost").exists());
    assert!(!home.palette_dir().join("themes/.ghost.partial").exists());
}
