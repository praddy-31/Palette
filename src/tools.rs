//! `palette tools`: the programs Palette themes but does not ship, how to get
//! them with Homebrew, and optional starter configs.
//!
//! Palette never runs Homebrew itself. This module only reports what is
//! installed and prints the exact commands; the person copies them into a
//! terminal (or the app's "Open Terminal" button) and runs them.
//!
//! Starter configs are the one place Palette writes a config file that is
//! not its own, and only on an explicit request into an empty slot:
//! `add_starter` refuses if the tool has any config at all. The files are
//! copied from `src/tools/starters/` (compiled in), then wired exactly like
//! `palette init` wires any config. A snapshot of what was written lives in
//! `~/.config/palette/starters/`, so `remove_starter` can offer to move the
//! config to the Trash only while it is still exactly what Palette wrote.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::paths::Paths;
use crate::{Effect, effects, install, system, targets, wiring};

/// A file of a starter config, relative to `~/.config`.
struct StarterFile {
    rel: &'static str,
    text: &'static str,
    executable: bool,
}

const SKETCHYBAR_FILES: &[StarterFile] = &[
    StarterFile {
        rel: "sketchybar/sketchybarrc",
        text: include_str!("tools/starters/sketchybar/sketchybarrc"),
        executable: true,
    },
    StarterFile {
        rel: "sketchybar/plugins/space.sh",
        text: include_str!("tools/starters/sketchybar/plugins/space.sh"),
        executable: true,
    },
    StarterFile {
        rel: "sketchybar/plugins/front_app.sh",
        text: include_str!("tools/starters/sketchybar/plugins/front_app.sh"),
        executable: true,
    },
    StarterFile {
        rel: "sketchybar/plugins/clock.sh",
        text: include_str!("tools/starters/sketchybar/plugins/clock.sh"),
        executable: true,
    },
    StarterFile {
        rel: "sketchybar/plugins/battery.sh",
        text: include_str!("tools/starters/sketchybar/plugins/battery.sh"),
        executable: true,
    },
];

const AEROSPACE_FILES: &[StarterFile] = &[StarterFile {
    rel: "aerospace/aerospace.toml",
    text: include_str!("tools/starters/aerospace/aerospace.toml"),
    executable: false,
}];

const BORDERS_FILES: &[StarterFile] = &[StarterFile {
    rel: "borders/bordersrc",
    text: include_str!("tools/starters/borders/bordersrc"),
    executable: true,
}];

/// One tool Palette knows how to describe.
struct Spec {
    id: &'static str,
    name: &'static str,
    /// `look` or `wallpaper`.
    group: &'static str,
    what: &'static str,
    install: &'static [&'static str],
    remove: &'static [&'static str],
    /// Run after installing, to start it.
    start: Option<&'static str>,
    /// Program name searched on `PATH` / Homebrew's folders.
    program: &'static str,
    /// App bundle, for tools installed as a cask.
    bundle: Option<&'static str>,
    starter: &'static [StarterFile],
    /// Palette targets to wire after writing the starter.
    wire: &'static [&'static str],
    /// Said after adding the starter.
    after: &'static str,
}

const SPECS: &[Spec] = &[
    Spec {
        id: "sketchybar",
        name: "SketchyBar",
        group: "look",
        what: "A bar along the top of the screen",
        install: &[
            "brew install FelixKratz/formulae/sketchybar",
            "brew services start sketchybar",
        ],
        remove: &["brew services stop sketchybar", "brew uninstall sketchybar"],
        start: Some("brew services start sketchybar"),
        program: "sketchybar",
        bundle: None,
        starter: SKETCHYBAR_FILES,
        wire: &["sketchybar", "sketchybar-item"],
        after: "SketchyBar reloads by itself if it is running",
    },
    Spec {
        id: "borders",
        name: "JankyBorders",
        group: "look",
        what: "Coloured borders around windows",
        install: &[
            "brew install FelixKratz/formulae/borders",
            "brew services start borders",
        ],
        remove: &["brew services stop borders", "brew uninstall borders"],
        start: Some("brew services start borders"),
        program: "borders",
        bundle: None,
        starter: BORDERS_FILES,
        wire: &["borders"],
        after: "Restart it with `brew services restart borders` if it is running",
    },
    Spec {
        id: "aerospace",
        name: "AeroSpace",
        group: "look",
        what: "Tiling windows, with Option-key shortcuts",
        install: &["brew install --cask nikitabobko/tap/aerospace"],
        remove: &["brew uninstall --cask aerospace"],
        start: None,
        program: "aerospace",
        bundle: Some("AeroSpace.app"),
        starter: AEROSPACE_FILES,
        wire: &[],
        after: "Open AeroSpace, or reload it from its menu bar icon, to use it",
    },
    Spec {
        id: "gowall",
        name: "gowall",
        group: "wallpaper",
        what: "Recolours wallpapers (an alternative engine in Wallpapers)",
        install: &["brew install gowall"],
        remove: &["brew uninstall gowall"],
        start: None,
        program: "gowall",
        bundle: None,
        starter: &[],
        wire: &[],
        after: "",
    },
];

