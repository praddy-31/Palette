//! `palette install / update / remove`: themes from the community, mostly
//! Omarchy theme repos on GitHub.
//!
//! Safety: a downloaded theme is data. Only colours, wallpapers and a preview
//! image are taken from it; config files (`ghostty.conf`, `kitty.conf`, …)
//! are ignored, because several of their settings can start programs. Only
//! regular files are copied (never symlinks), with size limits.
//!
//! Network: only here, only when asked. GitHub repos come as an archive via
//! macOS's own `curl` and `tar` (no git or Xcode tools needed); other hosts
//! need `git`. A local folder also works (tests, themes made by hand).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use crate::colour::mix;
use crate::paths::Paths;
use crate::system;
use crate::theme::{self, Origin, REQUIRED_KEYS};

/// Where an installed theme came from, kept in `themes/<name>/.palette-source`.
const SOURCE_FILE: &str = ".palette-source";
const MAX_IMAGE: u64 = 40 * 1024 * 1024;
const MAX_IMAGES: usize = 80;

#[derive(Debug, Clone)]
pub struct Installed {
    pub name: String,
    pub source: String,
    pub wallpapers: usize,
    /// Colour keys the theme didn't define, filled in from the others.
    pub filled: Vec<String>,
    /// Where the colours came from: `colors.toml` or `alacritty.toml`.
    pub colours_from: &'static str,
}

/// `palette install <catalogue name | github url | folder>`.
pub fn install(paths: &Paths, source: &str, name: Option<&str>) -> Result<Installed> {
    let url = resolve(source);
    let name = match name {
        Some(n) => n.to_string(),
        None => slug(&url),
    };
    if !theme::is_valid_name(&name) {
        bail!("`{name}` can't be a theme name: use lowercase letters, digits and `-` (--name …)");
    }
    match theme::list(paths)?.into_iter().find(|(n, _)| *n == name) {
        Some((_, Origin::Bundled)) => bail!(
            "Palette already has a built-in theme called `{name}`; install under another \
             name with --name"
        ),
        Some((_, Origin::User)) => bail!(
            "you already have a theme called `{name}` (update it with `palette update {name}`, \
             or pick another name with --name)"
        ),
        None => {}
    }
    let fetched = Fetched::get(paths, &url)?;
    let report = build(paths, &fetched.dir, &name, &url, &[])?;
    swap_in(paths, &name)?;
    Ok(report)
}

/// `palette update <name>`: download again and replace the theme's colours,
/// preview and the wallpapers it came with. Wallpapers you added stay.
pub fn update(paths: &Paths, name: &str) -> Result<Installed> {
    let dir = paths.themes().join(name);
    let source = read_source(&dir)
        .with_context(|| format!("`{name}` wasn't installed by Palette, so it can't be updated"))?;
    let fetched = Fetched::get(paths, &source.url)?;
    // Wallpapers the user added since: not in the old file list.
    let theirs: Vec<PathBuf> = list_files(&dir.join("backgrounds"))
        .into_iter()
        .filter(|p| {
            let rel = format!(
                "backgrounds/{}",
                p.file_name().unwrap_or_default().to_string_lossy()
            );
            !source.files.contains(&rel)
        })
        .collect();
    let report = build(paths, &fetched.dir, name, &source.url, &theirs)?;
    // Your colour edits stay on top of the new colours.
    let partial = paths.themes().join(format!(".{name}.partial"));
    copy_regular(
        &dir.join(crate::edits::FILE),
        &partial.join(crate::edits::FILE),
        1 << 16,
    )?;
    swap_in(paths, name)?;
    Ok(report)
}

/// Installed themes (those with a source file), sorted.
pub fn installed(paths: &Paths) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = fs::read_dir(paths.themes())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            let source = read_source(&e.path()).ok()?;
            theme::is_valid_name(&name).then_some((name, source.url))
        })
        .collect();
    out.sort();
    out
}

/// What `remove` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Removal {
    /// An installed (downloaded) theme, deleted.
    Deleted,
    /// A theme you made yourself, moved to the Trash here.
    Trashed(PathBuf),
}

