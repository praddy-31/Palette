//! Themes: parsing and validating `colors.toml`, finding and listing themes.
//! The format is described in `docs/THEME_SPEC.md`.

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use crate::paths::Paths;

/// Every key a `colors.toml` must define (Omarchy's names).
pub const REQUIRED_KEYS: [&str; 23] = [
    "accent",
    "selection",
    "muted",
    "background",
    "dark_background",
    "darker_background",
    "lighter_background",
    "foreground",
    "dark_foreground",
    "light_foreground",
    "bright_foreground",
    "red",
    "yellow",
    "green",
    "cyan",
    "blue",
    "magenta",
    "bright_red",
    "bright_yellow",
    "bright_green",
    "bright_cyan",
    "bright_blue",
    "bright_magenta",
];

/// Keys some Omarchy themes leave out, each with the key it copies when
/// missing. In order: `brown` falls back to `orange`, which is filled first.
const OPTIONAL_KEYS: [(&str, &str); 2] = [("orange", "yellow"), ("brown", "orange")];

/// The 16 ANSI terminal colours, as keys into `colors.toml`.
const ANSI_KEYS: [&str; 16] = [
    "dark_background",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "foreground",
    "muted",
    "bright_red",
    "bright_green",
    "bright_yellow",
    "bright_blue",
    "bright_magenta",
    "bright_cyan",
    "bright_foreground",
];

/// Themes compiled into the binary, so a fresh install has something to pick.
const BUNDLED: &[(&str, &str)] = &[
    (
        "catppuccin-latte",
        include_str!("../themes/catppuccin-latte/colors.toml"),
    ),
    (
        "catppuccin-mocha",
        include_str!("../themes/catppuccin-mocha/colors.toml"),
    ),
    ("ethereal", include_str!("../themes/ethereal/colors.toml")),
    (
        "everforest",
        include_str!("../themes/everforest/colors.toml"),
    ),
    (
        "flexoki-light",
        include_str!("../themes/flexoki-light/colors.toml"),
    ),
    (
        "gruvbox-dark",
        include_str!("../themes/gruvbox-dark/colors.toml"),
    ),
    ("hackerman", include_str!("../themes/hackerman/colors.toml")),
    ("kanagawa", include_str!("../themes/kanagawa/colors.toml")),
    (
        "last-horizon",
        include_str!("../themes/last-horizon/colors.toml"),
    ),
    ("lumon", include_str!("../themes/lumon/colors.toml")),
    ("lupine", include_str!("../themes/lupine/colors.toml")),
    (
        "matte-black",
        include_str!("../themes/matte-black/colors.toml"),
    ),
    ("miasma", include_str!("../themes/miasma/colors.toml")),
    ("nord", include_str!("../themes/nord/colors.toml")),
    (
        "osaka-jade",
        include_str!("../themes/osaka-jade/colors.toml"),
    ),
    ("retro-82", include_str!("../themes/retro-82/colors.toml")),
    ("ristretto", include_str!("../themes/ristretto/colors.toml")),
    (
        "rose-pine-dawn",
        include_str!("../themes/rose-pine-dawn/colors.toml"),
    ),
    ("solitude", include_str!("../themes/solitude/colors.toml")),
    ("starship", include_str!("../themes/starship/colors.toml")),
    (
        "tokyo-night",
        include_str!("../themes/tokyo-night/colors.toml"),
    ),
    (
        "vantablack",
        include_str!("../themes/vantablack/colors.toml"),
    ),
    ("white", include_str!("../themes/white/colors.toml")),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dark,
    Light,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Dark => "dark",
            Mode::Light => "light",
        }
    }
}

/// A validated `colors.toml`. Every required key is present and `#rrggbb`.
#[derive(Debug, Clone)]
pub struct Colors {
    values: BTreeMap<&'static str, String>,
    pub mode: Mode,
    /// Non-fatal problems, such as unknown keys.
    pub warnings: Vec<String>,
}

