//! Colour lab (IDEAS 9-15): before/after for every theme, as a static page and CSV tables.
//!
//!     cargo run --example colour_lab -- docs/reports/colour-lab [theme …]
//!
//! Writes `<out>/demo.html` (every theme), `<out>/tables/*.csv`, and for each theme named on
//! the command line `<out>/figs/<theme>.html` (one card, for the report's screenshots). Reads
//! your themes from ~/.config/palette read-only; never writes outside `<out>`.
//!
//! Methods: B as made (or as the importer guessed) · F Palette today (readability fixes,
//! Minimal) · R ranked roles (idea 9) + F · U R + reuse the theme's own colours (idea 10) ·
//! M U + spaced greys (idea 13A, monochrome only) · T M + tint (idea 13B, opt-in) · G glass
//! (idea 12, your themes over their own wallpaper at 50 %).

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use palette::colour;
use palette::normalise::{self, Level};
use palette::paths::Paths;
use palette::roles::{self, Step};
use palette::theme::{self, Colors};

struct Source {
    name: String,
    group: &'static str,
    /// The roles were guessed (no names in the theme).
    derived: bool,
    colors: Colors,
    wallpaper: Option<PathBuf>,
}

struct Variant {
    code: &'static str,
    title: &'static str,
    colors: Colors,
    steps: Vec<Step>,
}

const KEYS_SHOWN: [&str; 11] = [
    "darker_background",
    "dark_background",
    "background",
    "lighter_background",
    "selection",
    "muted",
    "dark_foreground",
    "foreground",
    "light_foreground",
    "bright_foreground",
    "accent",
];

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = PathBuf::from(
        args.first()
            .cloned()
            .unwrap_or("docs/reports/colour-lab".into()),
    );
    let figs: BTreeSet<String> = args.iter().skip(1).cloned().collect();
    fs::create_dir_all(out.join("tables"))?;
    fs::create_dir_all(out.join("figs"))?;

    let sources = sources()?;
    let mut html = String::new();
    let mut themes_csv = String::from(
        "group,theme,derived,mono,method,order_problems,low_pairs,terminals,editors,lists,ansi_on_selection,changes,reused,mean_de,max_de\n",
    );
    let mut apca_csv = String::from("group,theme,pair,wcag,apca_lc,wcag_pass,apca_pass\n");
    let mut glass_csv =
        String::from("theme,behind,opacity,low_before,low_after,lifted,solid_from_opacity\n");
    let mut index = String::new();
    let mut written = BTreeSet::new();

    for group in ["yours", "bundled", "catalogue"] {
        let list: Vec<&Source> = sources.iter().filter(|s| s.group == group).collect();
        let _ = write!(index, "<h3>{group} ({})</h3><p class=idx>", list.len());
        html.push_str(&format!("<h2 id=\"{group}\">{group} ({})</h2>", list.len()));
        for s in list {
            let _ = write!(
                index,
                "<a href=\"#{g}-{n}\">{n}</a> ",
                g = group,
                n = s.name
            );
            let variants = variants(s);
            for v in &variants {
                let a = roles::apps(&v.colors);
                let (mean, max) = drift(&s.colors, &v.colors);
                let _ = writeln!(
                    themes_csv,
                    "{},{},{},{},{},{},{},{},{},{},{},{},{},{:.4},{:.4}",
                    group,
                    s.name,
                    s.derived,
                    roles::monochrome(&s.colors),
                    v.code,
                    roles::order_problems(&v.colors).len(),
                    palette::check::colors(&s.name, &v.colors).issues.len(),
                    a[0].grade,
                    a[1].grade,
                    a[2].grade,
                    roles::ansi_on_selection(&v.colors),
                    v.steps.len(),
                    v.steps
                        .iter()
                        .filter(|s| s.why.starts_with("theme's own colour"))
                        .count(),
                    mean,
                    max
                );
            }
            for (pair, fg, bg, wcag_min, apca_min) in [
                ("text", "foreground", "background", 4.5, 75.0),
                ("selected text", "bright_foreground", "selection", 4.5, 60.0),
                ("secondary", "dark_foreground", "background", 2.5, 45.0),
                ("comments", "muted", "background", 1.6, 30.0),
            ] {
                let f = &variants[1].colors; // F: what Palette gives today
                let w = colour::contrast(f.get(fg), f.get(bg));
                let a = roles::apca(f.get(fg), f.get(bg)).abs();
                let _ = writeln!(
                    apca_csv,
                    "{group},{},{pair},{w:.2},{a:.1},{},{}",
                    s.name,
                    w >= wcag_min * normalise::SLACK,
                    a >= apca_min
                );
            }
            let glass = s
                .wallpaper
                .as_ref()
                .and_then(|w| glass_card(s, w, &mut glass_csv));
            // The first group with the theme wins (yours, with your wallpaper, before the
            // catalogue's copy); figures list at most four changes per column.
            if figs.contains(&s.name) && written.insert(s.name.clone()) {
                fs::write(
                    out.join("figs").join(format!("{}.html", s.name)),
                    page(
                        &format!("{} · colour lab", s.name),
                        &card(s, &variants, glass.as_deref(), Some(4)),
                    ),
                )?;
            }
            html.push_str(&card(s, &variants, glass.as_deref(), None));
        }
        index.push_str("</p>");
    }
    let body = format!(
        "<h1>Colour lab: before and after</h1><p class=note>B as made · F Palette today (readability \
         fixes, Minimal) · R ranked roles (idea 9) + F · U R + the theme's own colours reused \
         (idea 10) · M U + spaced greys (13A, monochrome) · T M + tint (13B, opt-in). Badges: \
         terminals / editors / lists (idea 11): ✓ ok, ~ only thanks to other cues, ✗ bad. \
         Generated by <code>cargo run --example colour_lab</code>.</p>{index}{html}"
    );
    fs::write(out.join("demo.html"), page("Colour lab", &body))?;
    fs::write(out.join("tables/themes.csv"), themes_csv)?;
    fs::write(out.join("tables/apca.csv"), apca_csv)?;
    fs::write(out.join("tables/glass.csv"), glass_csv)?;
    eprintln!("wrote {} themes to {}", sources.len(), out.display());
    Ok(())
}

