//! Themes and switching: the theme list, thumbnails, apply/step/undo, and the
//! "Switched to X" announcement (toast, menu).

use super::*;

#[derive(Serialize)]
pub(crate) struct ThemeInfo {
    pub(crate) name: String,
    pub(crate) origin: &'static str,
    pub(crate) mode: &'static str,
    pub(crate) colors: BTreeMap<String, String>,
    pub(crate) ansi: Vec<String>,
    pub(crate) current: bool,
    pub(crate) wallpapers: usize,
    /// Set when the theme can't be loaded (bad colors.toml); shown greyed out.
    pub(crate) invalid: Option<String>,
    /// One of the user's favourites (★ in the picker).
    pub(crate) favourite: bool,
    /// Glass (11g): the wallpaper this theme shows (`None` when no app is see-through), and
    /// the lift set by hand (F2: `None` = Auto, 0 = off, 1-100 = stronger).
    pub(crate) glass_wallpaper: Option<String>,
    pub(crate) glass_lift: Option<u8>,
    /// Low-contrast pairs (`palette check`), one line each with the fix.
    pub(crate) contrast: Vec<String>,
    /// Words for search by look (12c, `palette::search::tags`).
    pub(crate) tags: Vec<&'static str>,
}

/// `palette check` as lines for a badge's tooltip: "what: ratio … — fix".
pub(crate) fn contrast_lines(report: &palette::check::Report) -> Vec<String> {
    report
        .issues
        .iter()
        .map(|i| format!("{}. {}", i.summary(), i.fix()))
        .collect()
}

pub(crate) fn info(theme: &Theme, origin: &'static str, current: bool) -> ThemeInfo {
    ThemeInfo {
        name: theme.name.clone(),
        origin,
        mode: theme.colors.mode.as_str(),
        colors: theme
            .colors
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        ansi: theme.colors.ansi().iter().map(|c| c.to_string()).collect(),
        current,
        wallpapers: theme.backgrounds().map_or(0, |b| b.len()),
        invalid: None,
        favourite: false,
        glass_wallpaper: None,
        glass_lift: None,
        contrast: contrast_lines(&palette::check::theme(theme)),
        tags: palette::search::tags(&theme.colors),
    }
}

/// Every theme, with its colours, for the picker and the overlay.
#[tauri::command]
pub(crate) fn themes() -> Result<Vec<ThemeInfo>, AppError> {
    let paths = paths()?;
    let current = palette::state::current_theme(&paths).map_err(err)?;
    let favourites = palette::favourites::list(&paths).unwrap_or_default();
    let settings = palette::settings::load(&paths).unwrap_or_default();
    let see_through = palette::settings::GLASS_APPS
        .iter()
        .chain(&["zed"])
        .any(|a| palette::glass::opacity(&settings, a).is_some());
    let mut out = Vec::new();
    for (name, origin) in palette::theme::list(&paths).map_err(err)? {
        let origin = match origin {
            Origin::Bundled => "bundled",
            Origin::User => "user",
        };
        let is_current = current.as_deref() == Some(name.as_str());
        match palette::theme::load(&paths, &name) {
            Ok(theme) => {
                let shown = see_through
                    .then(|| palette::effects::wallpaper_to_use(&paths, &theme, None))
                    .flatten();
                out.push(ThemeInfo {
                    favourite: favourites.contains(&name),
                    glass_lift: settings.glass_lift.get(&name).copied(),
                    glass_wallpaper: shown
                        .and_then(|w| w.file_name().map(|f| f.to_string_lossy().to_string())),
                    ..info(&theme, origin, is_current)
                })
            }
            Err(e) => out.push(ThemeInfo {
                favourite: favourites.contains(&name),
                name,
                origin,
                mode: "dark",
                colors: BTreeMap::new(),
                ansi: Vec::new(),
                current: is_current,
                wallpapers: 0,
                glass_wallpaper: None,
                glass_lift: None,
                invalid: Some(err(e).message),
                contrast: Vec::new(),
                tags: Vec::new(),
            }),
        }
    }
    Ok(out)
}

