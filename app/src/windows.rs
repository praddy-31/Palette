//! The windows: showing, placing, building on demand, closing when idle, the hotkey
//! and `palette://` links.

use super::*;

pub(crate) fn register_hotkey(app: &AppHandle, combo: &str) -> Result<(), AppError> {
    let shortcuts = app.global_shortcut();
    let state = app.state::<Hotkey>();
    let mut current = state.0.lock().map_err(|e| err(e.to_string()))?;
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
            Err(format!("can't use {combo}: {e} (another app may own it)").into())
        }
    }
}

/// Re-read settings.toml: `palette hotkey <combo>` may have changed it.
pub(crate) fn sync_hotkey(app: &AppHandle) {
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

/// A menu bar app (LSUIElement) is never "active" on its own: activate it
/// first, or the window appears behind the current app, or not at all.
pub(crate) fn show(window: &WebviewWindow) {
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
pub(crate) fn show_settings(app: &AppHandle) {
    if let Some(window) = window(app, "settings") {
        #[cfg(target_os = "macos")]
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
        show(&window);
    }
}

/// The picker, Settings or the toast: the one that's loaded, or a new one. A new window
/// takes about a second to draw its page; the toast and Settings then ask for what they
/// missed meanwhile (`last_switch`, `pending_tab`).
pub(crate) fn window(app: &AppHandle, label: &str) -> Option<WebviewWindow> {
    if let Some(w) = app.get_webview_window(label) {
        return Some(w);
    }
    let url = WebviewUrl::App(format!("{label}.html").into());
    let b = WebviewWindowBuilder::new(app, label, url).visible(false);
    let b = match label {
        "overlay" => b
            .title("Palette quick switch")
            .inner_size(860.0, 500.0)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .resizable(false)
            .center()
            .skip_taskbar(true)
            .visible_on_all_workspaces(true),
        "picker" => b
            .title("Palette")
            .inner_size(440.0, 580.0)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .resizable(false)
            .skip_taskbar(true)
            .visible_on_all_workspaces(true),
        "settings" => b
            .title("Palette Settings")
            .inner_size(560.0, 680.0)
            .min_inner_size(480.0, 480.0)
            .resizable(true)
            .center(),
        "toast" => b
            .title("Palette")
            .inner_size(340.0, 72.0)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .resizable(false)
            .skip_taskbar(true)
            .visible_on_all_workspaces(true)
            .focused(false)
            .focusable(false)
            .accept_first_mouse(true)
            .shadow(false),
        _ => return None,
    };
    let w = match b.build() {
        Ok(w) => w,
        Err(e) => {
            eprintln!("palette: {label} window: {e}");
            return None;
        }
    };
    match label {
        // Settings is a normal window: closing it hides it, and Palette is a menu bar app
        // again (see show_settings). On demand it's closed for good a moment later.
        "settings" => {
            let me = w.clone();
            w.on_window_event(move |event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    put_away(&me);
                }
            });
        }
        // Both hide when focus moves elsewhere, like a menu.
        "picker" | "overlay" => hide_on_blur(&w),
        _ => {}
    }
    Some(w)
}

/// Hide when focus moves elsewhere, like a menu (picker, quick switcher).
pub(crate) fn hide_on_blur(window: &WebviewWindow) {
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
                put_away(&w);
            }
        }
    });
}

/// How long a hidden window waits before it's closed (windows on demand). Long enough for
/// the page's last call to arrive (the quick switcher hides *before* it asks for the
/// switch), short enough that its memory is back before you'd look.
const FREE_AFTER: Duration = Duration::from_secs(3);

