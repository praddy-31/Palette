//! Palette core. The CLI (`main.rs`) and, later, the Tauri UI both call into
//! this library, so no logic is duplicated between them.

pub mod catalog;
pub mod colour;
pub mod doctor;
pub mod edits;
pub mod effects;
pub mod fetch;
pub mod install;
pub mod library;
pub mod paths;
pub mod pixels;
pub mod render;
pub mod schedule;
pub mod settings;
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

use anyhow::{Context, Result, bail};

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
    let _lock = if dry_run {
        None
    } else {
        Some(lock::exclusive(paths)?)
    };
    let theme = theme::load(paths, name)?;
    let wallpaper = wallpaper
        .map(|w| effects::wallpaper_named(&theme, w))
        .transpose()?;
    let files = render::render(&theme, paths)?;
    let changes = state::diff(paths, &files)?;
    let mut effects = Vec::new();
    if !dry_run {
        state::apply(paths, &theme.name, &files)?;
        let _ = favourites::record(paths, &theme.name); // history only: never fail a switch
        effects = effects::apply(paths, Some(&theme), wallpaper.as_deref());
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
    // The theme may have been edited or deleted since; then just reload apps
    // with the files from back then.
    let theme = theme::load(paths, &name).ok();
    let _ = favourites::record(paths, &name);
    Ok((name, effects::apply(paths, theme.as_ref(), None)))
}

/// The active theme, loaded. Errors if none is set.
pub fn current(paths: &Paths) -> Result<theme::Theme> {
    let name = state::current_theme(paths)?
        .context("no theme is active yet: run `palette set <theme>` first")?;
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
    let missing: Vec<&str> = targets::ALL
        .iter()
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

/// `palette refresh`: render the active theme again, e.g. after editing a
/// config Palette copies (starship.toml).
pub fn refresh(paths: &Paths) -> Result<Switch> {
    let _lock = lock::exclusive(paths)?;
    let name = state::current_theme(paths)?
        .context("no theme is active yet: run `palette set <theme>` first")?;
    switch(paths, &name, false)
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
        Some(switch(paths, name, false)?)
    } else {
        None
    };
    Ok((changed, switched))
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
            refresh(paths)?;
        }
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
    let mut effects = Vec::new();
    if dry_run || paths.is_fake() {
        return Ok((report, effects));
    }
    let mut ran = effects::Reloads::default();
    for target in targets::ALL {
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
    let mut out: Vec<App> = targets::ALL
        .iter()
        .map(|t| {
            let installed = (t.installed)(paths);
            let wired = wiring::is_wired(paths, t);
            let detail = match (&t.wiring, installed, wired) {
                (wiring::Wiring::Shell, true, true) => "through shell.sh",
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
pub fn set_app(paths: &Paths, name: &str, on: bool) -> Result<(wiring::Report, Vec<Effect>)> {
    let target = targets::get(name);
    if target.is_none() && !EXTRAS.iter().any(|(n, _)| *n == name) {
        let mut names: Vec<&str> = targets::ALL.iter().map(|t| t.name).collect();
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
