use std::path::{Path, PathBuf};

use super::{BASE, Note, Output, Target};
use crate::paths::Paths;
use crate::system::Outcome;
use crate::wiring::{self, Import, Wiring};

pub const TARGET: Target = Target {
    name: "neovim",
    output: Some(Output {
        file: "neovim.lua",
        template: include_str!("template.lua"),
        // Lua runs inside Neovim, so only Palette's template may produce it.
        overridable: false,
        finish: None,
    }),
    wiring: Wiring::Import(Import {
        comment: "--",
        // A fresh Neovim has no config; `init` creates init.lua with the one line.
        create: true,
        ..Import::new(configs, line, wiring::at_end)
    }),
    installed: |_| super::has_program("nvim"),
    reload: |_| Outcome::Done("running Neovims reload themselves".into()),
    unload: Some(|_| Outcome::Unchanged("restart Neovim to drop the colours".into())),
    doctor,
    ..BASE
};

fn dir(paths: &Paths) -> PathBuf {
    paths.config_home().join("nvim")
}

/// init.lua (created if needed). Neovim refuses to start with both init.lua
/// and init.vim, so a Vimscript config is left to the user.
fn configs(paths: &Paths) -> Vec<PathBuf> {
    let dir = dir(paths);
    if dir.join("init.vim").is_file() && !dir.join("init.lua").is_file() {
        return Vec::new();
    }
    vec![dir.join("init.lua")]
}

/// `pcall`: no error if Palette's file is gone.
fn line(_: &Paths, generated: &Path) -> String {
    let path = generated
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!("pcall(dofile, \"{path}\")")
}

fn doctor(paths: &Paths) -> Vec<Note> {
    let dir = dir(paths);
    if dir.join("init.vim").is_file() && !dir.join("init.lua").is_file() {
        let generated = paths.current().join("neovim.lua");
        return vec![Note::fix(format!(
            "init.vim isn't wired automatically; add: lua pcall(dofile, \"{}\")",
            generated.display()
        ))];
    }
    Vec::new()
}
