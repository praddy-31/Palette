//! Noctalia (v5), the desktop shell on niri: its bar, panels and launcher
//! colours from a generated palette. Noctalia reads custom palettes from
//! `~/.config/noctalia/palettes/<name>.json`, so on every switch Palette
//! copies its file there as `Palette.json` (its own file) and selects it with
//! `noctalia msg color-scheme-set custom Palette`. The theme goes into both
//! the `dark` and `light` halves; light/dark itself is `noctalia msg
//! theme-mode-set` (`system::set_dark_mode`), the wallpaper `wallpaper-set`.
//! The scheme you had before (`noctalia msg color-scheme-get`, e.g.
//! `wallpaper m3-fruit-salad`) is remembered and put back when Palette stops
//! theming Noctalia.
//!
//! Noctalia can also theme apps itself (its templates). Palette themes the
//! same apps on every desktop; switch off Noctalia's template for any app
//! Palette themes, or `palette apps off <app>` to leave it to Noctalia.

use std::fs;
use std::path::PathBuf;

use super::{BASE, Output, Platforms, Target};
use crate::paths::Paths;
use crate::system::{self, Outcome};
use crate::wiring::Wiring;

pub const TARGET: Target = Target {
    name: "noctalia",
    output: Some(Output {
        file: "noctalia.json",
        template: include_str!("template.json"),
        overridable: false,
        finish: None,
    }),
    wiring: Wiring::Direct,
    installed: |_| super::has_program("noctalia"),
    reload,
    unload: Some(unload),
    platforms: Platforms::Linux,
    ..BASE
};

/// The palette's name in Noctalia (and its file name).
const NAME: &str = "Palette";

fn reload(paths: &Paths) -> Outcome {
    if !system::is_running("noctalia") {
        return Outcome::Skipped("not running".into());
    }
    let palettes = paths.config_home().join("noctalia/palettes");
    let copied = fs::create_dir_all(&palettes).and_then(|()| {
        fs::copy(
            paths.current().join("noctalia.json"),
            palettes.join(format!("{NAME}.json")),
        )
    });
    if let Err(e) = copied {
        return Outcome::Failed(format!("copying the palette: {e}"));
    }
    remember_previous(paths);
    match system::run_desktop("noctalia", &["msg", "color-scheme-set", "custom", NAME]) {
        Ok(_) => Outcome::Done("palette applied".into()),
        Err(e) => Outcome::Failed(e),
    }
}

/// Where the scheme you had before Palette is remembered.
fn previous_file(paths: &Paths) -> PathBuf {
    paths.root().join("noctalia-previous-scheme")
}

/// Keep the scheme in use unless it is already Palette's.
fn remember_previous(paths: &Paths) {
    let Ok(current) = system::run_desktop("noctalia", &["msg", "color-scheme-get"]) else {
        return;
    };
    let current = current.trim();
    if !current.is_empty() && current != format!("custom {NAME}") {
        let _ = fs::create_dir_all(paths.root());
        let _ = fs::write(previous_file(paths), current);
    }
}

/// Back to the scheme you had (Noctalia's own if Palette never saw one).
fn unload(paths: &Paths) -> Outcome {
    if !system::is_running("noctalia") {
        return Outcome::Skipped("not running".into());
    }
    let previous = fs::read_to_string(previous_file(paths))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "builtin Noctalia".into());
    let mut args = vec!["msg", "color-scheme-set"];
    args.extend(previous.split_whitespace());
    match system::run_desktop("noctalia", &args) {
        Ok(_) => Outcome::Done(format!("back to {previous}")),
        Err(e) => Outcome::Failed(e),
    }
}
