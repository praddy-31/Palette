//! Ranked roles and the theme's own colours (11e, from the colour lab's ideas 9 and 10;
//! report: `docs/reports/colour-lab/README.md`). Part of the readability fixes:
//! [`crate::normalise::apply`] runs [`rank`] first, then the fixes through [`reuse`].
//!
//! - [`rank`]: roles that break the ordering rules (shades darker than the background, comments
//!   quieter than secondary text quieter than text, bright text the strongest, …) are
//!   re-chosen from the theme's own colours; with none fitting, one is mixed.
//! - [`reuse`]: a colour the fixes would invent is swapped for one of the theme's own when one
//!   is within [`REUSE_DE`] (barely visible) and passes the same rules.

use std::collections::BTreeSet;

use crate::colour::{self, Lch};
use crate::normalise::{self, Change, Level};
use crate::theme::Colors;

/// Whether a theme's roles were named by its author or guessed by the importer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Roles {
    #[default]
    Named,
    Guessed,
}

/// The line the importer writes into a `colors.toml` whose roles it guessed.
pub const GUESSED_MARK: &str = "# Roles guessed from the terminal colours";

/// Whether `theme`'s roles were guessed: a catalogue theme the catalogue marks `derived`, or
/// an import whose `colors.toml` carries [`GUESSED_MARK`].
pub fn roles_of(theme: &crate::theme::Theme) -> Roles {
    let marked = std::fs::read_to_string(theme.dir.join("colors.toml"))
        .is_ok_and(|t| t.lines().take(3).any(|l| l.starts_with(GUESSED_MARK)));
    let derived = crate::catalog::find(&theme.name).is_some_and(|e| e.derived.is_some());
    if marked || derived {
        Roles::Guessed
    } else {
        Roles::Named
    }
}

fn l(hex: &str) -> f32 {
    Lch::from_hex(hex).l
}

fn cr(a: &str, b: &str) -> f32 {
    colour::contrast(a, b)
}

/// Every distinct colour the theme has: the pool roles are chosen from.
pub fn pool(c: &Colors) -> Vec<String> {
    let mut seen = BTreeSet::new();
    c.iter()
        .map(|(_, v)| v.to_string())
        .filter(|v| seen.insert(v.clone()))
        .collect()
}

/// Grey-ish, or tinted like the background: fit for shades and quiet text.
fn neutral(hex: &str, bg: &str) -> bool {
    let (x, b) = (Lch::from_hex(hex), Lch::from_hex(bg));
    let hue_gap = ((x.h - b.h).abs() % 360.0).min(360.0 - (x.h - b.h).abs() % 360.0);
    x.c < 0.05 || (x.c < 0.11 && hue_gap < 30.0)
}

/// What [`rank`] changed, and the keys it may not touch.
struct Steps<'a> {
    list: Vec<Change>,
    keep: &'a BTreeSet<String>,
}

fn set(c: &mut Colors, steps: &mut Steps, key: &'static str, to: String, why: &str) {
    let from = c.get(key).to_string();
    if from != to && !steps.keep.contains(key) {
        c.set(key, &to);
        steps.list.push(Change {
            key,
            from,
            to,
            why: format!("ranked: {why}"),
        });
    }
}

/// The candidate from `pool` that passes `ok`, nearest to `score` 0 (smaller is better).
fn pick(pool: &[String], ok: impl Fn(&str) -> bool, score: impl Fn(&str) -> f32) -> Option<String> {
    pool.iter()
        .filter(|h| ok(h))
        .min_by(|a, b| score(a).total_cmp(&score(b)))
        .cloned()
}

