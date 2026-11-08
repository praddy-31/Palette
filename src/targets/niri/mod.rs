//! niri, the scrolling window manager: the focus ring and window border
//! colours. One `include` line at the end of `~/.config/niri/config.kdl`
//! loads Palette's file (later settings win in niri), and niri reloads its
//! config by itself when a file changes. `include` needs niri 25.11 or newer.

use std::path::{Path, PathBuf};

use super::{BASE, Note, Output, Platforms, Target};
use crate::paths::Paths;
use crate::system::{self, Outcome};
use crate::wiring::{self, Import, Wiring};

pub const TARGET: Target = Target {
    name: "niri",
    output: Some(Output {
        file: "niri.kdl",
        template: include_str!("template.kdl"),
        // niri's config can start programs (`spawn-at-startup`): ours only.
        overridable: false,
        finish: None,
    }),
    wiring: Wiring::Import(Import {
        comment: "//",
        ..Import::new(configs, line, wiring::at_end)
    }),
    installed: |_| super::has_program("niri"),
    reload: |_| Outcome::Unchanged("niri reloads its config on its own".into()),
    unload: Some(|_| Outcome::Unchanged("niri reloads its config on its own".into())),
    doctor,
    platforms: Platforms::Linux,
    ..BASE
};

fn configs(paths: &Paths) -> Vec<PathBuf> {
    let conf = paths.config_home().join("niri/config.kdl");
    if conf.is_file() {
        vec![conf]
    } else {
        Vec::new()
    }
}

fn line(_: &Paths, generated: &Path) -> String {
    format!("include \"{}\"", generated.display())
}

/// The first niri with `include`.
const INCLUDE_SINCE: (u32, u32) = (25, 11);

fn doctor(_: &Paths) -> Vec<Note> {
    let Ok(out) = system::run("niri", &["--version"]) else {
        return Vec::new();
    };
    match system::parse_version(&out) {
        Some(v) if v < INCLUDE_SINCE => vec![Note::fix(format!(
            "niri {}.{} can't include files (25.11 or newer can): border colours won't apply",
            v.0, v.1
        ))],
        _ => Vec::new(),
    }
}
