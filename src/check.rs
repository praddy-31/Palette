//! `palette check [theme]`: a contrast report. Every colour pair a theme is
//! used for (text on the background, coloured text, selected text, comments)
//! is measured with WCAG contrast, and the pairs below the minimum for how
//! Palette uses them are listed with what Palette does about it and a
//! suggested colour. Separate from `palette doctor`, which checks wiring.
//!
//! Minimums (WCAG 2): 4.5 for body text, 3 for coloured and UI text
//! ([`colour::READABLE`]). Secondary text and comments are meant to recede
//! (WCAG exempts inactive text), so they get lower bars, set where they stop
//! being readable at all: [`SECONDARY`], [`COMMENTS`]. Tuned on the bundled
//! themes so that a badge means "hard to read", not "dim by design".

use std::collections::BTreeSet;

use anyhow::Result;

use crate::colour::{self, Lch};
use crate::normalise::Level;
use crate::paths::Paths;
use crate::theme::{Colors, Theme};
use crate::{state, theme};

/// Body text (WCAG AA).
pub const BODY: f32 = 4.5;
/// Secondary text (line numbers, inactive tabs): quiet, but legible.
pub const SECONDARY: f32 = 2.5;
/// Comments and autosuggestions: the quietest text, but still there.
pub const COMMENTS: f32 = 1.6;

/// One rule: `fg` text on a `bg` fill, needing `min`.
struct Rule {
    fg: &'static str,
    bg: &'static str,
    min: f32,
    /// Where Palette shows this pair.
    used_for: &'static str,
    /// What Palette already does about a low pair, if anything.
    applied: Option<&'static str>,
}

const COLOURED: Option<&str> = Some(
    "headings, prompts and highlights pick readable colours (lifted to 3:1); \
     terminals show it as is",
);

const RULES: &[Rule] = &[
    Rule {
        fg: "foreground",
        bg: "background",
        min: BODY,
        used_for: "body text everywhere",
        applied: None,
    },
    Rule {
        fg: "foreground",
        bg: "lighter_background",
        min: BODY,
        used_for: "text in panels, tabs and popups",
        applied: None,
    },
    Rule {
        fg: "dark_foreground",
        bg: "background",
        min: SECONDARY,
        used_for: "secondary text: line numbers, status lines",
        applied: None,
    },
    Rule {
        fg: "muted",
        bg: "background",
        min: COMMENTS,
        used_for: "comments and autosuggestions",
        applied: None,
    },
    Rule {
        fg: "accent",
        bg: "background",
        min: colour::READABLE,
        used_for: "accent text, links, the active border",
        applied: COLOURED,
    },
    Rule {
        fg: "red",
        bg: "background",
        min: colour::READABLE,
        used_for: "coloured text (errors, diffs)",
        applied: COLOURED,
    },
    Rule {
        fg: "yellow",
        bg: "background",
        min: colour::READABLE,
        used_for: "coloured text (warnings)",
        applied: COLOURED,
    },
    Rule {
        fg: "green",
        bg: "background",
        min: colour::READABLE,
        used_for: "coloured text (strings, success)",
        applied: COLOURED,
    },
    Rule {
        fg: "cyan",
        bg: "background",
        min: colour::READABLE,
        used_for: "coloured text",
        applied: COLOURED,
    },
    Rule {
        fg: "blue",
        bg: "background",
        min: colour::READABLE,
        used_for: "coloured text (directories, functions)",
        applied: COLOURED,
    },
    Rule {
        fg: "magenta",
        bg: "background",
        min: colour::READABLE,
        used_for: "coloured text (keywords)",
        applied: COLOURED,
    },
    Rule {
        fg: "foreground",
        bg: "selection",
        min: colour::READABLE,
        used_for: "selected lines in editors, tmux, lazygit",
        applied: None,
    },
    Rule {
        fg: "bright_foreground",
        bg: "selection",
        min: BODY,
        used_for: "selected text in terminals and fzf",
        applied: None,
    },
];

