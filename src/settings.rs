//! `~/.config/palette/settings.toml`: preferences. The overlay hotkey and
//! which apps Palette themes (`[apps]`, all on unless
//! set to `false`). Plain TOML so it can be edited by hand; `palette hotkey`
//! and `palette apps` edit it from the command line.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::ErrorKind;

use anyhow::{Context, Result, bail};

use crate::paths::Paths;

/// alt-shift-t: free in the owner's AeroSpace config (alt-based bindings).
pub const DEFAULT_HOTKEY: &str = "alt+shift+t";

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// Normalised: lowercase modifiers in a fixed order, then the key (`alt+shift+t`).
    pub hotkey: String,
    /// Apps the user keeps on their own look (target names, `wallpaper`, `appearance`).
    pub off: BTreeSet<String>,
    /// The app's welcome tour has been seen (it opens on first launch until then).
    pub welcomed: bool,
    /// Theme name → the wallpaper (file name) it starts with (`palette bg default`).
    pub wallpapers: BTreeMap<String, String>,
    /// Obsidian: an accent ring around the active pane when the editor is split.
    pub obsidian_ring: bool,
    /// Obsidian: fade the other panes a little.
    pub obsidian_dim: bool,
    /// Day and night themes on a schedule (`palette schedule`).
    pub schedule: crate::schedule::Schedule,
    /// Favourite themes, in the order added (at most ten; `palette fav`).
    pub favourites: Vec<String>,
    /// What plain `palette next`/`prev` and the bar pill step through.
    pub next_cycles: crate::favourites::Cycle,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: DEFAULT_HOTKEY.into(),
            off: BTreeSet::new(),
            welcomed: false,
            wallpapers: BTreeMap::new(),
            obsidian_ring: true,
            obsidian_dim: true,
            schedule: Default::default(),
            favourites: Vec::new(),
            next_cycles: crate::favourites::Cycle::All,
        }
    }
}

impl Settings {
    /// Does Palette theme this app?
    pub fn is_on(&self, app: &str) -> bool {
        !self.off.contains(app)
    }
}

fn file(paths: &Paths) -> std::path::PathBuf {
    paths.root().join("settings.toml")
}

