//! Readability fixes (Phase 11a): applied every time a theme is loaded for use, after your
//! colour edits, so theme files stay as their authors made them. Only what's broken moves,
//! and only in lightness (OKLCH): hue and saturation stay, so a grey theme stays grey.
//!
//! Two kinds of minimum:
//! - **readable**: text on a colour (WCAG contrast, 1.6 to 4.5:1 by role, as in `check`);
//! - **visible**: a surface against the background (selection, panels), a much lower bar.
//!   Palette's contrast check only had the first kind, so a selection the same shade as the
//!   background (ash: `#0d0d0d` on `#121212`, 1.04:1) passed, and Ghostty showed none.
//!
//! Levels (`settings.toml readability`, `palette check --level`): `off`, `minimal` (the
//! default: only what's broken) and `strong` (WCAG AA for text). A key you edited yourself
//! (`edits.toml`) is never touched; a theme listed in `original_colours` is left alone.

use std::collections::BTreeSet;

use anyhow::{Result, bail};

use crate::colour::{self, Lch};
use crate::ranking::{self, Roles};
use crate::theme::Colors;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Level {
    Off,
    #[default]
    Minimal,
    Strong,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Minimal => "minimal",
            Self::Strong => "strong",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "none" => Ok(Self::Off),
            "minimal" | "min" | "on" => Ok(Self::Minimal),
            "strong" | "max" | "aa" => Ok(Self::Strong),
            other => bail!("readability is off, minimal or strong (not `{other}`)"),
        }
    }

    /// Body text on a panel: 3:1 in Minimal, 4.5:1 otherwise (Off: the standard, for
    /// `palette check`).
    /// The text minimums (body, secondary, comments, coloured), for checks made outside the
    /// fixes (glass, `glass::lift`); `None` for Off.
    pub fn minimums(self) -> Option<(f32, f32, f32, f32)> {
        (self != Self::Off).then(|| {
            let m = self.mins();
            (m.body, m.secondary, m.comments, m.coloured)
        })
    }

    pub fn panel_text(self) -> f32 {
        match self {
            Self::Minimal => self.mins().panel_text,
            Self::Off | Self::Strong => Self::Strong.mins().panel_text,
        }
    }

    fn mins(self) -> Mins {
        match self {
            // Off never gets here; same numbers as Minimal to keep the type simple.
            Self::Off | Self::Minimal => Mins {
                body: 4.5,
                secondary: 2.5,
                comments: 1.6,
                coloured: 3.0,
                selection: 1.3,
                panel: 1.08,
                panel_text: colour::READABLE,
            },
            Self::Strong => Mins {
                body: 4.5,
                secondary: 4.5,
                comments: 3.0,
                coloured: 3.0,
                selection: 1.5,
                panel: 1.15,
                panel_text: 4.5,
            },
        }
    }
}

/// The minimums for one level.
struct Mins {
    /// Body text: foreground and its light/bright versions, text on the selection.
    body: f32,
    /// Line numbers, status lines (`dark_foreground`).
    secondary: f32,
    /// Comments and autosuggestions (`muted`).
    comments: f32,
    /// Coloured text: accent, the six ANSI colours and their bright versions.
    coloured: f32,
    /// Selection against the background (visible, not readable).
    selection: f32,
    /// Panels, popups (`lighter_background`) against the background.
    panel: f32,
    /// Body text on a panel.
    panel_text: f32,
}

/// One colour Palette changed, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub key: &'static str,
    pub from: String,
    pub to: String,
    pub why: String,
}

impl Change {
    /// `selection #0d0d0d → #2e2e2e: invisible on the background (1.0:1, needs 1.3)`
    pub fn summary(&self) -> String {
        format!("{} {} → {}: {}", self.key, self.from, self.to, self.why)
    }
}

/// A colour within 3 % of its minimum is left alone (tokyo-night's selection is 1.27:1
/// against 1.3, and nobody can see that difference); one that's fixed goes all the way.
pub const SLACK: f32 = 0.97;

/// The largest drop in lightness (OKLCH L) from a light theme's background to its shade, and
/// from one shade to the next (colour lab, 11d). 4 of 168 themes go past it: 3 guessed by the
/// importer (solarized-light, ristretto-light, mapquest) and venice-from-above.
pub const SHADE_STEP: f32 = 0.10;

