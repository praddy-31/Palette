//! What an import needs that this Mac doesn't have (ROADMAP 25a). An export carries configs
//! (SketchyBar, AeroSpace, Ghostty…); the programs they belong to may not be installed here.
//! This lists them with the Homebrew commands to get them, in an order that works when pasted
//! as one block: taps first, then formulae and casks, then `brew services start` for the
//! ones that run in the background. **Palette never runs any of it** (owner, 2026-11-05 and
//! 2026-11-16): the person copies the commands into a terminal, as with Get the tools.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::paths::Paths;

/// An app an import brought configs for.
struct App {
    /// The Palette target (or tool id) that tells whether it's installed.
    id: &'static str,
    name: &'static str,
    /// Config paths relative to home; a file matches a prefix ending in `/` by folder.
    configs: &'static [&'static str],
    /// `brew tap …` needed first (deduplicated across apps).
    tap: Option<&'static str>,
    /// The install command; `None` = not in Homebrew (`site` says where).
    install: Option<&'static str>,
    /// Started in the background after installing.
    service: Option<&'static str>,
    site: Option<&'static str>,
}

const FELIX: &str = "brew tap FelixKratz/formulae";

const APPS: &[App] = &[
    App {
        id: "sketchybar",
        name: "SketchyBar",
        configs: &[".config/sketchybar/"],
        tap: Some(FELIX),
        install: Some("brew install sketchybar"),
        service: Some("brew services start sketchybar"),
        site: None,
    },
    App {
        id: "borders",
        name: "JankyBorders",
        configs: &[".config/borders/"],
        tap: Some(FELIX),
        install: Some("brew install borders"),
        service: Some("brew services start borders"),
        site: None,
    },
    App {
        id: "aerospace",
        name: "AeroSpace",
        configs: &[".config/aerospace/", ".aerospace.toml"],
        tap: None,
        install: Some("brew install --cask nikitabobko/tap/aerospace"),
        service: None,
        site: None,
    },
    App {
        id: "ghostty",
        name: "Ghostty",
        configs: &[
            "Library/Application Support/com.mitchellh.ghostty/",
            ".config/ghostty/",
        ],
        tap: None,
        install: Some("brew install --cask ghostty"),
        service: None,
        site: None,
    },
    App {
        id: "kitty",
        name: "kitty",
        configs: &[".config/kitty/"],
        tap: None,
        install: Some("brew install --cask kitty"),
        service: None,
        site: None,
    },
    App {
        id: "alacritty",
        name: "Alacritty",
        configs: &[".config/alacritty/"],
        tap: None,
        install: Some("brew install --cask alacritty"),
        service: None,
        site: None,
    },
    App {
        id: "wezterm",
        name: "WezTerm",
        configs: &[".config/wezterm/", ".wezterm.lua"],
        tap: None,
        install: Some("brew install --cask wezterm"),
        service: None,
        site: None,
    },
    App {
        id: "zed",
        name: "Zed",
        configs: &[".config/zed/"],
        tap: None,
        install: Some("brew install --cask zed"),
        service: None,
        site: None,
    },
    App {
        id: "vscodium",
        name: "VSCodium",
        configs: &["Library/Application Support/VSCodium/"],
        tap: None,
        install: Some("brew install --cask vscodium"),
        service: None,
        site: None,
    },
    App {
        id: "neovim",
        name: "Neovim",
        configs: &[".config/nvim/"],
        tap: None,
        install: Some("brew install neovim"),
        service: None,
        site: None,
    },
    App {
        id: "btop",
        name: "btop",
        configs: &[".config/btop/"],
        tap: None,
        install: Some("brew install btop"),
        service: None,
        site: None,
    },
    App {
        id: "tmux",
        name: "tmux",
        configs: &[".tmux.conf", ".config/tmux/"],
        tap: None,
        install: Some("brew install tmux"),
        service: None,
        site: None,
    },
    App {
        id: "starship",
        name: "Starship",
        configs: &[".config/starship.toml"],
        tap: None,
        install: Some("brew install starship"),
        service: None,
        site: None,
    },
    App {
        id: "micro",
        name: "micro",
        configs: &[".config/micro/"],
        tap: None,
        install: Some("brew install micro"),
        service: None,
        site: None,
    },
    App {
        id: "herdr",
        name: "herdr",
        configs: &[".config/herdr/"],
        tap: None,
        install: None,
        service: None,
        site: Some("herdr's own website"),
    },
];

