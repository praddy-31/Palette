//! Settings → Move to another Mac: export and import.

use super::*;

#[derive(Serialize)]
pub(crate) struct ImportPlan {
    pub(crate) dir: String,
    pub(crate) files: usize,
    pub(crate) replaced: Vec<String>,
    pub(crate) theme: Option<String>,
    pub(crate) apps: Vec<&'static str>,
    /// Apps the configs need that aren't on this Mac (25a), and the brew commands as one block.
    pub(crate) missing: Vec<palette::missing::Missing>,
    pub(crate) script: Vec<String>,
}

/// Pick an export folder and describe what importing it would change
/// (a dry run). Nothing is written until `import_apply`.
#[tauri::command]
pub(crate) async fn import_dialog(app: AppHandle) -> Result<Option<ImportPlan>, AppError> {
    let Some(picked) = app
        .dialog()
        .file()
        .set_title("Choose a Palette Export folder")
        .blocking_pick_folder()
    else {
        return Ok(None);
    };
    let dir = picked.into_path().map_err(err)?;
    tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let (report, _, _) = palette::import(&paths, &dir, true).map_err(err)?;
        let missing = palette::import_needs(&paths, &report);
        Ok(Some(ImportPlan {
            dir: dir.display().to_string(),
            files: report.written.len(),
            replaced: report
                .replaced
                .iter()
                .filter(|p| !p.starts_with(".config/palette"))
                .map(|p| format!("~/{}", p.display()))
                .collect(),
            theme: report.theme.clone(),
            apps: palette::sync::imported_apps(&report),
            script: palette::missing::script(&missing),
            missing,
        }))
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub(crate) async fn import_apply(app: AppHandle, dir: String) -> Result<String, AppError> {
    let msg = tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let (report, _, effects) =
            palette::import(&paths, std::path::Path::new(&dir), false).map_err(err)?;
        let problems = applied(String::new(), &effects).problems;
        let mut msg = format!("Imported {} files", report.written.len());
        if let Some(b) = &report.backup {
            msg.push_str(&format!("; replaced files backed up to {}", b.display()));
        }
        if !problems.is_empty() {
            msg.push_str(&format!(". Problems: {}", problems.join(" · ")));
        }
        Ok::<_, AppError>(msg)
    })
    .await
    .map_err(err)??;
    let _ = app.emit("theme-changed", ());
    Ok(msg)
}

/// Open Settings on one tab (picker's Wallpapers button, say).
#[tauri::command]
pub(crate) fn open_settings_tab(app: AppHandle, tab: String) {
    if let Ok(mut pending) = app.state::<PendingTab>().0.lock() {
        *pending = Some(tab.clone());
    }
    show_settings(&app);
    let _ = app.emit("open-tab", tab);
}

#[derive(Serialize)]
pub(crate) struct SyncPrefs {
    pub(crate) wallpapers: bool,
    pub(crate) library: bool,
    pub(crate) configs: Vec<String>,
}

/// What an export takes by default (sync.toml, created on first use).
#[tauri::command]
pub(crate) fn sync_prefs() -> Result<SyncPrefs, AppError> {
    let (sync, _) = palette::sync::load_or_create(&paths()?).map_err(err)?;
    Ok(SyncPrefs {
        wallpapers: sync.wallpapers,
        library: sync.library,
        configs: sync.configs,
    })
}

/// Ask for a folder, export into `<folder>/Palette Export`, show it in
/// Finder. `None` when the user cancels the folder dialog.
#[tauri::command]
pub(crate) async fn export_dialog(
    app: AppHandle,
    wallpapers: bool,
    library: bool,
    configs: bool,
) -> Result<Option<String>, AppError> {
    let Some(picked) = app
        .dialog()
        .file()
        .set_title("Export Palette to…")
        .blocking_pick_folder()
    else {
        return Ok(None);
    };
    let picked = picked.into_path().map_err(err)?;
    tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        // Inside the chosen folder: reuse an earlier export, else a new name.
        let mut dir = picked.join("Palette Export");
        let mut n = 2;
        while dir.exists() && !dir.join("palette-export.toml").is_file() {
            dir = picked.join(format!("Palette Export {n}"));
            n += 1;
        }
        let options = palette::sync::ExportOptions {
            wallpapers: Some(wallpapers),
            library: Some(library),
            configs: Some(configs),
        };
        let report = palette::sync::export(&paths, &dir, &options).map_err(err)?;
        let _ = palette::system::run("open", &["-R", &dir.to_string_lossy()]);
        let mb = report.palette.bytes as f64 / 1_000_000.0
            + report.configs.iter().map(|(_, c)| c.bytes).sum::<u64>() as f64 / 1_000_000.0;
        Ok(Some(format!(
            "Exported {} configs and Palette ({mb:.1} MB) to {}",
            report.configs.len(),
            dir.display()
        )))
    })
    .await
    .map_err(err)?
}

/// 25b: apps an import was waiting for that are installed now.
#[tauri::command]
pub(crate) fn arrived() -> Result<Vec<palette::missing::Arrived>, AppError> {
    Ok(palette::arrived(&paths()?))
}

/// 25b: "Use it" (Palette's line in the imported config, then reload) or "Not now".
#[tauri::command]
pub(crate) async fn answer_arrived(
    app: AppHandle,
    id: String,
    use_it: bool,
) -> Result<Vec<String>, AppError> {
    let problems = tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        if use_it {
            let effects = palette::use_arrived(&paths, &id).map_err(err)?;
            Ok::<_, AppError>(applied(id, &effects).problems)
        } else {
            palette::later_arrived(&paths, &id).map_err(err)?;
            Ok(Vec::new())
        }
    })
    .await
    .map_err(err)??;
    let _ = app.emit("theme-changed", ());
    Ok(problems)
}
