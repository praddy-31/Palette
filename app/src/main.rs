//! Palette's menu bar app: a tray icon with a theme picker, and a hotkey
//! quick-switch overlay. All theme logic lives in the `palette` library
//! (the same code the CLI runs); this file only turns it into windows,
//! menus and shortcuts.
//!
//! Ways in: tray click (picker), the hotkey (overlay), `palette://` links
//! from the SketchyBar item or `palette ui` (picker, overlay, next, prev),
//! and opening the app again from Finder (picker).

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use palette::theme::{Origin, Theme};
use palette::{Effect, Outcome, Paths};
use serde::Serialize;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, RunEvent, WebviewWindow, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_positioner::{Position, WindowExt};

/// The hotkey currently registered, so a change made with `palette hotkey`
/// in a terminal is picked up the next time a window opens.
struct Hotkey(Mutex<String>);

/// When a window was last shown. Bringing a menu bar app forward makes the
/// previous app give up focus a moment later, which can arrive as a "lost
/// focus" event on our brand-new window; ignore those for a short while.
struct Shown(Mutex<Instant>);

/// Passed by the login item, so a login start stays quiet.
const AUTOSTART_ARG: &str = "--autostart";

fn paths() -> Result<Paths, String> {
    Paths::from_env().map_err(|e| format!("{e:#}"))
}

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

// ---------------------------------------------------------------- commands

#[derive(Serialize)]
struct ThemeInfo {
    name: String,
    origin: &'static str,
    mode: &'static str,
    colors: BTreeMap<String, String>,
    ansi: Vec<String>,
    current: bool,
    wallpapers: usize,
    /// Set when the theme can't be loaded (bad colors.toml); shown greyed out.
    invalid: Option<String>,
    /// One of the user's favourites (★ in the picker).
    favourite: bool,
    /// Low-contrast pairs (`palette check`), one line each with the fix.
    contrast: Vec<String>,
}

/// `palette check` as lines for a badge's tooltip: "what: ratio … — fix".
fn contrast_lines(report: &palette::check::Report) -> Vec<String> {
    report
        .issues
        .iter()
        .map(|i| format!("{}. {}", i.summary(), i.fix()))
        .collect()
}

fn info(theme: &Theme, origin: &'static str, current: bool) -> ThemeInfo {
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
        contrast: contrast_lines(&palette::check::theme(theme)),
    }
}

/// Every theme, with its colours, for the picker and the overlay.
#[tauri::command]
fn themes() -> Result<Vec<ThemeInfo>, String> {
    let paths = paths()?;
    let current = palette::state::current_theme(&paths).map_err(err)?;
    let favourites = palette::favourites::list(&paths).unwrap_or_default();
    let mut out = Vec::new();
    for (name, origin) in palette::theme::list(&paths).map_err(err)? {
        let origin = match origin {
            Origin::Bundled => "bundled",
            Origin::User => "user",
        };
        let is_current = current.as_deref() == Some(name.as_str());
        match palette::theme::load(&paths, &name) {
            Ok(theme) => out.push(ThemeInfo {
                favourite: favourites.contains(&name),
                ..info(&theme, origin, is_current)
            }),
            Err(e) => out.push(ThemeInfo {
                favourite: favourites.contains(&name),
                name,
                origin,
                mode: "dark",
                colors: BTreeMap::new(),
                ansi: Vec::new(),
                current: is_current,
                wallpapers: 0,
                invalid: Some(err(e)),
                contrast: Vec::new(),
            }),
        }
    }
    Ok(out)
}

/// A small JPEG of the theme's preview or first wallpaper, as a data URL
/// (no file access from the web view needed). Made on first use, then cached.
#[tauri::command]
async fn thumbnail(name: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let theme = palette::theme::load(&paths, &name).map_err(err)?;
        let Some(file) = palette::thumbs::thumbnail(&paths, &theme).map_err(err)? else {
            return Ok(None);
        };
        let bytes = std::fs::read(file).map_err(err)?;
        Ok(Some(format!("data:image/jpeg;base64,{}", base64(&bytes))))
    })
    .await
    .map_err(err)?
}

#[derive(Serialize, Clone)]
struct Applied {
    theme: String,
    /// Apps or settings that failed to update, for a short notice.
    problems: Vec<String>,
}

fn applied(theme: String, effects: &[Effect]) -> Applied {
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
async fn apply(app: AppHandle, name: String, wallpaper: Option<String>) -> Result<Applied, String> {
    run_switch(app, Switch::To(name, wallpaper)).await
}

/// `next`, `prev` or `undo`.
#[tauri::command]
async fn step(app: AppHandle, how: String) -> Result<Applied, String> {
    let how = match how.as_str() {
        "next" => Switch::Next,
        "prev" => Switch::Prev,
        "undo" => Switch::Undo,
        other => return Err(format!("unknown step `{other}`")),
    };
    run_switch(app, how).await
}

enum Switch {
    /// A theme, optionally starting on one of its wallpapers.
    To(String, Option<String>),
    Next,
    Prev,
    Undo,
}

/// Runs off the main thread (reloading apps and setting the wallpaper take a
/// moment), then tells both windows which theme is active now.
async fn run_switch(app: AppHandle, how: Switch) -> Result<Applied, String> {
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
        Ok::<_, String>((applied(switch.theme, &switch.effects), false))
    })
    .await
    .map_err(err)??;
    let (result, undo) = result;
    let _ = app.emit("theme-changed", result.clone());
    announce(&app, &result, undo);
    Ok(result)
}

/// What the "Switched to X · Undo" toast shows.
#[derive(Serialize, Clone)]
struct Switched {
    theme: String,
    problems: Vec<String>,
    /// This was an undo: say "Back to X", no Undo button.
    undo: bool,
    /// The new theme's colours, so the toast is painted in them.
    colors: BTreeMap<String, String>,
}

/// After a switch from the app (picker, quick switcher, tray, bar pill): a
/// toast with Undo. The picker shows it itself when open; Settings has its
/// own messages; otherwise a small toast window appears at the top right
/// without taking focus from the app you're in, and hides after 5 s.
fn announce(app: &AppHandle, result: &Applied, undo: bool) {
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
    let _ = app.emit(
        "switched",
        Switched {
            theme: result.theme.clone(),
            problems: result.problems.clone(),
            undo,
            colors,
        },
    );
    let open = |label: &str, check: fn(&WebviewWindow) -> bool| {
        app.get_webview_window(label).is_some_and(|w| check(&w))
    };
    if open("picker", |w| w.is_visible().unwrap_or(false))
        || open("settings", |w| w.is_focused().unwrap_or(false))
    {
        return;
    }
    if let Some(toast) = app.get_webview_window("toast") {
        place_top_right(&toast);
        // Not `show()` above: that activates Palette and takes focus.
        let _ = toast.show();
    }
}

