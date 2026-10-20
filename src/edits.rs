//! Your colour edits: `themes/<name>/edits.toml`, a few `key = "#rrggbb"`
//! lines laid over the theme's own colours when it loads (`theme::load`).
//! The theme's `colors.toml` (or the built-in copy) is never changed, so
//! Reset is deleting lines, and `palette update` can replace a downloaded
//! theme's colours while your edits stay on top.

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::check::{self, Report};
use crate::paths::Paths;
use crate::theme::{self, Mode};

pub const FILE: &str = "edits.toml";

/// Big enough for every key many times over; anything larger isn't ours.
const MAX_SIZE: u64 = 64 * 1024;

/// Keys that changed, each with its new colour.
pub type Changes = Vec<(String, String)>;

/// What to change.
#[derive(Debug, Clone)]
pub enum Edit {
    /// Set these keys to these colours (`#rrggbb`).
    Set(Vec<(String, String)>),
    /// Back to the theme's own colour: one key, or every key.
    Reset(Option<String>),
    /// Every suggestion from `palette check`, applied until none are left.
    FixContrast,
}

/// One colour of a theme, with what its author chose.
#[derive(Debug, Clone)]
pub struct Swatch {
    pub key: &'static str,
    pub value: String,
    pub original: String,
}

impl Swatch {
    pub fn edited(&self) -> bool {
        self.value != self.original
    }
}

/// A theme's colours as you see them, and the contrast report for them.
#[derive(Debug, Clone)]
pub struct Sheet {
    pub theme: String,
    pub mode: Mode,
    pub colours: Vec<Swatch>,
    pub report: Report,
}

/// Your edits in `dir`, as written. Unknown keys and bad values are skipped
/// when they are applied (`Colors::set`), never trusted.
pub fn read(dir: &Path) -> BTreeMap<String, String> {
    let file = dir.join(FILE);
    let Ok(meta) = fs::symlink_metadata(&file) else {
        return BTreeMap::new();
    };
    if !meta.is_file() || meta.len() > MAX_SIZE {
        return BTreeMap::new();
    }
    let Ok(table) = fs::read_to_string(&file)
        .unwrap_or_default()
        .parse::<toml::Table>()
    else {
        return BTreeMap::new();
    };
    table
        .into_iter()
        .filter_map(|(k, v)| Some((k, v.as_str()?.to_string())))
        .collect()
}

/// `palette colours <theme>`: every colour, its original, and the report.
pub fn sheet(paths: &Paths, name: &str) -> Result<Sheet> {
    let original = theme::load_original(paths, name)?;
    let now = theme::load(paths, name)?;
    let colours = now
        .colors
        .iter()
        .map(|(key, value)| Swatch {
            key,
            value: value.to_string(),
            original: original.colors.get(key).to_string(),
        })
        .collect();
    Ok(Sheet {
        theme: name.to_string(),
        mode: now.colors.mode,
        colours,
        report: check::theme(&now),
    })
}

/// Change a theme's colours. Returns the keys that changed, with their new
/// colour (the original's, for a reset). Only writes `edits.toml`; the
/// caller re-renders if the theme is active (`lib::edit_colours`).
pub fn apply(paths: &Paths, name: &str, edit: &Edit) -> Result<Changes> {
    let original = theme::load_original(paths, name)?;
    let mut edits = read(&original.dir);
    // What each key looks like now, to report only real changes.
    let now = theme::load(paths, name)?.colors;
    let wanted: Vec<(String, String)> = match edit {
        Edit::Set(changes) => {
            for (key, value) in changes {
                if theme::colour_key(key).is_none() {
                    bail!("`{key}` is not a theme colour (see `palette colours {name}`)");
                }
                if !theme::is_hex_colour(value) {
                    bail!("`{value}` is not a colour: use #rrggbb, like #7aa2f7");
                }
            }
            changes
                .iter()
                .map(|(k, v)| (k.clone(), v.to_ascii_lowercase()))
                .collect()
        }
        Edit::Reset(Some(key)) => {
            let Some(k) = theme::colour_key(key) else {
                bail!("`{key}` is not a theme colour (see `palette colours {name}`)");
            };
            vec![(key.clone(), original.colors.get(k).to_string())]
        }
        Edit::Reset(None) => edits
            .keys()
            .filter_map(|k| theme::colour_key(k))
            .map(|k| (k.to_string(), original.colors.get(k).to_string()))
            .collect(),
        Edit::FixContrast => fix_contrast(name, now.clone()),
    };

    let mut changed = Vec::new();
    for (key, value) in wanted {
        let k = theme::colour_key(&key).expect("checked above");
        if original.colors.get(k) == value {
            edits.remove(&key);
        } else {
            edits.insert(key.clone(), value.clone());
        }
        if now.get(k) != value {
            changed.push((key, value));
        }
    }
    if matches!(edit, Edit::Reset(None)) {
        edits.clear();
    }
    write(&original.dir, name, &edits)?;
    Ok(changed)
}

