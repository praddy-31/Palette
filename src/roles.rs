//! Colour lab prototype (2026-11-06, IDEAS 9-15): not used by Palette yet. The owner reviews
//! its before/after demo (`cargo run --example colour_lab`, docs/reports/colour-lab/) first;
//! what passes moves into `normalise.rs` / `install.rs`.
//!
//! - [`rank_roles`] (idea 9): themes that don't name their colours (only the 16 terminal
//!   colours, or an Alacritty file) get their *guessed* roles re-chosen from their own colours,
//!   by rank and rules (shades darker than the background, comments < secondary text < text, …).
//! - [`reuse`] (idea 10): Palette's fixes (`normalise`), but a fixed colour is swapped for one
//!   of the theme's own when one is close (OKLab ΔE ≤ [`REUSE_DE`]) and passes the same rules.
//! - [`spaced_greys`] / [`tint`] (idea 13): now `crate::monochrome` (11f); wrappers for the demo.
//! - [`apps`] (idea 11): per-app verdicts, from what each app actually draws.
//! - [`glass`] / [`fix_glass`] (idea 12): text on a see-through window over the wallpaper.
//! - [`ansi_on_selection`] (idea 14) and [`apca`] (idea 15): measures only.

use std::collections::BTreeSet;

use crate::colour::{self, Lch};
#[cfg(test)]
use crate::monochrome::BRIGHT;
use crate::normalise::{self, Level};
use crate::theme::Colors;

/// One change, for the demo's lists.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub key: &'static str,
    pub from: String,
    pub to: String,
    pub why: String,
}

const ANSI: [&str; 6] = ["red", "green", "yellow", "blue", "magenta", "cyan"];

fn l(hex: &str) -> f32 {
    Lch::from_hex(hex).l
}

fn cr(a: &str, b: &str) -> f32 {
    colour::contrast(a, b)
}

fn set(c: &mut Colors, steps: &mut Vec<Step>, key: &'static str, to: String, why: &str) {
    let from = c.get(key).to_string();
    if from != to {
        c.set(key, &to);
        steps.push(Step {
            key,
            from,
            to,
            why: why.to_string(),
        });
    }
}

/// [`crate::ranking::rank`] for the lab (no edits kept), as demo steps.
pub fn rank_roles(c: &mut Colors) -> Vec<Step> {
    steps(crate::ranking::rank(
        c,
        &BTreeSet::new(),
        crate::ranking::Roles::Guessed,
    ))
}

pub use crate::ranking::{REUSE_DE, order_problems, pool};

/// [`crate::ranking::reuse`] for the lab (no edits kept), as demo steps.
pub fn reuse(c: &mut Colors, level: Level) -> Vec<Step> {
    steps(crate::ranking::reuse(
        c,
        level,
        &BTreeSet::new(),
        crate::ranking::Roles::Guessed,
    ))
}

fn steps(changes: Vec<normalise::Change>) -> Vec<Step> {
    changes
        .into_iter()
        .map(|c| Step {
            key: c.key,
            from: c.from,
            to: c.to,
            why: c.why,
        })
        .collect()
}

/// Monochrome (moved into Palette as `crate::monochrome`, 11f).
pub fn monochrome(c: &Colors) -> bool {
    crate::monochrome::is_monochrome(c)
}

/// Idea 13, option A: [`crate::monochrome`]'s spaced greys, as demo steps.
pub fn spaced_greys(c: &mut Colors) -> Vec<Step> {
    let mono = crate::monochrome::Mono {
        greys: true,
        tint: false,
    };
    steps(crate::monochrome::apply(c, mono, &BTreeSet::new()))
}

/// Idea 13, option B: [`crate::monochrome`]'s tint (the theme's hue since 11f).
pub fn tint(c: &mut Colors) -> Vec<Step> {
    let mono = crate::monochrome::Mono {
        greys: false,
        tint: true,
    };
    steps(crate::monochrome::apply(c, mono, &BTreeSet::new()))
}

/// One app's verdict (idea 11).
#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub app: &'static str,
    /// "ok", "weak" (only thanks to other cues) or "bad".
    pub grade: &'static str,
    pub why: String,
}

