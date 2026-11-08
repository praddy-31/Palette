//! Everything that runs another program or changes the desktop. Kept in one
//! place so it is easy to audit, and never called for a fake home (see
//! `effects`). What differs between macOS and Linux lives in `system/macos.rs`
//! and `system/linux.rs`, with the same function names; only the one for the
//! system being built is compiled (`#[cfg(target_os = ...)]`).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[cfg(target_os = "macos")]
pub mod displays;
#[cfg(not(target_os = "macos"))]
#[path = "system/displays_linux.rs"]
pub mod displays;
#[cfg(target_os = "macos")]
pub mod fonts;
#[cfg(not(target_os = "macos"))]
#[path = "system/fonts_linux.rs"]
pub mod fonts;
pub mod session;

#[cfg(not(target_os = "macos"))]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
pub use linux::*;
#[cfg(target_os = "macos")]
pub use macos::*;

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
#[cfg(target_os = "macos")]
fn extra_path() -> Vec<PathBuf> {
    ["/opt/homebrew/bin", "/usr/local/bin"]
        .map(PathBuf::from)
        .to_vec()
}

/// Your own folders (`~/.local/bin`, where Ubuntu users link `bat` to
/// `batcat`; `~/.cargo/bin`): an SSH command or a launcher started by the
/// desktop may not have them on `PATH`.
#[cfg(not(target_os = "macos"))]
fn extra_path() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    [".local/bin", ".cargo/bin"]
        .iter()
        .filter_map(|dir| home.as_ref().map(|h| h.join(dir)))
        .collect()
}

/// `PATH` plus `extra_path()`, for every program Palette runs, so the
/// menu bar app finds `sketchybar`, and scripts it runs (`bordersrc`) find
/// `borders`, exactly as they do from a terminal.
fn search_path() -> std::ffi::OsString {
    let current = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&current).collect();
    for extra in extra_path() {
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

/// `sketchybar --reload`, never two close together. A reload runs sketchybarrc in the
/// background; a second one while it runs interleaves the two runs' `--add` commands and
/// scrambles the bar (items between other pills, missing icons). So a reload waits until
/// [`SKETCHYBAR_GAP`] has passed since the last one this process sent.
pub fn reload_sketchybar() -> Result<String, String> {
    static LAST: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(wait) = last.and_then(|t| SKETCHYBAR_GAP.checked_sub(t.elapsed())) {
        std::thread::sleep(wait);
    }
    let out = run("sketchybar", &["--reload"]);
    *last = Some(std::time::Instant::now());
    out
}

/// Time a sketchybarrc needs to finish (the owner's, about 60 commands, takes well under this).
pub const SKETCHYBAR_GAP: std::time::Duration = std::time::Duration::from_millis(2500);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
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
