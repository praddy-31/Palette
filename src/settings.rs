//! `~/.config/palette/settings.toml`: preferences. The overlay hotkey and
//! which apps Palette themes (`[apps]`, all on unless
//! set to `false`). Plain TOML so it can be edited by hand; `palette hotkey`
//! and `palette apps` edit it from the command line.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::ErrorKind;

use anyhow::{Context, Result, bail};

use crate::paths::Paths;

/// alt-shift-t: free in the owner's AeroSpace config (alt-based bindings).
pub const DEFAULT_HOTKEY: &str = "alt+shift+t";

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// Normalised: lowercase modifiers in a fixed order, then the key (`alt+shift+t`).
    pub hotkey: String,
    /// Apps the user keeps on their own look (target names, `wallpaper`, `appearance`).
    pub off: BTreeSet<String>,
    /// The app's welcome tour has been seen (it opens on first launch until then).
    pub welcomed: bool,
    /// Theme name → the wallpaper (file name) it starts with (`palette bg default`).
    pub wallpapers: BTreeMap<String, String>,
    /// Obsidian: an accent ring around the active pane when the editor is split.
    pub obsidian_ring: bool,
    /// Obsidian: fade the other panes a little.
    pub obsidian_dim: bool,
    /// Zed: solid, or a see-through window, blurred or clear (`palette zed`).
    pub zed_glass: Glass,
    /// Zed: how see-through the glass is, 0 (solid) to 100 (fully clear).
    pub zed_strength: u8,
    /// Terminal windows (`palette glass`): app name → its glass. Apps not
    /// listed keep their own setting ([`WindowGlass::default`]).
    pub glass: BTreeMap<String, WindowGlass>,
    /// Day and night themes on a schedule (`palette schedule`).
    pub schedule: crate::schedule::Schedule,
    /// Favourite themes, in the order added (at most ten; `palette fav`).
    pub favourites: Vec<String>,
    /// What plain `palette next`/`prev` and the bar pill step through.
    pub next_cycles: crate::favourites::Cycle,
    /// Performance mode (`palette performance`): solid windows and a plainer desktop.
    pub performance: Performance,
    /// The app's windows: all kept loaded, or made when opened (`palette ui --windows`).
    pub windows: AppWindows,
    /// What a click on the menu bar icon (and the bar's theme pill) opens (`palette ui --click`).
    pub menu_click: MenuClick,
    /// One line per switch in `palette.log` (`palette log`); off by default.
    pub log: bool,
    /// Windows on demand: also close Settings after 10 minutes without focus, even
    /// while it's open (on another workspace, behind other windows). Off by default.
    pub close_idle_settings: bool,
    /// Readability fixes applied when a theme is used (`palette check --level`).
    pub readability: crate::normalise::Level,
    /// Glass lift set by hand, per theme (F2, Settings → My themes): 0 = off, 1-100 = that
    /// much stronger than Auto (`glass::Lift`). A theme not listed lifts on Auto.
    pub glass_lift: BTreeMap<String, u8>,
    /// Scrolling on the bar's theme pill switches theme (on by default; `palette fav scroll`).
    pub bar_scroll: bool,
    /// Monochrome themes: spaced greys (on by default) and a theme-hue tint (off).
    pub monochrome: crate::monochrome::Mono,
    /// Themes shown exactly as their authors made them (`palette check <theme> --original on`).
    pub original_colours: BTreeSet<String>,
}

/// The app's windows: kept loaded so they open at once, or made when opened and
/// closed after a minute hidden, which frees their memory (each window is its own
/// WebKit process). `OnDemand` keeps the quick switcher loaded; `AllOnDemand` makes
/// it on demand too (the lightest, but the hotkey takes a second after a quiet minute).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppWindows {
    #[default]
    Ready,
    OnDemand,
    AllOnDemand,
}

impl AppWindows {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::OnDemand => "on-demand",
            Self::AllOnDemand => "all-on-demand",
        }
    }

    /// Are the picker, Settings and the toast made when opened?
    pub fn on_demand(self) -> bool {
        self != Self::Ready
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ready" | "keep" | "loaded" => Ok(Self::Ready),
            "on-demand" | "ondemand" | "demand" | "lazy" => Ok(Self::OnDemand),
            "all-on-demand" | "all" | "lightest" => Ok(Self::AllOnDemand),
            other => bail!("windows is ready, on-demand or all-on-demand (not `{other}`)"),
        }
    }
}

