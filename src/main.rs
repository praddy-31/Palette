use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use palette::doctor::{self, Status};
use palette::theme::{self, Mode, Origin, Theme};
use palette::wiring::Step;
use palette::{Change, Effect, Outcome, Paths, Switch, effects, state};

const AFTER_HELP: &str = "\
Commands by job:
  Themes           list, current, set, next, prev, undo, refresh, catalog, install, update, remove, new
  Picking          fav, recent, schedule
  Colours          check, colours
  Wallpapers       bg
  Desktop          desktop
  Setup, health    init, apps, doctor, tools
  Another Mac      export, import
  App, settings    ui, hotkey, obsidian

Everything the app does is also a command here. Run `palette <command> --help` for details.";

/// Restyle your whole desktop from one palette file.
#[derive(Parser)]
#[command(
    version,
    about,
    after_help = AFTER_HELP
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List available themes with colour swatches (* marks the active one)
    #[command(display_order = 1)]
    List,
    /// Print the active theme
    #[command(display_order = 2)]
    Current,
    /// Switch to a theme
    #[command(display_order = 3)]
    Set {
        name: String,
        /// Start on this one of the theme's wallpapers (name as `bg list` shows it)
        #[arg(long)]
        wallpaper: Option<String>,
        /// Show which files would change, without writing anything
        #[arg(long)]
        dry_run: bool,
    },
    /// Switch to the next theme (alphabetical, wraps around; favourites only
    /// if `palette fav cycle favourites` is set)
    #[command(display_order = 4)]
    Next {
        /// Show which files would change, without writing anything
        #[arg(long)]
        dry_run: bool,
        /// Step through favourites only, this time
        #[arg(long, conflicts_with = "all")]
        favourites: bool,
        /// Step through every theme, this time
        #[arg(long)]
        all: bool,
    },
    /// Switch to the previous theme (alphabetical, wraps around)
    #[command(display_order = 5)]
    Prev {
        /// Show which files would change, without writing anything
        #[arg(long)]
        dry_run: bool,
        /// Step through favourites only, this time
        #[arg(long, conflicts_with = "all")]
        favourites: bool,
        /// Step through every theme, this time
        #[arg(long)]
        all: bool,
    },
    /// Go back to the previous theme (repeat to go further back)
    #[command(display_order = 6)]
    Undo,
    /// Render the active theme again (after editing starship.toml, say)
    #[command(display_order = 7)]
    Refresh,
    /// Check that every app is installed, wired and up to date
    #[command(display_order = 16)]
    Doctor,
    /// Show or change the hotkey that opens the quick-switch overlay
    #[command(display_order = 21)]
    Hotkey {
        /// e.g. alt+shift+t, ctrl+cmd+space
        combo: Option<String>,
    },
    /// Obsidian's active pane: accent ring and fading the others
    ///
    /// Both apply when the editor is split, and are on by default. With no
    /// options this shows the current setting.
    #[command(display_order = 22)]
    Obsidian {
        /// Accent ring around the active pane
        #[arg(long, value_name = "on|off", value_parser = on_off)]
        ring: Option<bool>,
        /// Fade the other panes a little
        #[arg(long, value_name = "on|off", value_parser = on_off)]
        dim: Option<bool>,
    },
    /// Choose which apps Palette themes (all on by default)
    #[command(display_order = 15)]
    Apps {
        #[command(subcommand)]
        command: Option<AppsCommand>,
    },
    /// Copy your Palette setup (themes, wallpapers, settings, app configs) to a folder
    #[command(display_order = 18)]
    Export {
        /// New or empty folder, or an earlier export to refresh
        dir: PathBuf,
        /// Leave wallpapers out this time
        #[arg(long)]
        no_wallpapers: bool,
        /// Leave your app configs out this time
        #[arg(long)]
        no_configs: bool,
    },
    /// Set this Mac up from an export folder (backs up whatever it replaces)
    #[command(display_order = 19)]
    Import {
        dir: PathBuf,
        /// Show what would change, without touching anything
        #[arg(long)]
        dry_run: bool,
    },
    /// Open the Palette app's picker (or the overlay)
    #[command(display_order = 20)]
    Ui {
        #[arg(long)]
        overlay: bool,
    },
    /// Point your app configs at Palette's files (backs them up first)
    #[command(display_order = 14)]
    Init {
        /// Show what would change, without touching any file
        #[arg(long)]
        dry_run: bool,
        /// Take Palette's lines back out of your configs
        #[arg(long)]
        remove: bool,
    },
    /// Community themes you can install (Omarchy's extra themes); offline list
    #[command(display_order = 8)]
    Catalog {
        /// Only names/titles containing this
        search: Option<String>,
    },
    /// Install a theme from the catalogue, a GitHub link or a folder
    ///
    /// Takes only its colours, wallpapers and preview (never config files).
    #[command(display_order = 9)]
    Install {
        source: String,
        /// Install under this name instead
        #[arg(long)]
        name: Option<String>,
    },
    /// Download installed themes again (all of them, or the ones named)
    #[command(display_order = 10)]
    Update { names: Vec<String> },
    /// Remove a theme (one you made goes to the Trash)
    #[command(display_order = 11)]
    Remove { name: String },
    /// Make a theme from a wallpaper
    ///
    /// Colours are picked from the picture and every text colour is checked for
    /// contrast; the picture becomes the theme's wallpaper.
    #[command(display_order = 12)]
    New {
        /// Name for the new theme (lowercase letters, digits, -)
        name: String,
        #[arg(long, value_name = "IMAGE")]
        from_wallpaper: PathBuf,
        /// Make a light theme (default: light if the picture is mostly light)
        #[arg(long, conflicts_with = "dark")]
        light: bool,
        /// Make a dark theme
        #[arg(long)]
        dark: bool,
        /// Only show the colours, save nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Wallpapers of the active theme
    #[command(display_order = 13)]
    Bg {
        #[command(subcommand)]
        command: BgCommand,
    },
    /// SketchyBar, JankyBorders, AeroSpace, gowall: status and Homebrew commands
    ///
    /// Shows what is installed and the Homebrew commands to install or remove
    /// each, plus optional starter configs. Palette never runs Homebrew for you: it prints the commands to copy into
    /// a terminal. `palette tools starter <tool>` writes a small Palette-themed
    /// config, only for a tool that has no config at all (it never overwrites yours).
    #[command(display_order = 15)]
    Tools {
        #[command(subcommand)]
        command: Option<ToolsCommand>,
    },
    /// Day and night themes: show, or `on`, `off`, `run`, `cities`
    Schedule {
        #[command(subcommand)]
        command: Option<ScheduleCommand>,
    },
    /// Contrast report: colour pairs of a theme that are hard to read
    /// (default: the active theme)
    Check {
        theme: Option<String>,
        /// Apply every suggested colour (same as `palette colours --fix`)
        #[arg(long)]
        fix: bool,
    },
    /// A theme's colours: show them, change them, or undo your changes
    ///
    /// Edits are kept in the theme's edits.toml, on top of its own colours, so
    /// `--reset` always gets the original back. The active theme re-applies.
    #[command(alias = "colors")]
    Colours {
        /// Default: the active theme
        theme: Option<String>,
        /// Change a colour, e.g. --set accent=#7aa2f7 (repeat for more)
        #[arg(long, value_name = "KEY=#RRGGBB")]
        set: Vec<String>,
        /// Undo your edits: all of them, or just KEY
        #[arg(long, value_name = "KEY", num_args = 0..=1, conflicts_with = "set")]
        reset: Option<Option<String>>,
        /// Apply every suggestion from `palette check`
        #[arg(long, conflicts_with_all = ["set", "reset"])]
        fix: bool,
    },
    /// The bar, windows and borders: SketchyBar, AeroSpace, JankyBorders
    ///
    /// `palette desktop` shows all three; `palette desktop bar` shows the bar's
    /// settings. Palette writes a tool's config only after `manage`, backing
    /// yours up first; `release` gives it back.
    Desktop {
        /// bar, windows, borders; or macos (Dock and menu bar), fonts (installed, with styles)
        tool: Option<String>,
        #[command(subcommand)]
        command: Option<DesktopCommand>,
    },
    /// Favourite themes (up to ten): `next --favourites` steps through them
    Fav {
        #[command(subcommand)]
        command: Option<FavCommand>,
    },
    /// The last themes you switched to, newest first
    Recent,
}

