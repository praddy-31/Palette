//! btop has no include, and `color_theme` only accepts the name of a theme
//! in its own `themes/` folder (a full path is silently ignored). So `init`
//! links `~/.config/btop/themes/palette.theme` to Palette's file and sets
//! `color_theme = "palette"`. btop rewrites btop.conf itself on exit
//! (dropping comments), so the old value is remembered by Palette rather
//! than in a marker comment.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

use super::{BASE, Note, Output, Target};
use crate::paths::Paths;
use crate::system::{self, Outcome};
use crate::wiring::{Setting, Wiring};

pub const TARGET: Target = Target {
    name: "btop",
    output: Some(Output {
        file: "btop.theme",
        template: include_str!("template.theme"),
        overridable: true,
        finish: None,
    }),
    wiring: Wiring::Setting(Setting {
        configs,
        per_file: false,
        value,
        read,
        write,
        prepare: Some(prepare),
    }),
    installed: |_| super::has_program("btop"),
    reload,
    doctor,
    ..BASE
};

fn configs(paths: &Paths) -> Vec<PathBuf> {
    vec![paths.config_home().join("btop/btop.conf")]
}

fn value(_: &Paths) -> String {
    "palette".into()
}

/// `~/.config/btop/themes/palette.theme`, the link btop loads.
fn link(paths: &Paths) -> PathBuf {
    paths.config_home().join("btop/themes/palette.theme")
}

fn is_our_link(paths: &Paths) -> bool {
    fs::read_link(link(paths)).is_ok_and(|t| t == paths.current().join("btop.theme"))
}

/// Create the theme link before the setting changes; delete it after the
/// setting is restored. Never replaces a theme the user made.
fn prepare(paths: &Paths, remove: bool) -> Result<String, String> {
    let link = link(paths);
    if remove {
        if is_our_link(paths) {
            fs::remove_file(&link).map_err(|e| e.to_string())?;
        }
        return Ok("removed themes/palette.theme".into());
    }
    if is_our_link(paths) {
        return Ok("themes/palette.theme already links to Palette".into());
    }
    if fs::symlink_metadata(&link).is_ok() {
        return Err(format!(
            "{} already exists and isn't Palette's; rename it, then run `palette init` again",
            link.display()
        ));
    }
    if let Some(dir) = link.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    symlink(paths.current().join("btop.theme"), &link).map_err(|e| e.to_string())?;
    Ok("linked themes/palette.theme to Palette's file".into())
}

const KEY: &str = "color_theme";

fn is_key_line(line: &str) -> bool {
    line.trim_start()
        .strip_prefix(KEY)
        .is_some_and(|rest| rest.trim_start().starts_with('='))
}

fn read(text: &str) -> Option<String> {
    let line = text.lines().find(|l| is_key_line(l))?;
    let value = line.split_once('=')?.1.trim();
    Some(value.trim_matches('"').to_string())
}

fn write(text: &str, value: Option<&str>) -> String {
    let new_line = value.map(|v| format!("{KEY} = \"{v}\""));
    let mut found = false;
    let mut out = String::new();
    for line in text.split_inclusive('\n') {
        if is_key_line(line) && !found {
            found = true;
            if let Some(new_line) = &new_line {
                out.push_str(new_line);
                out.push('\n');
            }
        } else {
            out.push_str(line);
        }
    }
    if let (false, Some(new_line)) = (found, &new_line) {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(new_line);
        out.push('\n');
    }
    out
}

fn version() -> Option<(u32, u32)> {
    let out = system::run("btop", &["--version"]).ok()?;
    // "btop version: 1.4.7", possibly with terminal colour codes.
    let clean: String = out.chars().filter(|c| !c.is_control()).collect();
    let digits = clean.split("version:").nth(1)?;
    let digits: String = digits
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    system::parse_version(&digits)
}

/// btop 1.4 reloads its config (and theme) on SIGUSR2. Older versions would
/// be killed by the signal.
fn reload(_: &Paths) -> Outcome {
    if !system::is_running("btop") {
        return Outcome::Skipped("not running".into());
    }
    match version() {
        Some(v) if v >= (1, 4) => match system::signal("btop", "USR2") {
            Ok(()) => Outcome::Done("reloaded".into()),
            Err(e) => Outcome::Failed(e),
        },
        _ => Outcome::Skipped("restart btop to see the new colours".into()),
    }
}

fn doctor(paths: &Paths) -> Vec<Note> {
    let mut notes = Vec::new();
    if crate::wiring::is_wired(paths, &TARGET) && !is_our_link(paths) {
        notes.push(Note::fix(
            "themes/palette.theme link is missing: run `palette init --remove`, then `palette init`",
        ));
    }
    if let Some(v) = version()
        && v < (1, 4)
    {
        notes.push(Note::fix(
            "btop older than 1.4 can't reload live; restart it after a switch",
        ));
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_the_setting() {
        let conf = "#* Name of a theme\ncolor_theme = \"Default\"\n\ntheme_background = true\n";
        assert_eq!(read(conf).as_deref(), Some("Default"));
        let wired = write(conf, Some("/p/btop.theme"));
        assert_eq!(read(&wired).as_deref(), Some("/p/btop.theme"));
        assert!(wired.contains("theme_background = true"));
        assert_eq!(write(&wired, Some("Default")), conf);

        assert_eq!(read("x = 1\n"), None);
        assert_eq!(write("x = 1", Some("/p")), "x = 1\ncolor_theme = \"/p\"\n");
        assert_eq!(write("x = 1\ncolor_theme = \"/p\"\n", None), "x = 1\n");
    }
}
