//! fuzzel, the Wayland app launcher (niri's default): one `include=` line at
//! the top of `fuzzel.ini`, before any section (fuzzel takes `include` only
//! there; the included file has its own sections). fuzzel reads its config
//! each time it opens, so there is nothing to reload. Colours you set in your
//! own `[colors]` further down still win: `doctor` says so.

use std::fs;
use std::path::{Path, PathBuf};

use super::{BASE, Note, Output, Platforms, Target};
use crate::paths::Paths;
use crate::system::Outcome;
use crate::wiring::{Import, Wiring};

pub const TARGET: Target = Target {
    name: "fuzzel",
    output: Some(Output {
        file: "fuzzel.ini",
        template: include_str!("template.ini"),
        overridable: true,
        finish: None,
    }),
    wiring: Wiring::Import(Import::new(configs, line, at_top)),
    installed: |_| super::has_program("fuzzel"),
    reload: |_| Outcome::Unchanged("fuzzel uses it the next time it opens".into()),
    doctor,
    platforms: Platforms::Linux,
    ..BASE
};

fn config(paths: &Paths) -> PathBuf {
    paths.config_home().join("fuzzel/fuzzel.ini")
}

fn configs(paths: &Paths) -> Vec<PathBuf> {
    let conf = config(paths);
    if conf.is_file() {
        vec![conf]
    } else {
        Vec::new()
    }
}

fn line(_: &Paths, generated: &Path) -> String {
    format!("include={}", generated.display())
}

/// Before the first `[section]`, where fuzzel accepts `include`.
fn at_top(contents: &str, block: &str) -> Result<String, String> {
    Ok(format!("{block}\n{contents}"))
}

fn doctor(paths: &Paths) -> Vec<Note> {
    let Ok(text) = fs::read_to_string(config(paths)) else {
        return Vec::new();
    };
    let own = own_colours(&text);
    if own.is_empty() {
        return Vec::new();
    }
    vec![Note::fix(format!(
        "fuzzel.ini sets its own colours ({}), which win over Palette's: \
         delete them from its [colors] section to let Palette theme fuzzel",
        own.join(", ")
    ))]
}

/// The keys set in the user's own `[colors]` section.
pub fn own_colours(text: &str) -> Vec<String> {
    let mut in_colors = false;
    let mut keys = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_colors = line == "[colors]";
        } else if in_colors
            && !line.starts_with('#')
            && let Some((key, _)) = line.split_once('=')
        {
            keys.push(key.trim().to_string());
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_users_own_colours() {
        let text = "include=/p/fuzzel.ini\n[main]\nfont=x\n[colors]\n# old\nbackground=1a1b26f2\ntext = c0caf5ff\n[border]\nwidth=2\n";
        assert_eq!(own_colours(text), ["background", "text"]);
        assert!(own_colours("[main]\nfont=x\n[colors]\n").is_empty());
    }

    #[test]
    fn include_goes_before_any_section() {
        let out = at_top("[main]\nfont=x\n", "# palette\ninclude=/p/fuzzel.ini").unwrap();
        assert!(out.starts_with("# palette\ninclude=/p/fuzzel.ini\n[main]"));
    }
}
