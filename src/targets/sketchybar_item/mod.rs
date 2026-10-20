//! A SketchyBar item for Palette itself: current theme name, click to open
//! the picker, scroll to change theme. Talks to the app through `palette://`
//! links, so the bar never needs to know where the CLI is installed.

use std::path::PathBuf;

use super::{BASE, Output, Target};
use crate::paths::Paths;
use crate::wiring::{self, Import, Wiring};

pub const TARGET: Target = Target {
    name: "sketchybar-item",
    output: Some(Output {
        file: "sketchybar-item.sh",
        template: include_str!("template.sh"),
        overridable: false,
        finish: None,
    }),
    wiring: Wiring::Import(Import::new(configs, wiring::source_line, before_update)),
    installed: |_| super::has_program("sketchybar"),
    // The same bar reload as the `sketchybar` target: needed when only the
    // item changed (adding it with `init`). Runs once per switch either way.
    reload: super::sketchybar::reload,
    ..BASE
};

/// Only sketchybarrc: plugins run often, and must not add items.
fn configs(paths: &Paths) -> Vec<PathBuf> {
    let rc = paths.config_home().join("sketchybar/sketchybarrc");
    if rc.is_file() { vec![rc] } else { Vec::new() }
}

/// Just before the final `sketchybar --update`, so the item is drawn with
/// the rest; at the end if there is none. Right-hand items are added right
/// to left, so this lands left of the user's last right-hand item.
fn before_update(contents: &str, block: &str) -> Result<String, String> {
    let lines: Vec<&str> = contents.split_inclusive('\n').collect();
    match lines
        .iter()
        .rposition(|l| l.trim_start().starts_with("sketchybar --update"))
    {
        Some(i) => Ok(format!(
            "{}{block}\n{}",
            lines[..i].concat(),
            lines[i..].concat()
        )),
        None => wiring::at_end(contents, block),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goes_before_the_final_update() {
        let rc = "#!/bin/bash\nsketchybar --add item clock right\n\nsketchybar --update\n";
        assert_eq!(
            before_update(rc, "# M\nX").unwrap(),
            "#!/bin/bash\nsketchybar --add item clock right\n\n# M\nX\nsketchybar --update\n"
        );
        assert_eq!(before_update("a\n", "# M\nX").unwrap(), "a\n# M\nX\n");
    }
}
