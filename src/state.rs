//! Writing rendered files to disk atomically, and reading back what is active.
//!
//! Layout (see `docs/THEME_SPEC.md`):
//!   current -> generations/<millis>-<theme>     (a symlink)
//!   generations/<millis>-<theme>/{theme, sketchybar.sh, ...}
//!
//! A switch renders into a new generation folder, then swaps the `current`
//! symlink with `rename(2)`, which is atomic: readers see the old theme or the
//! new one, never a mix.

use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use crate::paths::Paths;
use crate::render::OutputFile;

/// Generations kept on disk (for `palette undo` later). The active one is never removed.
const KEEP_GENERATIONS: usize = 5;

/// Inside each generation: the name of the theme it was rendered from.
const THEME_MARKER: &str = "theme";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    New,
    Changed,
    Unchanged,
}

/// The active theme's name, or `None` if nothing has been set yet.
pub fn current_theme(paths: &Paths) -> Result<Option<String>> {
    let marker = paths.current().join(THEME_MARKER);
    match fs::read_to_string(&marker) {
        Ok(name) => Ok(Some(name.trim().to_string())),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", marker.display())),
    }
}

/// How each file would differ from what is active now.
pub fn diff(paths: &Paths, files: &[OutputFile]) -> Result<Vec<(&'static str, Change)>> {
    let current = paths.current();
    files
        .iter()
        .map(|f| {
            let path = current.join(f.name);
            let change = match fs::read(&path) {
                Ok(old) if old == f.contents => Change::Unchanged,
                Ok(_) => Change::Changed,
                Err(e) if e.kind() == ErrorKind::NotFound => Change::New,
                Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
            };
            Ok((f.name, change))
        })
        .collect()
}

/// Write `files` as a new generation and make it current. Returns the generation folder.
pub fn apply(paths: &Paths, theme: &str, files: &[OutputFile]) -> Result<PathBuf> {
    // Never clobber a `current` we didn't make (checked before writing anything).
    let current = paths.current();
    if let Ok(meta) = fs::symlink_metadata(&current)
        && !meta.file_type().is_symlink()
    {
        bail!(
            "{} exists but is not a symlink made by palette; move it aside and try again",
            current.display()
        );
    }

    let generations = paths.generations();
    fs::create_dir_all(&generations)
        .with_context(|| format!("creating {}", generations.display()))?;

    // 1. Write everything into a hidden staging folder...
    let name = generation_name(&generations, theme);
    let staging = generations.join(format!(".tmp-{name}"));
    fs::create_dir(&staging).with_context(|| format!("creating {}", staging.display()))?;
    let written = write_files(&staging, theme, files);
    if let Err(e) = written {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }

    // 2. ...rename it into place, so a crash never leaves a half-written generation...
    let generation = generations.join(&name);
    fs::rename(&staging, &generation)
        .with_context(|| format!("moving {} into place", generation.display()))?;

    // 3. ...and atomically repoint `current` at it. The link is relative so
    // the whole palette folder can be moved.
    swap_current(paths, &Path::new("generations").join(&name))?;

    remove_old_generations(paths, &name)?;
    Ok(generation)
}

fn write_files(dir: &Path, theme: &str, files: &[OutputFile]) -> Result<()> {
    fs::write(dir.join(THEME_MARKER), format!("{theme}\n"))?;
    for f in files {
        let path = dir.join(f.name);
        fs::write(&path, &f.contents).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

/// Rewrite one file of the active generation (temp file + rename, so
/// readers never see half of it). For changes that aren't a theme switch,
/// such as an app switched off in `shell.sh`, and so don't belong in `undo`.
pub fn replace_file(paths: &Paths, file: &OutputFile) -> Result<()> {
    let dir = fs::canonicalize(paths.current()).context("no theme is active yet")?;
    let tmp = dir.join(format!(".{}.tmp", file.name));
    fs::write(&tmp, &file.contents).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, dir.join(file.name)).with_context(|| format!("updating {}", file.name))
}

/// `<13-digit millis>-<theme>`: sorts by time as plain text. The number is
/// always above the newest existing generation's, so two switches in the
/// same millisecond (scrolling the bar item) still sort in the order they
/// happened, not by theme name.
fn generation_name(generations: &Path, theme: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let newest = fs::read_dir(generations)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let name = name.strip_prefix(".tmp-").unwrap_or(&name).to_string();
            name.split('-').next()?.parse::<u128>().ok()
        })
        .max();
    let millis = match newest {
        Some(n) if n >= now => n + 1,
        _ => now,
    };
    format!("{millis:013}-{theme}")
}

