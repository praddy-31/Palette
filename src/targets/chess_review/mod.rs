//! Chess Review: a colours file the app reads itself, when Settings → Appearance →
//! "Follow Palette's theme" is on. Nothing in the app's own config is touched.
//!
//! The same keys as the Habit Tracker's file. The app accepts only plain `#rrggbb`
//! values, works out its own shades and keeps the board and move-label colours its own.
//! It checks the file's date every 2 s while following, so no reload step here.

use super::{BASE, Output, Target};
use crate::system::Outcome;
use crate::wiring::Wiring;

pub const TARGET: Target = Target {
    name: "chess-review",
    output: Some(Output {
        file: "chess-review.json",
        template: include_str!("template.json"),
        // Data only, but keep it rendered from the colours so it always has every key.
        overridable: false,
        finish: None,
    }),
    // No config to wire: the app reads the file.
    wiring: Wiring::Shell,
    installed: |paths| super::has_app(paths, "Chess Review.app"),
    reload: |_| Outcome::Unchanged("the app follows the file live".into()),
    platforms: super::Platforms::Mac,
    ..BASE
};
