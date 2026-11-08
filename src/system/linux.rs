//! The Linux half of `system`: the same functions as `system/macos.rs`, done
//! the way the desktop you're logged into wants (see `session`). Written on a
//! Mac and only type-checked there; every command is checked on Ubuntu in
//! Phase 8 (`docs/user_test.md`).

use std::path::Path;
use std::process::{Command, Stdio};

use super::session::Session;
use super::{find_program, parse_utc_offset, run, run_env};

/// Variables a desktop's tools need to find the running desktop
/// (`noctalia msg` uses `$XDG_RUNTIME_DIR/noctalia-$WAYLAND_DISPLAY.sock`).
const DESKTOP_VARS: [&str; 5] = [
    "WAYLAND_DISPLAY",
    "DISPLAY",
    "NIRI_SOCKET",
    "DBUS_SESSION_BUS_ADDRESS",
    "XDG_CURRENT_DESKTOP",
];

/// Run a program that talks to the running desktop. Started outside it (an
/// SSH login, a timer) the desktop's variables are missing, so they're
/// borrowed from the desktop's own process (`/proc/<pid>/environ`, readable
/// for your own processes only).
pub fn run_desktop(program: &str, args: &[&str]) -> Result<String, String> {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return run(program, args);
    }
    let borrowed = desktop_environment();
    let env: Vec<(&str, Option<&str>)> = borrowed
        .iter()
        .map(|(k, v)| (k.as_str(), Some(v.as_str())))
        .collect();
    run_env(program, args, &env).map(|out| out.trim().to_string())
}

/// The desktop variables of one of your processes started inside the
/// desktop (Noctalia, Plasma's shell, any app). Not the compositor itself
/// (niri, GNOME Shell): it creates the display and only hands its name to the
/// programs it starts. Services from an earlier login (evolution, dconf) keep
/// that login's dead display, so only a display that answers counts, and the
/// newest process wins.
fn desktop_environment() -> Vec<(String, String)> {
    use std::os::unix::fs::MetadataExt;
    let Ok(me) = std::fs::metadata("/proc/self").map(|m| m.uid()) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let candidates = entries
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .bytes()
                .all(|b| b.is_ascii_digit())
        })
        .filter(|e| e.metadata().is_ok_and(|m| m.uid() == me))
        .filter_map(|e| {
            let env = parse_environ(&std::fs::read(e.path().join("environ")).ok()?);
            let stat = std::fs::read_to_string(e.path().join("stat")).ok()?;
            Some((start_time(&stat)?, env))
        });
    newest_live(candidates, &runtime_dir())
}

/// Where Wayland sockets live: `$XDG_RUNTIME_DIR`, else `/run/user/<uid>`.
fn runtime_dir() -> std::path::PathBuf {
    use std::os::unix::fs::MetadataExt;
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(Into::into)
        .or_else(|| {
            let uid = std::fs::metadata("/proc/self").ok()?.uid();
            Some(format!("/run/user/{uid}").into())
        })
        .unwrap_or_default()
}

/// A process's start time in clock ticks since boot: field 22 of
/// `/proc/<pid>/stat`, counted after the `(name)`, which may hold spaces.
fn start_time(stat: &str) -> Option<u64> {
    let (_, rest) = stat.rsplit_once(") ")?;
    rest.split_whitespace().nth(19)?.parse().ok()
}

/// The environment of the newest `(start, env)` whose `WAYLAND_DISPLAY`
/// socket accepts a connection; each display is tried once.
fn newest_live(
    candidates: impl Iterator<Item = (u64, Vec<(String, String)>)>,
    runtime: &Path,
) -> Vec<(String, String)> {
    let mut alive = std::collections::HashMap::new();
    candidates
        .filter(|(_, env)| {
            let Some((_, display)) = env.iter().find(|(k, _)| k == "WAYLAND_DISPLAY") else {
                return false;
            };
            *alive.entry(display.clone()).or_insert_with(|| {
                std::os::unix::net::UnixStream::connect(runtime.join(display)).is_ok()
            })
        })
        .max_by_key(|(start, _)| *start)
        .map(|(_, env)| env)
        .unwrap_or_default()
}

