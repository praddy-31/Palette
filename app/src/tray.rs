//! The menu bar icon and its native macOS menu (Phase 9c): the active theme, a Themes
//! submenu (★ favourites, recent, all, the active one ticked), next/previous/undo, next
//! wallpaper, the quick switcher, the picker, Performance mode, Settings, Quit.
//!
//! A native menu is an `NSMenu` inside the app process: no web page, no WebKit process,
//! so it costs next to nothing. It is built again whenever the pointer enters the icon
//! and after every switch from the app, so it's never stale, even after `palette set`
//! in a terminal.

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Wry};

use super::{
    Switch, TRAY_ICON, app_settings, paths, run_switch, set_performance, show_picker,
    show_settings, step, step_wallpaper, toggle_overlay,
};

pub const TRAY_ID: &str = "palette";

/// Themes shown under "Recent" (after the favourites, which aren't repeated there).
const RECENT: usize = 5;

/// What the menu shows, read from Palette's files. Kept apart from building the menu so
/// it can be tested without a running app.
#[derive(Debug, Default, PartialEq)]
pub struct MenuModel {
    pub current: Option<String>,
    pub favourites: Vec<String>,
    pub recent: Vec<String>,
    pub all: Vec<String>,
    pub performance: bool,
}

impl MenuModel {
    pub fn load() -> Self {
        let Ok(paths) = paths() else {
            return Self::default();
        };
        let settings = app_settings();
        let all: Vec<String> = palette::theme::list(&paths)
            .map(|l| l.into_iter().map(|(name, _)| name).collect())
            .unwrap_or_default();
        Self::from_parts(
            palette::state::current_theme(&paths).ok().flatten(),
            settings.favourites,
            palette::favourites::recents(&paths),
            all,
            settings.performance.on,
        )
    }

    /// Favourites and recents only if the theme still exists; recents without the
    /// favourites, at most [`RECENT`].
    pub fn from_parts(
        current: Option<String>,
        favourites: Vec<String>,
        recents: Vec<String>,
        all: Vec<String>,
        performance: bool,
    ) -> Self {
        let favourites: Vec<String> = favourites.into_iter().filter(|f| all.contains(f)).collect();
        let recent = recents
            .into_iter()
            .filter(|r| all.contains(r) && !favourites.contains(r))
            .take(RECENT)
            .collect();
        Self {
            current,
            favourites,
            recent,
            all,
            performance,
        }
    }
}

/// A theme item's id: `theme<section>:<name>`, so the same theme can appear under
/// Favourites and All (menu ids should be unique).
fn theme_id(section: u8, name: &str) -> String {
    format!("theme{section}:{name}")
}

/// The theme a menu id stands for, if it's a theme item.
pub fn theme_of(id: &str) -> Option<&str> {
    let (head, name) = id.split_once(':')?;
    let section = head.strip_prefix("theme")?;
    (!section.is_empty() && section.chars().all(|c| c.is_ascii_digit())).then_some(name)
}

fn build_menu(app: &AppHandle, m: &MenuModel) -> tauri::Result<Menu<Wry>> {
    let themes = Submenu::with_id(app, "themes", "Themes", true)?;
    let section = |title: &str, names: &[String], n: u8| -> tauri::Result<()> {
        if names.is_empty() {
            return Ok(());
        }
        if !themes.items()?.is_empty() {
            themes.append(&PredefinedMenuItem::separator(app)?)?;
        }
        themes.append(&MenuItem::with_id(
            app,
            format!("head{n}"),
            title,
            false,
            None::<&str>,
        )?)?;
        for name in names {
            let on = m.current.as_deref() == Some(name.as_str());
            themes.append(&CheckMenuItem::with_id(
                app,
                theme_id(n, name),
                name,
                true,
                on,
                None::<&str>,
            )?)?;
        }
        Ok(())
    };
    section("Favourites", &m.favourites, 0)?;
    section("Recent", &m.recent, 1)?;
    section("All themes", &m.all, 2)?;

    let current = MenuItem::with_id(
        app,
        "current",
        m.current.as_deref().unwrap_or("No theme yet"),
        false,
        None::<&str>,
    )?;
    let item = |id: &str, text: &str| MenuItem::with_id(app, id, text, true, None::<&str>);
    let next = item("next", "Next Theme")?;
    let prev = item("prev", "Previous Theme")?;
    let undo = item("undo", "Undo")?;
    let wallpaper = item("wallpaper", "Next Wallpaper")?;
    let overlay = item("overlay", "Quick Switch…")?;
    let open = item("open", "Open Picker")?;
    let performance = CheckMenuItem::with_id(
        app,
        "performance",
        "Performance Mode",
        true,
        m.performance,
        None::<&str>,
    )?;
    let settings = item("settings", "Settings…")?;
    let quit = item("quit", "Quit Palette")?;
    let sep = || PredefinedMenuItem::separator(app);
    let (s1, s2, s3, s4) = (sep()?, sep()?, sep()?, sep()?);
    let items: Vec<&dyn IsMenuItem<Wry>> = vec![
        &current,
        &themes,
        &s1,
        &next,
        &prev,
        &undo,
        &wallpaper,
        &s2,
        &overlay,
        &open,
        &s3,
        &performance,
        &settings,
        &s4,
        &quit,
    ];
    Menu::with_items(app, &items)
}

