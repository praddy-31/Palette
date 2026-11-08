//! GNOME Terminal (Ubuntu's terminal). Its settings live in dconf, GNOME's
//! settings store, one profile per folder. Palette makes its own profile,
//! "Palette", once: a copy of your default profile (font and all), added to
//! the list and made the default. Each switch writes only its colour keys
//! (`dconf write`), and open windows change at once. Your own profiles are
//! never touched; the default you had comes back when Palette stops theming
//! GNOME Terminal, and the Palette profile is removed.

use std::fs;
use std::path::PathBuf;

use super::{BASE, Output, Platforms, Target};
use crate::paths::Paths;
use crate::system::{self, Outcome};
use crate::wiring::Wiring;

pub const TARGET: Target = Target {
    name: "gnome-terminal",
    output: Some(Output {
        file: "gnome-terminal.dconf",
        template: include_str!("template.dconf"),
        overridable: false,
        finish: None,
    }),
    wiring: Wiring::Direct,
    installed: |_| super::has_program("gnome-terminal"),
    reload,
    unload: Some(unload),
    platforms: Platforms::Linux,
    ..BASE
};

/// Palette's profile id (GNOME Terminal names profiles by UUID).
pub const UUID: &str = "9a1e77e0-5a1e-4e77-9a1e-a1e77e0b0b0b";
const SCHEMA: &str = "org.gnome.Terminal.ProfilesList";

fn folder(uuid: &str) -> String {
    format!("/org/gnome/terminal/legacy/profiles:/:{uuid}/")
}

/// Where the default profile you had before Palette is remembered.
fn previous_file(paths: &Paths) -> PathBuf {
    paths.root().join("gnome-terminal-previous-profile")
}

fn gsettings_get(key: &str) -> Result<String, String> {
    system::run_desktop("gsettings", &["get", SCHEMA, key])
}

fn gsettings_set(key: &str, value: &str) -> Result<(), String> {
    system::run_desktop("gsettings", &["set", SCHEMA, key, value]).map(|_| ())
}

/// Write `key=value` lines (dconf's own format) into a profile folder.
fn write_keys(folder: &str, text: &str) -> Result<(), String> {
    for (key, value) in keys(text) {
        system::run_desktop("dconf", &["write", &format!("{folder}{key}"), &value])?;
    }
    Ok(())
}

fn reload(paths: &Paths) -> Outcome {
    match apply(paths) {
        Ok(true) => Outcome::Done("made the Palette profile your default".into()),
        Ok(false) => Outcome::Done("Palette profile".into()),
        Err(e) => Outcome::Failed(e),
    }
}

/// True when the profile was just made.
fn apply(paths: &Paths) -> Result<bool, String> {
    let colours = fs::read_to_string(paths.current().join("gnome-terminal.dconf"))
        .map_err(|e| format!("reading gnome-terminal.dconf: {e}"))?;
    let mut list = parse_list(&gsettings_get("list")?);
    let made = !list.iter().any(|p| p == UUID);
    if made {
        let previous = unquote(&gsettings_get("default")?);
        fs::create_dir_all(paths.root()).map_err(|e| e.to_string())?;
        fs::write(previous_file(paths), &previous).map_err(|e| e.to_string())?;
        if !previous.is_empty() {
            let yours = system::run_desktop("dconf", &["dump", &folder(&previous)])?;
            write_keys(&folder(UUID), &yours)?;
        }
        write_keys(&folder(UUID), "visible-name='Palette'")?;
        list.push(UUID.to_string());
        gsettings_set("list", &format_list(&list))?;
        gsettings_set("default", &format!("'{UUID}'"))?;
    }
    write_keys(&folder(UUID), &colours)?;
    Ok(made)
}

/// Your default profile back; the Palette profile removed.
fn unload(paths: &Paths) -> Outcome {
    let run = || -> Result<String, String> {
        let mut list = parse_list(&gsettings_get("list")?);
        list.retain(|p| p != UUID);
        let previous = fs::read_to_string(previous_file(paths)).unwrap_or_default();
        let previous = previous.trim();
        let back = if list.iter().any(|p| p == previous) {
            previous.to_string()
        } else {
            list.first().cloned().unwrap_or_default()
        };
        if !list.is_empty() {
            gsettings_set("list", &format_list(&list))?;
        }
        if !back.is_empty() {
            gsettings_set("default", &format!("'{back}'"))?;
        }
        system::run_desktop("dconf", &["reset", "-f", &folder(UUID)])?;
        Ok(back)
    };
    match run() {
        Ok(back) => Outcome::Done(format!("back to profile {back}")),
        Err(e) => Outcome::Failed(e),
    }
}

/// `['a', 'b']` (GVariant string array) → `["a", "b"]`.
pub fn parse_list(text: &str) -> Vec<String> {
    text.split('\'')
        .skip(1)
        .step_by(2)
        .map(String::from)
        .collect()
}

pub fn format_list(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|i| format!("'{i}'")).collect();
    format!("[{}]", quoted.join(", "))
}

fn unquote(text: &str) -> String {
    text.trim().trim_matches('\'').to_string()
}

/// `key=value` pairs from dconf's keyfile text (`dconf dump`, the template),
/// without comments, `[/]` headers and blank lines.
pub fn keys(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with('['))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_profile_lists() {
        let list = parse_list("['b1dcc9dd-5262-4d8d-a863-c897e6d979b9', 'x']");
        assert_eq!(list, ["b1dcc9dd-5262-4d8d-a863-c897e6d979b9", "x"]);
        assert_eq!(
            format_list(&list),
            "['b1dcc9dd-5262-4d8d-a863-c897e6d979b9', 'x']"
        );
        assert!(parse_list("@as []").is_empty());
        assert_eq!(unquote("'abc'\n"), "abc");
    }

    #[test]
    fn keys_skip_comments_and_headers() {
        let dump = "[/]\nfont='JetBrainsMono Nerd Font 12'\nuse-system-font=false\n\n# x\n";
        assert_eq!(
            keys(dump),
            [
                ("font".into(), "'JetBrainsMono Nerd Font 12'".into()),
                ("use-system-font".into(), "false".into())
            ]
        );
    }
}
