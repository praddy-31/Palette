use std::path::{Path, PathBuf};

use super::{BASE, Output, Target};
use crate::paths::Paths;
use crate::system::{self, Outcome};
use crate::wiring::{self, Import, Wiring};

pub const TARGET: Target = Target {
    name: "kitty",
    output: Some(Output {
        file: "kitty.conf",
        template: include_str!("template.conf"),
        overridable: true,
        finish: None,
    }),
    wiring: Wiring::Import(Import::new(configs, line, wiring::at_end)),
    installed: |paths| super::has_app(paths, "kitty.app") || super::has_program("kitty"),
    reload,
    ..BASE
};

fn configs(paths: &Paths) -> Vec<PathBuf> {
    let conf = paths.config_home().join("kitty/kitty.conf");
    if conf.is_file() {
        vec![conf]
    } else {
        Vec::new()
    }
}

fn line(_: &Paths, generated: &Path) -> String {
    format!("include {}", generated.display())
}

/// kitty reloads kitty.conf on SIGUSR1.
fn reload(_: &Paths) -> Outcome {
    if !system::is_running("kitty") {
        return Outcome::Skipped("not running".into());
    }
    match system::signal("kitty", "USR1") {
        Ok(()) => Outcome::Done("reloaded".into()),
        Err(e) => Outcome::Failed(e),
    }
}
