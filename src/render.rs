//! Turn a theme into the output files, one per target.

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;

use anyhow::{Context, Result, bail};
use minijinja::syntax::SyntaxConfig;
use minijinja::{Environment, Error, ErrorKind as TemplateErrorKind, UndefinedBehavior, Value};

use crate::paths::Paths;
use crate::targets::{self, Output};
use crate::theme::Theme;

/// One file to be written into `current/`.
#[derive(Debug, Clone)]
pub struct OutputFile {
    pub name: &'static str,
    pub contents: Vec<u8>,
    /// Copied from the theme folder instead of rendered.
    pub is_override: bool,
}

/// Render every target that has an output file. Static overrides in the
/// theme folder win. `paths` is for targets that build on the user's own
/// config (starship) or list Palette's paths (shell).
pub fn render(theme: &Theme, paths: &Paths) -> Result<Vec<OutputFile>> {
    render_with(theme, paths, None)
}

/// [`render`] for a switch that starts on `wallpaper` (`palette set --wallpaper`). See-through
/// apps (11g) get their own colours, lifted to read over the wallpaper this switch shows.
pub fn render_with(
    theme: &Theme,
    paths: &Paths,
    wallpaper: Option<&std::path::Path>,
) -> Result<Vec<OutputFile>> {
    render_selected(theme, paths, wallpaper, None)
}

/// [`render`] for only the targets named in `only` (`None`: all of them): a setting for one
/// app (Ghostty's glass) renders just that app's file (14a).
pub fn render_some(theme: &Theme, paths: &Paths, only: Option<&[&str]>) -> Result<Vec<OutputFile>> {
    render_selected(theme, paths, None, only)
}

fn render_selected(
    theme: &Theme,
    paths: &Paths,
    wallpaper: Option<&std::path::Path>,
    only: Option<&[&str]>,
) -> Result<Vec<OutputFile>> {
    let env = environment()?;
    let settings = crate::settings::load(paths).unwrap_or_default();
    let ctx = context(theme, &settings);
    let shown = crate::effects::wallpaper_to_use(paths, theme, wallpaper);
    let lifts = crate::glass::lifts(paths, theme, shown.as_deref());
    targets::all()
        .filter(|t| only.is_none_or(|names| names.contains(&t.name)))
        .filter_map(|t| Some((t.name, t.output.as_ref()?)))
        .map(|(name, output)| match lifts.get(name) {
            Some(lift) => {
                let lifted = Theme {
                    colors: lift.colors.clone(),
                    ..theme.clone()
                };
                let ctx = context(&lifted, &settings);
                render_one(&env, &ctx, theme, paths, name, output)
            }
            None => render_one(&env, &ctx, theme, paths, name, output),
        })
        .collect()
}

/// One target's template for this theme, before any `finish` step (for
/// previews that combine it with something not yet on disk).
pub fn template_text(theme: &Theme, paths: &Paths, target: &str) -> Result<String> {
    let env = environment()?;
    let ctx = context(theme, &crate::settings::load(paths).unwrap_or_default());
    env.get_template(target)?
        .render(ctx)
        .with_context(|| format!("rendering the {target} template"))
}

fn render_one(
    env: &Environment,
    ctx: &Value,
    theme: &Theme,
    paths: &Paths,
    name: &str,
    output: &Output,
) -> Result<OutputFile> {
    if let Some(contents) = read_override(theme, output.file)? {
        if !output.overridable {
            bail!(
                "theme `{}` contains {}, but themes cannot replace it: that file can run \
                 programs, and themes are data only. Remove it from {}",
                theme.name,
                output.file,
                theme.dir.display()
            );
        }
        return Ok(OutputFile {
            name: output.file,
            contents,
            is_override: true,
        });
    }
    let mut text = env
        .get_template(name)?
        .render(ctx)
        .with_context(|| format!("rendering the {name} template"))?;
    if let Some(finish) = output.finish {
        text = finish(paths, text).with_context(|| format!("building {}", output.file))?;
    }
    Ok(OutputFile {
        name: output.file,
        contents: text.into_bytes(),
        is_override: false,
    })
}