/// A pair below its minimum.
#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    /// Colour keys: the text and what it sits on.
    pub fg: &'static str,
    pub bg: &'static str,
    pub fg_hex: String,
    pub bg_hex: String,
    /// WCAG contrast, rounded down to one decimal (as shown).
    pub ratio: f32,
    pub min: f32,
    pub used_for: &'static str,
    /// What Palette already does about it, if anything.
    pub applied: Option<&'static str>,
    /// The nearest lighter/darker `fg` (same hue) that reaches `min`.
    pub suggested: String,
}

impl Issue {
    /// `red on background 2.4:1 (needs 3): coloured text …`
    pub fn summary(&self) -> String {
        format!(
            "{} on {} {:.1}:1 (needs {}): {}",
            self.fg, self.bg, self.ratio, self.min, self.used_for
        )
    }

    /// What to do: Palette's own fix, and the suggested colour.
    pub fn fix(&self) -> String {
        let suggest = format!("try {} = \"{}\"", self.fg, self.suggested);
        match self.applied {
            Some(done) => format!("Palette: {done}. To fix the theme, {suggest}"),
            None => format!("To fix the theme, {suggest}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Report {
    pub theme: String,
    /// Pairs measured.
    pub checked: usize,
    /// Pairs still low after Palette's readability fixes.
    pub issues: Vec<Issue>,
    /// What Palette's readability fixes change when this theme is used (`normalise`).
    pub fixed: Vec<crate::normalise::Change>,
    /// See-through apps (11g): what the glass lift changes in each one's file, for the
    /// wallpaper this theme shows (only for the active theme's or its own wallpaper).
    /// Apps with the same lift share one entry: "dispatch, ghostty, zed".
    pub glass: Vec<(String, Vec<String>)>,
}

impl Report {
    /// One line: all good, or how many pairs are low.
    pub fn verdict(&self) -> String {
        match self.issues.len() {
            0 => format!(
                "{}: all {} colour pairs are readable",
                self.theme, self.checked
            ),
            1 => format!("{}: 1 low-contrast pair", self.theme),
            n => format!("{}: {n} low-contrast pairs", self.theme),
        }
    }
}

/// Check a theme's colours as its author made them (the standard minimums: 4.5 for text on
/// panels too). [`colors_at`] checks them against what a readability level asks.
pub fn colors(name: &str, c: &Colors) -> Report {
    colors_at(name, c, Level::Strong)
}

/// Check a theme's colours against `level`'s minimums: text on panels needs 3:1 in Minimal
/// (what the fixes aim for there) and 4.5:1 otherwise, so the check never flags a pair the
/// fixes left as they meant to (11d).
pub fn colors_at(name: &str, c: &Colors, level: Level) -> Report {
    let issues = RULES
        .iter()
        .filter_map(|r| {
            let (fg, bg) = (c.get(r.fg), c.get(r.bg));
            let min = if r.bg == "lighter_background" {
                level.panel_text()
            } else {
                r.min
            };
            let r = &Rule { min, ..*r };
            // Within 3 % of the minimum counts as fine, as for the readability fixes
            // (`normalise::SLACK`), so a pair Palette leaves alone isn't flagged either.
            let exact = colour::contrast(fg, bg);
            if exact >= r.min * crate::normalise::SLACK {
                return None;
            }
            // Rounded down to what is shown, so 2.97 reads "2.9", not a failing "3.0".
            let ratio = (exact * 10.0).floor() / 10.0;
            Some(Issue {
                fg: r.fg,
                bg: r.bg,
                fg_hex: fg.to_string(),
                bg_hex: bg.to_string(),
                ratio,
                min: r.min,
                used_for: r.used_for,
                applied: r.applied,
                suggested: reach(fg, bg, r.min),
            })
        })
        .collect();
    Report {
        theme: name.to_string(),
        checked: RULES.len(),
        issues,
        fixed: Vec::new(),
        glass: Vec::new(),
    }
}

pub fn theme(t: &Theme) -> Report {
    colors(&t.name, &t.colors)
}

/// `palette check [theme]`: the named theme, or the active one, as it's used: what
/// Palette's readability fixes change, and what's still low after them.
pub fn run(paths: &Paths, name: Option<&str>) -> Result<Report> {
    let name = match name {
        Some(n) => n.to_string(),
        None => state::current_theme(paths)?.ok_or_else(|| {
            anyhow::anyhow!("no theme is active yet: name one (`palette check <theme>`)")
        })?,
    };
    let (t, fixed) = theme::load_fixed(paths, &name)?;
    let level = crate::settings::load(paths).unwrap_or_default().readability;
    let mut report = colors_at(&t.name, &t.colors, level);
    report.fixed = fixed;
    let shown = crate::effects::wallpaper_to_use(paths, &t, None);
    // Apps at the same opacity get the same lift: one block for all of them.
    for (app, lift) in crate::glass::lifts(paths, &t, shown.as_deref()) {
        let lines = lift.lines();
        match report.glass.iter_mut().find(|(_, l)| *l == lines) {
            Some((apps, _)) => *apps = format!("{apps}, {app}"),
            None => report.glass.push((app.to_string(), lines)),
        }
    }
    Ok(report)
}

/// `fg` made lighter (on a dark `bg`) or darker (on a light one), keeping its
/// hue, until it reaches `min` contrast on `bg`. Like `colour::readable`,
/// but for any minimum.
fn reach(fg: &str, bg: &str, min: f32) -> String {
    let mut lch = Lch::from_hex(fg);
    let up = Lch::from_hex(bg).l < 0.5;
    let mut out = lch.to_hex();
    for _ in 0..50 {
        if colour::contrast(&out, bg) >= min + 0.05 {
            break;
        }
        lch.l = (lch.l + if up { 0.02 } else { -0.02 }).clamp(0.0, 1.0);
        out = lch.to_hex();
    }
    out
}

/// One kind of app's verdict (11i, colour lab idea 11): how this theme looks in what that
/// app actually draws, after the fixes, the editor selection and any glass lift.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AppVerdict {
    /// "Terminals", "Editors", "Lists (tmux, lazygit, fzf, btop)", "Palette's windows".
    pub app: &'static str,
    /// "ok", "weak" (works, with something faint) or "bad" (something can't be seen).
    pub grade: &'static str,
    /// What's wrong, one short phrase each; empty when ok.
    pub why: Vec<String>,
}

/// `palette check --app`: one verdict per kind of app for `name` (default: the active theme).
pub fn apps(paths: &Paths, name: Option<&str>) -> Result<Vec<AppVerdict>> {
    let name = match name {
        Some(n) => n.to_string(),
        None => state::current_theme(paths)?.ok_or_else(|| {
            anyhow::anyhow!("no theme is active yet: name one (`palette check <theme> --app`)")
        })?,
    };
    let (t, _) = theme::load_fixed(paths, &name)?;
    let settings = crate::settings::load(paths).unwrap_or_default();
    let level = if settings.original_colours.contains(&name) {
        Level::Off
    } else {
        settings.readability
    };
    let shown = crate::effects::wallpaper_to_use(paths, &t, None);
    let lifts = crate::glass::lifts(paths, &t, shown.as_deref());
    let glass_note = |apps: &[&str]| -> Option<String> {
        let faint: BTreeSet<&str> = apps
            .iter()
            .filter_map(|a| lifts.get(a))
            .flat_map(|l| l.faint.iter().copied())
            .collect();
        (!faint.is_empty()).then(|| {
            let list: Vec<&str> = faint.into_iter().collect();
            format!(
                "on glass: {} can't read (raise the opacity)",
                list.join(", ")
            )
        })
    };
    Ok(app_verdicts(&t.colors, level, &glass_note))
}

/// [`apps`] for given colours, without files (the glass note comes from the caller).
pub fn app_verdicts(
    c: &Colors,
    level: Level,
    glass_note: &dyn Fn(&[&str]) -> Option<String>,
) -> Vec<AppVerdict> {
    use crate::normalise::SLACK;
    // Off still judges by Minimal's bar: it says what you'd see, it changes nothing.
    let (body, secondary, _comments, coloured) = level
        .minimums()
        .unwrap_or_else(|| Level::Minimal.minimums().expect("minimal has minimums"));
    let cr = |a: &str, b: &str| colour::contrast(a, b);
    let bg = c.get("background");
    let sel = c.get("selection");
    let seen = |s: &str| cr(s, bg) >= 1.3 * SLACK;
    const ANSI: [&str; 6] = ["red", "green", "yellow", "blue", "magenta", "cyan"];
    let worst_on = |keys: &[&'static str], under: &str| -> (f32, &'static str) {
        keys.iter()
            .map(|k| (cr(c.get(k), under), *k))
            .fold((f32::INFINITY, ""), |a, b| if b.0 < a.0 { b } else { a })
    };
    let verdict = |app, bad: Vec<String>, weak: Vec<String>| {
        let grade = if !bad.is_empty() {
            "bad"
        } else if !weak.is_empty() {
            "weak"
        } else {
            "ok"
        };
        AppVerdict {
            app,
            grade,
            why: bad.into_iter().chain(weak).collect(),
        }
    };

    // Terminals: the selection is the only marker, its text is the brightest text.
    let mut bad = Vec::new();
    let mut weak = Vec::new();
    if !seen(sel) {
        bad.push(format!("selection invisible ({:.1}:1)", cr(sel, bg)));
    }
    let on_sel = cr(c.get("bright_foreground"), sel);
    if on_sel < body * SLACK {
        bad.push(format!("selected text {on_sel:.1}:1"));
    }
    let faint: Vec<&str> = ANSI
        .iter()
        .copied()
        .filter(|k| cr(c.get(k), bg) < coloured * SLACK)
        .collect();
    if !faint.is_empty() {
        weak.push(format!("faint colours: {}", faint.join(", ")));
    }
    weak.extend(glass_note(&["ghostty", "kitty", "alacritty", "wezterm"]));
    let terminals = verdict("Terminals", bad, weak);

    // Editors: code stays in its colours on their own selection (11h); line numbers.
    let es = crate::normalise::editor_selection(c, level);
    let mut bad = Vec::new();
    let mut weak = Vec::new();
    if !seen(&es) {
        weak.push(format!("selection faint ({:.1}:1)", cr(&es, bg)));
    }
    let (w, key) = worst_on(&crate::normalise::SYNTAX, &es);
    if w < 2.0 {
        bad.push(format!("{key} hides on the selection ({w:.1}:1)"));
    } else if w < coloured * SLACK {
        weak.push(format!("{key} faint on the selection ({w:.1}:1)"));
    }
    let ln = cr(c.get("dark_foreground"), bg);
    if ln < secondary * SLACK {
        weak.push(format!("line numbers {ln:.1}:1"));
    }
    weak.extend(glass_note(&["zed"]));
    let editors = verdict("Editors (VSCodium, Zed)", bad, weak);

    // Lists: text and coloured text on their own selected line (F3: lazygit, fzf; tmux and
    // btop draw only the brightest text on the theme's selection).
    let ls = crate::normalise::list_selection(c, level);
    let mut bad = Vec::new();
    let mut weak = Vec::new();
    if !seen(&ls) {
        bad.push("selection invisible".to_string());
    }
    let text = cr(c.get("foreground"), &ls);
    if text < 3.0 * SLACK {
        bad.push(format!("text on the selection {text:.1}:1"));
    }
    let (w, key) = worst_on(&crate::normalise::LIST, &ls);
    if w < 2.0 {
        weak.push(format!("{key} hides on the selected line ({w:.1}:1)"));
    }
    let lists = verdict("Lists (tmux, lazygit, fzf, btop)", bad, weak);

    // Palette's own windows (11c): the brightest text on the selection, the background's
    // colour on the accent (primary buttons).
    let mut bad = Vec::new();
    if on_sel < body * SLACK {
        bad.push(format!("selected rows {on_sel:.1}:1"));
    }
    let button = cr(bg, c.get("accent"));
    if button < 3.0 * SLACK {
        bad.push(format!("button text {button:.1}:1"));
    }
    let palette = verdict("Palette's windows", bad, Vec::new());

    vec![terminals, editors, lists, palette]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundled(name: &str) -> Report {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        run(&paths, Some(name)).unwrap()
    }

    #[test]
    fn tokyo_night_reads_well() {
        let r = bundled("tokyo-night");
        assert_eq!(r.checked, RULES.len());
        assert!(
            r.issues
                .iter()
                .all(|i| i.fg != "foreground" || i.bg != "background"),
            "{:?}",
            r.issues
        );
    }

    #[test]
    fn low_pairs_are_found_and_fixed() {
        // Grey text on the dark background: far below 4.5.
        let src: String = include_str!("../themes/tokyo-night/colors.toml")
            .lines()
            .map(|l| {
                if l.trim_start().starts_with("foreground ") || l.starts_with("foreground=") {
                    "foreground = \"#3b3d4a\"".to_string()
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let parsed = Colors::parse(&src).unwrap();
        assert_eq!(parsed.get("foreground"), "#3b3d4a");
        let r = colors("test", &parsed);
        let body = r
            .issues
            .iter()
            .find(|i| i.fg == "foreground" && i.bg == "background")
            .expect("foreground on background flagged");
        assert!(body.ratio < BODY);
        assert!(colour::contrast(&body.suggested, &body.bg_hex) >= BODY);
        assert!(r.verdict().contains("low-contrast"));
        assert!(body.fix().contains("try foreground"));
    }

    fn fixed(name: &str, level: Level) -> Colors {
        let e = crate::catalog::entries()
            .iter()
            .find(|e| e.name == name)
            .expect("in the catalogue");
        let mut c = e.colors.clone();
        let roles = if e.derived.is_some() {
            crate::ranking::Roles::Guessed
        } else {
            crate::ranking::Roles::Named
        };
        crate::normalise::apply_with(&mut c, level, &BTreeSet::new(), roles);
        c
    }

    #[test]
    fn purple_moon_works_in_editors_after_11h_and_says_why_without_fixes() {
        let none = |_: &[&str]| None;
        let fixedc = fixed("purple-moon", Level::Minimal);
        let v = app_verdicts(&fixedc, Level::Minimal, &none);
        let editors = v.iter().find(|v| v.app.starts_with("Editors")).unwrap();
        assert_ne!(editors.grade, "bad", "{editors:?}");

        let raw = crate::catalog::entries()
            .iter()
            .find(|e| e.name == "purple-moon")
            .unwrap()
            .colors
            .clone();
        let v = app_verdicts(&raw, Level::Off, &none);
        let editors = v.iter().find(|v| v.app.starts_with("Editors")).unwrap();
        assert_eq!(editors.grade, "bad", "{editors:?}");
        assert!(
            editors.why[0].contains("hides on the selection"),
            "{editors:?}"
        );
    }

    #[test]
    fn with_the_fixes_no_catalogue_theme_is_bad_in_terminals_or_palette() {
        let none = |_: &[&str]| None;
        let mut grades = std::collections::BTreeMap::new();
        for e in crate::catalog::entries() {
            for v in app_verdicts(&fixed(&e.name, Level::Minimal), Level::Minimal, &none) {
                *grades.entry((v.app, v.grade)).or_insert(0) += 1;
                if v.app == "Terminals" || v.app == "Palette's windows" {
                    assert_ne!(v.grade, "bad", "{}: {v:?}", e.name);
                }
            }
        }
        assert!(grades.values().sum::<i32>() > 0);
    }
}
