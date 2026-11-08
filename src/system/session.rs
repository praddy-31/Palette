//! Which system and desktop Palette is running in: macOS, or on Linux the
//! desktop you logged into (GNOME, KDE Plasma, niri with or without Noctalia).
//!
//! Asked fresh every time, never saved: with several desktops installed you
//! pick one at the login screen, and that session's `XDG_CURRENT_DESKTOP`
//! says which. Over SSH (no desktop variables): the graphical session's
//! `Desktop` from systemd's `loginctl`, else which desktop program runs as
//! you (GDM leaves `Desktop` empty, seen on Ubuntu 26.04).

use std::ffi::OsString;

use super::{find_program, run};

/// The system Palette was built for, or (only with a fake home,
/// `PALETTE_HOME`) the one `PALETTE_PLATFORM=linux|macos` asks for, so tests
/// and manual runs on a Mac can see what Palette does on Linux without
/// touching anything real.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Linux,
}

impl Platform {
    pub fn current() -> Self {
        if std::env::var_os("PALETTE_HOME").is_some()
            && let Some(asked) = std::env::var_os("PALETTE_PLATFORM")
        {
            match asked.to_string_lossy().to_ascii_lowercase().as_str() {
                "linux" => return Self::Linux,
                "macos" | "mac" => return Self::Mac,
                _ => {}
            }
        }
        if cfg!(target_os = "macos") {
            Self::Mac
        } else {
            Self::Linux
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mac => "macos",
            Self::Linux => "linux",
        }
    }
}

/// The desktop session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Session {
    Mac,
    Gnome,
    Kde,
    /// niri; `noctalia` when the Noctalia shell is running (it then owns the
    /// wallpaper and the bar's colours).
    Niri {
        noctalia: bool,
    },
    /// A Linux desktop Palette doesn't know (its `XDG_CURRENT_DESKTOP`, or
    /// empty when there is none, e.g. a plain SSH login).
    Other(String),
}

impl Session {
    pub fn detect() -> Self {
        if Platform::current() == Platform::Mac {
            return Self::Mac;
        }
        // Tests and trying Linux on a Mac: `PALETTE_SESSION=gnome|kde|niri|
        // niri-noctalia`, honoured only with a fake home like `PALETTE_PLATFORM`.
        if std::env::var_os("PALETTE_HOME").is_some()
            && let Ok(asked) = std::env::var("PALETTE_SESSION")
        {
            return Self::from_id(&asked);
        }
        let desktop = std::env::var("XDG_CURRENT_DESKTOP")
            .ok()
            .filter(|d| !d.is_empty())
            .or_else(graphical_session_desktop)
            .or_else(|| running_desktop().map(String::from));
        let niri_socket = std::env::var_os("NIRI_SOCKET").is_some_and(|s| !s.is_empty());
        let noctalia = runs_as_me("noctalia");
        Self::from_parts(desktop.as_deref(), niri_socket, noctalia)
    }

    /// `desktop` is `XDG_CURRENT_DESKTOP` (a `:`-separated list, Ubuntu's
    /// GNOME says `ubuntu:GNOME`) or a session's `Desktop` from `loginctl`
    /// (`ubuntu`, `KDE`, `niri`).
    pub fn from_parts(desktop: Option<&str>, niri_socket: bool, noctalia: bool) -> Self {
        let names: Vec<String> = desktop
            .unwrap_or_default()
            .split(':')
            .map(|n| n.trim().to_ascii_lowercase())
            .filter(|n| !n.is_empty())
            .collect();
        let has = |want: &[&str]| names.iter().any(|n| want.contains(&n.as_str()));
        if niri_socket || has(&["niri"]) {
            Self::Niri { noctalia }
        } else if has(&["kde", "plasma"]) {
            Self::Kde
        } else if has(&["gnome", "ubuntu", "gnome-classic"]) {
            Self::Gnome
        } else {
            Self::Other(desktop.unwrap_or_default().to_string())
        }
    }

