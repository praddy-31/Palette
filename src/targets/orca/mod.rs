//! Orca, the agent IDE (stablyai). Orca themes are data-only plugins: a folder with
//! `orca-plugin.json` (`"execution": "data"`) that contributes app themes (colour tokens
//! such as `--background` plus `tokenColors` for code) and terminal themes (16 colours,
//! foreground, background, cursor). Orca installs such a plugin without asking
//! (`orca plugin install <folder>`), and `orca plugin update palette.theme` re-reads the
//! folder and repaints a running Orca (checked live, 2026-11-16).
//!
//! So each switch renders `current/orca.json` and builds the plugin folder from it in
//! `~/.config/palette/orca/plugin/`; then, when Orca is open, installs it once or updates it.
//! Orca's picker lists a theme only for its `base` (dark themes in dark mode), so the
//! plugin carries two entries with the same colours, "Palette Dark" and "Palette Light"
//! (owner, 2026-11-16): you pick each once and Orca shows the current theme in both modes.
//!
//! Orca keeps the chosen theme and its window glass in its own SQLite database, which
//! has no CLI: Palette never writes it (DECISIONS 2026-11-16). Orca's plugin commands
//! need Orca open; when it's closed the folder is ready and the next switch sends it.

use std::fs;
use std::path::{Path, PathBuf};

use super::{BASE, Note, Output, Platforms, Target};
use crate::paths::Paths;
use crate::system::{self, Outcome};
use crate::wiring::Wiring;

pub const TARGET: Target = Target {
    name: "orca",
    output: Some(Output {
        file: "orca.json",
        template: include_str!("template.json"),
        overridable: true,
        finish: None,
    }),
    wiring: Wiring::Direct,
    installed: |paths| super::has_app(paths, "Orca.app") || super::has_program("orca"),
    reload,
    unload: Some(unload),
    doctor,
    verify: |paths, _| Some(verify(paths)),
    platforms: Platforms::Mac,
    ..BASE
};

/// `publisher.id` from the manifest: the key Orca knows the plugin by.
const KEY: &str = "palette.theme";
const PICK: &str = "in Orca's theme settings, pick \"Palette Dark\" and \"Palette Light\" once";

fn plugin_dir(paths: &Paths) -> PathBuf {
    paths.root().join("orca/plugin")
}

fn program() -> Option<String> {
    system::find_program("orca")
        .or_else(|| {
            let bundled = PathBuf::from("/Applications/Orca.app/Contents/Resources/bin/orca");
            bundled.is_file().then_some(bundled)
        })
        .map(|p| p.to_string_lossy().into())
}

const MANIFEST: &str = r#"{
  "manifestVersion": 2,
  "id": "theme",
  "publisher": "palette",
  "name": "Palette",
  "version": "1.0.0",
  "execution": "data",
  "description": "Colours from Palette, generated on this Mac",
  "license": "MIT",
  "engines": { "orca": ">=1.4.245" },
  "capabilities": [],
  "contributes": {
    "themes": [
      { "id": "palette-dark", "title": "Palette Dark", "path": "themes/palette-dark.json" },
      { "id": "palette-light", "title": "Palette Light", "path": "themes/palette-light.json" }
    ],
    "terminalThemes": [
      { "id": "palette-dark", "title": "Palette Dark", "path": "terminal/palette-dark.json" },
      { "id": "palette-light", "title": "Palette Light", "path": "terminal/palette-light.json" }
    ]
  }
}
"#;

/// Between the app theme and the terminal theme in the rendered `orca.json`.
const MARKER: &str = "\n@@terminal@@\n";

/// The plugin's files (path inside the folder, text) from the rendered `orca.json`:
/// both halves twice, once with `@MODE@` = dark and once = light. No JSON crate in the
/// core (DECISIONS 2026-10-12): the template writes the JSON, Palette only fills the mode.
fn files(rendered: &str) -> Result<Vec<(String, String)>, String> {
    let (app, terminal) = rendered
        .split_once(MARKER)
        .ok_or("orca.json has no `@@terminal@@` line (a theme's own orca.json must keep it)")?;
    let mut out = vec![("orca-plugin.json".to_string(), MANIFEST.to_string())];
    for mode in ["dark", "light"] {
        let fill = |text: &str| format!("{}\n", text.trim().replace("@MODE@", mode));
        out.push((format!("themes/palette-{mode}.json"), fill(app)));
        out.push((format!("terminal/palette-{mode}.json"), fill(terminal)));
    }
    Ok(out)
}