/// Apply the report's suggestions, check again, repeat: fixing `foreground`
/// for the background can leave it short on a panel, and the next pass
/// lifts it further. Each key moves only in its own hue.
fn fix_contrast(name: &str, mut colours: theme::Colors) -> Changes {
    let mut changed = BTreeMap::new();
    for _ in 0..6 {
        let report = check::colors(name, &colours);
        if report.issues.is_empty() {
            break;
        }
        let mut seen = Vec::new();
        for issue in &report.issues {
            if !seen.contains(&issue.fg) {
                seen.push(issue.fg);
                colours.set(issue.fg, &issue.suggested);
                changed.insert(issue.fg.to_string(), issue.suggested.clone());
            }
        }
    }
    changed.into_iter().collect()
}

/// Save `edits` (or remove the file when there are none), atomically.
fn write(dir: &Path, name: &str, edits: &BTreeMap<String, String>) -> Result<()> {
    let file = dir.join(FILE);
    if edits.is_empty() {
        match fs::remove_file(&file) {
            Err(e) if e.kind() != ErrorKind::NotFound => {
                return Err(e).with_context(|| format!("removing {}", file.display()));
            }
            _ => {}
        }
        // A built-in theme's folder that only held edits: tidy it away.
        let _ = fs::remove_dir(dir);
        return Ok(());
    }
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let mut text = format!(
        "# Your colour edits to {name}, laid over its own colours.\n\
         # Undo them in Palette's Settings → Colours, or `palette colours {name} --reset`.\n"
    );
    for (key, value) in edits {
        text.push_str(&format!("{key} = {}\n", toml::Value::String(value.clone())));
    }
    let tmp = dir.join(format!(".{FILE}.tmp"));
    fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, &file).with_context(|| format!("saving {}", file.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> (tempfile::TempDir, Paths) {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        (home, paths)
    }

    fn set(key: &str, value: &str) -> Edit {
        Edit::Set(vec![(key.into(), value.into())])
    }

    #[test]
    fn edits_lay_over_a_built_in_theme_and_reset() {
        let (_h, paths) = home();
        let changed = apply(&paths, "tokyo-night", &set("accent", "#FF0000")).unwrap();
        assert_eq!(changed, [("accent".to_string(), "#ff0000".to_string())]);
        assert_eq!(
            theme::load(&paths, "tokyo-night")
                .unwrap()
                .colors
                .get("accent"),
            "#ff0000"
        );
        // Still the built-in theme, not a copy.
        assert_eq!(
            theme::load_original(&paths, "tokyo-night")
                .unwrap()
                .colors
                .get("accent"),
            "#7aa2f7"
        );
        let sheet = sheet(&paths, "tokyo-night").unwrap();
        let accent = sheet.colours.iter().find(|s| s.key == "accent").unwrap();
        assert!(accent.edited());
        assert_eq!(sheet.colours.iter().filter(|s| s.edited()).count(), 1);

        apply(&paths, "tokyo-night", &Edit::Reset(None)).unwrap();
        assert_eq!(
            theme::load(&paths, "tokyo-night")
                .unwrap()
                .colors
                .get("accent"),
            "#7aa2f7"
        );
        // The folder only held edits, so it's gone again.
        assert!(!paths.themes().join("tokyo-night").exists());
    }

    #[test]
    fn setting_the_original_back_removes_the_line() {
        let (_h, paths) = home();
        apply(&paths, "nord", &set("red", "#123456")).unwrap();
        apply(&paths, "nord", &set("blue", "#654321")).unwrap();
        let red = theme::load_original(&paths, "nord")
            .unwrap()
            .colors
            .get("red")
            .to_string();
        apply(&paths, "nord", &set("red", &red)).unwrap();
        let edits = read(&paths.themes().join("nord"));
        assert_eq!(edits.keys().collect::<Vec<_>>(), ["blue"]);
        apply(&paths, "nord", &Edit::Reset(Some("blue".into()))).unwrap();
        assert!(read(&paths.themes().join("nord")).is_empty());
    }

    #[test]
    fn bad_keys_and_values_are_refused() {
        let (_h, paths) = home();
        assert!(apply(&paths, "nord", &set("sparkle", "#ffffff")).is_err());
        assert!(apply(&paths, "nord", &set("red", "red")).is_err());
        assert!(apply(&paths, "no-such-theme", &set("red", "#ffffff")).is_err());
        // A hand-written bad line is ignored, not trusted.
        let dir = paths.themes().join("nord");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(FILE), "red = \"nope\"\nsparkle = \"#ffffff\"\n").unwrap();
        let t = theme::load(&paths, "nord").unwrap();
        assert_eq!(
            t.colors.get("red"),
            theme::load_original(&paths, "nord")
                .unwrap()
                .colors
                .get("red")
        );
    }

    #[test]
    fn fix_contrast_clears_the_report() {
        let (_h, paths) = home();
        // Grey text on the dark background: far below 4.5.
        apply(&paths, "tokyo-night", &set("foreground", "#3b3d4a")).unwrap();
        assert!(
            !sheet(&paths, "tokyo-night")
                .unwrap()
                .report
                .issues
                .is_empty()
        );
        let changed = apply(&paths, "tokyo-night", &Edit::FixContrast).unwrap();
        assert!(changed.iter().any(|(k, _)| k == "foreground"));
        let after = sheet(&paths, "tokyo-night").unwrap();
        assert!(after.report.issues.is_empty(), "{:?}", after.report.issues);
    }
}
