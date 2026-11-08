//! The wallpaper library and wallpapers from a link.

use super::*;

#[derive(Serialize)]
pub(crate) struct LibraryItem {
    /// Path inside the library (`CozyPixels/Nord/lake.png`): what the other commands take.
    pub(crate) name: String,
    pub(crate) folder: String,
    pub(crate) bytes: u64,
}

#[derive(Serialize)]
pub(crate) struct LibraryInfo {
    pub(crate) dir: String,
    pub(crate) bytes: u64,
    pub(crate) items: Vec<LibraryItem>,
}

/// The wallpaper library (`palette bg library`): folder, total size, pictures.
#[tauri::command]
pub(crate) fn library() -> Result<LibraryInfo, AppError> {
    let paths = paths()?;
    let lib = palette::library::list(&paths).map_err(err)?;
    Ok(LibraryInfo {
        dir: lib.dir.display().to_string(),
        bytes: lib.bytes,
        items: lib
            .items
            .into_iter()
            .map(|i| LibraryItem {
                name: i.name,
                folder: i.folder,
                bytes: i.bytes,
            })
            .collect(),
    })
}

/// Thumbnail of a library picture, looked up by its library name.
#[tauri::command]
pub(crate) async fn library_thumb(name: String) -> Result<String, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let item = palette::library::find(&paths, &name).map_err(err)?;
        let thumb = palette::thumbs::image(&paths, &item.path).map_err(err)?;
        Ok(asset_url(&thumb))
    })
    .await
    .map_err(err)?
}

/// Pick images and copy them into the library (`palette bg library add`).
#[tauri::command]
pub(crate) async fn library_add(app: AppHandle) -> Result<usize, AppError> {
    let Some(files) = app
        .dialog()
        .file()
        .set_title("Add pictures to your wallpaper library")
        .add_filter("Images", &IMAGE_TYPES)
        .blocking_pick_files()
    else {
        return Ok(0);
    };
    let files: Vec<std::path::PathBuf> = files
        .into_iter()
        .filter_map(|f| f.into_path().ok())
        .collect();
    tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        Ok(palette::library::add(&paths, &files, None)
            .map_err(err)?
            .len())
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub(crate) fn library_remove(name: String) -> Result<(), AppError> {
    let paths = paths()?;
    palette::library::remove(&paths, &name).map_err(err)?;
    Ok(())
}

/// Use a library picture now (until the next theme switch).
#[tauri::command]
pub(crate) async fn library_use(name: String) -> Result<Vec<String>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let outcome = palette::library::use_now(&paths, &name).map_err(err)?;
        Ok(applied(String::new(), &[("wallpaper".into(), outcome)]).problems)
    })
    .await
    .map_err(err)?
}

/// Copy (or recolour) a library picture into a theme; returns the new file's name.
#[tauri::command]
pub(crate) async fn library_to_theme(
    name: String,
    theme: String,
    recolour: bool,
) -> Result<String, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let theme = palette::theme::load(&paths, &theme).map_err(err)?;
        let made = palette::library::add_to_theme(&paths, &name, &theme, recolour).map_err(err)?;
        Ok(file_name(&made))
    })
    .await
    .map_err(err)?
}

/// The "new theme from a wallpaper" draft, for a library picture.
#[tauri::command]
pub(crate) async fn draft_theme_from_library(
    name: String,
    light: Option<bool>,
) -> Result<Option<Draft>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let item = palette::library::find(&paths, &name).map_err(err)?;
        draft_for(&item.path, light)
    })
    .await
    .map_err(err)?
}

/// Show the library folder (`what` = "library") or a theme's wallpaper
/// folder ("theme") in Finder. Only these two folders can be opened.
#[tauri::command]
pub(crate) fn reveal_folder(what: String, theme: Option<String>) -> Result<(), AppError> {
    let paths = paths()?;
    let dir = match what.as_str() {
        "library" => palette::library::dir(&paths),
        "theme" => theme_or_current(&paths, theme)?.backgrounds_dir(),
        // The log file itself, selected in Finder (its folder until there is one).
        "log" => {
            let file = palette::log::file(&paths);
            if file.exists() {
                return Ok(palette::system::reveal(&file)?);
            }
            paths.root().to_path_buf()
        }
        _ => return Err(format!("unknown folder `{what}`").into()),
    };
    palette::library::reveal_folder(&dir).map_err(err)
}