/// Build the plugin folder next to it, then swap it in, so Orca never reads half a folder.
fn build(paths: &Paths) -> Result<PathBuf, String> {
    let rendered = fs::read_to_string(paths.current().join("orca.json"))
        .map_err(|e| format!("set a theme first ({e})"))?;
    let dir = plugin_dir(paths);
    let parent = dir.parent().ok_or("no folder")?;
    let temp = parent.join(".plugin.partial");
    let _ = fs::remove_dir_all(&temp);
    for (name, text) in files(&rendered)? {
        let path = temp.join(name);
        fs::create_dir_all(path.parent().unwrap_or(&temp)).map_err(|e| e.to_string())?;
        fs::write(&path, text).map_err(|e| e.to_string())?;
    }
    swap(&temp, &dir)?;
    Ok(dir)
}

fn swap(new: &Path, dir: &Path) -> Result<(), String> {
    let old = dir.with_extension("old");
    let _ = fs::remove_dir_all(&old);
    if dir.exists() {
        fs::rename(dir, &old).map_err(|e| e.to_string())?;
    }
    fs::rename(new, dir).map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(&old);
    Ok(())
}

/// Orca's plugin folder; `plugins.lock.json` there names the copy (by content hash) it loads.
fn orca_plugins(paths: &Paths) -> PathBuf {
    paths
        .home()
        .join("Library/Application Support/Orca/plugins")
}

/// The content hash Orca's lock file records for Palette's plugin.
fn locked_hash(lock: &str) -> Option<String> {
    let rest = &lock[lock.find(&format!("\"{KEY}\": {{"))?..];
    let rest = &rest[rest.find("\"contentHash\"")? + 13..];
    let hash: String = rest
        .split('"')
        .nth(1)?
        .chars()
        .filter(char::is_ascii_hexdigit)
        .collect();
    (!hash.is_empty()).then_some(hash)
}

/// What Orca took: the copy its lock file points at holds the same two themes as Palette's
/// folder. (Orca reads that copy, so this holds whether it's running or not.)
fn verify(paths: &Paths) -> Outcome {
    let plugins = orca_plugins(paths);
    let Ok(lock) = fs::read_to_string(plugins.join("plugins.lock.json")) else {
        return Outcome::Skipped("Orca hasn't installed any plugin here yet".into());
    };
    let Some(hash) = locked_hash(&lock) else {
        return Outcome::Failed(format!(
            "Palette's plugin isn't installed in Orca: {PICK} after a switch with Orca open"
        ));
    };
    for entry in ["themes/palette-dark.json", "themes/palette-light.json"] {
        let loaded = plugins.join(KEY).join(&hash).join(entry);
        let outcome =
            crate::verify::same_file(&loaded, &plugin_dir(paths).join(entry), "Orca's copy");
        if !matches!(outcome, Outcome::Done(_)) {
            return match outcome {
                Outcome::Failed(_) => Outcome::Failed(
                    "Orca holds an older theme (it was closed during the switch?): switch again with Orca open".into(),
                ),
                other => other,
            };
        }
    }
    Outcome::Done("Orca's installed plugin is the current theme".into())
}

/// Is Palette's plugin in Orca's list? (`orca plugin list --json` names each by its key.)
fn is_installed(orca: &str) -> Result<bool, String> {
    let out = system::run(orca, &["plugin", "list", "--json"])?;
    Ok(out.contains(&format!("\"{KEY}\"")))
}

fn reload(paths: &Paths) -> Outcome {
    let dir = match build(paths) {
        Ok(dir) => dir,
        Err(e) => return Outcome::Failed(e),
    };
    if paths.is_fake() || !system::is_running("Orca") {
        return Outcome::Skipped("not running (Orca picks it up at the next switch)".into());
    }
    let Some(orca) = program() else {
        return Outcome::Failed("the `orca` command wasn't found".into());
    };
    let result = match is_installed(&orca) {
        Ok(true) => system::run(&orca, &["plugin", "update", KEY]).map(|_| "updated".to_string()),
        Ok(false) => system::run(&orca, &["plugin", "install", &dir.to_string_lossy()])
            .map(|_| format!("Palette's theme plugin installed: {PICK}")),
        Err(e) => Err(e),
    };
    match result {
        Ok(note) => Outcome::Done(note),
        Err(e) => Outcome::Failed(e),
    }
}