/// Ranked roles (idea 9, 11e): re-choose the roles that break the rules, from the theme's
/// own colours. Every theme (owner, 2026-11-07); keys you edited (`keep`) are never touched.
///
/// - `Roles::Guessed` (the importer mapped the terminal colours to roles): every rule,
///   including how a role should look (comments about 2.3:1 and greyish, line numbers
///   between comments and text, light text as strong as the text), as reviewed in the lab.
/// - `Roles::Named` (the author named them): only the *ordering* rules: shades in order,
///   the panel on the right side, comments quieter than the text, line numbers not louder
///   than the text, bright text the strongest. The looks stay the author's (dracula's line
///   numbers share its comment colour on purpose).
///
/// Background, text, selection, accent and the twelve terminal colours are always the
/// author's; only the shades, panel, comments and the three extra text colours move. A role
/// that fits stays; a replacement is picked from the theme's own colours (comments about
/// 2.3:1, secondary text about 3.5:1), or mixed as the importer does when none fits.
pub fn rank(c: &mut Colors, keep: &BTreeSet<String>, roles: Roles) -> Vec<Change> {
    let guessed = roles == Roles::Guessed;
    let mut steps = Steps {
        list: Vec::new(),
        keep,
    };
    let pool = pool(c);
    let bg = c.get("background").to_string();
    let fg = c.get("foreground").to_string();
    let dark_theme = l(&bg) < 0.5;
    let raised = |h: &str| (l(h) > l(&bg)) == dark_theme;
    let n = |h: &str| neutral(h, &bg) && h != bg && h != fg;
    let fg_c = cr(&fg, &bg);

    // Shades: darker than the background in both modes; on a light theme only a small step
    // (as the fixes, 11d: black is a fine darkest surface on a dark theme). The wrong way
    // round, they're swapped first, so the theme keeps its own two colours.
    let step = |h: &str, r: &str| dark_theme || l(r) - l(h) <= normalise::SHADE_STEP;
    let (d0, dd0) = (
        c.get("dark_background").to_string(),
        c.get("darker_background").to_string(),
    );
    let below = |h: &str| l(h) < l(&bg);
    if l(&bg) >= 0.03
        && below(&d0)
        && below(&dd0)
        && l(&d0) < l(&dd0)
        && step(&dd0, &bg)
        && !keep.contains("dark_background")
        && !keep.contains("darker_background")
    {
        set(
            c,
            &mut steps,
            "dark_background",
            dd0.clone(),
            "shades swapped: the other way round",
        );
        set(
            c,
            &mut steps,
            "darker_background",
            d0,
            "shades swapped: the other way round",
        );
    }
    // On a black background nothing is deeper: the shades stay the author's (vantablack), as
    // in the fixes.
    let black = l(&bg) < 0.03;
    let dark_ok = |h: &str| black || (l(h) < l(&bg) && step(h, &bg));
    let now = c.get("dark_background").to_string();
    if !dark_ok(&now) {
        let to = pick(
            &pool,
            |h| n(h) && dark_ok(h),
            |h| (l(&bg) - l(h) - 0.03).abs(),
        )
        .unwrap_or_else(|| colour::mix(&bg, "#000000", if dark_theme { 0.25 } else { 0.0625 }));
        set(
            c,
            &mut steps,
            "dark_background",
            to,
            "shade: darker than the background, a small step",
        );
    }
    let dark = c.get("dark_background").to_string();
    let darker_ok = |h: &str| l(h) < l(&dark) && step(h, &dark);
    let now = c.get("darker_background").to_string();
    if !black && ((now != dark && !darker_ok(&now)) || (now == dark && l(&dark) > 0.03)) {
        let to = pick(
            &pool,
            |h| n(h) && darker_ok(h),
            |h| (l(&dark) - l(h) - 0.03).abs(),
        )
        .unwrap_or_else(|| colour::mix(&dark, "#000000", if dark_theme { 0.3 } else { 0.06 }));
        set(
            c,
            &mut steps,
            "darker_background",
            to,
            "shade: darker than dark_background",
        );
    }

    // Panel: raised a little, text still readable on it.
    // A panel is its own surface: never one of the shades.
    let (dark, darker) = (
        c.get("dark_background").to_string(),
        c.get("darker_background").to_string(),
    );
    let panel_ok = |h: &str| {
        raised(h)
            && (1.08..=1.6).contains(&cr(h, &bg))
            && cr(&fg, h) >= 3.0
            && h != dark
            && h != darker
    };
    let now = c.get("lighter_background").to_string();
    if !(if guessed {
        panel_ok(&now)
    } else {
        raised(&now)
    }) {
        let to = pick(&pool, |h| n(h) && panel_ok(h), |h| (cr(h, &bg) - 1.2).abs()).unwrap_or_else(
            || colour::mix(&bg, if dark_theme { "#ffffff" } else { "#000000" }, 0.08),
        );
        set(
            c,
            &mut steps,
            "lighter_background",
            to,
            "panel: a small step towards the text",
        );
    }

    // Quiet text: comments < secondary text < text.
    // Lower bounds with the fixes' slack, so what they leave is never re-chosen (stable).
    let sl = normalise::SLACK;
    let muted_ok = |h: &str| (1.6 * sl..=3.5).contains(&cr(h, &bg)) && cr(h, &bg) < fg_c;
    let now = c.get("muted").to_string();
    if !(if guessed {
        muted_ok(&now)
    } else {
        cr(&now, &bg) < fg_c
    }) {
        // With no fitting colour, the importer's mix; otherwise a colour mixed for another role
        // would be picked here on a second run (not stable).
        let to = pick(&pool, |h| n(h) && muted_ok(h), |h| (cr(h, &bg) - 2.3).abs())
            .unwrap_or_else(|| colour::mix(&bg, &fg, 0.4));
        set(
            c,
            &mut steps,
            "muted",
            to,
            "comments: a quiet grey (about 2.3:1)",
        );
    }
    let muted_c = cr(c.get("muted"), &bg);
    let sec_ok = |h: &str| {
        let x = cr(h, &bg);
        x >= 2.5 * sl && x <= (fg_c * 0.85).min(7.0) && x > muted_c
    };
    let now = c.get("dark_foreground").to_string();
    if !(if guessed {
        sec_ok(&now)
    } else {
        cr(&now, &bg) <= fg_c * 1.05
    }) {
        let to = pick(&pool, |h| n(h) && sec_ok(h), |h| (cr(h, &bg) - 3.5).abs())
            .unwrap_or_else(|| colour::mix(&bg, &fg, 0.6));
        set(
            c,
            &mut steps,
            "dark_foreground",
            to,
            "secondary text: between comments and text",
        );
    }

    // Light and bright text: at least as strong as the text.
    let strong = |h: &str| cr(h, &bg) >= (fg_c * 0.9).max(4.5 * sl) && n(h) || h == fg;
    let now = c.get("light_foreground").to_string();
    if guessed && !strong(&now) {
        let to = pick(&pool, |h| strong(h), |h| (cr(h, &bg) - fg_c).abs()).unwrap_or(fg.clone());
        set(
            c,
            &mut steps,
            "light_foreground",
            to,
            "light text: as strong as the text",
        );
    }
    let light_c = cr(c.get("light_foreground"), &bg);
    let brightest = |h: &str| (strong(h) && cr(h, &bg) >= light_c) || h == fg;
    let now = c.get("bright_foreground").to_string();
    if !(if guessed {
        brightest(&now)
    } else {
        cr(&now, &bg) >= fg_c * 0.9
    }) {
        let to = pick(&pool, |h| brightest(h), |h| -cr(h, &bg)).unwrap_or(fg.clone());
        set(
            c,
            &mut steps,
            "bright_foreground",
            to,
            "bright text: the strongest",
        );
    }
    steps.list
}

