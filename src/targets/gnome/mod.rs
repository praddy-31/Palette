//! GNOME (Ubuntu's default desktop): the accent colour. GNOME 47+ has nine
//! accent presets (`org.gnome.desktop.interface accent-color`), so the theme's
//! accent becomes the nearest by hue; a near-grey accent becomes slate.
//! Wallpaper and light/dark are `system::set_wallpaper` / `set_dark_mode`.
//! Nothing in the user's own config is touched: it is one GNOME setting.

use super::{BASE, Output, Platforms, Target};
use crate::colour::Lch;
use crate::paths::Paths;
use crate::system::session::Session;
use crate::system::{self, Outcome};
use crate::wiring::Wiring;

pub const TARGET: Target = Target {
    name: "gnome",
    output: Some(Output {
        file: "gnome.ini",
        template: include_str!("template.ini"),
        overridable: false,
        finish: None,
    }),
    wiring: Wiring::Direct,
    installed: |_| super::has_program("gnome-shell"),
    reload,
    platforms: Platforms::Linux,
    ..BASE
};

/// GNOME's accent presets and their colours (libadwaita's `accent_bg_color`).
const PRESETS: [(&str, &str); 9] = [
    ("blue", "#3584e4"),
    ("teal", "#2190a4"),
    ("green", "#3a944a"),
    ("yellow", "#c88800"),
    ("orange", "#ed5b00"),
    ("red", "#e62d42"),
    ("pink", "#d56199"),
    ("purple", "#9141ac"),
    ("slate", "#6f8396"),
];

/// Below this chroma an accent reads as grey: slate.
const GREY: f32 = 0.04;

/// The preset nearest an accent by hue (lightness doesn't matter: GNOME
/// shades the preset itself for light and dark).
pub fn nearest_preset(accent: &str) -> &'static str {
    let want = Lch::from_hex(accent);
    if want.c < GREY {
        return "slate";
    }
    let hue_gap = |hex: &str| {
        let d = (Lch::from_hex(hex).h - want.h).rem_euclid(360.0);
        d.min(360.0 - d)
    };
    PRESETS
        .iter()
        .filter(|(name, _)| *name != "slate")
        .min_by(|a, b| hue_gap(a.1).total_cmp(&hue_gap(b.1)))
        .map_or("blue", |(name, _)| name)
}

fn reload(paths: &Paths) -> Outcome {
    if Session::detect() != Session::Gnome {
        return Outcome::Skipped("not the desktop you're logged into".into());
    }
    let text = std::fs::read_to_string(paths.current().join("gnome.ini")).unwrap_or_default();
    let Some(accent) = text.lines().find_map(|l| l.strip_prefix("accent=")) else {
        return Outcome::Failed("gnome.ini has no accent".into());
    };
    let preset = nearest_preset(accent.trim());
    match system::run(
        "gsettings",
        &["set", "org.gnome.desktop.interface", "accent-color", preset],
    ) {
        Ok(_) => Outcome::Done(format!("accent {preset}")),
        Err(e) => Outcome::Failed(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accents_map_to_the_nearest_preset() {
        assert_eq!(nearest_preset("#7aa2f7"), "blue"); // Tokyo Night
        assert_eq!(nearest_preset("#a6e3a1"), "green"); // Catppuccin green
        assert_eq!(nearest_preset("#f38ba8"), "pink"); // Catppuccin red-pink
        assert_eq!(nearest_preset("#cba6f7"), "purple"); // Catppuccin mauve
        assert_eq!(nearest_preset("#888888"), "slate");
        for (name, hex) in PRESETS {
            assert_eq!(nearest_preset(hex), name, "{hex}");
        }
    }
}
