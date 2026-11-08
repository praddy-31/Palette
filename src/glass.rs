//! Text on see-through windows (ROADMAP 11g, colour lab idea 12). A window at opacity `o`
//! shows `o` of the theme's background and `1 - o` of the wallpaper behind it, so text that
//! reads on the solid theme can wash out on glass (purple-moon over a bright wallpaper).
//! For each app whose glass Palette sets (`palette glass`, `palette zed`), faint text
//! colours are lifted, same hue, away from the theme's background until they read on that
//! mix. Only in that app's file: every other app keeps the solid theme's colours.
//!
//! What's behind is the wallpaper's **average** colour (the owner's own check: Ghostty at 50 %
//! over his wallpaper read fine, so the lab's brightest-tenth model was too pessimistic).
//! Blur spreads the wallpaper but keeps its average, so blurred and clear glass are the
//! same sum. The slider per theme (F2, Settings → My themes, [`Lift`]) sets it by hand: off,
//! or up to 2× the minimums against up to the brightest tenth (the darkest, light themes).
//!
//! Cost: the wallpaper is read once (from its 96-px thumbnail, ~12 ms) and its colours cached
//! in `cache/wallpaper-colours.toml`, keyed by path, size and date; a lift is a few hundred
//! contrast sums (< 0.1 ms).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::colour::{self, Lch};
use crate::normalise::{Change, Level, SLACK};
use crate::paths::Paths;
use crate::settings::{Glass, Settings, TermGlass};
use crate::theme::{Colors, Theme};

/// The text colours that sit on a window's background.
pub const TEXT: [&str; 13] = [
    "foreground",
    "light_foreground",
    "bright_foreground",
    "dark_foreground",
    "muted",
    "accent",
    "red",
    "orange",
    "yellow",
    "green",
    "cyan",
    "blue",
    "magenta",
];

/// A wallpaper's colours, as glass sees them.
#[derive(Debug, Clone, PartialEq)]
pub struct Behind {
    pub average: String,
    /// The brightest tenth of its pixels (the hard case for a dark theme).
    pub bright: String,
    /// The darkest tenth (the hard case for a light theme).
    pub dark: String,
}

/// The colours of an image's pixels (any size; a thumbnail is plenty).
pub fn measure(image: &crate::pixels::Image) -> Behind {
    let hex = |p: [u8; 3]| format!("#{:02x}{:02x}{:02x}", p[0], p[1], p[2]);
    let mut px = image.rgb.clone();
    if px.is_empty() {
        px.push([0, 0, 0]);
    }
    let lum = |p: &[u8; 3]| 0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32;
    px.sort_by(|a, b| lum(a).total_cmp(&lum(b)));
    let n = px.len();
    let sum = px.iter().fold([0u64; 3], |s, p| {
        [s[0] + p[0] as u64, s[1] + p[1] as u64, s[2] + p[2] as u64]
    });
    let avg = sum.map(|v| (v / n as u64) as u8);
    Behind {
        average: hex(avg),
        bright: hex(px[(n * 9 / 10).min(n - 1)]),
        dark: hex(px[n / 10]),
    }
}

