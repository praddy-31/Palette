//! Wallpapers of a theme and the wallpaper studio (recolour, generate, theme from a picture).

use super::*;

#[derive(Serialize)]
pub(crate) struct Wallpaper {
    pub(crate) name: String,
    pub(crate) current: bool,
    /// The one this theme starts with.
    pub(crate) default: bool,
}

#[derive(Serialize)]
pub(crate) struct Wallpapers {
    pub(crate) theme: String,
    pub(crate) active: bool,
    pub(crate) images: Vec<Wallpaper>,
    /// The theme's wallpaper folder and its size, shown so copies don't pile up unseen.
    pub(crate) dir: String,
    pub(crate) bytes: u64,
}

pub(crate) fn theme_or_current(paths: &Paths, theme: Option<String>) -> Result<Theme, AppError> {
    match theme {
        Some(name) => palette::theme::load(paths, &name).map_err(err),
        None => palette::current(paths).map_err(err),
    }
}

/// A theme's wallpapers (default: the active theme), current one marked.
#[tauri::command]
pub(crate) fn wallpapers(theme: Option<String>) -> Result<Wallpapers, AppError> {
    let paths = paths()?;
    let theme = theme_or_current(&paths, theme)?;
    let current = palette::effects::current_wallpaper(&paths);
    let default = palette::effects::default_wallpaper(&paths, &theme);
    let active = palette::state::current_theme(&paths)
        .map_err(err)?
        .as_deref()
        == Some(&theme.name);
    let images = theme
        .backgrounds()
        .map_err(err)?
        .into_iter()
        .map(|p| Wallpaper {
            current: current.as_ref() == Some(&p),
            default: default.as_ref() == Some(&p),
            name: p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
        })
        .collect();
    let dir = theme.backgrounds_dir();
    Ok(Wallpapers {
        bytes: palette::library::folder_size(&dir),
        dir: dir.display().to_string(),
        theme: theme.name,
        active,
        images,
    })
}

/// Thumbnail of one wallpaper, looked up by name inside the theme's own
/// folder (the web view can't ask for arbitrary files).
#[tauri::command]
pub(crate) async fn wallpaper_thumb(theme: String, name: String) -> Result<String, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let theme = palette::theme::load(&paths, &theme).map_err(err)?;
        let file = palette::effects::wallpaper_named(&theme, &name).map_err(err)?;
        let thumb = palette::thumbs::image(&paths, &file).map_err(err)?;
        Ok(asset_url(&thumb))
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub(crate) async fn set_wallpaper(theme: String, name: String) -> Result<Vec<String>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let theme = palette::theme::load(&paths, &theme).map_err(err)?;
        let file = palette::effects::wallpaper_named(&theme, &name).map_err(err)?;
        let outcome = palette::effects::set_wallpaper(&paths, &file).map_err(err)?;
        Ok(applied(theme.name, &[("wallpaper".into(), outcome)]).problems)
    })
    .await
    .map_err(err)?
}

/// The active theme's next (`by` = 1) or previous (-1) wallpaper (`palette bg next/prev`).
#[tauri::command]
pub(crate) async fn step_wallpaper(by: Option<isize>) -> Result<Vec<String>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let theme = palette::current(&paths).map_err(err)?;
        let outcome =
            palette::effects::step_wallpaper(&paths, &theme, by.unwrap_or(1)).map_err(err)?;
        Ok(applied(theme.name, &[("wallpaper".into(), outcome)]).problems)
    })
    .await
    .map_err(err)?
}

/// The wallpaper a theme starts with (`palette bg default`); `None` clears it.
#[tauri::command]
pub(crate) fn set_default_wallpaper(theme: String, name: Option<String>) -> Result<(), AppError> {
    let paths = paths()?;
    let theme = palette::theme::load(&paths, &theme).map_err(err)?;
    palette::effects::set_default_wallpaper(&paths, &theme, name.as_deref()).map_err(err)
}

