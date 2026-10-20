//! `palette export <dir>` / `palette import <dir>`: move a Palette setup (and,
//! opt-in, the user's own app configs) to another Mac as a plain folder.
//! Copied by hand (AirDrop, USB, a private git repo); no network.
//!
//! ```text
//! <dir>/
//!   palette-export.toml   version, the exporting home folder, active theme
//!   README.txt            what this is and how to import it
//!   palette/              settings.toml, sync.toml, themes/ (incl. wallpapers), wiring/
//!   configs/<path>        the user's own configs, by path under their home
//! ```
//!
//! Importing never runs anything from the folder: themes are data, configs
//! are copied as files (after a backup), and the only change to their text
//! is the exporting Mac's home path becoming this Mac's.

use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::paths::Paths;
use crate::{state, targets, wiring};

const MANIFEST: &str = "palette-export.toml";
const VERSION: i64 = 1;

/// `~/.config/palette/sync.toml`: what `export` takes along.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncConfig {
    /// Paths relative to home: files or whole folders.
    pub configs: Vec<String>,
    pub wallpapers: bool,
}

fn sync_file(paths: &Paths) -> PathBuf {
    paths.root().join("sync.toml")
}

/// Configs worth taking to another Mac, found on this one: every folder or
/// file a Palette target wires, plus AeroSpace. Shell rc files are left out
/// on purpose: they often hold API keys.
pub fn default_configs(paths: &Paths) -> Vec<String> {
    let home = paths.home();
    let config = paths.config_home();
    let mut found: Vec<PathBuf> = [
        config.join("sketchybar"),
        config.join("borders"),
        config.join("aerospace"),
        home.join(".aerospace.toml"),
        home.join("Library/Application Support/com.mitchellh.ghostty"),
        config.join("ghostty"),
        config.join("kitty"),
        config.join("alacritty"),
        home.join(".tmux.conf"),
        config.join("tmux"),
        config.join("nvim"),
        config.join("btop/btop.conf"),
        config.join("starship.toml"),
        home.join("Library/Application Support/VSCodium/User/settings.json"),
        home.join("Library/Application Support/VSCodium/User/keybindings.json"),
    ]
    .into_iter()
    .filter(|p| p.exists())
    .collect();
    found.dedup();
    found
        .iter()
        .filter_map(|p| p.strip_prefix(home).ok())
        .map(|p| p.to_string_lossy().to_string())
        .collect()
}

/// Load sync.toml, creating it with the defaults the first time.
/// Returns whether it was just created.
pub fn load_or_create(paths: &Paths) -> Result<(SyncConfig, bool)> {
    let file = sync_file(paths);
    match fs::read_to_string(&file) {
        Ok(text) => {
            let table: toml::Table = text
                .parse()
                .with_context(|| format!("{} is not valid TOML", file.display()))?;
            let configs = table
                .get("configs")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let wallpapers = table
                .get("wallpapers")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            Ok((
                SyncConfig {
                    configs,
                    wallpapers,
                },
                false,
            ))
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {
            let config = SyncConfig {
                configs: default_configs(paths),
                wallpapers: true,
            };
            save(paths, &config)?;
            Ok((config, true))
        }
        Err(e) => Err(e).with_context(|| format!("reading {}", file.display())),
    }
}

fn save(paths: &Paths, config: &SyncConfig) -> Result<()> {
    let mut text = String::from(
        "# What `palette export` copies besides Palette's own folder.\n\
         # Paths are relative to your home folder; folders are copied whole.\n\
         configs = [\n",
    );
    for c in &config.configs {
        text.push_str(&format!("    {},\n", toml::Value::from(c.as_str())));
    }
    text.push_str(
        "    # \".zshrc\",   # left out by default: shell files often hold API keys\n]\n\n\
         # Theme wallpapers (can be large)\n",
    );
    text.push_str(&format!("wallpapers = {}\n", config.wallpapers));
    fs::create_dir_all(paths.root())?;
    fs::write(sync_file(paths), text).context("writing sync.toml")
}

// ---------------------------------------------------------------- copying

#[derive(Debug, Default)]
pub struct Copied {
    pub files: usize,
    pub bytes: u64,
}

/// Copy a file, a symlink (as a link) or a folder tree. Anything else
/// (sockets, devices) is skipped. `skip` filters by path relative to `src`.
fn copy_tree(src: &Path, dst: &Path, skip: &dyn Fn(&Path) -> bool, out: &mut Copied) -> Result<()> {
    copy_inner(src, dst, Path::new(""), skip, out)
}

fn copy_inner(
    src: &Path,
    dst: &Path,
    rel: &Path,
    skip: &dyn Fn(&Path) -> bool,
    out: &mut Copied,
) -> Result<()> {
    if skip(rel) {
        return Ok(());
    }
    let meta = fs::symlink_metadata(src).with_context(|| format!("reading {}", src.display()))?;
    let kind = meta.file_type();
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    if kind.is_symlink() {
        let target = fs::read_link(src)?;
        let _ = fs::remove_file(dst);
        std::os::unix::fs::symlink(target, dst)
            .with_context(|| format!("linking {}", dst.display()))?;
        out.files += 1;
    } else if kind.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let name = entry.file_name();
            copy_inner(&entry.path(), &dst.join(&name), &rel.join(&name), skip, out)?;
        }
    } else if kind.is_file() {
        fs::copy(src, dst).with_context(|| format!("copying {}", src.display()))?;
        out.files += 1;
        out.bytes += meta.len();
    }
    Ok(())
}