fn environment() -> Result<Environment<'static>> {
    let mut env = Environment::new();
    // A typo like `{{ accnet }}` should fail loudly, not render as empty.
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.set_syntax(
        SyntaxConfig::builder()
            .keep_trailing_newline(true)
            .build()?,
    );
    env.add_filter("bare", bare);
    env.add_filter("argb", argb);
    env.add_filter("hue", |hex: String| hsl(&hex).0);
    env.add_filter("sat", |hex: String| hsl(&hex).1);
    env.add_filter("light", |hex: String| hsl(&hex).2);
    env.add_filter("rgb", rgb);
    env.add_filter("ink", ink);
    for target in targets::ALL {
        if let Some(output) = &target.output {
            env.add_template(target.name, output.template)
                .with_context(|| format!("the {} template does not parse", target.name))?;
        }
    }
    Ok(env)
}

/// The variables templates can use (documented in `docs/THEME_SPEC.md`).
fn context(theme: &Theme, settings: &crate::settings::Settings) -> Value {
    let mut vars: BTreeMap<String, Value> = theme
        .colors
        .iter()
        .map(|(key, hex)| (key.to_string(), Value::from(hex.to_string())))
        .collect();
    vars.insert(
        "ansi".into(),
        Value::from(theme.colors.ansi().map(str::to_string).to_vec()),
    );
    let look = crate::colour::analyse(theme);
    let [primary, secondary, tertiary] = look.signature;
    vars.insert("primary".into(), Value::from(primary));
    vars.insert("secondary".into(), Value::from(secondary));
    vars.insert("tertiary".into(), Value::from(tertiary));
    vars.insert("mood".into(), Value::from(look.mood.as_str()));
    vars.insert("family".into(), Value::from(look.family.to_vec()));
    vars.insert("pop".into(), Value::from(look.pop));
    vars.insert("pop2".into(), Value::from(look.pop2));
    vars.insert("mode".into(), Value::from(theme.colors.mode.as_str()));
    vars.insert("theme".into(), Value::from(theme.name.clone()));
    // Editors draw code on the selection (11h): their own, so syntax colours stay readable.
    let level = if settings.original_colours.contains(&theme.name) {
        crate::normalise::Level::Off
    } else {
        settings.readability
    };
    vars.insert(
        "editor_selection".into(),
        Value::from(crate::normalise::editor_selection(&theme.colors, level)),
    );
    // Lists draw coloured text on their selected line (F3): lazygit, fzf.
    vars.insert(
        "list_selection".into(),
        Value::from(crate::normalise::list_selection(&theme.colors, level)),
    );
    // Preferences that change how a target looks (`settings.toml`).
    vars.insert("bar_scroll".into(), Value::from(settings.bar_scroll));
    vars.insert("obsidian_ring".into(), Value::from(settings.obsidian_ring));
    vars.insert("obsidian_dim".into(), Value::from(settings.obsidian_dim));
    // Zed glass: two-digit alpha for see-through surfaces (strength 50 → 80)
    // and for raised ones (menus, popovers), which stay more solid (→ d1).
    let see = f64::from(settings.zed_strength.min(100)) / 100.0;
    let alpha = |opacity: f64| format!("{:02x}", (opacity * 255.0).round() as u8);
    vars.insert("zed_see".into(), Value::from(alpha(1.0 - see)));
    vars.insert(
        "zed_see_raised".into(),
        Value::from(alpha(1.0 - see * 0.36)),
    );
    vars.insert("glass".into(), glass_vars(settings));
    Value::from_object(vars)
}

/// Terminal glass (`palette glass`), one entry per app: `kind` (as in
/// settings.toml), `opacity` as text (`"0.80"`, `"1.00"` when solid) and `blur`
/// (0-100, 0 unless `blurred`). Each template turns these into its own keys.
fn glass_vars(settings: &crate::settings::Settings) -> Value {
    use crate::settings::TermGlass;
    let map: BTreeMap<String, Value> = crate::settings::GLASS_APPS
        .iter()
        .map(|app| {
            let g = settings.glass(app);
            let opacity = match g.kind {
                TermGlass::Own | TermGlass::Solid => "1.00".to_string(),
                _ => format!("{:.2}", 1.0 - f64::from(g.strength.min(100)) / 100.0),
            };
            let blur = if g.kind == TermGlass::Blurred {
                g.blur
            } else {
                0
            };
            let entry: BTreeMap<String, Value> = BTreeMap::from([
                ("kind".into(), Value::from(g.kind.as_str())),
                ("opacity".into(), Value::from(opacity)),
                ("blur".into(), Value::from(blur.min(100))),
            ]);
            (app.to_string(), Value::from_object(entry))
        })
        .collect();
    Value::from_object(map)
}

