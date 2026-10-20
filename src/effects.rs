//! What happens outside `~/.config/palette` after a switch: reload apps,
//! light/dark appearance, wallpaper.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::paths::Paths;
use crate::system::{self, Outcome};
use crate::targets::{self, Target};
use crate::theme::{Mode, Theme};
use crate::wiring::{self, Wiring};

/// One line of the report: what was affected and how it went.
pub type Effect = (String, Outcome);

/// Apply everything for `theme` (or only reload apps when the theme can't be
/// loaded, e.g. after `undo` to a theme that has since been deleted).
/// `wallpaper`: one of the theme's to use instead of its default (the quick
/// switcher lets you pick one before applying).
pub fn apply(paths: &Paths, theme: Option<&Theme>, wallpaper: Option<&Path>) -> Vec<Effect> {
    if paths.is_fake() {
        // Nothing runs, but the wallpaper choice is still recorded (if it's on).
        let mut effects = vec![(
            "apps & macOS".into(),
            Outcome::Skipped("left alone: fake home (PALETTE_HOME)".into()),
        )];
        let settings = crate::settings::load(paths).unwrap_or_default();
        // Off: the line is left out, as the real report hides it.
        if let Some(theme) = theme
            && settings.is_on("wallpaper")
            && let outcome @ Outcome::Skipped(_) =
                wallpaper_effect(paths, &settings, theme, wallpaper)
        {
            effects.push(("wallpaper".into(), outcome));
        }
        return effects;
    }

    let settings = crate::settings::load(paths).unwrap_or_default();
    let mut ran = Reloads::default();
    let mut effects: Vec<Effect> = targets::ALL
        .iter()
        .map(|t| {
            let outcome = if settings.is_on(t.name) {
                reload(paths, t, &mut ran)
            } else {
                Outcome::Skipped(OFF.into())
            };
            (t.name.to_string(), outcome)
        })
        .collect();

    if let Some(theme) = theme {
        let appearance = match settings.is_on("appearance") {
            true => appearance(theme.colors.mode),
            false => Outcome::Skipped(OFF.into()),
        };
        effects.push(("appearance".into(), appearance));
        effects.push((
            "wallpaper".into(),
            wallpaper_effect(paths, &settings, theme, wallpaper),
        ));
    }
    effects
}

/// Reload functions already run in this switch. Two targets can share one
/// (SketchyBar and its item): the bar should redraw once, not twice.
#[derive(Default)]
pub struct Reloads(Vec<fn(&Paths) -> Outcome>);

impl Reloads {
    pub fn run(&mut self, paths: &Paths, reload: fn(&Paths) -> Outcome) -> Outcome {
        if self.0.iter().any(|f| std::ptr::fn_addr_eq(*f, reload)) {
            return Outcome::Unchanged("reloaded together with the above".into());
        }
        self.0.push(reload);
        reload(paths)
    }
}

/// Reload one app, or say why not.
fn reload(paths: &Paths, target: &Target, ran: &mut Reloads) -> Outcome {
    let installed = (target.installed)(paths);
    let shell_tool = matches!(target.wiring, Wiring::Shell);
    if shell_tool && !installed {
        // Wired through shell.sh even when absent; nothing to report.
        Outcome::Skipped(NOT_INSTALLED.into())
    } else if wiring::is_wired(paths, target) {
        ran.run(paths, target.reload)
    } else if !installed {
        Outcome::Skipped(NOT_INSTALLED.into())
    } else if !wiring::has_config(paths, target) {
        Outcome::Skipped("no config found".into())
    } else {
        Outcome::Skipped("not wired: run `palette init`".into())
    }
}

/// Reports hide this outcome: most people have only some of the apps.
pub const NOT_INSTALLED: &str = "not installed";

/// For apps the user switched off (`palette apps off`).
pub const OFF: &str = "off (your choice)";

fn appearance(mode: Mode) -> Outcome {
    let dark = mode == Mode::Dark;
    if system::dark_mode() == Some(dark) {
        return Outcome::Unchanged(format!("already {}", mode.as_str()));
    }
    match system::set_dark_mode(dark) {
        Ok(()) => Outcome::Done(format!("switched to {}", mode.as_str())),
        Err(e) => Outcome::Failed(e),
    }
}

/// The wallpaper step of a switch. The setting is checked *before* anything
/// is chosen: `theme_wallpaper` sets the picture as it decides, so it must not
/// run at all when the wallpaper is off. (It once ran as half of a tuple that
/// was matched afterwards, and Rust evaluates both halves first.)
fn wallpaper_effect(
    paths: &Paths,
    settings: &crate::settings::Settings,
    theme: &Theme,
    wanted: Option<&Path>,
) -> Outcome {
    if !settings.is_on("wallpaper") {
        return Outcome::Skipped(OFF.into());
    }
    match theme_wallpaper(paths, theme, wanted) {
        Ok(outcome) => outcome,
        Err(e) => Outcome::Failed(format!("{e:#}")),
    }
}

/// The wallpaper asked for, else keep the current one if it already belongs
/// to this theme, else the theme's default (`bg default`), else its first.
/// Themes without wallpapers leave the user's alone.
fn theme_wallpaper(paths: &Paths, theme: &Theme, wanted: Option<&Path>) -> Result<Outcome> {
    if let Some(image) = wanted {
        return set_wallpaper(paths, image);
    }
    let images = theme.backgrounds()?;
    if images.is_empty() {
        return Ok(Outcome::Unchanged(format!(
            "{} has no wallpapers, yours is unchanged (add some: palette bg add <image>…)",
            theme.name
        )));
    }
    let current = current_wallpaper(paths);
    if current.as_ref().is_some_and(|c| images.contains(c)) {
        return Ok(Outcome::Unchanged("already one of this theme's".into()));
    }
    let start = default_wallpaper(paths, theme).unwrap_or_else(|| images[0].clone());
    set_wallpaper(paths, &start)
}

