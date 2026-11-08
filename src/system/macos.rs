//! The macOS half of `system`: settings through `defaults`, AppleScript for
//! wallpaper and light/dark, Finder and the clipboard.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::{parse_utc_offset, run};

/// Run a program that talks to the desktop: on macOS just `run`.
pub fn run_desktop(program: &str, args: &[&str]) -> Result<String, String> {
    run(program, args)
}

/// The program inside an app (`Ghostty` → `ghostty`): its Info.plist's
/// `CFBundleExecutable`, for apps in the usual folders. Some tools match
/// windows by this name, not the app's.
pub fn app_executable(app: &str) -> Option<String> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let dirs = [
        Some(PathBuf::from("/Applications")),
        Some(PathBuf::from("/System/Applications")),
        home.map(|h| h.join("Applications")),
    ];
    dirs.into_iter().flatten().find_map(|dir| {
        let plist = dir.join(format!("{app}.app/Contents/Info.plist"));
        if !plist.is_file() {
            return None;
        }
        run(
            "plutil",
            &[
                "-extract",
                "CFBundleExecutable",
                "raw",
                &plist.to_string_lossy(),
            ],
        )
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    })
}

/// One macOS setting (`defaults read <domain> <key>`), `None` when unset.
pub fn defaults_read(domain: &str, key: &str) -> Option<String> {
    run("defaults", &["read", domain, key])
        .ok()
        .map(|s| s.trim().to_string())
}

/// Run AppleScript (`osascript -e`). Changing settings goes through System
/// Events, which macOS asks you to allow once (Palette already asks it for
/// dark mode and wallpapers).
pub fn apple_script(script: &str) -> Result<String, String> {
    run("osascript", &["-e", script])
}

/// Minutes east of UTC in the Mac's time zone at a unix time, from
/// `date -r <time> +%z` (`+0800` → 480). Asking `date` follows daylight
/// saving without a time-zone library. For the schedule.
pub fn utc_offset_minutes(ts: i64) -> Option<i32> {
    let out = run("date", &["-r", &ts.to_string(), "+%z"]).ok()?;
    parse_utc_offset(&out)
}

/// Are Apple's command-line developer tools (git, …) installed? Without
/// them `/usr/bin/git` is a stub that pops up an install dialog.
/// The menu bar's height on the built-in screen, in points: the notch strip on MacBooks that have
/// one (32 on a 13" Air M4), else the standard menu bar (24). For "Match the menu bar" on the
/// Bar page. Read through AppKit (NSScreen) in a JavaScript-for-Automation one-liner.
pub fn menu_bar_height() -> Option<u32> {
    let script = r#"ObjC.import("AppKit");
const ss = $.NSScreen.screens; let h = 0;
for (let i = 0; i < ss.count; i++) {
  const s = ss.objectAtIndex(i);
  if (/built-in/i.test(ObjC.unwrap(s.localizedName))) { try { h = s.safeAreaInsets.top; } catch (e) {} }
}
h"#;
    let out = run("osascript", &["-l", "JavaScript", "-e", script]).ok()?;
    let notch = out.trim().parse::<f64>().ok().unwrap_or(0.0);
    Some(if notch > 0.0 {
        notch.round() as u32
    } else {
        24
    })
}

pub fn has_developer_tools() -> bool {
    std::process::Command::new("/usr/bin/xcode-select")
        .arg("-p")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Current macOS appearance: `Some(true)` for dark.
pub fn dark_mode() -> Option<bool> {
    let out = Command::new("defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    // The key is absent in light mode, which `defaults` reports as an error.
    Some(String::from_utf8_lossy(&out.stdout).trim() == "Dark")
}

/// Switch macOS light/dark. The first time, macOS asks whether your terminal
/// may control System Events.
pub fn set_dark_mode(dark: bool) -> Result<(), String> {
    let script = format!(
        "tell application \"System Events\" to tell appearance preferences to set dark mode to {dark}"
    );
    run("osascript", &["-e", &script]).map(|_| ())
}

/// Set the wallpaper on every display. The path is passed as an argument,
/// never pasted into the script, so odd file names cannot break out of it.
pub fn set_wallpaper(image: &Path) -> Result<(), String> {
    let path = image
        .to_str()
        .ok_or_else(|| format!("{} is not valid UTF-8", image.display()))?;
    run(
        "osascript",
        &[
            "-e",
            "on run argv",
            "-e",
            "tell application \"System Events\" to tell every desktop to set picture to POSIX file (item 1 of argv)",
            "-e",
            "end run",
            path,
        ],
    )
    .map(|_| ())
}

/// Put text on the clipboard (`pbcopy`), for the app's Copy buttons.
pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    use std::io::Write;
    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run pbcopy: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("no pbcopy input")?
        .write_all(text.as_bytes())
        .map_err(|e| e.to_string())?;
    child.wait().map(|_| ()).map_err(|e| e.to_string())
}

/// Open the Palette app's picker (`overlay`: the quick switcher) through its
/// `palette://` link, which starts the app if it isn't running.
pub fn open_app(overlay: bool) -> Result<(), String> {
    let url = if overlay {
        "palette://overlay"
    } else {
        "palette://picker"
    };
    run("open", &[url])
        .map(|_| ())
        .map_err(|e| format!("{e} (is the Palette app installed?)"))
}

/// Open a file Palette wrote (the inspection sheet) in its default app.
pub fn open_file(path: &Path) -> Result<(), String> {
    run("open", &[&path.to_string_lossy()]).map(|_| ())
}

/// Show a file or folder in Finder: a folder opens, a file is selected in
/// its folder (`open -R`). Only ever given paths inside Palette's folder.
pub fn reveal(path: &Path) -> Result<(), String> {
    let arg = path.to_string_lossy();
    if path.is_dir() {
        run("open", &[&arg]).map(|_| ())
    } else {
        run("open", &["-R", &arg]).map(|_| ())
    }
}
