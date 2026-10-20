//! The wallpaper library (`palette bg library …`) and the parts of
//! `palette bg get` that need no network. Fake homes only.

mod common;

use common::Home;
use palette::pixels::{self, Image};

fn s(p: &std::path::Path) -> String {
    p.to_string_lossy().to_string()
}

#[test]
fn add_list_use_theme_remove() {
    let home = Home::new();
    let lake = home.write("pics/lake.png", "png");
    let sky = home.write("pics/sky.jpg", "jpeg!");
    home.ok(&["set", "tokyo-night"]);

    let out = home.ok(&["bg", "library"]);
    assert!(out.contains("(0 pictures, 0 bytes)"), "{out}");
    assert!(out.contains(".config/palette/wallpapers"), "{out}");

    home.ok(&["bg", "library", "add", &s(&lake), &s(&sky)]);
    home.ok(&["bg", "library", "add", "--into", "Nord/Lakes", &s(&lake)]);
    let out = home.ok(&["bg", "library", "list"]);
    assert!(out.contains("(3 pictures, 11 bytes)"), "{out}");
    assert!(out.contains("Nord/Lakes/lake.png"), "{out}");

    // Use one now: recorded (fake home: never applied).
    home.ok(&["bg", "library", "use", "sky.jpg"]);
    assert!(
        home.read(".config/palette/wallpaper")
            .ends_with("wallpapers/sky.jpg\n")
    );

    // Into a theme, not forced: the library keeps its copy.
    home.ok(&[
        "bg",
        "library",
        "theme",
        "Nord/Lakes/lake.png",
        "--theme",
        "nord",
    ]);
    assert!(
        home.palette_dir()
            .join("themes/nord/backgrounds/lake.png")
            .is_file()
    );
    home.ok(&["bg", "library", "theme", "lake.png"]); // the active theme
    assert!(
        home.palette_dir()
            .join("themes/tokyo-night/backgrounds/lake.png")
            .is_file()
    );

    home.ok(&["bg", "library", "remove", "Nord/Lakes/lake.png"]);
    assert!(!home.palette_dir().join("wallpapers/Nord").exists());
    let err = home.fails(&["bg", "library", "remove", "../themes/nord/colors.toml"]);
    assert!(err.contains("no picture called"), "{err}");
    assert!(
        home.palette_dir()
            .join("themes/nord/backgrounds/lake.png")
            .is_file(),
        "the theme's copy stays"
    );
}

#[test]
fn new_theme_from_a_library_picture() {
    let home = Home::new();
    let path = home.path("dusk.png");
    let (w, h) = (40, 30);
    let mut img = Image::new(w, h);
    for (i, px) in img.rgb.iter_mut().enumerate() {
        *px = if i < w * h / 2 {
            [30, 40, 90]
        } else {
            [240, 120, 160]
        };
    }
    pixels::write(&path, &img).unwrap();
    home.ok(&["bg", "library", "add", &s(&path)]);

    let dry = home.ok(&[
        "bg",
        "library",
        "new-theme",
        "dusk.png",
        "dusk",
        "--dry-run",
    ]);
    assert!(dry.contains("background = "), "{dry}");
    assert!(!home.palette_dir().join("themes/dusk").exists());

    home.ok(&["bg", "library", "new-theme", "dusk.png", "dusk"]);
    assert!(
        home.palette_dir()
            .join("themes/dusk/backgrounds/dusk.png")
            .is_file()
    );
    assert!(home.palette_dir().join("wallpapers/dusk.png").is_file());
}

#[test]
fn bad_links_fail_before_any_download() {
    let home = Home::new();
    for (url, says) in [
        ("http://example.com/a.png", "https"),
        ("not a link", "isn't a link"),
        (
            "https://github.com/a/b/issues/3",
            "isn't a repo, folder or picture",
        ),
    ] {
        let err = home.fails(&["bg", "get", url]);
        assert!(err.contains(says), "{url}: {err}");
    }
    let err = home.fails(&["bg", "get", "https://github.com/a/b", "--recolor"]);
    assert!(err.contains("--theme"), "{err}");
}