/// Where a tool's starter stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Starter {
    /// This tool has no starter (gowall).
    None,
    /// No config yet: a starter can be added.
    Available,
    /// The tool already has a config (yours): no starter is offered.
    HasConfig,
    /// Palette added the starter. `unchanged`: it is still exactly what was
    /// written, so it can be removed (to the Trash).
    Added { unchanged: bool },
}

#[derive(Debug, Clone)]
pub struct Tool {
    pub id: &'static str,
    pub name: &'static str,
    /// `look` or `wallpaper`.
    pub group: &'static str,
    pub what: &'static str,
    pub installed: bool,
    pub install: Vec<String>,
    pub remove: Vec<String>,
    pub starter: Starter,
    /// The folder or file that holds its config.
    pub config: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Status {
    /// Is Homebrew installed? Palette never installs it.
    pub homebrew: bool,
    pub tools: Vec<Tool>,
}

pub const HOMEBREW_SITE: &str = "https://brew.sh";

/// `palette tools`: what is installed, and the commands to get or remove it.
pub fn status(paths: &Paths) -> Status {
    Status {
        homebrew: system::find_program("brew").is_some(),
        tools: SPECS.iter().map(|s| tool(paths, s)).collect(),
    }
}

fn tool(paths: &Paths, spec: &Spec) -> Tool {
    let installed = system::find_program(spec.program).is_some()
        || spec.bundle.is_some_and(|b| targets::has_app(paths, b));
    let starter = if spec.starter.is_empty() {
        Starter::None
    } else if snapshot_dir(paths, spec).is_dir() {
        Starter::Added {
            unchanged: is_unchanged(paths, spec),
        }
    } else if config_exists(paths, spec) {
        Starter::HasConfig
    } else {
        Starter::Available
    };
    Tool {
        id: spec.id,
        name: spec.name,
        group: spec.group,
        what: spec.what,
        installed,
        install: spec.install.iter().map(|s| s.to_string()).collect(),
        remove: spec.remove.iter().map(|s| s.to_string()).collect(),
        starter,
        config: (!spec.starter.is_empty()).then(|| config_path(paths, spec)),
    }
}

fn spec(id: &str) -> Result<&'static Spec> {
    SPECS.iter().find(|s| s.id == id).with_context(|| {
        let names: Vec<&str> = SPECS.iter().map(|s| s.id).collect();
        format!("no tool named `{id}`. Choose from: {}", names.join(", "))
    })
}

/// The ids that can have a starter config.
pub fn starter_ids() -> Vec<&'static str> {
    SPECS
        .iter()
        .filter(|s| !s.starter.is_empty())
        .map(|s| s.id)
        .collect()
}

/// What to run after installing, if the tool needs starting.
pub fn start_command(id: &str) -> Option<&'static str> {
    SPECS.iter().find(|s| s.id == id).and_then(|s| s.start)
}

// ---------------------------------------------------------------- config

/// The folder holding the tool's config (`~/.config/<id>`).
fn config_path(paths: &Paths, spec: &Spec) -> PathBuf {
    paths.config_home().join(spec.id)
}

fn has_entries(dir: &Path) -> bool {
    fs::read_dir(dir).is_ok_and(|d| {
        d.flatten()
            .any(|e| e.file_name().to_string_lossy() != ".DS_Store")
    })
}

/// Does the tool have any config at all? AeroSpace also reads `~/.aerospace.toml`.
fn config_exists(paths: &Paths, spec: &Spec) -> bool {
    has_entries(&config_path(paths, spec))
        || (spec.id == "aerospace" && paths.home().join(".aerospace.toml").exists())
}

fn snapshot_dir(paths: &Paths, spec: &Spec) -> PathBuf {
    paths.root().join("starters").join(spec.id)
}

/// Remove Palette's marker comment and the line after it (the import line).
fn strip_palette_blocks(text: &str) -> String {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut out = String::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_end().ends_with(wiring::MARKER) && i + 1 < lines.len() {
            i += 2;
        } else {
            out.push_str(lines[i]);
            i += 1;
        }
    }
    out
}

/// Every file below `dir`, relative to `base`.
fn files_below(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.file_name().is_some_and(|n| n == ".DS_Store") {
            continue;
        }
        if path.is_dir() {
            files_below(&path, base, out);
        } else if let Ok(rel) = path.strip_prefix(base) {
            out.push(rel.to_path_buf());
        }
    }
}

/// Is the config still exactly what Palette wrote (apart from Palette's own
/// import lines): same files, same bytes?
fn is_unchanged(paths: &Paths, spec: &Spec) -> bool {
    let mut found = Vec::new();
    files_below(&config_path(paths, spec), paths.config_home(), &mut found);
    found.sort();
    let mut expected: Vec<PathBuf> = spec.starter.iter().map(|f| PathBuf::from(f.rel)).collect();
    expected.sort();
    if found != expected {
        return false;
    }
    spec.starter.iter().all(|f| {
        let current = fs::read_to_string(paths.config_home().join(f.rel));
        let written = fs::read_to_string(paths.root().join("starters").join(f.rel));
        match (current, written) {
            (Ok(c), Ok(w)) => strip_palette_blocks(&c) == w,
            _ => false,
        }
    })
}