/// Hide a window. With windows on demand it is also closed a moment later, which frees its
/// page (measured 2026-11-08: Settings hidden and the quick switcher opened within the idle
/// check's 30-60 s shared one web process, ~220 MB in all). The toast is left to the idle
/// check: switches come in bursts (scrolling the bar pill) and rebuilding it each time
/// would delay it. Reopening a closed window takes about a second.
pub(crate) fn put_away(w: &WebviewWindow) {
    let _ = w.hide();
    #[cfg(target_os = "macos")]
    if w.label() == "settings" {
        // Settings gone: a menu bar app again (see show_settings).
        let _ = w
            .app_handle()
            .set_activation_policy(tauri::ActivationPolicy::Accessory);
    }
    if !frees_at_once(w.label(), app_settings().windows) {
        return;
    }
    let app = w.app_handle().clone();
    let label = w.label().to_string();
    std::thread::spawn(move || {
        std::thread::sleep(FREE_AFTER);
        on_main(&app, move |app| {
            // Shown again meanwhile: keep it.
            if let Some(w) = app.get_webview_window(&label)
                && !w.is_visible().unwrap_or(true)
            {
                let _ = w.destroy();
            }
        });
    });
}

/// Whether hiding this window also closes it: the windows that setting builds on demand,
/// except the toast (see [`put_away`]).
fn frees_at_once(label: &str, windows: palette::settings::AppWindows) -> bool {
    use palette::settings::AppWindows;
    label != "toast"
        && match windows {
            AppWindows::Ready => false,
            AppWindows::OnDemand => ON_DEMAND.contains(&label),
            AppWindows::AllOnDemand => ALL_WINDOWS.contains(&label),
        }
}

/// Run `f` on the main thread, without waiting for it. **Every window call from a
/// background thread (the idle check, a switch on a tokio worker) must go through this.**
/// On macOS a window getter called off the main thread (`is_visible`, `is_focused`) waits
/// for the main thread while holding Tauri's window lock; if the main thread is meanwhile
/// handling a link or a click that needs that lock, both wait forever and the app stops
/// answering (found 2026-11-06: an idle check and a `palette://picker` link at once).
pub(crate) fn on_main(app: &AppHandle, f: impl FnOnce(&AppHandle) + Send + 'static) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || f(&handle));
}

/// Like [`on_main`], but always *after* the current event, even when called on the main
/// thread. `run_on_main_thread` from the main thread runs the closure at once
/// (tauri-runtime-wry's `send_user_message`), so a plugin callback that holds Tauri's plugin
/// lock (deep links) would still build a window under it and freeze the app (ROADMAP 9h).
/// Posting from a short-lived thread queues it on the event loop instead.
pub(crate) fn later(app: &AppHandle, f: impl FnOnce(&AppHandle) + Send + 'static) {
    let app = app.clone();
    std::thread::spawn(move || on_main(&app, f));
}

/// What the idle check remembers between rounds (only touched on the main thread).
#[derive(Default)]
struct Idle {
    hidden_before: Vec<&'static str>,
    settings_unfocused: Option<Instant>,
}

/// How long Settings may sit open without focus before "close idle Settings" closes it.
const SETTINGS_IDLE: Duration = Duration::from_secs(10 * 60);

/// Windows on demand: every 30 s, close the picker, Settings or toast (and with "all on
/// demand" the quick switcher) if it was already hidden at the last check, so each is
/// closed 30-60 s after it was hidden. With `close_idle_settings`, Settings is also closed
/// after ten minutes without focus while still open (on another AeroSpace workspace it
/// counts as visible). Closing is the only way to free a page's memory: macOS WebKit has
/// no public way to suspend a page.
pub(crate) fn close_idle_windows(app: &AppHandle) {
    let app = app.clone();
    let idle = std::sync::Arc::new(Mutex::new(Idle::default()));
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(30));
            // Read the file here; touch windows and the tray only on the main thread.
            let settings = app_settings();
            let idle = idle.clone();
            on_main(&app, move |app| {
                if let Ok(mut idle) = idle.lock() {
                    idle_round(app, &settings, &mut idle);
                }
            });
        }
    });
}

