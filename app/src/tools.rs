//! Settings → Get the tools: SketchyBar, JankyBorders, AeroSpace, gowall.

use super::*;

#[derive(Serialize)]
pub(crate) struct ToolRow {
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) group: &'static str,
    pub(crate) what: &'static str,
    pub(crate) installed: bool,
    pub(crate) install: Vec<String>,
    pub(crate) remove: Vec<String>,
    /// none | available | has_config | added | added_edited
    pub(crate) starter: &'static str,
    pub(crate) config: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ToolsStatus {
    pub(crate) homebrew: bool,
    pub(crate) tools: Vec<ToolRow>,
}

/// `palette tools`: what is installed and the Homebrew commands.
#[tauri::command]
pub(crate) async fn tools_status() -> Result<ToolsStatus, AppError> {
    tauri::async_runtime::spawn_blocking(|| {
        use palette::tools::Starter;
        let status = palette::tools::status(&paths()?);
        Ok(ToolsStatus {
            homebrew: status.homebrew,
            tools: status
                .tools
                .into_iter()
                .map(|t| ToolRow {
                    id: t.id,
                    name: t.name,
                    group: t.group,
                    what: t.what,
                    installed: t.installed,
                    install: t.install,
                    remove: t.remove,
                    starter: match t.starter {
                        Starter::None => "none",
                        Starter::Available => "available",
                        Starter::HasConfig => "has_config",
                        Starter::Added { unchanged: true } => "added",
                        Starter::Added { unchanged: false } => "added_edited",
                    },
                    config: t.config.map(|p| p.display().to_string()),
                })
                .collect(),
        })
    })
    .await
    .map_err(err)?
}

/// `palette tools starter <tool>`. Returns a sentence for the toast.
#[tauri::command]
pub(crate) async fn add_starter(tool: String) -> Result<String, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let added = palette::tools::add_starter(&paths()?, &tool).map_err(err)?;
        let problems = applied(String::new(), &added.effects).problems;
        let mut text = format!("Added a starter config ({} file(s))", added.files.len());
        if !added.hint.is_empty() {
            text = format!("{text}. {}", added.hint);
        }
        if !problems.is_empty() {
            text = format!("{text}. {}", problems.join(" · "));
        }
        Ok(text)
    })
    .await
    .map_err(err)?
}

/// `palette tools starter <tool> --remove`: to the Trash, only if unchanged.
#[tauri::command]
pub(crate) async fn remove_starter(tool: String) -> Result<String, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        palette::tools::remove_starter(&paths()?, &tool).map_err(err)?;
        Ok("Moved the starter config to the Trash".to_string())
    })
    .await
    .map_err(err)?
}

/// Copy a command to the clipboard (the Copy buttons).
#[tauri::command]
pub(crate) fn copy_text(text: String) -> Result<(), AppError> {
    Ok(palette::system::copy_to_clipboard(&text)?)
}

/// "Open Terminal": the person pastes the Homebrew command there.
#[tauri::command]
pub(crate) fn open_terminal() -> Result<(), AppError> {
    palette::tools::open_terminal().map_err(err)
}

/// Opens brew.sh (the only address this opens).
#[tauri::command]
pub(crate) fn open_homebrew_site() -> Result<(), AppError> {
    palette::tools::open_homebrew_site().map_err(err)
}
