//! Desktop: settings pages for the tools around the themes. The bar
//! (SketchyBar), window borders (JankyBorders) and windows (AeroSpace).
//!
//! These are the one place Palette writes a user's config files whole. You
//! opt in per tool ("Let Palette manage it"): Palette backs up your files
//! once as `original` (kept until you give the tool back), reads what it can
//! from them into a small settings file (`desktop/<tool>.toml`), and from
//! then on writes the config from those settings. Every later change makes
//! a backup first (the last [`KEEP`] are kept, and [`restore`] goes back
//! to one). "Give it back" restores the original. AeroSpace is edited key by key, so everything Palette
//! doesn't show (your keybindings, callbacks) stays as you wrote it.

pub mod bar;
pub mod borders;
pub mod macos;
pub mod prompt;
pub mod windows;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::paths::Paths;
use crate::system::Outcome;

/// Backups kept per tool, besides the original. Each is a few KB; five
/// covers "undo my last few changes", the original covers the rest.
pub const KEEP: usize = 5;
const ORIGINAL: &str = "original";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Bar,
    Borders,
    Windows,
    Prompt,
}

impl Tool {
    pub const ALL: [Tool; 4] = [Tool::Bar, Tool::Windows, Tool::Borders, Tool::Prompt];

    pub fn name(self) -> &'static str {
        match self {
            Tool::Bar => "bar",
            Tool::Borders => "borders",
            Tool::Windows => "windows",
            Tool::Prompt => "prompt",
        }
    }

    /// The program behind it.
    pub fn program(self) -> &'static str {
        match self {
            Tool::Bar => "sketchybar",
            Tool::Borders => "borders",
            Tool::Windows => "aerospace",
            Tool::Prompt => "starship",
        }
    }

    pub fn app(self) -> &'static str {
        match self {
            Tool::Bar => "SketchyBar",
            Tool::Borders => "JankyBorders",
            Tool::Windows => "AeroSpace",
            Tool::Prompt => "Starship",
        }
    }

    /// `bar`, `windows`, `borders`, or the program's name.
    pub fn parse(name: &str) -> Result<Tool> {
        match name.to_ascii_lowercase().as_str() {
            "bar" | "sketchybar" => Ok(Tool::Bar),
            "borders" | "jankyborders" => Ok(Tool::Borders),
            "windows" | "aerospace" => Ok(Tool::Windows),
            "prompt" | "starship" => Ok(Tool::Prompt),
            other => bail!("`{other}` is not one of bar, windows, borders, prompt"),
        }
    }

    /// The config files Palette writes when it manages this tool.
    pub fn files(self, paths: &Paths) -> Vec<PathBuf> {
        match self {
            Tool::Bar => vec![paths.config_home().join("sketchybar/sketchybarrc")],
            Tool::Borders => vec![paths.config_home().join("borders/bordersrc")],
            Tool::Windows => vec![windows::config_file(paths)],
            Tool::Prompt => vec![prompt::config_file(paths)],
        }
    }

    pub fn installed(self) -> bool {
        crate::targets::has_program(self.program())
    }
}

/// `desktop/<tool>.toml`: the settings, while Palette manages the tool.
pub fn model_path(paths: &Paths, tool: Tool) -> PathBuf {
    paths
        .root()
        .join("desktop")
        .join(format!("{}.toml", tool.name()))
}

pub fn backups_dir(paths: &Paths, tool: Tool) -> PathBuf {
    paths.root().join("desktop/backups").join(tool.name())
}

pub fn is_managed(paths: &Paths, tool: Tool) -> bool {
    model_path(paths, tool).is_file()
}

#[derive(Debug, Clone)]
pub struct Status {
    pub tool: Tool,
    pub installed: bool,
    pub managed: bool,
    pub files: Vec<PathBuf>,
    /// Your files as they were before Palette managed them.
    pub original: Option<PathBuf>,
    /// Backups made before each change, newest first.
    pub backups: Vec<PathBuf>,
}

