//! The bar (SketchyBar), built by Palette: `desktop/bar.toml` says which
//! pills there are, what is in each, how often they refresh and what a click
//! opens; Palette writes `~/.config/sketchybar/sketchybarrc` from it and its
//! pill scripts to `~/.config/palette/bar/`. Colours are theme colour names
//! (`C_<name>` from Palette's `sketchybar.sh`), so a theme switch recolours
//! the bar without rebuilding it.
//!
//! The pill scripts started as the owner's own (spaces, stats, widgets,
//! habits): Palette only runs scripts it ships or ones you point it at.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use super::{File, Tool, sh_quote};
use crate::paths::Paths;
use crate::system::{self, Outcome};

pub mod live;
pub mod presets;
pub mod recolour;
pub mod saved;

const SCRIPTS: [(&str, &str); 9] = [
    ("spaces.sh", include_str!("spaces.sh")),
    ("stats.sh", include_str!("stats.sh")),
    ("stats_feed.sh", include_str!("stats_feed.sh")),
    ("widgets.sh", include_str!("widgets.sh")),
    ("habits.sh", include_str!("habits.sh")),
    ("dispatch.sh", include_str!("dispatch.sh")),
    ("front_app.sh", include_str!("front_app.sh")),
    ("nowplaying.sh", include_str!("nowplaying.sh")),
    ("nowplaying.js", include_str!("nowplaying.js")),
];

/// Something that can go in a pill.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Kind {
    pub id: &'static str,
    pub name: &'static str,
    pub what: &'static str,
    /// For grouping the "Add" menu: `desktop`, `system`, `stats`, `yours`.
    pub group: &'static str,
    /// Only one in the bar.
    pub single: bool,
    /// What a click opens, unless you change it.
    pub click: &'static str,
    /// Refresh, in seconds (0: updates on events, or never).
    pub every: u32,
    /// Uses the network (shown in the app).
    pub network: bool,
}

const OPEN_AM: &str = "open -a 'Activity Monitor'";
const fn kind(
    id: &'static str,
    name: &'static str,
    what: &'static str,
    group: &'static str,
    click: &'static str,
    every: u32,
) -> Kind {
    Kind {
        id,
        name,
        what,
        group,
        single: true,
        click,
        every,
        network: false,
    }
}

pub const KINDS: &[Kind] = &[
    kind(
        "workspaces",
        "Workspaces",
        "AeroSpace workspaces with their apps; click to go there",
        "desktop",
        "",
        // Only a safety net: events cover every change (ROADMAP 18d, was 30 s).
        120,
    ),
    kind(
        "front_app",
        "Active app",
        "The app you're in, icon and name",
        "desktop",
        "",
        0,
    ),
    kind(
        "palette",
        "Palette",
        "The theme: click for the picker, scroll to switch",
        "desktop",
        "",
        0,
    ),
    kind(
        "clock",
        "Clock",
        "Date and time",
        "system",
        "open -a Calendar",
        30,
    ),
    kind(
        "battery",
        "Battery",
        "Charge, and a bolt when plugged in",
        "system",
        "open 'x-apple.systempreferences:com.apple.Battery-Settings.extension'",
        120,
    ),
    kind(
        "volume",
        "Volume",
        "Sound level",
        "system",
        "open 'x-apple.systempreferences:com.apple.Sound-Settings.extension'",
        0,
    ),
    kind(
        "brightness",
        "Brightness",
        "Screen brightness (after the first change)",
        "system",
        "open 'x-apple.systempreferences:com.apple.Displays-Settings.extension'",
        0,
    ),
    Kind {
        network: true,
        ..kind(
            "weather",
            "Weather",
            "Temperature and sky, from Open-Meteo (uses the network)",
            "system",
            "open -a Weather",
            900,
        )
    },
    kind(
        "habits",
        "Habit Tracker",
        "Today's habits done / due, and a running timer",
        "system",
        "open 'habittracker://open?view=log'",
        30,
    ),
    kind(
        "dispatch",
        "Dispatch",
        "Papers read today, or what you pick in Dispatch's Settings",
        "system",
        "open 'dispatch://open/research'",
        60,
    ),
    kind(
        "nowplaying",
        "Now playing",
        "Title and artist of what's playing, with beat lines; click: play/pause or open the player",
        "system",
        "",
        0,
    ),
    kind("cpu", "CPU", "Processor use", "stats", OPEN_AM, 0),
    kind("gpu", "GPU", "Graphics use", "stats", OPEN_AM, 0),
    kind(
        "temp",
        "Temperature",
        "CPU temperature (needs macmon)",
        "stats",
        OPEN_AM,
        0,
    ),
    kind(
        "gputemp",
        "GPU temperature",
        "Graphics chip temperature (needs macmon)",
        "stats",
        OPEN_AM,
        0,
    ),
    kind(
        "pwr",
        "Power",
        "Whole-Mac watts (needs macmon)",
        "stats",
        OPEN_AM,
        0,
    ),
    kind(
        "mem",
        "Memory",
        "Memory in use, like Activity Monitor",
        "stats",
        OPEN_AM,
        0,
    ),
    kind(
        "disk",
        "Disk",
        "Disk use, shown only when nearly full",
        "stats",
        "open 'x-apple.systempreferences:com.apple.settings.Storage'",
        0,
    ),
    kind(
        "vpn",
        "VPN",
        "A shield while a VPN is connected",
        "stats",
        "open 'x-apple.systempreferences:com.apple.Network-Settings.extension'",
        0,
    ),
    kind(
        "net",
        "Network",
        "Wi-Fi state and download speed",
        "stats",
        "open 'x-apple.systempreferences:com.apple.wifi-settings-extension'",
        0,
    ),
    Kind {
        single: false,
        ..kind(
            "link",
            "Link",
            "An icon or text that opens an app, a website or a folder",
            "yours",
            "",
            0,
        )
    },
    Kind {
        single: false,
        ..kind(
            "script",
            "Your script",
            "Runs a script of yours to set its icon and label",
            "yours",
            "",
            60,
        )
    },
];

pub fn find_kind(id: &str) -> Option<&'static Kind> {
    KINDS.iter().find(|k| k.id == id)
}

const STATS: [&str; 9] = [
    "cpu", "gpu", "temp", "gputemp", "pwr", "mem", "disk", "vpn", "net",
];

/// Nerd Font glyphs, as in the owner's bar.
fn glyph(kind: &str) -> &'static str {
    match kind {
        "clock" => "\u{f017}",
        "cpu" => "\u{f2db}",
        "gpu" => "\u{f108}",
        "mem" => "\u{efc5}",
        "disk" => "\u{f0a0}",
        "net" => "\u{f019}",
        "temp" => "\u{f2c9}",
        "gputemp" => "\u{f2c8}",
        "pwr" => "\u{f1e6}",
        "vpn" => "\u{f132}",
        "link" => "\u{f0c1}",
        "nowplaying" => "\u{f001}",
        _ => "",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Item {
    pub kind: String,
    /// Shell command a click runs; empty: the kind's own; `none`: nothing.
    pub click: String,
    /// Refresh in seconds; 0: the kind's own.
    pub every: u32,
    /// Links and scripts: the icon (a Nerd Font glyph or text).
    pub icon: String,
    /// Links and scripts: the text.
    pub label: String,
    /// Clock: `date` format, `%a %d %H:%M`.
    pub format: String,
    /// Weather: city or `lat,lon`; empty guesses from your IP.
    pub location: String,
    /// Weather: `C` or `F`.
    pub units: String,
    /// Your script: its path.
    pub path: String,
    /// Workspaces: the apps' icons on each.
    pub show_apps: bool,
    /// Workspaces: hide empty ones you're not on.
    pub hide_empty: bool,
    /// Workspaces: every display's bar shows all of them (off: each shows
    /// the workspaces on that display).
    pub all_displays: bool,
    /// Disk: shown from this % full.
    pub disk_at: u8,
    /// GPU: `busy` (macmon's share of time busy, steady) or `raw` (the GPU driver's
    /// "Device Utilization %", jumps between 0 and ~50 at rest). Without macmon: raw.
    pub gpu: String,
    /// Hidden while Performance mode's "Slower bar" part is on (ROADMAP 18, S11).
    #[serde(skip_serializing_if = "is_false")]
    pub hide_in_performance: bool,
    /// Now playing: beat lines that move with what the Mac plays (Palette's helper, ROADMAP 20).
    #[serde(skip_serializing_if = "is_true")]
    pub beats: bool,
    /// Now playing: the title's width, in characters.
    #[serde(skip_serializing_if = "is_default_width")]
    pub width: u8,
    /// Now playing: a longer title scrolls (true) or is cut with … (false).
    #[serde(skip_serializing_if = "is_true")]
    pub scroll: bool,
    /// Now playing: what a click does unless `click` says otherwise: `toggle` (play/pause) or `open`
    /// (the player).
    #[serde(skip_serializing_if = "is_toggle")]
    pub action: String,
}

fn is_false(v: &bool) -> bool {
    !*v
}

fn is_true(v: &bool) -> bool {
    *v
}

fn is_default_width(v: &u8) -> bool {
    *v == NOWPLAYING_WIDTH
}

fn is_toggle(v: &String) -> bool {
    v == "toggle"
}

/// Now playing's title width by default, in characters.
pub const NOWPLAYING_WIDTH: u8 = 24;
/// Beat lines in the now-playing pill.
pub const BEAT_LINES: u8 = 5;

impl Default for Item {
    fn default() -> Self {
        Self {
            kind: String::new(),
            click: String::new(),
            every: 0,
            icon: String::new(),
            label: String::new(),
            format: "%a %d %H:%M".into(),
            location: String::new(),
            units: "C".into(),
            path: String::new(),
            show_apps: true,
            hide_empty: false,
            all_displays: false,
            disk_at: 80,
            gpu: "busy".into(),
            hide_in_performance: false,
            beats: true,
            width: NOWPLAYING_WIDTH,
            scroll: true,
            action: "toggle".into(),
        }
    }
}

impl Item {
    pub fn of(kind: &str) -> Self {
        Self {
            kind: kind.into(),
            ..Self::default()
        }
    }

    fn kind(&self) -> &'static Kind {
        find_kind(&self.kind).expect("validated")
    }

    fn every(&self) -> u32 {
        if self.every > 0 {
            self.every
        } else {
            self.kind().every
        }
    }

    fn click(&self) -> Option<String> {
        match self.click.as_str() {
            "none" => None,
            "" if self.kind().click.is_empty() => None,
            "" => Some(self.kind().click.to_string()),
            mine => Some(mine.to_string()),
        }
    }
}

