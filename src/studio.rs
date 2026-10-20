//! Wallpaper tools (Phase 5d): recolour a picture to a theme, make a theme
//! from a picture, and generate simple wallpapers from a theme's colours.
//! Everything works in OKLab (`colour.rs`), so "close" means "looks close".
//! Images go in and out through `pixels.rs` (macOS's `sips`, no image crate).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use crate::colour::{self, Lch, from_oklab, kmeans, lab_distance, oklab};
use crate::paths::Paths;
use crate::pixels::{self, Image};
use crate::system;
use crate::theme::{self, Theme};

// ---------------------------------------------------------------- recolour

/// Which program does the recolouring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// Palette's own (always there).
    Palette,
    /// `gowall convert`, if installed (`brew install gowall`). Optional: only
    /// its offline `convert` is ever run, with previews off.
    Gowall,
}

impl Engine {
    pub fn parse(name: &str) -> Result<Self> {
        match name {
            "palette" => Ok(Engine::Palette),
            "gowall" => Ok(Engine::Gowall),
            _ => bail!("unknown engine `{name}` (palette or gowall)"),
        }
    }
}

/// Is gowall installed?
pub fn has_gowall() -> bool {
    system::find_program("gowall").is_some()
}

/// The colours a picture is pulled towards: the theme's surfaces, text and
/// its terminal colours, without near-duplicates.
fn targets(theme: &Theme) -> Vec<[f32; 3]> {
    let keys = [
        "darker_background",
        "dark_background",
        "background",
        "lighter_background",
        "selection",
        "muted",
        "dark_foreground",
        "light_foreground",
        "foreground",
        "accent",
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
    let mut out: Vec<[f32; 3]> = Vec::new();
    for key in keys {
        let lab = Lch::from_hex(theme.colors.get(key)).lab();
        if out.iter().all(|o| lab_distance(*o, lab) > 0.02) {
            out.push(lab);
        }
    }
    out
}

/// Grid points per axis of the colour lookup table: 33³ ≈ 36,000 colours
/// worked out once, every pixel then blends its 8 neighbours. Same idea as
/// gowall's Hald CLUT and a photo editor's LUT.
const GRID: usize = 33;

/// How strongly a picture is pulled onto the theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Strength {
    /// The picture's own colours mostly survive, nudged towards the theme.
    Subtle,
    /// Clearly in the theme's colours, with the picture's detail kept.
    #[default]
    Balanced,
    /// Close to a flat, palette-mapped look (like gowall).
    Strong,
}

/// The knobs behind a [`Strength`].
#[derive(Debug, Clone, Copy)]
struct Tuning {
    /// How far apart theme colours can be and still blend (OKLab ΔE).
    /// Smaller is closer to a posterised look, larger is washier.
    spread: f32,
    /// How much of the picture's own light and shade survives (0: only the
    /// theme's lightness, 1: the original's). Keeps faces, clouds and texture.
    keep_light: f32,
    /// How much lightness counts when matching a theme colour (hue and
    /// chroma count 1).
    light_weight: f32,
}

impl Strength {
    pub const NAMES: [&str; 3] = ["subtle", "balanced", "strong"];

    pub fn parse(name: &str) -> Result<Self> {
        match name {
            "subtle" => Ok(Strength::Subtle),
            "balanced" => Ok(Strength::Balanced),
            "strong" => Ok(Strength::Strong),
            _ => bail!("unknown strength `{name}` (subtle, balanced or strong)"),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Strength::Subtle => "subtle",
            Strength::Balanced => "balanced",
            Strength::Strong => "strong",
        }
    }

    fn tuning(self) -> Tuning {
        match self {
            // Phase 5d's original recolour.
            Strength::Subtle => Tuning {
                spread: 0.07,
                keep_light: 0.5,
                light_weight: 0.5,
            },
            // Tuned by eye on photos, paintings and illustrations against
            // nord, everforest and osaka-jade.
            Strength::Balanced => Tuning {
                spread: 0.045,
                keep_light: 0.3,
                light_weight: 0.75,
            },
            Strength::Strong => Tuning {
                spread: 0.025,
                keep_light: 0.08,
                light_weight: 1.0,
            },
        }
    }
}

/// Recolour `image` towards `theme`'s colours.
pub fn recolour_image(image: &Image, theme: &Theme, strength: Strength) -> Image {
    let lut = lookup_table(&targets(theme), strength.tuning());
    let step = 255.0 / (GRID - 1) as f32;
    let at = |r: usize, g: usize, b: usize| lut[(r * GRID + g) * GRID + b];
    let rgb = image
        .rgb
        .iter()
        .map(|px| {
            let f = px.map(|c| c as f32 / step);
            let i = f.map(|v| (v as usize).min(GRID - 2));
            let t = [0, 1, 2].map(|k| f[k] - i[k] as f32);
            let mut out = [0f32; 3];
            for corner in 0..8 {
                let (dr, dg, db) = (corner >> 2 & 1, corner >> 1 & 1, corner & 1);
                let w = [dr, dg, db]
                    .iter()
                    .zip(t)
                    .map(|(d, t)| if *d == 1 { t } else { 1.0 - t })
                    .product::<f32>();
                let c = at(i[0] + dr, i[1] + dg, i[2] + db);
                (0..3).for_each(|k| out[k] += c[k] * w);
            }
            out.map(|v| v.round().clamp(0.0, 255.0) as u8)
        })
        .collect();
    Image { rgb, ..*image }
}

fn lookup_table(targets: &[[f32; 3]], tuning: Tuning) -> Vec<[f32; 3]> {
    let step = 255.0 / (GRID - 1) as f32;
    let mut lut = Vec::with_capacity(GRID * GRID * GRID);
    for r in 0..GRID {
        for g in 0..GRID {
            for b in 0..GRID {
                let lab = oklab([r, g, b].map(|v| v as f32 * step));
                lut.push(from_oklab(pull(lab, targets, tuning)));
            }
        }
    }
    lut
}

