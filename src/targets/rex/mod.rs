//! Rex, the terminal multiplexer app (Superlogical). Its config (`init.lua`) holds
//! keys only; themes live in the app's preferences. The app's own action
//! `client.theme.import path=…` reads a Ghostty theme file, adds it as a custom theme
//! named after the file and selects it; importing the same file again replaces that
//! theme (checked 2026-11-15). So each switch copies Palette's colours to
//! `~/.config/palette/rex/palette` and imports it: Rex shows "palette".
//!
//! Rex files the theme under light or dark by its background, so Palette's light/dark
//! switch picks the matching slot. The themes you had (`Workspace.darkThemeID` and
//! `lightThemeID`) are remembered before the first import and given back with
//! `client.theme.change` when Rex is switched off in Palette.
//!
//! Rex has its own light/dark setting (`Workspace.appearance`) and doesn't follow macOS
//! (owner, 2026-11-16: a light theme never showed). After the import, if that setting is
//! light or dark and not the theme's mode, `client.appearance.switch_mode` sets it; any
//! other value is left alone. Your setting is remembered and given back too.
//!
//! Rex accepts actions only with Remote Control on (Rex → Settings → Rex Server).

use std::fs;
use std::path::PathBuf;

use super::{BASE, Note, Output, Platforms, Target};
use crate::paths::Paths;
use crate::system::{self, Outcome};
use crate::wiring::Wiring;

pub const TARGET: Target = Target {
    name: "rex",
    output: Some(Output {
        file: "rex.conf",
        template: include_str!("template.conf"),
        // Ghostty's format can hold more than colours; keep it rendered from them.
        overridable: false,
        finish: None,
    }),
    wiring: Wiring::Direct,
    installed: |paths| super::has_app(paths, "Rex Beta.app") || super::has_app(paths, "Rex.app"),
    reload,
    unload: Some(unload),
    doctor,
    verify: |paths, _| Some(verify(paths)),
    platforms: Platforms::Mac,
    ..BASE
};

const DOMAIN: &str = "com.superlogical.rex";
const SLOTS: [&str; 2] = ["Workspace.darkThemeID", "Workspace.lightThemeID"];
const APPEARANCE: &str = "Workspace.appearance";
const REMOTE: &str = "turn on Remote Control in Rex → Settings → Rex Server";

/// The file Rex imports; its name is the theme's name in Rex.
fn theme_file(paths: &Paths) -> PathBuf {
    paths.root().join("rex/palette")
}

/// Your Rex themes before Palette's (`dark=<id>` / `light=<id>` lines).
fn previous_file(paths: &Paths) -> PathBuf {
    paths.root().join("rex-previous-themes")
}

/// The `rex` command: on PATH, or inside the app's server bundle (an app opened from
/// Finder has a short PATH).
fn program(paths: &Paths) -> Option<String> {
    if let Some(p) = system::find_program("rex") {
        return Some(p.to_string_lossy().into());
    }
    let bundled = paths.home().join(
        "Library/Application Support/com.superlogical.rex/RexServer/Versions/Current/\
         Rex Server.app/Contents/MacOS/rex",
    );
    bundled.is_file().then(|| bundled.to_string_lossy().into())
}

fn rex_do(paths: &Paths, args: &[&str]) -> Result<String, String> {
    let rex = program(paths).ok_or("the `rex` command wasn't found")?;
    let mut all = vec!["do"];
    all.extend_from_slice(args);
    system::run(&rex, &all).map_err(|e| {
        if e.contains("not accepting actions") {
            REMOTE.to_string()
        } else {
            e
        }
    })
}

/// `dark=abyssal-trench\nlight=merino-light\nappearance=dark\n` from the preference values.
fn previous_text(dark: Option<&str>, light: Option<&str>, appearance: Option<&str>) -> String {
    let mut out = String::new();
    for (slot, id) in [("dark", dark), ("light", light), ("appearance", appearance)] {
        if let Some(id) = id.map(str::trim).filter(|id| !id.is_empty()) {
            out.push_str(&format!("{slot}={id}\n"));
        }
    }
    out
}

/// The theme ids to give back, dark first.
fn previous_ids(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| l.split_once('='))
        .filter(|(slot, _)| matches!(slot.trim(), "dark" | "light"))
        .map(|(_, id)| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect()
}

/// The `appearance=` line, if one was remembered.
fn previous_appearance(text: &str) -> Option<String> {
    text.lines()
        .filter_map(|l| l.split_once('='))
        .find(|(slot, _)| slot.trim() == "appearance")
        .map(|(_, v)| v.trim().to_string())
        .filter(|v| v == "light" || v == "dark")
}