/// `#7aa2f7` → `7aa2f7`
fn bare(hex: String) -> String {
    hex.trim_start_matches('#').to_string()
}

/// `#7aa2f7` → `0xff7aa2f7`, or with a custom alpha: `argb("f0")` → `0xf07aa2f7`.
fn argb(hex: String, alpha: Option<String>) -> Result<String, Error> {
    let alpha = alpha.unwrap_or_else(|| "ff".into());
    if alpha.len() != 2 || !alpha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::new(
            TemplateErrorKind::InvalidOperation,
            format!("argb alpha must be two hex digits, got {alpha:?}"),
        ));
    }
    Ok(format!("0x{}{}", alpha.to_ascii_lowercase(), bare(hex)))
}

/// Text colour for a background: whichever of `dark` and `light` contrasts
/// more with it. `{{ primary | ink(darker_background, bright_foreground) }}`
fn ink(bg: String, dark: String, light: String) -> String {
    use crate::colour::contrast;
    if contrast(&dark, &bg) >= contrast(&light, &bg) {
        dark
    } else {
        light
    }
}

/// `#7aa2f7` → `122, 162, 247`: themes that build colours as
/// `rgb(var(--x))`, like Obsidian's Catppuccin, store them this way.
fn rgb(hex: String) -> String {
    let hex = bare(hex);
    let channel = |i: usize| u8::from_str_radix(hex.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    format!("{}, {}, {}", channel(0), channel(2), channel(4))
}

/// `#rrggbb` → (hue 0–359, saturation %, lightness %), rounded. Obsidian
/// builds its accent shades from these three numbers.
fn hsl(hex: &str) -> (u32, u32, u32) {
    let hex = hex.trim_start_matches('#');
    let channel = |i: usize| {
        f64::from(u8::from_str_radix(hex.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0)) / 255.0
    };
    let (r, g, b) = (channel(0), channel(2), channel(4));
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0, 0, (l * 100.0).round() as u32);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (
        (h.round() as u32) % 360,
        (s * 100.0).round() as u32,
        (l * 100.0).round() as u32,
    )
}

