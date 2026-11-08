//! Palette core. The CLI (`main.rs`) and, later, the Tauri UI both call into
//! this library, so no logic is duplicated between them.

pub mod catalog;
pub mod colour;
pub mod doctor;
pub mod edits;
pub mod effects;
pub mod fetch;
pub mod glass;
pub mod install;
pub mod launcher;
pub mod library;
pub mod log;
pub mod monochrome;
pub mod normalise;
pub mod paths;
pub mod pixels;
pub mod problem;
pub mod ranking;
pub mod render;
pub mod roles;
pub mod schedule;
pub mod search;
pub mod settings;
pub mod sheet;
pub mod state;
pub mod studio;
pub mod sync;
pub mod system;
pub mod targets;
pub mod theme;
pub mod thumbs;
pub mod tools;
pub mod wiring;
// Added in the "next five" round (kept together to ease merges).
pub mod check;
pub mod desktop;
pub mod favourites;
pub mod lock;

use anyhow::{Result, bail};

pub use effects::Effect;
pub use paths::Paths;
pub use state::Change;
pub use system::Outcome;

/// What `set` / `next` did, or would do with `dry_run`.
#[derive(Debug)]
pub struct Switch {
    pub theme: String,
    pub changes: Vec<(&'static str, Change)>,
    /// Files copied from the theme folder instead of rendered.
    pub overrides: Vec<&'static str>,
    pub warnings: Vec<String>,
    pub dry_run: bool,
    /// App reloads, appearance, wallpaper. Empty for a dry run.
    pub effects: Vec<Effect>,
}

/// Validate, render and (unless `dry_run`) activate a theme.
pub fn switch(paths: &Paths, name: &str, dry_run: bool) -> Result<Switch> {
    switch_with_wallpaper(paths, name, None, dry_run)
}

/// `palette set <theme> --wallpaper <name>`: switch and start on that one of
/// the theme's wallpapers instead of its default.
pub fn switch_with_wallpaper(
    paths: &Paths,
    name: &str,
    wallpaper: Option<&str>,
    dry_run: bool,
) -> Result<Switch> {
    let result = switch_inner(paths, name, wallpaper, dry_run);
    if !dry_run {
        match &result {
            Ok(s) => log::record(
                paths,
                &format!("set {}  {}", s.theme, log::summary(&s.effects)),
            ),
            Err(e) => log::record(paths, &format!("set {name} failed: {e:#}")),
        }
    }
    result
}

fn switch_inner(
    paths: &Paths,
    name: &str,
    wallpaper: Option<&str>,
    dry_run: bool,
) -> Result<Switch> {
    let _lock = if dry_run {
        None
    } else {
        Some(lock::exclusive(paths)?)
    };
    let theme = theme::load(paths, name)?;
    let wallpaper = wallpaper
        .map(|w| effects::wallpaper_named(&theme, w))
        .transpose()?;
    let files = render::render_with(&theme, paths, wallpaper.as_deref())?;
    let changes = state::diff(paths, &files)?;
    let mut effects = Vec::new();
    if !dry_run {
        state::apply(paths, &theme.name, &files)?;
        let _ = favourites::record(paths, &theme.name); // history only: never fail a switch
        // Only the apps whose file changed reload (14a): `palette set` to the active theme
        // reloads nothing; light/dark and the wallpaper follow the theme as before.
        let changed = changed_files(&changes);
        effects = effects::apply_only(paths, Some(&theme), wallpaper.as_deref(), Some(&changed));
    }
    Ok(Switch {
        theme: theme.name,
        changes,
        overrides: files
            .iter()
            .filter(|f| f.is_override)
            .map(|f| f.name)
            .collect(),
        warnings: theme.colors.warnings,
        dry_run,
        effects,
    })
}

/// `palette undo`: back to the previous theme. Returns its name and effects.
pub fn undo(paths: &Paths) -> Result<(String, Vec<Effect>)> {
    let _lock = lock::exclusive(paths)?;
    let Some(name) = state::step_back(paths)? else {
        bail!("nothing to undo: no earlier theme is kept");
    };
    // Rebuilt with today's settings (owner, 2026-11-08): glass, readability and colour edits
    // are yours, not part of the theme, so Undo doesn't bring old ones back. A theme deleted
    // since keeps the files from back then.
    let theme = theme::load(paths, &name).ok();
    if let Some(t) = &theme {
        for file in render::render(t, paths)? {
            state::replace_file(paths, &file)?;
        }
    }
    let _ = favourites::record(paths, &name);
    let effects = effects::apply(paths, theme.as_ref(), None);
    log::record(
        paths,
        &format!("undo to {name}  {}", log::summary(&effects)),
    );
    Ok((name, effects))
}

/// The active theme, loaded. Errors if none is set.
pub fn current(paths: &Paths) -> Result<theme::Theme> {
    let Some(name) = state::current_theme(paths)? else {
        bail!(problem::Problem::NoThemeYet);
    };
    theme::load(paths, &name)
}

/// The theme after the current one, alphabetically, wrapping around.
/// With nothing set yet, the first theme.
pub fn next_theme(paths: &Paths) -> Result<String> {
    step_theme(paths, 1)
}

/// The theme before the current one (`palette prev`, scrolling the bar item).
pub fn prev_theme(paths: &Paths) -> Result<String> {
    step_theme(paths, -1)
}

fn step_theme(paths: &Paths, by: isize) -> Result<String> {
    step_theme_in(paths, by, None)
}

/// The theme `by` steps from the current one, among `cycle`'s themes (`None`:
/// the `next_cycles` setting). A current theme outside the cycle steps to
/// its first (or, going back, last) theme.
fn step_theme_in(paths: &Paths, by: isize, cycle: Option<favourites::Cycle>) -> Result<String> {
    let names = favourites::cycle_names(paths, cycle)?;
    if names.is_empty() {
        bail!("no themes found");
    }
    let len = names.len() as isize;
    let current = state::current_theme(paths)?;
    let index = match current.and_then(|c| names.iter().position(|n| *n == c)) {
        Some(i) => (i as isize + by).rem_euclid(len),
        None if by < 0 => len - 1,
        None => 0,
    };
    Ok(names[index as usize].clone())
}

/// After an upgrade, `current/` may lack files for newly supported apps
/// (Obsidian's CSS, say). Render just those into the active theme's folder,
/// so wiring a new app points at a file that exists. Not a theme switch, so
/// no extra `undo` step.
pub fn fill_missing_outputs(paths: &Paths) -> Result<()> {
    let Some(name) = state::current_theme(paths)? else {
        return Ok(());
    };
    let missing: Vec<&str> = targets::all()
        .filter_map(|t| t.output.as_ref())
        .map(|o| o.file)
        .filter(|f| !paths.current().join(f).is_file())
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let theme = theme::load(paths, &name)?;
    for file in render::render(&theme, paths)? {
        if missing.contains(&file.name) {
            state::replace_file(paths, &file)?;
        }
    }
    Ok(())
}

/// Render one of the active theme's files again (`starship.toml` after the
/// Prompt page changed your config): no reloads, no undo step.
pub fn render_output(paths: &Paths, file: &str) -> Result<()> {
    let Some(name) = state::current_theme(paths)? else {
        return Ok(());
    };
    let theme = theme::load(paths, &name)?;
    for out in render::render(&theme, paths)? {
        if out.name == file {
            state::replace_file(paths, &out)?;
        }
    }
    Ok(())
}

/// `palette refresh`, Setup → Re-apply: render the active theme again (after editing a config
/// Palette copies, starship.toml say) and reload **every** app, for one that missed a change.
/// In place, like [`rerender`]: no Undo step.
pub fn refresh(paths: &Paths) -> Result<Switch> {
    match rerender_with(paths, None, true)? {
        Some(s) => Ok(s),
        None => bail!(problem::Problem::NoThemeYet),
    }
}

/// After a setting changed (glass, Zed, Obsidian, monochrome, readability, colour edits): render
/// the active theme again **in place** and reload only the apps whose file changed (14a).
/// `only` names the targets the setting is for (`None`: every app). No new generation, so
/// settings never use up Undo; no recents entry, no log line. `None` when no theme is active.
pub fn rerender(paths: &Paths, only: Option<&[&str]>) -> Result<Option<Switch>> {
    rerender_with(paths, only, false)
}

fn rerender_with(paths: &Paths, only: Option<&[&str]>, reload_all: bool) -> Result<Option<Switch>> {
    let _lock = lock::exclusive(paths)?;
    let Some(name) = state::current_theme(paths)? else {
        return Ok(None);
    };
    let theme = theme::load(paths, &name)?;
    let files = render::render_some(&theme, paths, only)?;
    let changes = state::diff(paths, &files)?;
    let changed = changed_files(&changes);
    for file in files.iter().filter(|f| changed.contains(&f.name)) {
        state::replace_file(paths, file)?;
    }
    let effects = match (reload_all, changed.is_empty()) {
        (true, _) => effects::apply_only(paths, None, None, None),
        (false, true) => Vec::new(),
        (false, false) => effects::apply_only(paths, None, None, Some(&changed)),
    };
    Ok(Some(Switch {
        theme: theme.name,
        changes,
        overrides: files
            .iter()
            .filter(|f| f.is_override)
            .map(|f| f.name)
            .collect(),
        warnings: theme.colors.warnings,
        dry_run: false,
        effects,
    }))
}

/// The files a render would write differently (new or changed).
fn changed_files(changes: &[(&'static str, state::Change)]) -> Vec<&'static str> {
    changes
        .iter()
        .filter(|(_, c)| !matches!(c, state::Change::Unchanged))
        .map(|(f, _)| *f)
        .collect()
}

/// `palette colours`: change a theme's colours (your `edits.toml`), and if
/// it is the active theme, render it again so every app picks them up.
/// Returns the keys that changed, and the switch when it re-rendered.
pub fn edit_colours(
    paths: &Paths,
    name: &str,
    edit: &edits::Edit,
) -> Result<(edits::Changes, Option<Switch>)> {
    let _lock = lock::exclusive(paths)?;
    let changed = edits::apply(paths, name, edit)?;
    let active = state::current_theme(paths)?.as_deref() == Some(name);
    let switched = if active && !changed.is_empty() {
        rerender(paths, None)?
    } else {
        None
    };
    Ok((changed, switched))
}

/// `palette check --level …` / `palette check <theme> --original on|off`: Palette's
/// readability fixes (`normalise`). `level` sets how hard they push for every theme;
/// `original` turns them off (true) or on (false) for one theme. `None` leaves each as it
/// is. When something changed and it touches the active theme, that theme is rendered
/// again so every app picks it up. Returns the level and the themes left original.
pub fn readability(
    paths: &Paths,
    level: Option<normalise::Level>,
    original: Option<(&str, bool)>,
) -> Result<(normalise::Level, std::collections::BTreeSet<String>)> {
    let _lock = lock::exclusive(paths)?;
    let mut s = settings::load(paths)?;
    let active = state::current_theme(paths)?;
    let mut rerender = false;
    let mut changed = false;
    if let Some(l) = level
        && l != s.readability
    {
        s.readability = l;
        rerender = active.is_some();
        changed = true;
    }
    if let Some((name, on)) = original {
        theme::load_original(paths, name)?; // a real theme, or a clear error
        let moved = if on {
            s.original_colours.insert(name.to_string())
        } else {
            s.original_colours.remove(name)
        };
        rerender |= moved && active.as_deref() == Some(name);
        changed |= moved;
    }
    if changed {
        settings::save(paths, &s)?;
    }
    if rerender {
        self::rerender(paths, None)?;
    }
    Ok((s.readability, s.original_colours))
}

/// `palette check --greys/--tint`, Settings → General (11f): what Palette does to monochrome
/// themes (`None` leaves a switch as it is). Saves, and renders the active theme again if
/// anything changed. Returns both switches as now set.
pub fn set_monochrome(
    paths: &Paths,
    greys: Option<bool>,
    tint: Option<bool>,
) -> Result<monochrome::Mono> {
    let _lock = lock::exclusive(paths)?;
    let mut s = settings::load(paths)?;
    let before = s.monochrome;
    if let Some(g) = greys {
        s.monochrome.greys = g;
    }
    if let Some(t) = tint {
        s.monochrome.tint = t;
    }
    if s.monochrome != before {
        settings::save(paths, &s)?;
        if state::current_theme(paths)?.is_some() {
            rerender(paths, None)?;
        }
    }
    Ok(s.monochrome)
}

/// `palette obsidian`: the active-pane ring and dimming in Obsidian (`None`
/// leaves that one as it is). Saves the choice and, if anything changed and a
/// theme is active, renders it again so Obsidian picks it up live.
/// Returns (ring, dim) as now set.
pub fn obsidian_look(paths: &Paths, ring: Option<bool>, dim: Option<bool>) -> Result<(bool, bool)> {
    let _lock = lock::exclusive(paths)?;
    let mut s = settings::load(paths)?;
    let before = (s.obsidian_ring, s.obsidian_dim);
    s.obsidian_ring = ring.unwrap_or(s.obsidian_ring);
    s.obsidian_dim = dim.unwrap_or(s.obsidian_dim);
    let now = (s.obsidian_ring, s.obsidian_dim);
    if now != before {
        settings::save(paths, &s)?;
        if state::current_theme(paths)?.is_some() {
            rerender(paths, Some(&["obsidian"]))?;
        }
    }
    Ok(now)
}

/// `palette zed`: Zed's window glass (off, blurred, clear) and how
/// see-through it is (0-100); `None` leaves that one. Saves the choice and, if
/// it changed and a theme is active, renders it again. A new glass kind also
/// re-points Zed's `theme` setting, as Zed only re-applies a window's blur
/// when its settings change. Returns the settings as now set.
pub fn zed_look(
    paths: &Paths,
    glass: Option<settings::Glass>,
    strength: Option<u8>,
) -> Result<(settings::Glass, u8)> {
    let _lock = lock::exclusive(paths)?;
    let mut s = settings::load(paths)?;
    let before = (s.zed_glass, s.zed_strength);
    s.zed_glass = glass.unwrap_or(s.zed_glass);
    s.zed_strength = strength.map_or(s.zed_strength, |n| n.min(100));
    let now = (s.zed_glass, s.zed_strength);
    if now != before {
        settings::save(paths, &s)?;
        if state::current_theme(paths)?.is_some() {
            rerender(paths, Some(&["zed"]))?;
        }
        if now.0 != before.0 && s.is_on("zed") {
            targets::zed::select(paths)?;
        }
    }
    Ok(now)
}

/// `palette glass <app>`: a terminal's window glass (kind, strength, blur;
/// `None` leaves that one). Saves the choice and, if it changed and a theme
/// is active, renders it again so the app reloads. Returns the glass as now set.
pub fn glass(
    paths: &Paths,
    app: &str,
    kind: Option<settings::TermGlass>,
    strength: Option<u8>,
    blur: Option<u8>,
) -> Result<settings::WindowGlass> {
    if !settings::GLASS_APPS.contains(&app) {
        bail!(
            "window glass is for {} (Zed has its own: `palette zed`)",
            settings::GLASS_APPS.join(", ")
        );
    }
    if let Some(k) = kind
        && !settings::TermGlass::kinds(app).contains(&k)
    {
        bail!("{} is Ghostty only (macOS 26's own glass)", k.as_str());
    }
    let _lock = lock::exclusive(paths)?;
    let mut s = settings::load(paths)?;
    let before = s.glass_saved(app);
    let now = settings::WindowGlass {
        kind: kind.unwrap_or(before.kind),
        strength: strength.map_or(before.strength, |n| n.min(100)),
        blur: blur.map_or(before.blur, |n| n.min(100)),
    };
    if now != before {
        s.glass.insert(app.to_string(), now);
        settings::save(paths, &s)?;
        if state::current_theme(paths)?.is_some() {
            rerender(paths, Some(&[app]))?;
        }
    }
    Ok(now)
}

/// `palette ui --windows … --click …`: how the app keeps its windows and what a click on its
/// menu bar icon opens. `None` leaves a choice as it is; returns both as they are now. The
/// running app reads them when it next opens a window, so no restart is needed.
pub fn app_prefs(
    paths: &Paths,
    windows: Option<settings::AppWindows>,
    click: Option<settings::MenuClick>,
) -> Result<(settings::AppWindows, settings::MenuClick)> {
    let _lock = lock::exclusive(paths)?;
    let mut s = settings::load(paths)?;
    let before = (s.windows, s.menu_click);
    if let Some(w) = windows {
        s.windows = w;
    }
    if let Some(c) = click {
        s.menu_click = c;
    }
    if (s.windows, s.menu_click) != before {
        settings::save(paths, &s)?;
    }
    Ok((s.windows, s.menu_click))
}

/// `palette ui --close-idle-settings on|off`: with windows on demand, close Settings after
/// ten minutes without focus even while it's open. `None` leaves it; returns whether it's on.
pub fn set_close_idle_settings(paths: &Paths, on: Option<bool>) -> Result<bool> {
    let _lock = lock::exclusive(paths)?;
    let mut s = settings::load(paths)?;
    if let Some(on) = on
        && on != s.close_idle_settings
    {
        s.close_idle_settings = on;
        settings::save(paths, &s)?;
    }
    Ok(s.close_idle_settings)
}

/// `palette log [on|off]`: one line per switch in `palette.log`. `None` leaves it; returns
/// whether it's on now.
pub fn set_log(paths: &Paths, on: Option<bool>) -> Result<bool> {
    let _lock = lock::exclusive(paths)?;
    let mut s = settings::load(paths)?;
    if let Some(on) = on
        && on != s.log
    {
        s.log = on;
        settings::save(paths, &s)?;
    }
    Ok(s.log)
}

/// `palette performance [on|off] [--glass on|off] …`: Performance mode. `on` switches it
/// (`None` leaves it), `parts` chooses what it covers. When anything changed, the theme is
/// rendered again (window glass), Zed's window kind is re-pointed, and a bar or borders that
/// Palette manages are written again, without a backup (the saved settings never change).
pub fn performance(
    paths: &Paths,
    on: Option<bool>,
    parts: &[(&str, bool)],
) -> Result<settings::Performance> {
    let _lock = lock::exclusive(paths)?;
    let mut s = settings::load(paths)?;
    let before = s.performance;
    if let Some(on) = on {
        s.performance.on = on;
    }
    for (part, value) in parts {
        s.performance.set_part(part, *value)?;
    }

    let now = s.performance;
    if now == before {
        return Ok(now);
    }
    settings::save(paths, &s)?;
    // Only what the mode writes differently, before and after, needs writing again. The
    // bar first, without a reload; then the theme (glass), which reloads SketchyBar only if
    // its colours file changed. The bar is reloaded once either way: two reloads at once
    // scramble it.
    let glass = before.glass_now() != now.glass_now();
    let bar = before.bar_now() != now.bar_now() || before.stats_now() != now.stats_now();
    let bar_written = bar && desktop::rewrite(paths, desktop::Tool::Bar, false)?.is_some();
    let theme_reloaded_bar = match glass.then(|| rerender(paths, None)).transpose()?.flatten() {
        Some(r) => s.is_on("sketchybar") && changed_files(&r.changes).contains(&"sketchybar.sh"),
        None => false,
    };
    if bar_written && !theme_reloaded_bar {
        desktop::reload(paths, desktop::Tool::Bar);
    }
    if glass && s.is_on("zed") && s.zed_glass != settings::Glass::Off {
        targets::zed::select(paths)?;
    }
    if before.borders_now() != now.borders_now() {
        desktop::rewrite(paths, desktop::Tool::Borders, true)?;
    }
    Ok(now)
}

/// `palette init [--remove]`: wire or unwire the configs, then reload the
/// apps that changed so the result shows straight away (never in a fake home).
pub fn init(paths: &Paths, remove: bool, dry_run: bool) -> Result<(wiring::Report, Vec<Effect>)> {
    let _lock = if dry_run {
        None
    } else {
        Some(lock::exclusive(paths)?)
    };
    if !remove && !dry_run {
        fill_missing_outputs(paths)?;
    }
    let report = wiring::run(paths, remove, dry_run)?;
    if !remove && !dry_run {
        rewrite_managed(paths, &report)?;
    }
    let mut effects = Vec::new();
    if dry_run || paths.is_fake() {
        return Ok((report, effects));
    }
    if remove && launcher::remove(paths)? {
        effects.push((
            "launcher".into(),
            Outcome::Done("app launcher entry removed".into()),
        ));
    }
    let mut ran = effects::Reloads::default();
    for target in targets::all() {
        let changed = report
            .items
            .iter()
            .any(|i| i.target == target.name && i.step.writes());
        if !changed {
            continue;
        }
        let outcome = match (remove, target.unload) {
            (true, Some(unload)) => unload(paths),
            _ => ran.run(paths, target.reload),
        };
        effects.push((target.name.to_string(), outcome));
    }
    Ok((report, effects))
}

/// Things besides apps that a theme changes, and can be switched off too.
pub const EXTRAS: [(&str, &str); 2] = [
    ("wallpaper", "the theme's wallpaper"),
    ("appearance", "macOS light/dark mode"),
];

/// Which system and desktop Palette runs on (the app's "Linux mode · niri +
/// Noctalia" line; the CLI shows it as `palette doctor`'s first row).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemInfo {
    /// `macos` or `linux`.
    pub platform: &'static str,
    /// `macos`, `gnome`, `kde`, `niri-noctalia`, `niri` or `other`.
    pub desktop: &'static str,
    /// For people: "macOS", "GNOME", "niri + Noctalia".
    pub label: String,
}

pub fn system_info() -> SystemInfo {
    let session = system::session::Session::detect();
    SystemInfo {
        platform: system::session::Platform::current().as_str(),
        desktop: session.id(),
        label: session.label(),
    }
}

/// How to open the quick switcher with a key on Linux, where Wayland doesn't
/// let an app grab keys: the desktop runs `<app> --quick` (the running app
/// gets the flag through its single-instance check). `None` on macOS, where
/// the app registers its own shortcut. `app` is the app's program path.
pub fn quick_switch_hint(desktop: &str, app: &str) -> Option<String> {
    let command = format!("{app} --quick");
    match desktop {
        "macos" => None,
        "niri" | "niri-noctalia" => Some(format!(
            "Add to the binds {{ }} section of ~/.config/niri/config.kdl:\n\
             Mod+Shift+T {{ spawn \"{app}\" \"--quick\"; }}"
        )),
        "gnome" => Some(format!(
            "Settings → Keyboard → View and Customise Shortcuts → Custom Shortcuts → +, \
             with the command:\n{command}"
        )),
        "kde" => Some(format!(
            "System Settings → Keyboard → Shortcuts → Add New → Command or Script:\n{command}"
        )),
        _ => Some(format!(
            "Bind a key in your desktop's settings to run:\n{command}"
        )),
    }
}

/// One line of `palette apps` (and a toggle in the app's Settings).
#[derive(Debug, Clone)]
pub struct App {
    pub name: &'static str,
    pub on: bool,
    pub installed: bool,
    pub wired: bool,
    /// For wallpaper/appearance: what it is. For apps: how it's hooked up.
    pub detail: String,
}

/// Every app and extra, with whether Palette themes it.
pub fn apps(paths: &Paths) -> Result<Vec<App>> {
    let settings = settings::load(paths)?;
    let mut out: Vec<App> = targets::all()
        .map(|t| {
            let installed = (t.installed)(paths);
            let wired = wiring::is_wired(paths, t);
            let detail = match (&t.wiring, installed, wired) {
                (wiring::Wiring::Shell, true, true) => "through shell.sh",
                (wiring::Wiring::Direct, true, _) => "applied on every switch",
                (wiring::Wiring::Direct, false, _) => "not installed",
                (_, _, true) => "wired",
                (_, false, false) => "not installed",
                (_, true, false) if !wiring::has_config(paths, t) => "no config file",
                (_, true, false) => "not wired yet: `palette init`",
            };
            App {
                name: t.name,
                on: settings.is_on(t.name),
                installed,
                wired,
                detail: detail.into(),
            }
        })
        .collect();
    out.extend(EXTRAS.iter().map(|(name, what)| App {
        name,
        on: settings.is_on(name),
        installed: true,
        wired: true,
        detail: (*what).into(),
    }));
    Ok(out)
}

/// `palette apps on|off <app>`: remember the choice, then wire or unwire
/// just that app (backing up first) and reload it, so the change shows
/// straight away. Switching off puts the app's own look back.
/// After wiring added Palette's lines to a bar or borders config that Palette manages (a
/// desktop page), write that file again from its settings, so the lines sit where the page puts
/// them. Appended at the end instead, the theme pill's line came after the block that colours
/// and places the pill: a grey pill in the wrong place (owner, 2026-11-08, after removing
/// Palette from all configs and setting it up again). The reload follows with the app's.
fn rewrite_managed(paths: &Paths, report: &wiring::Report) -> Result<()> {
    let wrote = |names: &[&str]| {
        report
            .items
            .iter()
            .any(|i| names.contains(&i.target) && i.step.writes())
    };
    if wrote(&["sketchybar", "sketchybar-item"]) {
        desktop::rewrite(paths, desktop::Tool::Bar, false)?;
    }
    if wrote(&["borders"]) {
        desktop::rewrite(paths, desktop::Tool::Borders, false)?;
    }
    Ok(())
}

pub fn set_app(paths: &Paths, name: &str, on: bool) -> Result<(wiring::Report, Vec<Effect>)> {
    let target = targets::get(name);
    if target.is_none() && !EXTRAS.iter().any(|(n, _)| *n == name) {
        let mut names: Vec<&str> = targets::all().map(|t| t.name).collect();
        names.extend(EXTRAS.iter().map(|(n, _)| *n));
        bail!("no app named `{name}`. Choose from: {}", names.join(", "));
    }
    let _lock = lock::exclusive(paths)?;
    let mut settings = settings::load(paths)?;
    if on {
        settings.off.remove(name);
    } else {
        settings.off.insert(name.to_string());
    }
    settings::save(paths, &settings)?;

    let empty = wiring::Report {
        items: Vec::new(),
        backup: None,
    };
    let Some(target) = target else {
        return Ok((empty, Vec::new())); // wallpaper/appearance: applies from the next switch
    };
    fill_missing_outputs(paths)?;
    // shell.sh lists only switched-on tools: rewrite it in place.
    if state::current_theme(paths)?.is_some()
        && let Some(shell) = targets::get("shell")
        && shell.output.is_some()
    {
        let theme = current(paths)?;
        if let Some(file) = render::render(&theme, paths)?
            .into_iter()
            .find(|f| f.name == "shell.sh")
        {
            state::replace_file(paths, &file)?;
        }
    }
    let report = wiring::run_for(paths, &[target], !on, false)?;
    if on {
        rewrite_managed(paths, &report)?;
    }
    let mut effects = Vec::new();
    if !paths.is_fake() && report.items.iter().any(|i| i.step.writes()) {
        let outcome = match (on, target.unload) {
            (false, Some(unload)) => unload(paths),
            _ => (target.reload)(paths),
        };
        effects.push((target.name.to_string(), outcome));
    }
    Ok((report, effects))
}

/// `palette import <dir>`: copy an export onto this Mac (backing up what it
/// replaces), then switch to the exported theme and wire any app that
/// isn't yet. A dry run only reports.
pub fn import(
    paths: &Paths,
    dir: &std::path::Path,
    dry_run: bool,
) -> Result<(sync::ImportReport, Option<wiring::Report>, Vec<Effect>)> {
    let _lock = if dry_run {
        None
    } else {
        Some(lock::exclusive(paths)?)
    };
    let report = sync::import(paths, dir, dry_run)?;
    if dry_run {
        return Ok((report, None, Vec::new()));
    }
    let mut effects = Vec::new();
    if let Some(theme) = &report.theme
        && theme::load(paths, theme).is_ok()
    {
        effects = switch(paths, theme, false)?.effects;
    }
    let wired = if state::current_theme(paths)?.is_some() {
        let (wired, reloads) = init(paths, false, false)?;
        effects.extend(reloads);
        Some(wired)
    } else {
        None
    };
    Ok((report, wired, effects))
}

/// `palette next` / `prev` (`by` = 1 / -1), the bar pill's scroll and the
/// app's ‹ ›. Picks the theme *inside* the switch lock, so several quick
/// `next`s step one theme each instead of all picking the same one.
/// `cycle`: `Some` to force all themes or favourites, `None` for the setting.
pub fn step(
    paths: &Paths,
    by: isize,
    cycle: Option<favourites::Cycle>,
    dry_run: bool,
) -> Result<Switch> {
    let _lock = if dry_run {
        None
    } else {
        Some(lock::exclusive(paths)?)
    };
    let name = step_theme_in(paths, by, cycle)?;
    switch(paths, &name, dry_run)
}

#[cfg(test)]
mod system_info_tests {
    use super::*;

    #[test]
    fn quick_switch_hints_name_the_desktop_and_the_command() {
        assert_eq!(quick_switch_hint("macos", "/x/palette-app"), None);
        let niri = quick_switch_hint("niri-noctalia", "/x/palette-app").unwrap();
        assert!(
            niri.contains("spawn \"/x/palette-app\" \"--quick\""),
            "{niri}"
        );
        for desktop in ["gnome", "kde", "other"] {
            let hint = quick_switch_hint(desktop, "/x/palette-app").unwrap();
            assert!(hint.contains("/x/palette-app --quick"), "{desktop}: {hint}");
        }
    }
}