/// A rounded group of items.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pill {
    /// `left`, `center` or `right`. Centre pills sit in the middle of the screen
    /// (behind the notch on a MacBook screen that has one).
    pub side: String,
    /// Left to right, as you see them.
    pub items: Vec<Item>,
    /// This pill's own colour (a theme colour name), `none` for no background; empty: the
    /// bar's pill colour (ROADMAP 18, S9).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub colour: String,
    /// This pill's own opacity, 0 to 100; unset: the bar's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opacity: Option<u8>,
}

impl Default for Pill {
    fn default() -> Self {
        Self {
            side: "left".into(),
            items: Vec::new(),
            colour: String::new(),
            opacity: None,
        }
    }
}

impl Pill {
    pub fn of(side: &str, kinds: &[&str]) -> Self {
        Self {
            side: side.into(),
            items: kinds.iter().map(|k| Item::of(k)).collect(),
            ..Self::default()
        }
    }

    /// `background.color=…` for this pill: its own colour and opacity, else the bar's.
    /// `None`: no background at all.
    fn background(&self, look: &PillLook) -> Option<String> {
        match self.colour.as_str() {
            "none" => None,
            "" => Some(argb(&look.colour, self.opacity.unwrap_or(look.opacity))),
            own => Some(argb(own, self.opacity.unwrap_or(look.opacity))),
        }
    }

    /// Whether this pill looks different from the bar's pills.
    fn own_look(&self) -> bool {
        !self.colour.is_empty() || self.opacity.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PillLook {
    pub height: u32,
    pub corner_radius: u32,
    /// Theme colour name.
    pub colour: String,
    /// 0 to 100.
    pub opacity: u8,
    pub border_width: u32,
    pub border_colour: String,
    /// Space between pills.
    pub gap: u32,
    /// Space inside a pill's ends.
    pub edge: u32,
}

impl Default for PillLook {
    fn default() -> Self {
        Self {
            height: 30,
            corner_radius: 14,
            colour: "selection".into(),
            opacity: 94,
            border_width: 0,
            border_colour: "accent".into(),
            gap: 6,
            edge: 12,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bar {
    /// `top` or `bottom`.
    pub position: String,
    pub height: u32,
    /// Distance from the screen edge.
    pub y_offset: i32,
    /// Space left and right of the bar.
    pub margin: u32,
    pub padding: u32,
    pub corner_radius: u32,
    pub blur_radius: u32,
    /// Theme colour behind the whole bar; empty: none (only the pills show).
    pub background: String,
    pub background_opacity: u8,
    pub font: String,
    /// `Bold`, `Medium`, `Regular`, …
    pub font_style: String,
    pub icon_size: f64,
    pub label_size: f64,
    /// Theme colour of icons.
    pub icon_colour: String,
    /// Theme colour of text.
    pub text_colour: String,
    pub pill: PillLook,
    /// How often the stats (CPU … network) refresh, in seconds.
    pub stats_every: u32,
    pub pills: Vec<Pill>,
    /// Your icon per app (IDEAS 37b, `palette desktop bar icon`): app name as AeroSpace shows
    /// it → a glyph of the icon font (`:ollama:`) or a few characters. Wins over the icon map.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub icons: BTreeMap<String, String>,
}

impl Default for Bar {
    /// The owner's bar: workspaces and stats on the left; habits and a
    /// status pill on the right, with Palette's theme pill.
    fn default() -> Self {
        let pill = Pill::of;
        Self {
            position: "top".into(),
            height: 36,
            y_offset: 4,
            margin: 0,
            padding: 10,
            corner_radius: 0,
            blur_radius: 0,
            background: String::new(),
            background_opacity: 100,
            font: "JetBrainsMono Nerd Font".into(),
            font_style: "Bold".into(),
            icon_size: 13.0,
            label_size: 12.0,
            icon_colour: "blue".into(),
            text_colour: "bright_foreground".into(),
            pill: PillLook::default(),
            stats_every: 5,
            pills: vec![
                pill("left", &["workspaces"]),
                pill(
                    "left",
                    &["cpu", "gpu", "temp", "pwr", "mem", "disk", "vpn", "net"],
                ),
                pill("right", &["palette"]),
                pill("right", &["habits"]),
                pill(
                    "right",
                    &["weather", "brightness", "volume", "battery", "clock"],
                ),
            ],
            icons: BTreeMap::new(),
        }
    }
}

/// Where [`Bar::move_pill`] puts a pill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// At the end of `left`, `center` or `right`.
    End(String),
    /// Just before this pill (an index into `pills`), on its side.
    Before(usize),
    /// One place right (1) or left (-1) on screen; at the end of its side it
    /// moves into the next side (left, centre, right) without passing a pill.
    Step(i32),
}

impl std::str::FromStr for Place {
    type Err = anyhow::Error;
    /// `left`, `center` (or `centre`), `right`, `next` (one place right), `prev`.
    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "next" => Place::Step(1),
            "prev" => Place::Step(-1),
            "centre" => Place::End("center".into()),
            s if SIDES.contains(&s) => Place::End(s.into()),
            _ => bail!("`{s}`: say left, center, right, next or prev"),
        })
    }
}

impl Bar {
    /// Indexes into `pills` in the order they appear on screen: the left
    /// pills, the centre ones, then the right ones.
    pub fn screen_order(&self) -> Vec<usize> {
        SIDES
            .iter()
            .flat_map(|side| (0..self.pills.len()).filter(move |&i| self.pills[i].side == *side))
            .collect()
    }

    /// The pill `what` names: its number on screen (1 is the leftmost, as
    /// `palette desktop bar pills` lists them) or an item it holds (`habits`).
    pub fn find_pill(&self, what: &str) -> Result<usize> {
        let order = self.screen_order();
        if let Ok(n) = what.parse::<usize>() {
            return match order.get(n.wrapping_sub(1)) {
                Some(&i) => Ok(i),
                None => bail!("there are {} pills; {n} isn't one", order.len()),
            };
        }
        let found: Vec<usize> = order
            .into_iter()
            .filter(|&i| self.pills[i].items.iter().any(|it| it.kind == what))
            .collect();
        match found[..] {
            [i] => Ok(i),
            [] => bail!("no pill holds `{what}` (see palette desktop bar pills)"),
            _ => {
                bail!("several pills hold `{what}`: use its number from palette desktop bar pills")
            }
        }
    }