/// Distance for matching, with lightness weighted: a theme's colours are
/// mostly text colours (light on dark themes), so at low weights a dark blue
/// sky still finds the theme's blue rather than a grey of the same lightness.
fn match_distance(x: [f32; 3], y: [f32; 3], light_weight: f32) -> f32 {
    (((x[0] - y[0]) * light_weight).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt()
}

/// One colour pulled onto the theme: a soft blend of the nearest theme
/// colours (Gaussian weights on distance, relative to the nearest so a far
/// colour still lands somewhere), keeping some of its own lightness.
fn pull(lab: [f32; 3], targets: &[[f32; 3]], tuning: Tuning) -> [f32; 3] {
    let d: Vec<f32> = targets
        .iter()
        .map(|t| match_distance(lab, *t, tuning.light_weight))
        .collect();
    let nearest = d.iter().copied().fold(f32::MAX, f32::min);
    let mut sum = [0f32; 3];
    let mut total = 0f32;
    for (t, d) in targets.iter().zip(&d) {
        let w = (-(d * d - nearest * nearest) / (tuning.spread * tuning.spread)).exp();
        (0..3).for_each(|k| sum[k] += t[k] * w);
        total += w;
    }
    let mut out = sum.map(|v| v / total);
    out[0] = out[0] * (1.0 - tuning.keep_light) + lab[0] * tuning.keep_light;
    out
}

/// The file name a recolour is saved as: `<stem>-<theme>.<ext>` for
/// Palette's default strength, with `-subtle`/`-strong` or `-gowall` added
/// otherwise, so every variant can sit side by side for comparing.
pub fn recolour_name(
    stem: &str,
    theme: &str,
    engine: Engine,
    strength: Strength,
    ext: &str,
) -> String {
    let suffix = match (engine, strength) {
        (Engine::Gowall, _) => "-gowall",
        (Engine::Palette, Strength::Balanced) => "",
        (Engine::Palette, Strength::Subtle) => "-subtle",
        (Engine::Palette, Strength::Strong) => "-strong",
    };
    format!("{stem}-{theme}{suffix}.{ext}")
}

/// `palette bg recolor`: a recoloured copy of `source` in the theme's
/// wallpapers (named by [`recolour_name`], `.png` or `.jpg` like the
/// original). `strength` is Palette's own; gowall has none. Returns the new
/// file.
pub fn recolour(
    theme: &Theme,
    source: &Path,
    engine: Engine,
    strength: Strength,
) -> Result<PathBuf> {
    if !fs::metadata(source).is_ok_and(|m| m.is_file()) || !theme::is_image(source) {
        bail!(
            "{} is not an image (png, jpg, jpeg, heic, webp, tiff)",
            source.display()
        );
    }
    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "wallpaper".into());
    let lossless = source
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("png") || e.eq_ignore_ascii_case("tiff"));
    let ext = if lossless { "png" } else { "jpg" };
    let dir = theme.backgrounds_dir();
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let dest = dir.join(recolour_name(&stem, &theme.name, engine, strength, ext));
    if dest.exists() {
        bail!("{} already exists; remove it first", dest.display());
    }
    match engine {
        Engine::Palette => {
            let image = pixels::read(source, None)?;
            pixels::write(&dest, &recolour_image(&image, theme, strength))?;
        }
        Engine::Gowall => gowall(theme, source, &dest)?,
    }
    Ok(dest)
}

/// `gowall convert <image> -t <theme.json> --output <dest> --preview false`.
/// The theme is a temporary JSON file of this theme's colours.
fn gowall(theme: &Theme, source: &Path, dest: &Path) -> Result<()> {
    if !has_gowall() {
        bail!("gowall is not installed (brew install gowall), or use Palette's own recolour");
    }
    let colours: Vec<String> = targets(theme)
        .into_iter()
        .map(|lab| {
            let [r, g, b] = from_oklab(lab).map(|c| c.round() as u8);
            format!("\"#{r:02x}{g:02x}{b:02x}\"")
        })
        .collect();
    let json = std::env::temp_dir().join(format!("palette-{}.json", std::process::id()));
    fs::write(
        &json,
        format!(
            "{{\"name\": \"palette-{}\", \"colors\": [{}]}}",
            theme.name,
            colours.join(", ")
        ),
    )?;
    let (src, out, json_arg) = (
        source.to_string_lossy(),
        dest.to_string_lossy(),
        json.to_string_lossy(),
    );
    let ran = system::run(
        "gowall",
        &[
            "convert",
            &src,
            "-t",
            &json_arg,
            "--output",
            &out,
            "--preview",
            "false",
        ],
    );
    let _ = fs::remove_file(&json);
    ran.map(|_| ()).map_err(|e| anyhow!(e))?;
    if !dest.is_file() {
        bail!("gowall finished but wrote no {}", dest.display());
    }
    Ok(())
}

// ---------------------------------------------------------------- new theme

/// A theme worked out from a picture, before it is saved.
#[derive(Debug, Clone)]
pub struct Drafted {
    pub colors_toml: String,
    pub light: bool,
    pub mood: colour::Mood,
    /// The picture's main colours, biggest share first (for showing).
    pub swatches: Vec<String>,
}

/// Where each terminal colour sits on the OKLCH hue wheel (degrees).
const HUES: [(&str, f32); 7] = [
    ("red", 25.0),
    ("orange", 60.0),
    ("yellow", 95.0),
    ("green", 145.0),
    ("cyan", 200.0),
    ("blue", 255.0),
    ("magenta", 330.0),
];