#[derive(Serialize, serde::Deserialize)]
pub(crate) struct LinkFile {
    pub(crate) path: String,
    pub(crate) bytes: u64,
}

#[derive(Serialize)]
pub(crate) struct LinkFolder {
    pub(crate) path: String,
    pub(crate) count: usize,
    pub(crate) bytes: u64,
}

#[derive(Serialize)]
pub(crate) struct LinkListing {
    /// "image" (one picture) or "github" (a repo or folder).
    pub(crate) kind: &'static str,
    pub(crate) title: String,
    pub(crate) folders: Vec<LinkFolder>,
    pub(crate) files: Vec<LinkFile>,
    pub(crate) truncated: bool,
    /// Where downloads go unless changed (library subfolder).
    pub(crate) into: String,
    pub(crate) max_files: usize,
    pub(crate) max_bytes: u64,
}

/// What a pasted link offers (`palette bg get <url>` without a choice).
/// A GitHub link costs one request to GitHub's API.
#[tauri::command]
pub(crate) async fn wallpaper_link(url: String) -> Result<LinkListing, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let link = palette::fetch::Link::parse(&url).map_err(err)?;
        let listing = palette::fetch::list(&link).map_err(err)?;
        Ok(LinkListing {
            kind: match link {
                palette::fetch::Link::Image { .. } => "image",
                palette::fetch::Link::GitHub { .. } => "github",
            },
            title: link.describe(),
            folders: palette::fetch::folders(&listing.files)
                .into_iter()
                .map(|f| LinkFolder {
                    path: f.path,
                    count: f.count,
                    bytes: f.bytes,
                })
                .collect(),
            files: listing
                .files
                .into_iter()
                .map(|f| LinkFile {
                    path: f.path,
                    bytes: f.bytes,
                })
                .collect(),
            truncated: listing.truncated,
            into: link.default_folder(),
            max_files: palette::fetch::MAX_FILES,
            max_bytes: palette::fetch::MAX_TOTAL,
        })
    })
    .await
    .map_err(err)?
}

#[derive(Serialize)]
pub(crate) struct LinkDownload {
    pub(crate) saved: usize,
    pub(crate) bytes: u64,
    pub(crate) dir: String,
    pub(crate) skipped: Vec<(String, String)>,
    /// How many were also added to `theme`.
    pub(crate) added: usize,
}

/// Download the chosen pictures of a link into the library
/// (`palette bg get … --pick/--folder/--all`), optionally also into a theme.
#[tauri::command]
pub(crate) async fn wallpaper_link_download(
    app: AppHandle,
    url: String,
    files: Vec<LinkFile>,
    theme: Option<String>,
    recolour: Option<bool>,
) -> Result<LinkDownload, AppError> {
    let done = tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let link = palette::fetch::Link::parse(&url).map_err(err)?;
        let theme = match theme.filter(|t| !t.is_empty()) {
            Some(t) => Some(palette::theme::load(&paths, &t).map_err(err)?),
            None => None,
        };
        let files: Vec<palette::fetch::RemoteFile> = files
            .into_iter()
            .map(|f| palette::fetch::RemoteFile {
                path: f.path,
                bytes: f.bytes,
            })
            .collect();
        let done = palette::fetch::download(&paths, &link, &files, None).map_err(err)?;
        let mut added = 0;
        let mut skipped = done.skipped;
        if let Some(theme) = &theme {
            for file in &done.saved {
                match palette::library::copy_to_theme(file, theme, recolour.unwrap_or(false)) {
                    Ok(_) => added += 1,
                    Err(e) => skipped.push((file_name(file), err(e).message)),
                }
            }
        }
        Ok::<_, AppError>(LinkDownload {
            saved: done.saved.len(),
            bytes: done.bytes,
            dir: done.dir.display().to_string(),
            skipped,
            added,
        })
    })
    .await
    .map_err(err)??;
    if done.added > 0 {
        let _ = app.emit("themes-changed", ());
    }
    Ok(done)
}