/// An app an import needs, with what to run for it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Missing {
    pub id: &'static str,
    pub name: &'static str,
    /// Its own commands (tap, install, start), for a row in a list.
    pub commands: Vec<String>,
    /// Not in Homebrew: where to get it instead.
    pub site: Option<&'static str>,
}

/// Does `rel` (a path relative to home, as an import reports it) belong to `app`?
fn belongs(app: &App, rel: &Path) -> bool {
    let rel = rel.to_string_lossy();
    app.configs.iter().any(|c| match c.strip_suffix('/') {
        Some(dir) => rel == dir || rel.starts_with(c),
        None => rel == *c,
    })
}

/// Is this app on this Mac? Palette's own check for a target, AeroSpace's for the tool.
pub fn installed(paths: &Paths, id: &str) -> bool {
    match id {
        "aerospace" => {
            crate::targets::has_app(paths, "AeroSpace.app")
                || crate::targets::has_program("aerospace")
        }
        _ => crate::targets::get(id).is_some_and(|t| (t.installed)(paths)),
    }
}

/// The apps whose configs are in `written` (paths relative to home) but which `installed`
/// says aren't here, in the table's order (the desktop first).
pub fn missing(written: &[impl AsRef<Path>], installed: impl Fn(&str) -> bool) -> Vec<Missing> {
    APPS.iter()
        .filter(|a| written.iter().any(|p| belongs(a, p.as_ref())))
        .filter(|a| !installed(a.id))
        .map(|a| Missing {
            id: a.id,
            name: a.name,
            commands: a
                .tap
                .into_iter()
                .chain(a.install)
                .chain(a.service)
                .map(String::from)
                .collect(),
            site: a.site,
        })
        .collect()
}

/// Every command for `apps` as one block to paste: each tap once, then the installs, then
/// the services (a service starts only once its program is there).
pub fn script(apps: &[Missing]) -> Vec<String> {
    let find = |id: &str| APPS.iter().find(|a| a.id == id);
    let mut out: Vec<String> = Vec::new();
    let mut push = |line: &str| {
        if !out.iter().any(|l| l == line) {
            out.push(line.to_string());
        }
    };
    for a in apps.iter().filter_map(|m| find(m.id)) {
        if let Some(t) = a.tap {
            push(t);
        }
    }
    for a in apps.iter().filter_map(|m| find(m.id)) {
        if let Some(i) = a.install {
            push(i);
        }
    }
    for a in apps.iter().filter_map(|m| find(m.id)) {
        if let Some(s) = a.service {
            push(s);
        }
    }
    out
}

// ---------------------------------------------------------------- 25b: once they arrive

/// Where an app the import was waiting for stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Waiting {
    /// Not asked yet: the popup offers it once it's installed.
    Ask,
    /// "Not now": no popup again; Settings → Apps keeps a row.
    Later,
}

/// An app from an import that is installed now and can be set up.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Arrived {
    pub id: &'static str,
    pub name: &'static str,
    pub state: Waiting,
    /// What starts it in the background (to copy; Palette never runs it).
    pub start: Option<&'static str>,
}

fn waiting_file(paths: &Paths) -> PathBuf {
    paths.root().join("import-waiting.toml")
}

fn load_waiting(paths: &Paths) -> BTreeMap<String, Waiting> {
    let Ok(text) = fs::read_to_string(waiting_file(paths)) else {
        return BTreeMap::new();
    };
    let Ok(table) = text.parse::<toml::Table>() else {
        return BTreeMap::new();
    };
    table
        .iter()
        .filter_map(|(k, v)| {
            let w = match v.as_str()? {
                "ask" => Waiting::Ask,
                "later" => Waiting::Later,
                _ => return None,
            };
            Some((k.clone(), w))
        })
        .collect()
}

fn save_waiting(paths: &Paths, waiting: &BTreeMap<String, Waiting>) -> Result<()> {
    let path = waiting_file(paths);
    if waiting.is_empty() {
        let _ = fs::remove_file(&path);
        return Ok(());
    }
    let mut text = String::from(
        "# Apps an import brought configs for, not installed then (`palette imported`).\n\
         # \"ask\" = offered once it's installed; \"later\" = you said not now.\n",
    );
    for (id, w) in waiting {
        let w = match w {
            Waiting::Ask => "ask",
            Waiting::Later => "later",
        };
        text.push_str(&format!("{id} = \"{w}\"\n"));
    }
    fs::create_dir_all(paths.root())?;
    fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
}

