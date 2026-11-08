//! The app's launcher entry on Linux: `~/.local/share/applications/palette.desktop`,
//! the file app launchers (fuzzel, GNOME's dash, KDE's menu, Noctalia's
//! launcher) list apps from. Right-click (or the launcher's actions) offers
//! "Quick switch" and "Settings". The app writes it at every start, so the
//! path stays right after a rebuild or a move; an app installed by a package
//! (under `/usr`) brings its own and is left alone. It's Palette's own file;
//! `palette init --remove` deletes it.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::paths::Paths;

pub fn path(paths: &Paths) -> PathBuf {
    paths
        .home()
        .join(".local/share/applications/palette.desktop")
}

/// The icon the entry points at, in Palette's own folder.
pub fn icon_path(paths: &Paths) -> PathBuf {
    paths.root().join("palette.png")
}

/// Write the entry for the app at `app` (with its icon) if it's missing or
/// different. True when something was written.
pub fn install(paths: &Paths, app: &Path, icon_png: &[u8]) -> Result<bool> {
    if app.starts_with("/usr") {
        return Ok(false);
    }
    let icon = icon_path(paths);
    let text = desktop_file(app, &icon);
    let file = path(paths);
    if fs::read_to_string(&file).is_ok_and(|t| t == text) && icon.is_file() {
        return Ok(false);
    }
    fs::create_dir_all(paths.root())?;
    fs::write(&icon, icon_png)?;
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(&file, text)?;
    Ok(true)
}

/// Delete the entry (and its icon). True when there was one.
pub fn remove(paths: &Paths) -> Result<bool> {
    let file = path(paths);
    if !file.is_file() {
        return Ok(false);
    }
    fs::remove_file(file)?;
    let _ = fs::remove_file(icon_path(paths));
    Ok(true)
}

pub fn desktop_file(app: &Path, icon: &Path) -> String {
    let exec = quote(&app.to_string_lossy());
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Palette\n\
         GenericName=Theme switcher\n\
         Comment=Switch the whole desktop's theme and wallpaper\n\
         Exec={exec}\n\
         Icon={icon}\n\
         Terminal=false\n\
         Categories=Settings;DesktopSettings;\n\
         Keywords=theme;colours;colors;wallpaper;dark;light;\n\
         StartupWMClass=palette-app\n\
         Actions=quick;settings;\n\
         \n\
         [Desktop Action quick]\n\
         Name=Quick switch\n\
         Exec={exec} --quick\n\
         \n\
         [Desktop Action settings]\n\
         Name=Settings\n\
         Exec={exec} --settings\n",
        icon = icon.display(),
    )
}

/// An `Exec=` argument: quoted when it has spaces or quotes, with `"`, `` ` ``,
/// `$` and `\` escaped (the desktop entry spec).
pub fn quote(arg: &str) -> String {
    if !arg.contains([' ', '"', '\'', '\\', '`', '$']) {
        return arg.to_string();
    }
    let mut out = String::from("\"");
    for c in arg.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_has_both_actions() {
        let text = desktop_file(Path::new("/home/me/palette-app"), Path::new("/i.png"));
        assert!(text.contains("Exec=/home/me/palette-app\n"));
        assert!(text.contains("Exec=/home/me/palette-app --quick\n"));
        assert!(text.contains("Exec=/home/me/palette-app --settings\n"));
        assert!(text.contains("Actions=quick;settings;"));
    }

    #[test]
    fn paths_with_spaces_are_quoted() {
        assert_eq!(
            quote("/home/me/My Apps/palette-app"),
            "\"/home/me/My Apps/palette-app\""
        );
        assert_eq!(quote("/a/$b"), "\"/a/\\$b\"");
        assert_eq!(quote("/plain"), "/plain");
    }

    #[test]
    fn writes_once_and_removes() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        let app = Path::new("/home/me/palette-app");
        assert!(install(&paths, app, b"png").unwrap());
        assert!(!install(&paths, app, b"png").unwrap());
        assert!(path(&paths).is_file());
        assert!(!install(&paths, Path::new("/usr/bin/palette-app"), b"png").unwrap());
        assert!(remove(&paths).unwrap());
        assert!(!path(&paths).exists());
        assert!(!remove(&paths).unwrap());
    }
}