impl Colors {
    /// Parse and validate. On failure, returns every problem found, not just the first.
    pub fn parse(src: &str) -> Result<Colors, Vec<String>> {
        let table: toml::Table = src
            .parse()
            .map_err(|e: toml::de::Error| vec![format!("not valid TOML: {}", e.message())])?;

        let mut problems = Vec::new();
        let mut values = BTreeMap::new();
        for key in REQUIRED_KEYS {
            match table.get(key) {
                None => problems.push(format!("missing key `{key}`")),
                Some(toml::Value::String(s)) if is_hex_colour(s) => {
                    values.insert(key, s.to_ascii_lowercase());
                }
                Some(other) => problems.push(format!("`{key}` = {other} is not #RRGGBB")),
            }
        }
        for (key, fallback) in OPTIONAL_KEYS {
            match table.get(key) {
                Some(toml::Value::String(s)) if is_hex_colour(s) => {
                    values.insert(key, s.to_ascii_lowercase());
                }
                Some(other) => problems.push(format!("`{key}` = {other} is not #RRGGBB")),
                None => {
                    if let Some(v) = values.get(fallback).cloned() {
                        values.insert(key, v);
                    }
                }
            }
        }

        let mode = match table.get("mode") {
            None => Mode::Dark,
            Some(toml::Value::String(s)) if s == "dark" => Mode::Dark,
            Some(toml::Value::String(s)) if s == "light" => Mode::Light,
            Some(other) => {
                problems.push(format!("`mode` = {other} must be \"dark\" or \"light\""));
                Mode::Dark
            }
        };

        if !problems.is_empty() {
            return Err(problems);
        }

        let warnings = table
            .keys()
            .filter(|k| *k != "mode" && !values.contains_key(k.as_str()))
            .map(|k| format!("unknown key `{k}` ignored"))
            .collect();

        Ok(Colors {
            values,
            mode,
            warnings,
        })
    }

    /// `#rrggbb` for a known key (required or optional, filled in).
    pub fn get(&self, key: &str) -> &str {
        &self.values[key]
    }

    /// Change one colour (`edits.toml`). `false` if `key` isn't a theme
    /// colour or `value` isn't `#rrggbb`.
    pub fn set(&mut self, key: &str, value: &str) -> bool {
        match colour_key(key) {
            Some(k) if is_hex_colour(value) => {
                self.values.insert(k, value.to_ascii_lowercase());
                true
            }
            _ => false,
        }
    }

    /// All keys and values, sorted by key.
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &str)> {
        self.values.iter().map(|(k, v)| (*k, v.as_str()))
    }

    /// The 16 ANSI colours derived from the semantic keys.
    pub fn ansi(&self) -> [&str; 16] {
        ANSI_KEYS.map(|k| self.get(k))
    }
}

/// A theme colour's name as the `'static` key `Colors` uses (required or
/// optional), or `None` for anything else.
pub fn colour_key(key: &str) -> Option<&'static str> {
    REQUIRED_KEYS
        .iter()
        .copied()
        .chain(OPTIONAL_KEYS.iter().map(|(k, _)| *k))
        .find(|k| *k == key)
}

/// `#` followed by exactly six hex digits.
pub fn is_hex_colour(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Theme names double as folder names, so keep them boring: `[a-z0-9-]+`.
/// This also stops `palette set ../../something` escaping the themes folder.
pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Bundled,
    User,
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub origin: Origin,
    /// `themes/<name>/`. Holds overrides and wallpapers. May not exist; a
    /// bundled theme gets one when you add wallpapers to it.
    pub dir: PathBuf,
    pub colors: Colors,
}

/// Wallpaper file extensions macOS can show.
const IMAGE_EXTENSIONS: [&str; 6] = ["png", "jpg", "jpeg", "heic", "webp", "tiff"];

pub fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

impl Theme {
    /// `themes/<name>/backgrounds/`.
    pub fn backgrounds_dir(&self) -> PathBuf {
        self.dir.join("backgrounds")
    }

    /// The theme's wallpapers, sorted by file name. Empty if it has none.
    pub fn backgrounds(&self) -> Result<Vec<PathBuf>> {
        let dir = self.backgrounds_dir();
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e).with_context(|| format!("reading {}", dir.display())),
        };
        let mut images = Vec::new();
        for entry in entries {
            let entry = entry?;
            // Regular files only, for the same reason as static overrides.
            if entry.file_type()?.is_file() && is_image(&entry.path()) {
                images.push(entry.path());
            }
        }
        images.sort();
        Ok(images)
    }
}

/// Load a theme by name, with your colour edits (`edits.toml`) on top.
pub fn load(paths: &Paths, name: &str) -> Result<Theme> {
    let mut theme = load_original(paths, name)?;
    for (key, value) in crate::edits::read(&theme.dir) {
        theme.colors.set(&key, &value);
    }
    Ok(theme)
}