/// Idea 11: what each kind of app draws, and whether it works. Terminals (Ghostty, kitty,
/// Alacritty, WezTerm) only have the selection to show where you are; editors (VSCodium,
/// Zed) also have the current-line highlight and find matches; list tools (tmux, lazygit,
/// fzf) put normal text on the selection.
pub fn apps(c: &Colors) -> Vec<Verdict> {
    let bg = c.get("background");
    let sel = c.get("selection");
    let sel_seen = cr(sel, bg) >= 1.3 * normalise::SLACK;
    let mut out = Vec::new();

    let mut t = Vec::new();
    if !sel_seen {
        t.push(format!("selection invisible ({:.2}:1)", cr(sel, bg)));
    }
    if cr(c.get("bright_foreground"), sel) < 4.5 * normalise::SLACK {
        t.push(format!(
            "selected text {:.1}:1",
            cr(c.get("bright_foreground"), sel)
        ));
    }
    let low: Vec<&str> = ANSI
        .iter()
        .copied()
        .filter(|k| cr(c.get(k), bg) < 3.0 * normalise::SLACK)
        .collect();
    if !low.is_empty() {
        t.push(format!("faint: {}", low.join(", ")));
    }
    out.push(Verdict {
        app: "terminals",
        grade: if t.is_empty() { "ok" } else { "bad" },
        why: t.join("; "),
    });

    // VSCodium draws the line highlight as lighter_background at 50 % and find matches as
    // yellow at 40 % (its template), on top of the background.
    let line = colour::mix(bg, c.get("lighter_background"), 0.5);
    let find = colour::mix(bg, c.get("yellow"), 0.4);
    let mut e = Vec::new();
    let mut grade = "ok";
    if !sel_seen {
        if cr(&line, bg) >= 1.1 || cr(&find, bg) >= 1.3 {
            grade = "weak";
            e.push("selection invisible, but the line highlight / find matches show".into());
        } else {
            grade = "bad";
            e.push("selection and line highlight invisible".into());
        }
    }
    if cr(c.get("dark_foreground"), bg) < 2.5 * normalise::SLACK {
        grade = "bad";
        e.push(format!(
            "line numbers {:.1}:1",
            cr(c.get("dark_foreground"), bg)
        ));
    }
    out.push(Verdict {
        app: "editors",
        grade,
        why: e.join("; "),
    });

    let mut li = Vec::new();
    if !sel_seen {
        li.push("selection invisible".to_string());
    }
    if cr(c.get("foreground"), sel) < 3.0 * normalise::SLACK {
        li.push(format!(
            "text on the selection {:.1}:1",
            cr(c.get("foreground"), sel)
        ));
    }
    out.push(Verdict {
        app: "lists",
        grade: if li.is_empty() { "ok" } else { "bad" },
        why: li.join("; "),
    });
    out
}

/// Idea 14: how many of the six terminal colours read under 3:1 on the selection (VSCodium
/// keeps syntax colours on a selection).
pub fn ansi_on_selection(c: &Colors) -> usize {
    ANSI.iter()
        .filter(|k| cr(c.get(k), c.get("selection")) < 3.0)
        .count()
}

/// Idea 15: APCA lightness contrast Lc (APCA-W3 0.0.98G), text on background. Positive for
/// dark text on light, negative for light on dark; |Lc| 75 = body text, 60 = content,
/// 45 = large text, 30 = placeholder, 15 = barely there.
pub fn apca(text: &str, bg: &str) -> f32 {
    let y = |hex: &str| {
        let h = hex.trim_start_matches('#');
        let ch = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0) as f32 / 255.0;
        let y =
            0.2126729 * ch(0).powf(2.4) + 0.7151522 * ch(2).powf(2.4) + 0.0721750 * ch(4).powf(2.4);
        if y < 0.022 {
            y + (0.022 - y).powf(1.414)
        } else {
            y
        }
    };
    let (yt, yb) = (y(text), y(bg));
    if (yb - yt).abs() < 0.0005 {
        return 0.0;
    }
    let out = if yb > yt {
        let s = (yb.powf(0.56) - yt.powf(0.57)) * 1.14;
        if s < 0.1 { 0.0 } else { s - 0.027 }
    } else {
        let s = (yb.powf(0.65) - yt.powf(0.62)) * 1.14;
        if s > -0.1 { 0.0 } else { s + 0.027 }
    };
    out * 100.0
}