/// Work out a theme from a picture. `light`: force a light or dark theme
/// (default: light if the picture is mostly light).
///
/// The picture's colours give the mood (hue of the background, accent,
/// how saturated everything is); the terminal colours keep their meaning
/// (red still looks red), leaning towards the picture's own hues where it
/// has something close. Every text colour is checked for contrast.
pub fn draft_from_image(image: &Image, light: Option<bool>) -> Result<Drafted> {
    let points: Vec<[f32; 3]> = image
        .rgb
        .iter()
        .map(|p| oklab(p.map(|c| c as f32)))
        .collect();
    let clusters = kmeans(&points, 10);
    if clusters.is_empty() {
        bail!("the picture has no pixels");
    }
    let lch: Vec<(Lch, f32)> = clusters
        .iter()
        .map(|(c, w)| (Lch::from_lab(*c), *w))
        .collect();
    let mean_l: f32 = lch.iter().map(|(c, w)| c.l * w).sum();
    let light = light.unwrap_or(mean_l > 0.68);

    // Background: the darkest (lightest) colour that covers a real part of
    // the picture, made calm enough to read on.
    let big: Vec<&(Lch, f32)> = lch.iter().filter(|(_, w)| *w >= 0.04).collect();
    let base = big
        .iter()
        .min_by(|a, b| {
            let (x, y) = if light {
                (b.0.l, a.0.l)
            } else {
                (a.0.l, b.0.l)
            };
            x.total_cmp(&y)
        })
        .map(|(c, _)| *c)
        .unwrap_or(lch[0].0);
    let bg = Lch {
        l: if light {
            base.l.clamp(0.95, 0.97)
        } else {
            base.l.clamp(0.17, 0.24)
        },
        c: base.c.min(if light { 0.015 } else { 0.03 }),
        h: base.h,
    };
    let fg = Lch {
        l: if light { 0.32 } else { 0.91 },
        c: bg.c.min(0.02),
        h: bg.h,
    };
    let bg_hex = bg.to_hex();

    // How colourful the picture is sets how colourful the theme is: its
    // overall chroma (greys count, so a black wall stays quiet) and its most
    // vivid real colour (so a neon sign lifts the theme).
    let chromatic: Vec<&(Lch, f32)> = lch.iter().filter(|(c, _)| c.c > 0.03).collect();
    let overall: f32 = lch.iter().map(|(c, w)| c.c * w).sum();
    let vivid = lch
        .iter()
        .filter(|(_, w)| *w >= 0.02)
        .map(|(c, _)| c.c)
        .fold(0.0, f32::max);
    let level = (0.045 + 0.6 * overall + 0.4 * vivid).clamp(0.06, 0.17);
    // Text lightness follows the picture's own (a bright picture gets
    // brighter text on a dark theme), within the range that reads.
    let text_l = if light {
        0.48 + 0.08 * mean_l
    } else {
        0.70 + 0.12 * mean_l.min(0.7) / 0.7
    };

    // Accent: the picture's most striking colour. Vividness counts far more
    // than area (a small pink sun beats a big blue sky), but specks don't count.
    let striking = |(c, w): &&(Lch, f32)| c.c * c.c * w.powf(0.25);
    let accent = chromatic
        .iter()
        .filter(|(_, w)| *w >= 0.01)
        .max_by(|a, b| striking(a).total_cmp(&striking(b)))
        .map(|(c, _)| Lch {
            l: text_l,
            c: c.c.max(level),
            h: c.h,
        })
        .unwrap_or(Lch {
            l: text_l,
            c: 0.02,
            h: bg.h,
        });

    let mut table = toml::Table::new();
    let mut put = |key: &str, hex: String| {
        table.insert(key.into(), toml::Value::String(hex));
    };
    put("background", bg_hex.clone());
    put("foreground", fg.to_hex());
    put("accent", colour::readable(&accent.to_hex(), &bg_hex));
    let source = if accent.c > 0.03 { accent.h } else { bg.h };
    let terminal = terminal_colours(&lch, source, level, text_l, light, &bg_hex);
    for (i, (name, _)) in HUES.iter().enumerate() {
        let (normal, bright) = &terminal[i];
        put(name, normal.clone());
        if *name != "orange" {
            put(&format!("bright_{name}"), bright.clone());
        }
    }
    put(
        "brown",
        Lch {
            l: 0.45,
            c: (level * 0.6).max(0.04),
            h: harmonise(60.0, source, cast_strength(&lch)),
        }
        .to_hex(),
    );
    let (text, _) = crate::install::complete(&table, light)?;
    let text = text.replacen(
        "# Imported by Palette. Keys marked \"filled\" were missing and derived from the others.",
        "# Made by Palette from a wallpaper. \"filled\" keys are mixed from the others; edit freely.",
        1,
    );
    let colors = theme::Colors::parse(&text)
        .map_err(|problems| anyhow!("made an invalid theme: {}", problems.join("; ")))?;
    let mood = colour::analyse_colors(&colors).mood;
    let swatches = clusters
        .iter()
        .take(8)
        .map(|(c, _)| Lch::from_lab(*c).to_hex())
        .collect();
    Ok(Drafted {
        colors_toml: text,
        light,
        mood,
        swatches,
    })
}

fn hue_gap(a: f32, b: f32) -> f32 {
    signed_gap(a, b).abs()
}

/// Natural lightness of each terminal colour relative to the others (yellow
/// reads lighter than blue at the same "level"), same order as [`HUES`].
const LIGHT_OFFSET: [f32; 7] = [-0.02, 0.02, 0.07, 0.03, 0.04, -0.03, -0.01];

/// Smallest OKLab distance kept between any two terminal colours, so seven
/// hues stay seven (orange vs yellow, cyan vs blue) even in a muted theme.
const MIN_GAP: f32 = 0.055;

/// Furthest each terminal colour may move from its usual hue (same order as
/// [`HUES`]): red stays red, its neighbours have room to lean.
const MAX_SHIFT: [f32; 7] = [16.0, 22.0, 25.0, 28.0, 28.0, 25.0, 25.0];

/// The picture's overall colour cast: the hue of its mean OKLab a/b and how
/// strong it is (0: grey or balanced, 1: clearly tinted).
fn cast(lch: &[(Lch, f32)]) -> (f32, f32) {
    let (mut a, mut b) = (0.0, 0.0);
    for (c, w) in lch {
        let lab = c.lab();
        a += lab[1] * w;
        b += lab[2] * w;
    }
    let mean = Lch::from_lab([0.5, a, b]);
    (mean.h, (mean.c / 0.03).min(1.0))
}

fn cast_strength(lch: &[(Lch, f32)]) -> f32 {
    cast(lch).1
}

/// `hue` turned partway towards `source` (Material You's "harmonise"): half
/// the gap, at most 10° for a grey picture up to 25° for a strongly tinted one.
fn harmonise(hue: f32, source: f32, strength: f32) -> f32 {
    let gap = signed_gap(hue, source);
    let limit = 10.0 + 15.0 * strength;
    (hue + gap.signum() * (gap.abs() * 0.5).min(limit)).rem_euclid(360.0)
}

