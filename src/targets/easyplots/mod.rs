//! EasyPlots: a colours file its launcher reads itself when `[launcher] theme = "palette"`
//! (on by default; `ui_theme.rs` in EasyPlots looks for `easyplots.json` first). Nothing in
//! the app's own config is touched.
//!
//! Its own file, not the Habit Tracker's: that one is Mac-only, while EasyPlots also runs
//! on Linux, and an app borrowing another's file breaks when that target changes. Same
//! keys as the Habit Tracker's; plain `#rrggbb` only, rechecked every 2 s by the app.
//!
//! `glass` carries the launcher's window look from `palette glass easyplots` (kind, opacity,
//! blur), as for Dispatch. Kind "own" means EasyPlots' own Settings → Launcher decides. The
//! file is read whatever colours the launcher uses, so the glass follows Palette even with a
//! built-in launcher theme.

use super::{BASE, Output, Target};
use crate::system::Outcome;
use crate::wiring::Wiring;

pub const TARGET: Target = Target {
    name: "easyplots",
    output: Some(Output {
        file: "easyplots.json",
        template: include_str!("template.json"),
        // Data only, but keep it rendered from the colours so it always has every key.
        overridable: false,
        finish: None,
    }),
    // No config to wire: the app reads the file.
    wiring: Wiring::Shell,
    installed: |paths| super::has_app(paths, "EasyPlots.app"),
    reload: |_| Outcome::Unchanged("the app follows the file live".into()),
    ..BASE
};