/// What's behind a see-through window: the brightest tenth (worst case for a dark theme;
/// the darkest tenth for a light one) and the average of a wallpaper.
pub fn behind(image: &crate::pixels::Image, dark_theme: bool) -> (String, String) {
    let hex = |p: [u8; 3]| format!("#{:02x}{:02x}{:02x}", p[0], p[1], p[2]);
    let mut px: Vec<[u8; 3]> = image.rgb.clone();
    let lum = |p: &[u8; 3]| colour::contrast(&hex(*p), "#000000");
    px.sort_by(|a, b| lum(a).total_cmp(&lum(b)));
    let n = px.len().max(1);
    let worst = if dark_theme {
        px[(n * 9 / 10).min(n - 1)]
    } else {
        px[n / 10]
    };
    let sum = px.iter().fold([0u64; 3], |s, p| {
        [s[0] + p[0] as u64, s[1] + p[1] as u64, s[2] + p[2] as u64]
    });
    let avg = [
        (sum[0] / n as u64) as u8,
        (sum[1] / n as u64) as u8,
        (sum[2] / n as u64) as u8,
    ];
    (hex(worst), hex(avg))
}

/// Text colours that idea 12 checks on glass.
const GLASS_TEXT: [&str; 11] = [
    "foreground",
    "dark_foreground",
    "muted",
    "accent",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "bright_foreground",
];

/// Idea 12: the background a see-through window shows (`opacity` of the theme's background
/// over `behind`), and the text colours below their minimum on it.
pub fn glass(c: &Colors, behind: &str, opacity: f32) -> (String, Vec<String>) {
    let eff = colour::mix(behind, c.get("background"), opacity);
    let min = |k: &str| match k {
        "foreground" | "bright_foreground" => 4.5,
        "dark_foreground" => 2.5,
        "muted" => 1.6,
        _ => 3.0,
    };
    let low = GLASS_TEXT
        .iter()
        .filter(|k| cr(c.get(k), &eff) < min(k) * normalise::SLACK)
        .map(|k| format!("{k} {:.1}:1", cr(c.get(k), &eff)))
        .collect();
    (eff, low)
}

