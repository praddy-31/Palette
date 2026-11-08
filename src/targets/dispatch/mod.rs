//! Dispatch: a colours file the app reads itself (`theme.rs` in Dispatch looks for
//! `dispatch.json` first). Nothing in the app's own config is touched.
//!
//! Its own file, not the Habit Tracker's: an app that borrows another app's file breaks
//! when that app's target is off, missing on this system, or changes its format. The keys
//! are the same as the Habit Tracker's; Dispatch takes only plain `#rrggbb` values and
//! checks the file's date every couple of seconds, so no reload step here.
//!
//! `glass` carries the window look from `palette glass dispatch` (kind, opacity, blur), as for
//! the terminals. Kind "own" means Dispatch's own Settings → Look decides.

use super::{BASE, Output, Target};
use crate::system::Outcome;
use crate::wiring::Wiring;

pub const TARGET: Target = Target {
    name: "dispatch",
    output: Some(Output {
        file: "dispatch.json",
        template: include_str!("template.json"),
        // Data only, but keep it rendered from the colours so it always has every key.
        overridable: false,
        finish: None,
    }),
    // No config to wire: the app reads the file.
    wiring: Wiring::Shell,
    installed: |paths| super::has_app(paths, "Dispatch.app"),
    reload: |_| Outcome::Unchanged("the app follows the file live".into()),
    platforms: super::Platforms::Mac,
    ..BASE
};