/// The seven terminal colours (normal, bright) for a picture: each usual hue
/// harmonised towards the picture's `source` hue and leaning to the
/// picture's own nearby colours, chroma and lightness spread from the
/// picture, warm pictures leaning warm. Kept readable and apart.
fn terminal_colours(
    lch: &[(Lch, f32)],
    source: f32,
    level: f32,
    text_l: f32,
    light: bool,
    bg: &str,
) -> Vec<(String, String)> {
    let (cast_h, strength) = cast(lch);
    // +1 for a warm (orange) cast, -1 for a cold (blue) one.
    let warmth = (cast_h - 55.0).to_radians().cos() * strength;
    let mean_l: f32 = lch.iter().map(|(c, w)| c.l * w).sum();
    let spread_l = lch
        .iter()
        .map(|(c, w)| (c.l - mean_l).powi(2) * w)
        .sum::<f32>()
        .sqrt();
    let contrast_scale = (0.6 + spread_l * 3.0).min(1.6);
    let chromatic: Vec<&(Lch, f32)> = lch
        .iter()
        .filter(|(c, w)| c.c > 0.03 && *w >= 0.01)
        .collect();
    let mut cols: Vec<Lch> = HUES
        .iter()
        .zip(LIGHT_OFFSET.iter().zip(MAX_SHIFT))
        .map(|((_, canon), (offset, max))| {
            let mut h = harmonise(*canon, source, strength);
            let mut c = level * (1.0 + 0.2 * warmth * (canon - 55.0).to_radians().cos());
            // Lean towards the picture's own colour if it has one nearby,
            // taking some of its strength too.
            let near = chromatic
                .iter()
                .filter(|(p, _)| hue_gap(p.h, h) < 30.0)
                .min_by(|a, b| hue_gap(a.0.h, h).total_cmp(&hue_gap(b.0.h, h)));
            if let Some((p, _)) = near {
                h += 0.5 * signed_gap(h, p.h);
                c = (c * 0.5 + p.c * 0.6).max(c).min(0.2);
            }
            let h = canon + signed_gap(*canon, h).clamp(-max, max);
            Lch {
                l: text_l + offset * contrast_scale,
                c,
                h: h.rem_euclid(360.0),
            }
        })
        .collect();

    // Readable on the background, then nudged apart until every pair differs.
    let shown = |cols: &[Lch]| -> Vec<Lch> {
        cols.iter()
            .map(|c| Lch::from_hex(&colour::readable(&c.to_hex(), bg)))
            .collect()
    };
    for _ in 0..40 {
        let now = shown(&cols);
        let mut worst: Option<(usize, usize, f32)> = None;
        for i in 0..now.len() {
            for j in i + 1..now.len() {
                let d = now[i].distance(&now[j]);
                if d < MIN_GAP && worst.is_none_or(|w| d < w.2) {
                    worst = Some((i, j, d));
                }
            }
        }
        let Some((i, j, _)) = worst else { break };
        let away = signed_gap(cols[j].h, cols[i].h).signum();
        for (k, sign) in [(i, away), (j, -away)] {
            let canon = HUES[k].1;
            let h = cols[k].h + 3.0 * sign;
            cols[k].h =
                (canon + signed_gap(canon, h).clamp(-MAX_SHIFT[k], MAX_SHIFT[k])).rem_euclid(360.0);
            cols[k].c = (cols[k].c * 1.06).min(0.22);
        }
    }
    cols.iter()
        .map(|n| {
            let bright = Lch {
                l: if light { n.l - 0.06 } else { n.l + 0.07 },
                c: n.c * 1.05,
                h: n.h,
            };
            (
                colour::readable(&n.to_hex(), bg),
                colour::readable(&bright.to_hex(), bg),
            )
        })
        .collect()
}

/// From `a` to `b` the short way round, in degrees (-180 to 180).
fn signed_gap(a: f32, b: f32) -> f32 {
    (b - a + 540.0).rem_euclid(360.0) - 180.0
}

/// `palette new <name> --from-wallpaper <image>`: draft a theme from the
/// picture and save it as a user theme, with the picture as its wallpaper.
/// `dry_run`: only work it out.
pub fn new_from_wallpaper(
    paths: &Paths,
    name: &str,
    image: &Path,
    light: Option<bool>,
    dry_run: bool,
) -> Result<Drafted> {
    if !theme::is_valid_name(name) {
        bail!("invalid theme name `{name}`: use lowercase letters, digits and `-`");
    }
    if theme::load(paths, name).is_ok() || paths.themes().join(name).exists() {
        bail!("a theme called `{name}` already exists; pick another name");
    }
    if !fs::metadata(image).is_ok_and(|m| m.is_file()) || !theme::is_image(image) {
        bail!(
            "{} is not an image (png, jpg, jpeg, heic, webp, tiff)",
            image.display()
        );
    }
    let small = pixels::read(image, Some(200))?;
    let drafted = draft_from_image(&small, light)?;
    if dry_run {
        return Ok(drafted);
    }
    let dir = paths.themes().join(name);
    let partial = paths.themes().join(format!(".{name}.partial"));
    let _ = fs::remove_dir_all(&partial);
    fs::create_dir_all(partial.join("backgrounds"))?;
    fs::write(partial.join("colors.toml"), &drafted.colors_toml)?;
    if drafted.light {
        fs::write(partial.join("light.mode"), "")?;
    }
    let file = image.file_name().context("image has no file name")?;
    fs::copy(image, partial.join("backgrounds").join(file))
        .with_context(|| format!("copying {}", image.display()))?;
    fs::rename(&partial, &dir).with_context(|| format!("saving {}", dir.display()))?;
    Ok(drafted)
}

// ---------------------------------------------------------------- generate

/// Generated wallpaper styles, calmest first.
pub const STYLES: [&str; 6] = ["gradient", "mesh", "waves", "topo", "geometric", "grain"];

/// 16:10, larger than most Mac screens; macOS scales it to fill.
const WIDTH: usize = 3200;
const HEIGHT: usize = 2000;

/// The theme's colours a painting uses, in OKLab.
struct Inks {
    deep: [f32; 3],
    bg: [f32; 3],
    raised: [f32; 3],
    accent: [f32; 3],
    /// The accent and its neighbours (`colour::analyse`), six of them.
    family: Vec<[f32; 3]>,
    light: bool,
}

impl Inks {
    fn new(theme: &Theme) -> Self {
        let lab = |key: &str| Lch::from_hex(theme.colors.get(key)).lab();
        let family = colour::analyse(theme)
            .family
            .iter()
            .map(|h| Lch::from_hex(h).lab())
            .collect();
        let bg = lab("background");
        Inks {
            deep: lab("darker_background"),
            bg,
            raised: lab("lighter_background"),
            accent: lab("accent"),
            family,
            light: bg[0] > 0.5,
        }
    }

    /// `c` softened into the background: 0 is the background, 1 is `c`.
    /// Light themes get paler tints so they stay light.
    fn tone(&self, c: [f32; 3], t: f32) -> [f32; 3] {
        mix(self.bg, c, if self.light { t * 0.75 } else { t })
    }

    fn fam(&self, i: usize) -> [f32; 3] {
        self.family[i % self.family.len()]
    }
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Small seeded random numbers (SplitMix64): the same seed always paints
/// the same picture.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }

    fn noise(&mut self) -> Noise {
        Noise((self.next() * u32::MAX as f32) as u32)
    }
}

/// A number from 0 to 1 for a grid point, the same every time.
fn hash3(x: i32, y: i32, seed: u32) -> f32 {
    let mut n = (x as u32).wrapping_mul(0x27d4_eb2d)
        ^ (y as u32).wrapping_mul(0x1656_67b1)
        ^ seed.wrapping_mul(0x9e37_79b9);
    n = (n ^ (n >> 15)).wrapping_mul(0x85eb_ca6b);
    n = (n ^ (n >> 13)).wrapping_mul(0xc2b2_ae35);
    (n ^ (n >> 16)) as f32 / u32::MAX as f32
}

/// Value noise: random heights on a grid, smoothly blended in between.
#[derive(Clone, Copy)]
struct Noise(u32);

impl Noise {
    fn value(self, x: f32, y: f32) -> f32 {
        let (fx, fy) = (x.floor(), y.floor());
        let (ix, iy) = (fx as i32, fy as i32);
        let ease = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
        let (sx, sy) = (ease(x - fx), ease(y - fy));
        let at = |dx: i32, dy: i32| hash3(ix + dx, iy + dy, self.0);
        let top = at(0, 0) + (at(1, 0) - at(0, 0)) * sx;
        let bottom = at(0, 1) + (at(1, 1) - at(0, 1)) * sx;
        top + (bottom - top) * sy
    }

    /// Fractal noise (fBm): `octaves` layers, each twice as fine and half as
    /// strong, 0 to 1 (mostly 0.25–0.75).
    fn fbm(self, x: f32, y: f32, octaves: u32) -> f32 {
        let (mut sum, mut amp, mut freq, mut norm) = (0.0, 0.5, 1.0, 0.0);
        for o in 0..octaves {
            let layer = Noise(self.0.wrapping_add(o.wrapping_mul(1013)));
            sum += amp * layer.value(x * freq + o as f32 * 17.3, y * freq - o as f32 * 9.1);
            norm += amp;
            amp *= 0.5;
            freq *= 2.03;
        }
        sum / norm
    }
}

/// `f(x, y)` for every pixel, rows shared out over the CPU's cores.
fn par_map<T: Send + Copy + Default>(
    width: usize,
    height: usize,
    f: impl Fn(usize, usize) -> T + Sync,
) -> Vec<T> {
    let mut out = vec![T::default(); width * height];
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(16);
    let rows = height.div_ceil(threads).max(1);
    let f = &f;
    std::thread::scope(|s| {
        for (i, chunk) in out.chunks_mut(rows * width).enumerate() {
            s.spawn(move || {
                for (k, px) in chunk.iter_mut().enumerate() {
                    let at = i * rows * width + k;
                    *px = f(at % width, at / width);
                }
            });
        }
    });
    out
}

/// The calm base most styles sit on: a multi-stop gradient across the whole
/// picture (raised, tinted towards the accent, through the background, to a
/// deep shade) with two soft glows of the family's colours.
/// A soft glow: centre, radius, colour, strength.
type Glow = ((f32, f32), f32, [f32; 3], f32);

struct Backdrop {
    stops: [[f32; 3]; 5],
    dir: (f32, f32),
    centre: (f32, f32),
    phase: f32,
    glows: [Glow; 2],
}

impl Backdrop {
    fn new(inks: &Inks, rng: &mut Rng, aspect: f32) -> Self {
        let deepest = {
            let mut d = inks.deep;
            d[0] = if inks.light { d[0] - 0.04 } else { d[0] * 0.8 };
            d
        };
        let angle = rng.range(0.35, 0.85) + if rng.next() < 0.5 { 0.0 } else { 1.9 };
        let top = (rng.range(0.6, 0.95) * aspect, rng.range(0.0, 0.3));
        let bottom = (rng.range(0.05, 0.4) * aspect, rng.range(0.75, 1.05));
        Backdrop {
            stops: [
                mix(
                    inks.raised,
                    inks.accent,
                    if inks.light { 0.1 } else { 0.18 },
                ),
                inks.raised,
                inks.bg,
                inks.deep,
                deepest,
            ],
            dir: (angle.cos(), angle.sin()),
            centre: (aspect / 2.0, 0.5),
            phase: rng.range(0.0, std::f32::consts::TAU),
            glows: [
                (top, 0.5, inks.tone(inks.accent, 0.8), 0.38),
                (bottom, 0.65, inks.tone(inks.fam(1), 0.7), 0.3),
            ],
        }
    }

    fn at(&self, u: f32, v: f32) -> [f32; 3] {
        let (du, dv) = (u - self.centre.0, v - self.centre.1);
        let t = (du * self.dir.0 + dv * self.dir.1) / 1.4
            + 0.5
            + 0.04 * (u * 2.3 + v * 1.7 + self.phase).sin();
        let t = (1.0 - t).clamp(0.0, 1.0) * 4.0;
        let i = (t as usize).min(3);
        let mut c = mix(
            self.stops[i],
            self.stops[i + 1],
            smoothstep(0.0, 1.0, t - i as f32),
        );
        for ((x, y), r, colour, strength) in self.glows {
            let d2 = (u - x).powi(2) + (v - y).powi(2);
            c = mix(c, colour, strength * (-d2 / (r * r)).exp());
        }
        c
    }
}