/// What a click on Palette's menu bar icon opens. A window no choice uses is never
/// made when windows load on demand. The other two stay in the right-click menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MenuClick {
    /// The native macOS menu (no window at all).
    Menu,
    /// The theme grid under the icon.
    #[default]
    Picker,
    /// The quick switcher in the middle of the screen.
    Quick,
}

impl MenuClick {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Menu => "menu",
            Self::Picker => "picker",
            Self::Quick => "quick-switcher",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "menu" | "native" | "native-menu" => Ok(Self::Menu),
            "picker" | "grid" => Ok(Self::Picker),
            "quick-switcher" | "quick" | "overlay" | "switcher" => Ok(Self::Quick),
            other => bail!("click is menu, picker or quick-switcher (not `{other}`)"),
        }
    }
}

/// Performance mode: one switch, and which parts it covers. It changes only what Palette
/// writes, never your saved choices, so turning it off puts every look back as it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Performance {
    pub on: bool,
    /// Every app with Palette glass solid: the terminals, Dispatch, EasyPlots, the Habit
    /// Tracker and Zed.
    pub glass: bool,
    /// SketchyBar without blur, its pills and background opaque (when Palette manages it).
    pub bar: bool,
    /// JankyBorders plain: no glow, normal resolution (hidpi off), when Palette manages it.
    pub borders: bool,
    /// The bar's stats pill every 30 s instead of every few seconds.
    pub stats: bool,
}

impl Default for Performance {
    fn default() -> Self {
        Self {
            on: false,
            glass: true,
            bar: true,
            borders: true,
            stats: true,
        }
    }
}

/// How often the stats pill updates in Performance mode, at most.
pub const PERFORMANCE_STATS_EVERY: u32 = 30;

impl Performance {
    pub fn glass_now(&self) -> bool {
        self.on && self.glass
    }
    pub fn bar_now(&self) -> bool {
        self.on && self.bar
    }
    pub fn borders_now(&self) -> bool {
        self.on && self.borders
    }
    pub fn stats_now(&self) -> bool {
        self.on && self.stats
    }

    /// The parts by name, for the CLI and the app (`glass`, `bar`, `borders`, `stats`).
    pub const PARTS: [&'static str; 4] = ["glass", "bar", "borders", "stats"];

    pub fn part(&self, name: &str) -> Option<bool> {
        match name {
            "glass" => Some(self.glass),
            "bar" => Some(self.bar),
            "borders" => Some(self.borders),
            "stats" => Some(self.stats),
            _ => None,
        }
    }

    pub fn set_part(&mut self, name: &str, on: bool) -> Result<()> {
        match name {
            "glass" => self.glass = on,
            "bar" => self.bar = on,
            "borders" => self.borders = on,
            "stats" => self.stats = on,
            other => bail!(
                "Performance mode covers {} (not `{other}`)",
                Self::PARTS.join(", ")
            ),
        }
        Ok(())
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: DEFAULT_HOTKEY.into(),
            off: BTreeSet::new(),
            welcomed: false,
            wallpapers: BTreeMap::new(),
            obsidian_ring: true,
            obsidian_dim: true,
            zed_glass: Glass::Blurred,
            zed_strength: 50,
            glass: BTreeMap::new(),
            schedule: Default::default(),
            favourites: Vec::new(),
            next_cycles: crate::favourites::Cycle::All,
            performance: Performance::default(),
            windows: AppWindows::Ready,
            menu_click: MenuClick::Picker,
            log: false,
            close_idle_settings: false,
            readability: crate::normalise::Level::Minimal,
            monochrome: crate::monochrome::Mono::default(),
            bar_scroll: true,
            glass_lift: BTreeMap::new(),
            original_colours: BTreeSet::new(),
        }
    }
}