/// Build the menu again from Palette's files (theme, favourites, Performance mode).
pub fn refresh(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(tray) = handle.tray_by_id(TRAY_ID) else {
            return;
        };
        match build_menu(&handle, &MenuModel::load()) {
            Ok(menu) => {
                let _ = tray.set_menu(Some(menu));
            }
            Err(e) => eprintln!("palette: menu: {e}"),
        }
    });
}

fn spawn<F>(f: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    tauri::async_runtime::spawn(f);
}

fn on_menu(app: &AppHandle, id: &str) {
    if let Some(name) = theme_of(id) {
        let (app, name) = (app.clone(), name.to_string());
        spawn(async move {
            let _ = run_switch(app, Switch::To(name, None)).await;
        });
        return;
    }
    match id {
        "open" => show_picker(app, false),
        "settings" => show_settings(app),
        "overlay" => toggle_overlay(app),
        "next" | "prev" | "undo" => {
            let (app, how) = (app.clone(), id.to_string());
            spawn(async move {
                let _ = step(app, how).await;
            });
        }
        "wallpaper" => spawn(async move {
            let _ = step_wallpaper(None).await;
        }),
        "performance" => {
            // The tick has already flipped; the file decides, so a change made in a
            // terminal meanwhile isn't undone.
            let app = app.clone();
            spawn(async move {
                let on = !app_settings().performance.on;
                let _ = set_performance(app.clone(), Some(on), None, None).await;
                let _ = app.emit("performance", on);
            });
        }
        "quit" => app.exit(0),
        _ => {}
    }
}

/// The icon in the menu bar. A left click does what Settings → General says
/// (`menu_click`); a right click always shows the menu.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app, &MenuModel::load())?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(TRAY_ICON)?)
        .icon_as_template(true)
        .tooltip("Palette")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
            match event {
                // Before any click: the menu matches the files (a switch in a terminal).
                TrayIconEvent::Enter { .. } => refresh(tray.app_handle()),
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => {
                    let app = tray.app_handle();
                    match app_settings().menu_click {
                        palette::settings::MenuClick::Picker => show_picker(app, true),
                        palette::settings::MenuClick::Quick => toggle_overlay(app),
                        // macOS opens the menu itself (show_menu_on_left_click).
                        palette::settings::MenuClick::Menu => {}
                    }
                }
                _ => {}
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn theme_ids_round_trip() {
        assert_eq!(theme_of(&theme_id(0, "tokyo-night")), Some("tokyo-night"));
        assert_eq!(theme_of(&theme_id(2, "nord")), Some("nord"));
        assert_eq!(theme_of("next"), None);
        assert_eq!(theme_of("themes"), None);
        assert_eq!(theme_of("theme:x"), None);
        assert_eq!(theme_of("themex:y"), None);
    }

    #[test]
    fn favourites_first_recents_without_them() {
        let m = MenuModel::from_parts(
            Some("nord".into()),
            names(&["nord", "gone"]),
            names(&["kanagawa", "nord", "ash", "gone", "a", "b", "c", "d"]),
            names(&["a", "ash", "b", "c", "d", "kanagawa", "nord"]),
            true,
        );
        assert_eq!(m.favourites, names(&["nord"]));
        assert_eq!(m.recent, names(&["kanagawa", "ash", "a", "b", "c"]));
        assert_eq!(m.all.len(), 7);
        assert!(m.performance);
    }
}