#[tauri::command]
fn get_hotkey(state: tauri::State<Hotkey>) -> String {
    state.0.lock().map(|h| h.clone()).unwrap_or_default()
}

/// Register a new hotkey and save it. The old one stays if the new one
/// can't be registered (another app owns it).
#[tauri::command]
fn set_hotkey(app: AppHandle, combo: String) -> Result<String, String> {
    let paths = paths()?;
    let combo = palette::settings::normalise_hotkey(&combo).map_err(err)?;
    register_hotkey(&app, &combo)?;
    let mut settings = palette::settings::load(&paths).map_err(err)?;
    settings.hotkey = combo.clone();
    palette::settings::save(&paths, &settings).map_err(err)?;
    Ok(combo)
}

#[tauri::command]
fn hide(window: WebviewWindow) {
    let _ = window.hide();
    // Settings gone: a menu bar app again (see show_settings).
    #[cfg(target_os = "macos")]
    if window.label() == "settings" {
        let _ = window
            .app_handle()
            .set_activation_policy(tauri::ActivationPolicy::Accessory);
    }
}

// ---------------------------------------------------------------- settings

#[derive(Serialize)]
struct AppRow {
    name: &'static str,
    on: bool,
    installed: bool,
    wired: bool,
    detail: String,
    /// On and installed but not hooked up yet (e.g. new in this version):
    /// Settings offers a "Set up" button.
    can_wire: bool,
}

/// Every app Palette can theme, plus wallpaper and light/dark.
#[tauri::command]
fn apps() -> Result<Vec<AppRow>, String> {
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
async fn set_app(name: String, on: bool) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let (_, effects) = palette::set_app(&paths, &name, on).map_err(err)?;
        Ok(applied(name, &effects).problems)
    })
    .await
    .map_err(err)?
}

#[derive(Serialize)]
struct Prefs {
    /// What next/prev and the bar pill step through: "all" or "favourites".
    next_cycles: &'static str,
    hotkey: String,
    login: bool,
    welcomed: bool,
    obsidian_ring: bool,
    obsidian_dim: bool,
}

#[tauri::command]
fn prefs(app: AppHandle) -> Result<Prefs, String> {
    let settings = palette::settings::load(&paths()?).map_err(err)?;
    Ok(Prefs {
        next_cycles: settings.next_cycles.as_str(),
        hotkey: settings.hotkey,
        login: app.autolaunch().is_enabled().unwrap_or(false),
        welcomed: settings.welcomed,
        obsidian_ring: settings.obsidian_ring,
        obsidian_dim: settings.obsidian_dim,
    })
}

/// Obsidian's active-pane ring and dimming (`palette obsidian`); `None` keeps
/// that one. Re-renders the theme, so Obsidian updates live.
#[tauri::command]
async fn set_obsidian_look(ring: Option<bool>, dim: Option<bool>) -> Result<(bool, bool), String> {
    tauri::async_runtime::spawn_blocking(move || {
        palette::obsidian_look(&paths()?, ring, dim).map_err(err)
    })
    .await
    .map_err(err)?
}

/// The welcome tour was finished or skipped (`false`: show it again next launch).
#[tauri::command]
fn set_welcomed(done: bool) -> Result<(), String> {
    let paths = paths()?;
    let mut settings = palette::settings::load(&paths).map_err(err)?;
    settings.welcomed = done;
    palette::settings::save(&paths, &settings).map_err(err)
}

#[tauri::command]
fn set_login(app: AppHandle, on: bool) -> Result<bool, String> {
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
fn route_login_item_through_macos() {
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
fn open_settings(app: AppHandle) {
    show_settings(&app);
}

// ---------------------------------------------------------------- get themes

#[derive(Serialize)]
struct CatalogItem {
    name: String,
    title: String,
    author: String,
    url: String,
    mode: &'static str,
    mood: &'static str,
    colors: BTreeMap<String, String>,
    installed: bool,
    /// Low-contrast pairs (`palette check`), for a badge.
    contrast: Vec<String>,
}

/// The curated community themes (`palette catalog`), offline.
#[tauri::command]
fn catalog() -> Result<Vec<CatalogItem>, String> {
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
        })
        .collect())
}

/// Themes installed with `palette install` that aren't in the catalogue (a link).
#[tauri::command]
fn installed_themes() -> Result<Vec<(String, String)>, String> {
    let paths = paths()?;
    Ok(palette::install::installed(&paths))
}

#[derive(Serialize)]
struct InstallResult {
    name: String,
    wallpapers: usize,
    filled: Vec<String>,
}

