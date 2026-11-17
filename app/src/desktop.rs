//! Settings → Bar, Windows, Borders, Prompt, Dock & menu bar (`ui/desktop.js`).

use super::*;

#[derive(Serialize)]
pub(crate) struct DesktopStatus {
    pub(crate) tool: &'static str,
    pub(crate) app: &'static str,
    pub(crate) installed: bool,
    pub(crate) managed: bool,
    pub(crate) files: Vec<String>,
    pub(crate) original: Option<String>,
    pub(crate) backups: usize,
    /// When each backup was made (folder names), newest first.
    pub(crate) history: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct DesktopApplied {
    pub(crate) written: Vec<String>,
    pub(crate) backup: Option<String>,
    /// "reloaded", "not running", or why it failed.
    pub(crate) reload: String,
    pub(crate) failed: bool,
    pub(crate) warnings: Vec<String>,
}

pub(crate) fn tool(name: &str) -> Result<palette::desktop::Tool, AppError> {
    palette::desktop::Tool::parse(name).map_err(err)
}

pub(crate) fn desktop_applied(a: palette::desktop::Applied) -> DesktopApplied {
    let (reload, failed) = match a.reload {
        Outcome::Done(m) | Outcome::Unchanged(m) | Outcome::Skipped(m) => (m, false),
        Outcome::Failed(m) => (m, true),
    };
    DesktopApplied {
        written: a.written.iter().map(|p| p.display().to_string()).collect(),
        backup: a.backup.map(|p| p.display().to_string()),
        reload,
        failed,
        warnings: a.warnings,
    }
}

/// `palette desktop`: the bar, windows and borders, managed or not.
#[tauri::command]
pub(crate) fn desktop_status(name: String) -> Result<DesktopStatus, AppError> {
    let t = tool(&name)?;
    let s = palette::desktop::status(&paths()?, t);
    Ok(DesktopStatus {
        tool: t.name(),
        app: t.app(),
        installed: s.installed,
        managed: s.managed,
        files: s.files.iter().map(|p| p.display().to_string()).collect(),
        original: s.original.map(|p| p.display().to_string()),
        backups: s.backups.len(),
        history: s
            .backups
            .iter()
            .filter_map(|p| Some(p.file_name()?.to_string_lossy().into_owned()))
            .collect(),
    })
}

/// The settings (what Palette would start from, while it doesn't manage it).
#[tauri::command]
pub(crate) fn desktop_get(name: String) -> Result<serde_json::Value, AppError> {
    let value = palette::desktop::get(&paths()?, tool(&name)?).map_err(err)?;
    serde_json::to_value(value).map_err(err)
}

/// Save settings and write the config (backup first), then reload the tool.
#[tauri::command]
pub(crate) async fn desktop_set(
    name: String,
    settings: serde_json::Value,
) -> Result<DesktopApplied, AppError> {
    let t = tool(&name)?;
    let value = to_toml(settings)?;
    let done = tauri::async_runtime::spawn_blocking(move || {
        palette::desktop::set(&paths()?, t, value)
            .map(desktop_applied)
            .map_err(err)
    })
    .await
    .map_err(err)?;
    crate::beats::sync_soon();
    done
}

/// "Let Palette manage it" (`manage`) or "Give it back" (`release`).
#[tauri::command]
pub(crate) async fn desktop_own(name: String, manage: bool) -> Result<DesktopApplied, AppError> {
    let t = tool(&name)?;
    let done = tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let done = if manage {
            palette::desktop::manage(&paths, t)
        } else {
            palette::desktop::release(&paths, t)
        };
        done.map(desktop_applied).map_err(err)
    })
    .await
    .map_err(err)??;
    crate::beats::sync_soon();
    Ok(done)
}

/// Go back to a backup, by its name in `history`.
#[tauri::command]
pub(crate) async fn desktop_restore(
    name: String,
    backup: String,
) -> Result<DesktopApplied, AppError> {
    let t = tool(&name)?;
    let done = tauri::async_runtime::spawn_blocking(move || {
        palette::desktop::restore(&paths()?, t, &backup)
            .map(desktop_applied)
            .map_err(err)
    })
    .await
    .map_err(err)??;
    crate::beats::sync_soon();
    Ok(done)
}