/// After an import: remember the missing apps Palette can set up later (those it themes).
pub fn remember(paths: &Paths, apps: &[Missing]) -> Result<()> {
    let mut waiting = load_waiting(paths);
    for m in apps.iter().filter(|m| crate::targets::get(m.id).is_some()) {
        waiting.insert(m.id.to_string(), Waiting::Ask);
    }
    save_waiting(paths, &waiting)
}

/// The waiting apps that are installed now (`installed` says which).
pub fn arrived_with(paths: &Paths, installed: impl Fn(&str) -> bool) -> Vec<Arrived> {
    load_waiting(paths)
        .into_iter()
        .filter_map(|(id, state)| {
            let app = APPS.iter().find(|a| a.id == id)?;
            installed(app.id).then_some(Arrived {
                id: app.id,
                name: app.name,
                state,
                start: app.service,
            })
        })
        .collect()
}

/// Forget an app (set up), or move it to "later" (not now).
pub fn answer(paths: &Paths, id: &str, later: bool) -> Result<()> {
    let mut waiting = load_waiting(paths);
    if later {
        if let Some(w) = waiting.get_mut(id) {
            *w = Waiting::Later;
        }
    } else {
        waiting.remove(id);
    }
    save_waiting(paths, &waiting)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn written(list: &[&str]) -> Vec<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn only_apps_with_configs_and_not_installed() {
        let w = written(&[
            ".config/sketchybar/sketchybarrc",
            ".config/sketchybar/plugins/battery.sh",
            ".config/borders/bordersrc",
            ".aerospace.toml",
            "Library/Application Support/com.mitchellh.ghostty/config",
            ".config/palette/settings.toml",
        ]);
        // Ghostty is here; the rest isn't.
        let m = missing(&w, |id| id == "ghostty");
        let names: Vec<_> = m.iter().map(|m| m.name).collect();
        assert_eq!(names, ["SketchyBar", "JankyBorders", "AeroSpace"]);
        // battery.sh must not count as bat, nor sketchybar's folder as anything else.
        assert_eq!(
            m[0].commands,
            [
                "brew tap FelixKratz/formulae",
                "brew install sketchybar",
                "brew services start sketchybar"
            ]
        );
        assert_eq!(
            script(&m),
            [
                "brew tap FelixKratz/formulae",
                "brew install sketchybar",
                "brew install borders",
                "brew install --cask nikitabobko/tap/aerospace",
                "brew services start sketchybar",
                "brew services start borders",
            ]
        );
    }

    #[test]
    fn everything_installed_means_nothing_to_do() {
        let w = written(&[".config/kitty/kitty.conf", ".tmux.conf"]);
        assert!(missing(&w, |_| true).is_empty());
        assert!(script(&[]).is_empty());
    }

    #[test]
    fn waiting_apps_are_offered_once_installed() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(dir.path());
        let w = written(&[".config/sketchybar/sketchybarrc", ".aerospace.toml"]);
        remember(&paths, &missing(&w, |_| false)).unwrap();
        // AeroSpace has no Palette line to add: it isn't kept.
        assert!(
            arrived_with(&paths, |_| true)
                .iter()
                .all(|a| a.id != "aerospace")
        );
        assert!(
            arrived_with(&paths, |_| false).is_empty(),
            "not installed yet"
        );
        let a = arrived_with(&paths, |_| true);
        assert_eq!(a.len(), 1);
        assert_eq!((a[0].id, a[0].state), ("sketchybar", Waiting::Ask));
        assert_eq!(a[0].start, Some("brew services start sketchybar"));
        answer(&paths, "sketchybar", true).unwrap();
        assert_eq!(arrived_with(&paths, |_| true)[0].state, Waiting::Later);
        answer(&paths, "sketchybar", false).unwrap();
        assert!(arrived_with(&paths, |_| true).is_empty());
        assert!(!waiting_file(&paths).exists());
    }

    #[test]
    fn not_in_homebrew_says_where() {
        let w = written(&[".config/herdr/config.toml", ".config/tmuxinator/x.yml"]);
        let m = missing(&w, |_| false);
        assert_eq!(m.len(), 1, "tmuxinator isn't tmux: {m:?}");
        assert!(m[0].commands.is_empty() && m[0].site.is_some());
        assert!(script(&m).is_empty());
    }
}