pub fn status(paths: &Paths, tool: Tool) -> Status {
    let original = backups_dir(paths, tool).join(ORIGINAL);
    Status {
        tool,
        installed: tool.installed(),
        managed: is_managed(paths, tool),
        files: tool.files(paths),
        original: original.is_dir().then_some(original),
        backups: backups(paths, tool),
    }
}

/// What a change did.
#[derive(Debug, Clone)]
pub struct Applied {
    pub tool: Tool,
    pub written: Vec<PathBuf>,
    /// The backup made first (`None` when there was nothing to back up).
    pub backup: Option<PathBuf>,
    pub reload: Outcome,
    /// Things Palette couldn't carry over or keep (shown, never fatal).
    pub warnings: Vec<String>,
}

/// A file Palette writes for a tool.
#[derive(Debug, Clone, PartialEq)]
pub struct File {
    pub path: PathBuf,
    pub text: String,
    pub executable: bool,
}

/// The settings as a TOML value, for both front doors. While Palette doesn't
/// manage the tool yet: what it would start from (read from your config).
pub fn get(paths: &Paths, tool: Tool) -> Result<toml::Value> {
    Ok(match tool {
        Tool::Bar => to_value(&load_or_import(paths, tool, bar::import)?)?,
        Tool::Borders => to_value(&load_or_import(paths, tool, borders::import)?)?,
        Tool::Windows => {
            let mut w: windows::Windows = load_or_import(paths, tool, windows::import)?;
            windows::prepare(paths, &mut w);
            to_value(&w)?
        }
        Tool::Prompt => to_value(&load_or_import(paths, tool, prompt::import)?)?,
    })
}

fn load_or_import<T: DeserializeOwned>(
    paths: &Paths,
    tool: Tool,
    import: fn(&Paths) -> (T, Vec<String>),
) -> Result<T> {
    if is_managed(paths, tool) {
        load(paths, tool)
    } else {
        Ok(import(paths).0)
    }
}

/// The saved settings. Missing keys take their defaults, so a settings file
/// from an older Palette still loads.
pub fn load<T: DeserializeOwned>(paths: &Paths, tool: Tool) -> Result<T> {
    let path = model_path(paths, tool);
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("{} has a mistake", path.display()))
}

fn to_value<T: Serialize>(model: &T) -> Result<toml::Value> {
    toml::Value::try_from(model).context("turning the settings into TOML")
}

/// "Let Palette manage it": back up your files as the original, start from
/// what Palette can read in them, and write the config.
pub fn manage(paths: &Paths, tool: Tool) -> Result<Applied> {
    if is_managed(paths, tool) {
        bail!("Palette already manages {}", tool.app());
    }
    let original = backups_dir(paths, tool).join(ORIGINAL);
    if !original.exists() {
        copy_files(paths, tool, &original)?;
    }
    let (value, warnings) = match tool {
        Tool::Bar => {
            let (m, w) = bar::import(paths);
            (to_value(&m)?, w)
        }
        Tool::Borders => {
            let (m, w) = borders::import(paths);
            (to_value(&m)?, w)
        }
        Tool::Windows => {
            let (m, w) = windows::import(paths);
            (to_value(&m)?, w)
        }
        Tool::Prompt => {
            let (m, w) = prompt::import(paths);
            (to_value(&m)?, w)
        }
    };
    let mut applied = write_and_reload(paths, tool, value, false)?;
    applied.backup = Some(original);
    applied.warnings.splice(0..0, warnings);
    Ok(applied)
}

/// Save new settings and write the config (backing up the current files).
/// Starts managing the tool if Palette didn't yet.
pub fn set(paths: &Paths, tool: Tool, value: toml::Value) -> Result<Applied> {
    if !is_managed(paths, tool) {
        manage(paths, tool)?;
    }
    write_and_reload(paths, tool, value, true)
}

