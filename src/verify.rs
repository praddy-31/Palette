//! `palette verify [<app>]` (ROADMAP 29): after a switch, read back what each app actually
//! took, not what Palette wrote. Read-only, like `doctor`, but where doctor asks "is it
//! wired?", verify asks "is it showing the current theme?": SketchyBar's live colours,
//! Rex's theme slot and appearance, the files Zed, VSCodium, the Herdr app and Orca load,
//! and macOS's light/dark. Each target that can be read back has a `verify` function in its
//! own module; apps with nothing to read (a terminal reading an include line) are left out.
//!
//! In a fake home nothing is asked of running programs (they are the real Mac's), so only
//! the file comparisons run there.

use std::fs;
use std::path::Path;

use anyhow::{Result, bail};

use crate::paths::Paths;
use crate::system::{self, Outcome};
use crate::theme::{self, Mode, Theme};
use crate::{settings, state, targets};

/// One app's answer: `Done` = it shows the theme, `Failed` = it doesn't (the text says what
/// it shows instead), `Skipped` = can't tell (not running, off, a fake home).
#[derive(Debug)]
pub struct Row {
    pub name: String,
    pub outcome: Outcome,
}

impl Row {
    pub fn failed(&self) -> bool {
        matches!(self.outcome, Outcome::Failed(_))
    }
}

/// Every app that can be read back, or only `only` (a target name, or `appearance`).
pub fn run(paths: &Paths, only: Option<&str>) -> Result<Vec<Row>> {
    let Some(name) = state::current_theme(paths)? else {
        bail!(crate::problem::Problem::NoThemeYet);
    };
    let theme = theme::load(paths, &name)?;
    let settings = settings::load(paths).unwrap_or_default();
    let wanted = |n: &str| only.is_none_or(|o| o == n);
    let mut rows = Vec::new();
    if wanted("appearance") && cfg!(target_os = "macos") {
        let outcome = match settings.is_on("appearance") {
            false => Outcome::Skipped(crate::effects::OFF.into()),
            true => appearance(paths, &theme),
        };
        rows.push(Row {
            name: "appearance".into(),
            outcome,
        });
    }
    for target in targets::all().filter(|t| wanted(t.name)) {
        if !settings.is_on(target.name) {
            if only.is_some() {
                rows.push(Row {
                    name: target.name.into(),
                    outcome: Outcome::Skipped(crate::effects::OFF.into()),
                });
            }
            continue;
        }
        if !(target.installed)(paths) && only.is_none() {
            continue;
        }
        if !crate::wiring::is_wired(paths, target) {
            if only.is_some() {
                rows.push(Row {
                    name: target.name.into(),
                    outcome: Outcome::Skipped("not wired: run `palette init`".into()),
                });
            }
            continue;
        }
        if let Some(outcome) = (target.verify)(paths, &theme) {
            rows.push(Row {
                name: target.name.into(),
                outcome,
            });
        } else if only.is_some() {
            rows.push(Row {
                name: target.name.into(),
                outcome: Outcome::Skipped(
                    "nothing to read back: it reads Palette's file itself (`palette doctor` checks the wiring)".into(),
                ),
            });
        }
    }
    if let Some(o) = only
        && rows.is_empty()
    {
        bail!("no app called {o}: see `palette apps`");
    }
    Ok(rows)
}

/// macOS light/dark against the theme's mode.
fn appearance(paths: &Paths, theme: &Theme) -> Outcome {
    if paths.is_fake() {
        return Outcome::Skipped(FAKE.into());
    }
    let want = mode_name(theme.colors.mode);
    match system::dark_mode() {
        None => Outcome::Skipped("couldn't read macOS's appearance".into()),
        Some(dark) => {
            let now = if dark { "dark" } else { "light" };
            pass_if(
                now == want,
                format!("macOS is {now}"),
                format!("macOS is {now}, the theme is {want}"),
            )
        }
    }
}

/// The reason a check that asks a running program skips in a fake home.
pub const FAKE: &str = "not asked: fake home (PALETTE_HOME)";

pub fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Dark => "dark",
        Mode::Light => "light",
    }
}

/// `Done(ok)` or `Failed(bad)`.
pub fn pass_if(ok: bool, pass: String, fail: String) -> Outcome {
    if ok {
        Outcome::Done(pass)
    } else {
        Outcome::Failed(fail)
    }
}

/// Does the file an app loads hold exactly what Palette rendered? `what` names it in the
/// answer ("Zed's copy").
pub fn same_file(loaded: &Path, rendered: &Path, what: &str) -> Outcome {
    match (fs::read(loaded), fs::read(rendered)) {
        (Err(_), _) => Outcome::Failed(format!("{what} is missing: {}", loaded.display())),
        (_, Err(_)) => Outcome::Skipped("Palette's file is missing: run `palette refresh`".into()),
        (Ok(a), Ok(b)) => pass_if(
            a == b,
            format!("{what} is the current theme"),
            format!("{what} differs from the current theme: run `palette refresh`"),
        ),
    }
}

/// `0xAARRGGBB` colours in `text` (SketchyBar's form), as `rrggbb`.
pub fn hex_colours(text: &str) -> Vec<String> {
    text.split("0x")
        .skip(1)
        .filter_map(|rest| {
            let hex: String = rest.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
            (hex.len() == 8).then(|| hex[2..].to_ascii_lowercase())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_from_sketchybar_text() {
        assert_eq!(
            hex_colours("export C_accent=0xff7fd8c4\n\"color\": \"0x66FFFFFF\", 0x0"),
            vec!["7fd8c4", "ffffff"]
        );
    }

    #[test]
    fn same_file_says_which() {
        let d = tempfile::tempdir().unwrap();
        let (a, b) = (d.path().join("a"), d.path().join("b"));
        fs::write(&b, "x").unwrap();
        assert!(matches!(same_file(&a, &b, "copy"), Outcome::Failed(_)));
        fs::write(&a, "x").unwrap();
        assert!(matches!(same_file(&a, &b, "copy"), Outcome::Done(_)));
        fs::write(&a, "y").unwrap();
        assert!(matches!(same_file(&a, &b, "copy"), Outcome::Failed(_)));
    }
}