impl Settings {
    /// Does Palette theme this app? One list for the whole machine, every
    /// desktop session alike (owner, 2026-10-28).
    pub fn is_on(&self, app: &str) -> bool {
        !self.off.contains(app)
    }

    /// A window's glass as Palette writes it now: solid in Performance mode, else
    /// [`Settings::glass_saved`].
    pub fn glass(&self, app: &str) -> WindowGlass {
        let saved = self.glass_saved(app);
        if self.performance.glass_now() {
            WindowGlass {
                kind: TermGlass::Solid,
                ..saved
            }
        } else {
            saved
        }
    }

    /// A terminal's window glass as chosen (its own setting unless chosen in Palette).
    pub fn glass_saved(&self, app: &str) -> WindowGlass {
        self.glass.get(app).copied().unwrap_or_default()
    }

    /// Zed's window as Palette writes it now (solid in Performance mode).
    pub fn zed_glass_now(&self) -> Glass {
        if self.performance.glass_now() {
            Glass::Off
        } else {
            self.zed_glass
        }
    }
}

fn file(paths: &Paths) -> std::path::PathBuf {
    paths.root().join("settings.toml")
}

pub fn load(paths: &Paths) -> Result<Settings> {
    let path = file(paths);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Settings::default()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let table: toml::Table = text
        .parse()
        .with_context(|| format!("{} is not valid TOML", path.display()))?;
    let hotkey = match table.get("hotkey").and_then(|v| v.as_str()) {
        Some(h) => normalise_hotkey(h)?,
        None => DEFAULT_HOTKEY.into(),
    };
    let off = table
        .get("apps")
        .and_then(|v| v.as_table())
        .map(|apps| {
            apps.iter()
                .filter(|(_, v)| v.as_bool() == Some(false))
                .map(|(k, _)| k.clone())
                .collect()
        })
        .unwrap_or_default();
    let wallpapers = table
        .get("wallpapers")
        .and_then(|v| v.as_table())
        .map(|t| {
            t.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let obsidian = table.get("obsidian").and_then(|v| v.as_table());
    let flag = |key: &str| {
        obsidian
            .and_then(|t| t.get(key))
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
    };
    let (obsidian_ring, obsidian_dim) = (flag("focus_ring"), flag("dim_inactive"));
    let zed = table.get("zed").and_then(|v| v.as_table());
    let zed_glass = match zed.and_then(|t| t.get("glass")) {
        // `true`/`false`: the first version of this setting (2026-11-01).
        Some(toml::Value::Boolean(on)) => {
            if *on {
                Glass::Blurred
            } else {
                Glass::Off
            }
        }
        Some(toml::Value::String(s)) => Glass::parse(s)?,
        _ => Glass::Blurred,
    };
    let zed_strength = zed
        .and_then(|t| t.get("strength"))
        .and_then(|v| v.as_integer())
        .map_or(50, |n| n.clamp(0, 100) as u8);
    let glass = glass_from_toml(table.get("glass"))?;
    let welcomed = table
        .get("welcomed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let favourites = table
        .get("favourites")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .take(crate::favourites::MAX_FAVOURITES)
                .collect()
        })
        .unwrap_or_default();
    let next_cycles = match table.get("next_cycles").and_then(|v| v.as_str()) {
        Some(c) => crate::favourites::Cycle::parse(c)?,
        None => crate::favourites::Cycle::All,
    };
    let perf = table.get("performance").and_then(|v| v.as_table());
    let base = Performance::default();
    let part = |key: &str, or: bool| {
        perf.and_then(|t| t.get(key))
            .and_then(|v| v.as_bool())
            .unwrap_or(or)
    };
    let performance = Performance {
        on: part("on", base.on),
        glass: part("glass", base.glass),
        bar: part("bar", base.bar),
        borders: part("borders", base.borders),
        stats: part("stats", base.stats),
    };
    let windows = match table.get("windows").and_then(|v| v.as_str()) {
        Some(w) => AppWindows::parse(w)?,
        None => AppWindows::default(),
    };
    let menu_click = match table.get("menu_click").and_then(|v| v.as_str()) {
        Some(c) => MenuClick::parse(c)?,
        None => MenuClick::default(),
    };
    let log = table.get("log").and_then(|v| v.as_bool()).unwrap_or(false);
    let close_idle_settings = table
        .get("close_idle_settings")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let readability = match table.get("readability").and_then(|v| v.as_str()) {
        Some(l) => crate::normalise::Level::parse(l)?,
        None => crate::normalise::Level::default(),
    };
    let mut glass_lift: BTreeMap<String, u8> = table
        .get("glass_lift")
        .and_then(|v| v.as_table())
        .map(|t| {
            t.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_integer()?.clamp(0, 100) as u8)))
                .collect()
        })
        .unwrap_or_default();
    // Before F2 (2026-11-07): "Glass +" was an on/off per "theme/wallpaper"; it judged against
    // the brightest tenth, about the slider's middle.
    for pair in table
        .get("strong_lift")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str())
    {
        if let Some((theme, _)) = pair.rsplit_once('/') {
            glass_lift.entry(theme.to_string()).or_insert(50);
        }
    }
    let bar_scroll = table
        .get("bar_scroll")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let mono_default = crate::monochrome::Mono::default();
    let monochrome = crate::monochrome::Mono {
        greys: table
            .get("monochrome_greys")
            .and_then(|v| v.as_bool())
            .unwrap_or(mono_default.greys),
        tint: table
            .get("monochrome_tint")
            .and_then(|v| v.as_bool())
            .unwrap_or(mono_default.tint),
    };
    let original_colours = table
        .get("original_colours")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    Ok(Settings {
        log,
        close_idle_settings,
        readability,
        monochrome,
        bar_scroll,
        glass_lift,
        original_colours,
        windows,
        menu_click,
        performance,
        hotkey,
        off,
        welcomed,
        wallpapers,
        obsidian_ring,
        obsidian_dim,
        zed_glass,
        zed_strength,
        glass,
        schedule: crate::schedule::from_toml(table.get("schedule")),
        favourites,
        next_cycles,
    })
}