/// The theme's mode from the `# mode: light` line of the imported file.
fn file_mode(text: &str) -> Option<&str> {
    text.lines()
        .find_map(|l| l.strip_prefix("# mode:"))
        .map(str::trim)
        .filter(|m| *m == "light" || *m == "dark")
}

/// The mode to switch Rex to: only when its own setting is light or dark and differs.
fn switch_to<'a>(current: Option<&str>, wanted: Option<&'a str>) -> Option<&'a str> {
    let current = current.map(str::trim)?;
    let wanted = wanted?;
    (matches!(current, "light" | "dark") && current != wanted).then_some(wanted)
}

fn read_appearance() -> Option<String> {
    system::run("defaults", &["read", DOMAIN, APPEARANCE]).ok()
}

fn remember_yours(paths: &Paths) -> Result<(), String> {
    let file = previous_file(paths);
    if file.exists() {
        return Ok(()); // remembered at the first import; Palette's own since
    }
    let read = |key: &str| system::run("defaults", &["read", DOMAIN, key]).ok();
    let (dark, light, look) = (read(SLOTS[0]), read(SLOTS[1]), read(APPEARANCE));
    let text = previous_text(dark.as_deref(), light.as_deref(), look.as_deref());
    fs::create_dir_all(paths.root()).map_err(|e| e.to_string())?;
    fs::write(file, text).map_err(|e| e.to_string())
}

/// Copy the rendered colours where Rex imports them from (temp file + rename).
fn place(paths: &Paths) -> Result<PathBuf, String> {
    let colours = fs::read(paths.current().join("rex.conf"))
        .map_err(|e| format!("set a theme first ({e})"))?;
    let file = theme_file(paths);
    let dir = file.parent().ok_or("no folder")?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let temp = dir.join(".palette.partial");
    fs::write(&temp, colours).map_err(|e| e.to_string())?;
    fs::rename(&temp, &file).map_err(|e| e.to_string())?;
    Ok(file)
}

fn reload(paths: &Paths) -> Outcome {
    if let Err(e) = place(paths) {
        return Outcome::Failed(e);
    }
    if paths.is_fake() || !system::is_running("Rex") {
        return Outcome::Skipped("not running (Rex picks it up at the next switch)".into());
    }
    if let Err(e) = remember_yours(paths) {
        return Outcome::Failed(e);
    }
    let path = format!("path={}", theme_file(paths).display());
    if let Err(e) = rex_do(paths, &["client.theme.import", &path]) {
        return Outcome::Failed(e);
    }
    let text = fs::read_to_string(theme_file(paths)).unwrap_or_default();
    match switch_to(read_appearance().as_deref(), file_mode(&text)) {
        None => Outcome::Done("theme \"palette\" imported".into()),
        Some(mode) => match rex_do(
            paths,
            &["client.appearance.switch_mode", &format!("mode={mode}")],
        ) {
            Ok(_) => Outcome::Done(format!("theme \"palette\" imported, Rex set to {mode}")),
            Err(e) => Outcome::Failed(e),
        },
    }
}

/// What Rex took: its "palette" theme has the rendered background, the slot Rex filed it
/// under (light or dark) points at it, and Rex's own appearance is the theme's mode.
fn verify(paths: &Paths) -> Outcome {
    if paths.is_fake() {
        return Outcome::Skipped(crate::verify::FAKE.into());
    }
    if !system::is_running("Rex") {
        return Outcome::Skipped("not running".into());
    }
    let rendered = fs::read_to_string(theme_file(paths)).unwrap_or_default();
    match system::run(
        "osascript",
        &["-l", "JavaScript", "-e", include_str!("verify.js")],
    ) {
        Ok(text) => judge(&text, &rendered),
        Err(e) => Outcome::Failed(e),
    }
}

/// `text`: `key=value` lines from `verify.js`; `rendered`: Palette's file for Rex.
fn judge(text: &str, rendered: &str) -> Outcome {
    let get = |k: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(&format!("{k}=")))
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let want_bg = rendered
        .lines()
        .find_map(|l| l.strip_prefix("background = #"))
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let (id, filed) = (get("id"), get("filed"));
    if id.is_empty() {
        return Outcome::Failed("Rex has no theme called \"palette\": `palette refresh`".into());
    }
    if get("bg") != want_bg {
        return Outcome::Failed(format!(
            "Rex's \"palette\" has background #{}, the theme #{want_bg}: `palette refresh`",
            get("bg")
        ));
    }
    if get(&filed) != id {
        return Outcome::Failed(format!(
            "Rex's {filed} slot is {}, not \"palette\"",
            get(&filed)
        ));
    }
    let appearance = get("appearance");
    match file_mode(rendered) {
        Some(mode) if matches!(appearance.as_str(), "light" | "dark") && appearance != mode => {
            Outcome::Failed(format!("Rex is set to {appearance}, the theme is {mode}"))
        }
        _ => Outcome::Done(format!(
            "\"palette\" in Rex's {filed} slot, Rex {appearance}"
        )),
    }
}

