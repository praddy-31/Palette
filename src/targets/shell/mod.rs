//! `shell.sh`: environment variables for tools that are configured that way
//! (fzf, bat, starship, lazygit). Each of those targets contributes its
//! lines through `env`; this target only collects them and wires the rc file.

use std::path::PathBuf;

use anyhow::Result;

use super::{BASE, Note, Output, Target};
use crate::paths::Paths;
use crate::system::Outcome;
use crate::wiring::{self, Import, Wiring};

pub const TARGET: Target = Target {
    name: "shell",
    output: Some(Output {
        file: "shell.sh",
        template: include_str!("template.sh"),
        overridable: false,
        finish: Some(finish),
    }),
    wiring: Wiring::Import(Import::new(configs, wiring::source_line, wiring::at_end)),
    installed: |_| true,
    reload: |_| Outcome::Unchanged("new shells load it".into()),
    unload: Some(|_| Outcome::Unchanged("new shells drop it".into())),
    doctor,
    ..BASE
};

/// zsh (the macOS default) and bash, whichever rc files exist.
fn configs(paths: &Paths) -> Vec<PathBuf> {
    let zdotdir = std::env::var_os("ZDOTDIR")
        .filter(|_| !paths.is_fake())
        .map(PathBuf::from)
        .unwrap_or_else(|| paths.home().to_path_buf());
    [zdotdir.join(".zshrc"), paths.home().join(".bashrc")]
        .into_iter()
        .filter(|p| p.is_file())
        .collect()
}

fn finish(paths: &Paths, header: String) -> Result<String> {
    let mut out = header;
    let settings = crate::settings::load(paths)?;
    for target in super::ALL.iter().filter(|t| settings.is_on(t.name)) {
        let lines = (target.env)(paths);
        if !lines.is_empty() {
            out.push_str(&format!("\n# {}\n", target.name));
            for line in lines {
                out.push_str(&line);
                out.push('\n');
            }
        }
    }
    Ok(out)
}

/// Shells started before `palette init` never sourced shell.sh, so starship,
/// fzf and bat look unthemed in them. Only meaningful for a real home, run
/// from a terminal: the app (opened from Finder) has no `TERM` and never
/// sourced shell.sh, so asking it would always say "open a new terminal".
fn doctor(paths: &Paths) -> Vec<Note> {
    if paths.is_fake()
        || std::env::var_os("TERM").is_none()
        || !crate::wiring::is_wired(paths, &TARGET)
    {
        return Vec::new();
    }
    let expected = paths.current().join("fzf.opts");
    match std::env::var_os("FZF_DEFAULT_OPTS_FILE") {
        Some(v) if std::path::Path::new(&v) == expected => Vec::new(),
        _ => vec![Note::fix(
            "this terminal was opened before `palette init`, so it lacks Palette's variables: \
             run `exec zsh` here (and in every older tmux pane), or open a new one",
        )],
    }
}
