//! Habit Tracker: a colours file the app reads itself, when Settings → Integrations →
//! "Follow Palette" is on (off by default). Nothing in the app's own config is touched.
//!
//! JSON rather than CSS: CSS can `@import` from the web, and the app accepts only plain
//! `#rrggbb` values from this file. The app checks the file's date every couple of seconds
//! while following, so a switch shows up live without a reload step here.

use super::{BASE, Output, Target};
use crate::system::Outcome;
use crate::wiring::Wiring;

pub const TARGET: Target = Target {
    name: "habit-tracker",
    output: Some(Output {
        file: "habit-tracker.json",
        template: include_str!("template.json"),
        // Data only, but keep it rendered from the colours so it always has every key.
        overridable: false,
        finish: None,
    }),
    // No config to wire: the app reads the file. (Same as bat, shown as part of the shell.)
    wiring: Wiring::Shell,
    installed: |paths| super::has_app(paths, "Habit Tracker.app"),
    reload: |_| Outcome::Unchanged("the app follows the file live".into()),
    ..BASE
};