/// Every regular file and link under `root` (or `root` itself), as paths
/// relative to `root`.
fn list_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    fn walk(dir: &Path, rel: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        let meta = fs::symlink_metadata(dir)?;
        if meta.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                walk(&entry.path(), &rel.join(entry.file_name()), out)?;
            }
        } else if meta.is_file() || meta.file_type().is_symlink() {
            out.push(rel.to_path_buf());
        }
        Ok(())
    }
    walk(root, Path::new(""), &mut out)?;
    Ok(out)
}

/// A relative path with no `..`, root or prefix: safe to join onto home.
fn is_plain_relative(p: &Path) -> bool {
    p.components().all(|c| matches!(c, Component::Normal(_)))
}

// ---------------------------------------------------------------- export

#[derive(Debug)]
pub struct ExportReport {
    pub dir: PathBuf,
    pub palette: Copied,
    pub configs: Vec<(String, Copied)>,
    pub missing: Vec<String>,
    /// sync.toml was created by this export (tell the user it exists).
    pub created_sync: bool,
}

pub struct ExportOptions {
    /// `None`: as sync.toml says.
    pub wallpapers: Option<bool>,
    pub configs: Option<bool>,
}

pub fn export(paths: &Paths, dir: &Path, options: &ExportOptions) -> Result<ExportReport> {
    let (sync, created_sync) = load_or_create(paths)?;
    // An existing folder is fine if it's empty or an earlier export (its
    // palette/ and configs/ are replaced; anything else, like .git, stays).
    if dir.exists() {
        let earlier = dir.join(MANIFEST).is_file();
        let empty = fs::read_dir(dir)?.next().is_none();
        if !earlier && !empty {
            bail!(
                "{} isn't empty and isn't a Palette export; choose a new folder",
                dir.display()
            );
        }
        for sub in ["palette", "configs"] {
            match fs::remove_dir_all(dir.join(sub)) {
                Err(e) if e.kind() != ErrorKind::NotFound => return Err(e.into()),
                _ => {}
            }
        }
    }
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;

    // Palette's own folder, minus what is rebuilt on any Mac.
    let wallpapers = options.wallpapers.unwrap_or(sync.wallpapers);
    let mut palette = Copied::default();
    let root = paths.root();
    for name in ["settings.toml", "sync.toml", "themes", "wiring"] {
        let src = root.join(name);
        if fs::symlink_metadata(&src).is_ok() {
            let skip = |rel: &Path| {
                !wallpapers && rel.components().any(|c| c.as_os_str() == "backgrounds")
            };
            copy_tree(&src, &dir.join("palette").join(name), &skip, &mut palette)?;
        }
    }

    let mut configs = Vec::new();
    let mut missing = Vec::new();
    if options.configs.unwrap_or(true) {
        for rel in &sync.configs {
            let rel_path = Path::new(rel);
            if !is_plain_relative(rel_path) {
                bail!(
                    "sync.toml: `{rel}` must be a path inside your home folder, like `.config/x`"
                );
            }
            let src = paths.home().join(rel_path);
            if fs::symlink_metadata(&src).is_err() {
                missing.push(rel.clone());
                continue;
            }
            let mut copied = Copied::default();
            // Logs and caches don't belong in a config sync.
            let skip = |rel: &Path| {
                rel.extension().is_some_and(|e| e == "log")
                    || rel.components().any(|c| c.as_os_str() == ".git")
            };
            copy_tree(
                &src,
                &dir.join("configs").join(rel_path),
                &skip,
                &mut copied,
            )?;
            configs.push((rel.clone(), copied));
        }
    }

    let theme = state::current_theme(paths)?.unwrap_or_default();
    fs::write(
        dir.join(MANIFEST),
        format!(
            "# A Palette export. On the other Mac: palette import <this folder>\n\
             version = {VERSION}\n\
             home = {}\n\
             theme = {}\n",
            toml::Value::from(paths.home().to_string_lossy().as_ref()),
            toml::Value::from(theme.as_str())
        ),
    )?;
    fs::write(dir.join("README.txt"), README)?;
    Ok(ExportReport {
        dir: dir.to_path_buf(),
        palette,
        configs,
        missing,
        created_sync,
    })
}

