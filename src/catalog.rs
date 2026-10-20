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
                })
            })
            .collect()
    })
}

pub fn find(name: &str) -> Option<&'static Entry> {
    entries().iter().find(|e| e.name == name)
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