/// Installed font families, for the bar's font.
#[tauri::command]
pub(crate) async fn font_families() -> Vec<String> {
    tauri::async_runtime::spawn_blocking(palette::system::fonts::families)
        .await
        .unwrap_or_default()
}

/// The styles a font really has (a missing one shows no icons).
#[tauri::command]
pub(crate) fn font_styles(family: String) -> Vec<String> {
    palette::system::fonts::styles(&family)
}

/// Dock and menu bar, as macOS has them now.
#[tauri::command]
pub(crate) fn macos_get() -> palette::desktop::macos::MacBits {
    palette::desktop::macos::read()
}

#[tauri::command]
pub(crate) async fn macos_set(
    settings: palette::desktop::macos::MacBits,
) -> Result<String, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        match palette::desktop::macos::set(&paths()?, &settings).map_err(err)? {
            Outcome::Failed(e) => Err(AppError::from(e)),
            Outcome::Done(m) | Outcome::Unchanged(m) | Outcome::Skipped(m) => Ok(m),
        }
    })
    .await
    .map_err(err)?
}

/// The prompt's modules and the options the page shows for each.
#[tauri::command]
pub(crate) fn prompt_meta() -> serde_json::Value {
    use palette::desktop::prompt;
    let options: Vec<prompt::Opt> = prompt::MODULES
        .iter()
        .map(|m| m.id)
        .chain(["character"])
        .flat_map(prompt::options_of)
        .collect();
    serde_json::json!({ "modules": prompt::MODULES, "options": options })
}

/// The real prompt for unsaved settings, as coloured runs of text.
#[tauri::command]
pub(crate) async fn prompt_preview(
    settings: serde_json::Value,
) -> Result<Vec<palette::desktop::prompt::Span>, AppError> {
    let value = to_toml(settings)?;
    let p: palette::desktop::prompt::Prompt = value.try_into().map_err(err)?;
    tauri::async_runtime::spawn_blocking(move || {
        palette::desktop::prompt::preview(&paths()?, &p).map_err(err)
    })
    .await
    .map_err(err)?
}

/// Starship's presets.
#[tauri::command]
pub(crate) async fn prompt_presets() -> Vec<String> {
    tauri::async_runtime::spawn_blocking(palette::desktop::prompt::presets)
        .await
        .unwrap_or_default()
}

/// Use a preset as your prompt (your file is backed up first).
#[tauri::command]
pub(crate) async fn prompt_preset(name: String) -> Result<DesktopApplied, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        palette::desktop::prompt_preset(&paths()?, &name)
            .map(desktop_applied)
            .map_err(err)
    })
    .await
    .map_err(err)?
}

/// AeroSpace commands with what they do, for the shortcut editor.
#[tauri::command]
pub(crate) fn aerospace_commands() -> &'static [(&'static str, &'static str)] {
    palette::desktop::windows::COMMANDS
}

/// Move a whole pill in the bar being edited (not saved: Apply saves). `to` is
/// left/center/right/next/prev, or `before` another pill's index. Returns the
/// settings and the pill's new index.
#[tauri::command]
pub(crate) fn bar_move_pill(
    settings: serde_json::Value,
    from: usize,
    to: Option<String>,
    before: Option<usize>,
) -> Result<(serde_json::Value, usize), AppError> {
    use palette::desktop::bar::{Bar, Place};
    let mut bar: Bar = serde_json::from_value(settings).map_err(err)?;
    let place = match (before, to) {
        (Some(i), _) => Place::Before(i),
        (None, Some(to)) => to.parse().map_err(err)?,
        (None, None) => return Err("say where the pill goes".into()),
    };
    let at = bar.move_pill(from, place).map_err(err)?;
    Ok((serde_json::to_value(&bar).map_err(err)?, at))
}

/// The Bar page's unsaved settings as the core's bar (JSON `null`s left out).
fn bar_of(settings: serde_json::Value) -> Result<palette::desktop::bar::Bar, AppError> {
    to_toml(settings)?.try_into().map_err(err)
}

fn bar_json(bar: &palette::desktop::bar::Bar) -> Result<serde_json::Value, AppError> {
    serde_json::to_value(bar).map_err(err)
}