    /// For the app's header and `palette doctor`: "GNOME", "niri + Noctalia".
    pub fn label(&self) -> String {
        match self {
            Self::Mac => "macOS".into(),
            Self::Gnome => "GNOME".into(),
            Self::Kde => "KDE Plasma".into(),
            Self::Niri { noctalia: true } => "niri + Noctalia".into(),
            Self::Niri { noctalia: false } => "niri".into(),
            Self::Other(d) if d.is_empty() => "no desktop found".into(),
            Self::Other(d) => format!("{d} (not supported yet)"),
        }
    }

    /// Short name for scripts and the app: `macos`, `gnome`, `kde`,
    /// `niri-noctalia`, `niri`, `other`.
    pub fn id(&self) -> &'static str {
        match self {
            Self::Mac => "macos",
            Self::Gnome => "gnome",
            Self::Kde => "kde",
            Self::Niri { noctalia: true } => "niri-noctalia",
            Self::Niri { noctalia: false } => "niri",
            Self::Other(_) => "other",
        }
    }

    /// The session for an [`id`](Self::id).
    pub fn from_id(id: &str) -> Self {
        match id {
            "macos" => Self::Mac,
            "gnome" => Self::Gnome,
            "kde" => Self::Kde,
            "niri-noctalia" => Self::Niri { noctalia: true },
            "niri" => Self::Niri { noctalia: false },
            other => Self::Other(other.to_string()),
        }
    }
}

/// The `Desktop` of this user's graphical session, from systemd-logind:
/// `loginctl show-user <uid> -p Display` names the session that owns the
/// screen, `show-session <id> -p Desktop` its desktop.
fn graphical_session_desktop() -> Option<String> {
    find_program("loginctl")?;
    let user = std::env::var_os("USER").map(OsString::into_string)?.ok()?;
    let session = run(
        "loginctl",
        &["show-user", &user, "-p", "Display", "--value"],
    )
    .ok()?;
    if session.is_empty() {
        return None;
    }
    run(
        "loginctl",
        &["show-session", &session, "-p", "Desktop", "--value"],
    )
    .ok()
    .filter(|d| !d.is_empty())
}

/// The desktop whose program runs as this user: `niri`, `plasmashell` or
/// `gnome-shell` (in that order: a niri or KDE login may leave GNOME helpers).
fn running_desktop() -> Option<&'static str> {
    [
        ("niri", "niri"),
        ("plasmashell", "KDE"),
        ("gnome-shell", "GNOME"),
    ]
    .into_iter()
    .find(|(program, _)| runs_as_me(program))
    .map(|(_, desktop)| desktop)
}

/// Is a process with exactly this name running as the current user? (Not
/// `is_running`: GDM's login screen runs a `gnome-shell` of its own.)
fn runs_as_me(name: &str) -> bool {
    let Some(user) = std::env::var_os("USER") else {
        return false;
    };
    run("pgrep", &["-u", &user.to_string_lossy(), "-x", name]).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktops_from_their_names() {
        assert_eq!(
            Session::from_parts(Some("ubuntu:GNOME"), false, false),
            Session::Gnome
        );
        assert_eq!(
            Session::from_parts(Some("ubuntu"), false, false),
            Session::Gnome
        );
        assert_eq!(Session::from_parts(Some("KDE"), false, false), Session::Kde);
        assert_eq!(
            Session::from_parts(Some("niri"), false, true),
            Session::Niri { noctalia: true }
        );
        // niri's socket wins even if the desktop variable says something else.
        assert_eq!(
            Session::from_parts(Some("GNOME"), true, false),
            Session::Niri { noctalia: false }
        );
        assert_eq!(
            Session::from_parts(Some("Hyprland"), false, false),
            Session::Other("Hyprland".into())
        );
        assert_eq!(
            Session::from_parts(None, false, false).label(),
            "no desktop found"
        );
    }
}