pub fn load(paths: &Paths) -> Result<Settings> {
    let path = file(paths);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Settings::default()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let table: toml::Table = text
        .parse()
        .with_context(|| format!("{} is not valid TOML", path.display()))?;
    let hotkey = match table.get("hotkey").and_then(|v| v.as_str()) {
        Some(h) => normalise_hotkey(h)?,
        None => DEFAULT_HOTKEY.into(),
    };
    let off = table
        .get("apps")
        .and_then(|v| v.as_table())
        .map(|apps| {
            apps.iter()
                .filter(|(_, v)| v.as_bool() == Some(false))
                .map(|(k, _)| k.clone())
                .collect()
        })
        .unwrap_or_default();
    let wallpapers = table
        .get("wallpapers")
        .and_then(|v| v.as_table())
        .map(|t| {
            t.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let obsidian = table.get("obsidian").and_then(|v| v.as_table());
    let flag = |key: &str| {
        obsidian
            .and_then(|t| t.get(key))
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
    };
    let (obsidian_ring, obsidian_dim) = (flag("focus_ring"), flag("dim_inactive"));
    let welcomed = table
        .get("welcomed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let favourites = table
        .get("favourites")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .take(crate::favourites::MAX_FAVOURITES)
                .collect()
        })
        .unwrap_or_default();
    let next_cycles = match table.get("next_cycles").and_then(|v| v.as_str()) {
        Some(c) => crate::favourites::Cycle::parse(c)?,
        None => crate::favourites::Cycle::All,
    };
    Ok(Settings {
        hotkey,
        off,
        welcomed,
        wallpapers,
        obsidian_ring,
        obsidian_dim,
        schedule: crate::schedule::from_toml(table.get("schedule")),
        favourites,
        next_cycles,
    })
}

pub fn save(paths: &Paths, settings: &Settings) -> Result<()> {
    let path = file(paths);
    fs::create_dir_all(paths.root())?;
    let mut text = format!(
        "# Palette settings (edit freely; `palette hotkey` and `palette apps` change them)\n\
         hotkey = \"{}\"\n\
         welcomed = {}\n\
         # Favourite themes (`palette fav`); what `palette next` steps through: \"all\" or \"favourites\"\n\
         favourites = {}\n\
         next_cycles = \"{}\"\n\n\
         # Apps Palette leaves on their own look (everything else is themed)\n\
         [apps]\n",
        settings.hotkey,
        settings.welcomed,
        toml::Value::Array(
            settings
                .favourites
                .iter()
                .map(|f| toml::Value::String(f.clone()))
                .collect()
        ),
        settings.next_cycles.as_str()
    );
    for app in &settings.off {
        text.push_str(&format!("{app} = false\n"));
    }
    if !settings.wallpapers.is_empty() {
        text.push_str(
            "\n# The wallpaper each theme starts with (`palette bg default`)\n[wallpapers]\n",
        );
        for (theme, file) in &settings.wallpapers {
            let quoted = toml::Value::String(theme.clone()).to_string();
            let bare = theme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            let key = if bare { theme } else { &quoted };
            text.push_str(&format!("{key} = {}\n", toml::Value::String(file.clone())));
        }
    }
    text.push_str(&format!(
        "\n# Obsidian: accent ring around the active pane, fade the others (`palette obsidian`)\n\
         [obsidian]\nfocus_ring = {}\ndim_inactive = {}\n",
        settings.obsidian_ring, settings.obsidian_dim
    ));
    text.push_str(&crate::schedule::to_toml(&settings.schedule));
    fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
}

const MODIFIERS: [(&str, &[&str]); 4] = [
    ("ctrl", &["ctrl", "control"]),
    ("alt", &["alt", "option", "opt"]),
    ("shift", &["shift"]),
    ("cmd", &["cmd", "command", "super", "meta"]),
];

/// `Option-Shift-T`, `alt+shift+t`, `shift + alt + T` → `alt+shift+t`.
/// At least one modifier and exactly one key: a letter, digit, F1–F20,
/// `space`, or an arrow.
pub fn normalise_hotkey(input: &str) -> Result<String> {
    let parts: Vec<String> = input
        .split(['+', '-'])
        .map(|p| p.trim().to_ascii_lowercase())
        .filter(|p| !p.is_empty())
        .collect();
    let mut mods = Vec::new();
    let mut key = None;
    for part in &parts {
        match MODIFIERS
            .iter()
            .find(|(_, names)| names.contains(&part.as_str()))
        {
            Some((name, _)) if !mods.contains(name) => mods.push(*name),
            Some(_) => bail!("`{input}`: {part} appears twice"),
            None if key.is_none() => key = Some(part.clone()),
            None => bail!("`{input}`: only one key besides the modifiers"),
        }
    }
    let Some(key) = key else {
        bail!("`{input}`: needs a key, e.g. alt+shift+t");
    };
    let valid_key = (key.len() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric()))
        || ["space", "up", "down", "left", "right", "tab", "enter"].contains(&key.as_str())
        || key
            .strip_prefix('f')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=20).contains(&n));
    if !valid_key {
        bail!(
            "`{input}`: `{key}` isn't a key Palette can bind (letters, digits, F1-F20, space, arrows)"
        );
    }
    if mods.is_empty() {
        bail!("`{input}`: needs at least one modifier (alt, shift, ctrl, cmd)");
    }
    mods.sort_by_key(|m| MODIFIERS.iter().position(|(n, _)| n == m));
    Ok(format!("{}+{key}", mods.join("+")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkeys() {
        assert_eq!(normalise_hotkey("Option-Shift-T").unwrap(), "alt+shift+t");
        assert_eq!(normalise_hotkey("shift + alt + T").unwrap(), "alt+shift+t");
        assert_eq!(
            normalise_hotkey("cmd+ctrl+space").unwrap(),
            "ctrl+cmd+space"
        );
        assert_eq!(normalise_hotkey("alt+f12").unwrap(), "alt+f12");
        assert!(normalise_hotkey("t").is_err());
        assert!(normalise_hotkey("alt+shift").is_err());
        assert!(normalise_hotkey("alt+a+b").is_err());
        assert!(normalise_hotkey("alt+alt+t").is_err());
        assert!(normalise_hotkey("alt+é").is_err());
    }

    #[test]
    fn load_and_save() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        assert_eq!(load(&paths).unwrap(), Settings::default());
        let mut s = Settings {
            hotkey: "ctrl+cmd+p".into(),
            ..Settings::default()
        };
        s.off.insert("sketchybar".into());
        s.obsidian_dim = false;
        s.favourites = vec!["nord".into(), "tokyo-night".into()];
        s.next_cycles = crate::favourites::Cycle::Favourites;
        save(&paths, &s).unwrap();
        let back = load(&paths).unwrap();
        assert_eq!(back, s);
        assert!(!back.is_on("sketchybar") && back.is_on("ghostty"));
    }

    /// Palette is menu-bar-only: an old `dock = true` still loads, and the next save drops it.
    #[test]
    fn old_dock_setting_is_ignored() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        fs::create_dir_all(paths.root()).unwrap();
        fs::write(file(&paths), "hotkey = \"alt+shift+w\"\ndock = true\n").unwrap();
        let s = load(&paths).unwrap();
        assert_eq!(s.hotkey, "alt+shift+w");
        save(&paths, &s).unwrap();
        assert!(!fs::read_to_string(file(&paths)).unwrap().contains("dock"));
    }
}