/// Change single settings by dotted key (`pill.height=28`), for the CLI.
/// The value is read as TOML (`28`, `true`, `"text"`), else as text.
pub fn set_keys(paths: &Paths, tool: Tool, pairs: &[(String, String)]) -> Result<Applied> {
    let mut value = get(paths, tool)?;
    for (key, raw) in pairs {
        let new = parse_value(raw);
        let mut at = &mut value;
        let parts: Vec<&str> = key.split('.').collect();
        for (i, part) in parts.iter().enumerate() {
            let table = at.as_table_mut().with_context(|| {
                format!(
                    "`{key}`: `{}` is not a group of settings",
                    parts[..i].join(".")
                )
            })?;
            if i + 1 == parts.len() {
                if !table.contains_key(*part) {
                    bail!(
                        "`{key}` is not a {} setting (see `palette desktop {}`)",
                        tool.name(),
                        tool.name()
                    );
                }
                table.insert(part.to_string(), new.clone());
                break;
            }
            at = table
                .get_mut(*part)
                .with_context(|| format!("`{key}` is not a {} setting", tool.name()))?;
        }
    }
    set(paths, tool, value)
}

fn parse_value(raw: &str) -> toml::Value {
    format!("v = {raw}")
        .parse::<toml::Table>()
        .ok()
        .and_then(|mut t| t.remove("v"))
        .unwrap_or_else(|| toml::Value::String(raw.to_string()))
}

fn write_and_reload(
    paths: &Paths,
    tool: Tool,
    value: toml::Value,
    backup_first: bool,
) -> Result<Applied> {
    // Typed and checked before anything is written.
    let (text, files, warnings) = match tool {
        Tool::Bar => {
            let m: bar::Bar = value.try_into().context("bar settings")?;
            m.validate()?;
            let warnings = bar::font_warnings(&m);
            (toml::to_string(&m)?, bar::render(paths, &m)?, warnings)
        }
        Tool::Borders => {
            let m: borders::Borders = value.try_into().context("borders settings")?;
            m.validate()?;
            (
                toml::to_string(&m)?,
                borders::render(paths, &m)?,
                Vec::new(),
            )
        }
        Tool::Windows => {
            let mut m: windows::Windows = value.try_into().context("windows settings")?;
            windows::prepare(paths, &mut m);
            m.validate()?;
            let (files, warnings) = windows::render(paths, &m)?;
            (toml::to_string(&m)?, files, warnings)
        }
        Tool::Prompt => {
            let m: prompt::Prompt = value.try_into().context("prompt settings")?;
            m.validate()?;
            (toml::to_string(&m)?, prompt::render(paths, &m)?, Vec::new())
        }
    };
    let backup = if backup_first {
        backup(paths, tool)?
    } else {
        None
    };
    let model = model_path(paths, tool);
    write_atomic(&model, &text, false)?;
    let mut written = Vec::new();
    for f in &files {
        write_atomic(&f.path, &f.text, f.executable)?;
        written.push(f.path.clone());
    }
    ensure_colours(paths, tool)?;
    Ok(Applied {
        tool,
        written,
        backup,
        reload: reload(paths, tool),
        warnings,
    })
}

/// The bar and borders read every theme colour by name (`C_accent`) from
/// Palette's generated files. Files rendered by an older Palette lack them:
/// render the active theme again, once.
fn ensure_colours(paths: &Paths, tool: Tool) -> Result<()> {
    let file = match tool {
        Tool::Bar => "sketchybar.sh",
        Tool::Borders => "borders.sh",
        Tool::Windows | Tool::Prompt => return Ok(()),
    };
    let current = paths.current().join(file);
    if current.is_file() && !fs::read_to_string(&current)?.contains("C_accent=") {
        crate::refresh(paths)?;
    }
    Ok(())
}

