//! herdr in the terminal (`~/.config/herdr/config.toml`). It has no include line, but
//! its built-in theme `terminal` draws herdr's panels from the host terminal's colours,
//! which Palette already sets. So `init` sets one key, `[theme] name = "terminal"`, and
//! a switch only asks a running herdr to read its config again. Your old value is
//! remembered and put back by `init --remove` (owner's pick, 2026-11-15).

use std::path::PathBuf;

use toml_edit::{DocumentMut, Item};

use super::{BASE, Target};
use crate::paths::Paths;
use crate::system::{self, Outcome};
use crate::wiring::{Setting, Wiring};

pub const TARGET: Target = Target {
    name: "herdr",
    wiring: Wiring::Setting(Setting {
        configs,
        per_file: false,
        value: |_| "terminal".into(),
        read,
        write,
        prepare: None,
        create: false,
    }),
    installed: |_| super::has_program("herdr"),
    reload,
    ..BASE
};

fn configs(paths: &Paths) -> Vec<PathBuf> {
    vec![paths.config_home().join("herdr/config.toml")]
}

fn read(text: &str) -> Option<String> {
    let doc: DocumentMut = text.parse().ok()?;
    doc.get("theme")?.get("name")?.as_str().map(str::to_string)
}

/// Set `[theme] name`, or (`None`) take it out, and the `[theme]` table too if that
/// leaves it empty. Everything else (comments, other keys) stays.
fn write(text: &str, value: Option<&str>) -> String {
    let Ok(mut doc) = text.parse::<DocumentMut>() else {
        return text.to_string(); // not TOML: leave it alone
    };
    match value {
        Some(v) => {
            if doc.get("theme").is_none() {
                // A `[theme]` section, as herdr's docs show it (not `theme = { … }`).
                doc.insert("theme", Item::Table(toml_edit::Table::new()));
            }
            doc["theme"]["name"] = toml_edit::value(v);
        }
        None => {
            if let Some(theme) = doc.get_mut("theme").and_then(Item::as_table_like_mut) {
                theme.remove("name");
                if theme.is_empty() {
                    doc.remove("theme");
                }
            }
        }
    }
    doc.to_string()
}

fn reload(_: &Paths) -> Outcome {
    if !system::is_running("herdr") {
        return Outcome::Skipped("not running".into());
    }
    match system::run("herdr", &["server", "reload-config"]) {
        Ok(_) => Outcome::Done("follows the terminal's colours".into()),
        Err(e) => Outcome::Failed(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_and_takes_out_the_theme_name() {
        let mine = "onboarding = false\n";
        let wired = write(mine, Some("terminal"));
        assert_eq!(read(&wired).as_deref(), Some("terminal"));
        assert!(wired.starts_with("onboarding = false\n"));
        assert_eq!(write(&wired, None), mine);

        let named = "# mine\n[theme]\nname = \"nord\" # cold\nauto_switch = true\n";
        assert_eq!(read(named).as_deref(), Some("nord"));
        let wired = write(named, Some("terminal"));
        assert!(wired.contains("auto_switch = true") && wired.contains("# mine"));
        assert_eq!(read(&write(&wired, Some("nord"))).as_deref(), Some("nord"));
        assert_eq!(write("not = [toml", Some("terminal")), "not = [toml");
    }
}
