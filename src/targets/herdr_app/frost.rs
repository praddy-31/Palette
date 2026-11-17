//! Herdr's frost (owner, 2026-11-17). Herdr's glass is two files: see-through colours in the
//! theme (Palette's `themes/palette.json`, the Clear kind) and `"window_blur": true` at the
//! root of a layout, which is what lets the desktop through. Herdr's own layouts (`main`,
//! `terminals`, `win98`) are read-only and rewritten from Herdr's copy at every start, so the
//! blur can only go in a personal layout: Herdr's own `layout.clone` copies the layout you use
//! into `palette-glass` (with its clicks) and switches to it, then Palette adds the one key.
//! Herdr remembers the layout it shows, so this lasts across restarts with no Palette at
//! start. Off switches back and moves the copy to the Bin. Herdr's CLI talks to a running
//! window, so both need Herdr open; a fake home never asks Herdr.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::paths::Paths;
use crate::system;

/// The frosted copy's name: `layouts/palette-glass.json`.
pub const LAYOUT: &str = "palette-glass";

fn layouts(paths: &Paths) -> PathBuf {
    super::dir(paths).join("layouts")
}

/// The layout Herdr shows (`"layout"` in its `shared.json`), read only.
pub fn current_layout(paths: &Paths) -> Option<String> {
    let text = fs::read_to_string(super::dir(paths).join("shared.json")).ok()?;
    let v = crate::jsonc::read(&text, "layout")?;
    let name = v.trim().trim_matches('"').to_string();
    (!name.is_empty()).then_some(name)
}

/// A layout's text with `"window_blur": true` at its root, and `"terminal_opacity": 0` so the
/// panes show the see-through `canvas` behind them (every other byte kept).
pub fn with_blur(text: &str) -> String {
    let text = crate::jsonc::write(text, "window_blur", Some("true"));
    crate::jsonc::write(&text, "terminal_opacity", Some("0"))
}

/// `herdr-app do <action> <json>`, with Herdr open.
fn herdr_do(action: &str, args: &str) -> Result<String> {
    let Some(herdr) = system::find_program("herdr-app") else {
        bail!("the `herdr-app` command wasn't found (Herdr → Settings installs it)");
    };
    if !system::is_running("Herdr") {
        bail!("open Herdr first: Palette asks the running Herdr to switch layouts");
    }
    system::run(&herdr.to_string_lossy(), &["do", action, args]).map_err(anyhow::Error::msg)
}

/// Is Herdr showing Palette's frosted copy?
pub fn is_on(paths: &Paths) -> bool {
    current_layout(paths).as_deref() == Some(LAYOUT)
}

/// Frost on: clone the layout you use into `palette-glass` (a fresh copy each time, so it
/// has Herdr's latest), switch to it, add the blur. Returns the layout it was made from.
pub fn on(paths: &Paths, saved_from: Option<&str>) -> Result<String> {
    if paths.is_fake() {
        bail!("not in a fake home: Herdr isn't asked (PALETTE_HOME)");
    }
    let shown = current_layout(paths).unwrap_or_else(|| "main".into());
    let from = if shown == LAYOUT {
        saved_from.unwrap_or("main").to_string()
    } else {
        shown.clone()
    };
    let file = layouts(paths).join(format!("{LAYOUT}.json"));
    if shown == LAYOUT {
        // Herdr can't clone over the layout it shows: back to yours first.
        herdr_do(
            "layout.switch",
            &format!("{{\"name\":{}}}", crate::jsonc::quoted(&from)),
        )?;
    }
    if file.exists() {
        crate::install::move_to_trash(paths, &file)?;
    }
    herdr_do(
        "layout.clone",
        &format!(
            "{{\"name\":{},\"as\":{}}}",
            crate::jsonc::quoted(&from),
            crate::jsonc::quoted(LAYOUT)
        ),
    )?;
    // The copy is Herdr's to write; wait for it (a moment) before adding the blur.
    let deadline = Instant::now() + Duration::from_secs(3);
    let text = loop {
        match fs::read_to_string(&file) {
            Ok(t) if !t.trim().is_empty() => break t,
            _ if Instant::now() > deadline => {
                bail!("Herdr didn't make {} in time", file.display())
            }
            _ => std::thread::sleep(Duration::from_millis(50)),
        }
    };
    crate::desktop::write_atomic(&file, &with_blur(&text), false)
        .with_context(|| format!("adding the blur to {}", file.display()))?;
    Ok(from)
}

/// Frost off: Herdr back on `from` (the layout the copy was made from), the copy to the Bin.
pub fn off(paths: &Paths, from: &str) -> Result<()> {
    if paths.is_fake() {
        bail!("not in a fake home: Herdr isn't asked (PALETTE_HOME)");
    }
    if is_on(paths) {
        herdr_do(
            "layout.switch",
            &format!("{{\"name\":{}}}", crate::jsonc::quoted(from)),
        )?;
    }
    let file = layouts(paths).join(format!("{LAYOUT}.json"));
    if file.exists() {
        crate::install::move_to_trash(paths, &file)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_the_blur_at_the_root_and_reads_the_layout_shown() {
        let layout = "{\n  \"id\": \"root\",\n  \"type\": \"row\",\n  \"children\": [{ \"id\": \"a\", \"window_blur\": false }]\n}\n";
        let blurred = with_blur(layout);
        assert!(blurred.contains("\"window_blur\": true"), "{blurred}");
        assert!(blurred.contains("\"terminal_opacity\": 0"), "{blurred}");
        // The nested key isn't the root's: still there, unchanged.
        assert!(blurred.contains("\"window_blur\": false"), "{blurred}");
        assert_eq!(with_blur(&blurred), blurred);

        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        assert_eq!(current_layout(&paths), None);
        let dir = super::super::dir(&paths);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("shared.json"),
            r#"{"layout":"main","frame.welcomed":"finished"}"#,
        )
        .unwrap();
        assert_eq!(current_layout(&paths).as_deref(), Some("main"));
        assert!(!is_on(&paths));
        // A fake home never asks Herdr.
        assert!(on(&paths, None).is_err());
    }
}
