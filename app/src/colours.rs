//! Favourites, the contrast report and Settings → Colours.

use super::*;

/// ★ a theme or not (`palette fav add/remove`). Returns the favourites now.
#[tauri::command]
pub(crate) fn set_favourite(name: String, on: bool) -> Result<Vec<String>, AppError> {
    palette::favourites::set(&paths()?, &name, on).map_err(err)
}

/// The last themes switched to, newest first (`palette recent`).
#[tauri::command]
pub(crate) fn recents() -> Result<Vec<String>, AppError> {
    Ok(palette::favourites::recents(&paths()?))
}

/// "all" or "favourites": what next/prev and the bar pill step through.
#[tauri::command]
pub(crate) fn set_next_cycles(cycle: String) -> Result<(), AppError> {
    let cycle = palette::favourites::Cycle::parse(&cycle).map_err(err)?;
    palette::favourites::set_cycle(&paths()?, cycle).map_err(err)
}

/// Settings → My themes: scrolling on the bar's theme pill switches theme. Re-renders.
#[tauri::command]
pub(crate) async fn set_bar_scroll(on: bool) -> Result<bool, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        palette::favourites::set_bar_scroll(&paths()?, Some(on)).map_err(err)
    })
    .await
    .map_err(err)?
}

/// `palette check <theme>`: the verdict and one line per low pair.
#[tauri::command]
pub(crate) fn contrast_report(name: String) -> Result<(String, Vec<String>), AppError> {
    let report = palette::check::run(&paths()?, Some(&name)).map_err(err)?;
    Ok((report.verdict(), contrast_lines(&report)))
}

#[derive(Serialize)]
pub(crate) struct SwatchOut {
    pub(crate) key: &'static str,
    pub(crate) value: String,
    pub(crate) original: String,
}

#[derive(Serialize)]
pub(crate) struct IssueOut {
    pub(crate) fg: &'static str,
    pub(crate) bg: &'static str,
    pub(crate) fg_hex: String,
    pub(crate) bg_hex: String,
    pub(crate) ratio: f32,
    pub(crate) min: f32,
    pub(crate) used_for: &'static str,
    pub(crate) applied: Option<&'static str>,
    pub(crate) suggested: String,
}

#[derive(Serialize)]
pub(crate) struct SheetOut {
    pub(crate) theme: String,
    pub(crate) mode: &'static str,
    pub(crate) colours: Vec<SwatchOut>,
    pub(crate) verdict: String,
    pub(crate) issues: Vec<IssueOut>,
    /// Palette's readability fixes: the level, whether this theme is left original, and
    /// what they change when it's used.
    pub(crate) level: &'static str,
    pub(crate) original: bool,
    pub(crate) fixed: Vec<FixOut>,
    /// Per kind of app (11i): ok / weak / bad and why.
    pub(crate) apps: Vec<palette::check::AppVerdict>,
    /// Glass lifts per see-through app (11g), as report lines.
    pub(crate) glass: Vec<(String, Vec<String>)>,
}

#[derive(Serialize)]
pub(crate) struct FixOut {
    pub(crate) key: &'static str,
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) why: String,
}

/// Settings → Colours: how hard the readability fixes push (`level`: off, minimal,
/// strong), and whether `name` is used exactly as made (`original`). `None` leaves each.
/// Re-renders the active theme when it's affected.
#[tauri::command]
pub(crate) async fn set_readability(
    app: AppHandle,
    name: String,
    level: Option<String>,
    original: Option<bool>,
) -> Result<(), AppError> {
    let level = level
        .as_deref()
        .map(palette::normalise::Level::parse)
        .transpose()
        .map_err(err)?;
    tauri::async_runtime::spawn_blocking(move || {
        palette::readability(&paths()?, level, original.map(|o| (name.as_str(), o))).map_err(err)
    })
    .await
    .map_err(err)??;
    tray::refresh(&app);
    Ok(())
}