/// Apply the saved settings again (after a theme switch the colours come
/// from the theme anyway; this is for the CLI's `apply` and tests).
pub fn reload(paths: &Paths, tool: Tool) -> Outcome {
    // A fake home's files mean nothing to the running programs (and running
    // its bordersrc would restyle the real borders).
    // The prompt's "reload" only renders Palette's copy of the file.
    if tool == Tool::Prompt {
        return prompt::reload(paths);
    }
    if paths.is_fake() {
        return Outcome::Skipped("fake home".into());
    }
    match tool {
        Tool::Bar => bar::reload(paths),
        Tool::Borders => borders::reload(paths),
        Tool::Windows => windows::reload(paths),
        Tool::Prompt => prompt::reload(paths),
    }
}

/// "Give it back": your original files again, and Palette stops writing
/// them. Files that didn't exist before Palette go to the Trash.
pub fn release(paths: &Paths, tool: Tool) -> Result<Applied> {
    if !is_managed(paths, tool) {
        bail!("Palette doesn't manage {}", tool.app());
    }
    let original = backups_dir(paths, tool).join(ORIGINAL);
    let backup = backup(paths, tool)?;
    let mut written = Vec::new();
    for path in tool.files(paths) {
        let saved = original.join(rel(paths, &path));
        if saved.is_file() {
            let text = fs::read_to_string(&saved)?;
            let exec = is_executable(&saved);
            write_atomic(&path, &text, exec)?;
            written.push(path);
        } else if path.exists() {
            crate::install::move_to_trash(paths, &path)?;
        }
    }
    // Kept beside, so managing again later could start from it.
    let model = model_path(paths, tool);
    fs::rename(&model, model.with_extension("toml.released"))?;
    fs::remove_dir_all(&original).ok();
    Ok(Applied {
        tool,
        written,
        backup,
        reload: reload(paths, tool),
        warnings: Vec::new(),
    })
}

/// A Starship preset as your prompt (`palette desktop prompt preset <name>`):
/// your file becomes the preset (backed up first; Give it back restores
/// yours), with its colours turned into the theme's, then Palette's blocks.
pub fn prompt_preset(paths: &Paths, name: &str) -> Result<Applied> {
    let (model, text, mut warnings) = prompt::preset(paths, name)?;
    if !is_managed(paths, Tool::Prompt) {
        manage(paths, Tool::Prompt)?;
    }
    backup(paths, Tool::Prompt)?;
    write_atomic(&prompt::config_file(paths), &text, false)?;
    let mut applied = write_and_reload(paths, Tool::Prompt, to_value(&model)?, false)?;
    warnings.append(&mut applied.warnings);
    applied.warnings = warnings;
    Ok(applied)
}

/// Go back to the settings and files of a backup, backing up the current
/// ones first, so a restore can be undone the same way. `which` is a
/// backup's name (as the app lists them) or its number (1 = the newest).
pub fn restore(paths: &Paths, tool: Tool, which: &str) -> Result<Applied> {
    if !is_managed(paths, tool) {
        bail!("Palette doesn't manage {}: nothing to restore", tool.app());
    }
    let list = backups(paths, tool);
    let by_name = list
        .iter()
        .find(|p| p.file_name().is_some_and(|n| n == which));
    let by_number = which
        .parse::<usize>()
        .ok()
        .and_then(|n| n.checked_sub(1))
        .and_then(|i| list.get(i));
    let Some(from) = by_name.or(by_number).cloned() else {
        bail!(
            "that backup isn't there any more ({} keeps the last {KEEP}): pick one from the list again",
            tool.app()
        );
    };
    let settings = from.join("settings.toml");
    if !settings.is_file() {
        bail!("{} has no saved settings to go back to", from.display());
    }
    // Read it all first: the backup made next trims the oldest, which may
    // be this one (restoring the last in the list failed that way).
    let settings = fs::read_to_string(&settings)?;
    let mut saved_files = Vec::new();
    for path in tool.files(paths) {
        let saved = from.join(rel(paths, &path));
        if saved.is_file() {
            saved_files.push((path, fs::read_to_string(&saved)?, is_executable(&saved)));
        }
    }
    let backup = backup(paths, tool)?;
    let mut written = Vec::new();
    for (path, text, exec) in saved_files {
        write_atomic(&path, &text, exec)?;
        written.push(path);
    }
    write_atomic(&model_path(paths, tool), &settings, false)?;
    // Scripts the bar runs come with Palette: the current ones fit any rc.
    if tool == Tool::Bar {
        let m: bar::Bar = load(paths, tool)?;
        for f in bar::render(paths, &m)?.into_iter().skip(1) {
            write_atomic(&f.path, &f.text, f.executable)?;
        }
    }
    Ok(Applied {
        tool,
        written,
        backup,
        reload: reload(paths, tool),
        warnings: Vec::new(),
    })
}