/// The owner's ordering rules (and the spec's); one line per rule broken.
pub fn order_problems(c: &Colors) -> Vec<String> {
    let bg = c.get("background");
    let dark_theme = l(bg) < 0.5;
    let raised = |h: &str| (l(h) > l(bg)) == dark_theme;
    let mut out = Vec::new();
    let mut rule = |broken: bool, text: &str| {
        if broken {
            out.push(text.to_string());
        }
    };
    let black = l(bg) < 0.03;
    rule(
        !black && l(c.get("dark_background")) >= l(bg) && c.get("dark_background") != bg,
        "dark shade not darker than the background",
    );
    rule(
        !black && l(c.get("darker_background")) > l(c.get("dark_background")),
        "darker shade lighter than dark",
    );
    rule(
        !raised(c.get("lighter_background")),
        "panel on the wrong side of the background",
    );
    rule(
        !raised(c.get("selection")),
        "selection on the wrong side of the background",
    );
    rule(
        cr(c.get("selection"), bg) >= cr(c.get("foreground"), bg),
        "selection stronger than the text",
    );
    let (m, s, f) = (
        cr(c.get("muted"), bg),
        cr(c.get("dark_foreground"), bg),
        cr(c.get("foreground"), bg),
    );
    // Not a rule: many good themes dim line numbers below comments on purpose (VS Code's
    // tokyo-night: comments #565f89, line numbers #3b4261). Only gross inversions count.
    let _ = (m, s);
    rule(s > f * 1.05, "secondary text stronger than text");
    rule(
        cr(c.get("bright_foreground"), bg) < f * 0.9,
        "bright text weaker than text",
    );
    out
}

