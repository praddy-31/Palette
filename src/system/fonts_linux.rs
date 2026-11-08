//! Installed fonts on Linux, from fontconfig (`fc-list`): the Linux stand-in
//! for `fonts.rs` (CoreText). Only `palette fonts` uses it there; the Bar page
//! that picks fonts is macOS only.

use super::run;

/// Every font family installed, sorted.
pub fn families() -> Vec<String> {
    let mut names: Vec<String> = run("fc-list", &[":", "family"])
        .unwrap_or_default()
        .lines()
        // `DejaVu Sans,DejaVu Sans Light`: the first name is the family's own.
        .filter_map(|l| l.split(',').next())
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    names
}

/// The styles a family has, sorted by name. Empty when it isn't installed.
pub fn styles(family: &str) -> Vec<String> {
    let mut out: Vec<String> = run("fc-list", &[family, "style"])
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.trim().strip_prefix("style="))
        .filter_map(|s| s.split(',').next())
        .map(str::to_string)
        .collect();
    out.sort();
    out.dedup();
    out
}