impl Removal {
    /// The sentence both front doors show.
    pub fn message(&self, name: &str) -> String {
        match self {
            Removal::Deleted => format!("Removed {name}"),
            Removal::Trashed(_) => format!(
                "{name} was made by you, not downloaded: moved to the Trash (restore it from there)"
            ),
        }
    }

    pub fn is_warning(&self) -> bool {
        matches!(self, Removal::Trashed(_))
    }
}

/// `palette remove <name>`: delete an installed theme. A theme you made
/// yourself (no source file) goes to the Trash instead of being deleted, so a
/// slip can be undone. Never the active theme, never a built-in one.
pub fn remove(paths: &Paths, name: &str) -> Result<Removal> {
    if !theme::is_valid_name(name) {
        bail!("invalid theme name `{name}`");
    }
    let dir = paths.themes().join(name);
    if !dir.is_dir() {
        let builtin = theme::list(paths)?
            .iter()
            .any(|(n, o)| n == name && *o == Origin::Bundled);
        if builtin {
            bail!("`{name}` is built into Palette, so it can't be removed");
        }
        bail!("there is no theme called `{name}` (see `palette list`)");
    }
    if crate::state::current_theme(paths)?.as_deref() == Some(name) {
        bail!("`{name}` is the active theme: switch to another one first");
    }
    let result = if read_source(&dir).is_ok() {
        fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
        Removal::Deleted
    } else {
        Removal::Trashed(move_to_trash(paths, &dir)?)
    };
    let mut settings = crate::settings::load(paths)?;
    if settings.wallpapers.remove(name).is_some() {
        crate::settings::save(paths, &settings)?;
    }
    Ok(result)
}

/// Move a file or folder into `~/.Trash`, adding a number if the name is
/// taken. A plain rename is used instead of asking Finder (`osascript`): it
/// needs no Automation permission and works from the app, the CLI and tests;
/// the cost is that Finder's "Put Back" doesn't know where it came from, so
/// restoring means dragging it back. (`rename` works because `~/.config` and
/// `~/.Trash` are on the same disk.) Also used for starter configs.
pub fn move_to_trash(paths: &Paths, from: &Path) -> Result<PathBuf> {
    let trash = paths.home().join(".Trash");
    fs::create_dir_all(&trash).with_context(|| format!("creating {}", trash.display()))?;
    let base = from
        .file_name()
        .context("nothing to move")?
        .to_string_lossy()
        .to_string();
    let mut target = trash.join(&base);
    let mut n = 2;
    while target.symlink_metadata().is_ok() {
        target = trash.join(format!("{base} {n}"));
        n += 1;
    }
    fs::rename(from, &target).with_context(|| format!("moving {} to the Trash", from.display()))?;
    Ok(target)
}

// ---------------------------------------------------------------- sources

/// A catalogue name becomes its repo URL; anything else is used as given.
fn resolve(source: &str) -> String {
    match crate::catalog::find(source) {
        Some(entry) => entry.url.clone(),
        None => source.trim().trim_end_matches('/').to_string(),
    }
}

