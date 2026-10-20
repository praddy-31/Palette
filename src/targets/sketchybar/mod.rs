use std::path::PathBuf;

use super::{BASE, Output, Target};
use crate::paths::Paths;
use crate::system::{self, Outcome};
use crate::wiring::{self, Import, Wiring};

pub const TARGET: Target = Target {
    name: "sketchybar",
    output: Some(Output {
        file: "sketchybar.sh",
        template: include_str!("template.sh"),
        overridable: false,
        finish: None,
    }),
    wiring: Wiring::Import(Import::new(
        configs,
        wiring::source_line,
        wiring::after_colours,
    )),
    installed: |_| super::has_program("sketchybar"),
    reload,
    ..BASE
};

/// `sketchybarrc` plus every plugin script that sets its own colours: plugins
/// run as separate scripts, so each needs the `source` line.
fn configs(paths: &Paths) -> Vec<PathBuf> {
    let dir = paths.config_home().join("sketchybar");
    let rc = dir.join("sketchybarrc");
    let mut files: Vec<PathBuf> = Vec::new();
    if rc.is_file() {
        files.push(rc);
        files.extend(wiring::scripts_with_colours(&dir));
    }
    files
}

/// Also used by `sketchybar-item`: both live in the same bar.
pub(super) fn reload(_: &Paths) -> Outcome {
    if !system::is_running("sketchybar") {
        return Outcome::Skipped("not running".into());
    }
    match system::run("sketchybar", &["--reload"]) {
        Ok(_) => Outcome::Done("reloaded".into()),
        Err(e) => Outcome::Failed(e),
    }
}