fn lift_of(value: Option<u8>) -> palette::glass::Lift {
    value.map_or(palette::glass::Lift::Auto, palette::glass::Lift::By)
}

/// Settings → My themes, the glass slider (F2): `None` = Auto, 0 = off, 1-100 = stronger.
/// Returns the label it now has ("Auto", "off", "60 %").
#[tauri::command]
pub(crate) async fn set_glass_lift(
    app: AppHandle,
    name: String,
    value: Option<u8>,
) -> Result<String, AppError> {
    let now = tauri::async_runtime::spawn_blocking(move || {
        palette::glass::set_lift(&paths()?, &name, Some(lift_of(value))).map_err(err)
    })
    .await
    .map_err(err)??;
    crate::tray::refresh(&app);
    Ok(now.label())
}

/// The slider's live preview: the theme's text on the most see-through app's glass at
/// `value`, without saving (`null` when nothing is see-through).
#[tauri::command]
pub(crate) async fn glass_preview(
    name: String,
    value: Option<u8>,
) -> Result<Option<palette::glass::Preview>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        palette::glass::preview(&paths()?, &name, Some(lift_of(value))).map_err(err)
    })
    .await
    .map_err(err)?
}

/// A small JPEG of the theme's preview or first wallpaper, as an `asset://` address the page
/// loads from the cache folder. Made on first use, then cached.
#[tauri::command]
pub(crate) async fn thumbnail(name: String) -> Result<Option<String>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let theme = palette::theme::load(&paths, &name).map_err(err)?;
        let Some(file) = palette::thumbs::thumbnail(&paths, &theme).map_err(err)? else {
            return Ok(None);
        };
        Ok(Some(asset_url(&file)))
    })
    .await
    .map_err(err)?
}

#[derive(Serialize, Clone)]
pub(crate) struct Applied {
    pub(crate) theme: String,
    /// Apps or settings that failed to update, for a short notice.
    pub(crate) problems: Vec<String>,
}

pub(crate) fn applied(theme: String, effects: &[Effect]) -> Applied {
    let problems = effects
        .iter()
        .filter_map(|(what, outcome)| match outcome {
            Outcome::Failed(why) => Some(format!("{what}: {why}")),
            _ => None,
        })
        .collect();
    Applied { theme, problems }
}

/// Switch to a theme: the same `palette::switch` that `palette set` runs.
#[tauri::command]
pub(crate) async fn apply(
    app: AppHandle,
    name: String,
    wallpaper: Option<String>,
) -> Result<Applied, AppError> {
    run_switch(app, Switch::To(name, wallpaper)).await
}

/// `next`, `prev` or `undo`.
#[tauri::command]
pub(crate) async fn step(app: AppHandle, how: String) -> Result<Applied, AppError> {
    let how = match how.as_str() {
        "next" => Switch::Next,
        "prev" => Switch::Prev,
        "undo" => Switch::Undo,
        other => return Err(format!("unknown step `{other}`").into()),
    };
    run_switch(app, how).await
}

pub(crate) enum Switch {
    /// A theme, optionally starting on one of its wallpapers.
    To(String, Option<String>),
    Next,
    Prev,
    Undo,
}

/// Runs off the main thread (reloading apps and setting the wallpaper take a
/// moment), then tells both windows which theme is active now.
pub(crate) async fn run_switch(app: AppHandle, how: Switch) -> Result<Applied, AppError> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        // Next/prev pick the theme inside the switch lock (`palette::step`).
        let switch = match how {
            Switch::To(name, wallpaper) => {
                palette::switch_with_wallpaper(&paths, &name, wallpaper.as_deref(), false)
            }
            Switch::Next => palette::step(&paths, 1, None, false),
            Switch::Prev => palette::step(&paths, -1, None, false),
            Switch::Undo => {
                let (name, effects) = palette::undo(&paths).map_err(err)?;
                return Ok((applied(name, &effects), true));
            }
        }
        .map_err(err)?;
        Ok::<_, AppError>((applied(switch.theme, &switch.effects), false))
    })
    .await
    .map_err(err)??;
    let (result, undo) = result;
    let _ = app.emit("theme-changed", result.clone());
    announce(&app, &result, undo);
    tray::refresh(&app);
    Ok(result)
}