/// The largest ΔE (OKLab) for a fixed colour to be swapped for one of the theme's own.
pub const REUSE_DE: f32 = 0.05;

/// The fixes ([`normalise::fix`]) at `level`, but each fixed colour is replaced by one of the
/// theme's own colours when one is within [`REUSE_DE`] of it and the fixes leave that one
/// alone (so it passes the same rules). Keys in `keep` are never touched.
pub fn reuse(c: &mut Colors, level: Level, keep: &BTreeSet<String>, roles: Roles) -> Vec<Change> {
    let own = pool(c);
    let before = c.clone();
    let mut fixed = c.clone();
    let changes = normalise::fix(&mut fixed, level, keep);
    let fixed_keys: BTreeSet<&str> = changes.iter().map(|c| c.key).collect();
    let mut out = Vec::new();
    for ch in &changes {
        let target = Lch::from_hex(&ch.to);
        let mut near: Vec<(&String, f32)> = own
            .iter()
            .map(|h| (h, Lch::from_hex(h).distance(&target)))
            .filter(|(h, d)| *d <= REUSE_DE && **h != ch.from)
            .collect();
        near.sort_by(|a, b| a.1.total_cmp(&b.1));
        for (h, d) in near {
            let mut trial = c.clone();
            trial.set(ch.key, h);
            // The fixes must leave the reused colour alone and move nothing they didn't
            // already move (a reused selection mustn't push them into lifting the text).
            // And ranking must keep it too (otherwise the next run re-chooses it).
            let ranked = rank(&mut trial.clone(), keep, roles);
            let again = normalise::fix(&mut trial, level, keep);
            if ranked.iter().all(|x| x.key != ch.key)
                && again
                    .iter()
                    .all(|x| x.key != ch.key && fixed_keys.contains(x.key))
            {
                c.set(ch.key, h);
                out.push(Change {
                    key: ch.key,
                    from: ch.from.clone(),
                    to: h.clone(),
                    why: format!("{} (the theme's own colour, ΔE {d:.3})", ch.why),
                });
                break;
            }
        }
    }
    // Whatever wasn't reused is fixed as usual.
    for ch in normalise::fix(c, level, keep) {
        out.push(Change {
            from: before.get(ch.key).to_string(),
            ..ch
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalogue(name: &str) -> Colors {
        crate::catalog::find(name).unwrap().colors.clone()
    }

    #[test]
    fn a_named_theme_keeps_its_looks() {
        // dracula's line numbers share its comment colour on purpose: not an order problem.
        let mut c = catalogue("dracula");
        let ln = c.get("dark_foreground").to_string();
        rank(&mut c, &BTreeSet::new(), Roles::Named);
        assert_eq!(c.get("dark_foreground"), ln);
    }

    #[test]
    fn a_named_theme_is_put_in_order() {
        // oxocarbon's "bright" text is a teal weaker than its body text.
        let mut c = catalogue("oxocarbon");
        assert!(!order_problems(&c).is_empty());
        rank(&mut c, &BTreeSet::new(), Roles::Named);
        assert!(order_problems(&c).is_empty(), "{:?}", order_problems(&c));
    }

    #[test]
    fn a_guessed_theme_gets_quiet_comments() {
        // aetheria (lab figure): the importer guessed loud pink comments.
        let mut c = catalogue("aetheria");
        let before = c.get("muted").to_string();
        rank(&mut c, &BTreeSet::new(), Roles::Guessed);
        assert_ne!(c.get("muted"), before);
        assert!(cr(c.get("muted"), c.get("background")) <= 3.5);
    }

    #[test]
    fn your_edits_are_never_ranked() {
        let mut c = catalogue("oxocarbon");
        let bright = c.get("bright_foreground").to_string();
        let keep = BTreeSet::from(["bright_foreground".to_string()]);
        let changes = rank(&mut c, &keep, Roles::Named);
        assert_eq!(c.get("bright_foreground"), bright);
        assert!(changes.iter().all(|ch| ch.key != "bright_foreground"));
    }
}
