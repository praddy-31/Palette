//! Favourites (up to ten themes, in `settings.toml`) and recents (the last
//! eight themes switched to, in `~/.config/palette/recent`).
//!
//! Favourites are a preference you edit, so they live in `settings.toml` and
//! travel with `palette export`. Recents are history that changes on every
//! switch, so they live in their own small state file next to `current`:
//! rewriting `settings.toml` on every scroll of the bar pill would be slow
//! and would race with someone editing it by hand.

use std::fs;

use anyhow::{Context, Result, bail};

use crate::paths::Paths;
use crate::{settings, theme};

/// The most favourites kept.
pub const MAX_FAVOURITES: usize = 10;
/// The most recents kept.
pub const MAX_RECENTS: usize = 8;

/// Which themes `palette next`/`prev` (and the bar pill's scroll) step through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cycle {
    #[default]
    All,
    Favourites,
}

impl Cycle {
    pub fn as_str(self) -> &'static str {
        match self {
            Cycle::All => "all",
            Cycle::Favourites => "favourites",
        }
    }

    pub fn parse(s: &str) -> Result<Cycle> {
        match s {
            "all" => Ok(Cycle::All),
            "favourites" | "favorites" => Ok(Cycle::Favourites),
            _ => bail!("`{s}`: next_cycles is \"all\" or \"favourites\""),
        }
    }
}

/// Favourite theme names, in the order they were added.
pub fn list(paths: &Paths) -> Result<Vec<String>> {
    Ok(settings::load(paths)?.favourites)
}

/// `palette fav add <theme>`. Returns false if it already was one.
pub fn add(paths: &Paths, name: &str) -> Result<bool> {
    let _lock = crate::lock::exclusive(paths)?;
    theme::load(paths, name)?;
    let mut s = settings::load(paths)?;
    if s.favourites.iter().any(|f| f == name) {
        return Ok(false);
    }
    if s.favourites.len() >= MAX_FAVOURITES {
        bail!(
            "you already have {MAX_FAVOURITES} favourites; remove one first (palette fav remove <theme>)"
        );
    }
    s.favourites.push(name.to_string());
    settings::save(paths, &s)?;
    Ok(true)
}

/// `palette fav remove <theme>`. Returns false if it wasn't one.
pub fn remove(paths: &Paths, name: &str) -> Result<bool> {
    let _lock = crate::lock::exclusive(paths)?;
    let mut s = settings::load(paths)?;
    let before = s.favourites.len();
    s.favourites.retain(|f| f != name);
    if s.favourites.len() == before {
        return Ok(false);
    }
    settings::save(paths, &s)?;
    Ok(true)
}

/// Favourite or not, as `on` says. Returns the favourites afterwards.
pub fn set(paths: &Paths, name: &str, on: bool) -> Result<Vec<String>> {
    if on {
        add(paths, name)?;
    } else {
        remove(paths, name)?;
    }
    list(paths)
}

/// `palette next_cycles`: save which themes `next`/`prev` step through.
pub fn set_cycle(paths: &Paths, cycle: Cycle) -> Result<()> {
    let _lock = crate::lock::exclusive(paths)?;
    let mut s = settings::load(paths)?;
    s.next_cycles = cycle;
    settings::save(paths, &s)
}

/// The themes to step through, alphabetical. `None`: the saved
/// `next_cycles` setting, which falls back to every theme when there are no
/// favourites (so the bar pill never stops working). Asking for favourites
/// explicitly with none set is an error.
pub fn cycle_names(paths: &Paths, cycle: Option<Cycle>) -> Result<Vec<String>> {
    let all: Vec<String> = theme::list(paths)?.into_iter().map(|(n, _)| n).collect();
    let s = settings::load(paths)?;
    let favourites: Vec<String> = all
        .iter()
        .filter(|n| s.favourites.contains(n))
        .cloned()
        .collect();
    match cycle {
        Some(Cycle::Favourites) if favourites.is_empty() => {
            bail!("no favourites yet: add some with `palette fav add <theme>`")
        }
        Some(Cycle::Favourites) => Ok(favourites),
        None if s.next_cycles == Cycle::Favourites && !favourites.is_empty() => Ok(favourites),
        _ => Ok(all),
    }
}

fn recent_file(paths: &Paths) -> std::path::PathBuf {
    paths.root().join("recent")
}

/// The last themes switched to, newest first (at most [`MAX_RECENTS`]).
pub fn recents(paths: &Paths) -> Vec<String> {
    match fs::read_to_string(recent_file(paths)) {
        Ok(text) => text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .take(MAX_RECENTS)
            .map(String::from)
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Remember a switch to `name` (moves it to the front). Temp file + rename.
pub fn record(paths: &Paths, name: &str) -> Result<()> {
    let mut list = recents(paths);
    list.retain(|n| n != name);
    list.insert(0, name.to_string());
    list.truncate(MAX_RECENTS);
    let path = recent_file(paths);
    let tmp = paths.root().join(".recent.tmp");
    fs::create_dir_all(paths.root())?;
    fs::write(&tmp, list.join("\n") + "\n")
        .with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, &path).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recents_keep_eight_newest_first() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        assert!(recents(&paths).is_empty());
        for i in 0..10 {
            record(&paths, &format!("t{i}")).unwrap();
        }
        record(&paths, "t5").unwrap();
        let r = recents(&paths);
        assert_eq!(r.len(), MAX_RECENTS);
        assert_eq!(r[0], "t5");
        assert_eq!(r[1], "t9");
        assert_eq!(r.iter().filter(|n| *n == "t5").count(), 1);
    }

    #[test]
    fn favourites_add_remove_and_cycle() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        assert!(cycle_names(&paths, Some(Cycle::Favourites)).is_err());
        let all = cycle_names(&paths, None).unwrap();
        assert!(add(&paths, "tokyo-night").unwrap());
        assert!(!add(&paths, "tokyo-night").unwrap());
        assert!(add(&paths, "catppuccin-mocha").unwrap());
        assert!(add(&paths, "no-such-theme").is_err());
        assert_eq!(
            cycle_names(&paths, Some(Cycle::Favourites)).unwrap(),
            ["catppuccin-mocha", "tokyo-night"]
        );
        // The setting decides plain `next`.
        assert_eq!(cycle_names(&paths, None).unwrap(), all);
        set_cycle(&paths, Cycle::Favourites).unwrap();
        assert_eq!(cycle_names(&paths, None).unwrap().len(), 2);
        assert!(remove(&paths, "catppuccin-mocha").unwrap());
        assert!(!remove(&paths, "catppuccin-mocha").unwrap());
        assert_eq!(list(&paths).unwrap(), ["tokyo-night"]);
    }
}
