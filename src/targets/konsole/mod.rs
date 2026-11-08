//! Konsole, KDE's terminal. Konsole takes colours from a colour scheme named
//! by a profile, so Palette keeps its own two files in
//! `~/.local/share/konsole/`: `Palette.colorscheme` (rewritten on every
//! switch) and `Palette.profile` (made once, as a copy of your default
//! profile, so your font and other settings come along; only its
//! `ColorScheme` is Palette's). `konsolerc`'s `DefaultProfile` points at it,
//! one key; the profile you had is remembered and put back when Palette stops
//! theming Konsole. New windows and tabs use the theme; open ones keep theirs.

use std::fs;
use std::path::PathBuf;

use super::{BASE, Output, Platforms, Target};
use crate::paths::Paths;
use crate::system::Outcome;
use crate::wiring::Wiring;

pub const TARGET: Target = Target {
    name: "konsole",
    output: Some(Output {
        file: "konsole.colorscheme",
        template: include_str!("template.colorscheme"),
        overridable: true,
        finish: None,
    }),
    wiring: Wiring::Direct,
    installed: |_| super::has_program("konsole"),
    reload,
    unload: Some(unload),
    platforms: Platforms::Linux,
    ..BASE
};

const PROFILE: &str = "Palette.profile";
const GROUP: &str = "Desktop Entry";
const KEY: &str = "DefaultProfile";

fn data_dir(paths: &Paths) -> PathBuf {
    paths.home().join(".local/share/konsole")
}

fn konsolerc(paths: &Paths) -> PathBuf {
    paths.config_home().join("konsolerc")
}

/// Where the default profile you had before Palette is remembered (empty:
/// Konsole's built-in one).
fn previous_file(paths: &Paths) -> PathBuf {
    paths.root().join("konsole-previous-profile")
}

fn reload(paths: &Paths) -> Outcome {
    match apply(paths) {
        Ok(()) => Outcome::Done("Palette profile; new windows and tabs use it".into()),
        Err(e) => Outcome::Failed(e),
    }
}