/// Your own Rex themes back. Rex's custom theme "palette" stays in its list.
fn unload(paths: &Paths) -> Outcome {
    let file = previous_file(paths);
    let Ok(text) = fs::read_to_string(&file) else {
        return Outcome::Unchanged("Rex was never themed by Palette".into());
    };
    if paths.is_fake() || !system::is_running("Rex") {
        return Outcome::Skipped("Rex isn't running; pick your theme in Rex".into());
    }
    let ids = previous_ids(&text);
    for id in &ids {
        if let Err(e) = rex_do(paths, &["client.theme.change", &format!("name={id}")]) {
            return Outcome::Failed(e);
        }
    }
    if let Some(mode) = previous_appearance(&text)
        && switch_to(read_appearance().as_deref(), Some(&mode)).is_some()
        && let Err(e) = rex_do(
            paths,
            &["client.appearance.switch_mode", &format!("mode={mode}")],
        )
    {
        return Outcome::Failed(e);
    }
    let _ = fs::remove_file(file);
    Outcome::Done(format!("back to {}", ids.join(" / ")))
}

fn doctor(paths: &Paths) -> Vec<Note> {
    if !(TARGET.installed)(paths) {
        return Vec::new();
    }
    if program(paths).is_none() {
        return vec![Note::fix(
            "Rex's `rex` command wasn't found: open Rex once so it installs its server",
        )];
    }
    vec![Note::info(format!(
        "Rex takes the theme only with Remote Control on: {REMOTE}"
    ))]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_judges_what_rex_took() {
        let rendered = "# mode: dark\nbackground = #06101A\n";
        let ok = "dark=custom-1\nlight=merino\nappearance=dark\nid=custom-1\nfiled=dark\nbg=06101a";
        assert!(matches!(judge(ok, rendered), Outcome::Done(_)));
        let other_slot = ok.replace("dark=custom-1", "dark=nord");
        assert!(matches!(judge(&other_slot, rendered), Outcome::Failed(_)));
        let old_colours = ok.replace("bg=06101a", "bg=000000");
        assert!(matches!(judge(&old_colours, rendered), Outcome::Failed(_)));
        let light = ok.replace("appearance=dark", "appearance=light");
        assert!(matches!(judge(&light, rendered), Outcome::Failed(_)));
        let system = ok.replace("appearance=dark", "appearance=system");
        assert!(matches!(judge(&system, rendered), Outcome::Done(_)));
        assert!(matches!(judge("id=", rendered), Outcome::Failed(_)));
    }

    #[test]
    fn remembers_both_slots_and_gives_them_back_in_order() {
        let text = previous_text(
            Some("abyssal-trench\n"),
            Some(" merino-light"),
            Some("dark\n"),
        );
        assert_eq!(
            text,
            "dark=abyssal-trench\nlight=merino-light\nappearance=dark\n"
        );
        assert_eq!(previous_ids(&text), ["abyssal-trench", "merino-light"]);
        assert_eq!(previous_appearance(&text).as_deref(), Some("dark"));
        assert_eq!(previous_text(None, Some(""), None), "");
        // Files from before 2026-11-16 have no appearance line.
        assert_eq!(previous_appearance("dark=a\nlight=b\n"), None);
    }

    #[test]
    fn switches_rex_only_when_its_own_setting_differs() {
        let file = "# Generated …\n# mode: light\nbackground = #eff1f5\n";
        assert_eq!(file_mode(file), Some("light"));
        assert_eq!(switch_to(Some("dark\n"), Some("light")), Some("light"));
        assert_eq!(switch_to(Some("light"), Some("light")), None);
        // Anything else (unset, a future "system") is left alone.
        assert_eq!(switch_to(None, Some("light")), None);
        assert_eq!(switch_to(Some("system"), Some("dark")), None);
        assert_eq!(switch_to(Some("dark"), None), None);
    }

    #[test]
    fn places_the_file_and_never_runs_rex_in_a_fake_home() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(tmp.path());
        assert!(matches!(reload(&paths), Outcome::Failed(_)));
        fs::create_dir_all(paths.current()).unwrap();
        fs::write(paths.current().join("rex.conf"), "background = #000000\n").unwrap();
        assert!(matches!(reload(&paths), Outcome::Skipped(_)));
        assert_eq!(
            fs::read_to_string(theme_file(&paths)).unwrap(),
            "background = #000000\n"
        );
        assert!(!previous_file(&paths).exists());
        assert!(matches!(unload(&paths), Outcome::Unchanged(_)));
    }
}
