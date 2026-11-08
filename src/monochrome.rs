//! Monochrome themes (ROADMAP 11f, colour lab idea 13): themes whose twelve terminal colours
//! are all grey (ash, frost, …) make a diff's red and green, or `ls`'s blue and cyan, look the
//! same. Two switches in Settings → General, run after the readability fixes when a theme is
//! used (`theme::load_fixed`), never on a key you edited or a theme left original:
//!
//! - **Spaced greys** (on by default): each group of six terminal colours is spread evenly in
//!   lightness, keeping its order, so no two look alike.
//! - **Tint** (off by default): the terminal colours take a faint version of the theme's own
//!   hue (the accent's, else the background's), with red nudged warm and green cool so diffs
//!   still read as diffs. A theme with no hue at all only gets red and green tinted.

use std::collections::BTreeSet;

use crate::colour::{self, Lch};
use crate::normalise::Change;
use crate::theme::Colors;

const ANSI: [&str; 6] = ["red", "green", "yellow", "blue", "magenta", "cyan"];
pub(crate) const BRIGHT: [&str; 6] = [
    "bright_red",
    "bright_green",
    "bright_yellow",
    "bright_blue",
    "bright_magenta",
    "bright_cyan",
];

/// Below this chroma (OKLCH) a colour reads as grey.
const GREY: f32 = 0.04;
/// The tint's chroma: enough to tell red from green, faint enough to stay monochrome.
const TINT: f32 = 0.06;
/// How far red and green turn from the theme's hue towards red (25°) and green (145°).
const NUDGE: f32 = 40.0;

/// The two switches (`settings.toml`: `monochrome_greys`, `monochrome_tint`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mono {
    pub greys: bool,
    pub tint: bool,
}

impl Default for Mono {
    fn default() -> Self {
        Mono {
            greys: true,
            tint: false,
        }
    }
}

/// Monochrome: the twelve terminal colours are all (nearly) grey.
pub fn is_monochrome(c: &Colors) -> bool {
    ANSI.iter()
        .chain(&BRIGHT)
        .all(|k| Lch::from_hex(c.get(k)).c < GREY)
}

/// Both steps as switched on, in order (greys, then the tint keeps their lightness).
/// Nothing happens to a theme that isn't monochrome.
pub fn apply(c: &mut Colors, mono: Mono, keep: &BTreeSet<String>) -> Vec<Change> {
    let mut out = Vec::new();
    if !is_monochrome(c) {
        return out;
    }
    if mono.greys {
        spaced_greys(c, keep, &mut out);
    }
    if mono.tint {
        tint(c, keep, &mut out);
    }
    out
}

fn set(
    c: &mut Colors,
    keep: &BTreeSet<String>,
    out: &mut Vec<Change>,
    key: &'static str,
    to: String,
    why: &str,
) {
    let from = c.get(key).to_string();
    if from != to && !keep.contains(key) {
        c.set(key, &to);
        out.push(Change {
            key,
            from,
            to,
            why: format!("monochrome: {why}"),
        });
    }
}