const README: &str = "\
This folder is a Palette export: themes, wallpapers, settings and (if chosen)
app configs from one Mac.

On the other Mac:
  1. Install Palette (drag Palette.app to Applications, open it, then
     Settings > Command-line tool > Install).
  2. In a terminal: palette import /path/to/this/folder --dry-run
     to see what would change, then without --dry-run to do it.

Anything the import replaces is backed up to ~/.config/palette/backup/ first.
Nothing in this folder is ever run; it's copied as files.
";

// ---------------------------------------------------------------- import

#[derive(Debug, Default)]
pub struct ImportReport {
    /// Paths (relative to home) that will be / were written.
    pub written: Vec<PathBuf>,
    /// Of those, the ones that existed before (and were backed up).
    pub replaced: Vec<PathBuf>,
    /// Text files whose old home path was rewritten.
    pub relocated: Vec<PathBuf>,
    pub backup: Option<PathBuf>,
    pub theme: Option<String>,
    pub old_home: String,
}

pub fn import(paths: &Paths, dir: &Path, dry_run: bool) -> Result<ImportReport> {
    let manifest = dir.join(MANIFEST);
    let text = fs::read_to_string(&manifest)
        .with_context(|| format!("{} isn't a Palette export (no {MANIFEST})", dir.display()))?;
    let table: toml::Table = text
        .parse()
        .context("the export's manifest is not valid TOML")?;
    let version = table
        .get("version")
        .and_then(|v| v.as_integer())
        .unwrap_or(0);
    if version != VERSION {
        bail!("this export has format version {version}; this Palette reads version {VERSION}");
    }
    let old_home = table
        .get("home")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let theme = table
        .get("theme")
        .and_then(|v| v.as_str())
        .filter(|t| !t.is_empty())
        .map(String::from);
    let new_home = paths.home().to_string_lossy().to_string();

    // Plan: every file, its destination, and whether it exists already.
    let mut plan: Vec<(PathBuf, PathBuf)> = Vec::new(); // (source, destination)
    let palette_src = dir.join("palette");
    if palette_src.is_dir() {
        for rel in list_files(&palette_src)? {
            plan.push((palette_src.join(&rel), paths.root().join(&rel)));
        }
    }
    let configs_src = dir.join("configs");
    if configs_src.is_dir() {
        for rel in list_files(&configs_src)? {
            if !is_plain_relative(&rel) {
                bail!("the export contains an unsafe path: {}", rel.display());
            }
            plan.push((configs_src.join(&rel), paths.home().join(&rel)));
        }
    }

    let mut report = ImportReport {
        theme: theme.clone(),
        old_home: old_home.clone(),
        ..ImportReport::default()
    };
    let home = paths.home();
    let rel_to_home = |p: &Path| p.strip_prefix(home).unwrap_or(p).to_path_buf();
    for (_, dst) in &plan {
        report.written.push(rel_to_home(dst));
        if fs::symlink_metadata(dst).is_ok() {
            report.replaced.push(rel_to_home(dst));
        }
    }
    if dry_run {
        return Ok(report);
    }

    // Back up what will be replaced, then copy.
    if !report.replaced.is_empty() {
        let backup = wiring::new_backup_dir(paths, "import");
        let mut copied = Copied::default();
        for rel in &report.replaced {
            copy_tree(&home.join(rel), &backup.join(rel), &|_| false, &mut copied)?;
        }
        report.backup = Some(backup);
    }
    for (src, dst) in &plan {
        let mut copied = Copied::default();
        copy_tree(src, dst, &|_| false, &mut copied)?;
        // The exporting Mac's home path (in Ghostty/tmux/Neovim import lines,
        // btop's theme link…) becomes this Mac's.
        let relocate = !old_home.is_empty() && old_home != new_home;
        if relocate && fs::symlink_metadata(dst).is_ok_and(|m| m.is_file()) {
            if let Ok(text) = fs::read_to_string(dst)
                && text.contains(&old_home)
            {
                fs::write(dst, text.replace(&old_home, &new_home))?;
                report.relocated.push(rel_to_home(dst));
            }
        } else if relocate
            && let Ok(target) = fs::read_link(dst)
            && let Ok(rest) = target.strip_prefix(&old_home)
        {
            fs::remove_file(dst)?;
            std::os::unix::fs::symlink(home.join(rest), dst)?;
            report.relocated.push(rel_to_home(dst));
        }
    }
    wiring::remove_old_backups(paths)?;
    Ok(report)
}

/// Targets whose configs an import brought along (for the summary).
pub fn imported_apps(report: &ImportReport) -> Vec<&'static str> {
    targets::ALL
        .iter()
        .map(|t| t.name)
        .filter(|name| {
            report.written.iter().any(|p| {
                let p = p.to_string_lossy();
                p.contains(name) || (*name == "ghostty" && p.contains("com.mitchellh.ghostty"))
            })
        })
        .collect()
}