/// Paint a wallpaper from the theme's colours (`width`×`height`). `seed`
/// picks a variant (the same seed always paints the same picture).
pub fn paint(
    theme: &Theme,
    style: &str,
    width: usize,
    height: usize,
    seed: Option<u64>,
) -> Result<Image> {
    if !STYLES.contains(&style) {
        bail!("unknown style `{style}` ({})", STYLES.join(", "));
    }
    let inks = Inks::new(theme);
    let mut rng = Rng(seed.unwrap_or(0) ^ 0x7061_6c65_7474_6521);
    let unit = height as f32;
    let aspect = width as f32 / unit;
    // Where pixel (x, y) sits, in picture heights.
    let at = |x: usize, y: usize| ((x as f32 + 0.5) / unit, (y as f32 + 0.5) / unit);
    let backdrop = Backdrop::new(&inks, &mut rng, aspect);
    let mut grain = 1.6;
    let lab: Vec<[f32; 3]> = match style {
        "gradient" => par_map(width, height, |x, y| {
            let (u, v) = at(x, y);
            backdrop.at(u, v)
        }),
        "mesh" => mesh(&inks, &mut rng, aspect, width, height, &at),
        "waves" => {
            let phases: Vec<f32> = (0..8)
                .map(|_| rng.range(0.0, std::f32::consts::TAU))
                .collect();
            par_map(width, height, |x, y| {
                let (u, v) = at(x, y);
                let u = u / aspect;
                // Layered hills from the family colours, toned down towards
                // the background, darker further down.
                let mut c = mix(inks.raised, inks.bg, v);
                for i in 0..4 {
                    let k = i as f32;
                    let crest = 0.42
                        + 0.13 * k
                        + 0.05 * (u * (5.0 + k * 1.7) + phases[i]).sin()
                        + 0.025 * (u * (11.0 + k * 2.3) + phases[i + 4]).sin();
                    let edge = ((v - crest) * unit / 1.5).clamp(0.0, 1.0);
                    let layer = mix(inks.tone(inks.fam(i), 0.42 - 0.06 * k), inks.deep, 0.22 * k);
                    c = mix(c, layer, edge);
                }
                c
            })
        }
        "topo" => topo(&inks, &mut rng, &backdrop, width, height, &at),
        "geometric" => geometric(&inks, &mut rng, aspect, width, height, &at),
        "grain" => {
            grain = 7.0;
            halftone(&inks, &mut rng, &backdrop, aspect, width, height, &at)
        }
        _ => unreachable!(),
    };
    let rgb = par_map(width, height, |x, y| {
        // A little noise, so smooth gradients don't show bands (and film
        // grain for the grain style).
        let noise = (hash3(x as i32, y as i32, 7) - 0.5) * grain;
        from_oklab(lab[y * width + x]).map(|c| (c + noise).round().clamp(0.0, 255.0) as u8)
    });
    Ok(Image { width, height, rgb })
}

/// Aurora / mesh gradient: big soft blobs of the family's colours, blended
/// in every direction and gently warped, like macOS Sonoma's wallpapers.
fn mesh(
    inks: &Inks,
    rng: &mut Rng,
    aspect: f32,
    width: usize,
    height: usize,
    at: &(impl Fn(usize, usize) -> (f32, f32) + Sync),
) -> Vec<[f32; 3]> {
    let blobs: Vec<((f32, f32), f32, [f32; 3])> = (0..6)
        .map(|i| {
            let centre = (rng.range(-0.1, 1.1) * aspect, rng.range(-0.15, 1.15));
            let r = if i == 0 {
                rng.range(0.55, 0.75)
            } else {
                rng.range(0.32, 0.6)
            };
            let strength = if i == 0 { 0.8 } else { rng.range(0.5, 0.75) };
            let mut c = inks.tone(inks.fam(i), strength);
            if !inks.light {
                // Keep it a dark wallpaper: lift, don't glare.
                c[0] = c[0].min(inks.bg[0] + 0.28);
            }
            (centre, r, c)
        })
        .collect();
    let (wx, wy) = (rng.noise(), rng.noise());
    par_map(width, height, |x, y| {
        let (u, v) = at(x, y);
        let (u, v) = (
            u + 0.3 * (wx.fbm(u * 1.2, v * 1.2, 3) - 0.5),
            v + 0.3 * (wy.fbm(u * 1.2, v * 1.2, 3) - 0.5),
        );
        let base = mix(inks.raised, inks.deep, v);
        let mut sum = base.map(|c| c * 0.2);
        let mut total = 0.2;
        for ((bx, by), r, c) in &blobs {
            let w = (-((u - bx).powi(2) + (v - by).powi(2)) / (r * r) * 1.6).exp();
            (0..3).for_each(|k| sum[k] += c[k] * w);
            total += w;
        }
        sum.map(|s| s / total)
    })
}

/// Topographic map: contour lines of a fractal height field in the accent,
/// every fifth line stronger, over the backdrop. Lines are drawn by distance
/// to the nearest height step (height ÷ slope), so they stay one or two
/// pixels wide and smooth at any size.
fn topo(
    inks: &Inks,
    rng: &mut Rng,
    backdrop: &Backdrop,
    width: usize,
    height: usize,
    at: &(impl Fn(usize, usize) -> (f32, f32) + Sync),
) -> Vec<[f32; 3]> {
    let (field, warp) = (rng.noise(), rng.noise());
    let heights = par_map(width, height, |x, y| {
        let (u, v) = at(x, y);
        let w = warp.fbm(u * 1.1, v * 1.1, 2) - 0.5;
        field.fbm(u * 1.5 + w, v * 1.5 - w, 4)
    });
    let levels = 22.0;
    let scale = (height as f32 / 2000.0).max(0.5);
    let minor = inks.tone(inks.accent, if inks.light { 0.55 } else { 0.5 });
    let major = inks.tone(inks.accent, 0.9);
    let high = inks.tone(inks.fam(1), 0.35);
    par_map(width, height, |x, y| {
        let (u, v) = at(x, y);
        let f = heights[y * width + x];
        let h = |xx: usize, yy: usize| heights[yy * width + xx];
        let (x0, x1) = (x.saturating_sub(1), (x + 1).min(width - 1));
        let (y0, y1) = (y.saturating_sub(1), (y + 1).min(height - 1));
        let gx = (h(x1, y) - h(x0, y)) / (x1 - x0).max(1) as f32;
        let gy = (h(x, y1) - h(x, y0)) / (y1 - y0).max(1) as f32;
        let slope = (gx * gx + gy * gy).sqrt() * levels;
        let g = f * levels;
        let step = g.round();
        let distance = (g - step).abs() / slope.max(1e-5);
        let is_major = (step as i32).rem_euclid(5) == 0;
        let half = if is_major { 1.3 } else { 0.7 } * scale;
        let alpha = (half + 0.5 - distance).clamp(0.0, 1.0) * if is_major { 0.85 } else { 0.65 };
        // Higher ground tinted a little, like a relief map.
        let base = mix(backdrop.at(u, v), high, smoothstep(0.45, 0.75, f) * 0.5);
        mix(base, if is_major { major } else { minor }, alpha)
    })
}