const COLOURED: [&str; 13] = [
    "accent",
    "red",
    "yellow",
    "green",
    "cyan",
    "blue",
    "magenta",
    "bright_red",
    "bright_yellow",
    "bright_green",
    "bright_cyan",
    "bright_blue",
    "bright_magenta",
];

/// Fix `c` in place for `level`, leaving the keys in `keep` (your edits) alone. Returns what
/// changed, one line per key. Running it again on its own result changes nothing.
///
/// Since 11e: the roles are put in order first ([`ranking::rank`]), then the fixes run,
/// preferring the theme's own colours over invented ones ([`ranking::reuse`]).
pub fn apply(c: &mut Colors, level: Level, keep: &BTreeSet<String>) -> Vec<Change> {
    apply_with(c, level, keep, Roles::Named)
}

/// [`apply`] for a theme whose roles are `roles` ([`ranking::roles_of`]).
pub fn apply_with(
    c: &mut Colors,
    level: Level,
    keep: &BTreeSet<String>,
    roles: Roles,
) -> Vec<Change> {
    if level == Level::Off {
        return Vec::new();
    }
    // Until nothing moves: a fix can change what ranking measures against (the text lifted
    // for the selection makes light text "weaker than the text"). Two passes at most in the
    // whole catalogue; four is a guard.
    let mut all = Vec::new();
    for _ in 0..4 {
        let pass: Vec<Change> = ranking::rank(c, keep, roles)
            .into_iter()
            .chain(ranking::reuse(c, level, keep, roles))
            .collect();
        if pass.is_empty() {
            break;
        }
        all.extend(pass);
    }
    merge(Vec::new(), all)
}

/// Everything Palette does to a theme's colours when it's used (`theme::load_fixed`): the
/// fixes with ranking and reuse, then monochrome (11f), then the fixes once more in case a
/// tint moved a colour's contrast (the greys already keep 3:1). One line per key.
pub fn apply_all(
    c: &mut Colors,
    level: Level,
    mono: crate::monochrome::Mono,
    keep: &BTreeSet<String>,
    roles: Roles,
) -> Vec<Change> {
    let changes = apply_with(c, level, keep, roles);
    let mono = crate::monochrome::apply(c, mono, keep);
    if mono.is_empty() {
        return changes;
    }
    let again = apply_with(c, level, keep, roles);
    merge(changes, mono.into_iter().chain(again).collect())
}

/// `later` added to `earlier`, one line per key: a key moved twice (ranked, then fixed)
/// reads from its original to where it ended, with both reasons; a key back where it
/// started is dropped.
pub fn merge(earlier: Vec<Change>, later: Vec<Change>) -> Vec<Change> {
    let mut out = earlier;
    for ch in later {
        match out.iter_mut().find(|o| o.key == ch.key) {
            Some(o) => {
                o.to = ch.to;
                o.why = format!("{}; then {}", o.why, ch.why);
            }
            None => out.push(ch),
        }
    }
    out.retain(|o| o.from != o.to);
    out
}