    /// Move pill `from` (an index into `pills`); returns where it ended up.
    pub fn move_pill(&mut self, from: usize, to: Place) -> Result<usize> {
        if from >= self.pills.len() {
            bail!("there is no pill {from}");
        }
        let side = self.pills[from].side.clone();
        let (side, before) = match to {
            Place::End(side) => {
                if !SIDES.contains(&side.as_str()) {
                    bail!("a pill's side is left, center or right");
                }
                (side, None)
            }
            Place::Before(i) => {
                if i >= self.pills.len() {
                    bail!("there is no pill {i}");
                }
                if i == from {
                    return Ok(from);
                }
                (self.pills[i].side.clone(), Some(i))
            }
            Place::Step(by) => {
                let mine: Vec<usize> = (0..self.pills.len())
                    .filter(|&i| self.pills[i].side == side)
                    .collect();
                let at = mine.iter().position(|&i| i == from).unwrap_or(0) as i32;
                if let Some(&next) = usize::try_from(at + by).ok().and_then(|j| mine.get(j)) {
                    self.pills.swap(from, next);
                    return Ok(next);
                }
                let s = SIDES.iter().position(|s| *s == side).unwrap_or(0) as i32 + by;
                let Some(next) = usize::try_from(s).ok().and_then(|s| SIDES.get(s)) else {
                    return Ok(from); // already at the screen's edge
                };
                // The next side's near end: its first pill going right, its last going left.
                let first = self.pills.iter().position(|p| p.side == *next);
                (next.to_string(), if by > 0 { first } else { None })
            }
        };
        let target = before.map(|i| (self.pills[i].clone(), i));
        let mut pill = self.pills.remove(from);
        pill.side = side.clone();
        let to = match target {
            // The target slid down one if the moved pill was above it.
            Some((_, i)) => i - usize::from(from < i),
            None => match self.pills.iter().rposition(|p| p.side == side) {
                Some(last) => last + 1,
                // An empty side: after the sides before it, so the file reads left to right.
                None => {
                    let later = &SIDES[SIDES.iter().position(|s| *s == side).unwrap_or(0) + 1..];
                    self.pills
                        .iter()
                        .position(|p| later.contains(&p.side.as_str()))
                        .unwrap_or(self.pills.len())
                }
            },
        };
        self.pills.insert(to, pill);
        Ok(to)
    }

    fn movable(&self, p: usize) -> Result<&Pill> {
        let Some(pill) = self.pills.get(p) else {
            bail!("there is no pill {p}");
        };
        if pill.items.iter().any(|i| i.kind == "palette") {
            bail!("Palette's theme pill can't be split, merged or copied");
        }
        Ok(pill)
    }

    /// Split pill `p` before its item `at` (S7): the items from `at` on make a new pill just
    /// right of it, with the same look. Returns the new pill's index.
    pub fn split_pill(&mut self, p: usize, at: usize) -> Result<usize> {
        let pill = self.movable(p)?;
        if at == 0 || at >= pill.items.len() {
            bail!(
                "split between two items: 1 to {}",
                pill.items.len().saturating_sub(1)
            );
        }
        let mut new = pill.clone();
        new.items = self.pills[p].items.split_off(at);
        self.pills.insert(p + 1, new);
        Ok(p + 1)
    }

    /// Merge pill `p` with the next pill on its side (the one right of it on screen); the
    /// merged pill keeps `p`'s look. Returns its index.
    pub fn merge_pill(&mut self, p: usize) -> Result<usize> {
        self.movable(p)?;
        let side = self.pills[p].side.clone();
        let Some(next) = (p + 1..self.pills.len()).find(|&i| self.pills[i].side == side) else {
            bail!("no pill right of it on the same side to merge with");
        };
        self.movable(next)?;
        let items = self.pills.remove(next).items;
        self.pills[p].items.extend(items);
        Ok(p)
    }

    /// A copy of pill `p` just right of it, without the items the bar can hold only once
    /// (the clock, the workspaces…). Returns the copy's index.
    pub fn duplicate_pill(&mut self, p: usize) -> Result<usize> {
        let mut new = self.movable(p)?.clone();
        new.items
            .retain(|i| find_kind(&i.kind).is_some_and(|k| !k.single));
        if new.items.is_empty() {
            bail!("everything in this pill can be in the bar only once, so a copy would be empty");
        }
        self.pills.insert(p + 1, new);
        Ok(p + 1)
    }
}

impl Bar {
    pub fn validate(&self) -> Result<()> {
        if !["top", "bottom"].contains(&self.position.as_str()) {
            bail!("position is top or bottom");
        }
        for (app, glyph) in &self.icons {
            check_icon(app, glyph)?;
        }
        let n = |v: u32, lo: u32, hi: u32, what: &str| -> Result<()> {
            if (lo..=hi).contains(&v) {
                Ok(())
            } else {
                bail!("{what} is {lo} to {hi}, not {v}")
            }
        };
        n(self.height, 10, 80, "height")?; // small bars are your call: the Bar page warns (owner)
        n(self.margin, 0, 400, "margin")?;
        n(self.padding, 0, 100, "padding")?;
        n(self.corner_radius, 0, 40, "corner_radius")?;
        n(self.blur_radius, 0, 100, "blur_radius")?;
        n(self.pill.height, 6, 80, "pill height")?;
        n(self.pill.corner_radius, 0, 40, "pill corner_radius")?;
        n(self.pill.border_width, 0, 6, "pill border_width")?;
        n(self.pill.gap, 0, 60, "pill gap")?;
        n(self.pill.edge, 0, 40, "pill edge")?;
        n(self.stats_every, 1, 600, "stats_every")?;
        if !(-40..=200).contains(&self.y_offset) {
            bail!("y_offset is -40 to 200");
        }
        if self.pill.opacity > 100 || self.background_opacity > 100 {
            bail!("opacity is 0 to 100");
        }
        if !(6.0..=40.0).contains(&self.icon_size) || !(6.0..=40.0).contains(&self.label_size) {
            bail!("font sizes are 6 to 40");
        }
        let plain = |s: &str| !s.is_empty() && !s.contains(['"', '\'', '\\', '$', '`', '\n', ':']);
        if !plain(&self.font) || !plain(&self.font_style) {
            bail!("font and font_style: a name, without quotes, $ or :");
        }
        for key in [
            &self.icon_colour,
            &self.text_colour,
            &self.pill.colour,
            &self.pill.border_colour,
        ] {
            colour(key)?;
        }
        if !self.background.is_empty() {
            colour(&self.background)?;
        }
        let mut seen = Vec::new();
        for pill in &self.pills {
            if !SIDES.contains(&pill.side.as_str()) {
                bail!("a pill's side is left, center or right");
            }
            if !pill.colour.is_empty() && pill.colour != "none" {
                colour(&pill.colour)?;
            }
            if pill.opacity.is_some_and(|o| o > 100) {
                bail!("a pill's opacity is 0 to 100");
            }
            for item in &pill.items {
                let Some(kind) = find_kind(&item.kind) else {
                    bail!("`{}` is not something a pill can hold", item.kind);
                };
                if kind.single {
                    if seen.contains(&kind.id) {
                        bail!("{} is in the bar twice", kind.name);
                    }
                    seen.push(kind.id);
                }
                if kind.id == "palette" && pill.items.len() > 1 {
                    bail!("the Palette item has a pill of its own");
                }
                validate_item(item, kind)?;
            }
        }
        Ok(())
    }
}

fn validate_item(item: &Item, kind: &Kind) -> Result<()> {
    if item.click.contains('\n') || item.icon.contains('\n') || item.label.contains('\n') {
        bail!("{}: one line only", kind.name);
    }
    if item.every > 86_400 {
        bail!("{}: refresh at most once a day (86400 s)", kind.name);
    }
    match kind.id {
        "clock" => {
            let ok = !item.format.is_empty()
                && item
                    .format
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || " %:/-.,".contains(c));
            if !ok {
                bail!("clock format: letters, digits, spaces and % : / - . , only");
            }
        }
        "weather" => {
            if !["C", "F"].contains(&item.units.as_str()) {
                bail!("weather units are C or F");
            }
            if !item
                .location
                .chars()
                .all(|c| c.is_alphanumeric() || " ,.-".contains(c))
            {
                bail!("weather location: a city name or lat,lon");
            }
        }
        "disk" if !(1..=100).contains(&item.disk_at) => bail!("disk_at is 1 to 100"),
        "nowplaying" if !(6..=80).contains(&item.width) => {
            bail!("now playing's width is 6 to 80 characters")
        }
        "nowplaying" if !["toggle", "open"].contains(&item.action.as_str()) => {
            bail!("now playing's click is toggle (play/pause) or open (the player)")
        }
        "gpu" if !["busy", "raw"].contains(&item.gpu.as_str()) => {
            bail!("gpu is busy or raw (not `{}`)", item.gpu)
        }
        "link" if item.icon.is_empty() && item.label.is_empty() => {
            bail!("a link needs an icon or a label")
        }
        "link" if item.click.is_empty() || item.click == "none" => {
            bail!("a link needs something to open")
        }
        "script" if item.path.trim().is_empty() || item.path.contains('\n') => {
            bail!("your script: the path to it")
        }
        _ => {}
    }
    Ok(())
}