/// `palette install` (a catalogue name, a GitHub link) or, with `update`,
/// `palette update`. Downloads, so it runs off the main thread.
#[tauri::command]
async fn install_theme(
    app: AppHandle,
    source: String,
    name: Option<String>,
    update: Option<bool>,
) -> Result<InstallResult, String> {
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
fn remove_theme(app: AppHandle, name: String) -> Result<Removed, String> {
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
struct Removed {
    message: String,
    warning: bool,
}

// ---------------------------------------------------------------- wallpapers

#[derive(Serialize)]
struct Wallpaper {
    name: String,
    current: bool,
    /// The one this theme starts with.
    default: bool,
}

#[derive(Serialize)]
struct Wallpapers {
    theme: String,
    active: bool,
    images: Vec<Wallpaper>,
    /// The theme's wallpaper folder and its size, shown so copies don't pile up unseen.
    dir: String,
    bytes: u64,
}

fn theme_or_current(paths: &Paths, theme: Option<String>) -> Result<Theme, String> {
    match theme {
        Some(name) => palette::theme::load(paths, &name).map_err(err),
        None => palette::current(paths).map_err(err),
    }
}

/// A theme's wallpapers (default: the active theme), current one marked.
#[tauri::command]
fn wallpapers(theme: Option<String>) -> Result<Wallpapers, String> {
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
async fn wallpaper_thumb(theme: String, name: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let theme = palette::theme::load(&paths, &theme).map_err(err)?;
        let file = palette::effects::wallpaper_named(&theme, &name).map_err(err)?;
        let thumb = palette::thumbs::image(&paths, &file).map_err(err)?;
        let bytes = std::fs::read(thumb).map_err(err)?;
        Ok(format!("data:image/jpeg;base64,{}", base64(&bytes)))
    })
    .await
    .map_err(err)?
}

#[tauri::command]
async fn set_wallpaper(theme: String, name: String) -> Result<Vec<String>, String> {
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
async fn step_wallpaper(by: Option<isize>) -> Result<Vec<String>, String> {
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
fn set_default_wallpaper(theme: String, name: Option<String>) -> Result<(), String> {
    let paths = paths()?;
    let theme = palette::theme::load(&paths, &theme).map_err(err)?;
    palette::effects::set_default_wallpaper(&paths, &theme, name.as_deref()).map_err(err)
}

#[tauri::command]
fn remove_wallpaper(theme: String, name: String) -> Result<(), String> {
    let paths = paths()?;
    let theme = palette::theme::load(&paths, &theme).map_err(err)?;
    palette::effects::remove_wallpaper(&theme, &name).map_err(err)?;
    Ok(())
}

/// Pick images in a file dialog and copy them into a theme. The theme's
/// first wallpapers are used straight away if it's the active theme.
#[tauri::command]
async fn add_wallpapers(app: AppHandle, theme: String) -> Result<usize, String> {
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

// ---------------------------------------------------------------- wallpaper studio

const IMAGE_TYPES: [&str; 6] = ["png", "jpg", "jpeg", "heic", "webp", "tiff"];

/// Is gowall installed (offered as a second recolour engine)?
#[tauri::command]
fn has_gowall() -> bool {
    palette::studio::has_gowall()
}

/// Pick images and save a copy of each recoloured to `theme`
/// (`palette bg recolor`). Returns the new wallpapers' names; empty when the
/// dialog is cancelled. `engine`: "palette" (default) or "gowall";
/// `strength`: "subtle", "balanced" (default) or "strong" (Palette's engine).
#[tauri::command]
async fn recolour_wallpapers(
    app: AppHandle,
    theme: String,
    engine: Option<String>,
    strength: Option<String>,
) -> Result<Vec<String>, String> {
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
async fn generate_wallpaper(
    theme: String,
    style: String,
    seed: Option<u64>,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let theme = palette::theme::load(&paths, &theme).map_err(err)?;
        let made = palette::studio::generate(&theme, &style, seed).map_err(err)?;
        Ok(file_name(&made))
    })
    .await
    .map_err(err)?
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

#[derive(Serialize)]
struct Draft {
    /// The picked image (passed back to re-draft or save).
    path: String,
    /// A free theme name from the file's name, for the name field.
    name: String,
    light: bool,
    mood: &'static str,
    /// The picture's main colours.
    swatches: Vec<String>,
    /// The theme's main colours (background, foreground, accent, terminal colours).
    colors: BTreeMap<String, String>,
    /// A small JPEG of the picture, as a data URL.
    thumb: Option<String>,
}

/// A theme name from a file name: lowercase letters, digits and `-`, not yet taken.
fn suggest_name(paths: &Paths, image: &std::path::Path) -> String {
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
async fn draft_theme_from_wallpaper(
    app: AppHandle,
    path: Option<String>,
    light: Option<bool>,
) -> Result<Option<Draft>, String> {
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
fn draft_for(image: &std::path::Path, light: Option<bool>) -> Result<Option<Draft>, String> {
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
        .and_then(|t| Ok(std::fs::read(t)?))
        .ok()
        .map(|bytes| format!("data:image/jpeg;base64,{}", base64(&bytes)));
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
async fn save_theme_from_wallpaper(
    app: AppHandle,
    name: String,
    path: String,
    light: Option<bool>,
) -> Result<String, String> {
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
        Ok::<_, String>(name)
    })
    .await
    .map_err(err)??;
    let _ = app.emit("themes-changed", ());
    Ok(saved)
}

// ---------------------------------------------------------------- wallpaper library

#[derive(Serialize)]
struct LibraryItem {
    /// Path inside the library (`CozyPixels/Nord/lake.png`): what the other commands take.
    name: String,
    folder: String,
    bytes: u64,
}

#[derive(Serialize)]
struct LibraryInfo {
    dir: String,
    bytes: u64,
    items: Vec<LibraryItem>,
}

/// The wallpaper library (`palette bg library`): folder, total size, pictures.
#[tauri::command]
fn library() -> Result<LibraryInfo, String> {
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
async fn library_thumb(name: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let item = palette::library::find(&paths, &name).map_err(err)?;
        let thumb = palette::thumbs::image(&paths, &item.path).map_err(err)?;
        let bytes = std::fs::read(thumb).map_err(err)?;
        Ok(format!("data:image/jpeg;base64,{}", base64(&bytes)))
    })
    .await
    .map_err(err)?
}

/// Pick images and copy them into the library (`palette bg library add`).
#[tauri::command]
async fn library_add(app: AppHandle) -> Result<usize, String> {
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
fn library_remove(name: String) -> Result<(), String> {
    let paths = paths()?;
    palette::library::remove(&paths, &name).map_err(err)?;
    Ok(())
}

/// Use a library picture now (until the next theme switch).
#[tauri::command]
async fn library_use(name: String) -> Result<Vec<String>, String> {
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
async fn library_to_theme(name: String, theme: String, recolour: bool) -> Result<String, String> {
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
async fn draft_theme_from_library(
    name: String,
    light: Option<bool>,
) -> Result<Option<Draft>, String> {
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
fn reveal_folder(what: String, theme: Option<String>) -> Result<(), String> {
    let paths = paths()?;
    let dir = match what.as_str() {
        "library" => palette::library::dir(&paths),
        "theme" => theme_or_current(&paths, theme)?.backgrounds_dir(),
        _ => return Err(format!("unknown folder `{what}`")),
    };
    palette::library::reveal_folder(&dir).map_err(err)
}

#[derive(Serialize, serde::Deserialize)]
struct LinkFile {
    path: String,
    bytes: u64,
}

#[derive(Serialize)]
struct LinkFolder {
    path: String,
    count: usize,
    bytes: u64,
}

#[derive(Serialize)]
struct LinkListing {
    /// "image" (one picture) or "github" (a repo or folder).
    kind: &'static str,
    title: String,
    folders: Vec<LinkFolder>,
    files: Vec<LinkFile>,
    truncated: bool,
    /// Where downloads go unless changed (library subfolder).
    into: String,
    max_files: usize,
    max_bytes: u64,
}

/// What a pasted link offers (`palette bg get <url>` without a choice).
/// A GitHub link costs one request to GitHub's API.
#[tauri::command]
async fn wallpaper_link(url: String) -> Result<LinkListing, String> {
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
struct LinkDownload {
    saved: usize,
    bytes: u64,
    dir: String,
    skipped: Vec<(String, String)>,
    /// How many were also added to `theme`.
    added: usize,
}

/// Download the chosen pictures of a link into the library
/// (`palette bg get … --pick/--folder/--all`), optionally also into a theme.
#[tauri::command]
async fn wallpaper_link_download(
    app: AppHandle,
    url: String,
    files: Vec<LinkFile>,
    theme: Option<String>,
    recolour: Option<bool>,
) -> Result<LinkDownload, String> {
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
                    Err(e) => skipped.push((file_name(file), err(e))),
                }
            }
        }
        Ok::<_, String>(LinkDownload {
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

// ---------------------------------------------------------------- setup

#[derive(Serialize)]
struct SetupLine {
    app: &'static str,
    what: String,
    path: String,
}

#[derive(Serialize)]
struct SetupResult {
    lines: Vec<SetupLine>,
    backup: Option<String>,
    problems: Vec<String>,
}

/// "Set up all apps" (`palette init`) or "Remove from all configs"
/// (`palette init --remove`).
#[tauri::command]
async fn setup_all(remove: bool) -> Result<SetupResult, String> {
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
struct CheckRow {
    name: String,
    status: &'static str,
    detail: String,
    notes: Vec<(&'static str, String)>,
}

/// `palette doctor`, as rows for the Setup tab.
#[tauri::command]
async fn check() -> Result<Vec<CheckRow>, String> {
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
async fn refresh(app: AppHandle) -> Result<Applied, String> {
    let result = tauri::async_runtime::spawn_blocking(|| {
        let paths = paths()?;
        let s = palette::refresh(&paths).map_err(err)?;
        Ok::<_, String>(applied(s.theme, &s.effects))
    })
    .await
    .map_err(err)??;
    let _ = app.emit("theme-changed", result.clone());
    Ok(result)
}

// ---------------------------------------------------------------- tools

#[derive(Serialize)]
struct ToolRow {
    id: &'static str,
    name: &'static str,
    group: &'static str,
    what: &'static str,
    installed: bool,
    install: Vec<String>,
    remove: Vec<String>,
    /// none | available | has_config | added | added_edited
    starter: &'static str,
    config: Option<String>,
}

#[derive(Serialize)]
struct ToolsStatus {
    homebrew: bool,
    tools: Vec<ToolRow>,
}

/// `palette tools`: what is installed and the Homebrew commands.
#[tauri::command]
async fn tools_status() -> Result<ToolsStatus, String> {
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
async fn add_starter(tool: String) -> Result<String, String> {
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
async fn remove_starter(tool: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        palette::tools::remove_starter(&paths()?, &tool).map_err(err)?;
        Ok("Moved the starter config to the Trash".to_string())
    })
    .await
    .map_err(err)?
}

/// Copy a command to the clipboard (the Copy buttons).
#[tauri::command]
fn copy_text(text: String) -> Result<(), String> {
    palette::system::copy_to_clipboard(&text)
}

/// "Open Terminal": the person pastes the Homebrew command there.
#[tauri::command]
fn open_terminal() -> Result<(), String> {
    palette::tools::open_terminal().map_err(err)
}

/// Opens brew.sh (the only address this opens).
#[tauri::command]
fn open_homebrew_site() -> Result<(), String> {
    palette::tools::open_homebrew_site().map_err(err)
}

// ---------------------------------------------------------------- import

#[derive(Serialize)]
struct ImportPlan {
    dir: String,
    files: usize,
    replaced: Vec<String>,
    theme: Option<String>,
    apps: Vec<&'static str>,
}

/// Pick an export folder and describe what importing it would change
/// (a dry run). Nothing is written until `import_apply`.
#[tauri::command]
async fn import_dialog(app: AppHandle) -> Result<Option<ImportPlan>, String> {
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
        }))
    })
    .await
    .map_err(err)?
}

#[tauri::command]
async fn import_apply(app: AppHandle, dir: String) -> Result<String, String> {
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
        Ok::<_, String>(msg)
    })
    .await
    .map_err(err)??;
    let _ = app.emit("theme-changed", ());
    Ok(msg)
}

/// Open Settings on one tab (picker's Wallpapers button, say).
#[tauri::command]
fn open_settings_tab(app: AppHandle, tab: String) {
    show_settings(&app);
    let _ = app.emit("open-tab", tab);
}

// ---------------------------------------------------------------- export

#[derive(Serialize)]
struct SyncPrefs {
    wallpapers: bool,
    configs: Vec<String>,
}

/// What an export takes by default (sync.toml, created on first use).
#[tauri::command]
fn sync_prefs() -> Result<SyncPrefs, String> {
    let (sync, _) = palette::sync::load_or_create(&paths()?).map_err(err)?;
    Ok(SyncPrefs {
        wallpapers: sync.wallpapers,
        configs: sync.configs,
    })
}

/// Ask for a folder, export into `<folder>/Palette Export`, show it in
/// Finder. `None` when the user cancels the folder dialog.
#[tauri::command]
async fn export_dialog(
    app: AppHandle,
    wallpapers: bool,
    configs: bool,
) -> Result<Option<String>, String> {
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

// ---------------------------------------------------------------- command-line tool

/// The `palette` CLI shipped inside the app (`Contents/MacOS/palette-cli`;
/// the terminal command is still `palette`, a link to it).
fn bundled_cli() -> Option<std::path::PathBuf> {
    let cli = std::env::current_exe().ok()?.with_file_name("palette-cli");
    cli.is_file().then_some(cli)
}

/// Apps before 1.1 shipped the CLI as `Contents/MacOS/palette`: point a
/// `~/.local/bin/palette` link made then at the new name. Only links into
/// an app bundle are touched.
fn relink_old_cli() {
    let (Some(cli), Ok(link)) = (bundled_cli(), cli_link()) else {
        return;
    };
    let Ok(target) = std::fs::read_link(&link) else {
        return;
    };
    let old = target.file_name().is_some_and(|n| n == "palette")
        && target.to_string_lossy().contains(".app/Contents/MacOS/");
    if old && std::fs::remove_file(&link).is_ok() {
        let _ = std::os::unix::fs::symlink(&cli, &link);
    }
}

/// `~/.local/bin`: on the default PATH of most shell setups, no admin rights.
fn cli_link() -> Result<std::path::PathBuf, String> {
    Ok(paths()?.home().join(".local/bin/palette"))
}

#[derive(Serialize)]
struct CliStatus {
    /// The app carries the CLI.
    bundled: bool,
    /// `~/.local/bin/palette` points at the app's copy.
    linked: bool,
    /// Where a `palette` command is found otherwise (e.g. ~/.cargo/bin).
    other: Option<String>,
}

#[tauri::command]
fn cli_status() -> Result<CliStatus, String> {
    let link = cli_link()?;
    let bundled = bundled_cli();
    let linked = bundled
        .as_ref()
        .is_some_and(|b| std::fs::read_link(&link).is_ok_and(|t| &t == b));
    let home = paths()?.home().to_path_buf();
    let mut candidates = vec![home.join(".cargo/bin/palette"), link.clone()];
    candidates.extend(palette::system::find_all_programs("palette"));
    let other = candidates
        .into_iter()
        .find(|p| p.is_file() && !(linked && *p == link))
        .map(|p| p.display().to_string());
    Ok(CliStatus {
        bundled: bundled.is_some(),
        linked,
        other,
    })
}

/// Link `~/.local/bin/palette` to the app's copy of the CLI. Never replaces
/// a `palette` that isn't such a link (a `cargo install`ed one, say).
#[tauri::command]
fn install_cli() -> Result<String, String> {
    let cli = bundled_cli().ok_or("this build of the app doesn't include the CLI")?;
    let link = cli_link()?;
    if let Ok(target) = std::fs::read_link(&link) {
        if target == cli {
            return Ok(format!("{} already links to the app", link.display()));
        }
        std::fs::remove_file(&link).map_err(err)?; // an old link (app moved): replace it
    } else if link.exists() {
        return Err(format!(
            "{} already exists and isn't a link; remove it first if you want the app's copy",
            link.display()
        ));
    }
    if let Some(dir) = link.parent() {
        std::fs::create_dir_all(dir).map_err(err)?;
    }
    std::os::unix::fs::symlink(&cli, &link).map_err(err)?;
    Ok(format!(
        "Installed: {} → the app. Open a new terminal and run `palette`.",
        link.display()
    ))
}

// ---------------------------------------------------------------- hotkey

fn register_hotkey(app: &AppHandle, combo: &str) -> Result<(), String> {
    let shortcuts = app.global_shortcut();
    let state = app.state::<Hotkey>();
    let mut current = state.0.lock().map_err(err)?;
    if !current.is_empty() {
        let _ = shortcuts.unregister(current.as_str());
    }
    match shortcuts.register(combo) {
        Ok(()) => {
            *current = combo.to_string();
            Ok(())
        }
        Err(e) => {
            // Put the old one back so the overlay stays reachable.
            if !current.is_empty() {
                let _ = shortcuts.register(current.as_str());
            }
            Err(format!("can't use {combo}: {e} (another app may own it)"))
        }
    }
}

/// Re-read settings.toml: `palette hotkey <combo>` may have changed it.
fn sync_hotkey(app: &AppHandle) {
    let Ok(settings) = paths().and_then(|p| palette::settings::load(&p).map_err(err)) else {
        return;
    };
    let registered = app
        .state::<Hotkey>()
        .0
        .lock()
        .map(|h| h.clone())
        .unwrap_or_default();
    if settings.hotkey != registered {
        let _ = register_hotkey(app, &settings.hotkey);
    }
}

// ---------------------------------------------------------------- favourites, contrast

/// ★ a theme or not (`palette fav add/remove`). Returns the favourites now.
#[tauri::command]
fn set_favourite(name: String, on: bool) -> Result<Vec<String>, String> {
    palette::favourites::set(&paths()?, &name, on).map_err(err)
}

/// The last themes switched to, newest first (`palette recent`).
#[tauri::command]
fn recents() -> Result<Vec<String>, String> {
    Ok(palette::favourites::recents(&paths()?))
}

/// "all" or "favourites": what next/prev and the bar pill step through.
#[tauri::command]
fn set_next_cycles(cycle: String) -> Result<(), String> {
    let cycle = palette::favourites::Cycle::parse(&cycle).map_err(err)?;
    palette::favourites::set_cycle(&paths()?, cycle).map_err(err)
}

/// `palette check <theme>`: the verdict and one line per low pair.
#[tauri::command]
fn contrast_report(name: String) -> Result<(String, Vec<String>), String> {
    let report = palette::check::run(&paths()?, Some(&name)).map_err(err)?;
    Ok((report.verdict(), contrast_lines(&report)))
}

// ---------------------------------------------------------------- colours

#[derive(Serialize)]
struct SwatchOut {
    key: &'static str,
    value: String,
    original: String,
}

#[derive(Serialize)]
struct IssueOut {
    fg: &'static str,
    bg: &'static str,
    fg_hex: String,
    bg_hex: String,
    ratio: f32,
    min: f32,
    used_for: &'static str,
    applied: Option<&'static str>,
    suggested: String,
}

#[derive(Serialize)]
struct SheetOut {
    theme: String,
    mode: &'static str,
    colours: Vec<SwatchOut>,
    verdict: String,
    issues: Vec<IssueOut>,
}

/// `palette colours <theme>`: every colour (yours and the original) and the
/// contrast report, for Settings → Colours.
#[tauri::command]
fn colour_sheet(name: String) -> Result<SheetOut, String> {
    let sheet = palette::edits::sheet(&paths()?, &name).map_err(err)?;
    Ok(SheetOut {
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
async fn edit_colours(
    app: AppHandle,
    name: String,
    how: String,
    colours: Option<Vec<(String, String)>>,
    key: Option<String>,
) -> Result<(Vec<(String, String)>, Vec<String>), String> {
    use palette::edits::Edit;
    let edit = match how.as_str() {
        "set" => Edit::Set(colours.unwrap_or_default()),
        "reset" => Edit::Reset(key),
        "fix" => Edit::FixContrast,
        other => return Err(format!("unknown colour edit `{other}`")),
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

// ---------------------------------------------------------------- desktop

#[derive(Serialize)]
struct DesktopStatus {
    tool: &'static str,
    app: &'static str,
    installed: bool,
    managed: bool,
    files: Vec<String>,
    original: Option<String>,
    backups: usize,
    /// When each backup was made (folder names), newest first.
    history: Vec<String>,
}

#[derive(Serialize)]
struct DesktopApplied {
    written: Vec<String>,
    backup: Option<String>,
    /// "reloaded", "not running", or why it failed.
    reload: String,
    failed: bool,
    warnings: Vec<String>,
}

fn tool(name: &str) -> Result<palette::desktop::Tool, String> {
    palette::desktop::Tool::parse(name).map_err(err)
}

fn desktop_applied(a: palette::desktop::Applied) -> DesktopApplied {
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
fn desktop_status(name: String) -> Result<DesktopStatus, String> {
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
fn desktop_get(name: String) -> Result<serde_json::Value, String> {
    let value = palette::desktop::get(&paths()?, tool(&name)?).map_err(err)?;
    serde_json::to_value(value).map_err(err)
}

/// Save settings and write the config (backup first), then reload the tool.
#[tauri::command]
async fn desktop_set(name: String, settings: serde_json::Value) -> Result<DesktopApplied, String> {
    let t = tool(&name)?;
    let value = toml::Value::try_from(settings).map_err(err)?;
    tauri::async_runtime::spawn_blocking(move || {
        palette::desktop::set(&paths()?, t, value)
            .map(desktop_applied)
            .map_err(err)
    })
    .await
    .map_err(err)?
}

/// "Let Palette manage it" (`manage`) or "Give it back" (`release`).
#[tauri::command]
async fn desktop_own(name: String, manage: bool) -> Result<DesktopApplied, String> {
    let t = tool(&name)?;
    tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let done = if manage {
            palette::desktop::manage(&paths, t)
        } else {
            palette::desktop::release(&paths, t)
        };
        done.map(desktop_applied).map_err(err)
    })
    .await
    .map_err(err)?
}

/// Go back to a backup, by its name in `history`.
#[tauri::command]
async fn desktop_restore(name: String, backup: String) -> Result<DesktopApplied, String> {
    let t = tool(&name)?;
    tauri::async_runtime::spawn_blocking(move || {
        palette::desktop::restore(&paths()?, t, &backup)
            .map(desktop_applied)
            .map_err(err)
    })
    .await
    .map_err(err)?
}

/// Installed font families, for the bar's font.
#[tauri::command]
async fn font_families() -> Vec<String> {
    tauri::async_runtime::spawn_blocking(palette::system::fonts::families)
        .await
        .unwrap_or_default()
}

/// The styles a font really has (a missing one shows no icons).
#[tauri::command]
fn font_styles(family: String) -> Vec<String> {
    palette::system::fonts::styles(&family)
}

/// Dock and menu bar, as macOS has them now.
#[tauri::command]
fn macos_get() -> palette::desktop::macos::MacBits {
    palette::desktop::macos::read()
}

#[tauri::command]
async fn macos_set(settings: palette::desktop::macos::MacBits) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        match palette::desktop::macos::set(&paths()?, &settings).map_err(err)? {
            Outcome::Failed(e) => Err(e),
            Outcome::Done(m) | Outcome::Unchanged(m) | Outcome::Skipped(m) => Ok(m),
        }
    })
    .await
    .map_err(err)?
}

/// The prompt's modules and the options the page shows for each.
#[tauri::command]
fn prompt_meta() -> serde_json::Value {
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
async fn prompt_preview(
    settings: serde_json::Value,
) -> Result<Vec<palette::desktop::prompt::Span>, String> {
    let value = toml::Value::try_from(settings).map_err(err)?;
    let p: palette::desktop::prompt::Prompt = value.try_into().map_err(err)?;
    tauri::async_runtime::spawn_blocking(move || {
        palette::desktop::prompt::preview(&paths()?, &p).map_err(err)
    })
    .await
    .map_err(err)?
}

/// Starship's presets.
#[tauri::command]
async fn prompt_presets() -> Vec<String> {
    tauri::async_runtime::spawn_blocking(palette::desktop::prompt::presets)
        .await
        .unwrap_or_default()
}

/// Use a preset as your prompt (your file is backed up first).
#[tauri::command]
async fn prompt_preset(name: String) -> Result<DesktopApplied, String> {
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
fn aerospace_commands() -> &'static [(&'static str, &'static str)] {
    palette::desktop::windows::COMMANDS
}

/// What can go in a bar pill.
#[tauri::command]
fn bar_kinds() -> &'static [palette::desktop::bar::Kind] {
    palette::desktop::bar::KINDS
}

/// Running apps (bundle id, name), for window rules and borders.
#[tauri::command]
async fn running_apps() -> Vec<(String, String)> {
    tauri::async_runtime::spawn_blocking(palette::desktop::windows::running_apps)
        .await
        .unwrap_or_default()
}

// ---------------------------------------------------------------- windows

/// A menu bar app (LSUIElement) is never "active" on its own: activate it
/// first, or the window appears behind the current app, or not at all.
fn show(window: &WebviewWindow) {
    let app = window.app_handle();
    if let Ok(mut at) = app.state::<Shown>().0.lock() {
        *at = Instant::now();
    }
    #[cfg(target_os = "macos")]
    let _ = app.show();
    let _ = window.show();
    let _ = window.set_focus();
}

/// While Settings is open Palette is a normal app (a Dock icon, its own
/// menu bar). A menu-bar-only app with a normal window open on another
/// AeroSpace workspace was brought forward by macOS whenever the hidden
/// menu bar slid down, and AeroSpace followed it to that workspace.
fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        #[cfg(target_os = "macos")]
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
        show(&window);
    }
}

/// The picker opens under the tray icon when clicked there; otherwise (bar
/// item, `palette ui`) at the top right, just below a SketchyBar-height bar.
fn show_picker(app: &AppHandle, from_tray: bool) {
    sync_hotkey(app);
    let Some(window) = app.get_webview_window("picker") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }
    let placed = from_tray && window.move_window(Position::TrayBottomCenter).is_ok();
    if !placed {
        place_top_right(&window);
    }
    show(&window);
}

fn place_top_right(window: &WebviewWindow) {
    let Ok(Some(monitor)) = window.primary_monitor() else {
        return;
    };
    let scale = monitor.scale_factor();
    let origin = monitor.position().to_logical::<f64>(scale);
    let screen = monitor.size().to_logical::<f64>(scale);
    let width = window
        .outer_size()
        .map(|s| s.to_logical::<f64>(scale).width)
        .unwrap_or(440.0);
    let _ = window.set_position(LogicalPosition::new(
        origin.x + screen.width - width - 12.0,
        origin.y + 48.0,
    ));
}

fn toggle_overlay(app: &AppHandle) {
    sync_hotkey(app);
    let Some(window) = app.get_webview_window("overlay") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
    } else {
        let _ = window.center();
        show(&window);
    }
}

/// `palette://picker`, `palette://overlay`, `palette://next`, `palette://prev`.
fn open_url(app: &AppHandle, url: &str) {
    // macOS re-reads "menu bar app" (LSUIElement) when it hands Palette a
    // link, which drops the Dock icon Settings has while open: put it back.
    #[cfg(target_os = "macos")]
    if app
        .get_webview_window("settings")
        .is_some_and(|w| w.is_visible().unwrap_or(false))
    {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            let again = app.clone();
            let _ = app.run_on_main_thread(move || {
                let _ = again.set_activation_policy(tauri::ActivationPolicy::Regular);
            });
        });
    }
    let what = url
        .strip_prefix("palette://")
        .unwrap_or("")
        .trim_end_matches('/');
    match what {
        "picker" | "" => show_picker(app, false),
        "overlay" => toggle_overlay(app),
        "next" | "prev" => {
            let app = app.clone();
            let how = what.to_string();
            tauri::async_runtime::spawn(async move {
                let _ = step(app, how).await;
            });
        }
        _ => {}
    }
}

// ---------------------------------------------------------------- app

fn main() {
    tauri::Builder::default()
        // One Palette at a time (first plugin, so a second copy quits before
        // anything else starts). macOS knows several copies of the app (build
        // folders, disk images), so a link can occasionally start a second one.
        // On macOS that copy quits before macOS hands it the link, so the link
        // is lost; the running copy shows the picker. The login item starts
        // Palette through LaunchServices (route_login_item_through_macos), so
        // macOS knows it's open and links normally go straight to it.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            let link = argv.iter().any(|a| a.starts_with("palette://"));
            let login = argv.iter().any(|a| a == AUTOSTART_ARG);
            if !link && !login {
                show_picker(app, false);
            }
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle_overlay(app);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![AUTOSTART_ARG]),
        ))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_positioner::init())
        .manage(Hotkey(Mutex::new(String::new())))
        .manage(Shown(Mutex::new(Instant::now())))
        .invoke_handler(tauri::generate_handler![
            themes,
            thumbnail,
            apply,
            step,
            get_hotkey,
            set_hotkey,
            hide,
            apps,
            set_app,
            prefs,
            set_welcomed,
            set_obsidian_look,
            set_login,
            open_settings,
            cli_status,
            install_cli,
            sync_prefs,
            export_dialog,
            wallpapers,
            wallpaper_thumb,
            set_wallpaper,
            remove_wallpaper,
            set_default_wallpaper,
            catalog,
            installed_themes,
            install_theme,
            remove_theme,
            step_wallpaper,
            add_wallpapers,
            has_gowall,
            recolour_wallpapers,
            generate_wallpaper,
            draft_theme_from_wallpaper,
            save_theme_from_wallpaper,
            setup_all,
            check,
            refresh,
            colour_sheet,
            edit_colours,
            desktop_status,
            desktop_get,
            desktop_set,
            desktop_own,
            desktop_restore,
            font_families,
            font_styles,
            macos_get,
            macos_set,
            aerospace_commands,
            prompt_meta,
            prompt_preview,
            prompt_presets,
            prompt_preset,
            bar_kinds,
            running_apps,
            import_dialog,
            import_apply,
            open_settings_tab,
            schedule_get,
            schedule_save,
            schedule_cities,
            tools_status,
            add_starter,
            remove_starter,
            copy_text,
            open_terminal,
            open_homebrew_site,
            library,
            library_thumb,
            library_add,
            library_remove,
            library_use,
            library_to_theme,
            draft_theme_from_library,
            reveal_folder,
            wallpaper_link,
            wallpaper_link_download,
            set_favourite,
            recents,
            set_next_cycles,
            contrast_report
        ])
        .setup(|app| {
            // A menu bar app, never in the Dock. Info.plist says so (LSUIElement),
            // which also keeps macOS from turning it into a Dock app when it hands
            // over a palette:// link. Tell the event loop too, so tao's launch-time
            // default (a Dock app) doesn't win.
            let handle = app.handle().clone();
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            route_login_item_through_macos();
            relink_old_cli();
            let hotkey = paths()
                .and_then(|p| palette::settings::load(&p).map_err(err))
                .map(|s| s.hotkey)
                .unwrap_or_else(|_| palette::settings::DEFAULT_HOTKEY.into());
            if let Err(e) = register_hotkey(&handle, &hotkey) {
                eprintln!("palette: {e}");
            }

            // Settings is a normal window: closing it only hides it, and
            // Palette is a menu bar app again (see show_settings).
            if let Some(window) = app.get_webview_window("settings") {
                let w = window.clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = w.hide();
                        #[cfg(target_os = "macos")]
                        let _ = w
                            .app_handle()
                            .set_activation_policy(tauri::ActivationPolicy::Accessory);
                    }
                });
            }

            // Hide when focus moves elsewhere, like a menu.
            for label in ["picker", "overlay"] {
                if let Some(window) = app.get_webview_window(label) {
                    let w = window.clone();
                    window.on_window_event(move |event| {
                        if let WindowEvent::Focused(false) = event {
                            let just_shown = w
                                .app_handle()
                                .state::<Shown>()
                                .0
                                .lock()
                                .is_ok_and(|at| at.elapsed() < Duration::from_millis(500));
                            if !just_shown {
                                let _ = w.hide();
                            }
                        }
                    });
                }
            }

            // Tray icon and its menu.
            let open = MenuItem::with_id(app, "open", "Open Picker", true, None::<&str>)?;
            let overlay = MenuItem::with_id(app, "overlay", "Quick Switch", true, None::<&str>)?;
            let next = MenuItem::with_id(app, "next", "Next Theme", true, None::<&str>)?;
            let undo = MenuItem::with_id(app, "undo", "Undo", true, None::<&str>)?;
            let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit Palette", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &open,
                    &overlay,
                    &next,
                    &undo,
                    &PredefinedMenuItem::separator(app)?,
                    &settings,
                    &PredefinedMenuItem::separator(app)?,
                    &quit,
                ],
            )?;
            TrayIconBuilder::with_id("palette")
                .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
                .icon_as_template(true)
                .tooltip("Palette")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "open" => show_picker(app, false),
                    "settings" => show_settings(app),
                    "overlay" => toggle_overlay(app),
                    "next" | "undo" => {
                        let app = app.clone();
                        let how = event.id().as_ref().to_string();
                        tauri::async_runtime::spawn(async move {
                            let _ = step(app, how).await;
                        });
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_picker(tray.app_handle(), true);
                    }
                })
                .build(app)?;

            // Day/night schedule: check at launch, then just after each minute
            // starts, so a fixed time of 14:05 switches at 14:05:01 rather than
            // up to a minute late. Waking from sleep is covered by the next tick
            // (the sleep timer pauses with the Mac), so no wake notification is
            // needed. A check only reads the settings and the clock unless a
            // boundary has passed.
            let ticker = handle.clone();
            std::thread::spawn(move || {
                loop {
                    schedule_tick(&ticker);
                    std::thread::sleep(until_next_minute());
                }
            });

            // palette:// links, including the one that launched the app.
            let links = handle.clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    open_url(&links, url.as_str());
                }
            });
            match app.deep_link().get_current() {
                Ok(Some(urls)) if !urls.is_empty() => {
                    for url in urls {
                        open_url(&handle, url.as_str());
                    }
                }
                // Opened by hand (Finder, Spotlight): show the picker, so it's
                // obvious the app is running even if its menu bar icon is
                // hidden (auto-hiding menu bar, or behind the notch).
                // First launch: the welcome tour (in Settings) instead.
                _ if !std::env::args().any(|a| a == AUTOSTART_ARG) => {
                    let welcomed = paths()
                        .and_then(|p| palette::settings::load(&p).map_err(err))
                        .is_ok_and(|s| s.welcomed);
                    if welcomed {
                        show_picker(&handle, false);
                    } else {
                        show_settings(&handle);
                    }
                }
                _ => {}
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Palette failed to start")
        .run(|app, event| {
            // Opening the app again (Finder, Spotlight, `open -a Palette`) shows the picker.
            if let RunEvent::Reopen { .. } = event {
                show_picker(app, false);
            }
        });
}