/// A wallpaper's colours, read once and cached (keyed by path, size and date, so a changed
/// file is read again). `None` if it can't be read.
pub fn behind(paths: &Paths, wallpaper: &Path) -> Option<Behind> {
    let meta = fs::metadata(wallpaper).ok()?;
    let stamp = format!(
        "{}-{}",
        meta.len(),
        meta.modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs())
    );
    let key = wallpaper.display().to_string();
    let file = paths.root().join("cache/wallpaper-colours.toml");
    let mut cache: toml::Table = fs::read_to_string(&file)
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or_default();
    if let Some(e) = cache.get(&key).and_then(|v| v.as_table())
        && e.get("stamp").and_then(|v| v.as_str()) == Some(stamp.as_str())
    {
        let get = |k: &str| e.get(k).and_then(|v| v.as_str()).map(String::from);
        if let (Some(average), Some(bright), Some(dark)) =
            (get("average"), get("bright"), get("dark"))
        {
            return Some(Behind {
                average,
                bright,
                dark,
            });
        }
    }
    let thumb = crate::thumbs::image(paths, wallpaper).ok()?;
    let b = measure(&crate::pixels::read(&thumb, Some(96)).ok()?);
    let mut e = toml::Table::new();
    e.insert("stamp".into(), stamp.into());
    e.insert("average".into(), b.average.clone().into());
    e.insert("bright".into(), b.bright.clone().into());
    e.insert("dark".into(), b.dark.clone().into());
    cache.insert(key, e.into());
    // Best effort: a cache that can't be written only costs a re-read next time.
    if fs::create_dir_all(paths.root().join("cache")).is_ok() {
        let tmp = file.with_extension("toml.tmp");
        if fs::write(&tmp, cache.to_string()).is_ok() {
            let _ = fs::rename(&tmp, &file);
        }
    }
    Some(b)
}

/// How opaque an app's window is (0-1) when Palette sets its glass, else `None` (solid, or
/// the app's own setting, which Palette doesn't know). `target` is the target's name.
pub fn opacity(settings: &Settings, target: &str) -> Option<f32> {
    if target == "zed" {
        return (settings.zed_glass_now() != Glass::Off)
            .then(|| 1.0 - f32::from(settings.zed_strength.min(100)) / 100.0);
    }
    if !crate::settings::GLASS_APPS.contains(&target) {
        return None;
    }
    let g = settings.glass(target);
    match g.kind {
        TermGlass::Own | TermGlass::Solid => None,
        _ => Some(1.0 - f32::from(g.strength.min(100)) / 100.0),
    }
}

/// How hard the glass lifts one theme's text (F2, the slider in Settings → My themes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lift {
    /// Reach the level's minimums over the wallpaper's average (11g; the default).
    Auto,
    /// By hand: 0 = no lift; 1-100 = a contrast target up to 2× the minimums, judged
    /// against a part of the wallpaper up to its brightest tenth (darkest, light themes).
    By(u8),
}

impl Lift {
    /// This theme's lift (`settings.glass_lift`).
    pub fn of(settings: &Settings, theme: &str) -> Lift {
        settings
            .glass_lift
            .get(theme)
            .map_or(Lift::Auto, |v| Lift::By((*v).min(100)))
    }

    /// `auto` or 0-100 (`palette check --glass-lift`).
    pub fn parse(s: &str) -> anyhow::Result<Lift> {
        if s.eq_ignore_ascii_case("auto") {
            return Ok(Lift::Auto);
        }
        match s.trim_end_matches('%').parse::<u8>() {
            Ok(v) if v <= 100 => Ok(Lift::By(v)),
            _ => anyhow::bail!("the glass lift is `auto` or a number 0-100 (not `{s}`)"),
        }
    }

    /// "Auto", "off" or "60 %".
    pub fn label(self) -> String {
        match self {
            Lift::Auto => "Auto".into(),
            Lift::By(0) => "off".into(),
            Lift::By(v) => format!("{v} %"),
        }
    }

    /// What the text is judged against and how far past the minimums it must go, for a
    /// theme with a `dark` background; `None` when the lift is off.
    pub fn target(self, b: &Behind, dark: bool) -> Option<(String, f32)> {
        match self {
            Lift::Auto => Some((b.average.clone(), 1.0)),
            Lift::By(0) => None,
            Lift::By(v) => {
                let t = f32::from(v.min(100)) / 100.0;
                let hard = if dark { &b.bright } else { &b.dark };
                Some((colour::mix(&b.average, hard, t), 1.0 + t))
            }
        }
    }
}

