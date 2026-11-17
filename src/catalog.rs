//! The curated list of community themes (`src/catalog.toml`, Omarchy's
//! "Extra themes"). Colours are baked in, so browsing needs no network;
//! `palette install <name>` downloads the repo.

use std::sync::OnceLock;

use crate::paths::Paths;
use crate::theme::{Colors, Origin, Theme};

pub struct Entry {
    pub name: String,
    pub title: String,
    pub url: String,
    pub author: String,
    /// Completed (missing keys filled in), ready for previews and analysis.
    pub colors: Colors,
    /// Where the roles came from: `None` = the author named them; `colors16` / `alacritty` =
    /// only terminal colours, so roles like `dark_background` were guessed.
    pub derived: Option<String>,
}

impl Entry {
    /// A stand-in theme for previews and `colour::analyse`.
    pub fn theme(&self, paths: &Paths) -> Theme {
        Theme {
            name: self.name.clone(),
            origin: Origin::Bundled,
            dir: paths.themes().join(&self.name),
            colors: self.colors.clone(),
        }
    }
}

pub fn entries() -> &'static [Entry] {
    static ENTRIES: OnceLock<Vec<Entry>> = OnceLock::new();
    ENTRIES.get_or_init(|| {
        let table: toml::Table = include_str!("catalog.toml")
            .parse()
            .expect("src/catalog.toml is valid TOML");
        let Some(themes) = table.get("theme").and_then(|t| t.as_array()) else {
            return Vec::new();
        };
        themes
            .iter()
            .filter_map(|t| {
                let s = |k: &str| t.get(k)?.as_str().map(str::to_string);
                let colors = t.get("colors")?.as_table()?;
                let light = s("mode").as_deref() == Some("light");
                let (text, _) = crate::install::complete(colors, light).ok()?;
                Some(Entry {
                    name: s("name")?,
                    title: s("title").unwrap_or_default(),
                    url: s("url")?,
                    author: s("author").unwrap_or_default(),
                    colors: Colors::parse(&text).ok()?,
                    derived: s("derived"),
                })
            })
            .collect()
    })
}

pub fn find(name: &str) -> Option<&'static Entry> {
    entries().iter().find(|e| e.name == name)
}

// ---------------------------------------------------------------- Rex's themes

/// The author shown for themes that come with the Rex app.
pub const REX: &str = "Rex";

/// Rex's own themes file, inside the installed app (never copied into Palette: the
/// themes are Rex's). `/Applications` is left out in a fake home.
pub fn rex_file(paths: &Paths) -> Option<std::path::PathBuf> {
    const INSIDE: &str =
        "Contents/Resources/SuperUI_SuperDesignTokens.bundle/Contents/Resources/themes.json";
    let mut dirs = vec![paths.home().join("Applications")];
    if !paths.is_fake() {
        dirs.insert(0, "/Applications".into());
    }
    dirs.iter()
        .flat_map(|d| ["Rex.app", "Rex Beta.app"].map(|app| d.join(app).join(INSIDE)))
        .find(|f| f.is_file())
}

/// One Rex theme as Palette colour keys (the `color0`…`color15` layout `complete` reads),
/// and whether it's light. Rex's cursor colour is each theme's signature colour: the accent.
fn rex_table(theme: &crate::fetch::json::Value) -> Option<(toml::Table, bool)> {
    let source = theme.get("source")?;
    let s = |k: &str| {
        source
            .get(k)?
            .as_str()
            .map(|v| toml::Value::String(v.into()))
    };
    let mut t = toml::Table::new();
    for (from, to) in [
        ("background", "background"),
        ("foreground", "foreground"),
        ("selectionBackground", "selection"),
        ("cursor", "accent"),
    ] {
        if let Some(v) = s(from) {
            t.insert(to.into(), v);
        }
    }
    for (i, c) in source.get("ansi")?.as_array()?.iter().enumerate().take(16) {
        t.insert(format!("color{i}"), toml::Value::String(c.as_str()?.into()));
    }
    let light = theme.get("appearance").and_then(|v| v.as_str()) == Some("light");
    Some((t, light))
}

/// Rex's themes on this Mac (none without Rex): `(id, title, colours, light)`.
pub fn rex_raw(paths: &Paths) -> Vec<(String, String, toml::Table, bool)> {
    let Some(text) = rex_file(paths).and_then(|f| std::fs::read_to_string(f).ok()) else {
        return Vec::new();
    };
    let Some(list) = crate::fetch::json::parse(&text) else {
        return Vec::new();
    };
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|theme| {
            let id = theme.get("id")?.as_str()?.to_string();
            let title = theme.get("name")?.as_str().unwrap_or(&id).to_string();
            let (table, light) = rex_table(theme)?;
            crate::theme::is_valid_name(&id).then_some((id, title, table, light))
        })
        .collect()
}

/// Rex's themes as catalogue entries (`url` = `rex:<id>`), for Get themes and `palette
/// catalog`. Read from the app each time: it's ~2.5 MB of JSON, ~40 themes.
pub fn rex_entries(paths: &Paths) -> Vec<Entry> {
    rex_raw(paths)
        .into_iter()
        .filter_map(|(id, title, table, light)| {
            let (text, _) = crate::install::complete(&table, light).ok()?;
            Some(Entry {
                url: format!("rex:{id}"),
                name: id,
                title,
                author: REX.into(),
                colors: Colors::parse(&text).ok()?,
                derived: Some("colors16".into()),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_is_usable_and_unique() {
        let raw: toml::Table = include_str!("catalog.toml").parse().unwrap();
        let count = raw["theme"].as_array().unwrap().len();
        assert_eq!(
            entries().len(),
            count,
            "an entry failed to complete or parse"
        );
        let mut names: Vec<&str> = entries().iter().map(|e| e.name.as_str()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), count, "duplicate names");
        for e in entries() {
            assert!(crate::theme::is_valid_name(&e.name), "{}", e.name);
            assert!(e.url.starts_with("https://github.com/"), "{}", e.url);
        }
    }
}
