//! `palette init`: point the user's own configs at Palette's generated files.
//!
//! Most targets add exactly two lines to a config: a marker comment and one
//! import line. Apps without an include mechanism (VSCodium, btop) get one
//! setting changed instead, with the old value remembered. Tools configured
//! through environment variables are wired by the `shell` target's line.
//! `--remove` undoes all of it, and every file is backed up before it is touched.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

use crate::paths::Paths;
use crate::targets::{self, Target};

/// The text of the marker comment above every import line Palette adds.
pub const MARKER: &str = "Added by palette init (undo: palette init --remove)";

/// Backup folders kept; older ones are deleted by the next `init`.
const KEEP_BACKUPS: usize = 5;

/// How a target hooks into the user's config.
#[derive(Debug)]
pub enum Wiring {
    /// Add a marker comment and one import line to the app's config files.
    Import(Import),
    /// Change one setting to point at Palette, remembering the old value.
    Setting(Setting),
    /// Configured by environment variables in `shell.sh`: wired when the
    /// `shell` target is.
    Shell,
}

#[derive(Debug)]
pub struct Import {
    /// The config files to wire: files that exist, or (with `create`) the
    /// file to create.
    pub configs: fn(&Paths) -> Vec<PathBuf>,
    /// The import line, given the generated file it should load.
    pub line: fn(&Paths, &Path) -> String,
    /// Insert `block` (marker + line) into a config's text, or explain why it
    /// has to be done by hand.
    pub insert: fn(&str, &str) -> Result<String, String>,
    /// Line comment of the config's language (`#`, `--`).
    pub comment: &'static str,
    /// Create the config file if it doesn't exist (for apps with no config yet).
    pub create: bool,
}

/// A setting's extra step: `(paths, remove)` → a note for the report, or why it failed.
pub type Prepare = fn(&Paths, bool) -> Result<String, String>;

#[derive(Debug)]
pub struct Setting {
    /// The config files holding the setting (one per Obsidian vault; at
    /// most one for most apps). Only files that exist are wired.
    pub configs: fn(&Paths) -> Vec<PathBuf>,
    /// Several files: remember each one's old value separately.
    pub per_file: bool,
    /// The value that points the app at Palette.
    pub value: fn(&Paths) -> String,
    /// The setting's current value in a config's text.
    pub read: fn(&str) -> Option<String>,
    /// Set the value (`Some`) or delete the setting (`None`) in a config's text.
    pub write: fn(&str, Option<&str>) -> String,
    /// Runs before the setting is changed, or after it is restored with
    /// `remove` true. VSCodium installs and removes its theme extension here.
    pub prepare: Option<Prepare>,
}

impl Import {
    /// Defaults: shell-style comments, never create the config.
    pub const fn new(
        configs: fn(&Paths) -> Vec<PathBuf>,
        line: fn(&Paths, &Path) -> String,
        insert: fn(&str, &str) -> Result<String, String>,
    ) -> Self {
        Self {
            configs,
            line,
            insert,
            comment: "#",
            create: false,
        }
    }
}

// ---------------------------------------------------------------- lines

/// `[ -f "$HOME/…/x.sh" ] && source "$HOME/…/x.sh"`: harmless if Palette is gone.
pub fn source_line(paths: &Paths, generated: &Path) -> String {
    let p = paths.for_shell(generated);
    format!("[ -f \"{p}\" ] && source \"{p}\"")
}

// ---------------------------------------------------------------- placement

/// Append at the end, so Palette's values override earlier ones.
pub fn at_end(contents: &str, block: &str) -> Result<String, String> {
    let mut out = contents.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(block);
    out.push('\n');
    Ok(out)
}

