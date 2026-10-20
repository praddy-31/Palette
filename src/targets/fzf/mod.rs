use super::{BASE, Note, Output, Target};
use crate::paths::Paths;
use crate::system::{self, Outcome};
use crate::wiring::Wiring;

pub const TARGET: Target = Target {
    name: "fzf",
    output: Some(Output {
        file: "fzf.opts",
        template: include_str!("template.opts"),
        // fzf options can run commands (`--bind 'x:execute(…)'`, `--preview`).
        overridable: false,
        finish: None,
    }),
    wiring: Wiring::Shell,
    installed: |_| super::has_program("fzf"),
    reload: |_| Outcome::Unchanged("used on next run".into()),
    env,
    doctor,
    ..BASE
};

/// fzf reads this file on every run (fzf 0.48+), so open shells follow
/// switches without re-sourcing anything.
fn env(paths: &Paths) -> Vec<String> {
    let file = paths.for_shell(&paths.current().join("fzf.opts"));
    vec![format!("export FZF_DEFAULT_OPTS_FILE=\"{file}\"")]
}

fn doctor(_: &Paths) -> Vec<Note> {
    let version = system::run("fzf", &["--version"])
        .ok()
        .and_then(|v| system::parse_version(&v));
    match version {
        Some(v) if v < (0, 48) => vec![Note::fix(
            "fzf older than 0.48 ignores FZF_DEFAULT_OPTS_FILE; update it",
        )],
        _ => Vec::new(),
    }
}
