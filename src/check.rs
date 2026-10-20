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

use anyhow::Result;

use crate::colour::{self, Lch};
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
    pub issues: Vec<Issue>,
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

/// Check a theme's colours.
pub fn colors(name: &str, c: &Colors) -> Report {
    let issues = RULES
        .iter()
        .filter_map(|r| {
            let (fg, bg) = (c.get(r.fg), c.get(r.bg));
            // Rounded down to what is shown, so 2.97 reads "2.9", not a failing "3.0".
            let ratio = (colour::contrast(fg, bg) * 10.0).floor() / 10.0;
            if ratio >= r.min {
                return None;
            }
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
    }
}

pub fn theme(t: &Theme) -> Report {
    colors(&t.name, &t.colors)
}

/// `palette check [theme]`: the named theme, or the active one.
pub fn run(paths: &Paths, name: Option<&str>) -> Result<Report> {
    let t = match name {
        Some(n) => theme::load(paths, n)?,
        None => {
            let n = state::current_theme(paths)?.ok_or_else(|| {
                anyhow::anyhow!("no theme is active yet: name one (`palette check <theme>`)")
            })?;
            theme::load(paths, &n)?
        }
    };
    Ok(theme(&t))
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
}