/// The readability fixes alone (11a), colour by colour, without ranking or reuse.
pub fn fix(c: &mut Colors, level: Level, keep: &BTreeSet<String>) -> Vec<Change> {
    if level == Level::Off {
        return Vec::new();
    }
    let m = level.mins();
    let mut fx = Fixer {
        c,
        keep,
        changes: Vec::new(),
    };
    let bg = fx.get("background");
    // Which way is "away from the background" for raised surfaces and text.
    let up = Lch::from_hex(&bg).l < 0.5;

    // 1. Shades in order: dark and darker are darker than the background in both modes
    //    (THEME_SPEC: "slightly darker surface; ANSI black", "darkest surface"; every
    //    bundled light theme does this), darker the darkest.
    let deep = "#000000";
    let deeper = |a: &str, b: &str| Lch::from_hex(a).l < Lch::from_hex(b).l;
    // On a black background nothing is deeper; slightly lighter shades are the author's
    // choice there (vantablack), so they stay.
    let black = Lch::from_hex(&bg).l < 0.03;
    // On a light theme a shade is a *small step* below what it sits on (colour lab, 11d):
    // the importer can guess a navy or mid grey there (solarized-light), which reads as a
    // dark bar, not a shade. Dark themes have no such limit: black is a fine darkest
    // surface (aetheria).
    let light = !up;
    let small = |x: &str, r: &str| !light || Lch::from_hex(r).l - Lch::from_hex(x).l <= SHADE_STEP;
    // How far a mixed shade goes towards black: a small step on light themes.
    let (dark_mix, darker_mix) = if light { (0.0625, 0.06) } else { (0.25, 0.3) };
    // Both darker than the background but the wrong way round: swap them, so the theme
    // keeps its own two colours (unless you edited one of them). Only when the lighter one
    // is a fitting shade: a far-off guess would otherwise land in the darkest shade.
    let (dark, darker) = (fx.get("dark_background"), fx.get("darker_background"));
    if !black
        && deeper(&dark, &bg)
        && deeper(&darker, &bg)
        && deeper(&dark, &darker)
        && small(&darker, &bg)
        && !fx.keep.contains("dark_background")
        && !fx.keep.contains("darker_background")
    {
        let why = "swapped: it was the other way round";
        fx.set("dark_background", darker.clone(), why.into());
        fx.set("darker_background", dark, why.into());
    }
    let dark = fx.get("dark_background");
    if !black && dark != bg && !deeper(&dark, &bg) {
        fx.set(
            "dark_background",
            colour::mix(&bg, deep, dark_mix),
            "on the wrong side of the background (should be deeper)".into(),
        );
    } else if !black && dark != bg && !small(&dark, &bg) {
        fx.set(
            "dark_background",
            colour::mix(&bg, deep, dark_mix),
            "too dark for a light theme's shade (should be a small step)".into(),
        );
    }
    let (dark, darker) = (fx.get("dark_background"), fx.get("darker_background"));
    if !black && darker != dark && (!deeper(&darker, &dark) || !small(&darker, &dark)) {
        // From dark, not the background: when dark is already black, darker is too.
        let why = if deeper(&darker, &dark) {
            "too dark for a light theme's shade (should be a small step)"
        } else {
            "not deeper than dark_background"
        };
        fx.set(
            "darker_background",
            colour::mix(&dark, deep, darker_mix),
            why.into(),
        );
    }

    // 2. Body text on the background.
    for key in ["foreground", "light_foreground", "bright_foreground"] {
        fx.text(key, &bg, m.body, up);
    }

    // 3. Panels: visible against the background, body text readable on them.
    fx.surface(
        "lighter_background",
        &bg,
        m.panel,
        &[("foreground", m.panel_text)],
        up,
    );

    // 4. Selection: visible against the background, selected text readable on it.
    fx.surface(
        "selection",
        &bg,
        m.selection,
        &[
            ("bright_foreground", m.body),
            ("foreground", colour::READABLE),
        ],
        up,
    );

    // 5. Quieter text, then coloured text.
    fx.text("dark_foreground", &bg, m.secondary, up);
    fx.text("muted", &bg, m.comments, up);
    for key in COLOURED {
        fx.text(key, &bg, m.coloured, up);
    }
    fx.changes
}

struct Fixer<'a> {
    c: &'a mut Colors,
    keep: &'a BTreeSet<String>,
    changes: Vec<Change>,
}

