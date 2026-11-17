//! Herdr, the desktop app (`~/.config/herdr-app/`). It loads `themes/<name>.json`
//! (background, foreground, selection, cursor, 16 ANSI colours, optional `colors`
//! overrides; panel colours are derived from them) and applies `"theme"` from its
//! `settings.json` live. So, like Zed: `init` copies Palette's file there as
//! `themes/palette.json` and sets `"theme": "palette"`; each switch copies the new
//! file over it. Layouts with their own look (Herdr 98, Terminals) and a theme picked
//! in Herdr's own menu win over settings.json, as Herdr documents.
//!
//! The theme format has no "author" field to mark the file as Palette's, so Palette
//! keeps a copy of what it last wrote (`herdr-app-theme.json` in its own folder): a
//! `palette.json` that differs from it is yours, and is never replaced or deleted.

mod frost;
pub(crate) use frost::is_on as frost_is_on;
pub(crate) use frost::{off as frost_off, on as frost_on};

use std::fs;
use std::path::PathBuf;

use super::{BASE, Output, Platforms, Target};
use crate::paths::Paths;
use crate::system::Outcome;
use crate::wiring::{Setting, Wiring};

pub const TARGET: Target = Target {
    name: "herdr-app",
    output: Some(Output {
        file: "herdr-app.json",
        template: include_str!("template.json"),
        overridable: true,
        finish: None,
    }),
    wiring: Wiring::Setting(Setting {
        configs,
        per_file: false,
        value: |_| crate::jsonc::quoted("palette"),
        read: |text| crate::jsonc::read(text, "theme"),
        write: |text, value| crate::jsonc::write(text, "theme", value),
        prepare: Some(prepare),
        create: false,
    }),
    installed: |paths| super::has_app(paths, "Herdr.app") || super::has_program("herdr-app"),
    reload,
    unload: Some(|_| Outcome::Done("switches back by itself".into())),
    verify: |paths, _| {
        Some(crate::verify::same_file(
            &theme_file(paths),
            &paths.current().join("herdr-app.json"),
            "Herdr's themes/palette.json",
        ))
    },
    platforms: Platforms::Mac,
    ..BASE
};

fn dir(paths: &Paths) -> PathBuf {
    paths.home().join(".config/herdr-app")
}

fn configs(paths: &Paths) -> Vec<PathBuf> {
    vec![dir(paths).join("settings.json")]
}

fn theme_file(paths: &Paths) -> PathBuf {
    dir(paths).join("themes/palette.json")
}

/// What Palette last wrote to `themes/palette.json`.
fn last_file(paths: &Paths) -> PathBuf {
    paths.root().join("herdr-app-theme.json")
}

fn is_ours(paths: &Paths, text: &str) -> bool {
    fs::read_to_string(last_file(paths)).is_ok_and(|last| last == text)
}

fn install(paths: &Paths) -> Result<(), String> {
    super::copy_ours(paths, "herdr-app.json", &theme_file(paths), |t| {
        is_ours(paths, t)
    })?;
    fs::copy(theme_file(paths), last_file(paths)).map_err(|e| e.to_string())?;
    Ok(())
}

fn prepare(paths: &Paths, remove: bool) -> Result<String, String> {
    let file = theme_file(paths);
    if remove {
        if fs::read_to_string(&file).is_ok_and(|t| is_ours(paths, &t)) {
            fs::remove_file(&file).map_err(|e| e.to_string())?;
            let _ = fs::remove_file(last_file(paths));
        }
        return Ok("removed themes/palette.json".into());
    }
    install(paths)?;
    Ok("copied Palette's theme to themes/palette.json".into())
}

fn reload(paths: &Paths) -> Outcome {
    match install(paths) {
        Ok(()) => Outcome::Done("updated (Herdr restyles by itself)".into()),
        Err(e) => Outcome::Failed(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_replaces_its_own_and_keeps_yours() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(tmp.path());
        assert!(install(&paths).unwrap_err().contains("set a theme first"));
        fs::create_dir_all(paths.current()).unwrap();
        fs::write(paths.current().join("herdr-app.json"), "{\"a\": 1}").unwrap();
        prepare(&paths, false).unwrap();
        fs::write(paths.current().join("herdr-app.json"), "{\"a\": 2}").unwrap();
        assert!(matches!(reload(&paths), Outcome::Done(_)));
        assert_eq!(
            fs::read_to_string(theme_file(&paths)).unwrap(),
            "{\"a\": 2}"
        );

        fs::write(theme_file(&paths), "{\"mine\": true}").unwrap();
        assert!(matches!(reload(&paths), Outcome::Failed(_)));
        prepare(&paths, true).unwrap();
        assert_eq!(
            fs::read_to_string(theme_file(&paths)).unwrap(),
            "{\"mine\": true}",
            "a file that isn't Palette's is never deleted"
        );
    }
}
