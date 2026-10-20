//! WezTerm. Palette writes `current/wezterm.lua`, a Lua file returning a
//! `colors` table, and `palette init` adds one line before the user's
//! `return config`:
//!
//! ```lua
//! pcall(function() config.colors = dofile("…/current/wezterm.lua"); … end)
//! ```
//!
//! `pcall` keeps WezTerm starting if Palette is gone. The same line adds
//! Palette's folder to WezTerm's reload watch list, so a switch (which swaps
//! the `current` link in that folder) makes running WezTerms reload on their
//! own: there is no reload command or signal to send.

use std::path::{Path, PathBuf};

use super::{BASE, Note, Output, Target};
use crate::paths::Paths;
use crate::system::Outcome;
use crate::wiring::{Import, Wiring};

pub const TARGET: Target = Target {
    name: "wezterm",
    output: Some(Output {
        file: "wezterm.lua",
        template: include_str!("template.lua"),
        // Lua runs inside WezTerm, so only Palette's template may produce it.
        overridable: false,
        finish: None,
    }),
    wiring: Wiring::Import(Import {
        comment: "--",
        ..Import::new(configs, line, before_return)
    }),
    installed: |paths| super::has_app(paths, "WezTerm.app") || super::has_program("wezterm"),
    reload: |_| Outcome::Unchanged("WezTerm reloads on its own (ctrl+shift+r forces it)".into()),
    unload: Some(|_| Outcome::Unchanged("WezTerm reloads on its own".into())),
    doctor,
    ..BASE
};

/// WezTerm's own search order: the XDG location, `~/.config/wezterm`, then
/// `~/.wezterm.lua`. The first that exists is the one it reads.
fn candidates(paths: &Paths) -> Vec<PathBuf> {
    let mut list = vec![paths.config_home().join("wezterm/wezterm.lua")];
    let dot_config = paths.home().join(".config/wezterm/wezterm.lua");
    if !list.contains(&dot_config) {
        list.push(dot_config);
    }
    list.push(paths.home().join(".wezterm.lua"));
    list
}

fn configs(paths: &Paths) -> Vec<PathBuf> {
    candidates(paths)
        .into_iter()
        .find(|p| p.is_file())
        .into_iter()
        .collect()
}

fn lua_string(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

/// Set the colours and watch Palette's folder, all inside `pcall`.
fn line(_: &Paths, generated: &Path) -> String {
    let watch = generated
        .parent()
        .and_then(Path::parent)
        .unwrap_or(generated);
    format!(
        "pcall(function() config.colors = dofile(\"{}\"); require(\"wezterm\").add_to_config_reload_watch_list(\"{}\") end)",
        lua_string(generated),
        lua_string(watch)
    )
}

/// The `return <name>` that ends the config: its line index and the name.
fn final_return(contents: &str) -> Option<(usize, &str)> {
    let lines: Vec<&str> = contents.lines().collect();
    let (i, last) = lines
        .iter()
        .enumerate()
        .rev()
        .find(|(_, l)| !l.trim().is_empty() && !l.trim_start().starts_with("--"))?;
    let name = last.strip_prefix("return ")?.trim();
    let is_name = !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.starts_with(|c: char| c.is_ascii_digit());
    is_name.then_some((i, name))
}

/// Put the block right before the final `return config`, so Palette's colours
/// win over a `color_scheme` set earlier. A config that returns a table
/// literal, or a variable not called `config`, is left to the user.
fn before_return(contents: &str, block: &str) -> Result<String, String> {
    let line = block.lines().last().unwrap_or_default();
    match final_return(contents) {
        Some((i, "config")) => {
            let mut lines: Vec<String> = contents.split_inclusive('\n').map(String::from).collect();
            lines.insert(i, format!("{block}\n"));
            Ok(lines.concat())
        }
        Some((_, name)) => Err(format!(
            "your wezterm.lua returns `{name}`; add this line before that return, with `config` changed to `{name}`: {line}"
        )),
        None => Err(format!(
            "your wezterm.lua doesn't end with `return config`; build it with `local config = wezterm.config_builder()`, or add before its return: {line}"
        )),
    }
}

fn doctor(paths: &Paths) -> Vec<Note> {
    let config = configs(paths);
    if config.is_empty() && (TARGET.installed)(paths) {
        return vec![Note::fix(
            "no wezterm.lua yet: create ~/.config/wezterm/wezterm.lua ending in `return config`, then run palette init",
        )];
    }
    match config.first().and_then(|p| std::fs::read_to_string(p).ok()) {
        Some(text) if final_return(&text).is_none_or(|(_, n)| n != "config") => {
            vec![Note::fix(
                "wezterm.lua doesn't end with `return config`: `palette init` shows the line to add by hand",
            )]
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_before_return_config() {
        let config = "local wezterm = require 'wezterm'\nlocal config = wezterm.config_builder()\nconfig.font_size = 13\nreturn config\n";
        let out = before_return(config, "-- marker\nLINE").unwrap();
        assert!(out.ends_with("config.font_size = 13\n-- marker\nLINE\nreturn config\n"));
        // Trailing comments and blank lines after the return are fine.
        let out = before_return("local config = {}\nreturn config\n\n-- end\n", "B").unwrap();
        assert_eq!(out, "local config = {}\nB\nreturn config\n\n-- end\n");
    }

    #[test]
    fn other_shapes_are_left_to_the_user() {
        let err = before_return("return {\n  font_size = 12,\n}\n", "-- m\nLINE").unwrap_err();
        assert!(err.contains("LINE"), "{err}");
        let err = before_return("local c = {}\nreturn c\n", "-- m\nLINE").unwrap_err();
        assert!(err.contains("returns `c`"), "{err}");
    }
}