fn colour(key: &str) -> Result<()> {
    crate::theme::colour_key(key)
        .map(|_| ())
        .ok_or_else(|| anyhow::anyhow!("`{key}` is not a theme colour"))
}

/// `${C_selection/#0xff/0xf0}`: a theme colour with its alpha.
fn argb(key: &str, opacity: u8) -> String {
    let alpha = ((u32::from(opacity) * 255 + 50) / 100) as u8;
    format!("${{C_{key}/#0xff/0x{alpha:02x}}}")
}

/// The beat lines' lowest and highest heights (points) for this bar's pills: the helper's
/// `--min` and `--max`.
pub fn beat_range(bar: &Bar) -> (u32, u32) {
    (2, bar.pill.height.saturating_sub(12).max(6))
}

/// Whether the bar wants beat lines now (a now-playing item with them on, and not Performance
/// mode's "Slower bar"): Palette's app runs the helper then.
pub fn wants_beats(paths: &Paths, bar: &Bar) -> bool {
    let slower = crate::settings::load(paths)
        .map(|s| s.performance.stats_now())
        .unwrap_or(false);
    !slower
        && bar
            .pills
            .iter()
            .flat_map(|p| &p.items)
            .any(|i| i.kind == "nowplaying" && i.beats)
}

pub fn scripts_dir(paths: &Paths) -> PathBuf {
    paths.root().join("bar")
}

/// The pill scripts follow the app's version (ROADMAP 28): after an update the scripts in
/// `~/.config/palette/bar/` would stay as the old version wrote them until the Bar page is
/// applied again. Rewrites each one whose text differs from the one built in (or that is
/// missing), only for a bar Palette manages; no SketchyBar reload, as every script is read
/// afresh when it next runs. Written by rename, so a running script (the stats feed) keeps the
/// old file it opened and the new one is used from its next start. Returns the names rewritten.
pub fn update_scripts(paths: &Paths) -> Result<Vec<&'static str>> {
    if !super::is_managed(paths, Tool::Bar) {
        return Ok(Vec::new());
    }
    let dir = scripts_dir(paths);
    let mut updated = Vec::new();
    for (name, text) in SCRIPTS {
        let path = dir.join(name);
        if fs::read_to_string(&path).ok().as_deref() != Some(text) {
            super::write_atomic(&path, text, true)?;
            updated.push(name);
        }
    }
    Ok(updated)
}

/// Workspace slots in the bar: more than AeroSpace's 9 shortcuts, plus a
/// few for extra displays and named workspaces.
pub const SLOTS: u8 = 12;

/// "Match the menu bar" (owner, 2026-11-08): the bar as tall as the menu bar strip (`menu`
/// points, from `system::menu_bar_height`), from the very top, pills as tall as the bar, and the
/// text about 0.42 of it (13.5 pt in a 32 pt strip; 11 to 15 pt). Still editable afterwards.
pub fn matched_to_menu_bar(bar: &Bar, menu: u32) -> Bar {
    let text = ((f64::from(menu) * 0.42 * 2.0).round() / 2.0).clamp(11.0, 15.0);
    let mut b = bar.clone();
    b.position = "top".into();
    b.height = menu;
    b.y_offset = 0;
    b.pill.height = menu;
    b.icon_size = text;
    b.label_size = text;
    b
}

/// The bar's colours as they are now, written before Palette's line (which overrides them), so
/// the bar keeps its text and pills when that line is gone (`palette init --remove`): unset, a
/// `$C_accent` is empty and SketchyBar draws it transparent (owner, 2026-11-08). Only the
/// colours this file uses; values from the active theme's `sketchybar.sh`.
fn with_fallback_colours(paths: &Paths, rc: String) -> String {
    let Ok(theme) = std::fs::read_to_string(paths.current().join("sketchybar.sh")) else {
        return rc;
    };
    let mut used: Vec<&str> = Vec::new();
    for (i, _) in rc.match_indices("C_") {
        let before = rc[..i].chars().last();
        if !matches!(before, Some('$') | Some('{')) {
            continue;
        }
        let name_len = rc[i..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rc.len() - i);
        let name = &rc[i..i + name_len];
        if !used.contains(&name) {
            used.push(name);
        }
    }
    let lines: Vec<String> = used
        .iter()
        .filter_map(|name| {
            let value = theme
                .lines()
                .find_map(|l| l.strip_prefix(&format!("export {name}=")))?;
            Some(format!("{name}={value}"))
        })
        .collect();
    if lines.is_empty() {
        return rc;
    }
    let block = format!(
        "# The theme's colours when this file was written: used only if Palette's line below is\n\
         # gone (palette init --remove), so the text and pills don't turn invisible.\n{}\n",
        lines.join("\n")
    );
    let path_line = "export PATH=\"$PATH:/opt/homebrew/bin\"\n";
    rc.replacen(path_line, &format!("{path_line}{block}"), 1)
}