fn swap_current(paths: &Paths, target: &Path) -> Result<()> {
    let current = paths.current();
    let tmp_link = paths.root().join(".current.tmp");
    match fs::remove_file(&tmp_link) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(e).with_context(|| format!("removing {}", tmp_link.display())),
    }
    symlink(target, &tmp_link).with_context(|| format!("creating {}", tmp_link.display()))?;
    fs::rename(&tmp_link, &current).with_context(|| format!("updating {}", current.display()))
}

/// Generation folder names, oldest first (staging folders excluded).
fn generation_names(paths: &Paths) -> Result<Vec<String>> {
    let dir = paths.generations();
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", dir.display())),
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    Ok(names)
}

/// `palette undo`: point `current` at the generation before the active one.
/// Returns the theme name it went back to, or `None` if there is no earlier one.
/// Repeating walks further back through the kept generations.
pub fn step_back(paths: &Paths) -> Result<Option<String>> {
    let names = generation_names(paths)?;
    let active = fs::read_link(paths.current())
        .ok()
        .and_then(|t| t.file_name()?.to_str().map(str::to_string));
    let Some(i) = active.and_then(|a| names.iter().position(|n| *n == a)) else {
        return Ok(None);
    };
    let Some(previous) = i.checked_sub(1).map(|j| &names[j]) else {
        return Ok(None);
    };
    swap_current(paths, &Path::new("generations").join(previous))?;
    current_theme(paths)
}

/// Keep the newest `KEEP_GENERATIONS` (always including `active`), delete the rest.
fn remove_old_generations(paths: &Paths, active: &str) -> Result<()> {
    let names = generation_names(paths)?;
    let excess = names.len().saturating_sub(KEEP_GENERATIONS);
    for old in names.iter().take(excess).filter(|n| *n != active) {
        let path = paths.generations().join(old);
        fs::remove_dir_all(&path).with_context(|| format!("removing {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &'static str, text: &str) -> OutputFile {
        OutputFile {
            name,
            contents: text.as_bytes().to_vec(),
            is_override: false,
        }
    }

    #[test]
    fn step_back_walks_history() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        assert_eq!(step_back(&paths).unwrap(), None);
        for name in ["one", "two", "three"] {
            apply(&paths, name, &[file("a.conf", name)]).unwrap();
        }
        assert_eq!(step_back(&paths).unwrap().as_deref(), Some("two"));
        assert_eq!(
            fs::read_to_string(paths.current().join("a.conf")).unwrap(),
            "two"
        );
        assert_eq!(step_back(&paths).unwrap().as_deref(), Some("one"));
        assert_eq!(step_back(&paths).unwrap(), None);
        assert_eq!(current_theme(&paths).unwrap().as_deref(), Some("one"));
    }

    #[test]
    fn quick_switches_keep_their_order() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        for name in ["zebra", "apple", "mango"] {
            apply(&paths, name, &[file("a.conf", name)]).unwrap();
        }
        let names = generation_names(&paths).unwrap();
        let themes: Vec<&str> = names.iter().map(|n| n.split_once('-').unwrap().1).collect();
        assert_eq!(themes, ["zebra", "apple", "mango"]);
    }

    #[test]
    fn apply_then_read_back() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        assert_eq!(current_theme(&paths).unwrap(), None);

        apply(&paths, "one", &[file("a.conf", "1")]).unwrap();
        assert_eq!(current_theme(&paths).unwrap().as_deref(), Some("one"));
        assert!(
            fs::symlink_metadata(paths.current())
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_to_string(paths.current().join("a.conf")).unwrap(),
            "1"
        );

        let next = [file("a.conf", "1"), file("b.conf", "2")];
        assert_eq!(
            diff(&paths, &next).unwrap(),
            [("a.conf", Change::Unchanged), ("b.conf", Change::New)]
        );
        let next = [file("a.conf", "changed")];
        assert_eq!(diff(&paths, &next).unwrap(), [("a.conf", Change::Changed)]);
    }

    #[test]
    fn keeps_only_recent_generations() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        for i in 0..8 {
            apply(&paths, &format!("t{i}"), &[file("a.conf", "x")]).unwrap();
        }
        let left = fs::read_dir(paths.generations()).unwrap().count();
        assert_eq!(left, KEEP_GENERATIONS);
        assert_eq!(current_theme(&paths).unwrap().as_deref(), Some("t7"));
    }

    #[test]
    fn refuses_to_replace_a_real_current_folder() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        fs::create_dir_all(paths.current()).unwrap();
        let err = apply(&paths, "one", &[file("a.conf", "1")]).unwrap_err();
        assert!(err.to_string().contains("not a symlink"));
    }
}
