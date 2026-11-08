//! The `palette` command-line tool inside the app, and linking it onto PATH.

use super::*;

/// The `palette` CLI shipped inside the app (`Contents/MacOS/palette-cli`;
/// the terminal command is still `palette`, a link to it).
pub(crate) fn bundled_cli() -> Option<std::path::PathBuf> {
    let cli = std::env::current_exe().ok()?.with_file_name("palette-cli");
    cli.is_file().then_some(cli)
}

/// Apps before 1.1 shipped the CLI as `Contents/MacOS/palette`: point a
/// `~/.local/bin/palette` link made then at the new name. Only links into
/// an app bundle are touched.
pub(crate) fn relink_old_cli() {
    let (Some(cli), Ok(link)) = (bundled_cli(), cli_link()) else {
        return;
    };
    let Ok(target) = std::fs::read_link(&link) else {
        return;
    };
    let old = target.file_name().is_some_and(|n| n == "palette")
        && target.to_string_lossy().contains(".app/Contents/MacOS/");
    if old && std::fs::remove_file(&link).is_ok() {
        let _ = std::os::unix::fs::symlink(&cli, &link);
    }
}

/// `~/.local/bin`: on the default PATH of most shell setups, no admin rights.
pub(crate) fn cli_link() -> Result<std::path::PathBuf, AppError> {
    Ok(paths()?.home().join(".local/bin/palette"))
}

#[derive(Serialize)]
pub(crate) struct CliStatus {
    /// The app carries the CLI.
    pub(crate) bundled: bool,
    /// `~/.local/bin/palette` points at the app's copy.
    pub(crate) linked: bool,
    /// Where a `palette` command is found otherwise (e.g. ~/.cargo/bin).
    pub(crate) other: Option<String>,
}

#[tauri::command]
pub(crate) fn cli_status() -> Result<CliStatus, AppError> {
    let link = cli_link()?;
    let bundled = bundled_cli();
    let linked = bundled
        .as_ref()
        .is_some_and(|b| std::fs::read_link(&link).is_ok_and(|t| &t == b));
    let home = paths()?.home().to_path_buf();
    let mut candidates = vec![home.join(".cargo/bin/palette"), link.clone()];
    candidates.extend(palette::system::find_all_programs("palette"));
    let other = candidates
        .into_iter()
        .find(|p| p.is_file() && !(linked && *p == link))
        .map(|p| p.display().to_string());
    Ok(CliStatus {
        bundled: bundled.is_some(),
        linked,
        other,
    })
}

/// Link `~/.local/bin/palette` to the app's copy of the CLI. Never replaces
/// a `palette` that isn't such a link (a `cargo install`ed one, say).
#[tauri::command]
pub(crate) fn install_cli() -> Result<String, AppError> {
    let cli = bundled_cli().ok_or("this build of the app doesn't include the CLI")?;
    let link = cli_link()?;
    if let Ok(target) = std::fs::read_link(&link) {
        if target == cli {
            return Ok(format!("{} already links to the app", link.display()));
        }
        std::fs::remove_file(&link).map_err(err)?; // an old link (app moved): replace it
    } else if link.exists() {
        return Err(format!(
            "{} already exists and isn't a link; remove it first if you want the app's copy",
            link.display()
        )
        .into());
    }
    if let Some(dir) = link.parent() {
        std::fs::create_dir_all(dir).map_err(err)?;
    }
    std::os::unix::fs::symlink(&cli, &link).map_err(err)?;
    Ok(format!(
        "Installed: {} → the app. Open a new terminal and run `palette`.",
        link.display()
    ))
}