/// One round of the idle check, on the main thread.
fn idle_round(app: &AppHandle, settings: &palette::settings::Settings, idle: &mut Idle) {
    use palette::settings::AppWindows;
    // A change made in a terminal (`palette ui --click`) catches up here.
    apply_app_prefs(app, settings.windows, settings.menu_click);
    let labels: &[&'static str] = match settings.windows {
        AppWindows::Ready => {
            *idle = Idle::default();
            return;
        }
        AppWindows::OnDemand => &ON_DEMAND,
        AppWindows::AllOnDemand => &ALL_WINDOWS,
    };
    let mut hidden_now = Vec::new();
    for &label in labels {
        let Some(w) = app.get_webview_window(label) else {
            if label == "settings" {
                idle.settings_unfocused = None;
            }
            continue;
        };
        if w.is_visible().unwrap_or(true) {
            if label == "settings" && settings.close_idle_settings {
                if w.is_focused().unwrap_or(true) {
                    idle.settings_unfocused = None;
                } else {
                    let since = *idle.settings_unfocused.get_or_insert_with(Instant::now);
                    if since.elapsed() >= SETTINGS_IDLE {
                        idle.settings_unfocused = None;
                        close_settings(&w);
                    }
                }
            }
            continue;
        }
        if idle.hidden_before.contains(&label) {
            let _ = w.destroy();
        } else {
            hidden_now.push(label);
        }
    }
    idle.hidden_before = hidden_now;
}

/// Close an open Settings window for good, and make Palette a menu bar app again (it was
/// a normal app with a Dock icon while Settings was open, see `show_settings`).
fn close_settings(w: &WebviewWindow) {
    let _ = w.destroy();
    #[cfg(target_os = "macos")]
    let _ = w
        .app_handle()
        .set_activation_policy(tauri::ActivationPolicy::Accessory);
}

/// The picker opens under the tray icon when clicked there; otherwise (bar
/// item, `palette ui`) at the top right, just below a SketchyBar-height bar.
pub(crate) fn show_picker(app: &AppHandle, from_tray: bool) {
    sync_hotkey(app);
    let Some(window) = window(app, "picker") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        put_away(&window);
        return;
    }
    let placed = from_tray && window.move_window(Position::TrayBottomCenter).is_ok();
    if !placed {
        place_top_right(&window);
    }
    show(&window);
}

pub(crate) fn place_top_right(window: &WebviewWindow) {
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

pub(crate) fn toggle_overlay(app: &AppHandle) {
    sync_hotkey(app);
    let Some(window) = window(app, "overlay") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        put_away(&window);
    } else {
        let _ = window.center();
        show(&window);
    }
}

/// `palette://picker`, `palette://overlay`, `palette://next`, `palette://prev`.
pub(crate) fn open_url(app: &AppHandle, url: &str) {
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
        // The bar's theme pill: whatever a menu bar click opens. The native menu
        // can only open from the menu bar icon, so the pill opens the quick switcher.
        "click" => match app_settings().menu_click {
            palette::settings::MenuClick::Picker => show_picker(app, false),
            _ => toggle_overlay(app),
        },
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

#[cfg(test)]
mod tests {
    use super::frees_at_once;
    use palette::settings::AppWindows;

    #[test]
    fn hiding_closes_only_the_windows_built_on_demand_and_never_the_toast() {
        for label in ["overlay", "picker", "settings", "toast"] {
            assert!(!frees_at_once(label, AppWindows::Ready), "{label}");
        }
        assert!(frees_at_once("settings", AppWindows::OnDemand));
        assert!(frees_at_once("picker", AppWindows::OnDemand));
        assert!(
            !frees_at_once("overlay", AppWindows::OnDemand),
            "stays loaded"
        );
        assert!(frees_at_once("overlay", AppWindows::AllOnDemand));
        assert!(
            !frees_at_once("toast", AppWindows::AllOnDemand),
            "left to the idle check"
        );
    }
}
