//! KDE Plasma: the whole desktop's colours (panels, windows, Breeze title
//! bars, Qt apps) from one generated colour scheme. Light or dark comes with
//! it: a light theme gives a light scheme.
//!
//! KDE only reads schemes from `~/.local/share/color-schemes/`, so on every
//! switch Palette copies its file there as `PaletteA.colors` or
//! `PaletteB.colors` (its own two files, nothing of yours) and applies the one
//! that isn't active: `plasma-apply-colorscheme` does nothing when asked for
//! the scheme already in use, even if the file changed. The scheme you had
//! before is remembered and put back when Palette stops theming KDE.

use std::fs;
use std::path::PathBuf;

use super::{BASE, Output, Platforms, Target};
use crate::paths::Paths;
use crate::system::session::Session;
use crate::system::{self, Outcome};
use crate::wiring::Wiring;

pub const TARGET: Target = Target {
    name: "kde",
    output: Some(Output {
        file: "kde.colors",
        template: include_str!("template.colors"),
        overridable: false,
        finish: None,
    }),
    wiring: Wiring::Direct,
    installed: |_| super::has_program("plasmashell"),
    reload,
    unload: Some(unload),
    platforms: Platforms::Linux,
    ..BASE
};

const NAMES: [&str; 2] = ["PaletteA", "PaletteB"];

fn schemes_dir(paths: &Paths) -> PathBuf {
    paths.home().join(".local/share/color-schemes")
}

/// The scheme KDE has now: `[General] ColorScheme=` in `kdeglobals`.
pub fn active_scheme(paths: &Paths) -> Option<String> {
    let text = fs::read_to_string(paths.config_home().join("kdeglobals")).ok()?;
    let mut in_general = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_general = line == "[General]";
        } else if in_general && let Some(v) = line.strip_prefix("ColorScheme=") {
            return Some(v.trim().to_string());
        }
    }
    None
}

/// Which of Palette's two names to use next: the one not active now.
pub fn next_name(active: Option<&str>) -> &'static str {
    match active {
        Some("PaletteA") => "PaletteB",
        _ => "PaletteA",
    }
}

/// Where the scheme you had before Palette is remembered.
fn previous_file(paths: &Paths) -> PathBuf {
    paths.root().join("kde-previous-scheme")
}

/// The scheme file for `name`: Palette's rendering with that name inside.
pub fn scheme_text(rendered: &str, name: &str) -> String {
    rendered
        .lines()
        .map(|l| match l {
            "ColorScheme=Palette" => format!("ColorScheme={name}"),
            "Name=Palette" => format!("Name={name}"),
            other => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn reload(paths: &Paths) -> Outcome {
    if Session::detect() != Session::Kde {
        return Outcome::Skipped("not the desktop you're logged into".into());
    }
    match apply(paths) {
        Ok(name) => Outcome::Done(format!("colour scheme {name}")),
        Err(e) => Outcome::Failed(e),
    }
}

fn apply(paths: &Paths) -> Result<&'static str, String> {
    let rendered = fs::read_to_string(paths.current().join("kde.colors"))
        .map_err(|e| format!("reading kde.colors: {e}"))?;
    let active = active_scheme(paths);
    if let Some(mine) = active.as_deref().filter(|a| !NAMES.contains(a)) {
        fs::create_dir_all(paths.root()).map_err(|e| e.to_string())?;
        fs::write(previous_file(paths), mine).map_err(|e| e.to_string())?;
    }
    let name = next_name(active.as_deref());
    let dir = schemes_dir(paths);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(
        dir.join(format!("{name}.colors")),
        scheme_text(&rendered, name),
    )
    .map_err(|e| e.to_string())?;
    system::run_desktop("plasma-apply-colorscheme", &[name])?;
    Ok(name)
}

/// Put your own scheme back (Breeze if Palette never saw one).
fn unload(paths: &Paths) -> Outcome {
    if Session::detect() != Session::Kde {
        return Outcome::Skipped("not the desktop you're logged into".into());
    }
    let previous = fs::read_to_string(previous_file(paths))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "BreezeDark".into());
    match system::run_desktop("plasma-apply-colorscheme", &[&previous]) {
        Ok(_) => Outcome::Done(format!("back to {previous}")),
        Err(e) => Outcome::Failed(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alternates_between_two_names() {
        assert_eq!(next_name(None), "PaletteA");
        assert_eq!(next_name(Some("BreezeDark")), "PaletteA");
        assert_eq!(next_name(Some("PaletteA")), "PaletteB");
        assert_eq!(next_name(Some("PaletteB")), "PaletteA");
    }

    #[test]
    fn reads_the_active_scheme() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        fs::create_dir_all(paths.config_home()).unwrap();
        fs::write(
            paths.config_home().join("kdeglobals"),
            "[KDE]\nColorScheme=Nope\n\n[General]\nColorScheme=BreezeDark\nfont=x\n",
        )
        .unwrap();
        assert_eq!(active_scheme(&paths).as_deref(), Some("BreezeDark"));
    }

    #[test]
    fn names_the_scheme_inside_the_file() {
        let text = scheme_text("[General]\nColorScheme=Palette\nName=Palette\n", "PaletteB");
        assert_eq!(text, "[General]\nColorScheme=PaletteB\nName=PaletteB\n");
    }
}
