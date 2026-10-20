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

const SCRIPTS: [(&str, &str); 5] = [
    ("spaces.sh", include_str!("spaces.sh")),
    ("stats.sh", include_str!("stats.sh")),
    ("widgets.sh", include_str!("widgets.sh")),
    ("habits.sh", include_str!("habits.sh")),
    ("front_app.sh", include_str!("front_app.sh")),
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
        30,
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
            "Temperature and sky, from wttr.in (uses the network)",
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

const STATS: [&str; 8] = ["cpu", "gpu", "temp", "pwr", "mem", "disk", "vpn", "net"];

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
        "pwr" => "\u{f1e6}",
        "vpn" => "\u{f132}",
        "link" => "\u{f0c1}",
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
}

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
    /// `left` or `right` of the notch.
    pub side: String,
    /// Left to right, as you see them.
    pub items: Vec<Item>,
}

impl Default for Pill {
    fn default() -> Self {
        Self {
            side: "left".into(),
            items: Vec::new(),
        }
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
}

impl Default for Bar {
    /// The owner's bar: workspaces and stats on the left; habits and a
    /// status pill on the right, with Palette's theme pill.
    fn default() -> Self {
        let pill = |side: &str, kinds: &[&str]| Pill {
            side: side.into(),
            items: kinds.iter().map(|k| Item::of(k)).collect(),
        };
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
        }
    }
}

impl Bar {
    pub fn validate(&self) -> Result<()> {
        if !["top", "bottom"].contains(&self.position.as_str()) {
            bail!("position is top or bottom");
        }
        let n = |v: u32, lo: u32, hi: u32, what: &str| -> Result<()> {
            if (lo..=hi).contains(&v) {
                Ok(())
            } else {
                bail!("{what} is {lo} to {hi}, not {v}")
            }
        };
        n(self.height, 16, 80, "height")?;
        n(self.margin, 0, 400, "margin")?;
        n(self.padding, 0, 100, "padding")?;
        n(self.corner_radius, 0, 40, "corner_radius")?;
        n(self.blur_radius, 0, 100, "blur_radius")?;
        n(self.pill.height, 10, 80, "pill height")?;
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
            if !["left", "right"].contains(&pill.side.as_str()) {
                bail!("a pill's side is left or right");
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

pub fn scripts_dir(paths: &Paths) -> PathBuf {
    paths.root().join("bar")
}

/// Workspace slots in the bar: more than AeroSpace's 9 shortcuts, plus a
/// few for extra displays and named workspaces.
pub const SLOTS: u8 = 12;

/// sketchybarrc and the pill scripts.
pub fn render(paths: &Paths, bar: &Bar) -> Result<Vec<File>> {
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
    // Where Palette's theme pill goes: after the item added just before it
    // on its side, or before the next pill's gap when it comes first.
    let mut want_palette: Option<(&str, Anchor)> = None;

    for side in ["left", "right"] {
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
                            format!("LOCATION=\"{}\" UNITS={} ", item.location, item.units)
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
                    "front_app" => {
                        line(format!(
                            "sketchybar --add item {name} {side} --set {name} icon.font=\"sketchybar-app-font:Regular:14.0\" icon.color=$C_{} \\\n  label.max_chars=16 script={}{click}{edges} --subscribe {name} front_app_switched",
                            bar.icon_colour,
                            sh_quote(&format!("bash {}", script("front_app.sh")))
                        ));
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
        }
    }

    if let Some(side) = stats_side {
        let names: Vec<&str> = stats.iter().map(|(s, _)| *s).collect();
        let disk_at = stats
            .iter()
            .find(|(s, _)| *s == "disk")
            .map_or(80, |(_, d)| *d);
        let cmd = format!(
            "STATS=\"{}\" DISK_AT={disk_at} ICON_KEY={} bash {}",
            names.join(" "),
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
        let (gap, mv) = match anchor {
            Anchor::After(prev) => (
                p.gap,
                format!(" \\\n  --move palette_gap after {prev} --move palette after palette_gap"),
            ),
            Anchor::Before(next) => (0, format!(" \\\n  --move palette before {next}")),
            Anchor::First => (0, String::new()),
        };
        line(format!(
            "sketchybar --set palette_pill background.color={} background.corner_radius={} background.height={} \\\n  background.border_width={} background.border_color=$C_{} --set palette_gap width={gap} position={side} \\\n  --set palette position={side} icon.padding_left={e} label.padding_right={e}{mv} 2>/dev/null",
            argb(&p.colour, p.opacity),
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
    Ok(files)
}

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

pub fn reload(_: &Paths) -> Outcome {
    if !system::is_running("sketchybar") {
        return Outcome::Skipped("not running".into());
    }
    match system::run("sketchybar", &["--reload"]) {
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
        let pill = Pill {
            side: side.clone(),
            items: pill_items,
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
