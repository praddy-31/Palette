//! Saved bars (ROADMAP 18c): the whole bar (items and look) under a name, in
//! `~/.config/palette/desktop/bars/<name>.toml`, so `palette export` carries them with the rest
//! of Palette's folder. One is "in use" (`bars/.in-use` holds its name): every change to the
//! bar (Apply, a CLI `set`, Go back to…) is written into it too, so switching to another bar
//! and back never loses anything. Colours aren't in a saved bar: they follow the theme.
//!
//! The list has room for Widgets layouts later (`sort`); today every entry is a bar.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use super::Bar;
use crate::desktop::{self, Applied, Tool, write_atomic};
use crate::paths::Paths;

const IN_USE: &str = ".in-use";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Saved {
    pub name: String,
    pub in_use: bool,
    /// `bar` (later also `widgets`).
    pub sort: &'static str,
}

pub fn dir(paths: &Paths) -> PathBuf {
    paths.root().join("desktop/bars")
}

fn file(paths: &Paths, name: &str) -> PathBuf {
    dir(paths).join(format!("{name}.toml"))
}

/// The saved bar's name as written, whatever case `name` uses.
fn find(paths: &Paths, name: &str) -> Option<String> {
    let want = name.trim().to_lowercase();
    list(paths)
        .into_iter()
        .map(|s| s.name)
        .find(|n| n.to_lowercase() == want)
}

fn existing(paths: &Paths, name: &str) -> Result<String> {
    find(paths, name)
        .with_context(|| format!("no saved bar called `{name}` (palette desktop bar saved)"))
}

/// A name that's safe as a file name and in a menu: letters, digits, spaces, `-` and `_`.
fn check_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 40 {
        bail!("a saved bar's name is 1 to 40 characters");
    }
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || " -_".contains(c))
    {
        bail!("a saved bar's name: letters, digits, spaces, - and _ only");
    }
    Ok(name.to_string())
}

/// The saved bar in use, if it's still there.
pub fn in_use(paths: &Paths) -> Option<String> {
    let name = fs::read_to_string(dir(paths).join(IN_USE)).ok()?;
    let name = name.trim();
    file(paths, name).is_file().then(|| name.to_string())
}

fn mark(paths: &Paths, name: Option<&str>) -> Result<()> {
    let at = dir(paths).join(IN_USE);
    match name {
        Some(n) => write_atomic(&at, &format!("{n}\n"), false),
        None => {
            let _ = fs::remove_file(at);
            Ok(())
        }
    }
}

/// Every saved bar, by name.
pub fn list(paths: &Paths) -> Vec<Saved> {
    let using = in_use(paths);
    let mut out: Vec<Saved> = fs::read_dir(dir(paths))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let name = p.file_stem()?.to_str()?.to_string();
            (p.extension()? == "toml" && !name.starts_with('.')).then_some(name)
        })
        .map(|name| Saved {
            in_use: using.as_deref() == Some(name.as_str()),
            name,
            sort: "bar",
        })
        .collect();
    out.sort_by_key(|s| s.name.to_lowercase());
    out
}

/// Save the bar under `name` (a new name) and make it the one in use. `edited`: the Bar page's
/// unapplied changes, applied as part of saving, so they go into the new bar only and never into
/// the bar that was in use (owner, 2026-11-11: both bars ended up with the same look).
pub fn save_as(paths: &Paths, name: &str, edited: Option<Bar>) -> Result<String> {
    let name = check_name(name)?;
    if find(paths, &name).is_some() {
        bail!("there's already a saved bar called `{name}`: pick another name");
    }
    let apply = edited.is_some();
    let bar: Bar = match edited {
        Some(b) => b,
        None => desktop::get(paths, Tool::Bar)?.try_into()?,
    };
    bar.validate()?;
    let before = in_use(paths);
    write_atomic(&file(paths, &name), &toml::to_string(&bar)?, false)?;
    mark(paths, Some(&name))?;
    // Marked first: applying writes into the new bar (`follow`), not the one before.
    if apply && let Err(e) = desktop::set(paths, Tool::Bar, toml::Value::try_from(&bar)?) {
        let _ = fs::remove_file(file(paths, &name));
        mark(paths, before.as_deref())?;
        return Err(e);
    }
    Ok(name)
}

/// Switch the bar to saved bar `name`: written and reloaded like any change (backup first).
pub fn use_bar(paths: &Paths, name: &str) -> Result<Applied> {
    let name = existing(paths, name)?;
    let path = file(paths, &name);
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let bar: Bar =
        toml::from_str(&text).with_context(|| format!("{} has a mistake", path.display()))?;
    bar.validate()
        .with_context(|| format!("the saved bar `{name}`"))?;
    let before = in_use(paths);
    // Marked first, so the change below is written into this bar, not the one before.
    mark(paths, Some(&name))?;
    match desktop::set(paths, Tool::Bar, toml::Value::try_from(&bar)?) {
        Ok(a) => Ok(a),
        Err(e) => {
            mark(paths, before.as_deref())?;
            Err(e)
        }
    }
}

/// `use_bar`, unless it's already the bar in use (the schedule's day and night bars).
pub fn use_if_needed(paths: &Paths, name: &str) -> Result<Option<Applied>> {
    let name = existing(paths, name)?;
    if in_use(paths).as_deref() == Some(name.as_str()) {
        return Ok(None);
    }
    use_bar(paths, &name).map(Some)
}

pub fn rename(paths: &Paths, old: &str, new: &str) -> Result<String> {
    let old = existing(paths, old)?;
    let new = check_name(new)?;
    if let Some(other) = find(paths, &new)
        && other != old
    {
        bail!("there's already a saved bar called `{other}`");
    }
    let was_in_use = in_use(paths).as_deref() == Some(old.as_str());
    fs::rename(file(paths, &old), file(paths, &new)).context("renaming the saved bar")?;
    if was_in_use {
        mark(paths, Some(&new))?;
    }
    rename_in_schedule(paths, &old, Some(&new))?;
    Ok(new)
}

/// Delete a saved bar. The bar on screen stays as it is; if it was the one in use, no saved
/// bar is in use any more (Save as… makes one again).
pub fn delete(paths: &Paths, name: &str) -> Result<String> {
    let name = existing(paths, name)?;
    let was_in_use = in_use(paths).as_deref() == Some(name.as_str());
    fs::remove_file(file(paths, &name)).context("deleting the saved bar")?;
    if was_in_use {
        mark(paths, None)?;
    }
    rename_in_schedule(paths, &name, None)?;
    Ok(name)
}

/// The schedule names saved bars for day and night: keep it pointing at the same bar.
fn rename_in_schedule(paths: &Paths, old: &str, new: Option<&str>) -> Result<()> {
    let mut all = crate::settings::load(paths)?;
    let mut changed = false;
    for slot in [&mut all.schedule.day_bar, &mut all.schedule.night_bar] {
        if slot.as_str() == old {
            *slot = new.unwrap_or_default().to_string();
            changed = true;
        }
    }
    if changed {
        crate::settings::save(paths, &all)?;
    }
    Ok(())
}

/// Called whenever the bar's settings are written: the saved bar in use gets them too.
pub(crate) fn follow(paths: &Paths, text: &str) -> Result<()> {
    if let Some(name) = in_use(paths) {
        write_atomic(&file(paths, &name), text, false)?;
    }
    Ok(())
}
