//! foot, the Wayland terminal: one `include=` line at the top of `foot.ini`
//! (the file's unnamed first part is foot's `[main]`, where `include` must
//! be). foot reads its config when a window opens: new windows get the theme,
//! open ones keep theirs. foot 1.26 renamed `[colors]` to `[colors-dark]`,
//! so the section name follows the installed version.

use std::path::{Path, PathBuf};

use anyhow::Result;

use super::{BASE, Output, Platforms, Target};
use crate::paths::Paths;
use crate::system::{self, Outcome};
use crate::wiring::{Import, Wiring};

pub const TARGET: Target = Target {
    name: "foot",
    output: Some(Output {
        file: "foot.ini",
        template: include_str!("template.ini"),
        overridable: true,
        finish: Some(finish),
    }),
    wiring: Wiring::Import(Import::new(configs, line, at_top)),
    installed: |_| super::has_program("foot"),
    reload: |_| Outcome::Unchanged("new foot windows use it; open ones keep their colours".into()),
    platforms: Platforms::Linux,
    ..BASE
};

fn configs(paths: &Paths) -> Vec<PathBuf> {
    let conf = paths.config_home().join("foot/foot.ini");
    if conf.is_file() {
        vec![conf]
    } else {
        Vec::new()
    }
}

fn line(_: &Paths, generated: &Path) -> String {
    format!("include={}", generated.display())
}

/// Before the first `[section]`, i.e. in `[main]`. Like Alacritty's import,
/// colours you set yourself further down still win.
fn at_top(contents: &str, block: &str) -> Result<String, String> {
    Ok(format!("{block}\n{contents}"))
}

/// The first foot whose colour section is `[colors-dark]`.
const COLORS_DARK_SINCE: (u32, u32) = (1, 26);

fn finish(_: &Paths, text: String) -> Result<String> {
    let version = system::run("foot", &["--version"])
        .ok()
        .and_then(|v| system::parse_version(&v));
    Ok(section_for(version, text))
}

pub fn section_for(version: Option<(u32, u32)>, text: String) -> String {
    match version {
        Some(v) if v >= COLORS_DARK_SINCE => text.replace("\n[colors]\n", "\n[colors-dark]\n"),
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_name_follows_the_version() {
        let text = "# x\n[colors]\nforeground=c0caf5\n".to_string();
        assert!(section_for(Some((1, 26)), text.clone()).contains("\n[colors-dark]\n"));
        assert!(section_for(Some((1, 25)), text.clone()).contains("\n[colors]\n"));
        assert!(section_for(None, text).contains("\n[colors]\n"));
    }

    #[test]
    fn include_goes_before_any_section() {
        let out = at_top(
            "[main]\nfont=x\n[colors]\n",
            "# palette\ninclude=/p/foot.ini",
        )
        .unwrap();
        assert!(out.starts_with("# palette\ninclude=/p/foot.ini\n[main]"));
    }
}
