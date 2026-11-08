//! Settings → Get themes: the community catalogue, install, update, remove.

use super::*;

#[derive(Serialize)]
pub(crate) struct CatalogItem {
    pub(crate) name: String,
    pub(crate) title: String,
    pub(crate) author: String,
    pub(crate) url: String,
    pub(crate) mode: &'static str,
    pub(crate) mood: &'static str,
    pub(crate) colors: BTreeMap<String, String>,
    pub(crate) installed: bool,
    /// Low-contrast pairs (`palette check`), for a badge.
    pub(crate) contrast: Vec<String>,
    /// Words for search by look (12c).
    pub(crate) tags: Vec<&'static str>,
}

/// The curated community themes (`palette catalog`), offline.
#[tauri::command]
pub(crate) fn catalog() -> Result<Vec<CatalogItem>, AppError> {
    let paths = paths()?;
    let installed: Vec<String> = palette::install::installed(&paths)
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    Ok(palette::catalog::entries()
        .iter()
        .map(|e| CatalogItem {
            name: e.name.clone(),
            title: e.title.clone(),
            author: e.author.clone(),
            url: e.url.clone(),
            mode: e.colors.mode.as_str(),
            mood: palette::colour::analyse(&e.theme(&paths)).mood.as_str(),
            colors: e
                .colors
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            installed: installed.contains(&e.name),
            contrast: contrast_lines(&palette::check::colors(&e.name, &e.colors)),
            tags: palette::search::tags(&e.colors),
        })
        .collect())
}

/// Themes installed with `palette install` that aren't in the catalogue (a link).
#[tauri::command]
pub(crate) fn installed_themes() -> Result<Vec<(String, String)>, AppError> {
    let paths = paths()?;
    Ok(palette::install::installed(&paths))
}

#[derive(Serialize)]
pub(crate) struct InstallResult {
    pub(crate) name: String,
    pub(crate) wallpapers: usize,
    pub(crate) filled: Vec<String>,
}

/// `palette install` (a catalogue name, a GitHub link) or, with `update`,
/// `palette update`. Downloads, so it runs off the main thread.
#[tauri::command]
pub(crate) async fn install_theme(
    app: AppHandle,
    source: String,
    name: Option<String>,
    update: Option<bool>,
) -> Result<InstallResult, AppError> {
    let done = tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        if update.unwrap_or(false) {
            palette::install::update(&paths, &source)
        } else {
            palette::install::install(&paths, &source, name.as_deref())
        }
        .map_err(err)
    })
    .await
    .map_err(err)??;
    let _ = app.emit("themes-changed", ());
    Ok(InstallResult {
        name: done.name,
        wallpapers: done.wallpapers,
        filled: done.filled,
    })
}

/// `palette remove`.
#[tauri::command]
pub(crate) fn remove_theme(app: AppHandle, name: String) -> Result<Removed, AppError> {
    let paths = paths()?;
    let done = palette::install::remove(&paths, &name).map_err(err)?;
    let _ = app.emit("themes-changed", ());
    Ok(Removed {
        message: done.message(&name),
        warning: done.is_warning(),
    })
}

/// What removing a theme did: a made-by-you theme goes to the Trash, with a warning.
#[derive(Serialize)]
pub(crate) struct Removed {
    pub(crate) message: String,
    pub(crate) warning: bool,
}

/// Get themes → "Inspection sheet" (12b): every catalogue theme as made and with Palette's
/// fixes, on one offline page opened in the browser (`palette catalog --sheet`).
#[tauri::command]
pub(crate) async fn open_sheet() -> Result<String, AppError> {
    tauri::async_runtime::spawn_blocking(|| {
        palette::sheet::open(&paths()?)
            .map(|f| f.display().to_string())
            .map_err(err)
    })
    .await
    .map_err(err)?
}