#[derive(Subcommand)]
enum ScheduleCommand {
    /// Turn it on (and switch now if the other theme is due)
    On {
        /// Theme from sunrise (or the first fixed time)
        #[arg(long)]
        day: Option<String>,
        /// Theme from sunset (or the second fixed time)
        #[arg(long)]
        night: Option<String>,
        /// A city from `palette schedule cities`
        #[arg(long, conflicts_with_all = ["lat", "lon", "at"])]
        city: Option<String>,
        /// Latitude, north positive (with --lon)
        #[arg(long, allow_hyphen_values = true, conflicts_with = "at")]
        lat: Option<f64>,
        /// Longitude, east positive (with --lat)
        #[arg(long, allow_hyphen_values = true, conflicts_with = "at")]
        lon: Option<f64>,
        /// Fixed times instead of the sun: day,night, e.g. 07:00,19:00
        #[arg(long)]
        at: Option<String>,
    },
    /// Turn it off (the choices are kept)
    Off,
    /// Switch now if a sunrise/sunset has passed (for cron, launchd, Shortcuts)
    Run,
    /// List the built-in cities, or search them
    Cities { search: Option<String> },
}

#[derive(Subcommand)]
enum DesktopCommand {
    /// Let Palette write this tool's config (yours is backed up as the original)
    Manage,
    /// Give the config back: your original file again
    Release,
    /// Change settings, e.g. `set height=32 pill.corner_radius=10`
    Set {
        #[arg(required = true, value_name = "KEY=VALUE")]
        pairs: Vec<String>,
    },
    /// Write the config again from the saved settings
    Apply,
    /// The backups made before each change, newest first
    Backups,
    /// Go back to a backup: 1 is the newest, or its name (see `backups`)
    Restore {
        #[arg(default_value = "1")]
        which: String,
    },
    /// windows: a keyboard shortcut, e.g. `bind alt-enter exec-and-forget open -a Ghostty`
    Bind {
        key: String,
        /// The command (several: separate them with `;`)
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
        /// The binding mode
        #[arg(long, default_value = "main")]
        mode: String,
    },
    /// windows: remove a keyboard shortcut
    Unbind {
        key: String,
        #[arg(long, default_value = "main")]
        mode: String,
    },
    /// prompt: list Starship's presets, or use one (your file is backed up)
    Preset { name: Option<String> },
    /// prompt: print the prompt as Starship draws it with the saved settings
    Preview,
}

#[derive(Subcommand)]
enum FavCommand {
    /// List favourites and what `palette next` steps through
    List,
    /// Add a theme to your favourites
    Add { theme: String },
    /// Take a theme out of your favourites
    Remove { theme: String },
    /// What plain `palette next`/`prev` and the bar pill step through
    Cycle {
        #[arg(value_parser = ["all", "favourites"])]
        what: String,
    },
}

#[derive(Subcommand)]
enum AppsCommand {
    /// Theme these apps again (wires them now)
    On {
        #[arg(required = true)]
        apps: Vec<String>,
    },
    /// Leave these apps on their own look (unwires them now, restoring their config)
    Off {
        #[arg(required = true)]
        apps: Vec<String>,
    },
}

#[derive(Subcommand)]
enum BgCommand {
    /// List the active theme's wallpapers (* current, d default)
    List,
    /// Switch to the active theme's next wallpaper
    Next,
    /// Switch to the active theme's previous wallpaper
    Prev,
    /// The wallpaper a theme starts with (instead of its first)
    Default {
        /// Name as `bg list` shows it; leave out with --clear
        #[arg(required_unless_present = "clear")]
        name: Option<String>,
        /// Back to starting with the first wallpaper
        #[arg(long, conflicts_with = "name")]
        clear: bool,
        #[arg(long)]
        theme: Option<String>,
    },
    /// Use one of a theme's wallpapers now (name as `bg list` shows it)
    Set {
        name: String,
        /// Theme it belongs to (default: the active theme)
        #[arg(long)]
        theme: Option<String>,
    },
    /// Delete Palette's copy of a wallpaper (your original stays where it was)
    Remove {
        name: String,
        #[arg(long)]
        theme: Option<String>,
    },
    /// Recolour pictures to a theme's colours; the copies join its wallpapers
    Recolor {
        #[arg(required = true)]
        images: Vec<PathBuf>,
        /// Theme to recolour to (default: the active theme)
        #[arg(long)]
        theme: Option<String>,
        /// palette (built in) or gowall (if installed: brew install gowall)
        #[arg(long, default_value = "palette")]
        engine: String,
        /// Use the (first) recoloured picture now
        #[arg(long)]
        apply: bool,
        /// How far Palette's engine pulls towards the theme: subtle,
        /// balanced or strong (near-flat, like gowall). Not used by gowall
        #[arg(long, default_value = "balanced")]
        strength: String,
    },
    /// Paint a wallpaper from a theme's colours (gradient, mesh, waves, topo,
    /// geometric or grain)
    Generate {
        #[arg(default_value = "gradient")]
        style: String,
        #[arg(long)]
        theme: Option<String>,
        /// Use it now
        #[arg(long)]
        apply: bool,
        /// Paint variant N (saved as palette-<style>-<N>.jpg); same N, same picture
        #[arg(long)]
        seed: Option<u64>,
    },
    /// Copy images into a theme's backgrounds folder
    Add {
        #[arg(required = true)]
        images: Vec<PathBuf>,
        /// Theme to add to (default: the active theme)
        #[arg(long)]
        theme: Option<String>,
    },
    /// Your wallpaper library (~/.config/palette/wallpapers): pictures for any theme
    Library {
        #[command(subcommand)]
        command: Option<LibraryCommand>,
    },
    /// Get wallpapers from a link: one picture, or a GitHub repo or folder
    /// (alone, it lists the repo's folders; then choose with --folder, --pick or --all)
    Get {
        url: String,
        /// Download everything under this folder of the repo
        #[arg(long)]
        folder: Option<String>,
        /// Download pictures whose name or path matches (`*`, `?`), e.g. '*lake*'
        #[arg(long)]
        pick: Option<String>,
        /// Download every picture (at most 200 / 2 GB at a time)
        #[arg(long)]
        all: bool,
        /// Library subfolder to save into (default: the repo's name, or `downloads`)
        #[arg(long)]
        into: Option<String>,
        /// Also add the pictures to this theme's wallpapers
        #[arg(long)]
        theme: Option<String>,
        /// With --theme: add copies recoloured to the theme
        #[arg(long, requires = "theme")]
        recolor: bool,
    },
}

