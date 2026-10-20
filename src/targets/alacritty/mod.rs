use std::path::{Path, PathBuf};

use super::{BASE, Output, Target};
use crate::paths::Paths;
use crate::system::{self, Outcome};
use crate::wiring::{Import, Wiring};

pub const TARGET: Target = Target {
    name: "alacritty",
    output: Some(Output {
        file: "alacritty.toml",
        template: include_str!("template.toml"),
        overridable: true,
        finish: None,
    }),
    wiring: Wiring::Import(Import::new(configs, line, insert)),
    installed: |paths| super::has_app(paths, "Alacritty.app") || super::has_program("alacritty"),
    reload,
    ..BASE
};

fn configs(paths: &Paths) -> Vec<PathBuf> {
    let conf = paths.config_home().join("alacritty/alacritty.toml");
    if conf.is_file() {
        vec![conf]
    } else {
        Vec::new()
    }
}

fn line(_: &Paths, generated: &Path) -> String {
    format!("import = [\"{}\"]", generated.display())
}

/// TOML is structured, so "append a line" doesn't work: `import` belongs in
/// the `[general]` table. Put it right under `[general]`, or as a dotted key
/// at the top when there is no such table. Alacritty loads imports first, so
/// colours in the user's own file still win; that is their call.
fn insert(contents: &str, block: &str) -> Result<String, String> {
    let has_import = contents.lines().any(|l| {
        let l = l.trim_start();
        let l = l.strip_prefix("general.").unwrap_or(l);
        l.strip_prefix("import")
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    });
    if has_import {
        let path = block.lines().last().unwrap_or_default();
        return Err(format!(
            "already has an import list; add the path from `{path}` to it"
        ));
    }
    let lines: Vec<&str> = contents.split_inclusive('\n').collect();
    match lines.iter().position(|l| l.trim() == "[general]") {
        Some(i) => {
            let mut out = lines[..=i].concat();
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(block);
            out.push('\n');
            out.push_str(&lines[i + 1..].concat());
            Ok(out)
        }
        None => {
            let (marker, line) = block.split_once('\n').unwrap_or(("", block));
            Ok(format!("{marker}\ngeneral.{line}\n{contents}"))
        }
    }
}

/// Alacritty watches its config and imports, and reloads by itself.
fn reload(_: &Paths) -> Outcome {
    if system::is_running("alacritty") {
        Outcome::Done("reloads by itself".into())
    } else {
        Outcome::Skipped("not running".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK: &str = "# M\nimport = [\"/x.toml\"]";

    #[test]
    fn inserts_under_general_or_as_dotted_key() {
        let out = insert("[general]\nlive_config_reload = true\n", BLOCK).unwrap();
        assert_eq!(
            out,
            "[general]\n# M\nimport = [\"/x.toml\"]\nlive_config_reload = true\n"
        );
        out.parse::<toml::Table>().unwrap();

        let out = insert("[font]\nsize = 14\n", BLOCK).unwrap();
        assert_eq!(
            out,
            "# M\ngeneral.import = [\"/x.toml\"]\n[font]\nsize = 14\n"
        );
        let table = out.parse::<toml::Table>().unwrap();
        assert!(table["general"]["import"].is_array());
    }

    #[test]
    fn existing_import_is_left_to_the_user() {
        assert!(insert("[general]\nimport = [\"a\"]\n", BLOCK).is_err());
    }
}