/// sketchybarrc and the pill scripts.
pub fn render(paths: &Paths, bar: &Bar) -> Result<Vec<File>> {
    // Performance mode changes what is written, never the saved settings.
    let saved = crate::settings::load(paths).unwrap_or_default();
    let perf = saved.performance;
    // The weather pill's place is Schedule's (owner, 2026-11-03), so one place changes both;
    // without one there, the item's own (older bars have "Singapore" there), else the city
    // the Mac's time zone suggests (Open-Meteo can't guess from the internet address).
    let place = saved
        .schedule
        .location()
        .ok()
        .map(|(lat, lon, _)| format!("{lat:.4},{lon:.4}"));
    let zone_city = crate::schedule::suggest_city().map(|c| format!("{:.4},{:.4}", c.lat, c.lon));
    let mut view = bar.clone();
    if perf.bar_now() {
        // No blur, and nothing see-through (the pills' and the bar's colours fully opaque).
        view.blur_radius = 0;
        view.pill.opacity = 100;
        view.background_opacity = 100;
    }
    if perf.stats_now() {
        use crate::settings::{
            PERFORMANCE_MAX_EVERY, PERFORMANCE_PILL_TIMES, PERFORMANCE_STATS_TIMES,
        };
        view.stats_every = (view.stats_every * PERFORMANCE_STATS_TIMES).min(PERFORMANCE_MAX_EVERY);
        for item in view.pills.iter_mut().flat_map(|p| p.items.iter_mut()) {
            let every = item.every();
            if every > 0 && item.kind != "clock" {
                item.every = (every * PERFORMANCE_PILL_TIMES).min(PERFORMANCE_MAX_EVERY);
            }
        }
        // Items marked "Hide in Performance mode" (S11) aren't written; a pill left empty goes.
        for pill in &mut view.pills {
            pill.items
                .retain(|i| !i.hide_in_performance || i.kind == "palette");
        }
    }
    let bar = &view;
    let dir = scripts_dir(paths);
    let script = |name: &str| format!("\"{}\"", paths.for_shell(&dir.join(name)));
    let mut out = String::new();
    let mut line = |s: String| {
        out.push_str(&s);
        out.push('\n');
    };
    line("#!/bin/bash".into());
    line("# SketchyBar, built by Palette from ~/.config/palette/desktop/bar.toml.".into());
    line(
        "# Change it in Palette (Settings → Bar) or with `palette desktop bar`; edits made".into(),
    );
    line("# here are replaced on the next change. Your own file comes back with".into());
    line("# `palette desktop bar release`.".into());
    line(String::new());
    line("export PATH=\"$PATH:/opt/homebrew/bin\"".into());
    if let Some(block) = crate::wiring::palette_block(paths, "sketchybar") {
        line(block);
    }
    line(format!("FONT={}", sh_quote(&bar.font)));
    line(String::new());
    let background = if bar.background.is_empty() {
        "0x00000000".to_string()
    } else {
        argb(&bar.background, bar.background_opacity)
    };
    line(format!(
        "sketchybar --bar position={} display=all height={} y_offset={} margin={} corner_radius={} blur_radius={} \\\n  color={background} padding_left={p} padding_right={p} shadow=off",
        bar.position,
        bar.height,
        bar.y_offset,
        bar.margin,
        bar.corner_radius,
        bar.blur_radius,
        p = bar.padding
    ));
    line(format!(
        "sketchybar --default icon.font=\"$FONT:{s}:{:.1}\" label.font=\"$FONT:{s}:{:.1}\" \\\n  icon.color=$C_{t} label.color=$C_{t} padding_left=3 padding_right=3 \\\n  icon.padding_left=5 icon.padding_right=3 label.padding_left=0 label.padding_right=5",
        bar.icon_size,
        bar.label_size,
        s = bar.font_style,
        t = bar.text_colour
    ));
    line(String::new());
    let p = &bar.pill;
    line(format!(
        "pill() {{ sketchybar --add bracket \"$1\" \"${{@:2}}\" --set \"$1\" background.color={} \\\n  background.corner_radius={} background.height={} background.border_width={} background.border_color=$C_{}; }}",
        argb(&p.colour, p.opacity),
        p.corner_radius,
        p.height,
        p.border_width,
        p.border_colour
    ));
    line(format!(
        "gap() {{ sketchybar --add item \"$1\" \"$2\" --set \"$1\" width={} padding_left=0 padding_right=0 icon.drawing=off label.drawing=off; }}",
        p.gap
    ));
    let mut events = Vec::new();
    let mut counters: BTreeMap<&str, usize> = BTreeMap::new();
    let mut stats = Vec::new();
    let mut stats_side = None;
    let mut gpu_mode = "busy";
    // Where Palette's theme pill goes: after the item added just before it
    // on its side, or before the next pill's gap when it comes first.
    let mut want_palette: Option<(&str, Anchor)> = None;

    for side in SIDES {
        // Right-hand items are added right to left.
        let mut pills: Vec<&Pill> = bar.pills.iter().filter(|p| p.side == side).collect();
        if side == "right" {
            pills.reverse();
        }
        let mut first = true;
        let mut last_added: Option<String> = None;
        for (n, pill) in pills.iter().enumerate() {
            if pill.items.iter().any(|i| i.kind == "palette") {
                // Added by Palette's own item file, below, then moved here.
                let anchor = match &last_added {
                    Some(prev) => Anchor::After(prev.clone()),
                    None => Anchor::First,
                };
                want_palette = Some((side, anchor));
                first = false;
                continue;
            }
            if pill.items.is_empty() {
                continue;
            }
            if !first {
                let gap = format!("gap.{side}.{n}");
                line(format!("gap {gap} {side}"));
                if let Some((s, a @ Anchor::First)) = &mut want_palette
                    && *s == side
                {
                    *a = Anchor::Before(gap.clone());
                }
                last_added = Some(gap);
            }
            first = false;
            line(String::new());
            let mut members = Vec::new();
            let mut items: Vec<&Item> = pill.items.iter().collect();
            if side == "right" {
                items.reverse();
            }
            let count = items.len();
            for (i, item) in items.into_iter().enumerate() {
                // The pill's visual ends: outermost left and right.
                let (left_end, right_end) = if side == "right" {
                    (i + 1 == count, i == 0)
                } else {
                    (i == 0, i + 1 == count)
                };
                let k = item.kind();
                let idx = counters.entry(k.id).or_insert(0);
                *idx += 1;
                let name = if k.single {
                    k.id.to_string()
                } else {
                    format!("{}.{}", k.id, idx)
                };
                let click = item
                    .click()
                    .map(|c| format!(" click_script={}", sh_quote(&c)))
                    .unwrap_or_default();
                let edges = format!(
                    "{}{}",
                    if left_end {
                        format!(" icon.padding_left={}", p.edge)
                    } else {
                        String::new()
                    },
                    if right_end {
                        format!(" label.padding_right={}", p.edge)
                    } else {
                        String::new()
                    }
                );
                match k.id {
                    "workspaces" => {
                        // Slots, filled by spaces.sh with whatever workspaces
                        // AeroSpace has (kept ones, ones with windows, a new
                        // display's): items can't be added into a pill later.
                        let mut ids: Vec<u8> = (1..=SLOTS).collect();
                        if side == "right" {
                            ids.reverse();
                        }
                        let list = ids.iter().map(u8::to_string).collect::<Vec<_>>().join(" ");
                        events.push("aerospace_workspace_change");
                        line(format!("for slot in {list}; do"));
                        line(format!(
                            "  sketchybar --add item space.$slot {side} --set space.$slot drawing=off icon.color=$GREY \\\n    icon.padding_left=8 icon.padding_right=8 label.font=\"sketchybar-app-font:Regular:14.0\" label.y_offset=-1 \\\n    label.padding_right=8 label.drawing=off background.color=$C_{ic} background.corner_radius=8 \\\n    background.height={h} background.drawing=off padding_left=1 padding_right=1",
                            ic = bar.icon_colour,
                            h = p.height.saturating_sub(8).max(10)
                        ));
                        line("done".into());
                        let env = format!(
                            "SLOTS={SLOTS} SHOW_APPS={} HIDE_EMPTY={} ALL_DISPLAYS={} bash {}",
                            u8::from(item.show_apps),
                            u8::from(item.hide_empty),
                            u8::from(item.all_displays),
                            script("spaces.sh")
                        );
                        line(format!(
                            "sketchybar --add item space_dispatcher {side} --set space_dispatcher drawing=off update_freq={} \\\n  script={} --subscribe space_dispatcher aerospace_workspace_change front_app_switched display_change",
                            item.every(),
                            sh_quote(&env)
                        ));
                        members.extend((1..=SLOTS).map(|i| format!("space.{i}")));
                        last_added = Some("space_dispatcher".into());
                        continue;
                    }
                    s if STATS.contains(&s) => {
                        stats.push((s, item.disk_at));
                        if s == "gpu" {
                            gpu_mode = item.gpu.as_str();
                        }
                        stats_side.get_or_insert(side);
                        let hidden = if ["disk", "vpn"].contains(&s) {
                            " drawing=off"
                        } else {
                            ""
                        };
                        let extra = if s == "net" {
                            " label.width=38 label.align=right"
                        } else {
                            ""
                        };
                        line(format!(
                            "sketchybar --add item {name} {side} --set {name} icon={} icon.color=$C_{} label=\"--\" \\\n  padding_left=1 padding_right=1 icon.padding_left=4 icon.padding_right=2 label.padding_right=4{hidden}{extra}{click}{edges}",
                            sh_quote(glyph(s)),
                            bar.icon_colour
                        ));
                    }
                    "clock" => {
                        let cmd = format!(
                            "sketchybar --set $NAME label=\"$(date \"+{}\")\"",
                            item.format
                        );
                        line(format!(
                            "sketchybar --add item {name} {side} --set {name} icon={} icon.color=$C_{} update_freq={} \\\n  script={}{click}{edges}",
                            sh_quote(glyph("clock")),
                            bar.icon_colour,
                            item.every(),
                            sh_quote(&cmd)
                        ));
                    }
                    "battery" | "volume" | "brightness" | "weather" => {
                        let env = if k.id == "weather" {
                            let at = place
                                .as_deref()
                                .or(Some(item.location.as_str()).filter(|l| !l.trim().is_empty()))
                                .or(zone_city.as_deref())
                                .unwrap_or("");
                            format!("LOCATION=\"{at}\" UNITS={} ", item.units)
                        } else {
                            String::new()
                        };
                        let cmd = format!(
                            "{env}ICON_KEY={} bash {} {}",
                            bar.icon_colour,
                            script("widgets.sh"),
                            k.id
                        );
                        let freq = if k.every > 0 {
                            format!(" update_freq={}", item.every())
                        } else {
                            String::new()
                        };
                        let subscribe = match k.id {
                            "battery" => {
                                format!(" \\\n  --subscribe {name} power_source_change system_woke")
                            }
                            "volume" => format!(" \\\n  --subscribe {name} volume_change"),
                            "brightness" => format!(" \\\n  --subscribe {name} brightness_change"),
                            _ => format!(" \\\n  --subscribe {name} system_woke"),
                        };
                        line(format!(
                            "sketchybar --add item {name} {side} --set {name} label=\"--\"{freq} \\\n  script={}{click}{edges}{subscribe}",
                            sh_quote(&cmd)
                        ));
                    }
                    "habits" => {
                        events.push("habit_tracker_update");
                        let cmd =
                            format!("ICON_KEY={} bash {}", bar.icon_colour, script("habits.sh"));
                        line(format!(
                            "sketchybar --add item {name} {side} --set {name} label=\"--\" update_freq={} script={}{click}{edges} \\\n  --subscribe {name} habit_tracker_update system_woke",
                            item.every(),
                            sh_quote(&cmd)
                        ));
                    }
                    "dispatch" => {
                        events.push("dispatch_update");
                        let cmd = format!(
                            "ICON_KEY={} bash {}",
                            bar.icon_colour,
                            script("dispatch.sh")
                        );
                        line(format!(
                            "sketchybar --add item {name} {side} --set {name} label=\"--\" update_freq={} script={}{click}{edges} \\\n  --subscribe {name} dispatch_update system_woke",
                            item.every(),
                            sh_quote(&cmd)
                        ));
                    }
                    "front_app" => {
                        line(format!(
                            "sketchybar --add item {name} {side} --set {name} icon.font=\"sketchybar-app-font:Regular:14.0\" icon.color=$C_{} \\\n  label.max_chars=16 script={}{click}{edges} --subscribe {name} front_app_switched",
                            bar.icon_colour,
                            sh_quote(&format!("bash {}", script("front_app.sh")))
                        ));
                    }
                    "nowplaying" => {
                        // Beat lines (Palette's helper sets their heights) left of the title on screen;
                        // the feeder shows both only while something plays. No beat lines while
                        // Performance mode's "Slower bar" is on.
                        let beats = item.beats && !perf.stats_now();
                        let (low, _) = beat_range(bar);
                        let ic = &bar.icon_colour;
                        let click = match item.click.as_str() {
                            "none" => String::new(),
                            "" => {
                                // Play/pause needs the app's library (perl loads it).
                                let media = crate::system::palette_app_file(
                                    "Contents/Frameworks/libpalette-media.dylib",
                                )
                                .map(|p| format!("MEDIA=\"{}\" ", paths.for_shell(&p)))
                                .unwrap_or_default();
                                format!(
                                    " click_script={}",
                                    sh_quote(&format!(
                                        "{media}bash {} {}",
                                        script("nowplaying.sh"),
                                        item.action
                                    ))
                                )
                            }
                            mine => format!(" click_script={}", sh_quote(mine)),
                        };
                        let cmd = format!(
                            "WIDTH={} SCROLL={} BEATS={} bash {}",
                            item.width,
                            u8::from(item.scroll),
                            if beats { BEAT_LINES } else { 0 },
                            script("nowplaying.sh")
                        );
                        let title_left = if left_end && !beats {
                            format!(" icon.padding_left={}", p.edge)
                        } else {
                            String::new()
                        };
                        let title_right = if right_end {
                            format!(" label.padding_right={}", p.edge)
                        } else {
                            String::new()
                        };
                        let title = format!(
                            "sketchybar --add item nowplaying {side} --set nowplaying drawing=off icon={} icon.color=$C_{ic} \\\n  label.max_chars={} scroll_texts={} update_freq=60 script={}{click}{title_left}{title_right} \\\n  --subscribe nowplaying system_woke",
                            sh_quote(glyph("nowplaying")),
                            item.width,
                            if item.scroll { "on" } else { "off" },
                            sh_quote(&cmd)
                        );
                        let beat = |b: u8| {
                            let left = if b == 1 && left_end { p.edge } else { 1 };
                            format!(
                                "sketchybar --add item beat.{b} {side} --set beat.{b} drawing=off width=3 icon.drawing=off label.drawing=off \\\n  background.drawing=on background.color=$C_{ic} background.height={low} background.corner_radius=1 padding_left={left} padding_right=1{click}"
                            )
                        };
                        let mut order: Vec<(String, String)> = if beats {
                            (1..=BEAT_LINES)
                                .map(|b| (format!("beat.{b}"), beat(b)))
                                .collect()
                        } else {
                            Vec::new()
                        };
                        order.push(("nowplaying".into(), title));
                        if side == "right" {
                            order.reverse(); // added right to left
                        }
                        for (n, text) in order {
                            line(text);
                            members.push(n.clone());
                            last_added = Some(n);
                        }
                        continue;
                    }
                    "link" => {
                        line(format!(
                            "sketchybar --add item {name} {side} --set {name} icon={} icon.color=$C_{} label={}{}{click}{edges}",
                            sh_quote(if item.icon.is_empty() {
                                glyph("link")
                            } else {
                                &item.icon
                            }),
                            bar.icon_colour,
                            sh_quote(&item.label),
                            if item.label.is_empty() {
                                " label.drawing=off"
                            } else {
                                ""
                            },
                        ));
                    }
                    "script" => {
                        let cmd = format!("bash {}", sh_quote(&item.path));
                        line(format!(
                            "sketchybar --add item {name} {side} --set {name} icon={} icon.color=$C_{} label={} update_freq={} \\\n  script={}{click}{edges}",
                            sh_quote(&item.icon),
                            bar.icon_colour,
                            sh_quote(if item.label.is_empty() {
                                "--"
                            } else {
                                &item.label
                            }),
                            item.every(),
                            sh_quote(&cmd)
                        ));
                    }
                    _ => {}
                }
                last_added = Some(name.clone());
                members.push(name);
            }
            line(format!("pill pill.{side}.{n} {}", members.join(" ")));
            if pill.own_look() {
                line(match pill.background(p) {
                    Some(c) => format!("sketchybar --set pill.{side}.{n} background.color={c}"),
                    None => format!("sketchybar --set pill.{side}.{n} background.drawing=off"),
                });
            }
        }
    }

    if let Some(side) = stats_side {
        let names: Vec<&str> = stats.iter().map(|(s, _)| *s).collect();
        let disk_at = stats
            .iter()
            .find(|(s, _)| *s == "disk")
            .map_or(80, |(_, d)| *d);
        let cmd = format!(
            "STATS=\"{}\" DISK_AT={disk_at} EVERY={} GPU={gpu_mode} ICON_KEY={} bash {}",
            names.join(" "),
            bar.stats_every,
            bar.icon_colour,
            script("stats.sh")
        );
        line(String::new());
        line("# One script updates all the stats.".into());
        line(format!(
            "sketchybar --add item stats_dispatcher {side} --set stats_dispatcher drawing=off update_freq={} script={}",
            bar.stats_every,
            sh_quote(&cmd)
        ));
    }
    let mut events_block = String::new();
    events.sort();
    events.dedup();
    for e in events {
        events_block.push_str(&format!("sketchybar --add event {e}\n"));
    }

    line(String::new());
    // Palette's theme item, from its own generated file (counts as wired).
    if let Some(block) = crate::wiring::palette_block(paths, "sketchybar-item") {
        line(block);
    }
    if let Some((side, anchor)) = &want_palette {
        let own = bar
            .pills
            .iter()
            .find(|p| p.items.iter().any(|i| i.kind == "palette"));
        let background =
            match own.map_or_else(|| Some(argb(&p.colour, p.opacity)), |o| o.background(p)) {
                Some(c) => format!("background.color={c}"),
                None => "background.drawing=off".into(),
            };
        let (gap, mv) = match anchor {
            Anchor::After(prev) => (
                p.gap,
                format!(" \\\n  --move palette_gap after {prev} --move palette after palette_gap"),
            ),
            Anchor::Before(next) => (0, format!(" \\\n  --move palette before {next}")),
            Anchor::First => (0, String::new()),
        };
        line(format!(
            "sketchybar --set palette_pill {background} background.corner_radius={} background.height={} \\\n  background.border_width={} background.border_color=$C_{} --set palette_gap width={gap} position={side} \\\n  --set palette position={side} icon.padding_left={e} label.padding_right={e}{mv} 2>/dev/null",
            p.corner_radius,
            p.height,
            p.border_width,
            p.border_colour,
            e = p.edge
        ));
    } else {
        line(
            "sketchybar --remove palette_pill --remove palette --remove palette_gap 2>/dev/null"
                .into(),
        );
    }
    line("sketchybar --update".into());

    // Events go before the items that subscribe to them.
    let marker = "\npill() {";
    let rc = out.replacen(marker, &format!("\n{events_block}{}", &marker[1..]), 1);
    let rc = with_fallback_colours(paths, rc);

    let mut files = vec![File {
        path: Tool::Bar.files(paths).remove(0),
        text: rc,
        executable: true,
    }];
    for (name, text) in SCRIPTS {
        files.push(File {
            path: dir.join(name),
            text: text.into(),
            executable: true,
        });
    }
    files.push(File {
        path: dir.join(ICONS_FILE),
        text: icons_script(&bar.icons),
        executable: false,
    });
    Ok(files)
}

