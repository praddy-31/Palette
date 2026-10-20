//! `palette doctor`: is everything hooked up? Read-only: it looks at files
//! and asks programs for their version, and never changes anything.

use std::fs;

use anyhow::Result;

use crate::paths::Paths;
use crate::targets::{self, Note, Target};
use crate::wiring::{self, Wiring};
use crate::{state, theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    /// Not applicable here (app not installed). Not a problem.
    Off,
    /// Something to fix; the detail says how.
    Fix,
}

#[derive(Debug)]
pub struct Row {
    pub name: String,
    pub status: Status,
    pub detail: String,
    /// Target-specific findings (versions, stale copies, …).
    pub notes: Vec<Note>,
}

impl Row {
    fn new(name: &str, status: Status, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status,
            detail: detail.into(),
            notes: Vec::new(),
        }
    }

    /// Does this row (or one of its notes) need fixing?
    pub fn needs_fix(&self) -> bool {
        self.status == Status::Fix || self.notes.iter().any(|n| !n.ok)
    }
}

pub fn run(paths: &Paths) -> Result<Vec<Row>> {
    let mut rows = vec![theme_row(paths)?];
    for target in targets::ALL {
        rows.push(target_row(paths, target));
    }
    if let Some(row) = command_row() {
        rows.push(row);
    }
    let backups = fs::read_dir(paths.backups()).map_or(0, |d| d.flatten().count());
    rows.push(Row::new(
        "backups",
        Status::Ok,
        format!("{backups} kept in {}", paths.backups().display()),
    ));
    Ok(rows)
}

/// Active theme, and are all its generated files there?
fn theme_row(paths: &Paths) -> Result<Row> {
    let Some(name) = state::current_theme(paths)? else {
        return Ok(Row::new(
            "theme",
            Status::Fix,
            "none set: run `palette set <theme>`",
        ));
    };
    let missing: Vec<&str> = targets::ALL
        .iter()
        .filter_map(|t| t.output.as_ref())
        .map(|o| o.file)
        .filter(|f| !paths.current().join(f).is_file())
        .collect();
    if !missing.is_empty() {
        return Ok(Row::new(
            "theme",
            Status::Fix,
            format!(
                "{name}, but {} missing: run `palette refresh`",
                missing.join(", ")
            ),
        ));
    }
    let mut row = Row::new("theme", Status::Ok, name.clone());
    match theme::load(paths, &name) {
        Ok(t) => {
            let n = t.backgrounds().map_or(0, |b| b.len());
            let showing = crate::effects::current_wallpaper(paths)
                .filter(|p| p.starts_with(t.backgrounds_dir()))
                .and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()));
            let plural = if n == 1 { "" } else { "s" };
            row.notes.push(Note::ok(match showing {
                Some(f) => format!("{n} wallpaper{plural}, showing {f}"),
                None => format!("{n} wallpaper{plural}"),
            }));
            // The colour check (`palette check`): information, since the
            // theme's author chose the colours; each line says what to try.
            let report = crate::check::theme(&t);
            row.notes.push(Note::info(format!(
                "colours: {}",
                report.verdict().trim_start_matches(&format!("{name}: "))
            )));
            for issue in &report.issues {
                row.notes
                    .push(Note::info(format!("{}. {}", issue.summary(), issue.fix())));
            }
            if !report.issues.is_empty() {
                row.notes.push(Note::info(
                    "apply the suggestions in Settings → Colours, or `palette colours --fix`",
                ));
            }
        }
        Err(e) => row
            .notes
            .push(Note::fix(format!("can't load it any more: {e:#}"))),
    }
    Ok(row)
}

fn target_row(paths: &Paths, target: &Target) -> Row {
    let settings = crate::settings::load(paths).unwrap_or_default();
    if !settings.is_on(target.name) {
        return Row::new(target.name, Status::Off, crate::effects::OFF);
    }
    let installed = (target.installed)(paths);
    let wired = wiring::is_wired(paths, target);
    let mut row = match &target.wiring {
        Wiring::Shell if !installed => Row::new(target.name, Status::Off, "not installed"),
        Wiring::Shell if wired => Row::new(target.name, Status::Ok, "set up through shell.sh"),
        Wiring::Import(import) if wired => {
            let configs: Vec<_> = (import.configs)(paths)
                .into_iter()
                .filter(|p| p.is_file())
                .collect();
            let unwired = configs
                .iter()
                .filter(|p| {
                    let line = wiring::import_line(paths, target, import);
                    !fs::read_to_string(p).is_ok_and(|s| wiring::has_block(&s, &line))
                })
                .count();
            if unwired > 0 {
                Row::new(
                    target.name,
                    Status::Fix,
                    format!(
                        "{} of {} config files wired: run `palette init` for the rest",
                        configs.len() - unwired,
                        configs.len()
                    ),
                )
            } else {
                Row::new(
                    target.name,
                    Status::Ok,
                    format!("wired ({} file{})", configs.len(), plural(configs.len())),
                )
            }
        }
        Wiring::Setting(_) if wired => Row::new(target.name, Status::Ok, "wired (setting)"),
        _ if !installed => Row::new(target.name, Status::Off, "not installed"),
        _ if !wiring::has_config(paths, target) => Row::new(
            target.name,
            Status::Off,
            "installed, but no config file to wire",
        ),
        _ => Row::new(
            target.name,
            Status::Fix,
            "installed, not wired: run `palette init`",
        ),
    };
    if row.status != Status::Off {
        row.notes = (target.doctor)(paths);
    }
    row
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// Several `palette` commands on PATH (an old `cargo install` next to the
/// app's copy) means the terminal may run an older build that doesn't know
/// newer apps. Shells also remember where they found a command until
/// `rehash` or a new terminal.
fn command_row() -> Option<Row> {
    let found = crate::system::find_all_programs("palette");
    match found.as_slice() {
        [] => None,
        [one] => Some(Row::new("command", Status::Ok, one.display().to_string())),
        [first, rest @ ..] => {
            let mut row = Row::new(
                "command",
                Status::Fix,
                format!(
                    "{} palette commands; {} runs first",
                    found.len(),
                    first.display()
                ),
            );
            for other in rest {
                row.notes.push(Note::fix(format!(
                    "also {}: remove the one you don't use (`cargo uninstall palette` for ~/.cargo/bin), then `rehash`",
                    other.display()
                )));
            }
            Some(row)
        }
    }
}
