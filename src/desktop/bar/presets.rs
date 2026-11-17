//! Ready-made pills, whole layouts and looks for the bar (ROADMAP 18b, S8).
//!
//! A **preset** is a pill of items: adding one makes a new pill at the end of a side, without
//! the items the bar may hold only once and already has (it says which). A **layout** replaces
//! every pill at once (Palette's theme pill stays if the bar has one). A **look** changes only
//! how the bar is drawn (height, background, pill shape), never its items, font or colours.

use anyhow::{Result, bail};
use serde::Serialize;

use super::{Bar, Pill, PillLook, Place, find_kind, matched_to_menu_bar};

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub what: &'static str,
    pub kinds: &'static [&'static str],
    /// `pill` (adds one pill) or `layout` (replaces the whole bar's pills).
    pub sort: &'static str,
}

const INFO: &[&str] = &["cpu", "gpu", "temp", "pwr", "mem", "disk", "vpn", "net"];
const STATUS: &[&str] = &["weather", "brightness", "volume", "battery", "clock"];
const APPS: &[&str] = &["habits", "dispatch"];

pub const PRESETS: &[Preset] = &[
    Preset {
        id: "info",
        name: "Info",
        what: "CPU, GPU, temperature, power, memory, disk, VPN and network",
        kinds: INFO,
        sort: "pill",
    },
    Preset {
        id: "status",
        name: "Status",
        what: "Weather, brightness, volume, battery and the clock",
        kinds: STATUS,
        sort: "pill",
    },
    Preset {
        id: "workspaces",
        name: "Workspaces + app",
        what: "AeroSpace's workspaces and the app you're in",
        kinds: &["workspaces", "front_app"],
        sort: "pill",
    },
    Preset {
        id: "clock-battery",
        name: "Clock + battery",
        what: "Battery and the clock",
        kinds: &["battery", "clock"],
        sort: "pill",
    },
    Preset {
        id: "nowplaying",
        name: "Now playing",
        what: "What's playing, with beat lines",
        kinds: &["nowplaying"],
        sort: "pill",
    },
    Preset {
        id: "apps",
        name: "Your apps",
        what: "Habit Tracker and Dispatch, the ones on this Mac",
        kinds: APPS,
        sort: "pill",
    },
    Preset {
        id: "default",
        name: "Default bar",
        what: "Workspaces and stats on the left; Palette's theme pill and the status on the right",
        kinds: &[],
        sort: "layout",
    },
    Preset {
        id: "minimal",
        name: "Minimal bar",
        what: "Workspaces on the left, the clock on the right",
        kinds: &[],
        sort: "layout",
    },
    Preset {
        id: "everything",
        name: "Everything",
        what: "Every item Palette has (your own links and scripts aside)",
        kinds: &[],
        sort: "layout",
    },
];

pub fn find(id: &str) -> Result<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id).ok_or_else(|| {
        let ids: Vec<&str> = PRESETS.iter().map(|p| p.id).collect();
        anyhow::anyhow!("no preset `{id}`: {}", ids.join(", "))
    })
}

/// What [`add`] did.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Added {
    /// The new pill's index (a pill preset), `None` for a layout.
    pub pill: Option<usize>,
    /// Item names left out: already on the bar, or (Your apps) not on this Mac.
    pub skipped: Vec<String>,
}

/// Add preset `id`: a pill at the end of `side`, or (a layout) every pill replaced.
/// `have(kind)` says whether an item's app is on this Mac (Your apps keeps only those).
pub fn add(bar: &mut Bar, id: &str, side: &str, have: &dyn Fn(&str) -> bool) -> Result<Added> {
    let preset = find(id)?;
    if preset.sort == "layout" {
        bar.pills = layout(bar, id, have);
        return Ok(Added {
            pill: None,
            skipped: Vec::new(),
        });
    }
    let used: Vec<String> = bar
        .pills
        .iter()
        .flat_map(|p| p.items.iter().map(|i| i.kind.clone()))
        .collect();
    let mut kinds = Vec::new();
    let mut skipped = Vec::new();
    for &k in preset.kinds {
        let kind = find_kind(k).expect("presets name real kinds");
        if kind.single && used.iter().any(|u| u == k) {
            skipped.push(format!("{} (already on the bar)", kind.name));
        } else if preset.id == "apps" && !have(k) {
            skipped.push(format!("{} (not on this Mac)", kind.name));
        } else {
            kinds.push(k);
        }
    }
    if kinds.is_empty() {
        bail!(
            "nothing to add: {} {}",
            preset.name,
            if skipped.is_empty() {
                "is empty".to_string()
            } else {
                format!("would only add {}", skipped.join(", "))
            }
        );
    }
    bar.pills.push(Pill::of(side, &kinds));
    let at = bar.move_pill(bar.pills.len() - 1, Place::End(side.to_string()))?;
    Ok(Added {
        pill: Some(at),
        skipped,
    })
}