/// Your icons, sourced by `spaces.sh` and `front_app.sh` (IDEAS 37b). Its own file, not in
/// the scripts, so the scripts stay the same for everyone (ROADMAP 28 keeps them current).
pub const ICONS_FILE: &str = "icons.sh";

/// `user_icon <app>` sets `icon_result` when you chose an icon for that app.
fn icons_script(icons: &BTreeMap<String, String>) -> String {
    let mut out = String::from(
        "# Palette: your app icons (Bar page → App icons, `palette desktop bar icon`). Generated:\n\
         # edits are overwritten. Sourced by spaces.sh and front_app.sh; wins over the icon map.\n\
         user_icon() {\n  case \"$1\" in\n",
    );
    for (app, glyph) in icons {
        out.push_str(&format!(
            "    {}) icon_result={} ;;\n",
            sh_quote(app),
            sh_quote(glyph)
        ));
    }
    out.push_str("  esac\n}\n");
    out
}

/// An app name and its icon must be safe inside the generated shell file and short enough
/// to fit a workspace pill: a glyph name (`:ollama:`) or up to 4 characters.
fn check_icon(app: &str, glyph: &str) -> Result<()> {
    if app.trim().is_empty() || app.chars().any(char::is_control) || app.chars().count() > 60 {
        bail!("an app name is 1 to 60 characters on one line");
    }
    let is_name = glyph.len() > 2
        && glyph.starts_with(':')
        && glyph.ends_with(':')
        && glyph[1..glyph.len() - 1]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    let short = !glyph.is_empty()
        && glyph.chars().count() <= 4
        && !glyph
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || "$`\\'\"".contains(c));
    if !(is_name || short) {
        bail!(
            "an icon is a glyph name like :ollama: (from the icon font) or up to 4 characters \
             (no spaces, quotes, $, ` or \\)"
        );
    }
    Ok(())
}