// ---------------------------------------------------------------- schedule

#[derive(Serialize)]
struct ScheduleView {
    on: bool,
    day: String,
    night: String,
    /// "sun" or "fixed".
    mode: String,
    city: Option<String>,
    lat: Option<f64>,
    lon: Option<f64>,
    day_at: String,
    night_at: String,
    /// "Singapore, Singapore", or typed coordinates.
    place: Option<String>,
    /// City suggested by the Mac's time zone.
    suggestion: Option<String>,
    /// "Sunrise 06:48, sunset 18:53" / "Day from 07:00, night from 19:00".
    today: Option<String>,
    phase: Option<String>,
    /// "18:53 today, to Tokyo Night".
    next: Option<String>,
    problem: Option<String>,
}

fn schedule_view(paths: &Paths) -> Result<ScheduleView, String> {
    use palette::schedule as sch;
    let st = sch::which_now(paths).map_err(err)?;
    let s = &st.schedule;
    let sun = s.mode == sch::Mode::Sun;
    let today = st
        .now
        .as_ref()
        .and_then(|n| match (n.polar, n.day_starts, n.night_starts) {
            (Some(sch::Phase::Day), ..) => Some("Polar day: the sun doesn't set today".to_string()),
            (Some(sch::Phase::Night), ..) => {
                Some("Polar night: the sun doesn't rise today".to_string())
            }
            (None, Some(d), Some(nt)) => Some(if sun {
                format!("Sunrise {}, sunset {}", st.hm(d), st.hm(nt))
            } else {
                format!("Day from {}, night from {}", st.hm(d), st.hm(nt))
            }),
            _ => None,
        });
    let next = st
        .now
        .as_ref()
        .and_then(|n| n.next)
        .map(|(t, to)| format!("{} {}, to {}", st.hm(t), st.day_word(t), s.theme_for(to)));
    Ok(ScheduleView {
        on: s.on,
        day: s.day.clone(),
        night: s.night.clone(),
        mode: s.mode.as_str().into(),
        city: s.city.clone(),
        lat: s.lat,
        lon: s.lon,
        day_at: sch::fmt_hm(s.day_at),
        night_at: sch::fmt_hm(s.night_at),
        place: st.place.clone(),
        suggestion: sch::suggest_city().map(|c| c.name.to_string()),
        today,
        phase: st.now.as_ref().map(|n| n.phase.as_str().to_string()),
        next,
        problem: st.problem.clone(),
    })
}