pub fn save(paths: &Paths, settings: &Settings) -> Result<()> {
    let path = file(paths);
    fs::create_dir_all(paths.root())?;
    let mut text = format!(
        "# Palette settings (edit freely; `palette hotkey` and `palette apps` change them)\n\
         hotkey = \"{}\"\n\
         welcomed = {}\n\
         # Favourite themes (`palette fav`); what `palette next` steps through: \"all\" or \"favourites\"\n\
         favourites = {}\n\
         next_cycles = \"{}\"\n\
         # Scroll on the bar's theme pill to switch theme\n\
         bar_scroll = {}\n\
         # The app (`palette ui`): windows \"ready\", \"on-demand\" or \"all-on-demand\"; menu bar click opens \"menu\", \"picker\" or \"quick-switcher\"\n\
         windows = \"{}\"\n\
         menu_click = \"{}\"\n\
         # Windows on demand: close Settings after 10 minutes without focus, even while open\n\
         close_idle_settings = {}\n\
         # One line per switch in palette.log, on this Mac only (`palette log`)\n\
         log = {}\n\
         # Readability fixes when a theme is used: \"off\", \"minimal\" or \"strong\"; themes left as made (`palette check`)\n\
         readability = \"{}\"\n\
         original_colours = {}\n\
         # Monochrome themes: terminal greys spaced apart; a faint tint of the theme's hue\n\
         monochrome_greys = {}\n\
         monochrome_tint = {}\n\n\
         # Apps Palette leaves on their own look (everything else is themed)\n\
         [apps]\n",
        settings.hotkey,
        settings.welcomed,
        toml::Value::Array(
            settings
                .favourites
                .iter()
                .map(|f| toml::Value::String(f.clone()))
                .collect()
        ),
        settings.next_cycles.as_str(),
        settings.bar_scroll,
        settings.windows.as_str(),
        settings.menu_click.as_str(),
        settings.close_idle_settings,
        settings.log,
        settings.readability.as_str(),
        toml::Value::Array(
            settings
                .original_colours
                .iter()
                .map(|t| toml::Value::String(t.clone()))
                .collect()
        ),
        settings.monochrome.greys,
        settings.monochrome.tint,
    );
    for app in &settings.off {
        text.push_str(&format!("{app} = false\n"));
    }
    if !settings.wallpapers.is_empty() {
        text.push_str(
            "\n# The wallpaper each theme starts with (`palette bg default`)\n[wallpapers]\n",
        );
        for (theme, file) in &settings.wallpapers {
            let quoted = toml::Value::String(theme.clone()).to_string();
            let bare = theme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            let key = if bare { theme } else { &quoted };
            text.push_str(&format!("{key} = {}\n", toml::Value::String(file.clone())));
        }
    }
    if !settings.glass_lift.is_empty() {
        text.push_str(
            "\n# Glass lift by hand, per theme (Settings → My themes): 0 = off, 1-100 = stronger than Auto\n[glass_lift]\n",
        );
        for (theme, v) in &settings.glass_lift {
            let quoted = toml::Value::String(theme.clone()).to_string();
            let bare = theme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            text.push_str(&format!("{} = {v}\n", if bare { theme } else { &quoted }));
        }
    }
    text.push_str(&format!(
        "\n# Obsidian: accent ring around the active pane, fade the others (`palette obsidian`)\n\
         [obsidian]\nfocus_ring = {}\ndim_inactive = {}\n",
        settings.obsidian_ring, settings.obsidian_dim
    ));
    text.push_str(&format!(
        "\n# Zed window (`palette zed`): glass \"off\", \"blurred\" or \"clear\"; strength 0-100 (how see-through)\n\
         [zed]\nglass = \"{}\"\nstrength = {}\n",
        settings.zed_glass.as_str(),
        settings.zed_strength
    ));
    if !settings.glass.is_empty() {
        text.push_str(
            "\n# Windows (`palette glass`: terminals, Dispatch, EasyPlots, Habit Tracker): kind \"own\" (the app decides), \"solid\",\n\
             # \"blurred\", \"clear\" (Ghostty also \"liquid-glass\", \"liquid-clear\");\n\
             # strength 0-100 (how see-through), blur 0-100\n",
        );
        for (app, g) in &settings.glass {
            text.push_str(&format!(
                "[glass.{app}]\nkind = \"{}\"\nstrength = {}\nblur = {}\n",
                g.kind.as_str(),
                g.strength,
                g.blur
            ));
        }
    }
    let p = settings.performance;
    if p != Performance::default() {
        text.push_str(&format!(
            "\n# Performance mode (`palette performance`): on, and which parts it covers\n\
             [performance]\non = {}\nglass = {}\nbar = {}\nborders = {}\nstats = {}\n",
            p.on, p.glass, p.bar, p.borders, p.stats
        ));
    }
    text.push_str(&crate::schedule::to_toml(&settings.schedule));
    fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
}