/// For the Bar page's App icons (IDEAS 37b): the icon font's glyph names, the apps open now
/// (AeroSpace's names), the installed apps, and what the map gives each of those (and each
/// app with your icon). Empty when there's no map / no AeroSpace.
#[tauri::command]
pub(crate) async fn bar_icon_choices() -> Result<serde_json::Value, AppError> {
    use palette::desktop::bar;
    tauri::async_runtime::spawn_blocking(|| {
        let paths = paths()?;
        let open = bar::open_apps(&paths);
        let installed = bar::installed_apps(&paths);
        let yours = palette::desktop::get(&paths, palette::desktop::Tool::Bar)
            .ok()
            .and_then(|v| {
                Some(
                    v.get("icons")?
                        .as_table()?
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>(),
                )
            })
            .unwrap_or_default();
        let mut all: Vec<String> = open
            .iter()
            .chain(&installed)
            .chain(&yours)
            .cloned()
            .collect();
        all.sort();
        all.dedup();
        Ok(serde_json::json!({
            "glyphs": bar::glyphs(&paths),
            "open": open,
            "installed": installed,
            "map": bar::map_icons(&paths, &all),
        }))
    })
    .await
    .map_err(err)?
}

/// Ready pills, whole layouts and looks for the Bar page's tray (18b, S8).
#[tauri::command]
pub(crate) fn bar_presets() -> serde_json::Value {
    use palette::desktop::bar::presets::{LOOKS, PRESETS};
    serde_json::json!({ "presets": PRESETS, "looks": LOOKS })
}

/// Add a ready pill (or a whole layout) to the bar being edited; Apply saves it. Returns the
/// settings and what was added (the new pill's index, items left out).
#[tauri::command]
pub(crate) fn bar_add_preset(
    settings: serde_json::Value,
    id: String,
    side: String,
) -> Result<(serde_json::Value, palette::desktop::bar::presets::Added), AppError> {
    let paths = paths()?;
    let mut bar = bar_of(settings)?;
    let have = |k: &str| palette::desktop::bar::live::have(&paths, k);
    let added = palette::desktop::bar::presets::add(&mut bar, &id, &side, &have).map_err(err)?;
    Ok((bar_json(&bar)?, added))
}

/// The bar being edited, drawn in a ready look (items stay).
#[tauri::command]
pub(crate) fn bar_look(
    settings: serde_json::Value,
    id: String,
) -> Result<serde_json::Value, AppError> {
    let bar = bar_of(settings)?;
    let menu = palette::system::menu_bar_height();
    bar_json(&palette::desktop::bar::presets::with_look(&bar, &id, menu).map_err(err)?)
}

/// Split (before item `at`), merge (with the next pill) or duplicate pill `pill` in the bar
/// being edited (S7). Returns the settings and the pill to select.
#[tauri::command]
pub(crate) fn bar_pill_action(
    settings: serde_json::Value,
    action: String,
    pill: usize,
    at: Option<usize>,
) -> Result<(serde_json::Value, usize), AppError> {
    let mut bar = bar_of(settings)?;
    let to = match action.as_str() {
        "split" => bar.split_pill(pill, at.unwrap_or(1)),
        "merge" => bar.merge_pill(pill),
        "duplicate" => bar.duplicate_pill(pill),
        other => return Err(format!("no pill action `{other}`").into()),
    }
    .map_err(err)?;
    Ok((bar_json(&bar)?, to))
}

/// What the running bar shows now, by item (S5), and why an item can't work here (S6).
#[tauri::command]
pub(crate) async fn bar_live(settings: serde_json::Value) -> Result<serde_json::Value, AppError> {
    let bar = bar_of(settings)?;
    tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        Ok(serde_json::json!({
            "values": palette::desktop::bar::live::values(&paths, &bar),
            "needs": palette::desktop::bar::live::needs(&paths),
        }))
    })
    .await
    .map_err(err)?
}

/// The saved bars (18c), the one in use marked.
#[tauri::command]
pub(crate) fn bar_saved() -> Result<Vec<palette::desktop::bar::saved::Saved>, AppError> {
    Ok(palette::desktop::bar::saved::list(&paths()?))
}

