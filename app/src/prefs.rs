//! Settings → Apps and General: which apps are themed, preferences, Performance mode,
//! window glass, and Mac or Linux mode.

use super::*;

#[derive(Serialize)]
pub(crate) struct AppRow {
    pub(crate) name: &'static str,
    pub(crate) on: bool,
    pub(crate) installed: bool,
    pub(crate) wired: bool,
    pub(crate) detail: String,
    /// On and installed but not hooked up yet (e.g. new in this version):
    /// Settings offers a "Set up" button.
    pub(crate) can_wire: bool,
}

/// Every app Palette can theme, plus wallpaper and light/dark.
#[tauri::command]
pub(crate) fn apps() -> Result<Vec<AppRow>, AppError> {
    let paths = paths()?;
    Ok(palette::apps(&paths)
        .map_err(err)?
        .into_iter()
        .map(|a| AppRow {
            can_wire: a.on && a.installed && !a.wired && a.detail.starts_with("not wired"),
            name: a.name,
            on: a.on,
            installed: a.installed,
            wired: a.wired,
            detail: a.detail,
        })
        .collect())
}

/// Switch one app on or off: the same `palette::set_app` as `palette apps`.
#[tauri::command]
pub(crate) async fn set_app(name: String, on: bool) -> Result<Vec<String>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let (_, effects) = palette::set_app(&paths, &name, on).map_err(err)?;
        Ok(applied(name, &effects).problems)
    })
    .await
    .map_err(err)?
}

#[derive(Serialize)]
pub(crate) struct Prefs {
    /// What next/prev and the bar pill step through: "all" or "favourites".
    pub(crate) next_cycles: &'static str,
    /// Scrolling on the bar's theme pill switches theme.
    pub(crate) bar_scroll: bool,
    pub(crate) hotkey: String,
    pub(crate) login: bool,
    pub(crate) welcomed: bool,
    pub(crate) obsidian_ring: bool,
    pub(crate) obsidian_dim: bool,
    /// "off", "blurred" or "clear".
    pub(crate) zed_glass: &'static str,
    pub(crate) zed_strength: u8,
    /// Terminal glass (`palette glass`): app → kind, strength, blur, and the
    /// kinds that app offers.
    pub(crate) glass: std::collections::BTreeMap<&'static str, GlassPref>,
    /// Performance mode and the parts it covers.
    pub(crate) performance: PerfPref,
    /// "ready" or "on-demand".
    pub(crate) windows: &'static str,
    /// What a menu bar click opens: "menu", "picker" or "quick-switcher".
    pub(crate) menu_click: &'static str,
    /// One line per switch in palette.log.
    pub(crate) close_idle_settings: bool,
    pub(crate) log: bool,
    /// Monochrome themes: spaced greys and the tint (Settings → General).
    pub(crate) monochrome: crate::colours::MonoPref,
}

#[derive(Serialize, Clone, Copy)]
pub(crate) struct PerfPref {
    pub(crate) on: bool,
    pub(crate) glass: bool,
    pub(crate) bar: bool,
    pub(crate) borders: bool,
    pub(crate) stats: bool,
}

impl From<palette::settings::Performance> for PerfPref {
    fn from(p: palette::settings::Performance) -> Self {
        Self {
            on: p.on,
            glass: p.glass,
            bar: p.bar,
            borders: p.borders,
            stats: p.stats,
        }
    }
}

/// Performance mode (`palette performance`): `on` switches it, `part` + `value` chooses
/// what it covers. Re-renders what changed; returns the state now.
#[tauri::command]
pub(crate) async fn set_performance(
    app: AppHandle,
    on: Option<bool>,
    part: Option<String>,
    value: Option<bool>,
) -> Result<PerfPref, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let parts: Vec<(&str, bool)> = match (&part, value) {
            (Some(p), Some(v)) => vec![(p.as_str(), v)],
            _ => vec![],
        };
        let p = palette::performance(&paths()?, on, &parts).map_err(err)?;
        tray::refresh(&app);
        Ok(p.into())
    })
    .await
    .map_err(err)?
}

#[derive(Serialize)]
pub(crate) struct GlassPref {
    pub(crate) kind: &'static str,
    pub(crate) strength: u8,
    pub(crate) blur: u8,
    pub(crate) kinds: Vec<&'static str>,
}

#[tauri::command]
pub(crate) fn prefs(app: AppHandle) -> Result<Prefs, AppError> {
    let settings = palette::settings::load(&paths()?).map_err(err)?;
    Ok(Prefs {
        next_cycles: settings.next_cycles.as_str(),
        bar_scroll: settings.bar_scroll,
        hotkey: settings.hotkey.clone(),
        login: app.autolaunch().is_enabled().unwrap_or(false),
        welcomed: settings.welcomed,
        windows: settings.windows.as_str(),
        menu_click: settings.menu_click.as_str(),
        log: settings.log,
        close_idle_settings: settings.close_idle_settings,
        monochrome: settings.monochrome.into(),
        obsidian_ring: settings.obsidian_ring,
        obsidian_dim: settings.obsidian_dim,
        zed_glass: settings.zed_glass.as_str(),
        zed_strength: settings.zed_strength,
        performance: settings.performance.into(),
        glass: palette::settings::GLASS_APPS
            .iter()
            .map(|&app| {
                // Your choice, not Performance mode's solid (Settings edits this one).
                let g = settings.glass_saved(app);
                let kinds = palette::settings::TermGlass::kinds(app)
                    .iter()
                    .map(|k| k.as_str())
                    .collect();
                (
                    app,
                    GlassPref {
                        kind: g.kind.as_str(),
                        strength: g.strength,
                        blur: g.blur,
                        kinds,
                    },
                )
            })
            .collect(),
    })
}