/// Spread each group of six evenly in lightness, keeping its order. The range runs from the
/// first lightness that reads 3:1 on the background to the text's, widened to at least 0.05
/// per step where there is room. A group already 0.04 apart is left alone.
fn spaced_greys(c: &mut Colors, keep: &BTreeSet<String>, out: &mut Vec<Change>) {
    let bg = c.get("background").to_string();
    let bg_l = Lch::from_hex(&bg).l;
    // Work in distance from the background (d), so dark and light themes are the same sum.
    let sign = if bg_l < 0.5 { 1.0 } else { -1.0 };
    let d_of = |x: f32| (x - bg_l) * sign;
    // Stop short of pure white/black: near them, 8-bit colour rounds steps to one value.
    let d_lim = if sign > 0.0 { 1.0 - bg_l } else { bg_l } - 0.06;
    let grey = |d: f32| {
        Lch {
            l: bg_l + sign * d,
            c: 0.0,
            h: 0.0,
        }
        .to_hex()
    };
    // The first distance at which a grey reads 3:1 on the background.
    let d_read = (0..=200)
        .map(|i| i as f32 / 200.0 * d_lim)
        .find(|d| colour::contrast(&grey(*d), &bg) >= 3.0)
        .unwrap_or(d_lim * 0.5);
    let need = 0.25_f32.min(d_lim - d_read);
    for group in [ANSI, BRIGHT] {
        let mut keys: Vec<(&'static str, f32)> = group
            .iter()
            .map(|k| (*k, d_of(Lch::from_hex(c.get(k)).l)))
            .collect();
        keys.sort_by(|a, b| a.1.total_cmp(&b.1));
        if keys.windows(2).all(|w| w[1].1 - w[0].1 >= 0.04) {
            continue;
        }
        // From the nearest to the farthest original, widened to `need`: first away from
        // the background, then towards it, never closer than readable or past the limit.
        let mut lo = keys[0].1.max(d_read);
        let mut hi = keys[5].1.max(lo).min(d_lim);
        if hi - lo < need {
            hi = (lo + need).min(d_lim);
        }
        if hi - lo < need {
            lo = (hi - need).max(d_read);
        }
        for (i, (key, _)) in keys.iter().enumerate() {
            let d = lo + (hi - lo) * i as f32 / 5.0;
            let to = Lch {
                l: (bg_l + sign * d).clamp(0.0, 1.0),
                ..Lch::from_hex(c.get(key))
            }
            .to_hex();
            set(
                c,
                keep,
                out,
                key,
                to,
                "spaced so each terminal colour differs",
            );
        }
    }
}

/// `from` turned towards `to` by at most `max` degrees, the short way round.
fn towards(from: f32, to: f32, max: f32) -> f32 {
    let d = (to - from + 540.0).rem_euclid(360.0) - 180.0;
    (from + d.clamp(-max, max)).rem_euclid(360.0)
}

/// The theme's hue: the accent's, else the background's, else none (a pure grey theme).
fn theme_hue(c: &Colors) -> Option<f32> {
    ["accent", "background"]
        .iter()
        .map(|k| Lch::from_hex(c.get(k)))
        .find(|x| x.c >= 0.02)
        .map(|x| x.h)
}

/// A faint theme hue on every terminal colour (lightness kept); red nudged warm and green
/// cool. Without a theme hue, only red and green get a plain red and green tint.
fn tint(c: &mut Colors, keep: &BTreeSet<String>, out: &mut Vec<Change>) {
    let base = theme_hue(c);
    for key in ANSI.iter().chain(&BRIGHT) {
        let red = key.ends_with("red");
        let green = key.ends_with("green");
        let hue = match (base, red, green) {
            (Some(h), true, _) => towards(h, 25.0, NUDGE),
            (Some(h), _, true) => towards(h, 145.0, NUDGE),
            (Some(h), _, _) => h,
            (None, true, _) => 25.0,
            (None, _, true) => 145.0,
            (None, _, _) => continue,
        };
        let to = Lch {
            c: TINT,
            h: hue,
            ..Lch::from_hex(c.get(key))
        }
        .to_hex();
        let why = if red || green {
            "a faint tint of the theme's hue, nudged so diffs read as diffs"
        } else {
            "a faint tint of the theme's hue"
        };
        set(c, keep, out, key, to, why);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ash() -> Colors {
        crate::catalog::entries()
            .iter()
            .find(|e| e.name == "ash")
            .expect("ash is in the catalogue")
            .colors
            .clone()
    }

    #[test]
    fn hue_turns_the_short_way_and_stops_at_the_nudge() {
        assert_eq!(towards(260.0, 25.0, 40.0), 300.0); // blue → magenta-ish, not backwards
        assert_eq!(towards(260.0, 145.0, 40.0), 220.0);
        assert_eq!(towards(30.0, 25.0, 40.0), 25.0); // close enough: lands on it
        assert_eq!(towards(350.0, 25.0, 40.0), 25.0); // across 0°
    }

    #[test]
    fn spaced_greys_tell_every_terminal_colour_apart() {
        let mut c = ash();
        let ch = apply(&mut c, Mono::default(), &BTreeSet::new());
        assert!(!ch.is_empty());
        for group in [ANSI, BRIGHT] {
            let mut ls: Vec<f32> = group.iter().map(|k| Lch::from_hex(c.get(k)).l).collect();
            ls.sort_by(f32::total_cmp);
            assert!(ls.windows(2).all(|w| w[1] - w[0] >= 0.035), "{ls:?}");
        }
        assert!(is_monochrome(&c), "greys stay grey");
    }

    #[test]
    fn the_tint_follows_the_theme_and_tells_red_from_green() {
        let mut c = ash();
        c.set("accent", "#7a8fd0"); // a blue accent on an otherwise grey theme
        apply(
            &mut c,
            Mono {
                greys: true,
                tint: true,
            },
            &BTreeSet::new(),
        );
        let red = Lch::from_hex(c.get("red"));
        let green = Lch::from_hex(c.get("green"));
        let blue = Lch::from_hex(c.get("blue"));
        assert!(red.c > 0.04 && green.c > 0.04);
        assert!(
            (blue.h - Lch::from_hex("#7a8fd0").h).abs() < 8.0,
            "blue takes the accent's hue"
        );
        assert!(towards(red.h, green.h, 360.0) != red.h && (red.h - green.h).abs() > 60.0);
    }

    #[test]
    fn a_hueless_theme_only_tints_red_and_green() {
        let mut c = ash();
        c.set("accent", "#808080");
        c.set("background", "#101010");
        apply(
            &mut c,
            Mono {
                greys: false,
                tint: true,
            },
            &BTreeSet::new(),
        );
        assert!(Lch::from_hex(c.get("red")).c > 0.04);
        assert!(Lch::from_hex(c.get("blue")).c < GREY);
    }

    #[test]
    fn edits_and_colourful_themes_are_left_alone() {
        let mut c = ash();
        let keep = BTreeSet::from(["green".to_string()]);
        let before = c.get("green").to_string();
        apply(
            &mut c,
            Mono {
                greys: true,
                tint: true,
            },
            &keep,
        );
        assert_eq!(c.get("green"), before);

        let mut dracula = crate::catalog::entries()
            .iter()
            .find(|e| e.name == "dracula")
            .map(|e| e.colors.clone())
            .unwrap_or_else(|| {
                let mut c = ash();
                c.set("red", "#ff5555");
                c
            });
        assert!(
            apply(
                &mut dracula,
                Mono {
                    greys: true,
                    tint: true
                },
                &BTreeSet::new()
            )
            .is_empty()
        );
    }
}
