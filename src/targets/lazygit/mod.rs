use super::{BASE, Output, Target};
use crate::paths::Paths;
use crate::system::Outcome;
use crate::wiring::Wiring;

pub const TARGET: Target = Target {
    name: "lazygit",
    output: Some(Output {
        file: "lazygit.yml",
        template: include_str!("template.yml"),
        // lazygit configs can define custom commands.
        overridable: false,
        finish: None,
    }),
    wiring: Wiring::Shell,
    installed: |_| super::has_program("lazygit"),
    reload: |_| Outcome::Unchanged("used on next run".into()),
    env,
    ..BASE
};

/// `LG_CONFIG_FILE` takes a comma-separated list, later files win. It
/// replaces lazygit's default location, so the user's own config (if any)
/// goes first. Decided when the shell starts, so a new config needs a new shell.
fn env(paths: &Paths) -> Vec<String> {
    let ours = paths.for_shell(&paths.current().join("lazygit.yml"));
    vec![
        "_palette_lg=\"${XDG_CONFIG_HOME:-$HOME/Library/Application Support}/lazygit/config.yml\""
            .into(),
        format!(
            "if [ -f \"$_palette_lg\" ]; then export LG_CONFIG_FILE=\"$_palette_lg,{ours}\"; \
             else export LG_CONFIG_FILE=\"{ours}\"; fi"
        ),
        "unset _palette_lg".into(),
    ]
}
