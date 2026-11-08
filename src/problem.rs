//! Errors the app acts on, not just shows (ARCHITECTURE 14.4). The core keeps using
//! `anyhow`; where a failure has a known cause, it bails with a [`Problem`] instead of
//! plain text. The message reads the same as before, so the CLI doesn't change, but the
//! app can find the cause with [`of`] and answer it (offer the brew command, point at
//! System Settings, select the name field) instead of matching on words.

use std::fmt;

/// The core's error type (`anyhow`), named here so the app can look for a [`Problem`]
/// without depending on `anyhow` itself.
pub type Error = anyhow::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// A program Palette needs isn't installed. `hint`: how to get it.
    NotInstalled { app: String, hint: String },
    /// No theme by this name.
    NoSuchTheme { name: String },
    /// Can't do this to the active theme (remove it, say).
    ThemeActive { name: String },
    /// A theme with this name already exists.
    NameTaken { name: String },
    /// Nothing has been switched to yet.
    NoThemeYet,
    /// A link that has no theme colours (colors.toml or alacritty.toml): maybe wallpapers.
    NotATheme { link: String },
    /// A download bigger than any theme should be (500 MB).
    TooBig { link: String },
    /// macOS hasn't allowed Palette to control another app (System Events):
    /// System Settings → Privacy & Security → Automation.
    NeedsPermission { what: String },
}

impl Problem {
    /// A short, stable code for the app (`not-installed`, `theme-active`, …).
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotInstalled { .. } => "not-installed",
            Self::NoSuchTheme { .. } => "no-such-theme",
            Self::ThemeActive { .. } => "theme-active",
            Self::NameTaken { .. } => "name-taken",
            Self::NoThemeYet => "no-theme-yet",
            Self::NotATheme { .. } => "not-a-theme",
            Self::TooBig { .. } => "too-big",
            Self::NeedsPermission { .. } => "needs-permission",
        }
    }

    /// What it's about: the app, the theme or the permission.
    pub fn subject(&self) -> Option<&str> {
        match self {
            Self::NotInstalled { app, .. } => Some(app),
            Self::NoSuchTheme { name } | Self::ThemeActive { name } | Self::NameTaken { name } => {
                Some(name)
            }
            Self::NoThemeYet => None,
            Self::NotATheme { link } | Self::TooBig { link } => Some(link),
            Self::NeedsPermission { what } => Some(what),
        }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInstalled { app, hint } => write!(f, "{app} is not installed: {hint}"),
            Self::NoSuchTheme { name } => write!(
                f,
                "no theme named `{name}`. Run `palette list` to see the available themes"
            ),
            Self::ThemeActive { name } => {
                write!(
                    f,
                    "`{name}` is the active theme: switch to another one first"
                )
            }
            Self::NameTaken { name } => {
                write!(
                    f,
                    "a theme called `{name}` already exists; pick another name"
                )
            }
            Self::NoThemeYet => {
                write!(f, "no theme is active yet: run `palette set <theme>` first")
            }
            Self::NotATheme { link } => write!(
                f,
                "{link} has no colors.toml or alacritty.toml: not a theme Palette can read"
            ),
            Self::TooBig { link } => write!(
                f,
                "downloading {link}: bigger than 500 MB, too big to be a theme"
            ),
            Self::NeedsPermission { what } => write!(
                f,
                "macOS hasn't allowed Palette to control {what}: System Settings → \
                 Privacy & Security → Automation, then turn {what} on for Palette (or your terminal)"
            ),
        }
    }
}

impl std::error::Error for Problem {}

/// The [`Problem`] behind an error, if it has one (anywhere in its chain of causes).
pub fn of(e: &anyhow::Error) -> Option<&Problem> {
    e.chain().find_map(|c| c.downcast_ref::<Problem>())
}

/// macOS's answer when an app may not send Apple events (osascript error -1743).
pub fn is_automation_denied(message: &str) -> bool {
    message.contains("-1743") || message.contains("Not authorized to send Apple events")
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Context, bail};

    fn fails() -> anyhow::Result<()> {
        bail!(Problem::ThemeActive {
            name: "nord".into()
        })
    }

    #[test]
    fn found_through_context_and_reads_the_same() {
        let e = fails().context("removing nord").unwrap_err();
        let p = of(&e).unwrap();
        assert_eq!(p.code(), "theme-active");
        assert_eq!(p.subject(), Some("nord"));
        assert_eq!(
            format!("{e:#}"),
            "removing nord: `nord` is the active theme: switch to another one first"
        );
        assert!(of(&anyhow::anyhow!("plain text")).is_none());
    }

    #[test]
    fn automation_denial_is_recognised() {
        assert!(is_automation_denied(
            "execution error: Not authorized to send Apple events to System Events. (-1743)"
        ));
        assert!(!is_automation_denied("execution error: (-128)"));
    }
}