/// Idea 12, the fix: text colours lifted (same hue) to read on the glass background, using
/// the same rules as the solid fixes. Also returns the lowest opacity at which the theme
/// needs no fix at all (in steps of 5 %), if any.
pub fn fix_glass(
    c: &mut Colors,
    behind: &str,
    opacity: f32,
    level: Level,
) -> (Vec<Step>, Option<u8>) {
    let _ = level; // the glass minimums are Minimal's; Strong would raise them
    let (eff, _) = glass(c, behind, opacity);
    // Lift each faint text colour away from the theme's own background (lighter on a dark
    // theme), same hue, until it reads on the glass: it must still read on the theme's solid
    // surfaces (selection, current line). Found in the lab: reusing the solid fixes on a
    // mid-grey glass background moved text darker (frost). At the limit, it stays lifted as
    // far as it goes and the opacity note says what's needed.
    let away = l(c.get("background")) < 0.5;
    let min = |k: &str| match k {
        "foreground" | "bright_foreground" => 4.5,
        "dark_foreground" => 2.5,
        "muted" => 1.6,
        _ => 3.0,
    };
    let mut steps = Vec::new();
    for key in GLASS_TEXT {
        let now = c.get(key).to_string();
        let need = min(key);
        if cr(&now, &eff) >= need * normalise::SLACK {
            continue;
        }
        let lch = Lch::from_hex(&now);
        let mut best = now.clone();
        for i in 1..=200 {
            let d = i as f32 * 0.005;
            let x = if away { lch.l + d } else { lch.l - d };
            if !(0.0..=1.0).contains(&x) {
                break;
            }
            let hex = Lch { l: x, ..lch }.to_hex();
            if cr(&hex, &eff) > cr(&best, &eff) {
                best = hex.clone();
            }
            if cr(&hex, &eff) >= need {
                best = hex;
                break;
            }
        }
        let why = if cr(&best, &eff) >= need * normalise::SLACK {
            "lifted to read on the glass"
        } else {
            "lifted as far as it goes; still faint on the glass (raise the opacity)"
        };
        set(c, &mut steps, key, best, why);
    }
    let solid_enough = (10..=20)
        .map(|i| i as f32 * 0.05)
        .find(|o| {
            glass(c, behind, *o).1.is_empty() && {
                let mut orig = c.clone();
                for s in &steps {
                    orig.set(s.key, &s.from);
                }
                glass(&orig, behind, *o).1.is_empty()
            }
        })
        .map(|o| (o * 100.0).round() as u8);
    (steps, solid_enough)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Colors {
        crate::theme::load_original(
            &crate::paths::Paths::fake_home("/nonexistent"),
            "matte-black",
        )
        .unwrap()
        .colors
    }

    #[test]
    fn apca_matches_published_examples() {
        // APCA's own reference values (0.0.98G): #888 on #fff ≈ 63.1, #fff on #888 ≈ -68.5.
        assert!(
            (apca("#888888", "#ffffff") - 63.06).abs() < 0.5,
            "{}",
            apca("#888888", "#ffffff")
        );
        assert!(
            (apca("#ffffff", "#888888") + 68.54).abs() < 0.5,
            "{}",
            apca("#ffffff", "#888888")
        );
        assert!(
            (apca("#000000", "#aaaaaa") - 58.15).abs() < 0.5,
            "{}",
            apca("#000000", "#aaaaaa")
        );
    }

    #[test]
    fn ranking_keeps_good_guesses_and_is_stable() {
        let mut c = theme();
        let first = rank_roles(&mut c);
        let again = rank_roles(&mut c);
        assert!(again.is_empty(), "first {first:?}, again {again:?}");
    }

    #[test]
    fn spaced_greys_make_every_terminal_colour_differ() {
        let mut c = theme();
        for (k, v) in [
            ("red", "#8a8a8a"),
            ("magenta", "#8a8a8a"),
            ("green", "#767676"),
            ("cyan", "#767676"),
            ("yellow", "#9e9e9e"),
            ("blue", "#626262"),
        ] {
            c.set(k, v);
        }
        for k in BRIGHT {
            c.set(k, "#9e9e9e");
        }
        assert!(monochrome(&c));
        spaced_greys(&mut c);
        let mut ls: Vec<f32> = ANSI.iter().map(|k| l(c.get(k))).collect();
        ls.sort_by(f32::total_cmp);
        assert!(ls.windows(2).all(|w| w[1] - w[0] >= 0.03), "{ls:?}");
    }

    /// Stress: every catalogue theme. Ranking fixes the order of every unnamed theme and is
    /// stable; reuse is never worse than today's fixes; spaced greys separate every
    /// monochrome theme.
    #[test]
    fn whole_catalogue_stress() {
        let mut fails = Vec::new();
        for e in crate::catalog::entries() {
            let mut r = e.colors.clone();
            if e.derived.is_some() {
                rank_roles(&mut r);
                // R in the demo: ranking, then the fixes (which own the selection).
                let mut rf = r.clone();
                normalise::fix(&mut rf, Level::Minimal, &BTreeSet::new());
                if !order_problems(&rf).is_empty() {
                    fails.push(format!("{}: order {:?}", e.name, order_problems(&rf)));
                }
                let again = rank_roles(&mut r.clone());
                if !again.is_empty() {
                    fails.push(format!("{}: ranking not stable: {:?}", e.name, again[0]));
                }
            }
            let mut f = r.clone();
            normalise::fix(&mut f, Level::Minimal, &BTreeSet::new());
            let mut u = r.clone();
            reuse(&mut u, Level::Minimal);
            let low = |c: &Colors| {
                crate::check::colors_at(&e.name, c, Level::Minimal)
                    .issues
                    .len()
            };
            if low(&u) > low(&f) {
                fails.push(format!(
                    "{}: reuse worse than the fixes ({} vs {})",
                    e.name,
                    low(&u),
                    low(&f)
                ));
            }
            if monochrome(&e.colors) {
                let mut m = e.colors.clone();
                spaced_greys(&mut m);
                for group in [ANSI, BRIGHT] {
                    let mut ls: Vec<f32> = group.iter().map(|k| l(m.get(k))).collect();
                    ls.sort_by(f32::total_cmp);
                    if !ls.windows(2).all(|w| w[1] - w[0] >= 0.03) {
                        fails.push(format!("{}: greys too close {ls:?}", e.name));
                    }
                }
            }
        }
        assert!(
            fails.is_empty(),
            "{} failures:\n{}",
            fails.len(),
            fails.join("\n")
        );
    }
}