/// The saved schedule with today's times and the next change.
#[tauri::command]
fn schedule_get() -> Result<ScheduleView, String> {
    schedule_view(&paths()?)
}

/// What the Settings pane sends back: the whole schedule.
#[derive(serde::Deserialize)]
struct ScheduleInput {
    on: bool,
    day: String,
    night: String,
    mode: String,
    city: Option<String>,
    lat: Option<f64>,
    lon: Option<f64>,
    day_at: String,
    night_at: String,
}

/// Save the schedule (`palette schedule on/off`). If it's on, switches at
/// once when the other theme is due, through the same path as `palette set`.
#[tauri::command]
async fn schedule_save(app: AppHandle, input: ScheduleInput) -> Result<ScheduleView, String> {
    use palette::schedule as sch;
    let (view, switched) = tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let new = sch::Schedule {
            on: input.on,
            day: input.day,
            night: input.night,
            mode: if input.mode == "fixed" {
                sch::Mode::Fixed
            } else {
                sch::Mode::Sun
            },
            city: input.city.filter(|c| !c.is_empty()),
            lat: input.lat,
            lon: input.lon,
            day_at: sch::parse_hm(&input.day_at).map_err(err)?,
            night_at: sch::parse_hm(&input.night_at).map_err(err)?,
        };
        let result = sch::save(&paths, new).map_err(err)?;
        Ok::<_, String>((schedule_view(&paths)?, schedule_applied(result)))
    })
    .await
    .map_err(err)??;
    if let Some(a) = switched {
        let _ = app.emit("theme-changed", a);
    }
    Ok(view)
}