/// What the "Switched to X · Undo" toast shows.
#[derive(Serialize, Clone)]
pub(crate) struct Switched {
    pub(crate) theme: String,
    pub(crate) problems: Vec<String>,
    /// This was an undo: say "Back to X", no Undo button.
    pub(crate) undo: bool,
    /// The new theme's colours, so the toast is painted in them.
    pub(crate) colors: BTreeMap<String, String>,
}

/// After a switch from the app (picker, quick switcher, tray, bar pill): a
/// toast with Undo. The picker shows it itself when open; Settings has its
/// own messages; otherwise a small toast window appears at the top right
/// without taking focus from the app you're in, and hides after 5 s (on Linux
/// a desktop notification instead, `notify_switch`).
pub(crate) fn announce(app: &AppHandle, result: &Applied, undo: bool) {
    let colors = paths()
        .ok()
        .and_then(|p| palette::theme::load(&p, &result.theme).ok())
        .map(|t| {
            t.colors
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let switched = Switched {
        theme: result.theme.clone(),
        problems: result.problems.clone(),
        undo,
        colors,
    };
    if let Ok(mut last) = app.state::<LastSwitch>().0.lock() {
        *last = Some(switched.clone());
    }
    let _ = app.emit("switched", switched);
    // This runs on a tokio worker after a switch: windows only on the main thread (`on_main`).
    let result = result.clone();
    on_main(app, move |app| {
        let open = |label: &str, check: fn(&WebviewWindow) -> bool| {
            app.get_webview_window(label).is_some_and(|w| check(&w))
        };
        if open("picker", |w| w.is_visible().unwrap_or(false))
            || open("settings", |w| w.is_focused().unwrap_or(false))
        {
            return;
        }
        #[cfg(target_os = "linux")]
        {
            notify_switch(app, &result, undo);
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = &result;
            if let Some(toast) = window(app, "toast") {
                place_top_right(&toast);
                // Not `show()` above: that activates Palette and takes focus.
                let _ = toast.show();
            }
        }
    });
}

/// Linux: a desktop notification instead of the toast window. Wayland lets
/// no app place a small window of its own, and a tiling compositor (niri)
/// tiled the toast full size, where the mouse over it kept it from hiding.
/// The notification server (Noctalia, GNOME, KDE) shows it and lets it
/// expire; its Undo action does what the toast's button does. `notify-send`
/// waits for the click, so it runs on its own thread.
#[cfg(target_os = "linux")]
pub(crate) fn notify_switch(app: &AppHandle, result: &Applied, undo: bool) {
    let title = if undo {
        format!("Back to {}", result.theme)
    } else {
        format!("Switched to {}", result.theme)
    };
    let mut args = vec![
        "--app-name=Palette".to_string(),
        "--expire-time=5000".into(),
    ];
    if let Some(icon) = paths()
        .ok()
        .map(|p| palette::launcher::icon_path(&p))
        .filter(|i| i.is_file())
    {
        args.push(format!("--icon={}", icon.display()));
    }
    if !undo {
        args.push("--action=undo=Undo".into());
    }
    args.push(title);
    if !result.problems.is_empty() {
        args.push(result.problems.join("\n"));
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        if palette::system::run("notify-send", &args).is_ok_and(|out| out == "undo") {
            tauri::async_runtime::spawn(async move {
                let _ = step(app, "undo".into()).await;
            });
        }
    });
}

/// The switch a toast made just now should show (once).
#[tauri::command]
pub(crate) fn last_switch(state: tauri::State<LastSwitch>) -> Option<Switched> {
    state.0.lock().ok().and_then(|mut s| s.take())
}

/// A fingerprint of Palette's files (`palette::state::stamp`): Settings redraws on focus only
/// when it changed since the last look (14b).
#[tauri::command]
pub(crate) fn stamp() -> Result<String, AppError> {
    Ok(palette::state::stamp(&paths()?))
}

/// The tab a Settings window made just now should open on (once).
#[tauri::command]
pub(crate) fn pending_tab(state: tauri::State<PendingTab>) -> Option<String> {
    state.0.lock().ok().and_then(|mut t| t.take())
}

/// Settings → General: one line per switch in palette.log.
#[tauri::command]
pub(crate) fn set_log(on: bool) -> Result<bool, AppError> {
    palette::set_log(&paths()?, Some(on)).map_err(err)
}

/// Settings → General: close Settings after ten idle minutes (windows on demand).
#[tauri::command]
pub(crate) fn set_close_idle_settings(on: bool) -> Result<bool, AppError> {
    palette::set_close_idle_settings(&paths()?, Some(on)).map_err(err)
}

/// Settings → General: windows ready or on demand, and what a menu bar click opens.
/// `None` leaves a choice as it is. Takes effect at once.
#[tauri::command]
pub(crate) fn set_app_prefs(
    app: AppHandle,
    windows: Option<String>,
    click: Option<String>,
) -> Result<(), AppError> {
    use palette::settings::{AppWindows, MenuClick};
    let windows = windows
        .as_deref()
        .map(AppWindows::parse)
        .transpose()
        .map_err(err)?;
    let click = click
        .as_deref()
        .map(MenuClick::parse)
        .transpose()
        .map_err(err)?;
    let (w, c) = palette::app_prefs(&paths()?, windows, click).map_err(err)?;
    apply_app_prefs(&app, w, c);
    Ok(())
}

/// Make the windows a "ready" choice keeps loaded, and set what a left click on the
/// tray icon does (macOS opens the native menu itself when told to).
pub(crate) fn apply_app_prefs(
    app: &AppHandle,
    windows: palette::settings::AppWindows,
    click: palette::settings::MenuClick,
) {
    use palette::settings::AppWindows;
    // The quick switcher is loaded unless everything is on demand.
    if windows != AppWindows::AllOnDemand {
        window(app, "overlay");
    }
    if windows == AppWindows::Ready {
        for label in ON_DEMAND {
            // Linux shows a notification instead of the toast.
            if cfg!(target_os = "linux") && label == "toast" {
                continue;
            }
            window(app, label);
        }
    }
    if let Some(tray) = app.tray_by_id(tray::TRAY_ID) {
        let _ = tray.set_show_menu_on_left_click(click == palette::settings::MenuClick::Menu);
    }
}

pub(crate) fn app_settings() -> palette::settings::Settings {
    paths()
        .and_then(|p| palette::settings::load(&p).map_err(err))
        .unwrap_or_default()
}

#[tauri::command]
pub(crate) fn get_hotkey(state: tauri::State<Hotkey>) -> String {
    state.0.lock().map(|h| h.clone()).unwrap_or_default()
}

/// Register a new hotkey and save it. The old one stays if the new one
/// can't be registered (another app owns it).
#[tauri::command]
pub(crate) fn set_hotkey(app: AppHandle, combo: String) -> Result<String, AppError> {
    let paths = paths()?;
    let combo = palette::settings::normalise_hotkey(&combo).map_err(err)?;
    register_hotkey(&app, &combo)?;
    let mut settings = palette::settings::load(&paths).map_err(err)?;
    settings.hotkey = combo.clone();
    palette::settings::save(&paths, &settings).map_err(err)?;
    Ok(combo)
}

#[tauri::command]
pub(crate) fn hide(window: WebviewWindow) {
    // Hidden, and closed a moment later with windows on demand (`put_away`).
    crate::windows::put_away(&window);
}