/// Zed's window: solid, or see-through with or without blur. Each is its own
/// theme name in Zed (`theme_name`), because Zed only re-applies the window's
/// blur when its settings change, not when a theme file reloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glass {
    Off,
    Blurred,
    Clear,
}

impl Glass {
    pub const ALL: [Glass; 3] = [Glass::Off, Glass::Blurred, Glass::Clear];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Blurred => "blurred",
            Self::Clear => "clear",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "solid" | "none" => Ok(Self::Off),
            "blurred" | "blur" | "on" => Ok(Self::Blurred),
            "clear" | "transparent" => Ok(Self::Clear),
            other => bail!("glass is off, blurred or clear (not `{other}`)"),
        }
    }

    /// The theme in Palette's Zed file that has this window.
    pub fn theme_name(self) -> &'static str {
        match self {
            Self::Off => "Palette",
            Self::Blurred => "Palette Glass",
            Self::Clear => "Palette Clear",
        }
    }
}

/// The apps with a window glass control (`palette glass`): the terminals, and Dispatch and
/// EasyPlots and the Habit Tracker (which read it from their colours files).
pub const GLASS_APPS: [&str; 7] = [
    "ghostty",
    "kitty",
    "alacritty",
    "wezterm",
    "dispatch",
    "easyplots",
    "habit-tracker",
];