#[derive(Subcommand)]
enum LibraryCommand {
    /// List the library's pictures, its folder and size (the default)
    List,
    /// Copy pictures into the library
    Add {
        #[arg(required = true)]
        images: Vec<PathBuf>,
        /// Subfolder, e.g. nord or CozyPixels/Nord
        #[arg(long)]
        into: Option<String>,
    },
    /// Delete a picture from the library (name as `bg library` shows it)
    Remove { name: String },
    /// Use a library picture as the wallpaper now (until the next theme switch)
    Use { name: String },
    /// Copy a library picture into a theme's wallpapers
    Theme {
        name: String,
        /// Theme to add to (default: the active theme)
        #[arg(long)]
        theme: Option<String>,
        /// Add a copy recoloured to the theme instead
        #[arg(long)]
        recolor: bool,
    },
    /// Make a new theme from a library picture (like `palette new --from-wallpaper`)
    NewTheme {
        name: String,
        /// The new theme's name
        theme_name: String,
        #[arg(long, conflicts_with = "dark")]
        light: bool,
        #[arg(long)]
        dark: bool,
        /// Only show the colours; save nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Open the library folder in Finder
    Open,
}

fn on_off(value: &str) -> Result<bool, String> {
    match value {
        "on" | "true" | "yes" => Ok(true),
        "off" | "false" | "no" => Ok(false),
        _ => Err(format!("`{value}`: use on or off")),
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let paths = Paths::from_env()?;
    match cli.command {
        Command::List => list(&paths)?,
        Command::Refresh => report(&palette::refresh(&paths)?, &paths),
        Command::Doctor => doctor(&paths)?,
        Command::Current => match state::current_theme(&paths)? {
            Some(name) => println!("{name}"),
            None => println!("(none: run `palette set <theme>`)"),
        },
        Command::Set {
            name,
            wallpaper,
            dry_run,
        } => report(
            &palette::switch_with_wallpaper(&paths, &name, wallpaper.as_deref(), dry_run)?,
            &paths,
        ),
        Command::Next {
            dry_run,
            favourites,
            all,
        } => report(
            &palette::step(&paths, 1, cycle(favourites, all), dry_run)?,
            &paths,
        ),
        Command::Prev {
            dry_run,
            favourites,
            all,
        } => report(
            &palette::step(&paths, -1, cycle(favourites, all), dry_run)?,
            &paths,
        ),
        Command::Hotkey { combo } => hotkey(&paths, combo)?,
        Command::Apps { command } => apps(&paths, command)?,
        Command::Obsidian { ring, dim } => {
            let (ring, dim) = palette::obsidian_look(&paths, ring, dim)?;
            let word = |on| if on { "on" } else { "off" };
            println!(
                "Obsidian active pane: ring {}, dim the others {}",
                word(ring),
                word(dim)
            );
        }
        Command::Export {
            dir,
            no_wallpapers,
            no_configs,
        } => export(&paths, &dir, no_wallpapers, no_configs)?,
        Command::Import { dir, dry_run } => import(&paths, &dir, dry_run)?,
        Command::Ui { overlay } => {
            let url = if overlay {
                "palette://overlay"
            } else {
                "palette://picker"
            };
            palette::system::run("open", &[url])
                .map_err(|e| anyhow::anyhow!("{e} (is the Palette app installed?)"))?;
        }
        Command::Undo => {
            let (name, effects) = palette::undo(&paths)?;
            println!("Back to {name}");
            print_effects(&effects);
        }
        Command::Init { dry_run, remove } => init(&paths, remove, dry_run)?,
        Command::Bg { command } => bg(&paths, command)?,
        Command::New {
            name,
            from_wallpaper,
            light,
            dark,
            dry_run,
        } => {
            let mode = match (light, dark) {
                (true, _) => Some(true),
                (_, true) => Some(false),
                _ => None,
            };
            let d =
                palette::studio::new_from_wallpaper(&paths, &name, &from_wallpaper, mode, dry_run)?;
            println!("Picture's colours: {}", d.swatches.join(" "));
            println!(
                "{} theme, {}",
                if d.light { "light" } else { "dark" },
                d.mood.as_str()
            );
            if dry_run {
                print!("{}", d.colors_toml);
            } else {
                println!(
                    "Saved {} (edit its colors.toml freely). Use it: palette set {name}",
                    paths.themes().join(&name).display()
                );
            }
        }
        Command::Schedule { command } => schedule(&paths, command)?,
        Command::Catalog { search } => catalog(&paths, search.as_deref()),
        Command::Install { source, name } => {
            println!("Downloading…");
            let done = palette::install::install(&paths, &source, name.as_deref())?;
            print_installed(&done);
            println!("Use it: palette set {}", done.name);
        }
        Command::Update { names } => {
            let names = if names.is_empty() {
                palette::install::installed(&paths)
                    .into_iter()
                    .map(|(n, _)| n)
                    .collect()
            } else {
                names
            };
            if names.is_empty() {
                println!("No installed themes (install some: palette catalog)");
            }
            for name in names {
                match palette::install::update(&paths, &name) {
                    Ok(done) => print_installed(&done),
                    Err(e) => println!("{name}: {e:#}"),
                }
            }
        }
        Command::Tools { command } => tools(&paths, command)?,
        Command::Remove { name } => {
            let done = palette::install::remove(&paths, &name)?;
            if done.is_warning() {
                eprintln!("warning: {}", done.message(&name));
            } else {
                println!("{}", done.message(&name));
            }
        }
        Command::Check { theme, fix: false } => check(&paths, theme.as_deref())?,
        Command::Check { theme, fix: true } => colours(&paths, theme, Vec::new(), None, true)?,
        Command::Colours {
            theme,
            set,
            reset,
            fix,
        } => colours(&paths, theme, set, reset, fix)?,
        Command::Desktop { tool, command } => desktop(&paths, tool.as_deref(), command)?,
        Command::Fav { command } => fav(&paths, command.unwrap_or(FavCommand::List))?,
        Command::Recent => {
            let current = state::current_theme(&paths)?;
            for name in palette::favourites::recents(&paths) {
                let mark = if current.as_deref() == Some(name.as_str()) {
                    "*"
                } else {
                    " "
                };
                println!("{mark} {name}");
            }
        }
    }
    Ok(())
}

fn cycle(favourites: bool, all: bool) -> Option<palette::favourites::Cycle> {
    use palette::favourites::Cycle;
    match (favourites, all) {
        (true, _) => Some(Cycle::Favourites),
        (_, true) => Some(Cycle::All),
        _ => None,
    }
}

fn check(paths: &Paths, theme: Option<&str>) -> Result<()> {
    let r = palette::check::run(paths, theme)?;
    for issue in &r.issues {
        println!(
            "!! {:<18} on {:<18} {:>4.1}:1  needs {}  ({})",
            issue.fg, issue.bg, issue.ratio, issue.min, issue.used_for
        );
        println!("   {} → {}", issue.fg_hex, issue.fix());
    }
    if r.issues.is_empty() {
        println!("{}", r.verdict());
        Ok(())
    } else {
        bail!("{}", r.verdict())
    }
}

fn colours(
    paths: &Paths,
    theme: Option<String>,
    set: Vec<String>,
    reset: Option<Option<String>>,
    fix: bool,
) -> Result<()> {
    use palette::edits::{self, Edit};
    let name = match theme {
        Some(n) => n,
        None => state::current_theme(paths)?
            .context("no theme is active yet: name one (`palette colours <theme>`)")?,
    };
    let edit = if fix {
        Some(Edit::FixContrast)
    } else if let Some(key) = reset {
        Some(Edit::Reset(key))
    } else if !set.is_empty() {
        let pairs = set
            .iter()
            .map(|kv| {
                kv.split_once('=')
                    .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                    .with_context(|| format!("`{kv}`: write it as KEY=#RRGGBB"))
            })
            .collect::<Result<Vec<_>>>()?;
        Some(Edit::Set(pairs))
    } else {
        None
    };
    if let Some(edit) = edit {
        let (changed, switched) = palette::edit_colours(paths, &name, &edit)?;
        if changed.is_empty() {
            println!("{name}: nothing to change");
        }
        for (key, value) in &changed {
            println!("{name}: {key} = {value}");
        }
        if let Some(s) = switched {
            print_effects(&s.effects);
        }
        return Ok(());
    }
    let sheet = edits::sheet(paths, &name)?;
    println!("{name} ({}):", sheet.mode.as_str());
    for s in &sheet.colours {
        if s.edited() {
            println!("  {:<20} {}  * yours (was {})", s.key, s.value, s.original);
        } else {
            println!("  {:<20} {}", s.key, s.value);
        }
    }
    println!("{}", sheet.report.verdict());
    if !sheet.report.issues.is_empty() {
        println!(
            "`palette colours {name} --fix` applies the suggestions (`palette check {name}` lists them)"
        );
    }
    Ok(())
}

fn desktop(paths: &Paths, tool: Option<&str>, command: Option<DesktopCommand>) -> Result<()> {
    use palette::desktop::{self, Tool};
    let Some(tool) = tool else {
        for tool in Tool::ALL {
            let s = desktop::status(paths, tool);
            let state = match (s.installed, s.managed) {
                (_, true) => "managed by Palette",
                (true, false) => "yours (Palette doesn't write it)",
                (false, false) => "not installed",
            };
            println!("{:<8} {:<13} {state}", tool.name(), tool.app());
        }
        println!("macos    Dock and menu bar (palette desktop macos)");
        println!("Show one: palette desktop bar | windows | borders | macos | fonts");
        return Ok(());
    };
    if tool == "fonts" || tool == "macos" {
        return desktop_mac(paths, tool, command);
    }
    let tool = Tool::parse(tool)?;
    let applied = match command {
        None => {
            let s = desktop::status(paths, tool);
            if s.managed {
                println!(
                    "# Palette manages {} ({} backups kept).",
                    tool.app(),
                    s.backups.len()
                );
            } else {
                println!(
                    "# Palette doesn't manage {} yet: these are the settings it would start from.\n# `palette desktop {} manage` backs up your config and lets Palette write it.",
                    tool.app(),
                    tool.name()
                );
            }
            print!("{}", toml::to_string(&desktop::get(paths, tool)?)?);
            return Ok(());
        }
        Some(DesktopCommand::Backups) => {
            let s = desktop::status(paths, tool);
            if let Some(o) = &s.original {
                println!("original  {}", o.display());
            }
            for b in &s.backups {
                println!("{}", b.display());
            }
            return Ok(());
        }
        Some(DesktopCommand::Restore { which }) => desktop::restore(paths, tool, &which)?,
        Some(DesktopCommand::Preset { name }) => {
            if tool != Tool::Prompt {
                bail!("presets are Starship's: palette desktop prompt preset …");
            }
            match name {
                Some(name) => desktop::prompt_preset(paths, &name)?,
                None => {
                    for p in desktop::prompt::presets() {
                        println!("{p}");
                    }
                    println!("Use one: palette desktop prompt preset <name>");
                    return Ok(());
                }
            }
        }
        Some(DesktopCommand::Preview) => {
            if tool != Tool::Prompt {
                bail!("only the prompt has a preview: palette desktop prompt preview");
            }
            let p: desktop::prompt::Prompt = desktop::get(paths, tool)?.try_into()?;
            let mut out = String::new();
            for s in desktop::prompt::preview(paths, &p)? {
                out.push_str(&ansi(&s));
            }
            println!("{out}\u{1b}[0m");
            return Ok(());
        }
        Some(DesktopCommand::Bind { key, command, mode }) => {
            let run: Vec<String> = command
                .join(" ")
                .split(';')
                .map(|c| c.trim().to_string())
                .filter(|c| !c.is_empty())
                .collect();
            bind(paths, tool, |list| {
                list.retain(|s| !(s.mode == mode && s.key == key));
                list.push(desktop::windows::Shortcut { mode, key, run });
                Ok(())
            })?
        }
        Some(DesktopCommand::Unbind { key, mode }) => bind(paths, tool, |list| {
            let before = list.len();
            list.retain(|s| !(s.mode == mode && s.key == key));
            if list.len() == before {
                bail!("{key} isn't a shortcut in {mode} mode");
            }
            Ok(())
        })?,
        Some(DesktopCommand::Manage) => desktop::manage(paths, tool)?,
        Some(DesktopCommand::Release) => desktop::release(paths, tool)?,
        Some(DesktopCommand::Apply) => desktop::set(paths, tool, desktop::get(paths, tool)?)?,
        Some(DesktopCommand::Set { pairs }) => {
            let pairs = pairs
                .iter()
                .map(|kv| {
                    kv.split_once('=')
                        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                        .with_context(|| format!("`{kv}`: write it as KEY=VALUE"))
                })
                .collect::<Result<Vec<_>>>()?;
            desktop::set_keys(paths, tool, &pairs)?
        }
    };
    for w in &applied.warnings {
        eprintln!("note: {w}");
    }
    if let Some(b) = &applied.backup {
        println!("backup   {}", b.display());
    }
    for f in &applied.written {
        println!("wrote    {}", f.display());
    }
    print_effects(&[(tool.program().to_string(), applied.reload.clone())]);
    Ok(())
}

/// A preview span as terminal colours again.
fn ansi(s: &palette::desktop::prompt::Span) -> String {
    let code = |c: &str, base: u8| -> String {
        const NAMES: [&str; 8] = [
            "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
        ];
        if let Some(hex) = c.strip_prefix('#') {
            let n = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0);
            format!("{};2;{};{};{}", base + 8, n(0), n(2), n(4))
        } else if let Some(i) = NAMES
            .iter()
            .position(|n| c.strip_prefix("bright_").unwrap_or(c) == *n)
        {
            let bright = if c.starts_with("bright_") { 60 } else { 0 };
            (base + bright + i as u8).to_string()
        } else {
            (base + 9).to_string()
        }
    };
    format!(
        "\u{1b}[0;{}{};{}m{}",
        if s.bold { "1;" } else { "" },
        code(&s.fg, 30),
        code(&s.bg, 40),
        s.text
    )
}

/// Change the windows shortcuts list, then save it like any setting.
fn bind(
    paths: &Paths,
    tool: palette::desktop::Tool,
    change: impl FnOnce(&mut Vec<palette::desktop::windows::Shortcut>) -> Result<()>,
) -> Result<palette::desktop::Applied> {
    use palette::desktop::{self, Tool, windows::Windows};
    if tool != Tool::Windows {
        bail!("shortcuts are AeroSpace's: palette desktop windows bind …");
    }
    let mut w: Windows = desktop::get(paths, tool)?.try_into()?;
    change(w.shortcuts.get_or_insert_with(Vec::new))?;
    desktop::set(paths, tool, toml::Value::try_from(&w)?)
}

/// `palette desktop fonts [family]` and `palette desktop macos [set k=v …]`.
fn desktop_mac(paths: &Paths, what: &str, command: Option<DesktopCommand>) -> Result<()> {
    use palette::desktop::macos;
    use palette::system::fonts;
    if what == "fonts" {
        if command.is_some() {
            bail!("palette desktop fonts lists the installed fonts and their styles");
        }
        for family in fonts::families() {
            println!("{family}: {}", fonts::styles(&family).join(", "));
        }
        return Ok(());
    }
    match command {
        None => {
            print!("{}", toml::to_string(&macos::read())?);
            println!(
                "# Change: palette desktop macos set dock_autohide=true menu_bar_autohide=false"
            );
        }
        Some(DesktopCommand::Set { pairs }) => {
            let mut value = toml::Value::try_from(macos::read())?;
            let table = value.as_table_mut().context("settings")?;
            for kv in &pairs {
                let (k, v) = kv
                    .split_once('=')
                    .with_context(|| format!("`{kv}`: write it as KEY=VALUE"))?;
                if !table.contains_key(k.trim()) {
                    bail!("`{k}` is not a macos setting (see `palette desktop macos`)");
                }
                let v = v.trim();
                let parsed = v
                    .parse::<bool>()
                    .map(toml::Value::Boolean)
                    .or_else(|_| v.parse::<i64>().map(toml::Value::Integer))
                    .unwrap_or_else(|_| toml::Value::String(v.to_string()));
                table.insert(k.trim().to_string(), parsed);
            }
            let new: macos::MacBits = value.try_into()?;
            print_effects(&[("macOS".to_string(), macos::set(paths, &new)?)]);
        }
        Some(_) => bail!("palette desktop macos [set KEY=VALUE …]"),
    }
    Ok(())
}

fn fav(paths: &Paths, command: FavCommand) -> Result<()> {
    use palette::favourites as f;
    match command {
        FavCommand::List => {
            let favs = f::list(paths)?;
            if favs.is_empty() {
                println!("No favourites yet. Add one: palette fav add <theme>");
            }
            for name in &favs {
                println!("★ {name}");
            }
            let cycle = palette::settings::load(paths)?.next_cycles;
            println!("`palette next` steps through: {}", cycle.as_str());
        }
        FavCommand::Add { theme } => {
            if f::add(paths, &theme)? {
                println!(
                    "★ {theme} added ({} of {})",
                    f::list(paths)?.len(),
                    f::MAX_FAVOURITES
                );
            } else {
                println!("{theme} already is a favourite");
            }
        }
        FavCommand::Remove { theme } => {
            if f::remove(paths, &theme)? {
                println!("{theme} is no longer a favourite");
            } else {
                println!("{theme} wasn't a favourite");
            }
        }
        FavCommand::Cycle { what } => {
            f::set_cycle(paths, f::Cycle::parse(&what)?)?;
            println!("`palette next`/`prev` and the bar pill now step through: {what}");
        }
    }
    Ok(())
}

fn list(paths: &Paths) -> Result<()> {
    let current = state::current_theme(paths)?;
    // Colours only for a terminal, and never when NO_COLOR is set (no-color.org).
    let colour = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    for (name, origin) in theme::list(paths)? {
        let mark = if current.as_deref() == Some(name.as_str()) {
            "*"
        } else {
            " "
        };
        let origin = match origin {
            Origin::Bundled => "bundled",
            Origin::User => "user",
        };
        let loaded = theme::load(paths, &name);
        let mode = match &loaded {
            Ok(t) if t.colors.mode == Mode::Light => "light",
            Ok(_) => "",
            Err(_) => "invalid",
        };
        let strip = match (&loaded, colour) {
            (Ok(t), true) => format!("{}  ", swatches(t)),
            _ => String::new(),
        };
        println!("{mark} {strip}{name:<24} {origin:<8} {mode}");
    }
    Ok(())
}

/// A strip of 24-bit colour blocks (terminal escape codes).
fn swatches(theme: &Theme) -> String {
    const KEYS: [&str; 9] = [
        "background",
        "foreground",
        "accent",
        "red",
        "yellow",
        "green",
        "cyan",
        "blue",
        "magenta",
    ];
    KEYS.iter()
        .filter_map(|k| {
            let hex = theme.colors.get(k).trim_start_matches('#');
            let channel = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
            let (r, g, b) = (channel(0)?, channel(2)?, channel(4)?);
            Some(format!("\x1b[48;2;{r};{g};{b}m  \x1b[0m"))
        })
        .collect()
}

fn apps(paths: &Paths, command: Option<AppsCommand>) -> Result<()> {
    let (names, on) = match command {
        None => {
            for app in palette::apps(paths)? {
                let state = if app.on { "on " } else { "off" };
                println!("  {state}  {:<16} {}", app.name, app.detail);
            }
            println!("Change with `palette apps off <app>` / `palette apps on <app>`.");
            return Ok(());
        }
        Some(AppsCommand::On { apps }) => (apps, true),
        Some(AppsCommand::Off { apps }) => (apps, false),
    };
    for name in names {
        let (report, reloads) = palette::set_app(paths, &name, on)?;
        let verb = if on { "on" } else { "off" };
        println!("{name}: {verb}");
        for item in report.items.iter().filter(|i| i.step.writes()) {
            let what = if on { "wired" } else { "restored" };
            let path = item
                .path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            println!("  {what} {path}");
        }
        if let Some(dir) = &report.backup {
            println!("  originals backed up to {}", dir.display());
        }
        print_effects(&reloads);
    }
    Ok(())
}

fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

fn export(
    paths: &Paths,
    dir: &std::path::Path,
    no_wallpapers: bool,
    no_configs: bool,
) -> Result<()> {
    let options = palette::sync::ExportOptions {
        wallpapers: no_wallpapers.then_some(false),
        configs: no_configs.then_some(false),
    };
    let r = palette::sync::export(paths, dir, &options)?;
    if r.created_sync {
        println!(
            "Created {} listing which of your configs to take along; edit it any time.",
            paths.root().join("sync.toml").display()
        );
    }
    println!("Exported to {}", r.dir.display());
    println!(
        "  palette   {} files, {} (themes, wallpapers, settings)",
        r.palette.files,
        mb(r.palette.bytes)
    );
    for (rel, copied) in &r.configs {
        println!("  config    ~/{rel} ({} files)", copied.files);
    }
    for rel in &r.missing {
        println!("  skipped   ~/{rel} (not on this Mac)");
    }
    println!("On the other Mac: palette import <folder> --dry-run, then without --dry-run.");
    Ok(())
}

fn import(paths: &Paths, dir: &std::path::Path, dry_run: bool) -> Result<()> {
    let (r, wired, effects) = palette::import(paths, dir, dry_run)?;
    let verb = if dry_run { "Would write" } else { "Wrote" };
    println!(
        "{verb} {} files ({} replace existing ones{})",
        r.written.len(),
        r.replaced.len(),
        if dry_run { ", backed up first" } else { "" }
    );
    for rel in r
        .replaced
        .iter()
        .filter(|p| !p.starts_with(".config/palette"))
        .take(40)
    {
        println!("  replace  ~/{}", rel.display());
    }
    if let Some(dir) = &r.backup {
        println!("Replaced files backed up to {}", dir.display());
    }
    if !r.relocated.is_empty() {
        println!(
            "Rewrote {} → {} in {} file(s)",
            r.old_home,
            paths.home().display(),
            r.relocated.len()
        );
    }
    let apps = palette::sync::imported_apps(&r);
    if !apps.is_empty() {
        println!("Configs for: {}", apps.join(", "));
    }
    if dry_run {
        println!("Dry run: nothing changed.");
        return Ok(());
    }
    if let Some(theme) = &r.theme {
        println!("Theme: {theme}");
    }
    if let Some(report) = wired {
        let added = report.items.iter().filter(|i| i.step.writes()).count();
        if added > 0 {
            println!("Wired {added} more config(s) on this Mac.");
        }
    }
    print_effects(&effects);
    Ok(())
}

fn hotkey(paths: &Paths, combo: Option<String>) -> Result<()> {
    let mut settings = palette::settings::load(paths)?;
    match combo {
        None => println!("{}", settings.hotkey),
        Some(combo) => {
            settings.hotkey = palette::settings::normalise_hotkey(&combo)?;
            palette::settings::save(paths, &settings)?;
            println!(
                "Hotkey set to {} (the Palette app picks it up when it next opens a window)",
                settings.hotkey
            );
        }
    }
    Ok(())
}

fn doctor(paths: &Paths) -> Result<()> {
    let rows = doctor::run(paths)?;
    for row in &rows {
        let sign = match (row.status, row.needs_fix()) {
            (Status::Off, _) => "-  ",
            (_, true) => "!! ",
            (Status::Ok, false) => "ok ",
            (Status::Fix, false) => "!! ",
        };
        println!("  {:<12} {sign} {}", row.name, row.detail);
        for note in &row.notes {
            let mark = match (note.ok, note.info) {
                (false, _) => "!",
                (true, true) => "i",
                _ => " ",
            };
            println!("  {:<12}   {mark} {}", "", note.text);
        }
    }
    let problems = rows.iter().filter(|r| r.needs_fix()).count();
    if problems > 0 {
        bail!("{problems} thing(s) to fix, see !! above");
    }
    println!("All good.");
    Ok(())
}

fn report(s: &Switch, paths: &Paths) {
    for w in &s.warnings {
        eprintln!("warning: {w}");
    }
    if s.dry_run {
        println!("Would switch to {} (dry run, nothing written):", s.theme);
    } else {
        println!("Switched to {} -> {}", s.theme, paths.current().display());
    }
    for (file, change) in &s.changes {
        let (sign, word) = match change {
            Change::New => ('+', "new"),
            Change::Changed => ('~', "changed"),
            Change::Unchanged => ('=', "unchanged"),
        };
        let note = if s.overrides.contains(file) {
            ", from theme folder"
        } else {
            ""
        };
        println!("  {sign} {file:<18} {word}{note}");
    }
    print_effects(&s.effects);
}

fn print_effects(effects: &[Effect]) {
    let shown = effects.iter().filter(|(_, o)| {
        *o != Outcome::Skipped(effects::NOT_INSTALLED.into())
            && *o != Outcome::Skipped(effects::OFF.into())
    });
    for (what, outcome) in shown {
        let line = match outcome {
            Outcome::Done(m) => format!("ok  {m}"),
            Outcome::Unchanged(m) => format!("=   {m}"),
            Outcome::Skipped(m) => format!("-   {m}"),
            Outcome::Failed(m) => format!("!!  failed: {m}"),
        };
        println!("  {what:<12} {line}");
    }
}

fn init(paths: &Paths, remove: bool, dry_run: bool) -> Result<()> {
    if !remove && !dry_run && state::current_theme(paths)?.is_none() {
        bail!("set a theme first (`palette set <theme>`), so the files your configs import exist");
    }
    let (report, reloads) = palette::init(paths, remove, dry_run)?;
    if dry_run {
        println!("Dry run, no file touched:");
    }
    for item in &report.items {
        let path = item
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let what = match (&item.step, dry_run) {
            (Step::Add, true) => "would add import line".to_string(),
            (Step::Add, false) => "added import line".to_string(),
            (Step::Set, true) => "would point setting at palette".to_string(),
            (Step::Set, false) => "pointed setting at palette".to_string(),
            (Step::Restore, true) => "would restore your setting".to_string(),
            (Step::Restore, false) => "restored your setting".to_string(),
            (Step::Remove, true) => "would remove palette's lines".to_string(),
            (Step::Remove, false) => "removed palette's lines".to_string(),
            (Step::AlreadyWired, _) => "already wired".to_string(),
            (Step::NotWired, _) => "not wired".to_string(),
            (Step::NotFound, _) => "no config found, skipped".to_string(),
            (Step::Off, _) => "off (your choice), skipped".to_string(),
            (Step::Manual(why), _) => format!("do by hand: {why}"),
        };
        println!("  {:<11} {what:<30} {path}", item.target);
        if let Some(note) = &item.note {
            println!("  {:<11} {note}", "");
        }
    }
    if let Some(dir) = &report.backup {
        println!("Originals backed up to {} (last 5 kept)", dir.display());
    }
    if !reloads.is_empty() {
        println!("Apps:");
        print_effects(&reloads);
    }
    Ok(())
}

fn catalog(paths: &Paths, search: Option<&str>) {
    let installed: Vec<String> = palette::install::installed(paths)
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    let q = search.unwrap_or_default().to_ascii_lowercase();
    for e in palette::catalog::entries()
        .iter()
        .filter(|e| e.name.contains(&q) || e.title.to_ascii_lowercase().contains(&q))
    {
        let mood = palette::colour::analyse(&e.theme(paths)).mood;
        let mark = if installed.contains(&e.name) {
            "*"
        } else {
            " "
        };
        println!(
            "{mark} {:<22} {:<6} {:<11} {} by {}",
            e.name,
            e.colors.mode.as_str(),
            mood.as_str(),
            swatches(&e.theme(paths)),
            e.author
        );
    }
    println!("\n* installed. Install one: palette install <name>");
}

fn print_installed(done: &palette::install::Installed) {
    println!(
        "Installed {} ({} wallpaper(s), colours from {}) from {}",
        done.name, done.wallpapers, done.colours_from, done.source
    );
    if !done.filled.is_empty() {
        println!("  filled in missing colours: {}", done.filled.join(", "));
    }
}

fn bg(paths: &Paths, command: BgCommand) -> Result<()> {
    match command {
        BgCommand::List => {
            let theme = palette::current(paths)?;
            let images = theme.backgrounds()?;
            if images.is_empty() {
                println!(
                    "{} has no wallpapers. Add some: palette bg add <image>…",
                    theme.name
                );
            }
            let current = effects::current_wallpaper(paths);
            let default = effects::default_wallpaper(paths, &theme);
            for image in images {
                let mark = if current.as_ref() == Some(&image) {
                    "*"
                } else {
                    " "
                };
                let def = if default.as_ref() == Some(&image) {
                    "d"
                } else {
                    " "
                };
                println!("{mark}{def} {}", image.display());
            }
        }
        BgCommand::Next | BgCommand::Prev => {
            let by = if matches!(command, BgCommand::Next) {
                1
            } else {
                -1
            };
            let theme = palette::current(paths)?;
            print_effects(&[(
                "wallpaper".into(),
                effects::step_wallpaper(paths, &theme, by)?,
            )]);
        }
        BgCommand::Default { name, clear, theme } => {
            let theme = match theme {
                Some(t) => theme::load(paths, &t)?,
                None => palette::current(paths)?,
            };
            let name = if clear { None } else { name };
            effects::set_default_wallpaper(paths, &theme, name.as_deref())?;
            match name {
                Some(n) => println!("{} now starts with {n}", theme.name),
                None => println!("{} starts with its first wallpaper again", theme.name),
            }
        }
        BgCommand::Set { name, theme } => {
            let theme = match theme {
                Some(t) => theme::load(paths, &t)?,
                None => palette::current(paths)?,
            };
            let file = effects::wallpaper_named(&theme, &name)?;
            print_effects(&[("wallpaper".into(), effects::set_wallpaper(paths, &file)?)]);
        }
        BgCommand::Remove { name, theme } => {
            let theme = match theme {
                Some(t) => theme::load(paths, &t)?,
                None => palette::current(paths)?,
            };
            let file = effects::remove_wallpaper(&theme, &name)?;
            println!("Removed {} from {}", file.display(), theme.name);
        }
        BgCommand::Recolor {
            images,
            theme,
            engine,
            apply,
            strength,
        } => {
            let theme = match theme {
                Some(name) => theme::load(paths, &name)?,
                None => palette::current(paths)?,
            };
            let engine = palette::studio::Engine::parse(&engine)?;
            let strength = palette::studio::Strength::parse(&strength)?;
            let mut made = Vec::new();
            for image in &images {
                println!("Recolouring {}…", image.display());
                made.push(palette::studio::recolour(&theme, image, engine, strength)?);
            }
            for file in &made {
                println!("Added {}", file.display());
            }
            if apply {
                print_effects(&[("wallpaper".into(), effects::set_wallpaper(paths, &made[0])?)]);
            }
        }
        BgCommand::Generate {
            style,
            theme,
            apply,
            seed,
        } => {
            let theme = match theme {
                Some(name) => theme::load(paths, &name)?,
                None => palette::current(paths)?,
            };
            let file = palette::studio::generate(&theme, &style, seed)?;
            println!("Added {}", file.display());
            if apply {
                print_effects(&[("wallpaper".into(), effects::set_wallpaper(paths, &file)?)]);
            }
        }
        BgCommand::Add { images, theme } => {
            let theme = match theme {
                Some(name) => theme::load(paths, &name)?,
                None => palette::current(paths)?,
            };
            let had_none = theme.backgrounds()?.is_empty();
            let added = effects::add_wallpapers(&theme, &images)?;
            println!(
                "Added {} wallpaper(s) to {} ({})",
                added.len(),
                theme.name,
                theme.backgrounds_dir().display()
            );
            // First wallpapers for the active theme: use one straight away.
            let is_active = state::current_theme(paths)?.as_deref() == Some(theme.name.as_str());
            if had_none && is_active {
                print_effects(&[(
                    "wallpaper".into(),
                    effects::set_wallpaper(paths, &added[0])?,
                )]);
            }
        }
        BgCommand::Library { command } => library(paths, command.unwrap_or(LibraryCommand::List))?,
        BgCommand::Get {
            url,
            folder,
            pick,
            all,
            into,
            theme,
            recolor,
        } => get_wallpapers(
            paths,
            &url,
            (folder.as_deref(), pick.as_deref(), all),
            into.as_deref(),
            theme.as_deref(),
            recolor,
        )?,
    }
    Ok(())
}

fn library(paths: &Paths, command: LibraryCommand) -> Result<()> {
    use palette::library as lib;
    match command {
        LibraryCommand::List => {
            let library = lib::list(paths)?;
            println!(
                "{} ({} picture{}, {})",
                library.dir.display(),
                library.items.len(),
                if library.items.len() == 1 { "" } else { "s" },
                lib::human_size(library.bytes)
            );
            if library.items.is_empty() {
                println!(
                    "Empty. Add pictures: palette bg library add <images…>, \
                     or palette bg get <link>"
                );
            }
            for item in &library.items {
                println!("{:>9}  {}", lib::human_size(item.bytes), item.name);
            }
        }
        LibraryCommand::Add { images, into } => {
            let added = lib::add(paths, &images, into.as_deref())?;
            for file in &added {
                println!("Added {}", file.display());
            }
        }
        LibraryCommand::Remove { name } => {
            let file = lib::remove(paths, &name)?;
            println!("Removed {}", file.display());
        }
        LibraryCommand::Use { name } => {
            print_effects(&[("wallpaper".into(), lib::use_now(paths, &name)?)]);
        }
        LibraryCommand::Theme {
            name,
            theme,
            recolor,
        } => {
            let theme = match theme {
                Some(t) => theme::load(paths, &t)?,
                None => palette::current(paths)?,
            };
            let file = lib::add_to_theme(paths, &name, &theme, recolor)?;
            println!("Added {} to {}", file.display(), theme.name);
        }
        LibraryCommand::NewTheme {
            name,
            theme_name,
            light,
            dark,
            dry_run,
        } => {
            let mode = match (light, dark) {
                (true, _) => Some(true),
                (_, true) => Some(false),
                _ => None,
            };
            let drafted = lib::new_theme(paths, &name, &theme_name, mode, dry_run)?;
            if dry_run {
                print!("{}", drafted.colors_toml);
            } else {
                println!(
                    "Saved {theme_name} ({}, {}), with {name} as its wallpaper. \
                     Try it: palette set {theme_name}",
                    if drafted.light { "light" } else { "dark" },
                    drafted.mood.as_str()
                );
            }
        }
        LibraryCommand::Open => {
            let dir = lib::dir(paths);
            lib::reveal_folder(&dir)?;
            println!("Opened {}", dir.display());
        }
    }
    Ok(())
}

/// `palette bg get <url>`: list a repo's folders, or download a choice.
fn get_wallpapers(
    paths: &Paths,
    url: &str,
    (folder, pick, all): (Option<&str>, Option<&str>, bool),
    into: Option<&str>,
    theme: Option<&str>,
    recolor: bool,
) -> Result<()> {
    use palette::fetch::{self, Link};
    use palette::library::human_size;
    let theme = theme.map(|t| theme::load(paths, t)).transpose()?;
    let link = Link::parse(url)?;
    let listing = fetch::list(&link)?;
    let chosen = match link {
        Link::Image { .. } => listing.files.clone(),
        Link::GitHub { .. } => {
            let total: u64 = listing.files.iter().map(|f| f.bytes).sum();
            let folders = fetch::folders(&listing.files);
            let choosing = folder.is_some() || pick.is_some() || all;
            if !choosing || listing.files.is_empty() {
                println!(
                    "{}: {} picture{}, {} in {} folder{}",
                    link.describe(),
                    listing.files.len(),
                    if listing.files.len() == 1 { "" } else { "s" },
                    human_size(total),
                    folders.len(),
                    if folders.len() == 1 { "" } else { "s" },
                );
                for f in &folders {
                    let name = if f.path.is_empty() { "(top)" } else { &f.path };
                    println!("{:>6}  {:>9}  {name}", f.count, human_size(f.bytes));
                }
            }
            if listing.truncated {
                let example = folders
                    .iter()
                    .find(|f| !f.path.is_empty())
                    .and_then(|f| link.folder_link(f.path.split('/').next().unwrap_or("")));
                println!(
                    "\nGitHub listed only part of this repo (it's very big). Paste a folder \
                     link instead{}",
                    example.map(|e| format!(", e.g. {e}")).unwrap_or_default()
                );
            }
            if listing.files.is_empty() {
                bail!("no pictures (png, jpg, jpeg, webp, heic, tiff) there");
            }
            if !choosing {
                println!(
                    "\nDownload with --folder <path>, --pick '<pattern>' or --all \
                     (at most {} pictures / {} at a time). They go to your wallpaper library.",
                    fetch::MAX_FILES,
                    human_size(fetch::MAX_TOTAL)
                );
                return Ok(());
            }
            fetch::select(&listing.files, folder, pick, all)?
        }
    };
    println!(
        "Downloading {} picture{}…",
        chosen.len(),
        if chosen.len() == 1 { "" } else { "s" }
    );
    let done = fetch::download(paths, &link, &chosen, into)?;
    for (path, why) in &done.skipped {
        println!("skipped {path}: {why}");
    }
    println!(
        "Saved {} picture{} ({}) to {}",
        done.saved.len(),
        if done.saved.len() == 1 { "" } else { "s" },
        human_size(done.bytes),
        done.dir.display()
    );
    if let Some(theme) = theme {
        for file in &done.saved {
            let added = palette::library::copy_to_theme(file, &theme, recolor)?;
            println!("Added {} to {}", added.display(), theme.name);
        }
    }
    if done.saved.is_empty() {
        bail!("nothing was saved");
    }
    Ok(())
}

fn schedule(paths: &Paths, command: Option<ScheduleCommand>) -> Result<()> {
    use palette::schedule as sch;
    match command {
        None => schedule_show(paths)?,
        Some(ScheduleCommand::On {
            day,
            night,
            city,
            lat,
            lon,
            at,
        }) => {
            let update = sch::Update {
                day,
                night,
                city,
                lat,
                lon,
                at,
            };
            let result = sch::enable(paths, update)?;
            println!("Schedule on.");
            schedule_result(&result, paths);
            println!();
            schedule_show(paths)?;
        }
        Some(ScheduleCommand::Off) => {
            sch::disable(paths)?;
            println!("Schedule off. Your day and night choices are kept.");
        }
        Some(ScheduleCommand::Run) => schedule_result(&sch::apply_now(paths)?, paths),
        Some(ScheduleCommand::Cities { search }) => {
            let hits = sch::search_cities(search.as_deref().unwrap_or(""));
            if hits.is_empty() {
                println!("No city matches. Use --lat and --lon instead.");
            }
            for c in hits {
                println!(
                    "{:<20} {:<18} {:>7.2} {:>8.2}",
                    c.name, c.country, c.lat, c.lon
                );
            }
            if let Some(c) = sch::suggest_city() {
                println!("\nYour Mac's time zone suggests: {}", c.name);
            }
        }
    }
    Ok(())
}

#[derive(Subcommand)]
enum ToolsCommand {
    /// Add (or with --remove, take away) a small Palette-themed starter config
    ///
    /// Only for a tool with no config at all; never overwrites yours. --remove
    /// moves the config to the Trash, only while it is still exactly what Palette wrote.
    Starter {
        /// sketchybar, borders or aerospace
        tool: String,
        /// Move the starter config Palette added to the Trash
        #[arg(long)]
        remove: bool,
    },
}

fn tools(paths: &Paths, command: Option<ToolsCommand>) -> Result<()> {
    use palette::tools::{self, Starter};
    match command {
        None => {
            let status = tools::status(paths);
            if !status.homebrew {
                println!(
                    "Homebrew isn't installed. Get it from {} first (Palette never installs it).\n",
                    tools::HOMEBREW_SITE
                );
            }
            for (group, title) in [("look", "The look"), ("wallpaper", "Wallpaper recolourer")] {
                println!("{title}");
                for t in status.tools.iter().filter(|t| t.group == group) {
                    let mark = if t.installed {
                        "installed"
                    } else {
                        "not installed"
                    };
                    println!("  {} ({}): {mark}. {}", t.name, t.id, t.what);
                    for (label, commands) in [("install", &t.install), ("remove", &t.remove)] {
                        for c in commands {
                            println!("    {label}:  {c}");
                        }
                    }
                    match t.starter {
                        Starter::Available => println!(
                            "    starter: none yet. Add one with `palette tools starter {}`",
                            t.id
                        ),
                        Starter::HasConfig => {
                            println!("    starter: not offered, you already have a config")
                        }
                        Starter::Added { unchanged: true } => println!(
                            "    starter: added by Palette, unchanged. Remove with `palette tools starter {} --remove`",
                            t.id
                        ),
                        Starter::Added { unchanged: false } => {
                            println!(
                                "    starter: added by Palette, since edited by you (yours now)"
                            )
                        }
                        Starter::None => {}
                    }
                }
                println!();
            }
            println!("Run the commands above in a terminal yourself: Palette never runs Homebrew.");
        }
        Some(ToolsCommand::Starter { tool, remove: true }) => {
            let trashed = tools::remove_starter(paths, &tool)?;
            println!(
                "Moved the {tool} starter config to the Trash ({})",
                trashed.display()
            );
        }
        Some(ToolsCommand::Starter {
            tool,
            remove: false,
        }) => {
            let added = tools::add_starter(paths, &tool)?;
            println!("Added the {tool} starter config:");
            for f in &added.files {
                println!("  {}", f.display());
            }
            if let Some(report) = &added.wiring {
                for item in report.items.iter().filter(|i| i.step.writes()) {
                    println!("  {:<16} wired to Palette's colours", item.target);
                }
            }
            print_effects(&added.effects);
            if !added.hint.is_empty() {
                println!("{}.", added.hint);
            }
        }
    }
    Ok(())
}

fn schedule_result(result: &palette::schedule::Applied, paths: &Paths) {
    use palette::schedule::Applied;
    match result {
        Applied::Off => println!("The schedule is off."),
        Applied::Unchanged { phase, theme } => {
            println!(
                "Nothing to do: {theme} is already right for the {}.",
                phase.as_str()
            )
        }
        Applied::KeptManual { wanted, active, .. } => {
            println!("Left {active} alone (you chose it); {wanted} comes back at the next change.")
        }
        Applied::Switched { phase, switch } => {
            println!("It is {}: switching.", phase.as_str());
            report(switch, paths);
        }
    }
}

fn schedule_show(paths: &Paths) -> Result<()> {
    use palette::schedule::{Mode, Phase};
    let st = palette::schedule::which_now(paths)?;
    let s = &st.schedule;
    println!("Schedule: {}", if s.on { "on" } else { "off" });
    println!(
        "  day theme    {}",
        if s.day.is_empty() {
            "(not set)"
        } else {
            &s.day
        }
    );
    println!(
        "  night theme  {}",
        if s.night.is_empty() {
            "(not set)"
        } else {
            &s.night
        }
    );
    match s.mode {
        Mode::Fixed => println!(
            "  fixed times  day {}, night {}",
            palette::schedule::fmt_hm(s.day_at),
            palette::schedule::fmt_hm(s.night_at)
        ),
        Mode::Sun => println!(
            "  sun at       {}",
            st.place
                .as_deref()
                .unwrap_or("(no place set: --city or --lat/--lon)")
        ),
    }
    if let Some(p) = &st.problem {
        println!("  note         {p}");
        if s.mode == Mode::Sun
            && let Some(c) = palette::schedule::suggest_city()
        {
            println!("  suggestion   {} (from your time zone)", c.name);
        }
        return Ok(());
    }
    let Some(now) = &st.now else { return Ok(()) };
    match (now.polar, now.day_starts, now.night_starts) {
        (Some(Phase::Day), ..) => println!("  today        polar day: the sun doesn't set"),
        (Some(Phase::Night), ..) => println!("  today        polar night: the sun doesn't rise"),
        (None, Some(d), Some(n)) => println!(
            "  today        {} {}, {} {}",
            if s.mode == Mode::Sun {
                "sunrise"
            } else {
                "day from"
            },
            st.hm(d),
            if s.mode == Mode::Sun {
                "sunset"
            } else {
                "night from"
            },
            st.hm(n)
        ),
        _ => {}
    }
    println!("  right now    {}", now.phase.as_str());
    match now.next {
        Some((t, to)) => println!(
            "  next change  {} {} -> {}",
            st.hm(t),
            st.day_word(t),
            s.theme_for(to)
        ),
        None => println!("  next change  none within a year"),
    }
    Ok(())
}
