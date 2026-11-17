//! The beat lines' helper (ROADMAP 20): `palette-beats` (Swift, `app/helpers/palette-beats.swift`, in
//! `Contents/MacOS`) runs as a child of this app while the bar Palette manages has a now-playing item
//! with beat lines and Performance mode's "Slower bar" is off. A child of the app, so macOS asks for
//! "System Audio Recording" for Palette (the app that started it). It taps audio only while the bar's
//! now-playing feeder says something is playing, and quits by itself when the app is gone.
//!
//! [`sync`] runs at launch, every minute (catching changes made with the CLI) and right after the app
//! changes the bar or Performance mode.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use palette::desktop::{self, Tool, bar};

use super::paths;

/// The helper running now, with the arguments it was started with.
static HELPER: Mutex<Option<(Child, Vec<String>)>> = Mutex::new(None);

/// Next to the app's own executable in the bundle; for a development build, the copy
/// `app/sidecar.sh` makes, or the installed app's.
fn helper() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let dir = exe.parent()?;
    [
        dir.join("palette-beats"),
        dir.join("../../app/binaries/palette-beats"),
        PathBuf::from("/Applications/Palette.app/Contents/MacOS/palette-beats"),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

/// The helper's arguments when the bar wants beat lines now, else `None`.
fn wanted() -> Option<Vec<String>> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let paths = paths().ok()?;
    if !desktop::is_managed(&paths, Tool::Bar) {
        return None;
    }
    let b: bar::Bar = desktop::load(&paths, Tool::Bar).ok()?;
    if !bar::wants_beats(&paths, &b) {
        return None;
    }
    let (low, high) = bar::beat_range(&b);
    let state = paths.home().join("Library/Caches/palette/nowplaying.state");
    Some(vec![
        "--bars".into(),
        bar::BEAT_LINES.to_string(),
        "--min".into(),
        low.to_string(),
        "--max".into(),
        high.to_string(),
        "--state".into(),
        state.display().to_string(),
    ])
}

/// Start, restart or stop the helper to match the bar's settings.
pub(crate) fn sync() {
    let want = wanted();
    let Ok(mut running) = HELPER.lock() else {
        return;
    };
    if let Some((child, args)) = running.as_mut() {
        let alive = matches!(child.try_wait(), Ok(None));
        if alive && want.as_ref() == Some(args) {
            return;
        }
        let _ = child.kill();
        let _ = child.wait();
        *running = None;
    }
    let (Some(args), Some(exe)) = (want, helper()) else {
        return;
    };
    match Command::new(&exe)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log_file().map_or_else(Stdio::null, Stdio::from))
        .spawn()
    {
        Ok(child) => *running = Some((child, args)),
        Err(e) => eprintln!("palette: beat lines: couldn't start {}: {e}", exe.display()),
    }
}

/// `~/Library/Caches/palette/beats.log`: what the helper did (permission, listening, errors), new
/// each start.
fn log_file() -> Option<std::fs::File> {
    let dir = paths().ok()?.home().join("Library/Caches/palette");
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::File::create(dir.join("beats.log")).ok()
}

/// `sync` off the calling thread (it may wait for the old helper to stop).
pub(crate) fn sync_soon() {
    std::thread::spawn(sync);
}