/// The wallpaper a theme starts with, if one was chosen and still exists.
pub fn default_wallpaper(paths: &Paths, theme: &Theme) -> Option<PathBuf> {
    let settings = crate::settings::load(paths).ok()?;
    let name = settings.wallpapers.get(&theme.name)?;
    wallpaper_named(theme, name).ok()
}

/// `palette bg default <name>`: start this theme with that wallpaper from now
/// on (`None`: back to its first).
pub fn set_default_wallpaper(paths: &Paths, theme: &Theme, name: Option<&str>) -> Result<()> {
    let _lock = crate::lock::exclusive(paths)?;
    let mut settings = crate::settings::load(paths)?;
    match name {
        Some(name) => {
            wallpaper_named(theme, name)?;
            settings
                .wallpapers
                .insert(theme.name.clone(), name.to_string());
        }
        None => {
            settings.wallpapers.remove(&theme.name);
        }
    }
    crate::settings::save(paths, &settings)
}

/// The wallpaper Palette last set, if any.
pub fn current_wallpaper(paths: &Paths) -> Option<PathBuf> {
    let text = fs::read_to_string(paths.wallpaper_state()).ok()?;
    Some(PathBuf::from(text.trim_end_matches('\n')))
}

/// Set (or, for a fake home, only record) the wallpaper.
pub fn set_wallpaper(paths: &Paths, image: &std::path::Path) -> Result<Outcome> {
    let _lock = crate::lock::exclusive(paths)?;
    let name = image
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let outcome = if paths.is_fake() {
        Outcome::Skipped(format!("{name} (not applied: fake home)"))
    } else {
        match system::set_wallpaper(image) {
            Ok(()) => Outcome::Done(name),
            Err(e) => return Ok(Outcome::Failed(e)),
        }
    };
    fs::create_dir_all(paths.root())?;
    fs::write(paths.wallpaper_state(), format!("{}\n", image.display()))
        .context("saving the wallpaper choice")?;
    Ok(outcome)
}

/// `palette bg next` / `prev`: the theme's next (`by` = 1) or previous (-1)
/// wallpaper, wrapping around.
pub fn step_wallpaper(paths: &Paths, theme: &Theme, by: isize) -> Result<Outcome> {
    let _lock = crate::lock::exclusive(paths)?;
    let images = theme.backgrounds()?;
    if images.is_empty() {
        bail!(
            "{} has no wallpapers. Add some with: palette bg add <image>…",
            theme.name
        );
    }
    let n = images.len() as isize;
    let current = current_wallpaper(paths);
    let at = match current.and_then(|c| images.iter().position(|i| *i == c)) {
        Some(i) => (i as isize + by).rem_euclid(n),
        None => 0,
    };
    set_wallpaper(paths, &images[at as usize])
}

/// `palette bg add`: copy images into the theme's `backgrounds/` folder.
/// Copies, not links, so the theme folder stays self-contained.
pub fn add_wallpapers(theme: &Theme, files: &[PathBuf]) -> Result<Vec<PathBuf>> {
    for file in files {
        let meta = fs::metadata(file).with_context(|| format!("reading {}", file.display()))?;
        if !meta.is_file() || !crate::theme::is_image(file) {
            bail!(
                "{} is not an image (png, jpg, jpeg, heic, webp, tiff)",
                file.display()
            );
        }
    }
    let dir = theme.backgrounds_dir();
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let mut added = Vec::new();
    for file in files {
        let dest = dir.join(file.file_name().context("file has no name")?);
        if dest.exists() {
            bail!("{} already exists; rename the image first", dest.display());
        }
        fs::copy(file, &dest).with_context(|| format!("copying {}", file.display()))?;
        added.push(dest);
    }
    Ok(added)
}

/// A theme's wallpaper by file name (what the app and `bg set` pass), so
/// callers can only ever reach images inside the theme's own folder.
pub fn wallpaper_named(theme: &Theme, name: &str) -> Result<PathBuf> {
    theme
        .backgrounds()?
        .into_iter()
        .find(|p| p.file_name().is_some_and(|n| n == name))
        .with_context(|| format!("{} has no wallpaper named `{name}`", theme.name))
}

/// `palette bg remove`: delete Palette's copy of a wallpaper (the original
/// you added it from is untouched). The desktop keeps showing it until the
/// next change.
pub fn remove_wallpaper(theme: &Theme, name: &str) -> Result<PathBuf> {
    let file = wallpaper_named(theme, name)?;
    fs::remove_file(&file).with_context(|| format!("removing {}", file.display()))?;
    Ok(file)
}

#[cfg(test)]
mod wallpaper_tests {
    use super::*;
    use crate::settings::Settings;

    /// The bug behind "Wallpaper off still switched it": the choice ran (and set
    /// the picture) before the setting was looked at.
    #[test]
    fn off_means_nothing_is_chosen() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        let bg = paths.themes().join("nord/backgrounds");
        fs::create_dir_all(&bg).unwrap();
        fs::write(bg.join("a.png"), "png").unwrap();
        let nord = crate::theme::load(&paths, "nord").unwrap();

        let mut settings = Settings::default();
        settings.off.insert("wallpaper".into());
        let outcome = wallpaper_effect(&paths, &settings, &nord, None);
        assert!(matches!(outcome, Outcome::Skipped(ref s) if s == OFF));
        assert!(current_wallpaper(&paths).is_none(), "nothing set while off");

        settings.off.clear();
        wallpaper_effect(&paths, &settings, &nord, None);
        assert_eq!(current_wallpaper(&paths), Some(bg.join("a.png")));
    }
}
