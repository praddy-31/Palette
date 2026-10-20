//! Everything that runs another program or changes macOS. Kept in one place so
//! it is easy to audit, and never called for a fake home (see `effects`).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub mod fonts;

/// Result of a side effect, for the report printed after a switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Done(String),
    Unchanged(String),
    Skipped(String),
    Failed(String),
}

/// Is a process with exactly this name running?
pub fn is_running(name: &str) -> bool {
    Command::new("pgrep")
        .args(["-x", name])
        .stdout(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Homebrew's folders. Apps opened from Finder get a minimal `PATH`
/// (`/usr/bin:/bin:/usr/sbin:/sbin`) without them.
const EXTRA_PATH: [&str; 2] = ["/opt/homebrew/bin", "/usr/local/bin"];

/// `PATH` plus Homebrew's folders, for every program Palette runs, so the
/// menu bar app finds `sketchybar`, and scripts it runs (`bordersrc`) find
/// `borders`, exactly as they do from a terminal.
fn search_path() -> std::ffi::OsString {
    let current = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&current).collect();
    for extra in EXTRA_PATH.map(PathBuf::from) {
        if !dirs.contains(&extra) {
            dirs.push(extra);
        }
    }
    std::env::join_paths(dirs).unwrap_or(current)
}

/// Every copy of a command on `search_path()`, in the order a shell tries
/// them (duplicates and links to the same file counted once).
pub fn find_all_programs(name: &str) -> Vec<PathBuf> {
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut out = Vec::new();
    for dir in std::env::split_paths(&search_path()) {
        let candidate = dir.join(name);
        if !candidate.is_file() {
            continue;
        }
        let real = std::fs::canonicalize(&candidate).unwrap_or(candidate.clone());
        if !seen.contains(&real) {
            seen.push(real);
            out.push(candidate);
        }
    }
    out
}

/// Where a command-line tool lives, searching `search_path()`.
pub fn find_program(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&search_path())
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

/// Run a program, turning a non-zero exit into an error message.
pub fn run(program: &str, args: &[&str]) -> Result<String, String> {
    run_in(None, program, args)
}

/// `run`, with a working directory.
pub fn run_in(dir: Option<&Path>, program: &str, args: &[&str]) -> Result<String, String> {
    // Resolve bare names ourselves: `Command` searches the parent's `PATH`.
    let resolved = match program.contains('/') {
        true => None,
        false => find_program(program),
    };
    let mut command = Command::new(resolved.as_deref().unwrap_or(Path::new(program)));
    command.env("PATH", search_path());
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    let out = command
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not run {program}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(format!("{program} failed: {}", err.trim()))
    }
}

/// `run`, with extra environment variables (`None` removes one), returning
/// the output untrimmed (a prompt's spacing matters).
pub fn run_env(
    program: &str,
    args: &[&str],
    env: &[(&str, Option<&str>)],
) -> Result<String, String> {
    let resolved = find_program(program).ok_or_else(|| format!("{program} isn't installed"))?;
    let mut command = Command::new(resolved);
    command.env("PATH", search_path());
    for (k, v) in env {
        match v {
            Some(v) => command.env(k, v),
            None => command.env_remove(k),
        };
    }
    let out = command
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not run {program}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
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

/// `+0530` → 330, `-0800` → -480.
pub fn parse_utc_offset(text: &str) -> Option<i32> {
    let text = text.trim();
    let sign = match text.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let digits = text
        .get(1..5)
        .filter(|d| d.bytes().all(|b| b.is_ascii_digit()))?;
    let (h, m): (i32, i32) = (digits[..2].parse().ok()?, digits[2..].parse().ok()?);
    Some(sign * (h * 60 + m))
}

/// The Mac's time zone name (`Asia/Singapore`), from where `/etc/localtime`
/// points. Read-only; no permission needed.
pub fn time_zone_name() -> Option<String> {
    let target = std::fs::read_link("/etc/localtime").ok()?;
    let text = target.to_string_lossy();
    text.split_once("zoneinfo/")
        .map(|(_, zone)| zone.to_string())
}

/// Are Apple's command-line developer tools (git, …) installed? Without
/// them `/usr/bin/git` is a stub that pops up an install dialog.
pub fn has_developer_tools() -> bool {
    std::process::Command::new("/usr/bin/xcode-select")
        .arg("-p")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Send a signal (e.g. `USR2`) to every process with this exact name.
pub fn signal(name: &str, signal: &str) -> Result<(), String> {
    run("pkill", &[&format!("-{signal}"), "-x", name]).map(|_| ())
}

/// `(major, minor)` from output like `Ghostty 1.3.1`.
pub fn parse_version(text: &str) -> Option<(u32, u32)> {
    let word = text
        .split_whitespace()
        .find(|w| w.starts_with(|c: char| c.is_ascii_digit()))?;
    let mut parts = word.trim_start_matches('v').split('.');
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_homebrew_programs_with_a_minimal_path() {
        // What an app opened from Finder gets.
        let minimal = "/usr/bin:/bin:/usr/sbin:/sbin";
        let out = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "system::tests::print_search_path", "--nocapture"])
            .env("PATH", minimal)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            text.contains("SEARCH=/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin:/usr/local/bin"),
            "{text}"
        );
    }

    #[test]
    fn print_search_path() {
        println!("SEARCH={}", search_path().to_string_lossy());
    }

    #[test]
    fn versions() {
        assert_eq!(parse_version("Ghostty 1.3.1\n\nVersion"), Some((1, 3)));
        assert_eq!(parse_version("borders-v1.9.0"), None);
        assert_eq!(
            parse_version("kitty 0.42.1 created by Kovid"),
            Some((0, 42))
        );
        assert_eq!(parse_version("nothing here"), None);
    }
}
