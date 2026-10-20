use std::path::{Path, PathBuf};

use super::{BASE, Note, Output, Target};
use crate::paths::Paths;
use crate::system::{self, Outcome};
use crate::wiring::{self, Import, Wiring};

pub const TARGET: Target = Target {
    name: "ghostty",
    output: Some(Output {
        file: "ghostty.conf",
        template: include_str!("template.conf"),
        overridable: true,
        finish: None,
    }),
    wiring: Wiring::Import(Import::new(configs, line, wiring::at_end)),
    installed: |paths| super::has_app(paths, "Ghostty.app") || super::has_program("ghostty"),
    reload,
    doctor,
    ..BASE
};

/// Ghostty reads the macOS location and the XDG one; wire the one in use.
fn configs(paths: &Paths) -> Vec<PathBuf> {
    let mac = paths
        .home()
        .join("Library/Application Support/com.mitchellh.ghostty");
    let xdg = paths.config_home().join("ghostty");
    wiring::first_config(&[
        mac.join("config.ghostty"),
        mac.join("config"),
        xdg.join("config.ghostty"),
        xdg.join("config"),
    ])
}

/// `?` makes the include optional, so Ghostty still starts without Palette.
/// Explicit colours here override a `theme = …` line in the user's config.
fn line(_: &Paths, generated: &Path) -> String {
    format!("config-file = ?{}", generated.display())
}

/// Ghostty reloads its config on SIGUSR2 from 1.2. Older versions would be
/// killed by the signal, so check first.
fn version() -> Option<(u32, u32)> {
    [
        "/Applications/Ghostty.app/Contents/MacOS/ghostty",
        "ghostty",
    ]
    .iter()
    .find_map(|bin| system::run(bin, &["--version"]).ok())
    .and_then(|out| system::parse_version(&out))
}

fn doctor(_: &Paths) -> Vec<Note> {
    match version() {
        Some(v) if v >= (1, 2) => vec![Note::ok("live reload supported")],
        Some(_) => vec![Note::fix(
            "Ghostty older than 1.2 can't reload live; update it or press cmd+shift+,",
        )],
        None => Vec::new(),
    }
}

fn reload(_: &Paths) -> Outcome {
    if !system::is_running("ghostty") {
        return Outcome::Skipped("not running".into());
    }
    match version() {
        Some(v) if v >= (1, 2) => match system::signal("ghostty", "USR2") {
            Ok(()) => Outcome::Done("reloaded".into()),
            Err(e) => Outcome::Failed(e),
        },
        _ => Outcome::Skipped(
            "can't reload this Ghostty version; press cmd+shift+, in Ghostty".into(),
        ),
    }
}