/// Insert right after the last line assigning a `0xAARRGGBB` colour, so the
/// user's own colour variables are overridden but everything that uses them
/// (further down) sees Palette's. Without colour lines: after the `#!` line.
pub fn after_colours(contents: &str, block: &str) -> Result<String, String> {
    let mut lines: Vec<String> = contents.split_inclusive('\n').map(String::from).collect();
    let is_continuation = |i: usize| i > 0 && lines[i - 1].trim_end().ends_with('\\');
    let last_colour_line = (0..lines.len())
        .rev()
        .find(|&i| is_colour_statement(&lines[i]) && !is_continuation(i));
    let index = match last_colour_line {
        Some(i) => i + 1,
        None if lines.first().is_some_and(|l| l.starts_with("#!")) => 1,
        None => 0,
    };
    if let Some(last) = lines.last_mut().filter(|l| !l.ends_with('\n')) {
        last.push('\n');
    }
    lines.insert(index, format!("{block}\n"));
    Ok(lines.concat())
}

/// Is this a top-level shell statement that only assigns variables, at least
/// one of them a `0xAARRGGBB` colour? `PILL=0x…`, `export BLUE=0x…` and
/// `A=0x…; B=0x…` count. Command arguments (`sketchybar --bar color=0x… \`),
/// indented lines (array items, function bodies) and `\` continuations don't:
/// inserting after those would split a command in two.
pub fn is_colour_statement(line: &str) -> bool {
    let line = line.trim_end_matches(['\n', '\r']);
    if line.starts_with(char::is_whitespace) || line.trim_end().ends_with('\\') {
        return false;
    }
    let code = line.split(" #").next().unwrap_or_default();
    let mut has_colour = false;
    for statement in code.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let statement = statement
            .strip_prefix("export ")
            .unwrap_or(statement)
            .trim();
        let Some((name, value)) = statement.split_once('=') else {
            return false;
        };
        let is_name =
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !is_name || value.contains(char::is_whitespace) {
            return false;
        }
        let value = value.trim_matches(|c| c == '"' || c == '\'');
        has_colour |= value.len() == 10
            && value.starts_with("0x")
            && value[2..].chars().all(|c| c.is_ascii_hexdigit());
    }
    has_colour
}