fn sources() -> anyhow::Result<Vec<Source>> {
    let mut out = Vec::new();
    // Yours: read-only from the real ~/.config/palette.
    let real = Paths::from_env()?;
    let mut names: Vec<String> = fs::read_dir(real.themes())?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    for name in names {
        let Ok(t) = theme::load_original(&real, &name) else {
            continue;
        };
        let derived = palette::catalog::find(&name).is_some_and(|e| e.derived.is_some());
        let wallpaper = t.backgrounds().ok().and_then(|b| b.into_iter().next());
        out.push(Source {
            name,
            group: "yours",
            derived,
            colors: t.colors,
            wallpaper,
        });
    }
    // Bundled: from an empty fake home.
    let tmp = std::env::temp_dir().join("palette-colour-lab-empty-home");
    let fake = Paths::fake_home(&tmp);
    for (name, _) in theme::list(&fake)? {
        let t = theme::load_original(&fake, &name)?;
        out.push(Source {
            name,
            group: "bundled",
            derived: false,
            colors: t.colors,
            wallpaper: None,
        });
    }
    for e in palette::catalog::entries() {
        out.push(Source {
            name: e.name.clone(),
            group: "catalogue",
            derived: e.derived.is_some(),
            colors: e.colors.clone(),
            wallpaper: None,
        });
    }
    Ok(out)
}

