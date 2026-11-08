//! Settings → Setup: wiring every app (`init`) and the doctor's checks.

use super::*;

#[derive(Serialize)]
pub(crate) struct SetupLine {
    pub(crate) app: &'static str,
    pub(crate) what: String,
    pub(crate) path: String,
}

#[derive(Serialize)]
pub(crate) struct SetupResult {
    pub(crate) lines: Vec<SetupLine>,
    pub(crate) backup: Option<String>,
    pub(crate) problems: Vec<String>,
}

/// "Set up all apps" (`palette init`) or "Remove from all configs"
/// (`palette init --remove`).
#[tauri::command]
pub(crate) async fn setup_all(remove: bool) -> Result<SetupResult, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let (report, effects) = palette::init(&paths, remove, false).map_err(err)?;
        let lines = report
            .items
            .iter()
            .filter(|i| i.step.writes() || matches!(i.step, palette::wiring::Step::Manual(_)))
            .map(|i| SetupLine {
                app: i.target,
                what: match &i.step {
                    palette::wiring::Step::Manual(why) => format!("needs you: {why}"),
                    palette::wiring::Step::Add | palette::wiring::Step::Set => "set up".into(),
                    _ => "restored".into(),
                },
                path: i
                    .path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
            })
            .collect();
        Ok(SetupResult {
            lines,
            backup: report.backup.map(|b| b.display().to_string()),
            problems: applied(String::new(), &effects).problems,
        })
    })
    .await
    .map_err(err)?
}

#[derive(Serialize)]
pub(crate) struct CheckRow {
    pub(crate) name: String,
    pub(crate) status: &'static str,
    pub(crate) detail: String,
    pub(crate) notes: Vec<(&'static str, String)>,
}

/// `palette doctor`, as rows for the Setup tab.
#[tauri::command]
pub(crate) async fn check() -> Result<Vec<CheckRow>, AppError> {
    tauri::async_runtime::spawn_blocking(|| {
        let paths = paths()?;
        Ok(palette::doctor::run(&paths)
            .map_err(err)?
            .into_iter()
            .map(|r| CheckRow {
                status: match (r.status, r.needs_fix()) {
                    (palette::doctor::Status::Off, _) => "off",
                    (_, true) => "fix",
                    _ => "ok",
                },
                name: r.name,
                detail: r.detail,
                notes: r
                    .notes
                    .into_iter()
                    .map(|n| {
                        let kind = match (n.ok, n.info) {
                            (false, _) => "fix",
                            (true, true) => "info",
                            _ => "ok",
                        };
                        (kind, n.text)
                    })
                    .collect(),
            })
            .collect())
    })
    .await
    .map_err(err)?
}

/// `palette refresh`: render the active theme again.
#[tauri::command]
pub(crate) async fn refresh(app: AppHandle) -> Result<Applied, AppError> {
    let result = tauri::async_runtime::spawn_blocking(|| {
        let paths = paths()?;
        let s = palette::refresh(&paths).map_err(err)?;
        Ok::<_, AppError>(applied(s.theme, &s.effects))
    })
    .await
    .map_err(err)??;
    let _ = app.emit("theme-changed", result.clone());
    Ok(result)
}