/// `palette check <theme> --glass-lift auto|0-100`, the slider in Settings → My themes: how
/// hard this theme's text is lifted on glass, with every wallpaper it shows (each is still
/// measured). `None` only reads it. Renders again if it's the active theme.
pub fn set_lift(paths: &Paths, name: &str, lift: Option<Lift>) -> anyhow::Result<Lift> {
    let changed = {
        let _lock = crate::lock::exclusive(paths)?;
        crate::theme::load(paths, name)?; // a theme that exists
        let mut s = crate::settings::load(paths)?;
        let changed = match lift {
            Some(Lift::Auto) => s.glass_lift.remove(name).is_some(),
            Some(Lift::By(v)) => s.glass_lift.insert(name.to_string(), v.min(100)) != Some(v),
            None => false,
        };
        if changed {
            crate::settings::save(paths, &s)?;
        }
        changed
    };
    if changed && crate::state::current_theme(paths)?.as_deref() == Some(name) {
        crate::rerender(paths, None)?;
    }
    Ok(Lift::of(&crate::settings::load(paths)?, name))
}

/// Lift the theme's faint text colours to read on its background at `opacity` over
/// `behind`, same hue, away from the theme's background (lighter on a dark theme). The
/// minimums are the level's (body, secondary, comments, coloured) times `factor` (1 on
/// Auto, up to 2 by hand); Off lifts nothing. A colour that can't reach the raised target
/// goes as far as it can, as long as it reaches the plain minimum.
///
/// A colour that can't reach its minimum at any lightness is left as it is and named in
/// the second list: lifting it "as far as it goes" turned every colour of a dark theme
/// white over a white wallpaper (red = green = blue), which reads no better and loses the
/// theme. Only a higher opacity helps there, and `palette check` says so.
pub fn lift(
    c: &mut Colors,
    behind: &str,
    opacity: f32,
    level: Level,
    factor: f32,
    keep: &BTreeSet<String>,
) -> (Vec<Change>, Vec<&'static str>) {
    let mut faint = Vec::new();
    let Some(m) = level.minimums() else {
        return (Vec::new(), faint);
    };
    let eff = colour::mix(behind, c.get("background"), opacity.clamp(0.0, 1.0));
    let away = Lch::from_hex(c.get("background")).l < 0.5;
    let mut out = Vec::new();
    for key in TEXT {
        if keep.contains(key) {
            continue;
        }
        let base = match key {
            "foreground" | "light_foreground" | "bright_foreground" => m.0,
            "dark_foreground" => m.1,
            "muted" => m.2,
            _ => m.3,
        };
        let need = base * factor.max(1.0);
        let now = c.get(key).to_string();
        if colour::contrast(&now, &eff) >= need * SLACK {
            continue;
        }
        let lch = Lch::from_hex(&now);
        let mut best = now.clone();
        for i in 1..=200 {
            let d = i as f32 * 0.005;
            let l = if away { lch.l + d } else { lch.l - d };
            if !(0.0..=1.0).contains(&l) {
                break;
            }
            let hex = Lch { l, ..lch }.to_hex_fit();
            if colour::contrast(&hex, &eff) > colour::contrast(&best, &eff) {
                best = hex;
            }
            if colour::contrast(&best, &eff) >= need {
                break;
            }
        }
        let r = colour::contrast(&best, &eff);
        if r < base * SLACK {
            faint.push(key);
            continue;
        }
        if best == now {
            continue; // already as far as it goes
        }
        c.set(key, &best);
        let why = if factor > 1.0 {
            format!("lifted to read on the glass ({r:.1}:1, aiming for {need:.1})")
        } else {
            format!("lifted to read on the glass ({r:.1}:1, needs {need})")
        };
        out.push(Change {
            key,
            from: now,
            to: best,
            why,
        });
    }
    (out, faint)
}

/// One app's lifted colours, and what changed.
#[derive(Debug, Clone)]
pub struct AppLift {
    pub colors: Colors,
    pub changes: Vec<Change>,
    /// Colours no lightness makes readable on this glass (raise the opacity).
    pub faint: Vec<&'static str>,
}

