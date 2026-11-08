//! Search themes by how they look (ROADMAP 12c): "warm dark", "purple", "calm light".
//! Every theme gets a few words from its colours (judged in OKLCH, `src/colour.rs`); a
//! search matches when every word typed is the start of one of those words or part of the
//! theme's name. The app, the CLI (`palette list warm dark`, `palette catalog warm`) and the
//! inspection sheet all use these words, so a search finds the same themes everywhere.

use crate::colour::{self, Lch, Mood};
use crate::theme::{Colors, Mode};

/// The colour names, by OKLCH hue (degrees, upper bound); pink wraps around 0.
const HUES: [(f32, &str); 8] = [
    (12.0, "pink"),
    (45.0, "red"),
    (80.0, "orange"),
    (120.0, "yellow"),
    (170.0, "green"),
    (230.0, "teal"),
    (285.0, "blue"),
    (340.0, "purple"),
];

/// A hue's name ("grey" when there's barely any colour).
fn hue_name(l: Lch, grey_below: f32) -> &'static str {
    if l.c < grey_below {
        return "grey";
    }
    HUES.iter()
        .find(|(top, _)| l.h < *top)
        .map_or("pink", |(_, n)| n)
}

/// +1 warm (pink to yellow), -1 cool (green to blue), 0 for purple and greys.
fn warmth(l: Lch, grey_below: f32) -> i32 {
    if l.c < grey_below {
        0
    } else if l.h >= 340.0 || l.h < 120.0 {
        1
    } else if (150.0..300.0).contains(&l.h) {
        -1
    } else {
        0
    }
}

/// A theme's words, aliases included (`vivid` and `colourful`, `mono` and `monochrome`).
pub fn tags(c: &Colors) -> Vec<&'static str> {
    let mut out = Vec::new();
    out.push(if c.mode == Mode::Light {
        "light"
    } else {
        "dark"
    });
    let mood = colour::analyse_colors(c).mood;
    out.extend(match mood {
        Mood::Colourful => ["colourful", "colorful", "vivid"].as_slice(),
        Mood::Muted => ["muted", "calm"].as_slice(),
        Mood::Monochrome => ["monochrome", "mono", "grey", "gray"].as_slice(),
    });
    let (bg, accent, fg) = (
        Lch::from_hex(c.get("background")),
        Lch::from_hex(c.get("accent")),
        Lch::from_hex(c.get("foreground")),
    );
    // Background and text fill most of the screen, so they count double (gruvbox: grey
    // background, cream text, teal accent → warm).
    let score = 2 * warmth(bg, 0.015) + 2 * warmth(fg, 0.02) + warmth(accent, 0.04);
    if score >= 1 {
        out.push("warm");
    } else if score <= -1 {
        out.extend(["cool", "cold"]);
    }
    let ratio = colour::contrast(c.get("foreground"), c.get("background"));
    if ratio >= 12.0 {
        out.extend(["high-contrast", "contrast"]);
    } else if ratio < 7.0 {
        out.extend(["low-contrast", "soft"]);
    }
    for name in [hue_name(accent, 0.04), hue_name(bg, 0.02)] {
        let names: Vec<&'static str> = match name {
            "teal" => vec!["teal", "cyan"],
            "purple" => vec!["purple", "violet"],
            "grey" => vec!["grey", "gray"],
            n => vec![n],
        };
        for n in names {
            if !out.contains(&n) {
                out.push(n);
            }
        }
    }
    out
}

/// Does `query` find this theme? Every word must start one of its `tags` or be part of
/// its `name` (any case). An empty query finds everything.
pub fn matches(name: &str, tags: &[&str], query: &str) -> bool {
    let name = name.to_lowercase();
    query.split_whitespace().all(|w| {
        let w = w.to_lowercase();
        name.contains(&w) || tags.iter().any(|t| t.starts_with(&w))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundled(name: &str) -> Vec<&'static str> {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::Paths::fake_home(home.path());
        tags(&crate::theme::load(&paths, name).unwrap().colors)
    }

    #[test]
    fn themes_get_the_words_you_would_use() {
        let tokyo = bundled("tokyo-night");
        assert!(
            tokyo.contains(&"dark") && tokyo.contains(&"cool"),
            "{tokyo:?}"
        );
        assert!(tokyo.contains(&"blue"), "{tokyo:?}");
        let gruvbox = bundled("gruvbox-dark");
        assert!(gruvbox.contains(&"warm"), "{gruvbox:?}");
        let latte = bundled("catppuccin-latte");
        assert!(latte.contains(&"light"), "{latte:?}");
        let vanta = bundled("vantablack");
        assert!(vanta.contains(&"mono"), "{vanta:?}");
    }

    #[test]
    fn every_word_must_match_a_tag_start_or_the_name() {
        let t = ["dark", "warm", "orange"];
        assert!(matches("gruvbox-dark", &t, "warm dark"));
        assert!(matches("gruvbox-dark", &t, "WAR"));
        assert!(matches("gruvbox-dark", &t, "gruv warm"));
        assert!(!matches("gruvbox-dark", &t, "warm light"));
        assert!(matches("gruvbox-dark", &t, ""));
    }

    #[test]
    fn the_catalogue_splits_into_useful_groups() {
        // A search word is only useful if it neither finds nothing nor everything.
        let all: Vec<Vec<&str>> = crate::catalog::entries()
            .iter()
            .map(|e| tags(&e.colors))
            .collect();
        for w in [
            "dark", "light", "warm", "cool", "calm", "vivid", "blue", "purple",
        ] {
            let n = all
                .iter()
                .filter(|t| t.iter().any(|x| x.starts_with(w)))
                .count();
            assert!(n > 0 && n < all.len(), "{w}: {n} of {}", all.len());
        }
    }
}