/// The pills of a whole layout, each with Palette's theme pill (owner, 2026-11-11). Default is
/// Palette's starting bar without the Habit Tracker (that's in Your apps).
fn layout(_bar: &Bar, id: &str, have: &dyn Fn(&str) -> bool) -> Vec<Pill> {
    match id {
        "minimal" => vec![
            Pill::of("left", &["workspaces"]),
            Pill::of("right", &["palette"]),
            Pill::of("right", &["clock"]),
        ],
        "everything" => {
            let apps: Vec<&str> = APPS.iter().copied().filter(|k| have(k)).collect();
            let mut v = vec![
                Pill::of("left", &["workspaces", "front_app"]),
                Pill::of("left", INFO),
                Pill::of("right", &["palette"]),
            ];
            if !apps.is_empty() {
                v.push(Pill::of("right", &apps));
            }
            v.push(Pill::of("center", &["nowplaying"]));
            v.push(Pill::of("right", STATUS));
            v
        }
        _ => {
            let mut pills = Bar::default().pills;
            pills.retain(|p| !p.items.iter().any(|i| i.kind == "habits"));
            pills
        }
    }
}

// ---------------------------------------------------------------- looks (S8)

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Look {
    pub id: &'static str,
    pub name: &'static str,
    pub what: &'static str,
}

pub const LOOKS: &[Look] = &[
    Look {
        id: "floating",
        name: "Floating pills",
        what: "Rounded pills over the wallpaper, a little below the top (Palette's default)",
    },
    Look {
        id: "solid",
        name: "Solid strip",
        what: "One full-width strip in the theme's background, pills faint inside it",
    },
    Look {
        id: "minimal",
        name: "Minimal text",
        what: "No bar and no pills: only icons and text",
    },
    Look {
        id: "notch",
        name: "Notch hugging",
        what: "As tall as the menu bar strip, from the very top, round-ended pills",
    },
];

/// `bar` drawn in look `id`; its items, font, colours and refresh stay. `menu` is the Mac's
/// menu bar height (for Notch hugging; 32 pt without it).
pub fn with_look(bar: &Bar, id: &str, menu: Option<u32>) -> Result<Bar> {
    let base = Bar::default();
    let mut b = bar.clone();
    let reset = |b: &mut Bar| {
        b.height = base.height;
        b.y_offset = base.y_offset;
        b.margin = base.margin;
        b.padding = base.padding;
        b.corner_radius = base.corner_radius;
        b.blur_radius = base.blur_radius;
        b.background = base.background.clone();
        b.background_opacity = base.background_opacity;
        b.pill = PillLook {
            colour: bar.pill.colour.clone(),
            border_colour: bar.pill.border_colour.clone(),
            ..PillLook::default()
        };
    };
    match id {
        "floating" => reset(&mut b),
        "solid" => {
            reset(&mut b);
            b.y_offset = 0;
            b.background = "background".into();
            b.background_opacity = 100;
            b.pill.opacity = 45;
            b.pill.height = b.height - 10;
            b.pill.corner_radius = 8;
        }
        "minimal" => {
            reset(&mut b);
            b.pill.opacity = 0;
            b.pill.border_width = 0;
            b.pill.edge = 4;
            b.pill.gap = 14;
        }
        "notch" => {
            reset(&mut b);
            b = matched_to_menu_bar(&b, menu.unwrap_or(32));
            b.pill.height = b.height.saturating_sub(6).max(6);
            b.pill.corner_radius = (b.pill.height / 2).min(40);
        }
        _ => {
            let ids: Vec<&str> = LOOKS.iter().map(|l| l.id).collect();
            bail!("no look `{id}`: {}", ids.join(", "));
        }
    }
    Ok(b)
}