/// Flat shapes in the family's colours, Omarchy-minimal: a diagonal band,
/// two overlapping circles, a ring and a dot, edges smoothed.
fn geometric(
    inks: &Inks,
    rng: &mut Rng,
    aspect: f32,
    width: usize,
    height: usize,
    at: &(impl Fn(usize, usize) -> (f32, f32) + Sync),
) -> Vec<[f32; 3]> {
    enum Shape {
        Disc((f32, f32), f32),
        Ring((f32, f32), f32, f32),
        Band((f32, f32), (f32, f32), f32),
    }
    let flip = rng.next() < 0.5;
    let side = |x: f32| if flip { aspect - x } else { x };
    let big = (side(rng.range(0.62, 0.82) * aspect), rng.range(0.2, 0.45));
    let big_r = rng.range(0.34, 0.46);
    let ring_dx = big_r * if flip { 0.3 } else { -0.3 };
    let angle: f32 = rng.range(-1.15, -0.75);
    let angle = if flip { -angle } else { angle };
    let shapes = [
        (
            Shape::Band(
                (side(rng.range(0.3, 0.5) * aspect), 0.5),
                (angle.cos(), angle.sin()),
                rng.range(0.1, 0.17),
            ),
            inks.tone(inks.fam(2), 0.3),
        ),
        (Shape::Disc(big, big_r), inks.tone(inks.accent, 0.6)),
        (
            Shape::Disc(
                (side(rng.range(0.15, 0.35) * aspect), rng.range(0.62, 0.9)),
                rng.range(0.2, 0.3),
            ),
            inks.tone(inks.fam(1), 0.5),
        ),
        (
            Shape::Ring((big.0 + ring_dx, big.1 + 0.55 * big_r), big_r * 0.55, 0.012),
            inks.tone(inks.fam(3), 0.85),
        ),
        (
            Shape::Disc(
                (side(rng.range(0.4, 0.55) * aspect), rng.range(0.12, 0.3)),
                0.04,
            ),
            inks.tone(inks.accent, 1.0),
        ),
    ];
    let unit = height as f32;
    par_map(width, height, |x, y| {
        let (u, v) = at(x, y);
        let mut c = mix(inks.raised, inks.deep, v * 0.9 + 0.05);
        for (shape, colour) in &shapes {
            let sd = match shape {
                Shape::Disc((cx, cy), r) => ((u - cx).powi(2) + (v - cy).powi(2)).sqrt() - r,
                Shape::Ring((cx, cy), r, w) => {
                    (((u - cx).powi(2) + (v - cy).powi(2)).sqrt() - r).abs() - w
                }
                Shape::Band((cx, cy), (nx, ny), w) => ((u - cx) * nx + (v - cy) * ny).abs() - w,
            };
            let cover = (0.5 - sd * unit).clamp(0.0, 1.0);
            // Slightly see-through, so overlaps show like layered paper.
            c = mix(c, *colour, cover * 0.92);
        }
        c
    })
}

/// A halftone dot grid over the backdrop: dots grow towards one corner and
/// swell and fade with a slow noise; the style also gets film grain.
fn halftone(
    inks: &Inks,
    rng: &mut Rng,
    backdrop: &Backdrop,
    aspect: f32,
    width: usize,
    height: usize,
    at: &(impl Fn(usize, usize) -> (f32, f32) + Sync),
) -> Vec<[f32; 3]> {
    let spacing = 0.024;
    let corner = (
        if rng.next() < 0.5 { 0.0 } else { aspect },
        if rng.next() < 0.5 { 0.0 } else { 1.0 },
    );
    let swell = rng.noise();
    let dot = inks.tone(inks.accent, if inks.light { 0.45 } else { 0.4 });
    let unit = height as f32;
    par_map(width, height, |x, y| {
        let (u, v) = at(x, y);
        let (cu, cv) = (
            ((u / spacing).floor() + 0.5) * spacing,
            ((v / spacing).floor() + 0.5) * spacing,
        );
        let near = 1.0 - ((cu - corner.0).powi(2) + (cv - corner.1).powi(2)).sqrt() / 1.6;
        let m = (near * 0.8 + (swell.fbm(cu * 2.0, cv * 2.0, 2) - 0.5) * 0.9).clamp(0.0, 1.0);
        let r = spacing * 0.5 * (0.08 + 0.75 * m * m);
        let d = ((u - cu).powi(2) + (v - cv).powi(2)).sqrt() - r;
        let cover = (0.5 - d * unit).clamp(0.0, 1.0);
        mix(backdrop.at(u, v), dot, cover * (0.45 + 0.5 * m))
    })
}