/// A terminal's window glass (`palette glass`): kind, strength 0-100, blur
/// 0-100; `None` keeps that one. Re-renders, so the terminal reloads.
#[tauri::command]
pub(crate) async fn set_glass(
    app: String,
    kind: Option<String>,
    strength: Option<u8>,
    blur: Option<u8>,
) -> Result<(), AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let kind = kind
            .map(|k| palette::settings::TermGlass::parse(&k))
            .transpose()
            .map_err(err)?;
        palette::glass(&paths()?, &app, kind, strength, blur)
            .map(|_| ())
            .map_err(err)
    })
    .await
    .map_err(err)?
}

/// Obsidian's active-pane ring and dimming (`palette obsidian`); `None` keeps
/// that one. Re-renders the theme, so Obsidian updates live.
#[tauri::command]
pub(crate) async fn set_obsidian_look(
    ring: Option<bool>,
    dim: Option<bool>,
) -> Result<(bool, bool), AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        palette::obsidian_look(&paths()?, ring, dim).map_err(err)
    })
    .await
    .map_err(err)?
}

/// Zed's window glass ("off", "blurred", "clear") and strength 0-100
/// (`palette zed`); `None` keeps that one. Re-renders, so Zed restyles live.
#[tauri::command]
pub(crate) async fn set_zed_look(
    glass: Option<String>,
    strength: Option<u8>,
) -> Result<(), AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let glass = glass
            .map(|g| palette::settings::Glass::parse(&g))
            .transpose()
            .map_err(err)?;
        palette::zed_look(&paths()?, glass, strength)
            .map(|_| ())
            .map_err(err)
    })
    .await
    .map_err(err)?
}

/// The welcome tour was finished or skipped (`false`: show it again next launch).
#[tauri::command]
pub(crate) fn set_welcomed(done: bool) -> Result<(), AppError> {
    let paths = paths()?;
    let mut settings = palette::settings::load(&paths).map_err(err)?;
    settings.welcomed = done;
    palette::settings::save(&paths, &settings).map_err(err)
}

#[tauri::command]
pub(crate) fn set_login(app: AppHandle, on: bool) -> Result<bool, AppError> {
    let autolaunch = app.autolaunch();
    if on {
        autolaunch.enable().map_err(err)?;
        route_login_item_through_macos();
    } else {
        autolaunch.disable().map_err(err)?;
    }
    Ok(autolaunch.is_enabled().unwrap_or(false))
}

/// The login item (a LaunchAgent written by the autostart plugin) runs the program file
/// directly, so macOS doesn't count Palette as open: every palette:// link then started a
/// second copy, which quit before it could pass the link on (scroll on the pill opened the
/// picker instead of switching). Start it through LaunchServices instead (`open -g -a`), so
/// links reach the running Palette. The plugin's "is it on?" only checks that the file exists.
pub(crate) fn route_login_item_through_macos() {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let agent = std::path::Path::new(&home).join("Library/LaunchAgents/Palette.plist");
    let Ok(text) = std::fs::read_to_string(&agent) else {
        return;
    };
    if text.contains("<string>/usr/bin/open</string>") {
        return;
    }
    // …/Palette.app/Contents/MacOS/palette-app → …/Palette.app (skip dev runs outside a bundle)
    let Some(bundle) = std::env::current_exe()
        .ok()
        .and_then(|e| e.ancestors().nth(3).map(|p| p.to_path_buf()))
    else {
        return;
    };
    if bundle.extension().and_then(|e| e.to_str()) != Some("app") {
        return;
    }
    let esc = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let plist = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n\
         \x20 <key>Label</key>\n  <string>Palette</string>\n\
         \x20 <key>AssociatedBundleIdentifiers</key>\n  <array><string>dev.palette.switcher</string></array>\n\
         \x20 <key>ProgramArguments</key>\n  <array><string>/usr/bin/open</string><string>-g</string><string>-a</string><string>{}</string><string>--args</string><string>{AUTOSTART_ARG}</string></array>\n\
         \x20 <key>RunAtLoad</key>\n  <true/>\n\
         </dict>\n</plist>\n",
        esc(&bundle.to_string_lossy())
    );
    let _ = std::fs::write(&agent, plist);
}

#[tauri::command]
pub(crate) fn open_settings(app: AppHandle) {
    show_settings(&app);
}

#[derive(Serialize)]
pub(crate) struct SystemView {
    pub(crate) platform: &'static str,
    pub(crate) desktop: &'static str,
    pub(crate) label: String,
    /// Linux: how to bind the quick switcher in the desktop (`None` on macOS).
    pub(crate) quick_switch_hint: Option<String>,
}

/// Mac or Linux mode, and the desktop: Settings hides the Mac pages on Linux.
#[tauri::command]
pub(crate) fn system_info() -> SystemView {
    let info = palette::system_info();
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "palette-app".into());
    SystemView {
        platform: info.platform,
        desktop: info.desktop,
        quick_switch_hint: palette::quick_switch_hint(info.desktop, &exe),
        label: info.label,
    }
}