/// A terminal window's look. `Own` writes nothing, so the app's own config
/// decides (the default: people often set opacity there already). The two
/// Liquid kinds are macOS 26's native glass, which only Ghostty offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermGlass {
    Own,
    Solid,
    Blurred,
    Clear,
    LiquidGlass,
    LiquidClear,
}

impl TermGlass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Own => "own",
            Self::Solid => "solid",
            Self::Blurred => "blurred",
            Self::Clear => "clear",
            Self::LiquidGlass => "liquid-glass",
            Self::LiquidClear => "liquid-clear",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "own" | "yours" | "default" | "leave" => Ok(Self::Own),
            "solid" | "off" | "none" => Ok(Self::Solid),
            "blurred" | "blur" | "on" => Ok(Self::Blurred),
            "clear" | "transparent" => Ok(Self::Clear),
            "liquid-glass" | "liquid" | "macos-glass-regular" => Ok(Self::LiquidGlass),
            "liquid-clear" | "macos-glass-clear" => Ok(Self::LiquidClear),
            other => bail!(
                "glass is own, solid, blurred, clear, liquid-glass or liquid-clear (not `{other}`)"
            ),
        }
    }

    /// The kinds `app` offers (Liquid only in Ghostty).
    pub fn kinds(app: &str) -> &'static [TermGlass] {
        use TermGlass::*;
        if app == "ghostty" {
            &[Own, Solid, Blurred, Clear, LiquidGlass, LiquidClear]
        } else {
            &[Own, Solid, Blurred, Clear]
        }
    }
}

/// One terminal's glass: the kind, how see-through (0 solid, 100 fully
/// clear) and how strong the blur is (0-100, used by `Blurred` only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowGlass {
    pub kind: TermGlass,
    pub strength: u8,
    pub blur: u8,
}

impl Default for WindowGlass {
    /// Strength 20 keeps text easy to read; blur 20 is Ghostty's own default.
    fn default() -> Self {
        Self {
            kind: TermGlass::Own,
            strength: 20,
            blur: 20,
        }
    }
}

/// `[glass.<app>]` tables; unknown apps are ignored, a bad kind is an error.
fn glass_from_toml(value: Option<&toml::Value>) -> Result<BTreeMap<String, WindowGlass>> {
    let mut out = BTreeMap::new();
    let Some(table) = value.and_then(|v| v.as_table()) else {
        return Ok(out);
    };
    for app in GLASS_APPS {
        let Some(t) = table.get(app).and_then(|v| v.as_table()) else {
            continue;
        };
        let base = WindowGlass::default();
        let kind = match t.get("kind").and_then(|v| v.as_str()) {
            Some(k) => TermGlass::parse(k)?,
            None => base.kind,
        };
        let number = |key: &str, or: u8| {
            t.get(key)
                .and_then(|v| v.as_integer())
                .map_or(or, |n| n.clamp(0, 100) as u8)
        };
        out.insert(
            app.to_string(),
            WindowGlass {
                kind,
                strength: number("strength", base.strength),
                blur: number("blur", base.blur),
            },
        );
    }
    Ok(out)
}

const MODIFIERS: [(&str, &[&str]); 4] = [
    ("ctrl", &["ctrl", "control"]),
    ("alt", &["alt", "option", "opt"]),
    ("shift", &["shift"]),
    ("cmd", &["cmd", "command", "super", "meta"]),
];