/// Save the bar as it is (the applied one) under a new name; it's then the one in use.
#[tauri::command]
/// `settings`: the page's unapplied changes, applied into the new bar only.
pub(crate) fn bar_save_as(
    app: AppHandle,
    name: String,
    settings: Option<serde_json::Value>,
) -> Result<String, AppError> {
    let edited = settings.map(bar_of).transpose()?;
    let name = palette::desktop::bar::saved::save_as(&paths()?, &name, edited).map_err(err)?;
    crate::tray::refresh(&app);
    Ok(name)
}

/// Switch to a saved bar: written and reloaded, a backup first.
#[tauri::command]
pub(crate) async fn bar_use(app: AppHandle, name: String) -> Result<DesktopApplied, AppError> {
    let a = tauri::async_runtime::spawn_blocking(move || {
        palette::desktop::bar::saved::use_bar(&paths()?, &name).map_err(err)
    })
    .await
    .map_err(err)??;
    crate::tray::refresh(&app);
    crate::beats::sync_soon();
    let _ = app.emit("bar-changed", ());
    Ok(desktop_applied(a))
}

#[tauri::command]
pub(crate) fn bar_rename_saved(
    app: AppHandle,
    name: String,
    new: String,
) -> Result<String, AppError> {
    let new = palette::desktop::bar::saved::rename(&paths()?, &name, &new).map_err(err)?;
    crate::tray::refresh(&app);
    Ok(new)
}

#[tauri::command]
pub(crate) fn bar_delete_saved(app: AppHandle, name: String) -> Result<String, AppError> {
    let name = palette::desktop::bar::saved::delete(&paths()?, &name).map_err(err)?;
    crate::tray::refresh(&app);
    Ok(name)
}

/// What can go in a bar pill.
#[tauri::command]
pub(crate) fn bar_kinds() -> &'static [palette::desktop::bar::Kind] {
    palette::desktop::bar::KINDS
}

/// Running apps (bundle id, name), for window rules and borders.
#[tauri::command]
pub(crate) async fn running_apps() -> Vec<(String, String)> {
    tauri::async_runtime::spawn_blocking(palette::desktop::windows::running_apps)
        .await
        .unwrap_or_default()
}

/// "Match the menu bar" on the Bar page: the unsaved settings sized to this Mac's menu bar strip
/// (`palette::desktop::bar::matched_to_menu_bar`); Apply saves them, as any change.
#[tauri::command]
pub(crate) fn bar_match_menu_bar(
    settings: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let bar: palette::desktop::bar::Bar = to_toml(settings)?.try_into().map_err(err)?;
    let menu = palette::system::menu_bar_height()
        .ok_or_else(|| AppError::from("couldn't read the menu bar's height".to_string()))?;
    serde_json::to_value(palette::desktop::bar::matched_to_menu_bar(&bar, menu)).map_err(err)
}

/// A desktop page's settings as TOML. JSON's `null` (a switch turned off, like "Different top
/// gap on other displays") has no TOML form: converting it failed with "unsupported None
/// value", so the whole Apply failed (owner, 2026-11-08). A missing field reads as off.
fn to_toml(settings: serde_json::Value) -> Result<toml::Value, AppError> {
    fn drop_nulls(v: serde_json::Value) -> serde_json::Value {
        use serde_json::Value::{Array, Null, Object};
        match v {
            Object(map) => Object(
                map.into_iter()
                    .filter(|(_, v)| !v.is_null())
                    .map(|(k, v)| (k, drop_nulls(v)))
                    .collect(),
            ),
            Array(list) => Array(
                list.into_iter()
                    .filter(|v| *v != Null)
                    .map(drop_nulls)
                    .collect(),
            ),
            other => other,
        }
    }
    toml::Value::try_from(drop_nulls(settings)).map_err(err)
}

#[cfg(test)]
mod tests {
    use super::to_toml;

    #[test]
    fn a_switched_off_value_is_left_out_instead_of_failing() {
        let page = serde_json::json!({
            "accordion_padding": 40,
            "gaps": { "outer_top": 4, "outer_top_other": null },
            "rules": [{ "app_id": "x", "workspace": null }],
        });
        let t = to_toml(page).expect("converts");
        assert_eq!(t["accordion_padding"].as_integer(), Some(40));
        assert!(t["gaps"].get("outer_top_other").is_none());
        assert!(t["rules"][0].get("workspace").is_none());
    }
}