fn variants(s: &Source) -> Vec<Variant> {
    let none = BTreeSet::new();
    let b = s.colors.clone();
    let mut f = b.clone();
    let f_steps: Vec<Step> = normalise::fix(&mut f, Level::Minimal, &none)
        .into_iter()
        .map(|c| Step {
            key: c.key,
            from: c.from,
            to: c.to,
            why: c.why,
        })
        .collect();
    let mut r = b.clone();
    let mut r_steps = if s.derived {
        roles::rank_roles(&mut r)
    } else {
        Vec::new()
    };
    let mut u = r.clone();
    let mut u_steps = r_steps.clone();
    r_steps.extend(
        normalise::fix(&mut r, Level::Minimal, &none)
            .into_iter()
            .map(|c| Step {
                key: c.key,
                from: c.from,
                to: c.to,
                why: c.why,
            }),
    );
    u_steps.extend(roles::reuse(&mut u, Level::Minimal));
    let mut v = vec![
        Variant {
            code: "B",
            title: "as made",
            colors: b,
            steps: Vec::new(),
        },
        Variant {
            code: "F",
            title: "Palette today",
            colors: f,
            steps: f_steps,
        },
    ];
    if s.derived {
        v.push(Variant {
            code: "R",
            title: "ranked roles",
            colors: r,
            steps: r_steps,
        });
    }
    v.push(Variant {
        code: "U",
        title: "reuse own colours",
        colors: u.clone(),
        steps: u_steps.clone(),
    });
    if roles::monochrome(&s.colors) {
        let mut m = u.clone();
        let mut m_steps = u_steps.clone();
        m_steps.extend(roles::spaced_greys(&mut m));
        m_steps.extend(
            normalise::fix(&mut m, Level::Minimal, &none)
                .into_iter()
                .map(|c| Step {
                    key: c.key,
                    from: c.from,
                    to: c.to,
                    why: c.why,
                }),
        );
        let mut t = m.clone();
        let mut t_steps = m_steps.clone();
        t_steps.extend(roles::tint(&mut t));
        v.push(Variant {
            code: "M",
            title: "spaced greys",
            colors: m,
            steps: m_steps,
        });
        v.push(Variant {
            code: "T",
            title: "+ tint (opt-in)",
            colors: t,
            steps: t_steps,
        });
    }
    v
}

