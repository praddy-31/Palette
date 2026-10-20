//! Looking at a theme's colours the way an eye does, not by their names.
//! Omarchy themes often name colours loosely (Osaka Jade's `yellow` is green,
//! its `blue` is the accent), so templates that pick "h1 = accent, h2 = blue…"
//! can end up all one colour. This module works in OKLCH (perceptual lightness,
//! chroma, hue) and WCAG contrast, and derives:
//!
//! - `mood`: colourful, muted or monochrome
//! - `family`: six readable colours that belong together (the accent and its
//!   nearest distinct neighbours on the colour wheel; shades of the accent for
//!   monochrome themes), for headings and prompt segments
//! - `pop`, `pop2`: readable colours that stand apart from the family, for
//!   italics, bold, highlights
//! - `signature`: the first three of the family (starship
//!   `primary/secondary/tertiary`)

use crate::theme::Theme;

/// How colourful a theme is, judged from its normal and bright colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    /// Several clearly different, saturated hues (Catppuccin, Tokyo Night).
    Colourful,
    /// Hues present but soft or few (Everforest, Nord, Kanagawa).
    Muted,
    /// One hue or none (Solitude, Vantablack, Lumon).
    Monochrome,
}

impl Mood {
    pub fn as_str(self) -> &'static str {
        match self {
            Mood::Colourful => "colourful",
            Mood::Muted => "muted",
            Mood::Monochrome => "monochrome",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Analysis {
    pub mood: Mood,
    pub family: [String; 6],
    pub pop: String,
    pub pop2: String,
    pub signature: [String; 3],
}

/// Minimum contrast for coloured text on the theme background (WCAG "large
/// text"/UI level: headings, prompt text).
pub const READABLE: f32 = 3.0;

pub fn analyse(theme: &Theme) -> Analysis {
    analyse_colors(&theme.colors)
}

/// [`analyse`] for colours that aren't a saved theme yet.
pub fn analyse_colors(c: &crate::theme::Colors) -> Analysis {
    let bg = c.get("background");
    let accent = readable(c.get("accent"), bg);
    let names = [
        "red",
        "orange",
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
    // Distinct, readable candidates (the accent first).
    let mut pool: Vec<Lch> = vec![Lch::from_hex(&accent)];
    for name in names {
        let lch = Lch::from_hex(&readable(c.get(name), bg));
        if pool.iter().all(|p| p.distance(&lch) > 0.06) {
            pool.push(lch);
        }
    }

    let mood = mood(&pool);
    let family = family(&pool, mood, bg);
    let first = standout(&pool, &family, bg, &[]);
    let second = standout(&pool, &family, bg, std::slice::from_ref(&first));
    let signature = [family[0].clone(), family[1].clone(), family[2].clone()];
    Analysis {
        mood,
        family,
        pop: first,
        pop2: second,
        signature,
    }
}

/// Chromatic colours (chroma > 0.05) grouped into hue clusters 35° wide.
fn hue_clusters(pool: &[Lch]) -> Vec<f32> {
    let mut clusters: Vec<f32> = Vec::new();
    for p in pool.iter().filter(|p| p.c > 0.05) {
        if clusters.iter().all(|h| hue_gap(*h, p.h) > 35.0) {
            clusters.push(p.h);
        }
    }
    clusters
}

fn mood(pool: &[Lch]) -> Mood {
    let clusters = hue_clusters(pool).len();
    let chroma = pool.iter().map(|p| p.c).sum::<f32>() / pool.len() as f32;
    if clusters <= 1 || chroma < 0.035 {
        Mood::Monochrome
    } else if chroma >= 0.085 && (clusters >= 5 || (clusters >= 4 && chroma >= 0.12)) {
        Mood::Colourful
    } else {
        Mood::Muted
    }
}

/// Six colours that belong together: the accent, then the most distinct
/// colours nearest to it in hue (one family, not a rainbow). The arc starts
/// at ±60° (Catppuccin's rosewater → peach headings span about that) and
/// widens until there are enough real hues: six for a colourful theme, four
/// for a muted one. Lighter and darker steps fill whatever is left (all of it
/// for a monochrome theme).
fn family(pool: &[Lch], mood: Mood, bg: &str) -> [String; 6] {
    let accent = pool[0];
    let want = match mood {
        Mood::Colourful => 6,
        Mood::Muted => 4,
        Mood::Monochrome => 1,
    };
    let mut near: Vec<&Lch> = pool[1..].iter().filter(|p| p.c > 0.04).collect();
    near.sort_by(|a, b| hue_gap(a.h, accent.h).total_cmp(&hue_gap(b.h, accent.h)));
    let mut chosen = vec![accent];
    let mut arc = 60.0;
    while chosen.len() < want && arc <= 180.0 {
        for p in near.iter().filter(|p| hue_gap(p.h, accent.h) <= arc) {
            if chosen.len() == want {
                break;
            }
            if chosen.iter().all(|c| c.distance(p) > 0.08) {
                chosen.push(**p);
            }
        }
        arc += 30.0;
    }
    // Lighter and darker steps of the colours so far, kept readable.
    let bases = chosen.clone();
    let mut step = 0;
    while chosen.len() < 6 && step < 40 {
        let base = bases[step % bases.len()];
        let ring = step / bases.len();
        let sign = if ring % 2 == 0 { 1.0 } else { -1.0 };
        let l = (base.l + sign * 0.08 * (1 + ring / 2) as f32).clamp(0.15, 0.97);
        let shade = Lch::from_hex(&readable(&Lch { l, ..base }.to_hex(), bg));
        if chosen.iter().all(|c| c.distance(&shade) > 0.03) {
            chosen.push(shade);
        }
        step += 1;
    }
    // Pathological themes (one grey on a near-identical background).
    while chosen.len() < 6 {
        let last = *chosen.last().unwrap();
        chosen.push(Lch {
            l: (last.l + 0.05).min(1.0),
            ..last
        });
    }
    std::array::from_fn(|i| chosen[i].to_hex())
}

/// The readable colour furthest in hue from the family (weighted by chroma),
/// for emphasis that should stand out. Monochrome: the strongest text colour.
fn standout(pool: &[Lch], family: &[String; 6], bg: &str, not: &[String]) -> String {
    let fam: Vec<Lch> = family.iter().map(|h| Lch::from_hex(h)).collect();
    pool.iter()
        .filter(|p| p.c > 0.05 && !not.contains(&p.to_hex()))
        .map(|p| {
            let apart = fam
                .iter()
                .filter(|f| f.c > 0.04)
                .map(|f| hue_gap(f.h, p.h))
                .fold(180.0, f32::min);
            (apart * (0.4 + p.c * 4.0), p)
        })
        .filter(|(score, _)| *score > 20.0)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, p)| p.to_hex())
        .unwrap_or_else(|| {
            // Nothing stands apart: a lighter (dark theme) or darker shade.
            let a = fam[0];
            let l = if Lch::from_hex(bg).l < 0.5 {
                0.92
            } else {
                0.35
            };
            readable(&Lch { l, ..a }.to_hex(), bg)
        })
}

/// `hex`, or the nearest lighter/darker version of it that reaches
/// [`READABLE`] contrast on `bg`.
pub fn readable(hex: &str, bg: &str) -> String {
    if contrast(hex, bg) >= READABLE {
        return normalise(hex);
    }
    let mut lch = Lch::from_hex(hex);
    let up = Lch::from_hex(bg).l < 0.5;
    for _ in 0..20 {
        lch.l = (lch.l + if up { 0.03 } else { -0.03 }).clamp(0.0, 1.0);
        let out = lch.to_hex();
        if contrast(&out, bg) >= READABLE {
            return out;
        }
    }
    lch.to_hex()
}

/// `a` moved `t` (0–1) of the way towards `b`, in sRGB.
pub fn mix(a: &str, b: &str, t: f32) -> String {
    let (x, y) = (channels(a), channels(b));
    let [r, g, b] = std::array::from_fn(|i| ((x[i] + (y[i] - x[i]) * t) * 255.0).round() as u8);
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn normalise(hex: &str) -> String {
    format!("#{}", hex.trim_start_matches('#').to_ascii_lowercase())
}

fn hue_gap(a: f32, b: f32) -> f32 {
    let d = (a - b).abs() % 360.0;
    d.min(360.0 - d)
}

// ---------------------------------------------------------------- maths

fn channels(hex: &str) -> [f32; 3] {
    let hex = hex.trim_start_matches('#');
    std::array::from_fn(|i| {
        u8::from_str_radix(hex.get(i * 2..i * 2 + 2).unwrap_or("00"), 16).unwrap_or(0) as f32
            / 255.0
    })
}

fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn from_linear(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// WCAG relative luminance contrast, 1 to 21.
pub fn contrast(a: &str, b: &str) -> f32 {
    let lum = |hex: &str| {
        let [r, g, b] = channels(hex).map(to_linear);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    };
    let (x, y) = (lum(a), lum(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// OKLCH: lightness 0–1, chroma (0 = grey, ~0.3 = vivid), hue in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Lch {
    pub(crate) l: f32,
    pub(crate) c: f32,
    pub(crate) h: f32,
}

// The OKLab matrices as published (Björn Ottosson, 2020), digits kept as-is.
#[allow(clippy::excessive_precision)]
impl Lch {
    pub(crate) fn from_hex(hex: &str) -> Self {
        Self::from_lab(oklab(channels(hex).map(|c| c * 255.0)))
    }

    pub(crate) fn from_lab([l, a, b]: [f32; 3]) -> Self {
        Lch {
            l,
            c: (a * a + b * b).sqrt(),
            h: b.atan2(a).to_degrees().rem_euclid(360.0),
        }
    }

    pub(crate) fn lab(&self) -> [f32; 3] {
        let h = self.h.to_radians();
        [self.l, self.c * h.cos(), self.c * h.sin()]
    }

    pub(crate) fn to_hex(self) -> String {
        let [r, g, b] = from_oklab(self.lab()).map(|c| c.round() as u8);
        format!("#{r:02x}{g:02x}{b:02x}")
    }

    /// Perceptual distance (OKLab ΔE): ~0.02 barely visible, 0.1 clearly different.
    pub(crate) fn distance(&self, o: &Lch) -> f32 {
        lab_distance(self.lab(), o.lab())
    }
}

/// OKLab distance (ΔE).
pub(crate) fn lab_distance(x: [f32; 3], y: [f32; 3]) -> f32 {
    ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt()
}

/// sRGB (0–255 per channel) to OKLab.
// The OKLab matrices as published (Björn Ottosson, 2020), digits kept as-is.
#[allow(clippy::excessive_precision)]
pub(crate) fn oklab(rgb: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = rgb.map(|c| to_linear(c / 255.0));
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

/// OKLab to sRGB (0–255 per channel, clamped into the screen's range).
#[allow(clippy::excessive_precision)]
pub(crate) fn from_oklab([ok_l, a, b]: [f32; 3]) -> [f32; 3] {
    let l = (ok_l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m = (ok_l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s = (ok_l - 0.0894841775 * a - 1.2914855480 * b).powi(3);
    [
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
    ]
    .map(|c| from_linear(c.clamp(0.0, 1.0)) * 255.0)
}

/// The main colours of a picture: k-means in OKLab (so groups are colours that
/// *look* alike), started k-means++ style from a fixed seed, so the same
/// image always gives the same answer. Returns (colour, share of pixels),
/// biggest first.
pub(crate) fn kmeans(points: &[[f32; 3]], k: usize) -> Vec<([f32; 3], f32)> {
    if points.is_empty() || k == 0 {
        return Vec::new();
    }
    let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
    let mut random = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 11) as f32 / (1u64 << 53) as f32
    };
    // k-means++: each new centre is picked with odds growing with its distance
    // from the centres so far, so small but distinct colours get a centre.
    let mut centres = vec![points[points.len() / 2]];
    let mut nearest: Vec<f32> = points
        .iter()
        .map(|p| lab_distance(*p, centres[0]).powi(2))
        .collect();
    while centres.len() < k.min(points.len()) {
        let total: f32 = nearest.iter().sum();
        if total <= f32::EPSILON {
            break;
        }
        let mut pick = random() * total;
        let index = nearest
            .iter()
            .position(|d| {
                pick -= d;
                pick <= 0.0
            })
            .unwrap_or(points.len() - 1);
        centres.push(points[index]);
        for (d, p) in nearest.iter_mut().zip(points) {
            *d = d.min(lab_distance(*p, points[index]).powi(2));
        }
    }
    let mut owner = vec![0usize; points.len()];
    for _ in 0..20 {
        for (o, p) in owner.iter_mut().zip(points) {
            *o = (0..centres.len())
                .min_by(|a, b| {
                    lab_distance(*p, centres[*a]).total_cmp(&lab_distance(*p, centres[*b]))
                })
                .unwrap_or(0);
        }
        let mut sums = vec![([0f32; 3], 0usize); centres.len()];
        for (o, p) in owner.iter().zip(points) {
            (0..3).for_each(|i| sums[*o].0[i] += p[i]);
            sums[*o].1 += 1;
        }
        let mut moved = 0f32;
        for (c, (sum, n)) in centres.iter_mut().zip(&sums) {
            if *n > 0 {
                let next = sum.map(|v| v / *n as f32);
                moved = moved.max(lab_distance(*c, next));
                *c = next;
            }
        }
        if moved < 1e-4 {
            break;
        }
    }
    let mut counts = vec![0usize; centres.len()];
    owner.iter().for_each(|o| counts[*o] += 1);
    let mut out: Vec<([f32; 3], f32)> = centres
        .into_iter()
        .zip(counts)
        .filter(|(_, n)| *n > 0)
        .map(|(c, n)| (c, n as f32 / points.len() as f32))
        .collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;
    use crate::theme;

    fn load(name: &str) -> Theme {
        theme::load(&Paths::fake_home("/nonexistent"), name).unwrap()
    }

    #[test]
    fn round_trips_and_contrast() {
        for hex in ["#7aa2f7", "#1a1b26", "#ffffff", "#509475"] {
            assert_eq!(Lch::from_hex(hex).to_hex(), hex);
        }
        assert!((contrast("#000000", "#ffffff") - 21.0).abs() < 0.01);
    }

    #[test]
    fn moods_match_how_themes_look() {
        let mood = |n| analyse(&load(n)).mood;
        assert_eq!(mood("catppuccin-mocha"), Mood::Colourful);
        assert_eq!(mood("tokyo-night"), Mood::Colourful);
        assert_eq!(mood("solitude"), Mood::Monochrome);
        assert_eq!(mood("vantablack"), Mood::Monochrome);
        assert_eq!(mood("lumon"), Mood::Monochrome);
        assert_eq!(mood("everforest"), Mood::Muted);
    }

    #[test]
    fn family_is_six_distinct_readable_colours() {
        for name in theme::list(&Paths::fake_home("/nonexistent"))
            .unwrap()
            .into_iter()
            .map(|(name, _)| name)
        {
            let t = load(&name);
            let a = analyse(&t);
            let bg = t.colors.get("background");
            for (i, h) in a.family.iter().enumerate() {
                assert!(contrast(h, bg) >= READABLE - 0.05, "{name} family {i} {h}");
                for g in &a.family[..i] {
                    assert_ne!(g, h, "{name}: family repeats {h}");
                }
            }
            assert!(contrast(&a.pop, bg) >= READABLE - 0.05, "{name} pop");
        }
    }
}

#[cfg(test)]
#[test]
#[ignore = "prints a table for tuning: cargo test colour::dump -- --ignored --nocapture"]
fn dump() {
    use crate::paths::Paths;
    let paths = Paths::fake_home("/nonexistent");
    for (name, _) in crate::theme::list(&paths).unwrap() {
        let t = crate::theme::load(&paths, &name).unwrap();
        let bg = t.colors.get("background");
        let mut pool = vec![Lch::from_hex(t.colors.get("accent"))];
        for n in [
            "red",
            "orange",
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
        ] {
            let l = Lch::from_hex(&readable(t.colors.get(n), bg));
            if pool.iter().all(|p| p.distance(&l) > 0.06) {
                pool.push(l);
            }
        }
        let chroma = pool.iter().map(|p| p.c).sum::<f32>() / pool.len() as f32;
        let a = analyse(&t);
        println!(
            "{name:17} clusters {} chroma {chroma:.3} {:10} fam {} pop {} {}",
            hue_clusters(&pool).len(),
            a.mood.as_str(),
            a.family.join(" "),
            a.pop,
            a.pop2
        );
    }
}