/// `Option-Shift-T`, `alt+shift+t`, `shift + alt + T` → `alt+shift+t`.
/// At least one modifier and exactly one key: a letter, digit, F1–F20,
/// `space`, or an arrow.
pub fn normalise_hotkey(input: &str) -> Result<String> {
    let parts: Vec<String> = input
        .split(['+', '-'])
        .map(|p| p.trim().to_ascii_lowercase())
        .filter(|p| !p.is_empty())
        .collect();
    let mut mods = Vec::new();
    let mut key = None;
    for part in &parts {
        match MODIFIERS
            .iter()
            .find(|(_, names)| names.contains(&part.as_str()))
        {
            Some((name, _)) if !mods.contains(name) => mods.push(*name),
            Some(_) => bail!("`{input}`: {part} appears twice"),
            None if key.is_none() => key = Some(part.clone()),
            None => bail!("`{input}`: only one key besides the modifiers"),
        }
    }
    let Some(key) = key else {
        bail!("`{input}`: needs a key, e.g. alt+shift+t");
    };
    let valid_key = (key.len() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric()))
        || ["space", "up", "down", "left", "right", "tab", "enter"].contains(&key.as_str())
        || key
            .strip_prefix('f')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=20).contains(&n));
    if !valid_key {
        bail!(
            "`{input}`: `{key}` isn't a key Palette can bind (letters, digits, F1-F20, space, arrows)"
        );
    }
    if mods.is_empty() {
        bail!("`{input}`: needs at least one modifier (alt, shift, ctrl, cmd)");
    }
    mods.sort_by_key(|m| MODIFIERS.iter().position(|(n, _)| n == m));
    Ok(format!("{}+{key}", mods.join("+")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkeys() {
        assert_eq!(normalise_hotkey("Option-Shift-T").unwrap(), "alt+shift+t");
        assert_eq!(normalise_hotkey("shift + alt + T").unwrap(), "alt+shift+t");
        assert_eq!(
            normalise_hotkey("cmd+ctrl+space").unwrap(),
            "ctrl+cmd+space"
        );
        assert_eq!(normalise_hotkey("alt+f12").unwrap(), "alt+f12");
        assert!(normalise_hotkey("t").is_err());
        assert!(normalise_hotkey("alt+shift").is_err());
        assert!(normalise_hotkey("alt+a+b").is_err());
        assert!(normalise_hotkey("alt+alt+t").is_err());
        assert!(normalise_hotkey("alt+é").is_err());
    }

    #[test]
    fn load_and_save() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        assert_eq!(load(&paths).unwrap(), Settings::default());
        let mut s = Settings {
            hotkey: "ctrl+cmd+p".into(),
            ..Settings::default()
        };
        s.off.insert("sketchybar".into());
        s.obsidian_dim = false;
        s.favourites = vec!["nord".into(), "tokyo-night".into()];
        s.next_cycles = crate::favourites::Cycle::Favourites;
        s.windows = AppWindows::AllOnDemand;
        s.menu_click = MenuClick::Quick;
        s.log = true;
        s.close_idle_settings = true;
        s.readability = crate::normalise::Level::Strong;
        s.original_colours.insert("ash".into());
        s.glass.insert(
            "ghostty".into(),
            WindowGlass {
                kind: TermGlass::LiquidClear,
                strength: 35,
                blur: 60,
            },
        );
        s.glass.insert(
            "kitty".into(),
            WindowGlass {
                kind: TermGlass::Solid,
                ..Default::default()
            },
        );
        save(&paths, &s).unwrap();
        let back = load(&paths).unwrap();
        assert_eq!(back, s);
        assert!(!back.is_on("sketchybar") && back.is_on("ghostty"));
        assert_eq!(back.glass("wezterm"), WindowGlass::default());
    }

    #[test]
    fn glass_kinds() {
        assert_eq!(TermGlass::parse("Off").unwrap(), TermGlass::Solid);
        assert_eq!(TermGlass::parse("liquid").unwrap(), TermGlass::LiquidGlass);
        assert!(TermGlass::parse("frosted").is_err());
        assert!(TermGlass::kinds("ghostty").contains(&TermGlass::LiquidClear));
        assert!(!TermGlass::kinds("kitty").contains(&TermGlass::LiquidClear));
        for kind in TermGlass::kinds("ghostty") {
            assert_eq!(TermGlass::parse(kind.as_str()).unwrap(), *kind);
        }
    }

    /// Palette is menu-bar-only: an old `dock = true` still loads, and the next save drops it.
    #[test]
    fn old_dock_setting_is_ignored() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        fs::create_dir_all(paths.root()).unwrap();
        fs::write(file(&paths), "hotkey = \"alt+shift+w\"\ndock = true\n").unwrap();
        let s = load(&paths).unwrap();
        assert_eq!(s.hotkey, "alt+shift+w");
        save(&paths, &s).unwrap();
        assert!(!fs::read_to_string(file(&paths)).unwrap().contains("dock"));
    }
}