/// Settings → General: monochrome themes' spaced greys and tint (`None` leaves each).
/// Re-renders the active theme. Returns both as now set.
#[tauri::command]
pub(crate) async fn set_monochrome(
    app: AppHandle,
    greys: Option<bool>,
    tint: Option<bool>,
) -> Result<MonoPref, AppError> {
    let m = tauri::async_runtime::spawn_blocking(move || {
        palette::set_monochrome(&paths()?, greys, tint).map_err(err)
    })
    .await
    .map_err(err)??;
    tray::refresh(&app);
    Ok(MonoPref::from(m))
}

/// The monochrome switches, for the page.
#[derive(Serialize, Clone, Copy)]
pub(crate) struct MonoPref {
    pub(crate) greys: bool,
    pub(crate) tint: bool,
}

impl From<palette::monochrome::Mono> for MonoPref {
    fn from(m: palette::monochrome::Mono) -> Self {
        MonoPref {
            greys: m.greys,
            tint: m.tint,
        }
    }
}

/// `palette colours <theme>`: every colour (yours and the original) and the
/// contrast report, for Settings → Colours.
#[tauri::command]
pub(crate) fn colour_sheet(name: String) -> Result<SheetOut, AppError> {
    let paths = paths()?;
    let sheet = palette::edits::sheet(&paths, &name).map_err(err)?;
    let settings = palette::settings::load(&paths).map_err(err)?;
    let report = palette::check::run(&paths, Some(&name)).map_err(err)?;
    let apps = palette::check::apps(&paths, Some(&name)).map_err(err)?;
    let glass = report.glass;
    let fixed = report
        .fixed
        .into_iter()
        .map(|c| FixOut {
            key: c.key,
            from: c.from,
            to: c.to,
            why: c.why,
        })
        .collect();
    Ok(SheetOut {
        level: settings.readability.as_str(),
        original: settings.original_colours.contains(&name),
        fixed,
        apps,
        glass,
        verdict: sheet.report.verdict(),
        theme: sheet.theme,
        mode: sheet.mode.as_str(),
        colours: sheet
            .colours
            .into_iter()
            .map(|s| SwatchOut {
                key: s.key,
                value: s.value,
                original: s.original,
            })
            .collect(),
        issues: sheet
            .report
            .issues
            .into_iter()
            .map(|i| IssueOut {
                fg: i.fg,
                bg: i.bg,
                fg_hex: i.fg_hex,
                bg_hex: i.bg_hex,
                ratio: i.ratio,
                min: i.min,
                used_for: i.used_for,
                applied: i.applied,
                suggested: i.suggested,
            })
            .collect(),
    })
}

/// Change colours: `how` is "set" (with `colours`), "reset" (with an
/// optional `key`) or "fix" (every suggestion). Re-applies the active theme.
/// Returns the keys that changed, and any app that failed to reload.
#[tauri::command]
pub(crate) async fn edit_colours(
    app: AppHandle,
    name: String,
    how: String,
    colours: Option<Vec<(String, String)>>,
    key: Option<String>,
) -> Result<(Vec<(String, String)>, Vec<String>), AppError> {
    use palette::edits::Edit;
    let edit = match how.as_str() {
        "set" => Edit::Set(colours.unwrap_or_default()),
        "reset" => Edit::Reset(key),
        "fix" => Edit::FixContrast,
        other => return Err(format!("unknown colour edit `{other}`").into()),
    };
    let (changed, switched) = tauri::async_runtime::spawn_blocking(move || {
        palette::edit_colours(&paths()?, &name, &edit).map_err(err)
    })
    .await
    .map_err(err)??;
    let mut problems = Vec::new();
    if let Some(s) = switched {
        let result = applied(s.theme, &s.effects);
        problems = result.problems.clone();
        let _ = app.emit("theme-changed", result);
    }
    Ok((changed, problems))
}