/// `KEY=value\0KEY=value\0…`, keeping only `DESKTOP_VARS`.
pub fn parse_environ(bytes: &[u8]) -> Vec<(String, String)> {
    bytes
        .split(|b| *b == 0)
        .filter_map(|entry| {
            let (k, v) = std::str::from_utf8(entry).ok()?.split_once('=')?;
            DESKTOP_VARS
                .contains(&k)
                .then(|| (k.to_string(), v.to_string()))
        })
        .collect()
}

/// macOS apps have an inner program name; Linux programs are just their name.
pub fn app_executable(_app: &str) -> Option<String> {
    None
}

/// macOS settings (`defaults`) don't exist here.
pub fn defaults_read(_domain: &str, _key: &str) -> Option<String> {
    None
}

pub fn apple_script(_script: &str) -> Result<String, String> {
    Err("AppleScript is macOS only".into())
}

/// Minutes east of UTC at a unix time: GNU `date -d @<time> +%z`.
pub fn utc_offset_minutes(ts: i64) -> Option<i32> {
    let out = run("date", &["-d", &format!("@{ts}"), "+%z"]).ok()?;
    parse_utc_offset(&out)
}

/// git is a normal package on Linux (no install-dialog stub as on macOS).
pub fn has_developer_tools() -> bool {
    find_program("git").is_some()
}

const GNOME_INTERFACE: &str = "org.gnome.desktop.interface";

/// Current light/dark: GNOME's `color-scheme`, which GTK apps, the desktop
/// portal and Noctalia (its `sync_color_scheme`) all use. `None` where it
/// isn't the source of truth (KDE: the colour scheme decides).
/// macOS only (the Bar page is Mac-only): no menu bar height to match.
pub fn menu_bar_height() -> Option<u32> {
    None
}

pub fn dark_mode() -> Option<bool> {
    match Session::detect() {
        Session::Kde => None,
        _ => {
            let out = run("gsettings", &["get", GNOME_INTERFACE, "color-scheme"]).ok()?;
            Some(out.contains("prefer-dark"))
        }
    }
}

/// Switch light/dark. Noctalia: `noctalia msg theme-mode-set`; GNOME and plain
/// niri: GNOME's `color-scheme`, live; KDE: nothing to do here, the `kde`
/// target applies a light or a dark colour scheme.
pub fn set_dark_mode(dark: bool) -> Result<(), String> {
    match Session::detect() {
        Session::Kde => Ok(()),
        Session::Niri { noctalia: true } => {
            let mode = if dark { "dark" } else { "light" };
            run_desktop("noctalia", &["msg", "theme-mode-set", mode]).map(|_| ())
        }
        _ => {
            let scheme = if dark { "prefer-dark" } else { "prefer-light" };
            run(
                "gsettings",
                &["set", GNOME_INTERFACE, "color-scheme", scheme],
            )
            .map(|_| ())
        }
    }
}

/// Set the wallpaper on every display, the way the desktop does it.
pub fn set_wallpaper(image: &Path) -> Result<(), String> {
    let path = image
        .to_str()
        .ok_or_else(|| format!("{} is not valid UTF-8", image.display()))?;
    match Session::detect() {
        Session::Gnome => {
            // Both keys: GNOME shows `picture-uri-dark` in dark mode.
            let uri = file_uri(path);
            for key in ["picture-uri", "picture-uri-dark"] {
                run(
                    "gsettings",
                    &["set", "org.gnome.desktop.background", key, &uri],
                )?;
            }
            Ok(())
        }
        Session::Kde => run_desktop("plasma-apply-wallpaperimage", &[path]).map(|_| ()),
        Session::Niri { noctalia: true } => {
            run_desktop("noctalia", &["msg", "wallpaper-set", path]).map(|_| ())
        }
        Session::Niri { noctalia: false } => {
            Err("on niri Palette sets the wallpaper through Noctalia, which isn't running".into())
        }
        other => Err(format!(
            "wallpapers on {} aren't supported yet",
            other.label()
        )),
    }
}