/// `palette bg generate`: paint a wallpaper and save it among the theme's
/// wallpapers as `palette-<style>.jpg`, or `palette-<style>-<seed>.jpg` for a
/// chosen variant (replacing an earlier one of the same name).
pub fn generate(theme: &Theme, style: &str, seed: Option<u64>) -> Result<PathBuf> {
    let image = paint(theme, style, WIDTH, HEIGHT, seed)?;
    let dir = theme.backgrounds_dir();
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let dest = dir.join(match seed {
        Some(seed) => format!("palette-{style}-{seed}.jpg"),
        None => format!("palette-{style}.jpg"),
    });
    pixels::write(&dest, &image)?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colour::contrast;

    fn load(name: &str) -> Theme {
        theme::load(&Paths::fake_home("/nonexistent"), name).unwrap()
    }

    #[test]
    fn recolour_lands_on_theme_colours() {
        let nord = load("nord");
        let targets = targets(&nord);
        let mut image = Image::new(8, 1);
        image.rgb = vec![
            [0, 0, 0],
            [255, 255, 255],
            [255, 0, 0],
            [0, 200, 0],
            [0, 0, 255],
            [128, 128, 128],
            [255, 200, 0],
            [200, 0, 200],
        ];
        let out = recolour_image(&image, &nord, Strength::Subtle);
        for (before, after) in image.rgb.iter().zip(&out.rgb) {
            let lab = oklab(after.map(|c| c as f32));
            let nearest = targets
                .iter()
                .map(|t| lab_distance(lab, *t))
                .fold(f32::MAX, f32::min);
            assert!(nearest < 0.16, "{before:?} → {after:?} is far from nord");
        }
    }

    #[test]
    fn drafts_follow_the_picture() {
        // A dusky blue picture with a pink sun.
        let mut image = Image::new(20, 20);
        for (i, p) in image.rgb.iter_mut().enumerate() {
            *p = if i % 20 < 4 && i / 20 < 4 {
                [240, 110, 160]
            } else if i / 20 < 12 {
                [40, 60, 110]
            } else {
                [15, 20, 40]
            };
        }
        let d = draft_from_image(&image, None).unwrap();
        assert!(!d.light);
        let colors = theme::Colors::parse(&d.colors_toml).unwrap();
        let bg = colors.get("background");
        assert!(
            Lch::from_hex(bg).h > 220.0 && Lch::from_hex(bg).h < 300.0,
            "blue-ish {bg}"
        );
        let accent = Lch::from_hex(colors.get("accent"));
        assert!(
            accent.h < 20.0 || accent.h > 330.0,
            "pink accent {accent:?}"
        );
        for key in ["foreground", "accent", "red", "green", "blue", "yellow"] {
            assert!(contrast(colors.get(key), bg) >= 3.0, "{key} readable");
        }
        // Red is still red, green still green.
        assert!(hue_gap(Lch::from_hex(colors.get("red")).h, 25.0) < 25.0);
        assert!(hue_gap(Lch::from_hex(colors.get("green")).h, 145.0) <= 30.0);

        let light = draft_from_image(&image, Some(true)).unwrap();
        assert!(light.light && light.colors_toml.contains("mode = \"light\""));
    }

    #[test]
    fn paints_every_style() {
        for theme in ["tokyo-night", "catppuccin-latte"] {
            let t = load(theme);
            for style in STYLES {
                let image = paint(&t, style, 160, 100, None).unwrap();
                assert_eq!(image.rgb.len(), 160 * 100);
                // Not flat: a real spread of lightness across the picture.
                let l: Vec<f32> = image
                    .rgb
                    .iter()
                    .map(|p| oklab(p.map(|c| c as f32))[0])
                    .collect();
                let (lo, hi) = l
                    .iter()
                    .fold((1f32, 0f32), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
                assert!(hi - lo > 0.03, "{theme} {style} is flat ({lo}..{hi})");
                // Same seed, same picture; another seed, another picture.
                assert_eq!(image, paint(&t, style, 160, 100, None).unwrap());
                let a = paint(&t, style, 160, 100, Some(1)).unwrap();
                assert_eq!(a, paint(&t, style, 160, 100, Some(1)).unwrap());
                assert_ne!(a, paint(&t, style, 160, 100, Some(2)).unwrap(), "{style}");
            }
        }
        assert!(paint(&load("nord"), "spirals", 4, 4, None).is_err());
    }

    #[test]
    fn strengths_parse_and_name() {
        for name in Strength::NAMES {
            assert_eq!(Strength::parse(name).unwrap().as_str(), name);
        }
        assert!(Strength::parse("loud").is_err());
        assert_eq!(Strength::default(), Strength::Balanced);
        let name = |e, s| recolour_name("sky", "nord", e, s, "png");
        assert_eq!(name(Engine::Palette, Strength::Balanced), "sky-nord.png");
        assert_eq!(
            name(Engine::Palette, Strength::Subtle),
            "sky-nord-subtle.png"
        );
        assert_eq!(
            name(Engine::Palette, Strength::Strong),
            "sky-nord-strong.png"
        );
        assert_eq!(
            name(Engine::Gowall, Strength::Balanced),
            "sky-nord-gowall.png"
        );
    }

    #[test]
    fn stronger_lands_closer_to_the_theme() {
        let nord = load("nord");
        let targets = targets(&nord);
        // A smooth sweep of colours, like a photo's.
        let mut image = Image::new(24, 24);
        for (i, p) in image.rgb.iter_mut().enumerate() {
            let (x, y) = (i % 24, i / 24);
            *p = [(x * 11) as u8, (y * 10) as u8, ((x + y) * 5) as u8];
        }
        let off = |s| {
            let out = recolour_image(&image, &nord, s);
            out.rgb
                .iter()
                .map(|p| {
                    let lab = oklab(p.map(|c| c as f32));
                    targets
                        .iter()
                        .map(|t| lab_distance(lab, *t))
                        .fold(f32::MAX, f32::min)
                })
                .sum::<f32>()
        };
        let (subtle, balanced, strong) = (
            off(Strength::Subtle),
            off(Strength::Balanced),
            off(Strength::Strong),
        );
        assert!(
            strong < balanced && balanced < subtle,
            "{subtle} {balanced} {strong}"
        );
    }

    /// A picture of a few colours in horizontal stripes, shares in percent.
    fn stripes(parts: &[([u8; 3], usize)]) -> Image {
        let mut image = Image::new(10, 100);
        let mut row = 0;
        for (colour, share) in parts {
            for y in row..(row + share).min(100) {
                for x in 0..10 {
                    image.rgb[y * 10 + x] = *colour;
                }
            }
            row += share;
        }
        image
    }

    #[test]
    fn drafts_differ_between_pictures() {
        // A warm, muted portrait (skin, hair, a brown room)…
        let portrait = stripes(&[
            ([40, 28, 24], 45),
            ([190, 140, 115], 25),
            ([120, 80, 60], 20),
            ([225, 190, 170], 10),
        ]);
        // …and a cold, vivid neon picture on black.
        let neon = stripes(&[
            ([6, 8, 20], 60),
            ([0, 200, 255], 20),
            ([40, 60, 230], 12),
            ([0, 255, 170], 8),
        ]);
        let draft = |image: &Image| {
            let d = draft_from_image(image, Some(false)).unwrap();
            theme::Colors::parse(&d.colors_toml).unwrap()
        };
        let (a, b) = (draft(&portrait), draft(&neon));
        for key in ["blue", "yellow", "green"] {
            let d = Lch::from_hex(a.get(key)).distance(&Lch::from_hex(b.get(key)));
            assert!(d > 0.05, "{key}: {} vs {} (ΔE {d})", a.get(key), b.get(key));
        }
        for colors in [&a, &b] {
            let bg = colors.get("background");
            let red = Lch::from_hex(colors.get("red"));
            assert!(hue_gap(red.h, 25.0) <= 30.0, "red is reddish: {red:?}");
            let hues: Vec<Lch> = HUES
                .iter()
                .map(|(name, _)| {
                    assert!(contrast(colors.get(name), bg) >= 3.0, "{name} readable");
                    Lch::from_hex(colors.get(name))
                })
                .collect();
            for i in 0..hues.len() {
                for j in i + 1..hues.len() {
                    assert!(
                        hues[i].distance(&hues[j]) > 0.04,
                        "{} and {} too alike",
                        HUES[i].0,
                        HUES[j].0
                    );
                }
            }
        }
        // Muted picture, muted theme; vivid picture, vivid theme.
        let chroma = |c: &theme::Colors| -> f32 {
            HUES.iter()
                .map(|(n, _)| Lch::from_hex(c.get(n)).c)
                .sum::<f32>()
                / 7.0
        };
        assert!(
            chroma(&b) > chroma(&a) + 0.02,
            "{} {}",
            chroma(&a),
            chroma(&b)
        );
    }
}