fn schedule_applied(result: palette::schedule::Applied) -> Option<Applied> {
    match result {
        palette::schedule::Applied::Switched { switch, .. } => {
            Some(applied(switch.theme, &switch.effects))
        }
        _ => None,
    }
}

#[derive(Serialize)]
struct CityRow {
    name: &'static str,
    country: &'static str,
}

/// Built-in cities matching a search (name or country); empty: all of them.
#[tauri::command]
fn schedule_cities(query: String) -> Vec<CityRow> {
    palette::schedule::search_cities(&query)
        .into_iter()
        .map(|c| CityRow {
            name: c.name,
            country: c.country,
        })
        .collect()
}

/// Time until one second past the next whole minute of the clock.
fn until_next_minute() -> Duration {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    Duration::from_secs(60 - secs % 60 + 1)
}

/// One check of the schedule (`palette schedule run`): the app does this at
/// launch and every minute.
fn schedule_tick(app: &AppHandle) {
    let Ok(paths) = paths() else { return };
    match palette::schedule::apply_now(&paths) {
        Ok(result) => {
            if let Some(a) = schedule_applied(result) {
                let _ = app.emit("theme-changed", a);
            }
        }
        Err(e) => eprintln!("palette: schedule: {e:#}"),
    }
}

/// Standard base64, for data URLs. Twenty lines instead of a dependency.
fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::base64;

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