impl AppLift {
    /// The report's lines: one per change, then the colours left faint.
    pub fn lines(&self) -> Vec<String> {
        let mut out: Vec<String> = self.changes.iter().map(Change::summary).collect();
        if !self.faint.is_empty() {
            out.push(format!(
                "{}: can't read on this glass at any lightness; left as they are (raise the opacity)",
                self.faint.join(", ")
            ));
        }
        out
    }
}

/// The lift for every see-through app, for `theme` (already fixed) over `wallpaper`. Empty
/// when nothing is see-through, the wallpaper is unknown, the level is Off or the theme is
/// left original.
pub fn lifts(
    paths: &Paths,
    theme: &Theme,
    wallpaper: Option<&Path>,
) -> BTreeMap<&'static str, AppLift> {
    let settings = crate::settings::load(paths).unwrap_or_default();
    let how = Lift::of(&settings, &theme.name);
    lifts_at(paths, &settings, theme, wallpaper, how).0
}

type Lifts = BTreeMap<&'static str, AppLift>;
/// Every see-through app with its opacity.
type SeeThrough = Vec<(&'static str, f32)>;

/// [`lifts`] at a given `lift`, every see-through app, and what the text was judged against.
fn lifts_at(
    paths: &Paths,
    settings: &Settings,
    theme: &Theme,
    wallpaper: Option<&Path>,
    how: Lift,
) -> (Lifts, SeeThrough, Option<String>) {
    let mut out = BTreeMap::new();
    if settings.readability == Level::Off || settings.original_colours.contains(&theme.name) {
        return (out, Vec::new(), None);
    }
    let apps: Vec<(&'static str, f32)> = crate::settings::GLASS_APPS
        .iter()
        .chain(&["zed"])
        .filter_map(|t| Some((*t, opacity(settings, t)?)))
        .collect();
    let Some(wallpaper) = wallpaper.filter(|_| !apps.is_empty()) else {
        return (out, apps, None);
    };
    let Some(b) = behind(paths, wallpaper) else {
        return (out, apps, None);
    };
    let dark = Lch::from_hex(theme.colors.get("background")).l < 0.5;
    let Some((under, factor)) = how.target(&b, dark) else {
        return (out, apps, Some(b.average));
    };
    let keep: BTreeSet<String> = crate::edits::read(&theme.dir).into_keys().collect();
    for &(app, o) in &apps {
        let mut colors = theme.colors.clone();
        let (changes, faint) = lift(&mut colors, &under, o, settings.readability, factor, &keep);
        if !changes.is_empty() || !faint.is_empty() {
            out.insert(
                app,
                AppLift {
                    colors,
                    changes,
                    faint,
                },
            );
        }
    }
    (out, apps, Some(under))
}

/// The slider's live preview (F2): the most see-through app's glass for this theme over the
/// wallpaper it shows, and the theme's text colours on it at `lift`, without saving.
/// `None` when no app is see-through or the wallpaper is unknown.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Preview {
    /// The app whose glass is shown (the most see-through one).
    pub app: &'static str,
    /// What the text sits on: the window's mix of theme background and wallpaper.
    pub glass: String,
    /// Every text colour as it would be drawn there, in [`TEXT`] order.
    pub colours: Vec<(&'static str, String)>,
    /// How many colours the lift changes.
    pub changed: usize,
    /// Colours no lightness makes readable there.
    pub faint: Vec<&'static str>,
    /// The worst text colour's contrast on the glass, after the lift.
    pub worst: f32,
}

pub fn preview(paths: &Paths, name: &str, how: Option<Lift>) -> anyhow::Result<Option<Preview>> {
    let theme = crate::theme::load(paths, name)?;
    let settings = crate::settings::load(paths).unwrap_or_default();
    let how = how.unwrap_or_else(|| Lift::of(&settings, name));
    let shown = crate::effects::wallpaper_to_use(paths, &theme, None);
    let (lifts, apps, under) = lifts_at(paths, &settings, &theme, shown.as_deref(), how);
    let (Some(&(app, o)), Some(under)) = (apps.iter().min_by(|a, b| a.1.total_cmp(&b.1)), under)
    else {
        return Ok(None);
    };
    let (colors, changed, faint) = match lifts.get(app) {
        Some(l) => (l.colors.clone(), l.changes.len(), l.faint.clone()),
        None => (theme.colors.clone(), 0, Vec::new()),
    };
    let glass = colour::mix(&under, theme.colors.get("background"), o);
    let colours: Vec<(&'static str, String)> = TEXT
        .iter()
        .map(|k| (*k, colors.get(k).to_string()))
        .collect();
    let worst = colours
        .iter()
        .map(|(_, c)| colour::contrast(c, &glass))
        .fold(f32::INFINITY, f32::min);
    Ok(Some(Preview {
        app,
        glass,
        colours,
        changed,
        faint,
        worst,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn purple_moon() -> Colors {
        let e = crate::catalog::entries()
            .iter()
            .find(|e| e.name == "purple-moon")
            .expect("in the catalogue");
        let mut c = e.colors.clone();
        crate::normalise::apply_with(
            &mut c,
            Level::Minimal,
            &BTreeSet::new(),
            crate::ranking::Roles::Named,
        );
        c
    }

    #[test]
    fn text_on_glass_over_a_bright_wallpaper_is_lifted_until_it_reads() {
        let mut c = purple_moon();
        let before = c.clone();
        let (ch, _) = lift(
            &mut c,
            "#d8d0c0",
            0.5,
            Level::Minimal,
            1.0,
            &BTreeSet::new(),
        );
        assert!(!ch.is_empty());
        let eff = colour::mix("#d8d0c0", c.get("background"), 0.5);
        for ch in &ch {
            assert!(
                colour::contrast(c.get(ch.key), &eff) > colour::contrast(before.get(ch.key), &eff),
                "{}",
                ch.summary()
            );
            // Same hue (within rounding), only lightness moves.
            let (a, b) = (Lch::from_hex(&ch.from), Lch::from_hex(&ch.to));
            assert!(a.c < 0.03 || (a.h - b.h).abs() < 12.0, "{}", ch.summary());
        }
    }

    #[test]
    fn solid_windows_and_level_off_lift_nothing() {
        let mut c = purple_moon();
        assert!(
            lift(
                &mut c,
                "#ffffff",
                1.0,
                Level::Minimal,
                1.0,
                &BTreeSet::new()
            )
            .0
            .is_empty()
        );
        assert!(
            lift(&mut c, "#ffffff", 0.3, Level::Off, 1.0, &BTreeSet::new())
                .0
                .is_empty()
        );
    }

    #[test]
    fn your_edits_are_never_lifted() {
        let mut c = purple_moon();
        let keep: BTreeSet<String> = TEXT.iter().map(|k| k.to_string()).collect();
        assert!(
            lift(&mut c, "#ffffff", 0.3, Level::Strong, 1.0, &keep)
                .0
                .is_empty()
        );
    }

    #[test]
    fn a_wallpaper_is_measured_by_average_and_its_brightest_tenth() {
        let mut img = crate::pixels::Image::new(10, 1);
        for (i, p) in img.rgb.iter_mut().enumerate() {
            *p = if i == 9 { [255; 3] } else { [0; 3] };
        }
        let b = measure(&img);
        assert_eq!(b.average, "#191919");
        assert_eq!(b.bright, "#ffffff");
        assert_eq!(b.dark, "#000000");
    }

    #[test]
    fn every_catalogue_theme_reads_on_half_glass_over_its_average_wallpaper() {
        // A mid-grey and a near-white wallpaper at 50 %: every lifted colour reads, or is
        // lifted as far as it goes (then the change says so).
        for e in crate::catalog::entries() {
            let mut c = e.colors.clone();
            crate::normalise::apply_with(
                &mut c,
                Level::Minimal,
                &BTreeSet::new(),
                crate::ranking::Roles::Named,
            );
            for behind in ["#808080", "#f0f0f0", "#101010"] {
                let mut lifted = c.clone();
                let eff = colour::mix(behind, c.get("background"), 0.5);
                let (changes, _) = lift(
                    &mut lifted,
                    behind,
                    0.5,
                    Level::Minimal,
                    1.0,
                    &BTreeSet::new(),
                );
                for ch in changes {
                    // Every change reaches its minimum (else it's left alone and reported).
                    let r = colour::contrast(&ch.to, &eff);
                    assert!(r >= 1.6 * SLACK, "{} on {behind}: {}", e.name, ch.summary());
                    assert!(r > colour::contrast(&ch.from, &eff), "{}", ch.summary());
                }
            }
        }
    }

    #[test]
    fn the_slider_sets_the_target_and_the_part_of_the_wallpaper() {
        let b = Behind {
            average: "#404040".into(),
            bright: "#ffffff".into(),
            dark: "#000000".into(),
        };
        assert_eq!(Lift::Auto.target(&b, true), Some(("#404040".into(), 1.0)));
        assert_eq!(Lift::By(0).target(&b, true), None);
        assert_eq!(
            Lift::By(100).target(&b, true),
            Some(("#ffffff".into(), 2.0))
        );
        assert_eq!(
            Lift::By(100).target(&b, false),
            Some(("#000000".into(), 2.0))
        );
        let (mid, f) = Lift::By(50).target(&b, true).unwrap();
        assert!((f - 1.5).abs() < 1e-6);
        assert!(Lch::from_hex(&mid).l > Lch::from_hex("#404040").l);
        assert_eq!(Lift::parse("auto").unwrap(), Lift::Auto);
        assert_eq!(Lift::parse("60%").unwrap(), Lift::By(60));
        assert!(Lift::parse("101").is_err());
    }

    #[test]
    fn a_stronger_lift_goes_further_but_never_less_than_auto() {
        let behind = "#b0a8a0";
        let eff = |c: &Colors| colour::mix(behind, c.get("background"), 0.5);
        let mut auto = purple_moon();
        lift(
            &mut auto,
            behind,
            0.5,
            Level::Minimal,
            1.0,
            &BTreeSet::new(),
        );
        let mut strong = purple_moon();
        lift(
            &mut strong,
            behind,
            0.5,
            Level::Minimal,
            2.0,
            &BTreeSet::new(),
        );
        let worst = |c: &Colors| {
            TEXT.iter()
                .map(|k| colour::contrast(c.get(k), &eff(c)))
                .fold(f32::INFINITY, f32::min)
        };
        assert!(
            worst(&strong) > worst(&auto) + 0.3,
            "{} vs {}",
            worst(&strong),
            worst(&auto)
        );
        for k in TEXT {
            assert!(
                colour::contrast(strong.get(k), &eff(&strong)) + 0.01
                    >= colour::contrast(auto.get(k), &eff(&auto)),
                "{k}"
            );
        }
    }

    #[test]
    fn glass_plus_from_before_becomes_the_sliders_middle() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        fs::create_dir_all(paths.root()).unwrap();
        fs::write(
            paths.root().join("settings.toml"),
            "strong_lift = [\"purple-moon/cc05.jpg\"]\n",
        )
        .unwrap();
        let s = crate::settings::load(&paths).unwrap();
        assert_eq!(Lift::of(&s, "purple-moon"), Lift::By(50));
        assert_eq!(Lift::of(&s, "tokyo-night"), Lift::Auto);
        crate::settings::save(&paths, &s).unwrap();
        let again = crate::settings::load(&paths).unwrap();
        assert_eq!(again.glass_lift, s.glass_lift);
    }
}