/// Load a theme as its author made it, without your edits. A user theme (a
/// folder with `colors.toml`) shadows a bundled one with the same name. A
/// folder without `colors.toml` only adds wallpapers (and edits) to the
/// bundled theme of that name.
pub fn load_original(paths: &Paths, name: &str) -> Result<Theme> {
    if !is_valid_name(name) {
        bail!("invalid theme name `{name}`: use lowercase letters, digits and `-`");
    }

    let dir = paths.themes().join(name);
    let file = dir.join("colors.toml");
    match fs::read_to_string(&file) {
        Ok(src) => {
            let mut colors =
                Colors::parse(&src).map_err(|p| invalid(name, &file.display().to_string(), p))?;
            if dir.join("light.mode").exists() {
                colors.mode = Mode::Light;
            }
            return Ok(Theme {
                name: name.to_string(),
                origin: Origin::User,
                dir,
                colors,
            });
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(e).with_context(|| format!("reading {}", file.display())),
    }

    let Some((_, src)) = BUNDLED.iter().find(|(n, _)| *n == name) else {
        bail!("no theme named `{name}`. Run `palette list` to see the available themes");
    };
    let colors = Colors::parse(src).map_err(|p| invalid(name, "bundled", p))?;
    Ok(Theme {
        name: name.to_string(),
        origin: Origin::Bundled,
        dir,
        colors,
    })
}

fn invalid(name: &str, location: &str, problems: Vec<String>) -> anyhow::Error {
    anyhow!(
        "theme `{name}` is invalid ({location}):\n  - {}",
        problems.join("\n  - ")
    )
}

/// All theme names, sorted, with where each comes from. Contents are not
/// validated here; `load` does that.
pub fn list(paths: &Paths) -> Result<Vec<(String, Origin)>> {
    let mut found: BTreeMap<String, Origin> = BUNDLED
        .iter()
        .map(|(n, _)| (n.to_string(), Origin::Bundled))
        .collect();

    let themes_dir = paths.themes();
    let entries = match fs::read_dir(&themes_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(found.into_iter().collect()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", themes_dir.display())),
    };
    for entry in entries {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if is_valid_name(&name) && entry.path().join("colors.toml").is_file() {
            found.insert(name, Origin::User);
        }
    }
    Ok(found.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKYO: &str = include_str!("../themes/tokyo-night/colors.toml");

    #[test]
    fn bundled_themes_are_valid() {
        for (name, src) in BUNDLED {
            let colors = Colors::parse(src).unwrap_or_else(|p| panic!("{name}: {p:?}"));
            assert!(colors.warnings.is_empty(), "{name}: {:?}", colors.warnings);
        }
    }

    #[test]
    fn parses_tokyo_night() {
        let c = Colors::parse(TOKYO).unwrap();
        assert_eq!(c.get("accent"), "#7aa2f7");
        assert_eq!(c.mode, Mode::Dark);
        let ansi = c.ansi();
        assert_eq!(ansi[0], "#13141c");
        assert_eq!(ansi[8], "#414868");
        assert_eq!(ansi[15], "#c0caf5");
    }

    #[test]
    fn orange_and_brown_are_optional() {
        let src = TOKYO
            .replace("orange = \"#eb927b\"\n", "")
            .replace("brown = ", "unused = ");
        let c = Colors::parse(&src).unwrap();
        assert_eq!(c.get("orange"), c.get("yellow"));
        assert_eq!(c.get("brown"), c.get("yellow"));
        assert_eq!(c.warnings, ["unknown key `unused` ignored"]);
    }

    #[test]
    fn reports_every_problem() {
        let src = TOKYO
            .replace("accent = \"#7aa2f7\"", "accent = \"#7aa2f\"")
            .replace("red = \"#f7768e\"\n", "")
            .replace("blue = \"#7aa2f7\"", "blue = 7");
        let problems = Colors::parse(&src).unwrap_err();
        assert_eq!(
            problems,
            [
                "`accent` = \"#7aa2f\" is not #RRGGBB",
                "missing key `red`",
                "`blue` = 7 is not #RRGGBB",
            ]
        );
    }

    #[test]
    fn rejects_bad_mode_and_bad_toml() {
        let src = TOKYO.replace("mode = \"dark\"", "mode = \"dim\"");
        assert!(Colors::parse(&src).unwrap_err()[0].contains("`mode`"));
        assert!(Colors::parse("accent = ").unwrap_err()[0].starts_with("not valid TOML"));
    }

    #[test]
    fn unknown_keys_warn_and_case_is_normalised() {
        let src = format!(
            "{}\nsparkle = \"#ffffff\"\n",
            TOKYO.replace("#7aa2f7", "#7AA2F7")
        );
        let c = Colors::parse(&src).unwrap();
        assert_eq!(c.get("accent"), "#7aa2f7");
        assert_eq!(c.warnings, ["unknown key `sparkle` ignored"]);
    }

    #[test]
    fn names() {
        assert!(is_valid_name("tokyo-night"));
        assert!(is_valid_name("gruvbox2"));
        for bad in ["", "../x", "Tokyo", "a/b", "-x", "a b", "."] {
            assert!(!is_valid_name(bad), "{bad:?}");
        }
    }
}