/// Remove the plugin: Orca goes back to its own theme.
fn unload(paths: &Paths) -> Outcome {
    if paths.is_fake() || !system::is_running("Orca") {
        return Outcome::Skipped("Orca isn't running; pick your theme in Orca".into());
    }
    let Some(orca) = program() else {
        return Outcome::Failed("the `orca` command wasn't found".into());
    };
    match is_installed(&orca) {
        Ok(false) => Outcome::Unchanged("Palette's plugin isn't in Orca".into()),
        Ok(true) => match system::run(&orca, &["plugin", "uninstall", KEY]) {
            Ok(_) => Outcome::Done("Palette's plugin removed; Orca uses its own theme".into()),
            Err(e) => Outcome::Failed(e),
        },
        Err(e) => Outcome::Failed(e),
    }
}

fn doctor(paths: &Paths) -> Vec<Note> {
    if !(TARGET.installed)(paths) {
        return Vec::new();
    }
    if program().is_none() {
        return vec![Note::fix(
            "Orca's `orca` command wasn't found: in Orca, install its command-line tool",
        )];
    }
    vec![Note::info(format!(
        "Orca: {PICK}. Window glass stays Orca's own setting"
    ))]
}

#[cfg(test)]
mod tests {
    use super::*;

    const RENDERED: &str =
        "{ \"base\": \"@MODE@\", \"tokens\": {} }\n@@terminal@@\n{ \"mode\": \"@MODE@\" }\n";

    #[test]
    fn verify_compares_the_copy_orca_locked() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        let lock = r#"{"plugins": {"other": {"contentHash": "aa"}, "palette.theme": {"pluginKey": "palette.theme", "contentHash": "40144e"}}}"#;
        assert_eq!(locked_hash(lock).as_deref(), Some("40144e"));
        assert!(matches!(verify(&paths), Outcome::Skipped(_)));
        let plugins = orca_plugins(&paths);
        fs::create_dir_all(&plugins).unwrap();
        fs::write(plugins.join("plugins.lock.json"), r#"{"plugins": {}}"#).unwrap();
        assert!(matches!(verify(&paths), Outcome::Failed(_)));
        let ours = plugin_dir(&paths).join("themes");
        let theirs = plugins.join(KEY).join("40144e/themes");
        for dir in [&ours, &theirs] {
            fs::create_dir_all(dir).unwrap();
            fs::write(dir.join("palette-dark.json"), "dark").unwrap();
            fs::write(dir.join("palette-light.json"), "light").unwrap();
        }
        fs::write(plugins.join("plugins.lock.json"), lock).unwrap();
        assert!(matches!(verify(&paths), Outcome::Done(_)));
        fs::write(ours.join("palette-light.json"), "new").unwrap();
        assert!(matches!(verify(&paths), Outcome::Failed(_)));
    }

    #[test]
    fn two_entries_with_the_same_colours() {
        let files = files(RENDERED).unwrap();
        let names: Vec<_> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "orca-plugin.json",
                "themes/palette-dark.json",
                "terminal/palette-dark.json",
                "themes/palette-light.json",
                "terminal/palette-light.json"
            ]
        );
        let get = |n: &str| files.iter().find(|(name, _)| name == n).unwrap().1.as_str();
        assert_eq!(
            get("themes/palette-light.json"),
            "{ \"base\": \"light\", \"tokens\": {} }\n"
        );
        assert_eq!(
            get("terminal/palette-dark.json"),
            "{ \"mode\": \"dark\" }\n"
        );
        assert!(
            MANIFEST.contains("\"execution\": \"data\""),
            "data only: no approval"
        );
        assert!(
            MANIFEST.contains("\"publisher\": \"palette\"")
                && MANIFEST.contains("\"id\": \"theme\"")
        );
        // Every path the manifest names is a file Palette writes.
        for path in MANIFEST.split("\"path\": \"").skip(1) {
            let path = &path[..path.find('"').unwrap()];
            assert!(names.contains(&path), "{path}");
        }
        assert!(super::files("{}").unwrap_err().contains("@@terminal@@"));
    }

    #[test]
    fn builds_the_folder_and_swaps_it_whole() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(tmp.path());
        assert!(build(&paths).unwrap_err().contains("set a theme first"));
        fs::create_dir_all(paths.current()).unwrap();
        fs::write(paths.current().join("orca.json"), RENDERED).unwrap();
        assert!(
            matches!(reload(&paths), Outcome::Skipped(_)),
            "fake home: never runs orca"
        );
        let dir = plugin_dir(&paths);
        fs::write(dir.join("stray.json"), "{}").unwrap();
        build(&paths).unwrap();
        assert!(dir.join("terminal/palette-light.json").is_file());
        assert!(!dir.join("stray.json").exists(), "a fresh folder each time");
        assert!(!dir.with_extension("old").exists());
        assert!(fs::read_dir(dir.parent().unwrap()).unwrap().count() == 1);
    }
}