/// Backups made before changes, newest first (not the original).
pub fn backups(paths: &Paths, tool: Tool) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(backups_dir(paths, tool))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.file_name().is_some_and(|n| n != ORIGINAL))
        .collect();
    dirs.sort();
    dirs.reverse();
    dirs
}

/// Copy the tool's files into a new timestamped folder, then trim to [`KEEP`].
fn backup(paths: &Paths, tool: Tool) -> Result<Option<PathBuf>> {
    if !tool.files(paths).iter().any(|p| p.exists()) && !model_path(paths, tool).exists() {
        return Ok(None);
    }
    let stamp = crate::wiring::timestamp_now();
    // Several in one second: number past the highest (never reuse a name a
    // trim freed, or the newest would sort as the oldest), zero-padded so
    // names sort in the order they were made.
    let next = backups(paths, tool)
        .iter()
        .filter_map(|p| {
            let name = p.file_name()?.to_str()?;
            let rest = name.strip_prefix(stamp.as_str())?;
            Some(
                rest.strip_prefix('-')
                    .map_or(Some(0), |n| n.parse::<u32>().ok())?
                    + 1,
            )
        })
        .max();
    let dir = backups_dir(paths, tool).join(match next {
        None => stamp,
        Some(n) => format!("{stamp}-{n:03}"),
    });
    copy_files(paths, tool, &dir)?;
    let model = model_path(paths, tool);
    if model.is_file() {
        fs::copy(&model, dir.join("settings.toml"))?;
    }
    for old in backups(paths, tool).into_iter().skip(KEEP) {
        fs::remove_dir_all(&old).with_context(|| format!("removing {}", old.display()))?;
    }
    Ok(Some(dir))
}

fn copy_files(paths: &Paths, tool: Tool, dir: &Path) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    for path in tool.files(paths) {
        if !path.is_file() {
            continue;
        }
        let dest = dir.join(rel(paths, &path));
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&path, &dest).with_context(|| format!("backing up {}", path.display()))?;
    }
    Ok(())
}

fn rel(paths: &Paths, path: &Path) -> PathBuf {
    path.strip_prefix(paths.home())
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| path.strip_prefix("/").unwrap_or(path).to_path_buf())
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

/// Write via a temp file and rename. A symlinked config (dotfiles managers)
/// is written through, keeping the link.
pub(crate) fn write_atomic(path: &Path, text: &str, executable: bool) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let real = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if let Some(dir) = real.parent() {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let name = real.file_name().unwrap_or_default().to_string_lossy();
    let tmp = real.with_file_name(format!(".{name}.palette-tmp"));
    fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    let mode = if executable { 0o755 } else { 0o644 };
    fs::set_permissions(&tmp, fs::Permissions::from_mode(mode))?;
    fs::rename(&tmp, &real).with_context(|| format!("saving {}", real.display()))
}

/// Single quotes for the shell: `it's` → `'it'\''s'`.
pub(crate) fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(sh_quote("open -a Calendar"), "'open -a Calendar'");
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn values_parse_as_toml_or_text() {
        assert_eq!(parse_value("28"), toml::Value::Integer(28));
        assert_eq!(parse_value("true"), toml::Value::Boolean(true));
        assert_eq!(parse_value("round"), toml::Value::String("round".into()));
        assert_eq!(parse_value("\"a b\""), toml::Value::String("a b".into()));
    }
}
