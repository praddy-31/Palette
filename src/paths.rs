//! Where Palette keeps its files, and where the user's app configs live.

use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

/// The user's home, config folder and Palette's own folder (`~/.config/palette`).
///
/// Passed around explicitly instead of reading the environment everywhere, so
/// tests can point it at a temporary fake home.
#[derive(Debug, Clone)]
pub struct Paths {
    home: PathBuf,
    config_home: PathBuf,
    root: PathBuf,
    fake: bool,
}

impl Paths {
    /// A fake home (tests, trial runs). Palette never reloads apps or changes
    /// macOS settings for a fake home, since those are shared with the real one.
    pub fn fake_home(home: impl Into<PathBuf>) -> Self {
        Self::at(home.into(), None, true)
    }

    fn at(home: PathBuf, xdg_config_home: Option<PathBuf>, fake: bool) -> Self {
        let config_home = xdg_config_home.unwrap_or_else(|| home.join(".config"));
        Self {
            root: config_home.join("palette"),
            home,
            config_home,
            fake,
        }
    }

    /// Resolve from the environment:
    /// 1. `$PALETTE_HOME` as a fake home (configs in `$PALETTE_HOME/.config`)
    /// 2. `$HOME`, with configs in `$XDG_CONFIG_HOME` or `$HOME/.config`
    pub fn from_env() -> Result<Self> {
        resolve(
            env::var_os("PALETTE_HOME"),
            env::var_os("XDG_CONFIG_HOME"),
            env::var_os("HOME"),
        )
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    /// `~/.config` (or `$XDG_CONFIG_HOME`): where most apps keep their config.
    pub fn config_home(&self) -> &Path {
        &self.config_home
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// True for `PALETTE_HOME` runs: files only, no reloads or system changes.
    pub fn is_fake(&self) -> bool {
        self.fake
    }

    /// User themes: `themes/<name>/colors.toml`.
    pub fn themes(&self) -> PathBuf {
        self.root.join("themes")
    }

    /// Symlink to the active generation. User configs import files from here.
    pub fn current(&self) -> PathBuf {
        self.root.join("current")
    }

    /// One folder per `palette set`, newest last.
    pub fn generations(&self) -> PathBuf {
        self.root.join("generations")
    }

    /// Copies of user configs taken by `palette init`, one folder per run.
    pub fn backups(&self) -> PathBuf {
        self.root.join("backup")
    }

    /// Path of the wallpaper Palette last set.
    pub fn wallpaper_state(&self) -> PathBuf {
        self.root.join("wallpaper")
    }

    /// How a shell script should spell `path`: `$HOME/...` when it is inside
    /// the home folder (portable across machines), absolute otherwise.
    /// Safe inside double quotes: `"`, `\`, `` ` `` and `$` in the path are escaped.
    pub fn for_shell(&self, path: &Path) -> String {
        let escape = |p: &Path| {
            p.display()
                .to_string()
                .chars()
                .flat_map(|c| {
                    let backslash = matches!(c, '"' | '\\' | '`' | '$').then_some('\\');
                    backslash.into_iter().chain([c])
                })
                .collect::<String>()
        };
        match path.strip_prefix(&self.home) {
            Ok(rel) => format!("$HOME/{}", escape(rel)),
            Err(_) => escape(path),
        }
    }
}

fn resolve(
    palette_home: Option<OsString>,
    xdg_config_home: Option<OsString>,
    home: Option<OsString>,
) -> Result<Paths> {
    let non_empty = |v: Option<OsString>| v.filter(|s| !s.is_empty()).map(PathBuf::from);

    if let Some(fake_home) = non_empty(palette_home) {
        let fake_home = std::path::absolute(&fake_home).unwrap_or(fake_home);
        return Ok(Paths::at(fake_home, None, true));
    }
    match non_empty(home) {
        Some(home) => Ok(Paths::at(home, non_empty(xdg_config_home), false)),
        None => bail!("cannot find your home folder: $HOME is not set"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(s: &str) -> Option<OsString> {
        Some(s.into())
    }

    #[test]
    fn palette_home_is_a_fake_home() {
        let p = resolve(os("/fake"), os("/xdg"), os("/home")).unwrap();
        assert_eq!(p.root(), Path::new("/fake/.config/palette"));
        assert_eq!(p.config_home(), Path::new("/fake/.config"));
        assert!(p.is_fake());
    }

    #[test]
    fn real_home_with_and_without_xdg() {
        let p = resolve(None, os("/xdg"), os("/home")).unwrap();
        assert_eq!(p.root(), Path::new("/xdg/palette"));
        assert!(!p.is_fake());
        let p = resolve(None, os(""), os("/home")).unwrap();
        assert_eq!(p.root(), Path::new("/home/.config/palette"));
    }

    #[test]
    fn no_home_is_an_error() {
        assert!(resolve(None, None, None).is_err());
    }

    #[test]
    fn shell_spelling() {
        let p = Paths::fake_home("/Users/me");
        assert_eq!(
            p.for_shell(&p.current().join("a.sh")),
            "$HOME/.config/palette/current/a.sh"
        );
        assert_eq!(p.for_shell(Path::new("/opt/x")), "/opt/x");
        assert_eq!(p.for_shell(Path::new("/opt/a\"$b`")), "/opt/a\\\"\\$b\\`");
    }
}
