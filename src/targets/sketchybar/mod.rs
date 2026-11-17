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
    verify: |paths, _| Some(verify(paths)),
    platforms: super::Platforms::Mac,
    ..BASE
};

/// Every colour the running bar draws (each item's `…color` settings, read back with
/// `sketchybar --query`) should be one of the theme's (`current/sketchybar.sh`), at any
/// alpha. Left out: fully transparent ones, `highlight_color` (only drawn when highlighted)
/// and opaque black, SketchyBar's default for shadows that aren't drawn.
fn verify(paths: &Paths) -> Outcome {
    if paths.is_fake() {
        return Outcome::Skipped(crate::verify::FAKE.into());
    }
    if !system::is_running("sketchybar") {
        return Outcome::Skipped("not running".into());
    }
    let Ok(colours) = std::fs::read_to_string(paths.current().join("sketchybar.sh")) else {
        return Outcome::Skipped("Palette's colours are missing: run `palette refresh`".into());
    };
    let theme = crate::verify::hex_colours(&colours);
    let bar = match system::run("sketchybar", &["--query", "bar"]) {
        Ok(text) => text,
        Err(e) => return Outcome::Failed(e),
    };
    let (mut seen, mut off) = (0, Vec::new());
    for item in items(&bar) {
        let Ok(text) = system::run("sketchybar", &["--query", &item]) else {
            continue;
        };
        for (key, argb) in drawn_colours(&text) {
            if key == "highlight_color" || argb == "ff000000" || argb.starts_with("00") {
                continue;
            }
            seen += 1;
            if !theme.iter().any(|t| *t == argb[2..]) {
                off.push(format!("{item} {key}=0x{argb}"));
            }
        }
    }
    match (seen, off.len()) {
        (0, _) => Outcome::Skipped("the bar has no coloured items".into()),
        (n, 0) => Outcome::Done(format!("all {n} colours on the bar are the theme's")),
        (n, k) => Outcome::Failed(format!(
            "{k} of {n} colours aren't the theme's (first: {}): `palette refresh`",
            off[0]
        )),
    }
}

/// The item names in `sketchybar --query bar` (its `"items": [ … ]` list).
fn items(bar: &str) -> Vec<String> {
    let Some(start) = bar.find("\"items\"") else {
        return Vec::new();
    };
    let rest = &bar[start + 7..];
    let list = rest.split(']').next().unwrap_or("");
    list.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
        .collect()
}

/// `("color", "aarrggbb")` for each `"…color": "0x…"` line of an item's query.
fn drawn_colours(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.trim().split_once(':')?;
            let key = key.trim().trim_matches('"');
            if !key.ends_with("color") {
                return None;
            }
            let hex = value.trim().trim_end_matches(',').trim_matches('"');
            let hex = hex.strip_prefix("0x")?;
            // SketchyBar prints 0x0 for transparent; pad to 8 digits.
            let v = u32::from_str_radix(hex, 16).ok()?;
            Some((key.to_string(), format!("{v:08x}")))
        })
        .collect()
}

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
/// A bar Palette manages only gets its colours set (ROADMAP 15a); yours is reloaded, and so
/// is Palette's if setting them failed.
pub(super) fn reload(paths: &Paths) -> Outcome {
    if !system::is_running("sketchybar") {
        return Outcome::Skipped("not running".into());
    }
    if let Some(done @ Outcome::Done(_)) = crate::desktop::bar::recolour(paths) {
        return done;
    }
    match system::reload_sketchybar() {
        Ok(_) => Outcome::Done("reloaded".into()),
        Err(e) => Outcome::Failed(e),
    }
}

#[cfg(test)]
mod verify_tests {
    use super::*;

    const BAR: &str = "{\n\t\"position\": \"top\",\n\t\"color\": \"0x0\",\n\t\"items\": [\n\t\t \"space.1\",\n\t\t \"clock\"\n\t]\n}";

    #[test]
    fn reads_the_items_and_their_colours() {
        assert_eq!(items(BAR), vec!["space.1", "clock"]);
        let item = "\t\"icon\": {\n\t\t\"color\": \"0xff7FD8C4\",\n\t\t\"highlight_color\": \"0xff000000\",\n\t\"value\": \"0x12\"\n\t\t\"border_color\": \"0x0\",";
        assert_eq!(
            drawn_colours(item),
            vec![
                ("color".into(), "ff7fd8c4".into()),
                ("highlight_color".into(), "ff000000".into()),
                ("border_color".into(), "00000000".into())
            ]
        );
    }
}