/// `/home/me/My Walls/a.png` → `file:///home/me/My%20Walls/a.png`.
pub fn file_uri(path: &str) -> String {
    let mut out = String::from("file://");
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Put text on the clipboard: `wl-copy` on Wayland, else `xclip`.
pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    use std::io::Write;
    let (program, args): (&str, &[&str]) = if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        ("wl-copy", &[])
    } else {
        ("xclip", &["-selection", "clipboard"])
    };
    let resolved = find_program(program).ok_or_else(|| format!("{program} isn't installed"))?;
    let mut child = Command::new(resolved)
        .args(args)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run {program}: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("no clipboard input")?
        .write_all(text.as_bytes())
        .map_err(|e| e.to_string())?;
    child.wait().map(|_| ()).map_err(|e| e.to_string())
}

/// Open the Palette app's picker (`overlay`: the quick switcher), starting
/// the app if it isn't running; a running app gets the request through its
/// single-instance check. The app is the `palette-app` next to this command
/// (a build folder, the app's own folder), else the one on `PATH`. It gets
/// its own process group, so closing the terminal doesn't close it.
pub fn open_app(overlay: bool) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    let app = std::env::current_exe()
        .ok()
        .map(|me| me.with_file_name("palette-app"))
        .filter(|p| p.is_file())
        .or_else(|| find_program("palette-app"))
        .ok_or(
            "the Palette app isn't installed (no palette-app next to this command or on PATH)",
        )?;
    let mut command = Command::new(&app);
    if overlay {
        command.arg("--quick");
    }
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        command.envs(desktop_environment());
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not start {}: {e}", app.display()))
}

/// Open a file Palette wrote (the inspection sheet) in its default app.
pub fn open_file(path: &Path) -> Result<(), String> {
    run("xdg-open", &[&path.to_string_lossy()]).map(|_| ())
}

/// Open a folder in the file manager; for a file, the folder it is in.
pub fn reveal(path: &Path) -> Result<(), String> {
    let folder = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    run("xdg-open", &[&folder.to_string_lossy()]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_desktop_variables() {
        let env = parse_environ(b"HOME=/h\0WAYLAND_DISPLAY=wayland-1\0DISPLAY=:0\0\0");
        assert_eq!(
            env,
            [
                ("WAYLAND_DISPLAY".to_string(), "wayland-1".to_string()),
                ("DISPLAY".to_string(), ":0".to_string())
            ]
        );
    }

    #[test]
    fn start_time_survives_spaces_in_the_name() {
        let stat =
            "7314 (plasma shell) S 1 7314 7314 0 -1 4194560 1 2 3 4 5 6 7 8 20 0 9 0 98765 1 2";
        assert_eq!(start_time(stat), Some(98765));
    }

    /// Found on the VM: services from an earlier niri login kept
    /// `wayland-1`, whose socket was dead, while KDE ran on `wayland-0`.
    #[test]
    fn borrows_the_newest_display_that_answers() {
        use std::os::unix::net::UnixListener;
        let dir = tempfile::tempdir().unwrap();
        let _live = UnixListener::bind(dir.path().join("wayland-0")).unwrap();
        drop(UnixListener::bind(dir.path().join("wayland-1")).unwrap()); // file stays, nobody listens
        let env = |display: &str, desktop: &str| {
            vec![
                ("WAYLAND_DISPLAY".to_string(), display.to_string()),
                ("XDG_CURRENT_DESKTOP".to_string(), desktop.to_string()),
            ]
        };
        let candidates = vec![
            (100, env("wayland-0", "KDE-old")),
            (200, env("wayland-0", "KDE")),
            (300, env("wayland-1", "niri")),
            (400, vec![("DISPLAY".to_string(), ":0".to_string())]),
        ];
        assert_eq!(
            newest_live(candidates.into_iter(), dir.path()),
            env("wayland-0", "KDE")
        );
        assert!(newest_live(std::iter::empty(), dir.path()).is_empty());
    }

    #[test]
    fn file_uris_escape_spaces() {
        assert_eq!(
            file_uri("/home/me/My Walls/a.png"),
            "file:///home/me/My%20Walls/a.png"
        );
    }
}