impl Fixer<'_> {
    fn get(&self, key: &str) -> String {
        self.c.get(key).to_string()
    }

    fn set(&mut self, key: &'static str, to: String, why: String) {
        if self.keep.contains(key) {
            return;
        }
        let from = self.get(key);
        if from == to {
            return;
        }
        self.c.set(key, &to);
        self.changes.push(Change { key, from, to, why });
    }

    /// Text `key` lifted (same hue) until it reads `min` on `bg`.
    fn text(&mut self, key: &'static str, bg: &str, min: f32, up: bool) {
        let now = self.get(key);
        let ratio = colour::contrast(&now, bg);
        if ratio >= min * SLACK {
            return;
        }
        let Some(to) = nearest(&now, |hex| colour::contrast(hex, bg) >= min, up) else {
            return;
        };
        let why = format!("hard to read on the background ({ratio:.1}:1, needs {min})");
        self.set(key, to, why);
    }

    /// Surface `key`: at least `visible` against `bg`, on the raised side of it, with each
    /// `(text key, min)` readable on it. Keeps the original if it passes; else the passing
    /// lightness nearest the original. When none passes (the text is too close to the
    /// background for both), the text is lifted first, as both need about `min × visible`
    /// between text and background. If still none, a surface that's at least visible stays.
    fn surface(
        &mut self,
        key: &'static str,
        bg: &str,
        visible: f32,
        texts: &[(&'static str, f32)],
        up: bool,
    ) {
        let now = self.get(key);
        if self.passes(&now, bg, visible, texts, up, SLACK) {
            return;
        }
        let seen = |hex: &str| {
            (Lch::from_hex(hex).l > Lch::from_hex(bg).l) == up
                && colour::contrast(hex, bg) >= visible * SLACK
        };
        let why = if !seen(&now) {
            format!(
                "barely visible on the background ({:.2}:1, needs {visible})",
                colour::contrast(&now, bg)
            )
        } else {
            let (t, min) = texts
                .iter()
                .map(|(k, min)| (self.get(k), *min))
                .find(|(t, min)| colour::contrast(t, &now) < *min)
                .unwrap_or_else(|| (self.get(texts[0].0), texts[0].1));
            format!(
                "text on it is hard to read ({:.1}:1, needs {min})",
                colour::contrast(&t, &now)
            )
        };
        let mut to = self.find(&now, bg, visible, texts, up);
        if to.is_none() {
            for (text, min) in texts {
                // A little over, so rounding to #rrggbb can't land just under.
                let need = min * visible * 1.03;
                let t = self.get(text);
                let ratio = colour::contrast(&t, bg);
                if ratio < need
                    && let Some(lifted) = nearest(&t, |hex| colour::contrast(hex, bg) >= need, up)
                {
                    let why = format!(
                        "lifted so it stays readable on {key} ({ratio:.1}:1 on the background)"
                    );
                    self.set(text, lifted, why);
                }
            }
            to = self.find(&now, bg, visible, texts, up);
        }
        match to {
            Some(to) => self.set(key, to, why),
            None if seen(&now) => {}
            None => {
                if let Some(to) = nearest(&now, seen, up) {
                    self.set(key, to, why);
                }
            }
        }
    }

    /// Does `hex` work as a surface: visible, raised, text readable on it? `slack` scales
    /// the minimums (1 when choosing a new colour, [`SLACK`] when judging the current one).
    fn passes(
        &self,
        hex: &str,
        bg: &str,
        visible: f32,
        texts: &[(&str, f32)],
        up: bool,
        slack: f32,
    ) -> bool {
        (Lch::from_hex(hex).l > Lch::from_hex(bg).l) == up
            && colour::contrast(hex, bg) >= visible * slack
            && texts
                .iter()
                .all(|(k, min)| colour::contrast(self.c.get(k), hex) >= *min * slack)
    }

    /// The passing lightness nearest `now`, on either side.
    fn find(
        &self,
        now: &str,
        bg: &str,
        visible: f32,
        texts: &[(&str, f32)],
        up: bool,
    ) -> Option<String> {
        nearest_any(now, |hex| self.passes(hex, bg, visible, texts, up, 1.0))
    }
}

/// `hex` with OKLCH lightness `l`, hue and chroma kept.
fn with_l(hex: &str, l: f32) -> String {
    Lch {
        l: l.clamp(0.0, 1.0),
        ..Lch::from_hex(hex)
    }
    .to_hex()
}

/// The nearest lightness away from the background (lighter if `up`) that passes `ok`.
fn nearest(hex: &str, ok: impl Fn(&str) -> bool, up: bool) -> Option<String> {
    let l = Lch::from_hex(hex).l;
    (1..=200).map(|i| i as f32 * 0.005).find_map(|d| {
        let out = with_l(hex, if up { l + d } else { l - d });
        ok(&out).then_some(out)
    })
}

/// The nearest lightness on either side that passes `ok`.
fn nearest_any(hex: &str, ok: impl Fn(&str) -> bool) -> Option<String> {
    let l = Lch::from_hex(hex).l;
    (1..=200).map(|i| i as f32 * 0.005).find_map(|d| {
        [l + d, l - d]
            .into_iter()
            .filter(|x| (0.0..=1.0).contains(x))
            .map(|x| with_l(hex, x))
            .find(|out| ok(out))
    })
}

/// The colours editors draw code in (VSCodium's token colours, Zed's syntax), all of which
/// can sit on a selection. Terminals don't need this: they swap in their own selected-text
/// colour.
pub const SYNTAX: [&str; 11] = [
    "foreground",
    "light_foreground",
    "dark_foreground",
    "accent",
    "red",
    "orange",
    "yellow",
    "green",
    "cyan",
    "blue",
    "magenta",
];

/// How visible an editor's selection must stay on its background (Minimal's selection
/// minimum, with the usual slack).
const EDITOR_SEL_VISIBLE: f32 = 1.3 * SLACK;

/// The selection for editors (ROADMAP 11h, colour lab idea 14): an editor draws syntax
/// colours on the selection, so a selection close to them hides them (purple-moon's `fn`,
/// #9b49c8 on #9060c3, is 1.1:1). The theme's selection is kept when the worst syntax colour
/// reads on it well enough: in Minimal, a colour under 2:1 (hidden) is what moves it; in
/// Strong, anything under the target; Off keeps it. The target is 3:1, or 85 % of what that
/// colour has on the background when that is lower. When it moves, its lightness slides towards the
/// background, keeping its hue, to the first step where that holds and it is still visible;
/// failing that, to the most readable visible step. Towards the background means darker on
/// dark themes and lighter on light ones, so light themes need no rule of their own.
pub fn editor_selection(c: &Colors, level: Level) -> String {
    selection_for(c, level, &SYNTAX)
}

/// The colours list apps draw on their selected line: lazygit's commit hashes, branches and
/// file states, fzf's matched letters (`hl+`), and the text itself.
pub const LIST: [&str; 9] = [
    "foreground",
    "bright_foreground",
    "accent",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
];

/// The selected line for list apps (F3, idea 19): like [`editor_selection`], for the
/// coloured text lazygit and fzf draw on it ([`LIST`]); purple-moon's red was 1.9:1 there.
pub fn list_selection(c: &Colors, level: Level) -> String {
    selection_for(c, level, &LIST)
}

/// The theme's selection, or one slid towards the background until every colour in `keys`
/// reads on it (the rules above [`editor_selection`]).
fn selection_for(c: &Colors, level: Level, keys: &[&str]) -> String {
    let sel = c.get("selection");
    let bg = c.get("background");
    let worst = |under: &str| {
        keys.iter()
            .map(|k| colour::contrast(c.get(k), under))
            .fold(f32::INFINITY, f32::min)
    };
    let need = 3.0_f32.min(worst(bg) * 0.85);
    let hidden = match level {
        Level::Off => return sel.to_string(),
        Level::Minimal => 2.0_f32.min(need),
        Level::Strong => need,
    };
    if worst(sel) >= hidden * SLACK {
        return sel.to_string();
    }
    let (from, to) = (Lch::from_hex(sel), Lch::from_hex(bg));
    let steps = 40;
    let mut best: Option<(f32, String)> = None;
    for i in 1..=steps {
        let hex = Lch {
            l: from.l + (to.l - from.l) * i as f32 / steps as f32,
            ..from
        }
        .to_hex();
        if colour::contrast(&hex, bg) < EDITOR_SEL_VISIBLE {
            break; // too close to the background from here on
        }
        let w = worst(&hex);
        if w >= need * SLACK {
            return hex;
        }
        if best.as_ref().is_none_or(|(bw, _)| w > *bw) {
            best = Some((w, hex));
        }
    }
    best.filter(|(w, _)| *w > worst(sel))
        .map_or_else(|| sel.to_string(), |(_, hex)| hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ash as Omarchy ships it (imported unchanged).
    const ASH: &str = r##"
accent = "#626262"
selection = "#0d0d0d"
muted = "#5a5a5a"
background = "#121212"
dark_background = "#0d0d0d"
darker_background = "#080808"
lighter_background = "#212121"
foreground = "#e0e0e0"
dark_foreground = "#adadad"
light_foreground = "#eaeaea"
bright_foreground = "#fafafa"
red = "#8a8a8a"
yellow = "#9e9e9e"
green = "#767676"
cyan = "#767676"
blue = "#626262"
magenta = "#8a8a8a"
bright_red = "#9e9e9e"
bright_yellow = "#b2b2b2"
bright_green = "#767676"
bright_cyan = "#9e9e9e"
bright_blue = "#626262"
bright_magenta = "#8a8a8a"
"##;

    fn ash() -> Colors {
        Colors::parse(ASH).unwrap()
    }

    #[test]
    fn ash_gets_a_visible_grey_selection_with_readable_text() {
        let mut c = ash();
        let changes = apply(&mut c, Level::Minimal, &BTreeSet::new());
        let sel = c.get("selection");
        assert!(colour::contrast(sel, "#121212") >= 1.3, "{sel}");
        assert!(colour::contrast("#fafafa", sel) >= 4.5, "{sel}");
        assert!(Lch::from_hex(sel).c < 0.01, "stays grey: {sel}");
        assert!(
            changes.iter().any(|ch| ch.key == "selection"),
            "{changes:?}"
        );
        // The greys that were fine stay as they are.
        assert_eq!(c.get("foreground"), "#e0e0e0");
        assert_eq!(c.get("lighter_background"), "#212121");
    }

    #[test]
    fn fixing_twice_changes_nothing_more() {
        for level in [Level::Minimal, Level::Strong] {
            let mut c = ash();
            apply(&mut c, level, &BTreeSet::new());
            let again = apply(&mut c, level, &BTreeSet::new());
            assert!(again.is_empty(), "{level:?}: {again:?}");
        }
    }

    #[test]
    fn off_and_your_edits_are_left_alone() {
        let mut c = ash();
        assert!(apply(&mut c, Level::Off, &BTreeSet::new()).is_empty());
        let keep = BTreeSet::from(["selection".to_string()]);
        let changes = apply(&mut c, Level::Minimal, &keep);
        assert_eq!(c.get("selection"), "#0d0d0d");
        assert!(changes.iter().all(|ch| ch.key != "selection"));
    }

    #[test]
    fn a_light_selection_meant_for_dark_text_is_darkened() {
        // A lavender selection on a dark theme, with light text on it (catppuccin-dark).
        let mut c = ash();
        c.set("background", "#1e1e2e");
        c.set("selection", "#b4befe");
        c.set("bright_foreground", "#f5f5f5");
        apply(&mut c, Level::Minimal, &BTreeSet::new());
        let sel = c.get("selection");
        assert!(colour::contrast("#f5f5f5", sel) >= 4.5, "{sel}");
        assert!(colour::contrast(sel, "#1e1e2e") >= 1.3, "{sel}");
        let (a, b) = (Lch::from_hex(sel), Lch::from_hex("#b4befe"));
        assert!((a.h - b.h).abs() < 8.0, "hue kept: {sel}");
    }

    #[test]
    fn shades_out_of_order_are_put_back() {
        let mut c = ash();
        c.set("dark_background", "#1a1a1a"); // lighter than the background
        apply(&mut c, Level::Minimal, &BTreeSet::new());
        assert!(Lch::from_hex(c.get("dark_background")).l < Lch::from_hex("#121212").l);
    }

    #[test]
    fn shades_the_wrong_way_round_are_swapped() {
        let mut c = ash();
        c.set("dark_background", "#050505");
        c.set("darker_background", "#0c0c0c");
        apply(&mut c, Level::Minimal, &BTreeSet::new());
        assert_eq!(c.get("dark_background"), "#0c0c0c");
        assert_eq!(c.get("darker_background"), "#050505");
    }

    #[test]
    fn a_white_background_keeps_its_grey_shades() {
        // The bundled "white": shades darker than #ffffff are right (nothing is lighter).
        let mut c = ash();
        for (k, v) in [
            ("background", "#ffffff"),
            ("dark_background", "#f5f5f5"),
            ("darker_background", "#e8e8e8"),
            ("lighter_background", "#c0c0c0"),
            ("foreground", "#000000"),
            ("light_foreground", "#1a1a1a"),
            ("bright_foreground", "#000000"),
            ("selection", "#d0d0d0"),
        ] {
            c.set(k, v);
        }
        let changes = apply(&mut c, Level::Minimal, &BTreeSet::new());
        assert_eq!(c.get("dark_background"), "#f5f5f5", "{changes:?}");
        assert_eq!(c.get("darker_background"), "#e8e8e8", "{changes:?}");
    }

    #[test]
    fn a_guessed_navy_never_becomes_a_light_themes_darkest_shade() {
        // solarized-light as imported: a navy "dark" shade and a cream "darker" one. The old
        // swap put the navy in `darker_background` (the tab bar); both stay a small step now.
        let mut c = ash();
        for (k, v) in [
            ("background", "#fdf6e3"),
            ("dark_background", "#073642"),
            ("darker_background", "#eee8d5"),
            ("lighter_background", "#eee8d5"),
            ("foreground", "#586e75"),
            ("light_foreground", "#586e75"),
            ("bright_foreground", "#002b36"),
            ("selection", "#d6d0bd"),
        ] {
            c.set(k, v);
        }
        let changes = apply(&mut c, Level::Minimal, &BTreeSet::new());
        let l = |k: &str| Lch::from_hex(c.get(k)).l;
        let bg = Lch::from_hex("#fdf6e3").l;
        assert_eq!(c.get("dark_background"), "#eee8d5", "{changes:?}");
        assert!(l("darker_background") < l("dark_background"));
        assert!(l("dark_background") - l("darker_background") <= SHADE_STEP);
        assert!(bg - l("dark_background") <= SHADE_STEP);
        // Running again changes nothing.
        assert!(apply(&mut c, Level::Minimal, &BTreeSet::new()).is_empty());
    }

    #[test]
    fn a_dark_theme_keeps_black_as_its_darkest_shade() {
        let mut c = ash();
        c.set("dark_background", "#0a0a0a");
        c.set("darker_background", "#000000");
        apply(&mut c, Level::Minimal, &BTreeSet::new());
        assert_eq!(c.get("darker_background"), "#000000");
    }

    #[test]
    fn check_asks_panel_text_what_the_level_fixes_it_to() {
        // Text at about 3.5:1 on a panel: fine for Minimal (3:1), low for Strong (4.5:1).
        let mut c = ash();
        c.set("lighter_background", "#505050");
        c.set("foreground", "#a8a8a8");
        let panel = |l: Level| {
            crate::check::colors_at("t", &c, l)
                .issues
                .iter()
                .any(|i| i.bg == "lighter_background")
        };
        assert!(!panel(Level::Minimal));
        assert!(panel(Level::Strong));
        assert!(panel(Level::Off));
    }

    #[test]
    fn light_themes_move_the_other_way() {
        let mut c = ash();
        c.set("background", "#eff1f5");
        c.set("foreground", "#4c4f69");
        c.set("bright_foreground", "#1e1e1e");
        c.set("light_foreground", "#5c5f77");
        c.set("lighter_background", "#e6e9ef");
        c.set("selection", "#eef0f4"); // the background's shade: invisible
        c.set("dark_background", "#e6e9ef");
        c.set("darker_background", "#dce0e8");
        apply(&mut c, Level::Minimal, &BTreeSet::new());
        let sel = c.get("selection");
        assert!(
            Lch::from_hex(sel).l < Lch::from_hex("#eff1f5").l,
            "darker: {sel}"
        );
        assert!(colour::contrast(sel, "#eff1f5") >= 1.3);
        assert!(colour::contrast("#1e1e1e", sel) >= 4.5);
    }

    #[test]
    fn strong_lifts_line_numbers_and_comments() {
        let mut c = ash();
        c.set("dark_foreground", "#444444");
        let mut s = c.clone();
        apply(&mut c, Level::Minimal, &BTreeSet::new());
        apply(&mut s, Level::Strong, &BTreeSet::new());
        assert!(colour::contrast(c.get("dark_foreground"), "#121212") >= 2.5);
        assert!(colour::contrast(s.get("dark_foreground"), "#121212") >= 4.5);
        assert!(colour::contrast(s.get("muted"), "#121212") >= 3.0);
    }

    /// Phase 11b: every catalogue theme, at both levels, ends up with what the levels
    /// promise, and a second run changes nothing. Lists every failure at once.
    #[test]
    fn every_catalogue_theme_meets_its_minimums() {
        use crate::monochrome::Mono;
        let mut failures = Vec::new();
        for e in crate::catalog::entries() {
            // Monochrome (11f): the default (greys) and both switches on.
            let both = crate::monochrome::Mono {
                greys: true,
                tint: true,
            };
            for (level, mono) in [
                (Level::Minimal, Mono::default()),
                (Level::Strong, Mono::default()),
                (Level::Minimal, both),
            ] {
                let m = level.mins();
                let mut c = e.colors.clone();
                let roles = if e.derived.is_some() {
                    Roles::Guessed
                } else {
                    Roles::Named
                };
                apply_all(&mut c, level, mono, &BTreeSet::new(), roles);
                let bg = c.get("background").to_string();
                let up = Lch::from_hex(&bg).l < 0.5;
                let mut fail =
                    |what: String| failures.push(format!("{} {level:?}: {what}", e.name));
                let cr = |a: &str, b: &str| colour::contrast(c.get(a), c.get(b));
                for (key, min) in [
                    ("foreground", m.body),
                    ("light_foreground", m.body),
                    ("bright_foreground", m.body),
                    ("dark_foreground", m.secondary),
                    ("muted", m.comments),
                ]
                .into_iter()
                .chain(COLOURED.iter().map(|k| (*k, m.coloured)))
                {
                    if cr(key, "background") < min * SLACK {
                        fail(format!("{key} {:.2} < {min}", cr(key, "background")));
                    }
                }
                let sel = c.get("selection");
                let raised = |k: &str| (Lch::from_hex(c.get(k)).l > Lch::from_hex(&bg).l) == up;
                if cr("selection", "background") < m.selection * SLACK || !raised("selection") {
                    fail(format!(
                        "selection {sel} not visible ({:.2})",
                        cr("selection", "background")
                    ));
                }
                if cr("bright_foreground", "selection") < m.body * SLACK {
                    fail(format!(
                        "text on selection {sel} {:.2}",
                        cr("bright_foreground", "selection")
                    ));
                }
                if cr("lighter_background", "background") < m.panel * SLACK
                    || !raised("lighter_background")
                {
                    fail(format!("panel {} not visible", c.get("lighter_background")));
                }
                let black = Lch::from_hex(&bg).l < 0.03;
                let deep = |a: &str, b: &str| {
                    let (x, y) = (c.get(a), c.get(b));
                    black || x == y || Lch::from_hex(x).l < Lch::from_hex(y).l
                };
                if !deep("dark_background", "background")
                    || !deep("darker_background", "dark_background")
                {
                    fail("shades out of order".into());
                }
                let again = apply_all(&mut c, level, mono, &BTreeSet::new(), roles);
                if !again.is_empty() {
                    fail(format!("not stable: {}", again[0].summary()));
                }
                // 11e: every theme in order, named ones too.
                let problems = ranking::order_problems(&c);
                if !problems.is_empty() {
                    fail(format!("order: {problems:?}"));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} failures:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    fn catalogue(name: &str) -> Colors {
        let e = crate::catalog::entries()
            .iter()
            .find(|e| e.name == name)
            .expect("in the catalogue");
        let mut c = e.colors.clone();
        apply_with(
            &mut c,
            Level::Minimal,
            &BTreeSet::new(),
            if e.derived.is_some() {
                Roles::Guessed
            } else {
                Roles::Named
            },
        );
        c
    }

    fn worst_syntax(c: &Colors, under: &str) -> f32 {
        SYNTAX
            .iter()
            .map(|k| colour::contrast(c.get(k), under))
            .fold(f32::INFINITY, f32::min)
    }

    #[test]
    fn purple_moons_keywords_read_on_the_editor_selection() {
        let c = catalogue("purple-moon");
        assert!(worst_syntax(&c, c.get("selection")) < 1.5);
        let es = editor_selection(&c, Level::Minimal);
        assert!(worst_syntax(&c, &es) >= 2.0, "{es}");
        assert!(colour::contrast(&es, c.get("background")) >= EDITOR_SEL_VISIBLE);
        assert!(
            Lch::from_hex(&es).l < Lch::from_hex(c.get("selection")).l,
            "darker on a dark theme"
        );
        assert_eq!(editor_selection(&c, Level::Off), c.get("selection"));
    }

    #[test]
    fn a_light_themes_editor_selection_moves_lighter() {
        let c = catalogue("ristretto-light");
        let es = editor_selection(&c, Level::Minimal);
        assert!(
            Lch::from_hex(&es).l > Lch::from_hex(c.get("selection")).l,
            "{es}"
        );
    }

    #[test]
    fn purple_moons_red_reads_on_lazygits_selected_line() {
        let c = catalogue("purple-moon");
        let worst = |under: &str| {
            LIST.iter()
                .map(|k| colour::contrast(c.get(k), under))
                .fold(f32::INFINITY, f32::min)
        };
        assert!(worst(c.get("selection")) < 2.0);
        let ls = list_selection(&c, Level::Minimal);
        assert!(worst(&ls) >= 2.0, "{ls}");
        assert!(colour::contrast(&ls, c.get("background")) >= EDITOR_SEL_VISIBLE);
    }

    #[test]
    fn every_editor_selection_is_visible_and_never_less_readable() {
        for e in crate::catalog::entries() {
            let c = catalogue(&e.name);
            for level in [Level::Minimal, Level::Strong] {
                let sel = c.get("selection");
                let es = editor_selection(&c, level);
                if es == sel {
                    continue;
                }
                assert!(
                    worst_syntax(&c, &es) > worst_syntax(&c, sel),
                    "{} {es}",
                    e.name
                );
                assert!(
                    colour::contrast(&es, c.get("background")) >= EDITOR_SEL_VISIBLE,
                    "{} {es}",
                    e.name
                );
            }
        }
    }
}
