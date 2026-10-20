//! Phase 5a: choosing which apps Palette themes.

mod common;

use common::Home;

const RC: &str = "#!/bin/bash\nPILL=0xf0292e42\nBLUE=0xff7aa2f7\n\nsketchybar --update\n";
const TMUX: &str = "set -g default-terminal \"tmux-256color\"\n";

fn desktop() -> Home {
    let home = Home::new();
    home.write(".config/sketchybar/sketchybarrc", RC);
    home.write(".tmux.conf", TMUX);
    home.write(".zshrc", "eval \"$(starship init zsh)\"\n");
    home.ok(&["set", "tokyo-night"]);
    home.ok(&["init"]);
    home
}

#[test]
fn everything_is_on_by_default() {
    let home = Home::new();
    let out = home.ok(&["apps"]);
    for app in [
        "sketchybar",
        "tmux",
        "vscodium",
        "fzf",
        "wallpaper",
        "appearance",
    ] {
        let line = out
            .lines()
            .find(|l| l.contains(&format!(" {app} ")))
            .unwrap();
        assert!(line.trim_start().starts_with("on"), "{out}");
    }
}

#[test]
fn switching_an_app_off_restores_it_and_keeps_it_off() {
    let home = desktop();
    assert!(home.read(".tmux.conf").contains("palette"));

    let out = home.ok(&["apps", "off", "tmux"]);
    assert!(
        out.contains("tmux: off") && out.contains("restored"),
        "{out}"
    );
    assert_eq!(home.read(".tmux.conf"), TMUX);
    assert!(
        home.read(".config/palette/settings.toml")
            .contains("tmux = false")
    );

    // init leaves it alone from now on, and says why.
    let out = home.ok(&["init"]);
    assert!(out.contains("off (your choice)"), "{out}");
    assert_eq!(home.read(".tmux.conf"), TMUX);
    let doctor = String::from_utf8_lossy(&home.run(&["doctor"]).stdout).to_string();
    let line = doctor
        .lines()
        .find(|l| l.trim_start().starts_with("tmux"))
        .unwrap();
    assert!(line.contains("off (your choice)"), "{doctor}");

    // On again: wired straight away.
    home.ok(&["apps", "on", "tmux"]);
    assert!(home.read(".tmux.conf").contains("source-file -q"));
    assert!(
        !home
            .read(".config/palette/settings.toml")
            .contains("tmux = false")
    );
}

#[test]
fn off_keeps_the_other_block_in_a_shared_file() {
    let home = desktop();
    home.ok(&["apps", "off", "sketchybar"]);
    let rc = home.read(".config/sketchybar/sketchybarrc");
    assert!(!rc.contains("current/sketchybar.sh"), "{rc}");
    assert!(rc.contains("current/sketchybar-item.sh"), "{rc}");
}

#[test]
fn shell_tools_drop_out_of_shell_sh() {
    let home = desktop();
    assert!(
        home.current_file("shell.sh")
            .contains("FZF_DEFAULT_OPTS_FILE")
    );
    home.ok(&["apps", "off", "fzf"]);
    let shell = home.current_file("shell.sh");
    assert!(!shell.contains("FZF_DEFAULT_OPTS_FILE"), "{shell}");
    assert!(shell.contains("BAT_THEME"), "{shell}");
    // Not a theme switch: no extra undo step.
    assert!(home.fails(&["undo"]).contains("nothing to undo"));
    home.ok(&["apps", "on", "fzf"]);
    assert!(
        home.current_file("shell.sh")
            .contains("FZF_DEFAULT_OPTS_FILE")
    );
}

#[test]
fn wallpaper_and_appearance_can_be_off_too() {
    let home = desktop();
    home.ok(&["apps", "off", "wallpaper", "appearance"]);
    let settings = home.read(".config/palette/settings.toml");
    assert!(settings.contains("wallpaper = false") && settings.contains("appearance = false"));
    assert!(
        home.fails(&["apps", "off", "zen"])
            .contains("no app named `zen`")
    );
}
