//! bat has no file of its own: its built-in `ansi` theme uses the terminal's
//! 16 colours, which Palette already sets. No theme cache to rebuild per switch.

use super::{BASE, Target};
use crate::paths::Paths;
use crate::system::Outcome;
use crate::wiring::Wiring;

pub const TARGET: Target = Target {
    name: "bat",
    wiring: Wiring::Shell,
    installed: |_| super::has_program("bat"),
    reload: |_| Outcome::Unchanged("follows the terminal colours".into()),
    env,
    ..BASE
};

fn env(_: &Paths) -> Vec<String> {
    vec!["export BAT_THEME=\"ansi\"".into()]
}