#[tauri::command]
pub(crate) fn remove_wallpaper(theme: String, name: String) -> Result<(), AppError> {
    let paths = paths()?;
    let theme = palette::theme::load(&paths, &theme).map_err(err)?;
    palette::effects::remove_wallpaper(&theme, &name).map_err(err)?;
    Ok(())
}

/// Pick images in a file dialog and copy them into a theme. The theme's
/// first wallpapers are used straight away if it's the active theme.
#[tauri::command]
pub(crate) async fn add_wallpapers(app: AppHandle, theme: String) -> Result<usize, AppError> {
    let Some(files) = app
        .dialog()
        .file()
        .set_title(format!("Add wallpapers to {theme}"))
        .add_filter("Images", &["png", "jpg", "jpeg", "heic", "webp", "tiff"])
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
        let theme = palette::theme::load(&paths, &theme).map_err(err)?;
        let had_none = theme.backgrounds().map_err(err)?.is_empty();
        let added = palette::effects::add_wallpapers(&theme, &files).map_err(err)?;
        let active = palette::state::current_theme(&paths)
            .map_err(err)?
            .as_deref()
            == Some(&theme.name);
        if had_none
            && active
            && let Some(first) = added.first()
        {
            let _ = palette::effects::set_wallpaper(&paths, first);
        }
        Ok(added.len())
    })
    .await
    .map_err(err)?
}

pub(crate) const IMAGE_TYPES: [&str; 6] = ["png", "jpg", "jpeg", "heic", "webp", "tiff"];

/// Is gowall installed (offered as a second recolour engine)?
#[tauri::command]
pub(crate) fn has_gowall() -> bool {
    palette::studio::has_gowall()
}

/// Pick images and save a copy of each recoloured to `theme`
/// (`palette bg recolor`). Returns the new wallpapers' names; empty when the
/// dialog is cancelled. `engine`: "palette" (default) or "gowall";
/// `strength`: "subtle", "balanced" (default) or "strong" (Palette's engine).
#[tauri::command]
pub(crate) async fn recolour_wallpapers(
    app: AppHandle,
    theme: String,
    engine: Option<String>,
    strength: Option<String>,
) -> Result<Vec<String>, AppError> {
    let engine =
        palette::studio::Engine::parse(engine.as_deref().unwrap_or("palette")).map_err(err)?;
    let strength = match strength.as_deref() {
        Some(s) => palette::studio::Strength::parse(s).map_err(err)?,
        None => palette::studio::Strength::default(),
    };
    let Some(files) = app
        .dialog()
        .file()
        .set_title(format!("Recolour images to {theme}"))
        .add_filter("Images", &IMAGE_TYPES)
        .blocking_pick_files()
    else {
        return Ok(Vec::new());
    };
    let files: Vec<std::path::PathBuf> = files
        .into_iter()
        .filter_map(|f| f.into_path().ok())
        .collect();
    tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let theme = palette::theme::load(&paths, &theme).map_err(err)?;
        files
            .iter()
            .map(|f| {
                let made = palette::studio::recolour(&theme, f, engine, strength).map_err(err)?;
                Ok(file_name(&made))
            })
            .collect()
    })
    .await
    .map_err(err)?
}

/// Paint a wallpaper from `theme`'s colours (`palette bg generate`);
/// returns its name. `style`: one of `palette::studio::STYLES`; `seed`: a
/// variant ("Another" in Settings), none for the default picture.
#[tauri::command]
pub(crate) async fn generate_wallpaper(
    theme: String,
    style: String,
    seed: Option<u64>,
) -> Result<String, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let theme = palette::theme::load(&paths, &theme).map_err(err)?;
        let made = palette::studio::generate(&theme, &style, seed).map_err(err)?;
        Ok(file_name(&made))
    })
    .await
    .map_err(err)?
}

pub(crate) fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