/// Shell scripts directly in `dir` or one folder below that assign colours
/// (e.g. SketchyBar plugins), sorted.
pub fn scripts_with_colours(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut folders = vec![dir.to_path_buf()];
    if let Ok(entries) = fs::read_dir(dir) {
        folders.extend(entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
    }
    for folder in folders {
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };
        for path in entries.flatten().map(|e| e.path()) {
            let is_script = path.extension().is_some_and(|e| e == "sh") && path.is_file();
            if is_script
                && fs::read_to_string(&path).is_ok_and(|s| s.lines().any(is_colour_statement))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// The first of `candidates` that exists and has content, else the first that
/// exists. For apps that read one of several possible config files.
pub fn first_config(candidates: &[PathBuf]) -> Vec<PathBuf> {
    let existing: Vec<&PathBuf> = candidates.iter().filter(|p| p.is_file()).collect();
    let pick = existing
        .iter()
        .find(|p| fs::metadata(p).is_ok_and(|m| m.len() > 0))
        .or(existing.first());
    pick.map(|p| vec![(*p).clone()]).unwrap_or_default()
}

/// The import line `target` adds (or added) to its configs.
pub fn import_line(paths: &Paths, target: &Target, import: &Import) -> String {
    let file = target.output.as_ref().map_or("", |o| o.file);
    (import.line)(paths, &paths.current().join(file))
}

/// The block `palette init` adds for an import-wired target (marker comment
/// and line), for files Palette writes whole (`desktop`), so they count as
/// wired. `None` for other targets.
pub fn palette_block(paths: &Paths, target: &str) -> Option<String> {
    let target = targets::get(target)?;
    let Wiring::Import(import) = &target.wiring else {
        return None;
    };
    Some(format!(
        "{} {MARKER}\n{}",
        import.comment,
        import_line(paths, target, import)
    ))
}

/// A backup folder name: the UTC time, `2026-10-17T093000Z`.
pub fn timestamp_now() -> String {
    timestamp()
}

/// Does `contents` hold a marker comment followed by `line`? Matching the
/// line, not just the marker, lets two targets wire the same file
/// (sketchybarrc: colours and the bar item) without mistaking each other's
/// block for their own. "Contains" because an insert may prefix the line
/// (Alacritty's `general.import`).
pub fn has_block(contents: &str, line: &str) -> bool {
    let lines: Vec<&str> = contents.lines().collect();
    lines
        .windows(2)
        .any(|w| w[0].trim_end().ends_with(MARKER) && w[1].contains(line))
}

/// Remove `line`'s block: its marker comment and the line itself. Other
/// targets' blocks in the same file stay.
pub fn remove_block(contents: &str, line: &str) -> String {
    let lines: Vec<&str> = contents.split_inclusive('\n').collect();
    let mut out = String::new();
    let mut i = 0;
    while i < lines.len() {
        let is_ours = lines[i].trim_end().ends_with(MARKER)
            && lines.get(i + 1).is_some_and(|next| next.contains(line));
        if is_ours {
            i += 2;
        } else {
            out.push_str(lines[i]);
            i += 1;
        }
    }
    out
}

// ---------------------------------------------------------------- init / remove

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Will add (or added) the import line.
    Add,
    /// Will change (or changed) the setting to Palette's value.
    Set,
    AlreadyWired,
    /// Could not be done automatically; the message says what to do.
    Manual(String),
    /// Will remove (or removed) Palette's lines.
    Remove,
    /// Will put (or put) the user's old setting back.
    Restore,
    NotWired,
    /// No config file found for this app.
    NotFound,
    /// Switched off by the user (`palette apps off`); left alone.
    Off,
}

impl Step {
    /// Did (or would) this step change a file?
    pub fn writes(&self) -> bool {
        matches!(self, Step::Add | Step::Set | Step::Remove | Step::Restore)
    }
}

#[derive(Debug)]
pub struct Item {
    pub target: &'static str,
    pub path: Option<PathBuf>,
    pub step: Step,
    /// What `prepare` reported (VSCodium: "theme extension installed").
    pub note: Option<String>,
}

#[derive(Debug)]
pub struct Report {
    pub items: Vec<Item>,
    /// Where the originals were copied, if anything was written.
    pub backup: Option<PathBuf>,
}

/// Is this target hooked up to Palette?
pub fn is_wired(paths: &Paths, target: &Target) -> bool {
    match &target.wiring {
        Wiring::Import(import) => {
            let line = import_line(paths, target, import);
            (import.configs)(paths)
                .iter()
                .any(|p| fs::read_to_string(p).is_ok_and(|s| has_block(&s, &line)))
        }
        Wiring::Setting(setting) => (setting.configs)(paths).iter().any(|p| {
            fs::read_to_string(p)
                .is_ok_and(|text| (setting.read)(&text) == Some((setting.value)(paths)))
        }),
        Wiring::Shell => targets::get("shell").is_some_and(|shell| is_wired(paths, shell)),
    }
}

/// Does the app have a config Palette could wire (or create)?
pub fn has_config(paths: &Paths, target: &Target) -> bool {
    match &target.wiring {
        Wiring::Import(import) => (import.configs)(paths)
            .iter()
            .any(|p| import.create || p.is_file()),
        Wiring::Setting(setting) => (setting.configs)(paths).iter().any(|p| p.is_file()),
        Wiring::Shell => targets::get("shell").is_some_and(|shell| has_config(paths, shell)),
    }
}

/// Plan a write. Two targets may change the same file (sketchybarrc): the
/// second builds on the first's text, so one write carries both changes.
fn plan_write(writes: &mut Vec<Write>, write: Write) {
    match writes.iter_mut().find(|w| w.path == write.path) {
        Some(existing) => existing.text = write.text,
        None => writes.push(write),
    }
}

/// The text a file will have once earlier planned writes are applied.
fn planned_text(writes: &[Write], path: &Path) -> Option<String> {
    writes
        .iter()
        .find(|w| w.path == path)
        .map(|w| w.text.clone())
}

/// One planned file change.
struct Write {
    item: usize,
    path: PathBuf,
    text: String,
    /// For settings: the value to remember (`Some(None)` = it wasn't set).
    remember: Option<Option<String>>,
}

/// Wire every installed app (`remove: false`) or unwire it (`remove: true`).
/// Apps the user switched off are skipped when wiring (and reported as off);
/// `--remove` covers every app, so nothing of Palette's is left behind.
pub fn run(paths: &Paths, remove: bool, dry_run: bool) -> Result<Report> {
    let settings = crate::settings::load(paths)?;
    let (on, off): (Vec<&Target>, Vec<&Target>) = targets::ALL
        .iter()
        .partition(|t| remove || settings.is_on(t.name));
    let mut report = run_for(paths, &on, remove, dry_run)?;
    report.items.extend(
        off.iter()
            .filter(|t| !matches!(t.wiring, Wiring::Shell))
            .map(|t| Item {
                target: t.name,
                path: None,
                step: Step::Off,
                note: None,
            }),
    );
    Ok(report)
}

/// `run` for just these targets (`palette apps on|off <app>`).
pub fn run_for(paths: &Paths, chosen: &[&Target], remove: bool, dry_run: bool) -> Result<Report> {
    let mut items = Vec::new();
    let mut writes: Vec<Write> = Vec::new();

    for target in chosen.iter().copied() {
        match &target.wiring {
            Wiring::Import(import) => {
                plan_import(paths, target, import, remove, &mut items, &mut writes)?
            }
            Wiring::Setting(setting) => {
                plan_setting(paths, target, setting, remove, &mut items, &mut writes)?
            }
            Wiring::Shell => {}
        }
    }
    if dry_run || writes.is_empty() {
        return Ok(Report {
            items,
            backup: None,
        });
    }

    // Settings with a `prepare` step (VSCodium's extension) run it first when
    // wiring; if it fails, that setting is left alone.
    if !remove {
        writes.retain(|w| match prepare_of(&items[w.item]) {
            None => true,
            Some(prepare) => match prepare(paths, false) {
                Ok(note) => {
                    items[w.item].note = Some(note);
                    true
                }
                Err(why) => {
                    items[w.item].step = Step::Manual(why);
                    false
                }
            },
        });
    }

    let dir = backup_dir(paths);
    for w in &writes {
        back_up(paths, &dir, &w.path)?;
    }
    for w in &writes {
        write_in_place(&w.path, &w.text)?;
        let key = key_for(&items[w.item], &w.path);
        match &w.remember {
            Some(previous) => remember(paths, &key, previous)?,
            None if items[w.item].step == Step::Restore => forget(paths, &key)?,
            None => {}
        }
    }
    if remove {
        for w in &writes {
            if let Some(prepare) = prepare_of(&items[w.item]) {
                items[w.item].note = Some(prepare(paths, true).unwrap_or_else(|e| e));
            }
        }
    }
    remove_old_backups(paths)?;
    Ok(Report {
        items,
        backup: dir.is_dir().then_some(dir),
    })
}

fn prepare_of(item: &Item) -> Option<Prepare> {
    match &targets::get(item.target)?.wiring {
        Wiring::Setting(setting) => setting.prepare,
        _ => None,
    }
}

fn plan_import(
    paths: &Paths,
    target: &Target,
    import: &Import,
    remove: bool,
    items: &mut Vec<Item>,
    writes: &mut Vec<Write>,
) -> Result<()> {
    let configs: Vec<PathBuf> = (import.configs)(paths)
        .into_iter()
        .filter(|p| import.create || p.is_file())
        .collect();
    if configs.is_empty() {
        items.push(Item {
            target: target.name,
            path: None,
            step: Step::NotFound,
            note: None,
        });
    }
    let line = import_line(paths, target, import);
    for path in configs {
        let text = match planned_text(writes, &path) {
            Some(text) => text,
            None => match fs::read_to_string(&path) {
                Ok(text) => text,
                Err(e) if e.kind() == ErrorKind::NotFound => String::new(),
                Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
            },
        };
        let wired = has_block(&text, &line);
        let step = match (remove, wired) {
            (false, true) => Step::AlreadyWired,
            (true, false) => Step::NotWired,
            (true, true) => {
                plan_write(
                    writes,
                    Write {
                        item: items.len(),
                        path: path.clone(),
                        text: remove_block(&text, &line),
                        remember: None,
                    },
                );
                Step::Remove
            }
            (false, false) => {
                let block = format!("{} {MARKER}\n{line}", import.comment);
                match (import.insert)(&text, &block) {
                    Ok(new_text) => {
                        plan_write(
                            writes,
                            Write {
                                item: items.len(),
                                path: path.clone(),
                                text: new_text,
                                remember: None,
                            },
                        );
                        Step::Add
                    }
                    Err(why) => Step::Manual(why),
                }
            }
        };
        items.push(Item {
            target: target.name,
            path: Some(path),
            step,
            note: None,
        });
    }
    Ok(())
}

fn plan_setting(
    paths: &Paths,
    target: &Target,
    setting: &Setting,
    remove: bool,
    items: &mut Vec<Item>,
    writes: &mut Vec<Write>,
) -> Result<()> {
    let configs: Vec<PathBuf> = (setting.configs)(paths)
        .into_iter()
        .filter(|p| p.is_file())
        .collect();
    if configs.is_empty() {
        items.push(Item {
            target: target.name,
            path: None,
            step: Step::NotFound,
            note: None,
        });
    }
    for path in configs {
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let ours = (setting.value)(paths);
        let current = (setting.read)(&text);
        let wired = current.as_deref() == Some(ours.as_str());
        let item = items.len();
        let key = previous_key(target, setting, &path);
        let step = match (remove, wired) {
            (false, true) => Step::AlreadyWired,
            (true, false) => Step::NotWired,
            (false, false) => {
                writes.push(Write {
                    item,
                    path: path.clone(),
                    text: (setting.write)(&text, Some(&ours)),
                    remember: Some(current),
                });
                Step::Set
            }
            (true, true) => match remembered(paths, &key)? {
                Some(previous) => {
                    writes.push(Write {
                        item,
                        path: path.clone(),
                        text: (setting.write)(&text, previous.as_deref()),
                        remember: None,
                    });
                    Step::Restore
                }
                None => Step::Manual("old value unknown; set it back by hand".into()),
            },
        };
        items.push(Item {
            target: target.name,
            path: Some(path),
            step,
            note: None,
        });
    }
    Ok(())
}

/// Where a setting's old value is kept: `<target>` for single-file apps,
/// `<target>-<path>` when each file has its own (Obsidian vaults).
fn previous_key(target: &Target, setting: &Setting, path: &Path) -> String {
    if !setting.per_file {
        return target.name.to_string();
    }
    let flat: String = path
        .to_string_lossy()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{}-{}", target.name, flat.trim_start_matches('_'))
}

/// `previous_key` for a planned write.
fn key_for(item: &Item, path: &Path) -> String {
    match targets::get(item.target).map(|t| (t, &t.wiring)) {
        Some((t, Wiring::Setting(setting))) => previous_key(t, setting, path),
        _ => item.target.to_string(),
    }
}

// ---------------------------------------------------------------- remembered settings

fn previous_file(paths: &Paths, target: &str) -> PathBuf {
    paths
        .root()
        .join("wiring")
        .join(format!("{target}.previous"))
}

/// Save a setting's value from before `init` (`None`: it wasn't set). An
/// existing record wins: if the setting was changed again since the first
/// `init` (an upgrade moving to a new value), the original is still the one
/// to restore.
fn remember(paths: &Paths, target: &str, previous: &Option<String>) -> Result<()> {
    let file = previous_file(paths, target);
    if file.exists() {
        return Ok(());
    }
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    let text = match previous {
        Some(value) => format!("set\n{value}"),
        None => "unset".to_string(),
    };
    fs::write(&file, text).with_context(|| format!("writing {}", file.display()))
}

/// The value saved by `remember`: `Some(None)` means "it wasn't set".
fn remembered(paths: &Paths, target: &str) -> Result<Option<Option<String>>> {
    let file = previous_file(paths, target);
    match fs::read_to_string(&file) {
        Ok(text) => Ok(Some(text.strip_prefix("set\n").map(str::to_string))),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", file.display())),
    }
}

fn forget(paths: &Paths, target: &str) -> Result<()> {
    match fs::remove_file(previous_file(paths, target)) {
        Err(e) if e.kind() != ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

// ---------------------------------------------------------------- backups

/// A new `backup/<UTC time>` folder name, suffixed if two runs share a second
/// (so a quick `init --remove` never overwrites the originals).
fn backup_dir(paths: &Paths) -> PathBuf {
    new_backup_dir(paths, "")
}

/// A fresh `backup/<UTC time>[-label]` folder name (not yet created).
pub fn new_backup_dir(paths: &Paths, label: &str) -> PathBuf {
    let base = match label {
        "" => timestamp(),
        label => format!("{}-{label}", timestamp()),
    };
    let mut dir = paths.backups().join(&base);
    let mut n = 1;
    while dir.exists() {
        dir = paths.backups().join(format!("{base}-{n}"));
        n += 1;
    }
    dir
}

pub fn remove_old_backups(paths: &Paths) -> Result<()> {
    let Ok(entries) = fs::read_dir(paths.backups()) else {
        return Ok(());
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    let excess = dirs.len().saturating_sub(KEEP_BACKUPS);
    for old in &dirs[..excess] {
        fs::remove_dir_all(old).with_context(|| format!("removing {}", old.display()))?;
    }
    Ok(())
}

/// Copy `path` into `dir`, keeping its path relative to home.
fn back_up(paths: &Paths, dir: &Path, path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(()); // a config Palette is about to create
    }
    let rel = path
        .strip_prefix(paths.home())
        .unwrap_or_else(|_| path.strip_prefix("/").unwrap_or(path));
    let dest = dir.join(rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::copy(path, &dest).with_context(|| format!("backing up {}", path.display()))?;
    Ok(())
}

/// Replace a file's contents atomically, keeping its permissions (sketchybarrc
/// must stay executable). If the path is a symlink (dotfiles managers), the
/// file it points to is updated and the link is kept.
fn write_in_place(path: &Path, text: &str) -> Result<()> {
    if !path.exists() {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        return fs::write(path, text).with_context(|| format!("creating {}", path.display()));
    }
    let real = fs::canonicalize(path).with_context(|| format!("resolving {}", path.display()))?;
    let permissions = fs::metadata(&real)?.permissions();
    let name = real.file_name().unwrap_or_default().to_string_lossy();
    let tmp = real.with_file_name(format!(".{name}.palette-tmp"));
    fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    fs::set_permissions(&tmp, permissions)?;
    fs::rename(&tmp, &real).with_context(|| format!("updating {}", real.display()))
}

/// `2026-10-11T104412Z`, UTC (std has no local time zone support).
fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (y, m, d) = civil_from_days(days as i64);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}{:02}{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Days since 1970-01-01 to (year, month, day). Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK: &str = "# M\nLINE";

    #[test]
    fn colour_statements() {
        assert!(is_colour_statement("PILL=0xf0292e42\n"));
        assert!(is_colour_statement("export BLUE=0xff7aa2f7"));
        assert!(is_colour_statement(
            "BLUE=0xff7aa2f7; GREY=0xff737aa2; RED=0xfff7768e"
        ));
        assert!(is_colour_statement("X=\"0xff7aa2f7\"  # accent"));
        assert!(!is_colour_statement("sketchybar --set x icon.color=$BLUE"));
        assert!(!is_colour_statement(
            "sketchybar --bar height=36 color=0x00000000 \\"
        ));
        assert!(!is_colour_statement("    active_color=0xff7aa2f7"));
        assert!(!is_colour_statement("FOO=0xff7aa2f7 some_command"));
        assert!(!is_colour_statement("width=6.0"));
        assert!(!is_colour_statement("SHORT=0xfff"));
    }

    /// Regression: the owner's sketchybarrc has `color=0x…` inside a
    /// continued command after the colour block.
    #[test]
    fn never_splits_a_continued_command() {
        let src = "#!/bin/bash\nPILL=0xf0292e42\nGREY=0xff737aa2\n\n\
            sketchybar --bar height=36 color=0x00000000 \\\n  padding_left=10 shadow=off\n\
            BAR=1 \\\nX=0xff000000\n";
        let out = after_colours(src, BLOCK).unwrap();
        assert!(
            out.starts_with("#!/bin/bash\nPILL=0xf0292e42\nGREY=0xff737aa2\n# M\nLINE\n"),
            "{out}"
        );
    }

    #[test]
    fn inserts_after_last_colour_line() {
        let src = "#!/bin/bash\nA=1\nBLUE=0xff7aa2f7\nFG=0xffc0caf5\n\nuse $BLUE\n";
        let out = after_colours(src, BLOCK).unwrap();
        assert_eq!(
            out,
            "#!/bin/bash\nA=1\nBLUE=0xff7aa2f7\nFG=0xffc0caf5\n# M\nLINE\n\nuse $BLUE\n"
        );
    }

    #[test]
    fn inserts_after_shebang_or_at_top() {
        assert_eq!(
            after_colours("#!/bin/sh\necho\n", BLOCK).unwrap(),
            "#!/bin/sh\n# M\nLINE\necho\n"
        );
        assert_eq!(after_colours("echo\n", BLOCK).unwrap(), "# M\nLINE\necho\n");
        assert_eq!(after_colours("", BLOCK).unwrap(), "# M\nLINE\n");
        assert_eq!(
            after_colours("X=0xff000000", BLOCK).unwrap(),
            "X=0xff000000\n# M\nLINE\n"
        );
    }

    #[test]
    fn appends_at_end() {
        assert_eq!(at_end("a\n", BLOCK).unwrap(), "a\n# M\nLINE\n");
        assert_eq!(at_end("a", BLOCK).unwrap(), "a\n# M\nLINE\n");
    }

    #[test]
    fn remove_undoes_insert() {
        let src = "#!/bin/bash\nBLUE=0xff7aa2f7\n\nuse $BLUE\n";
        let block = format!("# {MARKER}\nsource x");
        let other = format!("# {MARKER}\nsource other");
        let wired = after_colours(src, &block).unwrap();
        assert_ne!(wired, src);
        assert!(has_block(&wired, "source x"));
        assert_eq!(remove_block(&wired, "source x"), src);
        let wired = at_end(src, &block).unwrap();
        assert_eq!(remove_block(&wired, "source x"), src);

        // A second target's block in the same file is left alone.
        let both = at_end(&wired, &other).unwrap();
        assert!(!has_block(&remove_block(&both, "source x"), "source x"));
        assert!(has_block(&remove_block(&both, "source x"), "source other"));
    }

    #[test]
    fn dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_372), (2025, 10, 11));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }
}