/// The theme's own copy of `file`, if it has one. Only regular files count:
/// a symlink in a downloaded theme could otherwise copy any file on the Mac
/// into `current/`.
fn read_override(theme: &Theme, file: &str) -> Result<Option<Vec<u8>>> {
    let path = theme.dir.join(file);
    match fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_file() => fs::read(&path)
            .map(Some)
            .with_context(|| format!("reading {}", path.display())),
        Ok(_) => bail!(
            "{} must be a regular file (symlinks are not followed)",
            path.display()
        ),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn signature_is_accent_then_two_distinct_colours() {
        let theme = tokyo();
        let [a, b, c] = crate::colour::analyse(&theme).signature;
        assert_eq!(a, theme.colors.get("accent").to_lowercase());
        assert!(a != b && b != c && a != c, "{a} {b} {c}");
        assert_eq!(super::rgb("#1a1b26".into()), "26, 27, 38");
    }

    use super::*;
    use crate::paths::Paths;
    use crate::theme;

    fn paths() -> Paths {
        Paths::fake_home("/nonexistent")
    }

    fn tokyo() -> Theme {
        theme::load(&paths(), "tokyo-night").unwrap()
    }

    fn text<'a>(files: &'a [OutputFile], name: &str) -> &'a str {
        let f = files.iter().find(|f| f.name == name).unwrap();
        std::str::from_utf8(&f.contents).unwrap()
    }

    #[test]
    fn filters() {
        assert_eq!(bare("#7aa2f7".into()), "7aa2f7");
        assert_eq!(argb("#7aa2f7".into(), None).unwrap(), "0xff7aa2f7");
        assert_eq!(
            argb("#292e42".into(), Some("F0".into())).unwrap(),
            "0xf0292e42"
        );
        assert!(argb("#292e42".into(), Some("f".into())).is_err());
        assert_eq!(hsl("#ff0000"), (0, 100, 50));
        assert_eq!(hsl("#7aa2f7"), (221, 89, 72));
        assert_eq!(hsl("#808080"), (0, 0, 50));
    }

    #[test]
    fn renders_every_target() {
        let files = render(&tokyo(), &paths()).unwrap();
        let with_output = targets::all().filter(|t| t.output.is_some()).count();
        assert_eq!(files.len(), with_output);
        for f in files.iter().filter(|f| f.name != "starship.toml") {
            let s = std::str::from_utf8(&f.contents).unwrap();
            assert!(s.contains("tokyo-night"), "{} lacks the theme name", f.name);
            assert!(s.ends_with('\n'), "{} lacks a trailing newline", f.name);
            assert!(!f.is_override);
        }
    }

    #[test]
    #[cfg(target_os = "macos")] // the Mac's targets / CoreText fonts
    fn sketchybar_matches_owner_names() {
        let files = render(&tokyo(), &paths()).unwrap();
        let s = text(&files, "sketchybar.sh");
        assert!(s.contains("export PILL=0xf0292e42\n"));
        assert!(s.contains("export BLUE=0xff7aa2f7\n"));
        assert!(s.contains("export FG=0xffc0caf5\n"));
    }

    #[test]
    fn terminals_get_all_16_ansi_colours() {
        let files = render(&tokyo(), &paths()).unwrap();
        let ghostty = text(&files, "ghostty.conf");
        assert!(ghostty.contains("\npalette = 0=#13141c\n"));
        assert!(ghostty.contains("\npalette = 15=#c0caf5\n"));
        let kitty = text(&files, "kitty.conf");
        assert_eq!(kitty.matches("\ncolor").count(), 16);
        let alacritty = text(&files, "alacritty.toml");
        alacritty
            .parse::<toml::Table>()
            .expect("alacritty.toml is valid TOML");
    }

    /// Terminal glass: "own" writes nothing (the user's config decides); every
    /// other kind writes that app's keys, and Alacritty's file stays valid TOML.
    #[test]
    fn terminal_glass_lines() {
        use crate::settings::{Settings, TermGlass, WindowGlass};
        let env = environment().unwrap();
        let theme = theme::load(&paths(), "tokyo-night").unwrap();
        let render = |app: &str, kind: TermGlass| {
            let mut settings = Settings::default();
            settings.glass.insert(
                app.into(),
                WindowGlass {
                    kind,
                    strength: 30,
                    blur: 80,
                },
            );
            env.get_template(app)
                .unwrap()
                .render(context(&theme, &settings))
                .unwrap()
        };
        assert!(!render("ghostty", TermGlass::Own).contains("background-opacity"));
        assert!(!render("kitty", TermGlass::Own).contains("background_opacity"));
        assert!(!render("alacritty", TermGlass::Own).contains("[window]"));
        let g = render("ghostty", TermGlass::Blurred);
        assert!(
            g.contains("\nbackground-opacity = 0.70\nbackground-blur = 80\n"),
            "{g}"
        );
        let g = render("ghostty", TermGlass::Solid);
        assert!(
            g.contains("\nbackground-opacity = 1.00\nbackground-blur = false\n"),
            "{g}"
        );
        let g = render("ghostty", TermGlass::LiquidGlass);
        assert!(g.contains("background-blur = macos-glass-regular\n"), "{g}");
        let k = render("kitty", TermGlass::Blurred);
        assert!(
            k.contains("background_opacity 0.70\n") && k.contains("background_blur 64\n"),
            "{k}"
        );
        // Dispatch reads `glass` from its JSON colours file; it must stay valid JSON.
        for kind in TermGlass::kinds("dispatch") {
            let text = render("dispatch", *kind);
            let json: serde_json::Value =
                serde_json::from_str(&text).unwrap_or_else(|e| panic!("{kind:?}: {e}\n{text}"));
            assert_eq!(json["glass"]["kind"], kind.as_str());
            assert!(json["glass"]["opacity"].is_f64(), "{text}");
        }
        let d: serde_json::Value =
            serde_json::from_str(&render("dispatch", TermGlass::Blurred)).unwrap();
        assert_eq!(d["glass"]["blur"], 80);
        assert_eq!(d["glass"]["opacity"], 0.7);
        // EasyPlots (also on Linux) and the Habit Tracker read it the same way.
        for (app, kind) in ["easyplots", "habit-tracker"]
            .into_iter()
            .flat_map(|a| TermGlass::kinds(a).iter().map(move |k| (a, k)))
        {
            let text = render(app, *kind);
            let json: serde_json::Value =
                serde_json::from_str(&text).unwrap_or_else(|e| panic!("{kind:?}: {e}\n{text}"));
            assert_eq!(json["glass"]["kind"], kind.as_str());
        }
        for kind in TermGlass::kinds("alacritty") {
            let text = render("alacritty", *kind);
            let toml: toml::Table = text.parse().unwrap_or_else(|e| panic!("{kind:?}: {e}"));
            if *kind != TermGlass::Own {
                let window = toml["window"].as_table().unwrap();
                assert!(window["opacity"].is_float(), "{kind:?}");
                assert_eq!(window["blur"].as_bool(), Some(*kind == TermGlass::Blurred));
            }
        }
    }

    /// Zed drops a whole theme file over one bad colour, so every bundled
    /// theme, at every strength, must give strict JSON with `#rrggbb[aa]`
    /// only, and the three window kinds under their fixed names.
    #[test]
    fn zed_theme_is_strict_json_with_valid_colours() {
        let env = environment().unwrap();
        for (name, _) in theme::list(&paths()).unwrap() {
            let theme = theme::load(&paths(), &name).unwrap();
            for strength in [0, 50, 100] {
                let settings = crate::settings::Settings {
                    zed_strength: strength,
                    ..Default::default()
                };
                let text = env
                    .get_template("zed")
                    .unwrap()
                    .render(context(&theme, &settings))
                    .unwrap();
                let json: serde_json::Value = serde_json::from_str(&text)
                    .unwrap_or_else(|e| panic!("{name} {strength}: {e}\n{text}"));
                let themes = json["themes"].as_array().unwrap();
                let kinds: Vec<_> = themes
                    .iter()
                    .map(|t| {
                        (
                            t["name"].as_str().unwrap(),
                            t["style"]["background.appearance"].as_str().unwrap(),
                        )
                    })
                    .collect();
                assert_eq!(
                    kinds,
                    [
                        ("Palette", "opaque"),
                        ("Palette Glass", "blurred"),
                        ("Palette Clear", "transparent")
                    ]
                );
                let see = ["ff", "80", "00"][usize::from(strength / 50)];
                assert!(
                    themes[1]["style"]["background"]
                        .as_str()
                        .unwrap()
                        .ends_with(see)
                );
                for t in themes {
                    assert_eq!(t["appearance"], theme.colors.mode.as_str());
                    let mut colours = Vec::new();
                    for (key, v) in t["style"].as_object().unwrap() {
                        match (key.as_str(), v) {
                            ("background.appearance", _) => {}
                            ("players", serde_json::Value::Array(ps)) => {
                                for p in ps {
                                    colours.extend(p.as_object().unwrap().values().cloned());
                                }
                            }
                            ("syntax", serde_json::Value::Object(syn)) => {
                                for s in syn.values() {
                                    colours.push(s["color"].clone());
                                    assert!(
                                        s.get("font_style").is_some()
                                            && s.get("font_weight").is_some()
                                    );
                                }
                            }
                            _ => colours.push(v.clone()),
                        }
                    }
                    for c in colours {
                        let c = c.as_str().unwrap_or_else(|| panic!("{name}: {c}"));
                        let hex = c.strip_prefix('#').unwrap_or("");
                        assert!(
                            (hex.len() == 6 || hex.len() == 8)
                                && hex.chars().all(|ch| ch.is_ascii_hexdigit()),
                            "{name} {strength}: bad colour {c}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn new_targets_render_usable_files() {
        let files = render(&tokyo(), &paths()).unwrap();
        let json = text(&files, "vscodium.json");
        assert!(
            json.contains("\"editor.background\": \"#1a1b26\""),
            "{json}"
        );
        let lua = text(&files, "neovim.lua");
        assert!(lua.contains("local ansi = { \"#13141c\", "), "{lua}");
        let starship: toml::Table = text(&files, "starship.toml").parse().unwrap();
        assert_eq!(starship["palette"].as_str(), Some("palette"));
        let shell = text(&files, "shell.sh");
        assert!(
            shell.contains(
                "export FZF_DEFAULT_OPTS_FILE=\"$HOME/.config/palette/current/fzf.opts\""
            ),
            "{shell}"
        );
        assert!(shell.contains("export BAT_THEME=\"ansi\""), "{shell}");
        assert!(shell.contains("export STARSHIP_CONFIG="), "{shell}");
        assert!(text(&files, "btop.theme").contains("theme[hi_fg]=\"#7aa2f7\""));
        assert!(text(&files, "tmux.conf").contains("pane-active-border-style \"fg=#7aa2f7\""));
    }
}