/// Choose `app`'s icon on the bar, or (`None`) give it back to the icon map. Saved in
/// `bar.toml` and applied like any bar change. The app name is AeroSpace's (`palette desktop
/// bar icons` lists the apps open now).
pub fn set_icon(paths: &Paths, app: &str, glyph: Option<&str>) -> Result<super::Applied> {
    if !super::is_managed(paths, Tool::Bar) {
        bail!("app icons are for a bar Palette manages: palette desktop bar manage");
    }
    let mut bar: Bar = super::get(paths, Tool::Bar)?.try_into()?;
    match glyph.map(str::trim) {
        Some(g) => {
            check_icon(app, g)?;
            bar.icons.insert(app.to_string(), g.to_string());
        }
        None => {
            bar.icons.remove(app);
        }
    }
    super::set(paths, Tool::Bar, toml::Value::try_from(&bar)?)
}

/// The glyph names the icon font's map knows (`:ollama:` …), sorted, plus Palette's own
/// fallbacks; empty when the map isn't installed (`~/.config/sketchybar/helpers/icon_map.sh`).
pub fn glyphs(paths: &Paths) -> Vec<String> {
    let map = paths.config_home().join("sketchybar/helpers/icon_map.sh");
    let Ok(text) = fs::read_to_string(map) else {
        return Vec::new();
    };
    let mut out: Vec<String> = text
        .split("icon_result=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .filter(|g| g.len() > 2 && g.starts_with(':') && g.ends_with(':'))
        .map(String::from)
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Apps with a window now (AeroSpace's names), to choose icons for. Empty without AeroSpace.
pub fn open_apps(paths: &Paths) -> Vec<String> {
    if paths.is_fake() {
        return Vec::new();
    }
    let mut apps: Vec<String> = system::run(
        "aerospace",
        &["list-windows", "--all", "--format", "%{app-name}"],
    )
    .unwrap_or_default()
    .lines()
    .map(|l| l.trim().to_string())
    .filter(|l| !l.is_empty())
    .collect();
    apps.sort();
    apps.dedup();
    apps
}

/// Apps installed on this Mac, by the name AeroSpace shows (the `.app` folder's name), for
/// giving an icon to an app that isn't open now. Empty in a fake home.
pub fn installed_apps(paths: &Paths) -> Vec<String> {
    if paths.is_fake() {
        return Vec::new();
    }
    let mut dirs = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/Applications/Utilities"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
    ];
    dirs.push(paths.home().join("Applications"));
    let mut apps: Vec<String> = dirs
        .iter()
        .filter_map(|d| fs::read_dir(d).ok())
        .flatten()
        .flatten()
        .filter_map(|e| {
            e.file_name()
                .to_str()?
                .strip_suffix(".app")
                .map(String::from)
        })
        .collect();
    apps.sort_by_key(|a| a.to_lowercase());
    apps.dedup();
    apps
}

/// What the icon font's map gives each app (`:zed:`, or its catch-all `:default:`), asked of
/// the map itself in one `bash` run, so patterns like `"Adobe Bridge"*` match as on the bar.
/// Empty when the map isn't installed.
pub fn map_icons(paths: &Paths, apps: &[String]) -> BTreeMap<String, String> {
    let map = paths.config_home().join("sketchybar/helpers/icon_map.sh");
    if apps.is_empty() || !map.is_file() {
        return BTreeMap::new();
    }
    // Sourcing the map defines `__icon_map`. The arguments are cleared first: a sourced file
    // sees the caller's `"$@"`, and the map maps its arguments when run on its own.
    let script = r#"map="$1"; shift; apps=("$@"); set --; source "$map" >/dev/null 2>&1
for app in "${apps[@]}"; do icon_result=""; __icon_map "$app"; printf '%s\t%s\n' "$app" "$icon_result"; done"#;
    let map = map.to_string_lossy();
    let mut args = vec!["-c", script, "palette", map.as_ref()];
    args.extend(apps.iter().map(String::as_str));
    system::run("bash", &args)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .filter(|(_, g)| !g.is_empty())
        .map(|(a, g)| (a.to_string(), g.to_string()))
        .collect()
}

/// Where a pill can sit, in SketchyBar's words. Left and centre items are
/// added left to right, right items right to left.
const SIDES: [&str; 3] = ["left", "center", "right"];

/// Where the theme pill goes among the bar's items.
enum Anchor {
    After(String),
    Before(String),
    /// Alone on its side.
    First,
}

/// The font and style must exist, or SketchyBar quietly uses a system font
/// (no Nerd Font icons). Asked of macOS (a few ms); warnings, not refusals.
pub fn font_warnings(bar: &Bar) -> Vec<String> {
    let styles = system::fonts::styles(&bar.font);
    if styles.is_empty() {
        return vec![format!(
            "{} isn't installed: the bar uses a system font, without icons",
            bar.font
        )];
    }
    if !styles.contains(&bar.font_style) {
        return vec![format!(
            "{} has no {} style (it has {}): the bar uses a system font, without icons",
            bar.font,
            bar.font_style,
            styles.join(", ")
        )];
    }
    Vec::new()
}

/// A theme switch on a bar Palette manages: the colours set in place (no blank bar, no
/// restarted scripts). `None` when the bar isn't Palette's (or its file is gone): reload it.
pub fn recolour(paths: &Paths) -> Option<Outcome> {
    if !super::is_managed(paths, Tool::Bar) {
        return None;
    }
    let rc = fs::read_to_string(Tool::Bar.files(paths).remove(0)).ok()?;
    let item = fs::read_to_string(paths.current().join("sketchybar-item.sh")).ok();
    let script = recolour::script(&rc, item.as_deref());
    Some(match system::recolour_sketchybar(&script) {
        Ok(_) => Outcome::Done("colours set (no reload)".into()),
        Err(e) => Outcome::Failed(e),
    })
}

pub fn reload(_: &Paths) -> Outcome {
    if !system::is_running("sketchybar") {
        return Outcome::Skipped("not running".into());
    }
    match system::reload_sketchybar() {
        Ok(_) => Outcome::Done("reloaded".into()),
        Err(e) => Outcome::Failed(e),
    }
}

// ---------------------------------------------------------------- import

/// Start from your sketchybarrc: the bar's size, the pills (its brackets)
/// and the items in them that Palette has a pill for, with their clicks and
/// refresh times. Items Palette doesn't know are listed (they stay in the
/// backup). Nothing readable: the default bar.
pub fn import(paths: &Paths) -> (Bar, Vec<String>) {
    let mut bar = Bar::default();
    let mut warnings = Vec::new();
    let rc_path = Tool::Bar.files(paths).remove(0);
    let Ok(rc) = fs::read_to_string(&rc_path) else {
        return (bar, warnings);
    };
    let lines = statements(&rc);
    let vars: BTreeMap<String, String> = lines
        .iter()
        .filter_map(|s| {
            let (k, v) = s.split_once('=')?;
            (k.chars().all(|c| c.is_ascii_uppercase() || c == '_') && !k.is_empty())
                .then(|| (k.to_string(), unquote(v)))
        })
        .collect();

    // name -> (side, click, update_freq)
    let mut items: BTreeMap<String, (String, Option<String>, Option<u32>)> = BTreeMap::new();
    let mut brackets: Vec<(String, Vec<String>)> = Vec::new();
    for s in &lines {
        let words = words(s);
        if words.first().map(String::as_str) == Some("stat_item") && words.len() >= 4 {
            items.insert(
                words[1].clone(),
                ("left".into(), Some(resolve(&words[3], &vars)), None),
            );
            continue;
        }
        if words.first().map(String::as_str) != Some("sketchybar") {
            continue;
        }
        let mut i = 0;
        let mut current: Option<String> = None;
        while i < words.len() {
            match words[i].as_str() {
                "--add" if words.get(i + 1).map(String::as_str) == Some("item") => {
                    if let (Some(name), Some(side)) = (words.get(i + 2), words.get(i + 3)) {
                        items.insert(name.clone(), (side.clone(), None, None));
                        current = Some(name.clone());
                    }
                    i += 4;
                    continue;
                }
                "--add" if words.get(i + 1).map(String::as_str) == Some("bracket") => {
                    let name = words.get(i + 2).cloned().unwrap_or_default();
                    let members: Vec<String> = words[i + 3..]
                        .iter()
                        .take_while(|w| !w.starts_with("--"))
                        .cloned()
                        .collect();
                    brackets.push((name, members));
                    i += 3;
                    continue;
                }
                "--set" => current = words.get(i + 1).cloned(),
                "--bar" => current = Some("--bar".into()),
                w => {
                    if let (Some(name), Some((key, value))) = (&current, w.split_once('=')) {
                        let value = resolve(value, &vars);
                        if name == "--bar" {
                            match key {
                                "position" if ["top", "bottom"].contains(&value.as_str()) => {
                                    bar.position = value
                                }
                                "height" => bar.height = value.parse().unwrap_or(bar.height),
                                "y_offset" => bar.y_offset = value.parse().unwrap_or(bar.y_offset),
                                "margin" => bar.margin = value.parse().unwrap_or(bar.margin),
                                "padding_left" => {
                                    bar.padding = value.parse().unwrap_or(bar.padding)
                                }
                                "corner_radius" => bar.corner_radius = value.parse().unwrap_or(0),
                                "blur_radius" => bar.blur_radius = value.parse().unwrap_or(0),
                                _ => {}
                            }
                        } else if brackets.iter().any(|(b, _)| b == name) {
                            match key {
                                "background.corner_radius" => {
                                    bar.pill.corner_radius =
                                        value.parse().unwrap_or(bar.pill.corner_radius)
                                }
                                "background.height" => {
                                    bar.pill.height = value.parse().unwrap_or(bar.pill.height)
                                }
                                _ => {}
                            }
                        } else if let Some(entry) = items.get_mut(name) {
                            match key {
                                "click_script" => entry.1 = Some(value),
                                "update_freq" => entry.2 = value.parse().ok(),
                                _ => {}
                            }
                        }
                    }
                }
            }
            i += 1;
        }
    }
    if let Some(font) = vars.get("FONT") {
        bar.font = font.clone();
    }

    let kind_of = |name: &str| -> Option<&'static str> {
        if name.starts_with("space.") || name.contains("SPACES") {
            return Some("workspaces");
        }
        find_kind(name)
            .filter(|k| k.single && k.id != "palette")
            .map(|k| k.id)
    };
    let mut pills = Vec::new();
    let (mut left, mut right) = (Vec::new(), Vec::new());
    for (_, members) in &brackets {
        let mut pill_items: Vec<Item> = Vec::new();
        let mut side = "left".to_string();
        for m in members {
            let Some(kind) = kind_of(m) else {
                warnings.push(format!("`{m}` in your bar isn't one Palette knows: it's in the backup, add it again as a link or your script"));
                continue;
            };
            if pill_items.iter().any(|i| i.kind == kind) {
                continue;
            }
            let mut item = Item::of(kind);
            if let Some((s, click, every)) = items.get(m) {
                side = s.clone();
                let default_click = find_kind(kind).map_or("", |k| k.click);
                // `open -a "X"` and `open -a 'X'` are the same click.
                if let Some(c) = click
                    .as_ref()
                    .filter(|c| kind != "workspaces" && words(c) != words(default_click))
                {
                    item.click = c.clone();
                }
                if let Some(e) = every.filter(|e| {
                    Some(*e) != find_kind(kind).map(|k| k.every) && kind != "workspaces"
                }) {
                    item.every = e;
                }
            } else if kind == "workspaces" {
                side = items
                    .iter()
                    .find(|(n, _)| n.starts_with("space."))
                    .map_or(side, |(_, v)| v.0.clone());
            }
            pill_items.push(item);
        }
        if pill_items.is_empty() {
            continue;
        }
        if !SIDES.contains(&side.as_str()) {
            side = "left".into();
        }
        let pill = Pill {
            side: side.clone(),
            items: pill_items,
            ..Pill::default()
        };
        if side == "right" {
            right.push(pill)
        } else {
            left.push(pill)
        }
    }
    if left.is_empty() && right.is_empty() {
        return (bar, warnings);
    }
    // Right brackets were added right to left.
    right.reverse();
    let palette = rc.contains("sketchybar-item.sh");
    pills.extend(left);
    if palette {
        pills.push(Pill {
            side: "right".into(),
            items: vec![Item::of("palette")],
            ..Pill::default()
        });
    }
    pills.extend(right);
    bar.pills = pills;

    // The weather city lives in the owner's widgets.sh.
    if let Ok(widgets) = fs::read_to_string(rc_path.with_file_name("plugins/widgets.sh")) {
        for s in statements(&widgets) {
            if let Some(v) = s.strip_prefix("LOCATION=") {
                for item in bar
                    .pills
                    .iter_mut()
                    .flat_map(|p| p.items.iter_mut())
                    .filter(|i| i.kind == "weather")
                {
                    item.location = unquote(v.split('#').next().unwrap_or("").trim());
                }
            }
            if let Some(v) = s.strip_prefix("UNITS=") {
                let u = unquote(v.split('#').next().unwrap_or("").trim());
                for item in bar
                    .pills
                    .iter_mut()
                    .flat_map(|p| p.items.iter_mut())
                    .filter(|i| i.kind == "weather")
                {
                    item.units = if u == "F" { "F".into() } else { "C".into() };
                }
            }
        }
    }
    (bar, warnings)
}

/// Lines joined at `\` continuations, without comments or blank lines.
fn statements(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut acc = String::new();
    for line in text.lines() {
        let t = line.trim();
        if acc.is_empty() && (t.is_empty() || t.starts_with('#')) {
            continue;
        }
        if let Some(head) = t.strip_suffix('\\') {
            acc.push_str(head);
            acc.push(' ');
        } else {
            acc.push_str(t);
            out.push(std::mem::take(&mut acc));
        }
    }
    out
}

/// Shell-ish words: quotes group, and are kept off the word.
fn words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for c in s.chars() {
        match (quote, c) {
            (None, '#') if !started => break,
            (None, c) if c.is_whitespace() => {
                if started {
                    out.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            (None, '"' | '\'') => {
                quote = Some(c);
                started = true;
            }
            (Some(q), c) if c == q => quote = None,
            (_, c) => {
                cur.push(c);
                started = true;
            }
        }
    }
    if started {
        out.push(cur);
    }
    out
}

fn unquote(v: &str) -> String {
    words(v).join(" ")
}

/// `$OPEN_AM` → its value.
fn resolve(v: &str, vars: &BTreeMap<String, String>) -> String {
    match v.strip_prefix('$') {
        Some(name) if vars.contains_key(name.trim_matches(['{', '}'])) => {
            vars[name.trim_matches(['{', '}'])].clone()
        }
        _ => v.to_string(),
    }
}

#[cfg(test)]
mod tests;