pub fn apply(paths: &Paths) -> Result<(), String> {
    let rendered = fs::read_to_string(paths.current().join("konsole.colorscheme"))
        .map_err(|e| format!("reading konsole.colorscheme: {e}"))?;
    let dir = data_dir(paths);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(dir.join("Palette.colorscheme"), rendered).map_err(|e| e.to_string())?;

    let rc_path = konsolerc(paths);
    let rc = fs::read_to_string(&rc_path).unwrap_or_default();
    let current = ini_get(&rc, GROUP, KEY);
    if current.as_deref() != Some(PROFILE) {
        fs::create_dir_all(paths.root()).map_err(|e| e.to_string())?;
        fs::write(previous_file(paths), current.as_deref().unwrap_or(""))
            .map_err(|e| e.to_string())?;
    }

    let profile_path = dir.join(PROFILE);
    let profile = match fs::read_to_string(&profile_path) {
        Ok(text) => text,
        Err(_) => {
            let base = current
                .filter(|p| p != PROFILE)
                .and_then(|p| fs::read_to_string(dir.join(p)).ok());
            profile_text(base.as_deref())
        }
    };
    let profile = ini_set(&profile, "Appearance", "ColorScheme", "Palette");
    fs::write(&profile_path, profile).map_err(|e| e.to_string())?;

    if ini_get(&rc, GROUP, KEY).as_deref() != Some(PROFILE) {
        fs::write(&rc_path, ini_set(&rc, GROUP, KEY, PROFILE)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Your default profile back (Konsole's built-in one if you had none).
/// Palette's two files stay, in case you edited the profile.
fn unload(paths: &Paths) -> Outcome {
    let rc_path = konsolerc(paths);
    let rc = fs::read_to_string(&rc_path).unwrap_or_default();
    let previous = fs::read_to_string(previous_file(paths)).unwrap_or_default();
    let previous = previous.trim();
    let text = if previous.is_empty() {
        ini_remove(&rc, GROUP, KEY)
    } else {
        ini_set(&rc, GROUP, KEY, previous)
    };
    match fs::write(&rc_path, text) {
        Ok(()) if previous.is_empty() => Outcome::Done("back to Konsole's built-in profile".into()),
        Ok(()) => Outcome::Done(format!("back to {previous}")),
        Err(e) => Outcome::Failed(e.to_string()),
    }
}

/// Palette's profile: a copy of `base` (your default profile) renamed, or a
/// minimal one on top of Konsole's built-in profile.
pub fn profile_text(base: Option<&str>) -> String {
    match base {
        Some(base) => ini_set(base, "General", "Name", "Palette"),
        None => "[Appearance]\nColorScheme=Palette\n\n[General]\nName=Palette\nParent=FALLBACK/\n"
            .into(),
    }
}

/// `key`'s value in `[group]` of an INI text (KDE's config format).
pub fn ini_get(text: &str, group: &str, key: &str) -> Option<String> {
    let header = format!("[{group}]");
    let mut inside = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            inside = line == header;
        } else if inside
            && let Some((k, v)) = line.split_once('=')
            && k.trim() == key
        {
            return Some(v.trim().to_string());
        }
    }
    None
}

/// The text with `key=value` in `[group]`: replaced where it is, else added
/// at the end of the group (a new group at the end if there's none).
/// Everything else stays as it was.
pub fn ini_set(text: &str, group: &str, key: &str, value: &str) -> String {
    let header = format!("[{group}]");
    let entry = format!("{key}={value}");
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let Some(start) = lines.iter().position(|l| l.trim() == header) else {
        let mut out = text.trim_end().to_string();
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        return format!("{out}{header}\n{entry}\n");
    };
    let end = lines[start + 1..]
        .iter()
        .position(|l| l.trim_start().starts_with('['))
        .map_or(lines.len(), |i| start + 1 + i);
    let found = lines[start + 1..end]
        .iter()
        .position(|l| l.split_once('=').is_some_and(|(k, _)| k.trim() == key));
    match found {
        Some(i) => lines[start + 1 + i] = entry,
        None => {
            // After the group's last setting, before any blank lines.
            let last = (start + 1..end)
                .rev()
                .find(|&i| !lines[i].trim().is_empty())
                .unwrap_or(start);
            lines.insert(last + 1, entry);
        }
    }
    lines.join("\n") + "\n"
}

/// The text without `key` in `[group]`.
pub fn ini_remove(text: &str, group: &str, key: &str) -> String {
    let header = format!("[{group}]");
    let mut inside = false;
    let kept: Vec<&str> = text
        .lines()
        .filter(|line| {
            let t = line.trim();
            if t.starts_with('[') {
                inside = t == header;
                return true;
            }
            !(inside && t.split_once('=').is_some_and(|(k, _)| k.trim() == key))
        })
        .collect();
    kept.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    const RC: &str = "[Desktop Entry]\nDefaultProfile=Mine.profile\n\n[MainWindow]\nState=x\n";

    #[test]
    fn reads_and_sets_one_key() {
        assert_eq!(ini_get(RC, GROUP, KEY).as_deref(), Some("Mine.profile"));
        let set = ini_set(RC, GROUP, KEY, PROFILE);
        assert_eq!(set, RC.replace("Mine.profile", PROFILE));
        assert_eq!(ini_get("[MainWindow]\nState=x\n", GROUP, KEY), None);
    }

    #[test]
    fn adds_a_missing_key_or_group() {
        let added = ini_set("[MainWindow]\nState=x\n", GROUP, KEY, PROFILE);
        assert_eq!(
            added,
            "[MainWindow]\nState=x\n\n[Desktop Entry]\nDefaultProfile=Palette.profile\n"
        );
        let in_group = ini_set("[Desktop Entry]\nA=1\n\n[B]\nC=2\n", GROUP, KEY, PROFILE);
        assert_eq!(
            in_group,
            "[Desktop Entry]\nA=1\nDefaultProfile=Palette.profile\n\n[B]\nC=2\n"
        );
        assert_eq!(
            ini_set("", GROUP, KEY, PROFILE),
            "[Desktop Entry]\nDefaultProfile=Palette.profile\n"
        );
    }

    #[test]
    fn removes_only_that_key() {
        assert_eq!(
            ini_remove(RC, GROUP, KEY),
            "[Desktop Entry]\n\n[MainWindow]\nState=x\n"
        );
    }

    #[test]
    fn profile_keeps_your_settings() {
        let base =
            "[Appearance]\nColorScheme=Breeze\nFont=JetBrains Mono,12\n\n[General]\nName=Mine\n";
        let text = ini_set(
            &profile_text(Some(base)),
            "Appearance",
            "ColorScheme",
            "Palette",
        );
        assert!(text.contains("Font=JetBrains Mono,12"));
        assert!(text.contains("ColorScheme=Palette"));
        assert!(text.contains("Name=Palette"));
        assert!(profile_text(None).contains("ColorScheme=Palette"));
    }

    #[test]
    fn switch_and_give_back_in_a_fake_home() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        fs::create_dir_all(paths.current()).unwrap();
        fs::write(
            paths.current().join("konsole.colorscheme"),
            "[Background]\nColor=1,2,3\n",
        )
        .unwrap();
        fs::create_dir_all(data_dir(&paths)).unwrap();
        fs::write(
            data_dir(&paths).join("Mine.profile"),
            "[Appearance]\nFont=X,12\n",
        )
        .unwrap();
        fs::create_dir_all(paths.config_home()).unwrap();
        fs::write(konsolerc(&paths), RC).unwrap();

        apply(&paths).unwrap();
        let rc = fs::read_to_string(konsolerc(&paths)).unwrap();
        assert_eq!(ini_get(&rc, GROUP, KEY).as_deref(), Some(PROFILE));
        assert!(rc.contains("State=x"));
        let profile = fs::read_to_string(data_dir(&paths).join(PROFILE)).unwrap();
        assert!(profile.contains("Font=X,12") && profile.contains("ColorScheme=Palette"));
        assert!(data_dir(&paths).join("Palette.colorscheme").is_file());

        apply(&paths).unwrap(); // a second switch keeps the remembered profile
        unload(&paths);
        let rc = fs::read_to_string(konsolerc(&paths)).unwrap();
        assert_eq!(ini_get(&rc, GROUP, KEY).as_deref(), Some("Mine.profile"));
    }
}
