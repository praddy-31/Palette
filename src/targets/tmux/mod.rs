use std::path::{Path, PathBuf};

use super::{BASE, Output, Target};
use crate::paths::Paths;
use crate::system::{self, Outcome};
use crate::wiring::{self, Import, Wiring};

pub const TARGET: Target = Target {
    name: "tmux",
    output: Some(Output {
        file: "tmux.conf",
        template: include_str!("template.conf"),
        // tmux configs can `run-shell`, so a theme may not supply its own.
        overridable: false,
        finish: None,
    }),
    wiring: Wiring::Import(Import::new(configs, line, wiring::at_end)),
    installed: |_| super::has_program("tmux"),
    reload,
    unload: Some(unload),
    ..BASE
};

fn configs(paths: &Paths) -> Vec<PathBuf> {
    wiring::first_config(&[
        paths.home().join(".tmux.conf"),
        paths.config_home().join("tmux/tmux.conf"),
    ])
}

/// `-q`: no error if Palette's file is gone.
fn line(_: &Paths, generated: &Path) -> String {
    format!("source-file -q \"{}\"", generated.display())
}

fn server_running() -> bool {
    system::run("tmux", &["list-sessions"]).is_ok()
}

/// Re-reading Palette's file is enough: it only sets colour options.
fn reload(paths: &Paths) -> Outcome {
    if !server_running() {
        return Outcome::Skipped("not running".into());
    }
    let file = paths.current().join("tmux.conf");
    match system::run("tmux", &["source-file", "-q", &file.to_string_lossy()]) {
        Ok(_) => Outcome::Done("reloaded".into()),
        Err(e) => Outcome::Failed(e),
    }
}

/// After `init --remove`: unset every option Palette's file set, so tmux
/// falls back to the user's own settings and its defaults.
fn unload(paths: &Paths) -> Outcome {
    if !server_running() {
        return Outcome::Skipped("not running".into());
    }
    let text = std::fs::read_to_string(paths.current().join("tmux.conf")).unwrap_or_default();
    // Ignore errors: an older tmux may not know every option.
    for option in options(&text) {
        let _ = system::run("tmux", &["set", "-gu", option]);
    }
    for conf in configs(paths) {
        let _ = system::run("tmux", &["source-file", "-q", &conf.to_string_lossy()]);
    }
    Outcome::Done("back to your colours".into())
}

/// Option names from `set -gq <option> …` lines.
fn options(text: &str) -> Vec<&str> {
    text.lines()
        .filter_map(|l| l.strip_prefix("set -gq "))
        .filter_map(|rest| rest.split_whitespace().next())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_set_options() {
        let text = "# x\nset -gq status-style \"bg=default\"\nset -gq clock-mode-colour \"#fff\"\n";
        assert_eq!(options(text), ["status-style", "clock-mode-colour"]);
        assert_eq!(options(include_str!("template.conf")).len(), 20);
    }
}