#[derive(Serialize)]
pub(crate) struct Draft {
    /// The picked image (passed back to re-draft or save).
    pub(crate) path: String,
    /// A free theme name from the file's name, for the name field.
    pub(crate) name: String,
    pub(crate) light: bool,
    pub(crate) mood: &'static str,
    /// The picture's main colours.
    pub(crate) swatches: Vec<String>,
    /// The theme's main colours (background, foreground, accent, terminal colours).
    pub(crate) colors: BTreeMap<String, String>,
    /// A small JPEG of the picture, as a data URL.
    pub(crate) thumb: Option<String>,
}

/// A theme name from a file name: lowercase letters, digits and `-`, not yet taken.
pub(crate) fn suggest_name(paths: &Paths, image: &std::path::Path) -> String {
    let stem = image
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let mut base = String::new();
    for c in stem.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            base.push(c);
        } else if !base.ends_with('-') && !base.is_empty() {
            base.push('-');
        }
    }
    let mut base = base.trim_end_matches('-').to_string();
    if !palette::theme::is_valid_name(&base) {
        base = "my-theme".into();
    }
    let free =
        |n: &str| palette::theme::load(paths, n).is_err() && !paths.themes().join(n).exists();
    let mut name = base.clone();
    let mut n = 2;
    while !free(&name) {
        name = format!("{base}-{n}");
        n += 1;
    }
    name
}

/// Work out a theme from a picture without saving it (`palette new
/// --from-wallpaper --dry-run`). With no `path`, asks for a picture first;
/// `None` when that dialog is cancelled. `light`: force light or dark.
#[tauri::command]
pub(crate) async fn draft_theme_from_wallpaper(
    app: AppHandle,
    path: Option<String>,
    light: Option<bool>,
) -> Result<Option<Draft>, AppError> {
    let image = match path {
        Some(p) => std::path::PathBuf::from(p),
        None => {
            let Some(picked) = app
                .dialog()
                .file()
                .set_title("Make a theme from a wallpaper")
                .add_filter("Images", &IMAGE_TYPES)
                .blocking_pick_file()
            else {
                return Ok(None);
            };
            picked.into_path().map_err(err)?
        }
    };
    tauri::async_runtime::spawn_blocking(move || draft_for(&image, light))
        .await
        .map_err(err)?
}

/// The draft shown for a picture (shared by the file dialog and the library).
pub(crate) fn draft_for(
    image: &std::path::Path,
    light: Option<bool>,
) -> Result<Option<Draft>, AppError> {
    let paths = paths()?;
    let image = image.to_path_buf();
    let name = suggest_name(&paths, &image);
    let d = palette::studio::new_from_wallpaper(&paths, &name, &image, light, true).map_err(err)?;
    let parsed = palette::theme::Colors::parse(&d.colors_toml).map_err(|p| p.join("; "))?;
    let colors = [
        "background",
        "foreground",
        "accent",
        "red",
        "yellow",
        "green",
        "cyan",
        "blue",
        "magenta",
    ]
    .into_iter()
    .map(|k| (k.to_string(), parsed.get(k).to_string()))
    .collect();
    let thumb = palette::thumbs::image(&paths, &image)
        .ok()
        .map(|t| asset_url(&t));
    Ok(Some(Draft {
        path: image.display().to_string(),
        name,
        light: d.light,
        mood: d.mood.as_str(),
        swatches: d.swatches,
        colors,
        thumb,
    }))
}

/// Save the theme drafted from `path` as `name` (`palette new
/// --from-wallpaper`), with the picture as its wallpaper.
#[tauri::command]
pub(crate) async fn save_theme_from_wallpaper(
    app: AppHandle,
    name: String,
    path: String,
    light: Option<bool>,
) -> Result<String, AppError> {
    let saved = tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        palette::studio::new_from_wallpaper(
            &paths,
            &name,
            std::path::Path::new(&path),
            light,
            false,
        )
        .map_err(err)?;
        Ok::<_, AppError>(name)
    })
    .await
    .map_err(err)??;
    let _ = app.emit("themes-changed", ());
    Ok(saved)
}
