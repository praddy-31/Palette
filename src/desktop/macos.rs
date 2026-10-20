//! Dock and menu bar (macOS): hide them automatically, which suits a
//! floating bar, plus where the Dock sits and how big it is. Read from the
//! macOS settings. The Dock's are written to its settings (`defaults`) and
//! the Dock restarted, which applies them exactly and on every display
//! (System Events rounded the size, 40 became 39, and didn't always reach
//! other displays). The menu bar goes through System Events, as dark mode
//! does. Switching back is the same switch.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::paths::Paths;
use crate::system::{self, Outcome};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MacBits {
    /// Hide the Dock until the pointer reaches its edge.
    pub dock_autohide: bool,
    /// `bottom`, `left` or `right`.
    pub dock_position: String,
    /// Icon size, 16 to 128 points.
    pub dock_size: u32,
    /// Icons grow under the pointer.
    pub dock_magnify: bool,
    /// Hide the macOS menu bar until the pointer reaches the top. With a
    /// notch (MacBook screens) windows still stay below it; on other screens
    /// they can then use the top, under a bar drawn there.
    pub menu_bar_autohide: bool,
}

impl Default for MacBits {
    /// macOS's own defaults.
    fn default() -> Self {
        Self {
            dock_autohide: false,
            dock_position: "bottom".into(),
            dock_size: 48,
            dock_magnify: false,
            menu_bar_autohide: false,
        }
    }
}

impl MacBits {
    pub fn validate(&self) -> Result<()> {
        if !["bottom", "left", "right"].contains(&self.dock_position.as_str()) {
            bail!("dock_position is bottom, left or right");
        }
        if !(16..=128).contains(&self.dock_size) {
            bail!("dock_size is 16 to 128");
        }
        Ok(())
    }
}

/// The current settings (unset ones are macOS's defaults).
pub fn read() -> MacBits {
    let d = MacBits::default();
    let flag = |domain: &str, key: &str, default: bool| {
        system::defaults_read(domain, key).map_or(default, |v| v == "1")
    };
    MacBits {
        dock_autohide: flag("com.apple.dock", "autohide", d.dock_autohide),
        dock_position: system::defaults_read("com.apple.dock", "orientation")
            .filter(|p| ["bottom", "left", "right"].contains(&p.as_str()))
            .unwrap_or(d.dock_position),
        dock_size: system::defaults_read("com.apple.dock", "tilesize")
            .and_then(|v| v.parse::<f64>().ok())
            .map_or(d.dock_size, |v| v.round().clamp(16.0, 128.0) as u32),
        dock_magnify: flag("com.apple.dock", "magnification", d.dock_magnify),
        menu_bar_autohide: flag("NSGlobalDomain", "_HIHideMenuBar", d.menu_bar_autohide),
    }
}

/// Change what differs from the current settings.
pub fn set(paths: &Paths, new: &MacBits) -> Result<Outcome> {
    new.validate()?;
    let before = read();
    let dock = dock_writes(&before, new);
    let menu = (before.menu_bar_autohide != new.menu_bar_autohide).then(|| {
        format!(
            "tell application \"System Events\" to tell dock preferences to set autohide menu bar to {}",
            new.menu_bar_autohide
        )
    });
    if dock.is_empty() && menu.is_none() {
        return Ok(Outcome::Unchanged("already set".into()));
    }
    if paths.is_fake() {
        return Ok(Outcome::Skipped("fake home".into()));
    }
    for args in &dock {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        if let Err(e) = system::run("defaults", &args) {
            return Ok(Outcome::Failed(e));
        }
    }
    // macOS starts the Dock again at once, with the new settings.
    if !dock.is_empty()
        && let Err(e) = system::signal("Dock", "TERM")
    {
        return Ok(Outcome::Failed(e));
    }
    if let Some(script) = menu
        && let Err(e) = system::apple_script(&script)
    {
        return Ok(Outcome::Failed(e));
    }
    Ok(Outcome::Done("changed".into()))
}

/// `defaults write com.apple.dock …` for what changed.
fn dock_writes(before: &MacBits, new: &MacBits) -> Vec<Vec<String>> {
    let w = |key: &str, kind: &str, value: String| -> Vec<String> {
        ["write", "com.apple.dock", key, kind]
            .into_iter()
            .map(str::to_string)
            .chain([value])
            .collect()
    };
    let mut out = Vec::new();
    if before.dock_autohide != new.dock_autohide {
        out.push(w("autohide", "-bool", new.dock_autohide.to_string()));
    }
    if before.dock_position != new.dock_position {
        out.push(w("orientation", "-string", new.dock_position.clone()));
    }
    if before.dock_size != new.dock_size {
        out.push(w("tilesize", "-int", new.dock_size.to_string()));
    }
    if before.dock_magnify != new.dock_magnify {
        out.push(w("magnification", "-bool", new.dock_magnify.to_string()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_what_changed() {
        let before = MacBits::default();
        assert!(dock_writes(&before, &before).is_empty());
        let new = MacBits {
            dock_autohide: true,
            dock_size: 40,
            menu_bar_autohide: true,
            ..before.clone()
        };
        let w = dock_writes(&before, &new);
        assert_eq!(w.len(), 2, "{w:?}");
        assert_eq!(
            w[0],
            ["write", "com.apple.dock", "autohide", "-bool", "true"]
        );
        // The size exactly as chosen (System Events turned 40 into 39).
        assert_eq!(w[1], ["write", "com.apple.dock", "tilesize", "-int", "40"]);
    }

    #[test]
    fn refuses_bad_values() {
        for bad in [
            MacBits {
                dock_position: "top".into(),
                ..MacBits::default()
            },
            MacBits {
                dock_size: 300,
                ..MacBits::default()
            },
        ] {
            assert!(bad.validate().is_err());
        }
    }

    #[test]
    fn a_fake_home_changes_nothing() {
        let h = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(h.path());
        let mut new = read();
        new.dock_magnify = !new.dock_magnify;
        assert!(matches!(set(&paths, &new).unwrap(), Outcome::Skipped(_)));
    }
}