// ---------------------------------------------------------------- starters

/// What `add_starter` did.
#[derive(Debug)]
pub struct Added {
    pub files: Vec<PathBuf>,
    pub wiring: Option<wiring::Report>,
    pub effects: Vec<Effect>,
    pub hint: &'static str,
}

/// `palette tools starter <tool>`: write the small Palette-themed config for a
/// tool that has none, then wire it like `palette init`. Refuses if the tool
/// has any config. Writes nothing outside the tool's own config folder and
/// Palette's folder.
pub fn add_starter(paths: &Paths, id: &str) -> Result<Added> {
    let spec = spec(id)?;
    if spec.starter.is_empty() {
        bail!(
            "{} has no starter config (starters: {})",
            spec.name,
            starter_ids().join(", ")
        );
    }
    if snapshot_dir(paths, spec).is_dir() {
        bail!(
            "{}: the starter config is already added ({})",
            spec.name,
            config_path(paths, spec).display()
        );
    }
    if config_exists(paths, spec) {
        bail!(
            "{} already has a config ({}): Palette only adds a starter when there is none, \
             and never overwrites yours",
            spec.name,
            config_path(paths, spec).display()
        );
    }
    let mut files = Vec::new();
    for f in spec.starter {
        let path = paths.config_home().join(f.rel);
        write_file(&path, f.text, f.executable)?;
        write_file(&paths.root().join("starters").join(f.rel), f.text, false)?;
        files.push(path);
    }
    // Wire like `palette init`: one guarded import line, for the switched-on targets.
    let mut report = None;
    let mut done = Vec::new();
    if !spec.wire.is_empty() {
        let settings = crate::settings::load(paths)?;
        let chosen: Vec<&targets::Target> = spec
            .wire
            .iter()
            .filter(|n| settings.is_on(n))
            .filter_map(|n| targets::get(n))
            .collect();
        crate::fill_missing_outputs(paths)?;
        let r = wiring::run_for(paths, &chosen, false, false)?;
        if !paths.is_fake() {
            let mut ran = effects::Reloads::default();
            for target in &chosen {
                if r.items
                    .iter()
                    .any(|i| i.target == target.name && i.step.writes())
                {
                    done.push((target.name.to_string(), ran.run(paths, target.reload)));
                }
            }
        }
        report = Some(r);
    }
    Ok(Added {
        files,
        wiring: report,
        effects: done,
        hint: spec.after,
    })
}

/// `palette tools starter <tool> --remove`: move the starter config to the
/// Trash, only while it is still exactly what Palette wrote. Uninstalling the
/// program itself is the tool's own removal (`brew uninstall …`).
pub fn remove_starter(paths: &Paths, id: &str) -> Result<PathBuf> {
    let spec = spec(id)?;
    if !snapshot_dir(paths, spec).is_dir() {
        bail!("{}: Palette didn't add a starter config", spec.name);
    }
    if !is_unchanged(paths, spec) {
        bail!(
            "{}: the config has changed since Palette wrote it, so Palette won't remove it \
             (it's yours now: {})",
            spec.name,
            config_path(paths, spec).display()
        );
    }
    let trashed = install::move_to_trash(paths, &config_path(paths, spec))?;
    fs::remove_dir_all(snapshot_dir(paths, spec))
        .with_context(|| format!("forgetting the {} starter", spec.name))?;
    Ok(trashed)
}

/// Write via a temporary file and a rename, so a crash never leaves half a file.
fn write_file(path: &Path, text: &str, executable: bool) -> Result<()> {
    let dir = path.parent().context("no folder")?;
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let tmp = dir.join(format!(".palette-{}.tmp", std::process::id()));
    fs::write(&tmp, text).with_context(|| format!("writing {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if executable { 0o755 } else { 0o644 };
        fs::set_permissions(&tmp, fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    let _ = executable;
    fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

// ---------------------------------------------------------------- app helpers

/// Open Terminal.app (the person pastes the command there).
pub fn open_terminal() -> Result<()> {
    system::run("open", &["-a", "Terminal"])
        .map(|_| ())
        .map_err(anyhow::Error::msg)
}

/// Open the Homebrew website in the browser.
pub fn open_homebrew_site() -> Result<()> {
    system::run("open", &[HOMEBREW_SITE])
        .map(|_| ())
        .map_err(anyhow::Error::msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_only_palette_blocks() {
        let text = format!("a\n# {}\nsource x\nb\n", wiring::MARKER);
        assert_eq!(strip_palette_blocks(&text), "a\nb\n");
    }

    #[test]
    fn every_starter_file_is_known() {
        for spec in SPECS {
            for f in spec.starter {
                assert!(f.rel.starts_with(spec.id), "{}", f.rel);
                assert!(!f.text.is_empty());
            }
        }
        assert_eq!(starter_ids(), ["sketchybar", "borders", "aerospace"]);
    }
}