/// Mean and largest perceptual change (OKLab ΔE) over the colours shown.
fn drift(a: &Colors, b: &Colors) -> (f32, f32) {
    let d: Vec<f32> = a
        .iter()
        .map(|(k, v)| colour::delta_e(v, b.get(k)))
        .collect();
    let mean = d.iter().sum::<f32>() / d.len().max(1) as f32;
    (mean, d.iter().copied().fold(0.0, f32::max))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A small editor + terminal painted with `c`.
fn mock(c: &Colors) -> String {
    let k = |key: &str| c.get(key).to_string();
    let line = colour::mix(&k("background"), &k("lighter_background"), 0.5);
    format!(
        r#"<div class=mock style="background:{bg};color:{fg}">
<div class=tabs style="background:{darker}"><span style="background:{bg};color:{fg};border-top:2px solid {accent}">main.rs</span><span style="background:{panel};color:{dfg}">notes.md</span><span style="background:{dark};color:{dfg}">bar</span></div>
<div class=ln><i style="color:{dfg}">1</i><span style="color:{muted}">// a comment, quiet</span></div>
<div class=ln><i style="color:{dfg}">2</i><span style="color:{magenta}">fn</span> <span style="color:{blue}">main</span>() {{</div>
<div class=ln style="background:{line}"><i style="color:{dfg}">3</i>  let s = <span style="color:{green}">"text"</span>; <span style="color:{yellow}">42</span></div>
<div class=ln style="background:{sel};color:{bright}"><i style="color:{dfg}">4</i>  selected <span style="color:{red}">err</span> <span style="color:{green}">ok</span> <span style="color:{blue}">fn</span></div>
<div class=ln><i style="color:{dfg}">5</i>  <span style="color:{accent}">link</span> <span style="color:{cyan}">cyan</span> }}</div>
<div class=term><span style="color:{blue}">~/p</span> <span style="color:{green}">❯</span> git diff<br><span style="color:{red}">- old line</span><br><span style="color:{green}">+ new line</span><br><span style="background:{sel};color:{bright}">selected in the terminal</span></div>
</div>"#,
        bg = k("background"),
        fg = k("foreground"),
        darker = k("darker_background"),
        dark = k("dark_background"),
        panel = k("lighter_background"),
        accent = k("accent"),
        dfg = k("dark_foreground"),
        muted = k("muted"),
        magenta = k("magenta"),
        blue = k("blue"),
        green = k("green"),
        yellow = k("yellow"),
        red = k("red"),
        cyan = k("cyan"),
        sel = k("selection"),
        bright = k("bright_foreground"),
    )
}

fn swatches(c: &Colors, before: &Colors) -> String {
    let mut s = String::from("<div class=sw>");
    for key in KEYS_SHOWN {
        let changed = c.get(key) != before.get(key);
        let _ = write!(
            s,
            "<span title=\"{key} {v}\" class=\"{cls}\" style=\"background:{v}\"></span>",
            v = c.get(key),
            cls = if changed { "ch" } else { "" }
        );
    }
    s.push_str("</div><div class=sw>");
    for key in ["red", "green", "yellow", "blue", "magenta", "cyan"] {
        let changed = c.get(key) != before.get(key);
        let _ = write!(
            s,
            "<span title=\"{key} {v}\" class=\"{cls}\" style=\"background:{v}\"></span>",
            v = c.get(key),
            cls = if changed { "ch" } else { "" }
        );
    }
    s.push_str("</div>");
    s
}

fn badges(c: &Colors) -> String {
    roles::apps(c)
        .iter()
        .map(|v| {
            let mark = match v.grade {
                "ok" => "✓",
                "weak" => "~",
                _ => "✗",
            };
            format!(
                "<span class=\"b {}\" title=\"{}\">{} {}</span>",
                v.grade,
                esc(&v.why),
                v.app,
                mark
            )
        })
        .collect()
}

fn card(s: &Source, variants: &[Variant], glass: Option<&str>, max_steps: Option<usize>) -> String {
    let mut h = format!(
        "<section class=card id=\"{}-{}\"><h3>{} <small>{}{}{}</small></h3><div class=row>",
        s.group,
        s.name,
        s.name,
        if s.derived {
            "roles guessed"
        } else {
            "named roles"
        },
        if roles::monochrome(&s.colors) {
            " · monochrome"
        } else {
            ""
        },
        match roles::order_problems(&s.colors).len() {
            0 => String::new(),
            n => format!(" · {n} order problem{}", if n == 1 { "" } else { "s" }),
        }
    );
    for v in variants {
        let shown = max_steps.unwrap_or(usize::MAX);
        let more = v.steps.len().saturating_sub(shown);
        let mut steps: String = v
            .steps
            .iter()
            .take(shown)
            .map(|st| {
                format!(
                    "<li><b>{}</b> <code>{}</code>→<code>{}</code> {}</li>",
                    st.key,
                    st.from,
                    st.to,
                    esc(&st.why)
                )
            })
            .collect();
        if more > 0 {
            let _ = write!(steps, "<li>+{more} more (demo.html)</li>");
        }
        let problems = roles::order_problems(&v.colors);
        let _ = write!(
            h,
            "<div class=col><div class=head><b>{}</b> {}</div>{}{}<div class=badges>{}</div>{}<ul>{}</ul></div>",
            v.code,
            v.title,
            mock(&v.colors),
            swatches(&v.colors, &variants[0].colors),
            badges(&v.colors),
            if problems.is_empty() {
                String::new()
            } else {
                format!("<div class=warn>{}</div>", esc(&problems.join("; ")))
            },
            steps
        );
    }
    h.push_str("</div>");
    if let Some(g) = glass {
        h.push_str(g);
    }
    h.push_str("</section>");
    h
}

/// Idea 12 for one of your themes: the wallpaper's brightest tenth behind 50 % glass.
fn glass_card(s: &Source, wallpaper: &Path, csv: &mut String) -> Option<String> {
    let image = palette::pixels::read(wallpaper, Some(96)).ok()?;
    let dark_theme = colour::contrast(s.colors.get("background"), "#000000") < 4.0;
    let (worst, _avg) = roles::behind(&image, dark_theme);
    let mut base = s.colors.clone();
    normalise::apply(&mut base, Level::Minimal, &BTreeSet::new());
    let (_, low_before) = roles::glass(&base, &worst, 0.5);
    let mut fixed = base.clone();
    let (steps, solid) = roles::fix_glass(&mut fixed, &worst, 0.5, Level::Minimal);
    let (_, low_after) = roles::glass(&fixed, &worst, 0.5);
    let _ = writeln!(
        csv,
        "{},{},0.5,{},{},{},{}",
        s.name,
        worst,
        low_before.len(),
        low_after.len(),
        steps.len(),
        solid.map(|o| o.to_string()).unwrap_or("none".into())
    );
    let pane = |c: &Colors, title: &str, low: &[String]| {
        let bg = c.get("background");
        format!(
            "<div class=col><div class=head><b>G</b> {title}</div><div class=glass style=\"background:{worst}\"><div class=tint style=\"background:{bg}80\">{}</div></div><div class=warn>{}</div></div>",
            mock(c).replace(
                &format!("background:{bg};color"),
                "background:transparent;color"
            ),
            if low.is_empty() {
                "all text readable on the brightest part".into()
            } else {
                esc(&low.join(", "))
            }
        )
    };
    let note = match solid {
        Some(o) => format!("without any fix, readable from {o} % opacity"),
        None => "never fully readable over this wallpaper without a fix".into(),
    };
    Some(format!(
        "<div class=row>{}{}<div class=col><div class=head><b>G</b> what's behind</div><div class=swbig style=\"background:{worst}\"></div><p class=note>Brightest tenth of the wallpaper, <code>{worst}</code>. 50 % glass (your Ghostty). {note}.</p></div></div>",
        pane(&base, "glass 50 %, Palette today", &low_before),
        pane(&fixed, "glass 50 %, text lifted", &low_after),
    ))
}

fn page(title: &str, body: &str) -> String {
    format!(
        r#"<!doctype html><meta charset=utf-8><title>{title}</title><style>
body{{font:13px -apple-system,system-ui,sans-serif;background:#f4f4f2;color:#222;margin:16px}}
h1{{font-size:20px}} h2{{margin-top:28px;border-bottom:1px solid #ccc}} h3{{margin:6px 0;font-size:14px}}
small{{color:#777;font-weight:normal}} .note{{color:#555;max-width:900px}} .idx a{{margin-right:6px}}
.card{{background:#fff;border:1px solid #ddd;border-radius:8px;padding:8px 10px;margin:10px 0}}
.row{{display:flex;gap:10px;flex-wrap:wrap;align-items:flex-start}} .col{{width:270px}}
.head{{font-size:12px;margin-bottom:3px}}
.mock{{font:11px/1.45 ui-monospace,Menlo,monospace;border-radius:6px;overflow:hidden;border:1px solid #0002}}
.tabs{{display:flex}} .tabs span{{padding:2px 8px}} .ln{{padding:0 6px;white-space:pre}} .ln i{{display:inline-block;width:14px;font-style:normal}}
.term{{padding:4px 6px;border-top:1px solid #8883}}
.sw{{display:flex;margin-top:3px}} .sw span{{flex:1;height:12px}} .sw span.ch{{outline:2px solid #e33;outline-offset:-2px}}
.badges{{margin:3px 0}} .b{{font-size:11px;padding:1px 5px;border-radius:4px;margin-right:3px}}
.b.ok{{background:#d8f0d8}} .b.weak{{background:#f6ebc6}} .b.bad{{background:#f6d0d0}}
.warn{{font-size:11px;color:#a33}} ul{{margin:3px 0;padding-left:16px;font-size:11px;color:#444}} code{{font-size:10.5px}}
.glass{{background-size:cover;background-position:center;border-radius:6px;padding:8px}} .tint{{backdrop-filter:blur(8px);-webkit-backdrop-filter:blur(8px);border-radius:6px}}
.swbig{{height:60px;border-radius:6px;border:1px solid #0002}}
</style>{body}"#
    )
}