/// `https://github.com/someone/omarchy-aura-theme` → `aura`.
pub fn slug(url: &str) -> String {
    let last = url
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .rsplit(['/', ':'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .replace(['_', ' ', '.'], "-");
    let s = last.strip_prefix("omarchy-").unwrap_or(&last);
    let s = s
        .strip_suffix("-omarchy-theme")
        .or_else(|| s.strip_suffix("-theme"))
        .or_else(|| s.strip_suffix("-omarchy"))
        .unwrap_or(s);
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

/// A download (or local folder) ready to read, removed again when dropped.
struct Fetched {
    dir: PathBuf,
    temp: Option<PathBuf>,
}

impl Drop for Fetched {
    fn drop(&mut self) {
        if let Some(t) = &self.temp {
            let _ = fs::remove_dir_all(t);
        }
    }
}

impl Fetched {
    fn get(paths: &Paths, url: &str) -> Result<Fetched> {
        let local = Path::new(url.strip_prefix("file://").unwrap_or(url));
        if local.is_dir() {
            return Ok(Fetched {
                dir: local.to_path_buf(),
                temp: None,
            });
        }
        let temp = paths
            .root()
            .join("cache")
            .join(format!("install-{}", millis()));
        let dir = temp.join("theme");
        fs::create_dir_all(&dir)?;
        let fetched = Fetched {
            dir: dir.clone(),
            temp: Some(temp.clone()),
        };
        if let Some((owner, repo)) = github(url) {
            let archive = temp.join("theme.tar.gz");
            let archive_s = archive.to_string_lossy().to_string();
            let from = format!("https://codeload.github.com/{owner}/{repo}/tar.gz/HEAD");
            system::run(
                "curl",
                &[
                    "-fsSL",
                    "--max-time",
                    "300",
                    "--max-filesize",
                    "500000000",
                    "-o",
                    &archive_s,
                    &from,
                ],
            )
            .map_err(|e| anyhow::anyhow!("downloading {url}: {e}"))?;
            // bsdtar refuses absolute paths and `..` entries by default.
            let dir_s = dir.to_string_lossy().to_string();
            system::run(
                "tar",
                &["-xzf", &archive_s, "-C", &dir_s, "--strip-components", "1"],
            )
            .map_err(|e| anyhow::anyhow!("unpacking {url}: {e}"))?;
        } else if url.starts_with("https://") {
            if !system::has_developer_tools() {
                bail!(
                    "{url} isn't on GitHub, and downloading it needs git (Xcode command-line \
                     tools: `xcode-select --install`)"
                );
            }
            let dir_s = dir.to_string_lossy().to_string();
            system::run("git", &["clone", "--depth", "1", "--quiet", url, &dir_s])
                .map_err(|e| anyhow::anyhow!("downloading {url}: {e}"))?;
        } else {
            bail!(
                "`{url}` isn't a theme name from `palette catalog`, an https:// link or a folder"
            );
        }
        Ok(fetched)
    }
}

/// `https://github.com/owner/repo(.git)` → (owner, repo).
fn github(url: &str) -> Option<(String, String)> {
    let rest = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
        .or_else(|| url.strip_prefix("github.com/"))?;
    let mut parts = rest.trim_end_matches('/').split('/');
    let owner = parts.next()?;
    let repo = parts.next()?.trim_end_matches(".git");
    let ok = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    };
    (ok(owner) && ok(repo) && parts.next().is_none()).then(|| (owner.to_string(), repo.to_string()))
}

// ---------------------------------------------------------------- building

/// Build `themes/.<name>.partial` from a fetched folder: colours (filled in
/// where keys are missing), light.mode, preview, wallpapers. Nothing else.
fn build(paths: &Paths, from: &Path, name: &str, url: &str, keep: &[PathBuf]) -> Result<Installed> {
    let partial = paths.themes().join(format!(".{name}.partial"));
    let _ = fs::remove_dir_all(&partial);
    fs::create_dir_all(partial.join("backgrounds"))?;

    let (table, colours_from) = match read_regular(&from.join("colors.toml"), 1 << 20) {
        Some(text) => (
            text.parse::<toml::Table>()
                .with_context(|| format!("{url}: colors.toml is not valid TOML"))?,
            "colors.toml",
        ),
        None => match read_regular(&from.join("alacritty.toml"), 1 << 20) {
            Some(text) => (from_alacritty(&text)?, "alacritty.toml"),
            None => {
                bail!("{url} has no colors.toml or alacritty.toml: not a theme Palette can read")
            }
        },
    };
    let light = from.join("light.mode").is_file()
        || table.get("mode").and_then(|v| v.as_str()) == Some("light");
    let (colours, filled) = complete(&table, light)?;
    let mut files = vec!["colors.toml".to_string()];
    fs::write(partial.join("colors.toml"), colours)?;
    if light {
        fs::write(partial.join("light.mode"), "")?;
        files.push("light.mode".into());
    }
    for preview in ["preview.png", "preview.jpg"] {
        if copy_regular(&from.join(preview), &partial.join(preview), MAX_IMAGE)? {
            files.push(preview.into());
            break;
        }
    }
    let mut wallpapers = 0;
    let mut images = list_files(&from.join("backgrounds"));
    images.retain(|p| theme::is_image(p));
    for image in images.into_iter().take(MAX_IMAGES) {
        let file = image
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if copy_regular(&image, &partial.join("backgrounds").join(&file), MAX_IMAGE)? {
            files.push(format!("backgrounds/{file}"));
            wallpapers += 1;
        }
    }
    for own in keep {
        let file = own.file_name().unwrap_or_default();
        fs::copy(own, partial.join("backgrounds").join(file))?;
    }
    let source = format!(
        "# Installed by Palette. `palette update {name}` downloads it again.\n\
         url = {}\ninstalled = {}\nfiles = {}\n",
        toml::Value::String(url.into()),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        toml::Value::Array(files.into_iter().map(toml::Value::String).collect()),
    );
    fs::write(partial.join(SOURCE_FILE), source)?;

    // The real parser has the last word.
    let text = fs::read_to_string(partial.join("colors.toml"))?;
    theme::Colors::parse(&text)
        .map_err(|p| anyhow::anyhow!("{url}: colours still invalid: {}", p.join("; ")))?;
    Ok(Installed {
        name: name.to_string(),
        source: url.to_string(),
        wallpapers,
        filled,
        colours_from,
    })
}

/// Replace `themes/<name>` with the partial build, in two renames.
fn swap_in(paths: &Paths, name: &str) -> Result<()> {
    let themes = paths.themes();
    let partial = themes.join(format!(".{name}.partial"));
    let target = themes.join(name);
    let old = themes.join(format!(".{name}.old"));
    let _ = fs::remove_dir_all(&old);
    if target.exists() {
        fs::rename(&target, &old)?;
    }
    fs::rename(&partial, &target)?;
    let _ = fs::remove_dir_all(&old);
    Ok(())
}

/// A regular file's text (not a symlink, not too big).
fn read_regular(path: &Path, max: u64) -> Option<String> {
    let meta = fs::symlink_metadata(path).ok()?;
    (meta.is_file() && meta.len() <= max)
        .then(|| fs::read_to_string(path).ok())
        .flatten()
}

fn copy_regular(from: &Path, to: &Path, max: u64) -> Result<bool> {
    match fs::symlink_metadata(from) {
        Ok(m) if m.is_file() && m.len() <= max => {
            fs::copy(from, to)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Regular files directly in `dir`, sorted (symlinks and folders skipped).
fn list_files(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

struct Source {
    url: String,
    files: BTreeSet<String>,
}

fn read_source(dir: &Path) -> Result<Source> {
    let text = fs::read_to_string(dir.join(SOURCE_FILE))?;
    let table: toml::Table = text.parse()?;
    let url = table
        .get("url")
        .and_then(|v| v.as_str())
        .context("no url in .palette-source")?
        .to_string();
    let files = table
        .get("files")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Ok(Source { url, files })
}

fn millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

// ---------------------------------------------------------------- colours

/// `0xRRGGBB`, `#RGB`, `#RRGGBBAA` → `#rrggbb`.
fn hex(value: &toml::Value) -> Option<String> {
    let s = value.as_str()?.trim();
    let digits = s
        .strip_prefix('#')
        .or_else(|| s.strip_prefix("0x"))
        .or_else(|| s.strip_prefix("0X"))?;
    let digits = match digits.len() {
        3 => digits.chars().flat_map(|c| [c, c]).collect(),
        6 => digits.to_string(),
        8 => digits[..6].to_string(),
        _ => return None,
    };
    digits
        .chars()
        .all(|c| c.is_ascii_hexdigit())
        .then(|| format!("#{}", digits.to_ascii_lowercase()))
}

/// `color0`…`color15` themes, mapped onto Palette's names.
const COLOURS16: [(&str, &str); 17] = [
    ("color0", "dark_background"),
    ("color1", "red"),
    ("color2", "green"),
    ("color3", "yellow"),
    ("color4", "blue"),
    ("color5", "magenta"),
    ("color6", "cyan"),
    ("color7", "light_foreground"),
    ("color8", "muted"),
    ("color9", "bright_red"),
    ("color10", "bright_green"),
    ("color11", "bright_yellow"),
    ("color12", "bright_blue"),
    ("color13", "bright_magenta"),
    ("color14", "bright_cyan"),
    ("color15", "bright_foreground"),
    ("selection_background", "selection"),
];

/// A full `colors.toml`: every required key, missing ones derived from the
/// others the way Omarchy themes usually relate them. Returns the text and
/// which keys were filled.
pub fn complete(table: &toml::Table, light: bool) -> Result<(String, Vec<String>)> {
    let mut v: std::collections::BTreeMap<&str, String> = std::collections::BTreeMap::new();
    for key in REQUIRED_KEYS.iter().chain(&["orange", "brown"]) {
        if let Some(h) = table.get(*key).and_then(hex) {
            v.insert(key, h);
        }
    }
    // The other Omarchy layout: `color0`…`color15` (ANSI order) and
    // `selection_background`. Named keys win where both exist.
    for (from, key) in COLOURS16 {
        if let Some(h) = table.get(from).and_then(hex) {
            v.entry(key).or_insert(h);
        }
    }
    for must in ["background", "foreground"] {
        if !v.contains_key(must) {
            bail!("no `{must}` colour");
        }
    }
    let base = ["red", "yellow", "green", "cyan", "blue", "magenta"];
    if base.iter().filter(|k| v.contains_key(**k)).count() < 4 {
        bail!("too few colours (needs most of red, yellow, green, cyan, blue, magenta)");
    }
    let mut filled = Vec::new();
    let bg = v["background"].clone();
    let fg = v["foreground"].clone();
    // Away from the background: darker on dark themes, lighter on light ones.
    let (deep, raised) = if light {
        ("#ffffff", "#000000")
    } else {
        ("#000000", "#ffffff")
    };
    let mut fill =
        |v: &mut std::collections::BTreeMap<&str, String>, key: &'static str, value: String| {
            if !v.contains_key(key) {
                v.insert(key, value);
                filled.push(key.to_string());
            }
        };
    // Normal colours missing: borrow a neighbour.
    for (key, from) in [
        ("red", "magenta"),
        ("magenta", "red"),
        ("yellow", "green"),
        ("green", "yellow"),
        ("cyan", "blue"),
        ("blue", "cyan"),
    ] {
        if let Some(c) = v.get(from).cloned() {
            fill(&mut v, key, c);
        }
    }
    let accent = v.get("blue").cloned().unwrap_or_else(|| fg.clone());
    fill(&mut v, "accent", accent);
    fill(&mut v, "dark_background", mix(&bg, deep, 0.25));
    fill(&mut v, "darker_background", mix(&bg, deep, 0.45));
    fill(&mut v, "lighter_background", mix(&bg, raised, 0.08));
    fill(&mut v, "selection", mix(&bg, &fg, 0.2));
    fill(&mut v, "muted", mix(&bg, &fg, 0.4));
    fill(&mut v, "dark_foreground", mix(&bg, &fg, 0.6));
    fill(&mut v, "light_foreground", mix(&fg, &bg, 0.15));
    fill(&mut v, "bright_foreground", mix(&fg, raised, 0.25));
    for key in base {
        let bright: &'static str = match key {
            "red" => "bright_red",
            "yellow" => "bright_yellow",
            "green" => "bright_green",
            "cyan" => "bright_cyan",
            "blue" => "bright_blue",
            _ => "bright_magenta",
        };
        let c = mix(&v[key], raised, 0.2);
        fill(&mut v, bright, c);
    }
    let mut out = String::from(
        "# Imported by Palette. Keys marked \"filled\" were missing and derived from the others.\n",
    );
    if light {
        out.push_str("mode = \"light\"\n");
    }
    for key in REQUIRED_KEYS.iter().chain(&["orange", "brown"]) {
        if let Some(value) = v.get(*key) {
            let note = if filled.iter().any(|f| f == key) {
                "  # filled"
            } else {
                ""
            };
            out.push_str(&format!("{key} = \"{value}\"{note}\n"));
        }
    }
    Ok((out, filled))
}

/// Omarchy themes from before `colors.toml`: read the Alacritty colours.
fn from_alacritty(text: &str) -> Result<toml::Table> {
    let table: toml::Table = text.parse().context("alacritty.toml is not valid TOML")?;
    let colors = table
        .get("colors")
        .and_then(|c| c.as_table())
        .context("alacritty.toml has no [colors]")?;
    let get = |section: &str, key: &str| -> Option<toml::Value> {
        colors.get(section)?.get(key).cloned()
    };
    let mut out = toml::Table::new();
    let mut put = |key: &str, value: Option<toml::Value>| {
        if let Some(v) = value {
            out.insert(key.into(), v);
        }
    };
    put("background", get("primary", "background"));
    put("foreground", get("primary", "foreground"));
    for c in ["red", "yellow", "green", "cyan", "blue", "magenta"] {
        put(c, get("normal", c));
        put(&format!("bright_{c}"), get("bright", c));
    }
    put("dark_background", get("normal", "black"));
    put("muted", get("bright", "black"));
    put("bright_foreground", get("bright", "white"));
    put("selection", get("selection", "background"));
    put("accent", get("normal", "blue"));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(
            slug("https://github.com/bjarneo/omarchy-aura-theme"),
            "aura"
        );
        assert_eq!(slug("https://github.com/JJDizz1L/aetheria/"), "aetheria");
        assert_eq!(
            slug("https://github.com/ankur311sudo/snow_black"),
            "snow-black"
        );
        assert_eq!(
            slug("https://github.com/mattbbia/venice-from-above-omarchy"),
            "venice-from-above"
        );
        assert_eq!(slug("https://github.com/eldritch-theme/omarchy"), "omarchy");
    }

    #[test]
    fn github_urls() {
        assert_eq!(
            github("https://github.com/a/b.git"),
            Some(("a".into(), "b".into()))
        );
        assert_eq!(github("https://github.com/a/b/tree/main"), None);
        assert_eq!(github("https://github.com/a/b;rm -rf"), None);
        assert_eq!(github("https://gitlab.com/a/b"), None);
    }

    #[test]
    fn reads_the_color0_to_15_layout() {
        let mut t = String::from(
            "accent = \"#e66868\"\nbackground = \"#1c1b1a\"\nforeground = \"#cecdc3\"\n\
             selection_background = \"#403e3c\"\n",
        );
        let ansi = [
            "#100f0f", "#d14d41", "#879a39", "#d0a215", "#4385be", "#ce5d97", "#3aa99f", "#878580",
            "#575653", "#af3029", "#66800b", "#ad8301", "#205ea6", "#a02f6f", "#24837b", "#cecdc3",
        ];
        for (i, c) in ansi.iter().enumerate() {
            t.push_str(&format!("color{i} = \"{c}\"\n"));
        }
        let (text, filled) = complete(&t.parse().unwrap(), false).unwrap();
        let colours = theme::Colors::parse(&text).unwrap();
        assert_eq!(colours.get("blue"), "#4385be");
        assert_eq!(colours.get("bright_cyan"), "#24837b");
        assert_eq!(colours.get("selection"), "#403e3c");
        assert!(!filled.contains(&"red".to_string()));
    }

    #[test]
    fn fills_missing_keys_and_reads_0x() {
        let t: toml::Table = "background = \"0x1a1b26\"\nforeground = \"#C0CAF5\"\n\
            red = \"#f7768e\"\nyellow = \"#e0af68\"\ngreen = \"#9ece6a\"\nblue = \"#7aa2f7\"\n"
            .parse()
            .unwrap();
        let (text, filled) = complete(&t, false).unwrap();
        assert!(text.contains("background = \"#1a1b26\"\n"), "{text}");
        assert!(filled.contains(&"accent".to_string()));
        assert!(filled.contains(&"cyan".to_string()));
        theme::Colors::parse(&text).unwrap();
    }
}
