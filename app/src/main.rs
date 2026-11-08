//! Palette's menu bar app: a tray icon with a theme picker, and a hotkey
//! quick-switch overlay. All theme logic lives in the `palette` library
//! (the same code the CLI runs); this file only turns it into windows,
//! menus and shortcuts.
//!
//! Ways in: tray click (picker), the hotkey (overlay), `palette://` links
//! from the SketchyBar item or `palette ui` (picker, overlay, next, prev),
//! and opening the app again from Finder (picker).
//!
//! This file holds the shared state, the error type the pages get, and the app's
//! setup (`main`). The commands live in one module per window or Settings tab.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use palette::theme::{Origin, Theme};
use palette::{Effect, Outcome, Paths};
use serde::Serialize;
use tauri::RunEvent;

use tauri::{
    AppHandle, Emitter, LogicalPosition, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_positioner::{Position, WindowExt};

mod catalog;
mod cli;
mod colours;
mod desktop;
mod library;
mod prefs;
mod schedule;
mod setup;
mod sync;
mod themes;
mod tools;
mod tray;
mod wallpapers;
mod windows;

use catalog::*;
use cli::*;
use colours::*;
use desktop::*;
use library::*;
use prefs::*;
use schedule::*;
use setup::*;
use sync::*;
use themes::*;
use tools::*;
use wallpapers::*;
use windows::*;

/// The hotkey currently registered, so a change made with `palette hotkey`
/// in a terminal is picked up the next time a window opens.
struct Hotkey(Mutex<String>);

/// When a window was last shown. Bringing a menu bar app forward makes the
/// previous app give up focus a moment later, which can arrive as a "lost
/// focus" event on our brand-new window; ignore those for a short while.
struct Shown(Mutex<Instant>);

/// The last switch, for a toast window made just now: it misses the `switched` event
/// sent while its page was still loading, so it asks for this when it starts.
struct LastSwitch(Mutex<Option<Switched>>);

/// The tab `open_settings_tab` asked for, for a Settings window made just now (same reason).
struct PendingTab(Mutex<Option<String>>);

/// Windows made from code (not `tauri.conf.json`), so with windows on demand they exist only
/// while needed. The quick switcher is in `tauri.conf.json`: it always stays loaded.
const ON_DEMAND: [&str; 3] = ["picker", "settings", "toast"];

/// Every window, for "all on demand" (the quick switcher too).
const ALL_WINDOWS: [&str; 4] = ["overlay", "picker", "settings", "toast"];

/// Passed by the login item, so a login start stays quiet.
const AUTOSTART_ARG: &str = "--autostart";
/// `palette-app --quick` / `--settings`: open the quick switcher or Settings.
/// On Linux the desktop's own shortcut runs these (Wayland doesn't let apps
/// grab keys); a second launch hands them to the running app.
const QUICK_ARG: &str = "--quick";
const SETTINGS_ARG: &str = "--settings";

/// The tray icon. macOS recolours the black "template" image for the menu
/// bar; Linux trays show it as it is, so there it's light with a dark edge,
/// readable on dark and light bars.
#[cfg(target_os = "macos")]
const TRAY_ICON: &[u8] = include_bytes!("../icons/tray.png");
#[cfg(not(target_os = "macos"))]
const TRAY_ICON: &[u8] = include_bytes!("../icons/tray-linux.png");

fn paths() -> Result<Paths, AppError> {
    Paths::from_env().map_err(err)
}

/// What a command's failure looks like to the page: the message, and for the causes the
/// app acts on (`palette::problem::Problem`) a code and what it's about. In JS it arrives as
/// a `Problem` (shared.js), which still prints as just the message.
#[derive(Debug, Serialize)]
struct AppError {
    message: String,
    code: Option<&'static str>,
    subject: Option<String>,
}

impl From<String> for AppError {
    fn from(message: String) -> Self {
        Self {
            message,
            code: None,
            subject: None,
        }
    }
}

impl From<&str> for AppError {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// Any error as an [`AppError`]; an `anyhow` error keeps its `Problem`, if it has one.
fn err<E: std::fmt::Display + 'static>(e: E) -> AppError {
    let any: &dyn std::any::Any = &e;
    let problem = any
        .downcast_ref::<palette::problem::Error>()
        .and_then(palette::problem::of);
    AppError {
        message: format!("{e:#}"),
        code: problem.map(|p| p.code()),
        subject: problem.and_then(|p| p.subject().map(String::from)),
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
            let quick = argv.iter().any(|a| a == QUICK_ARG);
            let settings = argv.iter().any(|a| a == SETTINGS_ARG);
            // May arrive on the plugin's own thread: windows only on the main one (`on_main`).
            on_main(app, move |app| {
                if quick {
                    toggle_overlay(app);
                } else if settings {
                    show_settings(app);
                } else if !link && !login {
                    show_picker(app, false);
                }
            });
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        on_main(app, toggle_overlay);
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
        .manage(LastSwitch(Mutex::new(None)))
        .manage(PendingTab(Mutex::new(None)))
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
            last_switch,
            pending_tab,
            stamp,
            bar_match_menu_bar,
            set_app_prefs,
            set_log,
            set_obsidian_look,
            set_performance,
            set_zed_look,
            set_glass,
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
            bar_move_pill,
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
            set_bar_scroll,
            set_glass_lift,
            open_sheet,
            glass_preview,
            set_close_idle_settings,
            set_readability,
            set_monochrome,
            contrast_report,
            system_info
        ])
        .setup(|app| {
            // A menu bar app, never in the Dock. Info.plist says so (LSUIElement),
            // which also keeps macOS from turning it into a Dock app when it hands
            // over a palette:// link. Tell the event loop too, so tao's launch-time
            // default (a Dock app) doesn't win.
            let handle = app.handle().clone();
            palette::log::set_source("app");
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            route_login_item_through_macos();
            relink_old_cli();
            allow_cache(app);
            let hotkey = paths()
                .and_then(|p| palette::settings::load(&p).map_err(err))
                .map(|s| s.hotkey)
                .unwrap_or_else(|_| palette::settings::DEFAULT_HOTKEY.into());
            if let Err(e) = register_hotkey(&handle, &hotkey) {
                eprintln!("palette: {e}");
            }

            // The menu bar icon and its native menu (`tray.rs`).
            tray::install(&handle)?;

            // The windows "ready" keeps loaded, the tray's left click, and closing idle
            // windows when they load on demand.
            let prefs = app_settings();
            apply_app_prefs(&handle, prefs.windows, prefs.menu_click);
            close_idle_windows(&handle);

            // Linux: the launcher entry (fuzzel, GNOME's dash, KDE's menu), so
            // Palette can be started again after quitting it.
            #[cfg(target_os = "linux")]
            if let (Ok(paths), Ok(exe)) = (paths(), std::env::current_exe())
                && !paths.is_fake()
                && let Err(e) =
                    palette::launcher::install(&paths, &exe, include_bytes!("../icons/128x128.png"))
            {
                eprintln!("palette: launcher entry: {e:#}");
            }

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

            // A display plugged in (or switched back on) keeps the wallpaper it
            // had when it was last connected; give it the current one.
            palette::system::displays::on_added(|| {
                let Ok(paths) = paths() else { return };
                if let Ok(Outcome::Failed(e)) = palette::effects::reapply_wallpaper(&paths) {
                    eprintln!("palette: wallpaper on the new display: {e}");
                }
            });

            // palette:// links, including the one that launched the app.
            let links = handle.clone();
            // The plugin calls this while holding Tauri's plugin lock, and building a window
            // (the quick switcher, closed when idle) takes that lock again: the main thread
            // waited on itself and the app froze. So the work runs after this event (`later`).
            app.deep_link().on_open_url(move |event| {
                let urls: Vec<String> = event.urls().iter().map(|u| u.to_string()).collect();
                later(&links, move |app| {
                    for url in &urls {
                        open_url(app, url);
                    }
                });
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
                _ if std::env::args().any(|a| a == QUICK_ARG) => toggle_overlay(&handle),
                _ if std::env::args().any(|a| a == SETTINGS_ARG) => show_settings(&handle),
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
        .run(|app, event| match event {
            // The last window closed (the idle check, or you closed Settings): Tauri would
            // quit here. Palette lives in the menu bar, so it stays; Quit still quits.
            RunEvent::ExitRequested { code, api, .. } if keep_running(code) => api.prevent_exit(),
            // Opening the app again (Finder, Spotlight, `open -a Palette`) shows the picker.
            // macOS only: on Linux a second launch reaches the single-instance handler.
            #[cfg(target_os = "macos")]
            RunEvent::Reopen { .. } => later(app, |app| show_picker(app, false)),
            _ => {}
        });
}

/// Whether an exit request should be refused (ROADMAP 9h). Tauri asks with no code when the
/// last window closes, which with "Everything on demand" happens a minute after you look
/// away; Quit (`app.exit(0)`) and a restart come with a code and go through.
fn keep_running(code: Option<i32>) -> bool {
    code.is_none()
}

/// The address the page loads a cached thumbnail from (14c): Tauri's asset protocol reads the
/// JPEG from disk, instead of the file crossing IPC as base64 text (a third bigger, kept as JS
/// strings). Encoded like JavaScript's `encodeURIComponent`, as `convertFileSrc` does. Only
/// Palette's cache folder is in the protocol's scope (`allow_cache`).
fn asset_url(path: &std::path::Path) -> String {
    let mut out = String::from("asset://localhost/");
    for b in path.to_string_lossy().bytes() {
        match b {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Let the page read thumbnails (and nothing else) through the asset protocol: the cache
/// folder of whichever home Palette uses (a fake one in tests too).
fn allow_cache(app: &tauri::App) {
    use tauri::Manager;
    if let Ok(p) = paths() {
        let dir = p.root().join("cache");
        let _ = std::fs::create_dir_all(&dir);
        if let Err(e) = app.asset_protocol_scope().allow_directory(&dir, true) {
            eprintln!("palette: thumbnails: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{asset_url, err, keep_running};

    #[test]
    fn the_last_window_closing_never_quits_but_quit_does() {
        assert!(keep_running(None), "the idle check closing the last window");
        assert!(!keep_running(Some(0)), "Quit in the menu");
        assert!(!keep_running(Some(tauri::RESTART_EXIT_CODE)), "a restart");
    }

    #[test]
    fn thumbnails_are_asset_addresses_encoded_like_convert_file_src() {
        let url = asset_url(std::path::Path::new(
            "/Users/me/.config/palette/cache/thumbs/a b+é.jpg",
        ));
        assert_eq!(
            url,
            "asset://localhost/%2FUsers%2Fme%2F.config%2Fpalette%2Fcache%2Fthumbs%2Fa%20b%2B%C3%A9.jpg"
        );
    }

    #[test]
    fn errors_keep_their_cause_for_the_page() {
        let e: palette::problem::Error = palette::problem::Problem::ThemeActive {
            name: "nord".into(),
        }
        .into();
        let e = err(e.context("removing nord"));
        assert_eq!(e.code, Some("theme-active"));
        assert_eq!(e.subject.as_deref(), Some("nord"));
        assert!(
            e.message
                .starts_with("removing nord: `nord` is the active theme")
        );

        let plain = err("just text".to_string());
        assert_eq!((plain.code, plain.message.as_str()), (None, "just text"));
    }
}
