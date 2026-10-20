//! Windows (AeroSpace): gaps, layout, workspaces, per-app rules and
//! keyboard shortcuts in
//! `~/.aerospace.toml`. AeroSpace can't import a second file, so Palette
//! edits this one, key by key (`toml_edit`): comments, order and every
//! setting it doesn't show (your keybindings, callbacks) stay as written.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, Table, TableLike, value};

use super::{File, Tool};
use crate::paths::Paths;
use crate::system::{self, Outcome};

/// Where AeroSpace reads its config: `~/.aerospace.toml`, else the XDG one.
pub fn config_file(paths: &Paths) -> PathBuf {
    let home = paths.home().join(".aerospace.toml");
    let xdg = paths.config_home().join("aerospace/aerospace.toml");
    if !home.exists() && xdg.exists() {
        xdg
    } else {
        home
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Gaps {
    pub inner_horizontal: i64,
    pub inner_vertical: i64,
    pub outer_left: i64,
    pub outer_right: i64,
    pub outer_top: i64,
    pub outer_bottom: i64,
    /// The top gap on displays other than the built-in screen, when it
    /// differs (a MacBook keeps windows below the notch row; an external
    /// display doesn't). Written as AeroSpace's per-monitor value:
    /// `[{ monitor."built-in" = top }, other]`.
    pub outer_top_other: Option<i64>,
}

impl Default for Gaps {
    fn default() -> Self {
        Self {
            inner_horizontal: 10,
            inner_vertical: 10,
            outer_left: 10,
            outer_right: 10,
            outer_top: 48,
            outer_top_other: None,
            outer_bottom: 10,
        }
    }
}

/// An app's windows: float, or open on a workspace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rule {
    /// Bundle id, `com.apple.finder`.
    pub app_id: String,
    /// For showing; AeroSpace matches the id.
    pub app_name: String,
    /// `float`, `tile` or `workspace`.
    pub action: String,
    /// With `workspace`.
    pub workspace: u8,
}

impl Default for Rule {
    fn default() -> Self {
        Self {
            app_id: String::new(),
            app_name: String::new(),
            action: "float".into(),
            workspace: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Windows {
    pub gaps: Gaps,
    /// `tiles` or `accordion`.
    pub layout: String,
    /// `auto`, `horizontal` or `vertical`.
    pub orientation: String,
    pub accordion_padding: i64,
    /// Workspaces 1 to N: kept alive when empty (`persistent-workspaces`),
    /// shown on the bar, each with a shortcut (modifier-N, and with shift to
    /// move a window there). Shortcuts above N that you have stay.
    pub workspaces: u8,
    /// The shortcut modifier (`alt`, `cmd-alt`, …), found in your config.
    pub modifier: String,
    pub rules: Vec<Rule>,
    pub start_at_login: bool,
    /// Every shortcut, in every binding mode. `None` (settings saved by an
    /// older Palette): read from your file by [`prepare`].
    pub shortcuts: Option<Vec<Shortcut>>,
}

/// A keyboard shortcut: in a binding mode, a key runs AeroSpace commands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shortcut {
    /// `main` (always on), or a mode a shortcut enters (`service`).
    pub mode: String,
    /// `alt-shift-h`: modifiers, then the key.
    pub key: String,
    /// One or more commands, run in order (`focus left`).
    pub run: Vec<String>,
}

impl Default for Shortcut {
    fn default() -> Self {
        Self {
            mode: "main".into(),
            key: String::new(),
            run: Vec::new(),
        }
    }
}

/// AeroSpace's key names (its guide, "Binding modes").
const KEYS: &[&str] = &[
    "minus",
    "equal",
    "period",
    "comma",
    "slash",
    "backslash",
    "quote",
    "semicolon",
    "backtick",
    "leftSquareBracket",
    "rightSquareBracket",
    "space",
    "enter",
    "esc",
    "backspace",
    "tab",
    "pageUp",
    "pageDown",
    "home",
    "end",
    "forwardDelete",
    "sectionSign",
    "keypadClear",
    "keypadDecimalMark",
    "keypadDivide",
    "keypadEnter",
    "keypadEqual",
    "keypadMinus",
    "keypadMultiply",
    "keypadPlus",
    "left",
    "down",
    "up",
    "right",
];

fn key_ok(key: &str) -> bool {
    let parts: Vec<&str> = key.split('-').collect();
    let Some((last, mods)) = parts.split_last() else {
        return false;
    };
    let mut seen = Vec::new();
    for m in mods {
        if !["alt", "cmd", "ctrl", "shift"].contains(m) || seen.contains(m) {
            return false;
        }
        seen.push(m);
    }
    let one = |c: char| last.len() == 1 && (c.is_ascii_lowercase() || c.is_ascii_digit());
    let numbered = |prefix: &str, max: u32| {
        last.strip_prefix(prefix)
            .and_then(|n| n.parse::<u32>().ok())
            .is_some_and(|n| n <= max && (prefix != "f" || n >= 1))
    };
    last.chars().next().is_some_and(one)
        || numbered("keypad", 9)
        || numbered("f", 20)
        || KEYS.contains(last)
}

/// AeroSpace's commands, with what each does: suggestions in the app, and
/// a typo check (a command AeroSpace doesn't know makes it refuse the file).
pub const COMMANDS: &[(&str, &str)] = &[
    (
        "focus left",
        "Focus the window to the left (also right, up, down)",
    ),
    ("move left", "Move the window left (also right, up, down)"),
    ("resize smart +50", "Make the window bigger (-50: smaller)"),
    ("workspace 1", "Go to a workspace"),
    ("move-node-to-workspace 1", "Send the window to a workspace"),
    (
        "workspace-back-and-forth",
        "Back to the workspace you were on",
    ),
    (
        "move-workspace-to-monitor --wrap-around next",
        "Move the workspace to the next screen",
    ),
    ("focus-monitor --wrap-around next", "Focus the next screen"),
    (
        "layout tiles horizontal vertical",
        "Tile side by side (again: flip direction)",
    ),
    (
        "layout accordion horizontal vertical",
        "Stack as an accordion (again: flip direction)",
    ),
    (
        "layout floating tiling",
        "Float the window, or tile it again",
    ),
    ("fullscreen", "Fill the screen (inside AeroSpace)"),
    (
        "macos-native-fullscreen",
        "macOS full screen, its own space",
    ),
    ("balance-sizes", "Make all windows equal"),
    (
        "flatten-workspace-tree",
        "Reset the layout of the workspace",
    ),
    ("join-with left", "Group with the window to the left"),
    ("close", "Close the window"),
    (
        "close-all-windows-but-current",
        "Close every other window on the workspace",
    ),
    (
        "exec-and-forget open -a Ghostty",
        "Open an app (or run any command)",
    ),
    ("mode service", "Switch to another binding mode"),
    ("mode main", "Back to the normal shortcuts"),
    ("reload-config", "Read the config again"),
];

const COMMAND_NAMES: &[&str] = &[
    "balance-sizes",
    "close",
    "close-all-windows-but-current",
    "config",
    "debug-windows",
    "enable",
    "exec-and-forget",
    "flatten-workspace-tree",
    "focus",
    "focus-back-and-forth",
    "focus-monitor",
    "fullscreen",
    "join-with",
    "layout",
    "list-apps",
    "list-exec-env-vars",
    "list-modes",
    "list-monitors",
    "list-windows",
    "list-workspaces",
    "macos-native-fullscreen",
    "macos-native-minimize",
    "mode",
    "move",
    "move-mouse",
    "move-node-to-monitor",
    "move-node-to-workspace",
    "move-workspace-to-monitor",
    "reload-config",
    "resize",
    "split",
    "summon-workspace",
    "swap",
    "trigger-binding",
    "volume",
    "workspace",
    "workspace-back-and-forth",
];

impl Default for Windows {
    fn default() -> Self {
        Self {
            gaps: Gaps::default(),
            layout: "tiles".into(),
            orientation: "auto".into(),
            accordion_padding: 30,
            workspaces: 5,
            modifier: "alt".into(),
            rules: Vec::new(),
            start_at_login: true,
            shortcuts: None,
        }
    }
}

impl Windows {
    pub fn validate(&self) -> Result<()> {
        if !["tiles", "accordion"].contains(&self.layout.as_str()) {
            bail!("layout is tiles or accordion, not `{}`", self.layout);
        }
        if !["auto", "horizontal", "vertical"].contains(&self.orientation.as_str()) {
            bail!("orientation is auto, horizontal or vertical");
        }
        if !(1..=9).contains(&self.workspaces) {
            bail!("workspaces is 1 to 9");
        }
        let g = &self.gaps;
        for gap in [
            g.inner_horizontal,
            g.inner_vertical,
            g.outer_left,
            g.outer_right,
            g.outer_top,
            g.outer_bottom,
        ] {
            if !(0..=400).contains(&gap) {
                bail!("gaps are 0 to 400 points");
            }
        }
        if g.outer_top_other.is_some_and(|o| !(0..=400).contains(&o)) {
            bail!("gaps are 0 to 400 points");
        }
        if !(0..=200).contains(&self.accordion_padding) {
            bail!("accordion_padding is 0 to 200");
        }
        let modifier_ok = !self.modifier.is_empty()
            && self
                .modifier
                .split('-')
                .all(|m| ["alt", "cmd", "ctrl", "shift"].contains(&m));
        if !modifier_ok {
            bail!("modifier is alt, cmd, ctrl or shift, joined with - (cmd-alt)");
        }
        for r in &self.rules {
            let id_ok = !r.app_id.is_empty()
                && r.app_id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || ".-_".contains(c));
            if !id_ok {
                bail!("`{}` is not an app id (like com.apple.finder)", r.app_id);
            }
            match r.action.as_str() {
                "float" | "tile" => {}
                "workspace" if (1..=self.workspaces).contains(&r.workspace) => {}
                "workspace" => bail!("{}: workspace {} doesn't exist", r.app_id, r.workspace),
                other => bail!("{}: `{other}` is float, tile or workspace", r.app_id),
            }
        }
        let mut seen: Vec<(&str, &str)> = Vec::new();
        for s in self.shortcuts.iter().flatten() {
            let mode_ok = !s.mode.is_empty()
                && s.mode
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_".contains(c));
            if !mode_ok {
                bail!("`{}` is not a binding mode name", s.mode);
            }
            if !key_ok(&s.key) {
                bail!(
                    "`{}` is not a shortcut AeroSpace knows: modifiers (alt, cmd, ctrl, shift) then a key, like alt-shift-h",
                    s.key
                );
            }
            if seen.contains(&(s.mode.as_str(), s.key.as_str())) {
                bail!("{} is used twice in {} mode", s.key, s.mode);
            }
            seen.push((&s.mode, &s.key));
            if s.run.is_empty()
                || s.run
                    .iter()
                    .any(|c| c.trim().is_empty() || c.contains('\n'))
            {
                bail!("{}: what it runs, one command per line", s.key);
            }
        }
        Ok(())
    }

    /// Commands AeroSpace may not know (likely typos). Not refused: a newer
    /// AeroSpace may have commands this list lacks.
    pub fn command_warnings(&self) -> Vec<String> {
        self.shortcuts
            .iter()
            .flatten()
            .flat_map(|s| s.run.iter().map(move |c| (s, c)))
            .filter(|(_, c)| {
                let name = c.split_whitespace().next().unwrap_or("");
                !COMMAND_NAMES.contains(&name)
            })
            .map(|(s, c)| {
                format!(
                    "{}: `{c}` isn't an AeroSpace command Palette knows (a typo?)",
                    s.key
                )
            })
            .collect()
    }
}

/// Fill in what the saved settings lack: the shortcuts from your file (if
/// settings from an older Palette don't have them), and a shortcut to go to
/// and move to each of the workspaces 1..=N (keys already taken are kept).
pub fn prepare(paths: &Paths, w: &mut Windows) {
    if w.shortcuts.is_none() {
        w.shortcuts = Some(read(paths).map(|d| shortcuts(&d)).unwrap_or_default());
    }
    let list = w.shortcuts.get_or_insert_with(Vec::new);
    for n in 1..=w.workspaces {
        for (key, cmd) in [
            (format!("{}-{n}", w.modifier), format!("workspace {n}")),
            (
                format!("{}-shift-{n}", w.modifier),
                format!("move-node-to-workspace {n}"),
            ),
        ] {
            if !list.iter().any(|s| s.mode == "main" && s.key == key) {
                list.push(Shortcut {
                    mode: "main".into(),
                    key,
                    run: vec![cmd],
                });
            }
        }
    }
}

/// The commands of a binding: `'focus left'` or `['a', 'b']`. `None` for
/// anything else (left in the file as it is).
fn commands(item: &Item) -> Option<Vec<String>> {
    let v = item.as_value()?;
    if let Some(s) = v.as_str() {
        return Some(vec![s.to_string()]);
    }
    v.as_array()?
        .iter()
        .map(|c| c.as_str().map(str::to_string))
        .collect()
}

/// Every `[mode.<name>.binding]` shortcut, in file order.
fn shortcuts(doc: &DocumentMut) -> Vec<Shortcut> {
    let Some(modes) = doc.get("mode").and_then(|m| m.as_table_like()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (mode, item) in modes.iter() {
        let Some(table) = item.get("binding").and_then(|b| b.as_table_like()) else {
            continue;
        };
        for (key, value) in table.iter() {
            if let Some(run) = commands(value) {
                out.push(Shortcut {
                    mode: mode.to_string(),
                    key: key.to_string(),
                    run,
                });
            }
        }
    }
    out
}

/// Start from your aerospace.toml.
pub fn import(paths: &Paths) -> (Windows, Vec<String>) {
    let mut w = Windows::default();
    let mut warnings = Vec::new();
    let Ok(doc) = read(paths) else {
        return (w, warnings);
    };
    let int = |path: &[&str]| -> Option<i64> { get(doc.as_item(), path)?.as_integer() };
    let mut g_other = None;
    let g = &mut w.gaps;
    for (field, path) in [
        (&mut g.inner_horizontal, ["gaps", "inner", "horizontal"]),
        (&mut g.inner_vertical, ["gaps", "inner", "vertical"]),
        (&mut g.outer_left, ["gaps", "outer", "left"]),
        (&mut g.outer_right, ["gaps", "outer", "right"]),
        (&mut g.outer_top, ["gaps", "outer", "top"]),
        (&mut g.outer_bottom, ["gaps", "outer", "bottom"]),
    ] {
        match get(doc.as_item(), &path) {
            Some(item) if item.is_integer() => *field = item.as_integer().unwrap_or(*field),
            Some(item) if path[2] == "top" && top_per_display(item).is_some() => {
                if let Some((top, other)) = top_per_display(item) {
                    *field = top;
                    g_other = Some(other);
                }
            }
            Some(_) => warnings.push(format!(
                "gaps.{}.{} is set per monitor in your config: Palette leaves it as it is",
                path[1], path[2]
            )),
            None => {}
        }
    }
    w.gaps.outer_top_other = g_other;
    let text = |key: &str| doc.get(key).and_then(|i| i.as_str()).map(str::to_string);
    if let Some(l) = text("default-root-container-layout") {
        w.layout = l;
    }
    if let Some(o) = text("default-root-container-orientation") {
        w.orientation = o;
    }
    if let Some(p) = int(&["accordion-padding"]) {
        w.accordion_padding = p;
    }
    if let Some(b) = doc.get("start-at-login").and_then(|i| i.as_bool()) {
        w.start_at_login = b;
    }
    if let Some(bindings) = bindings(&doc) {
        let found: Vec<(String, u8)> = bindings
            .iter()
            .filter_map(|(key, item)| {
                let (modifier, n) = key.rsplit_once('-')?;
                let n: u8 = n.parse().ok()?;
                (item.as_str()? == format!("workspace {n}")).then(|| (modifier.to_string(), n))
            })
            .collect();
        if let Some((modifier, _)) = found.first() {
            w.modifier = modifier.clone();
        }
        if let Some(max) = found.iter().map(|(_, n)| *n).max() {
            w.workspaces = max.clamp(1, 9);
        }
    }
    // The ones kept alive are the ones you use (the bar shows these).
    if let Some(list) = doc.get("persistent-workspaces").and_then(|i| i.as_array())
        && let Some(n) = list
            .iter()
            .filter_map(|v| v.as_str()?.parse::<u8>().ok())
            .max()
    {
        w.workspaces = n.clamp(1, 9);
    }
    w.rules = rules(&doc)
        .into_iter()
        .filter_map(|(_, rule)| rule)
        .collect();
    w.shortcuts = Some(shortcuts(&doc));
    (w, warnings)
}

fn read(paths: &Paths) -> Result<DocumentMut> {
    let file = config_file(paths);
    let text = fs::read_to_string(&file).unwrap_or_default();
    text.parse::<DocumentMut>()
        .with_context(|| format!("{} is not valid TOML", file.display()))
}

fn get<'a>(item: &'a Item, path: &[&str]) -> Option<&'a Item> {
    path.iter().try_fold(item, |at, key| at.get(*key))
}

fn bindings(doc: &DocumentMut) -> Option<&dyn TableLike> {
    get(doc.as_item(), &["mode", "main", "binding"])?.as_table_like()
}

/// `[[on-window-detected]]` entries with the rule each is, if it is one
/// Palette shows: only `if.app-id`, and `run` floating, tiling or moving to
/// a workspace. Anything else (more conditions, several commands) is yours.
fn rules(doc: &DocumentMut) -> Vec<(usize, Option<Rule>)> {
    let Some(list) = doc
        .get("on-window-detected")
        .and_then(|i| i.as_array_of_tables())
    else {
        return Vec::new();
    };
    list.iter()
        .enumerate()
        .map(|(i, t)| (i, as_rule(t)))
        .collect()
}

fn as_rule(t: &Table) -> Option<Rule> {
    if t.len() != 2 {
        return None;
    }
    let cond = t.get("if")?.as_table_like()?;
    if cond.len() != 1 {
        return None;
    }
    let app_id = cond.get("app-id")?.as_str()?.to_string();
    let run = t.get("run")?.as_str()?;
    let (action, workspace) = match run {
        "layout floating" => ("float", 1),
        "layout tiling" => ("tile", 1),
        _ => (
            "workspace",
            run.strip_prefix("move-node-to-workspace ")?.parse().ok()?,
        ),
    };
    Some(Rule {
        app_name: app_id.rsplit('.').next().unwrap_or(&app_id).to_string(),
        app_id,
        action: action.into(),
        workspace,
    })
}

/// `[{ monitor."built-in" = 4 }, 32]` → (4, 32): the built-in screen's
/// top gap and everyone else's. `None` for any other per-monitor value.
fn top_per_display(item: &Item) -> Option<(i64, i64)> {
    let list = item.as_array()?;
    if list.len() != 2 {
        return None;
    }
    let first = list.get(0)?.as_inline_table()?;
    let monitor = first.get("monitor")?.as_inline_table()?;
    if monitor.len() != 1 {
        return None;
    }
    let built_in = monitor.get("built-in")?.as_integer()?;
    Some((built_in, list.get(1)?.as_integer()?))
}

/// Equal as values, whatever the quoting: `'tiles'` and `"tiles"`.
fn same(a: &toml_edit::Value, b: &toml_edit::Value) -> bool {
    match (a, b) {
        (toml_edit::Value::Array(x), toml_edit::Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| same(p, q))
        }
        _ => {
            a.type_name() == b.type_name()
                && a.as_str() == b.as_str()
                && a.as_integer() == b.as_integer()
                && a.as_bool() == b.as_bool()
                && a.as_float() == b.as_float()
        }
    }
}

/// Set a value, keeping the comment after it. An equal value is left as
/// written, so a save changes only what you changed.
pub(super) fn put(item: &mut Item, new: toml_edit::Value) {
    match item.as_value_mut() {
        Some(old) if same(old, &new) => {}
        Some(old) => {
            let decor = old.decor().clone();
            *old = new;
            *old.decor_mut() = decor;
        }
        None => *item = Item::Value(new),
    }
}

/// Your aerospace.toml with these settings in it.
pub fn render(paths: &Paths, w: &Windows) -> Result<(Vec<File>, Vec<String>)> {
    let mut doc = read(paths)?;
    let mut warnings = Vec::new();

    let gaps = &w.gaps;
    for (v, path) in [
        (gaps.inner_horizontal, ["gaps", "inner", "horizontal"]),
        (gaps.inner_vertical, ["gaps", "inner", "vertical"]),
        (gaps.outer_left, ["gaps", "outer", "left"]),
        (gaps.outer_right, ["gaps", "outer", "right"]),
        (gaps.outer_top, ["gaps", "outer", "top"]),
        (gaps.outer_bottom, ["gaps", "outer", "bottom"]),
    ] {
        let item = &mut doc[path[0]][path[1]][path[2]];
        let ours = item.is_none()
            || item.is_integer()
            || (path[2] == "top" && top_per_display(item).is_some());
        if !ours {
            continue; // per monitor in a way of your own: yours
        }
        match gaps.outer_top_other.filter(|o| path[2] == "top" && *o != v) {
            Some(other) => {
                if top_per_display(item) != Some((v, other)) {
                    let snippet: DocumentMut =
                        format!("v = [{{ monitor.\"built-in\" = {v} }}, {other}]")
                            .parse()
                            .expect("a valid per-display gap");
                    let new = snippet["v"].as_value().cloned().expect("a value");
                    put(item, new);
                }
            }
            None => put(item, v.into()),
        }
    }
    // `doc["gaps"]["inner"]` made fresh would print as [gaps.inner]: dotted reads better.
    for sub in ["inner", "outer"] {
        if let Some(t) = doc["gaps"][sub].as_table_mut()
            && t.position().is_none()
        {
            t.set_dotted(true);
        }
    }
    put(
        &mut doc["default-root-container-layout"],
        w.layout.as_str().into(),
    );
    put(
        &mut doc["default-root-container-orientation"],
        w.orientation.as_str().into(),
    );
    put(&mut doc["accordion-padding"], w.accordion_padding.into());
    put(&mut doc["start-at-login"], w.start_at_login.into());

    if let Some(list) = &w.shortcuts {
        write_shortcuts(&mut doc, list);
    }
    workspaces(&mut doc, w, &mut warnings);
    write_rules(&mut doc, &w.rules);
    warnings.extend(w.command_warnings());

    // The bar's workspaces update on these: a workspace change, and a focus
    // change (moving a window to a new workspace changes focus, not workspace).
    if super::is_managed(paths, Tool::Bar) && doc.get("on-focus-changed").is_none() {
        let mut cmd = toml_edit::Array::new();
        cmd.push("exec-and-forget sketchybar --trigger aerospace_workspace_change");
        doc["on-focus-changed"] = value(cmd);
    }
    if super::is_managed(paths, Tool::Bar) && doc.get("exec-on-workspace-change").is_none() {
        let mut cmd = toml_edit::Array::new();
        for part in [
            "/bin/bash",
            "-c",
            "sketchybar --trigger aerospace_workspace_change FOCUSED_WORKSPACE=$AEROSPACE_FOCUSED_WORKSPACE",
        ] {
            cmd.push(part);
        }
        doc["exec-on-workspace-change"] = value(cmd);
    }

    Ok((
        vec![File {
            path: config_file(paths),
            text: doc.to_string(),
            executable: false,
        }],
        warnings,
    ))
}

/// modifier-N and modifier-shift-N for 1..=workspaces (shortcuts above
/// that stay: spare workspaces). Keys you bound to something else are left
/// alone, with a warning. `persistent-workspaces` lists 1..=workspaces.
fn workspaces(doc: &mut DocumentMut, w: &Windows, warnings: &mut Vec<String>) {
    let binding = &mut doc["mode"]["main"]["binding"];
    if binding.is_none() {
        *binding = Item::Table(Table::new());
    }
    let Some(table) = binding.as_table_like_mut() else {
        warnings.push("[mode.main.binding] isn't a table: workspace shortcuts unchanged".into());
        return;
    };
    for n in 1..=9u8 {
        for (key, cmd) in [
            (format!("{}-{n}", w.modifier), format!("workspace {n}")),
            (
                format!("{}-shift-{n}", w.modifier),
                format!("move-node-to-workspace {n}"),
            ),
        ] {
            let current = table.get(&key).map(|i| i.as_str().map(str::to_string));
            match current {
                Some(Some(c)) if c == cmd => {}
                Some(_) => {
                    if n <= w.workspaces {
                        warnings.push(format!(
                            "{key} already does something else in your config: kept"
                        ));
                    }
                }
                None if n <= w.workspaces => {
                    // Indented like the keys around it.
                    let indent = indent_of(table);
                    table.insert(&key, value(cmd));
                    if let Some(mut k) = table.key_mut(&key) {
                        k.leaf_decor_mut().set_prefix(indent);
                    }
                }
                None => {}
            }
        }
    }
    // Since config-version 2 (AeroSpace 0.19); older configs would reject it.
    let has = doc
        .get("persistent-workspaces")
        .is_some_and(|i| i.is_array());
    let v2 = doc
        .get("config-version")
        .and_then(|i| i.as_integer())
        .unwrap_or(1)
        >= 2;
    if has || v2 {
        let mut list = toml_edit::Array::new();
        for n in 1..=w.workspaces {
            list.push(n.to_string());
        }
        put(&mut doc["persistent-workspaces"], list.into());
    }
}

/// The shortcuts in the file become exactly `wanted`: unchanged ones stay
/// as written (quotes, comments), changed ones are rewritten in place, new
/// ones go at the end of their mode, and removed ones go (a comment above a
/// removed one moves to the next key, so section notes stay).
fn write_shortcuts(doc: &mut DocumentMut, wanted: &[Shortcut]) {
    let mut modes: Vec<String> = doc
        .get("mode")
        .and_then(|m| m.as_table_like())
        .map(|m| m.iter().map(|(k, _)| k.to_string()).collect())
        .unwrap_or_default();
    for s in wanted {
        if !modes.contains(&s.mode) {
            modes.push(s.mode.clone());
        }
    }
    for mode in modes {
        let mine: Vec<&Shortcut> = wanted.iter().filter(|s| s.mode == mode).collect();
        let binding = &mut doc["mode"][&mode]["binding"];
        if binding.is_none() {
            if mine.is_empty() {
                continue;
            }
            *binding = Item::Table(Table::new());
        }
        let Some(table) = binding.as_table_like_mut() else {
            continue;
        };
        let gone: Vec<String> = table
            .iter()
            .filter(|(k, v)| commands(v).is_some() && !mine.iter().any(|s| s.key == *k))
            .map(|(k, _)| k.to_string())
            .collect();
        for key in gone {
            let keys: Vec<String> = table.iter().map(|(k, _)| k.to_string()).collect();
            let comment = table
                .key(&key)
                .and_then(|k| k.leaf_decor().prefix()?.as_str().map(str::to_string))
                .filter(|p| p.contains('#'));
            table.remove(&key);
            let next = keys.iter().skip_while(|k| **k != key).nth(1);
            if let (Some(comment), Some(next)) = (comment, next)
                && let Some(mut k) = table.key_mut(next)
            {
                let rest = k
                    .leaf_decor()
                    .prefix()
                    .and_then(|p| p.as_str())
                    .unwrap_or("")
                    .to_string();
                let head = &comment[..comment.rfind('\n').map_or(0, |i| i + 1)];
                k.leaf_decor_mut().set_prefix(format!("{head}{rest}"));
            }
        }
        for s in mine {
            let new: toml_edit::Value = if s.run.len() == 1 {
                s.run[0].as_str().into()
            } else {
                let mut a = toml_edit::Array::new();
                for c in &s.run {
                    a.push(c.as_str());
                }
                a.into()
            };
            match table.get_mut(&s.key) {
                Some(item) if commands(item).as_ref() == Some(&s.run) => {}
                Some(item) => put(item, new),
                None => {
                    let indent = indent_of(table);
                    table.insert(&s.key, Item::Value(new));
                    if let Some(mut k) = table.key_mut(&s.key) {
                        k.leaf_decor_mut().set_prefix(indent);
                    }
                }
            }
        }
    }
}

/// The indentation of a table's last key, for keys added to it.
fn indent_of(table: &dyn TableLike) -> String {
    table
        .iter()
        .last()
        .and_then(|(k, _)| {
            table
                .key(k)?
                .leaf_decor()
                .prefix()?
                .as_str()
                .map(str::to_string)
        })
        .map(|p| p.rsplit('\n').next().unwrap_or("").to_string())
        .unwrap_or_default()
}

fn write_rules(doc: &mut DocumentMut, wanted: &[Rule]) {
    let ours: Vec<usize> = rules(doc)
        .into_iter()
        .filter(|(_, r)| r.is_some())
        .map(|(i, _)| i)
        .collect();
    if ours.is_empty() && wanted.is_empty() {
        return;
    }
    let item = &mut doc["on-window-detected"];
    if item.is_none() {
        *item = Item::ArrayOfTables(toml_edit::ArrayOfTables::new());
    }
    let Some(list) = item.as_array_of_tables_mut() else {
        return;
    };
    for i in ours.into_iter().rev() {
        list.remove(i);
    }
    for r in wanted {
        let mut cond = Table::new();
        cond.set_dotted(true);
        cond.insert("app-id", value(r.app_id.as_str()));
        let mut t = Table::new();
        t.insert("if", Item::Table(cond));
        let run = match r.action.as_str() {
            "float" => "layout floating".to_string(),
            "tile" => "layout tiling".to_string(),
            _ => format!("move-node-to-workspace {}", r.workspace),
        };
        t.insert("run", value(run));
        list.push(t);
    }
    if list.is_empty() {
        doc.remove("on-window-detected");
    }
}

/// `aerospace reload-config`.
pub fn reload(_: &Paths) -> Outcome {
    if !system::is_running("AeroSpace") && !system::is_running("aerospace") {
        return Outcome::Skipped("not running".into());
    }
    match system::run("aerospace", &["reload-config"]) {
        Ok(_) => {
            // The bar redraws its workspaces only on AeroSpace's events: tell
            // it now, or a new workspace count shows only after the next switch.
            if system::is_running("sketchybar") {
                let _ = system::run("sketchybar", &["--trigger", "aerospace_workspace_change"]);
            }
            Outcome::Done("reloaded".into())
        }
        Err(e) => Outcome::Failed(e),
    }
}

/// Running apps (bundle id, name), for picking a rule's app.
pub fn running_apps() -> Vec<(String, String)> {
    let Ok(out) = system::run(
        "aerospace",
        &["list-apps", "--format", "%{app-bundle-id}|%{app-name}"],
    ) else {
        return Vec::new();
    };
    let mut apps: Vec<(String, String)> = out
        .lines()
        .filter_map(|l| {
            let (id, name) = l.split_once('|')?;
            (!id.trim().is_empty()).then(|| (id.trim().to_string(), name.trim().to_string()))
        })
        .collect();
    apps.sort_by_key(|a| a.1.to_lowercase());
    apps.dedup();
    apps
}

#[cfg(test)]
mod tests {
    use super::*;

    const STARTER: &str = include_str!("../tools/starters/aerospace/aerospace.toml");

    fn home(text: &str) -> (tempfile::TempDir, Paths) {
        let h = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(h.path());
        fs::write(paths.home().join(".aerospace.toml"), text).unwrap();
        (h, paths)
    }

    #[test]
    fn imports_the_starter() {
        let (_h, paths) = home(STARTER);
        let (w, warnings) = import(&paths);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(w.gaps.outer_top, 48);
        assert_eq!(w.workspaces, 5);
        assert_eq!(w.modifier, "alt");
        assert_eq!(w.layout, "tiles");
        assert!(w.rules.is_empty());
    }

    #[test]
    fn edits_keep_everything_else() {
        let (_h, paths) = home(STARTER);
        let (mut w, _) = import(&paths);
        w.gaps.inner_horizontal = 6;
        w.workspaces = 7;
        w.layout = "accordion".into();
        w.rules = vec![
            Rule {
                app_id: "com.apple.finder".into(),
                action: "float".into(),
                ..Rule::default()
            },
            Rule {
                app_id: "com.spotify.client".into(),
                action: "workspace".into(),
                workspace: 7,
                ..Rule::default()
            },
        ];
        w.validate().unwrap();
        let (files, warnings) = render(&paths, &w).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let text = &files[0].text;
        // Untouched: comments, bindings, the service mode.
        assert!(text.contains("# Gaps: the big top gap leaves room for a floating SketchyBar."));
        assert!(text.contains("alt-h = 'focus left'"));
        assert!(text.contains("backspace = ['close-all-windows-but-current', 'mode main']"));
        // Changed in place.
        assert!(text.contains("inner.horizontal = 6\n"), "{text}");
        assert!(
            text.contains("default-root-container-layout = \"accordion\""),
            "{text}"
        );
        assert!(text.contains("alt-7 = \"workspace 7\""));
        assert!(text.contains("alt-shift-6 = \"move-node-to-workspace 6\""));
        assert!(text.contains("if.app-id = \"com.apple.finder\""), "{text}");
        assert!(text.contains("run = \"move-node-to-workspace 7\""));
        // And it reads back the same.
        fs::write(config_file(&paths), text).unwrap();
        let (back, _) = import(&paths);
        assert_eq!(back.workspaces, 7);
        assert_eq!(back.rules.len(), 2);
        assert_eq!(back.gaps.inner_horizontal, 6);

        // Fewer workspaces: spare shortcuts stay.
        w.workspaces = 3;
        w.rules.clear();
        let text = &render(&paths, &w).unwrap().0[0].text;
        assert!(text.contains("alt-4 = 'workspace 4'"));
        assert!(text.contains("alt-3 = 'workspace 3'"));
        assert!(!text.contains("on-window-detected"));
    }

    #[test]
    fn the_owners_config() {
        let (_h, paths) = home(include_str!("../../tests/fixtures/aerospace-owner.toml"));
        let (mut w, warnings) = import(&paths);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(
            w.workspaces, 6,
            "persistent-workspaces, not the 9 shortcuts"
        );
        assert_eq!((w.gaps.outer_top, w.gaps.inner_vertical), (12, 8));
        w.workspaces = 7;
        w.gaps.outer_top = 14;
        let text = &render(&paths, &w).unwrap().0[0].text;
        assert!(text.contains("gaps.outer.top    = 14\n"), "{text}");
        assert!(
            text.contains(
                "persistent-workspaces = [\"1\", \"2\", \"3\", \"4\", \"5\", \"6\", \"7\"]"
            ),
            "{text}"
        );
        assert!(text.contains("    alt-9 = 'workspace 9'"));
        assert!(text.contains("# Possible values: tiles|accordion"));
        assert!(text.contains("alt-shift-b = 'balance-sizes'"));
        // Nothing else moved: only the two lines differ.
        let before = include_str!("../../tests/fixtures/aerospace-owner.toml");
        let changed = before
            .lines()
            .zip(text.lines())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(changed, 2, "{text}");
        assert_eq!(before.lines().count(), text.lines().count());
    }

    #[test]
    fn shortcuts_change_key_by_key() {
        let before = include_str!("../../tests/fixtures/aerospace-owner.toml");
        let (_h, paths) = home(before);
        let (mut w, _) = import(&paths);
        prepare(&paths, &mut w);
        let list = w.shortcuts.as_mut().unwrap();
        assert!(list.iter().any(|s| s.mode == "service"
            && s.key == "esc"
            && s.run == ["reload-config", "mode main"]));
        // Rebind, change a command, remove one with a comment above it, add two.
        list.iter_mut().find(|s| s.key == "alt-f").unwrap().key = "alt-shift-f".into();
        list.iter_mut().find(|s| s.key == "alt-tab").unwrap().run =
            vec!["focus-monitor next".into()];
        list.retain(|s| s.key != "alt-slash");
        list.push(Shortcut {
            mode: "main".into(),
            key: "alt-enter".into(),
            run: vec!["exec-and-forget open -a Ghostty".into()],
        });
        list.push(Shortcut {
            mode: "service".into(),
            key: "b".into(),
            run: vec!["balance-sizes".into(), "mode main".into()],
        });
        w.validate().unwrap();
        let (files, warnings) = render(&paths, &w).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let text = &files[0].text;
        assert!(
            text.contains("    alt-shift-f = \"fullscreen\"\n"),
            "{text}"
        );
        assert!(!text.contains("\n    alt-f ="));
        assert!(text.contains("alt-tab = \"focus-monitor next\""));
        assert!(!text.contains("alt-slash ="));
        // The comment above the removed key stays, on the next one.
        assert!(
            text.contains(
                "# See: https://nikitabobko.github.io/AeroSpace/commands#layout\n    alt-comma"
            ),
            "{text}"
        );
        assert!(text.contains("    alt-enter = \"exec-and-forget open -a Ghostty\""));
        assert!(
            text.contains("    b = [\"balance-sizes\", \"mode main\"]"),
            "{text}"
        );
        // Untouched ones keep their quotes.
        assert!(text.contains("    alt-h = 'focus left'"));
        assert!(text.contains("    esc = ['reload-config', 'mode main']"));
        // It reads back the same.
        fs::write(config_file(&paths), text).unwrap();
        let (mut back, _) = import(&paths);
        prepare(&paths, &mut back);
        // (A renamed shortcut moves to the end of its mode.)
        let sorted = |w: &Windows| {
            let mut l = w.shortcuts.clone().unwrap();
            l.sort_by(|a, b| (&a.mode, &a.key).cmp(&(&b.mode, &b.key)));
            l
        };
        assert_eq!(sorted(&back), sorted(&w));
    }

    #[test]
    fn a_top_gap_per_display() {
        let (_h, paths) = home(include_str!("../../tests/fixtures/aerospace-owner.toml"));
        let (mut w, _) = import(&paths);
        w.gaps.outer_top = 4;
        w.gaps.outer_top_other = Some(40);
        let text = &render(&paths, &w).unwrap().0[0].text;
        assert!(
            text.contains("gaps.outer.top    = [{ monitor.\"built-in\" = 4 }, 40]\n"),
            "{text}"
        );
        fs::write(config_file(&paths), text).unwrap();
        let (back, warnings) = import(&paths);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(
            (back.gaps.outer_top, back.gaps.outer_top_other),
            (4, Some(40))
        );
        // Same on every display again: a plain number.
        w.gaps.outer_top_other = None;
        assert!(
            render(&paths, &w).unwrap().0[0]
                .text
                .contains("gaps.outer.top    = 4\n")
        );
    }

    #[test]
    fn shortcut_checks() {
        for ok in [
            "alt-h",
            "cmd-alt-shift-1",
            "alt-leftSquareBracket",
            "f12",
            "ctrl-keypad5",
            "alt-minus",
        ] {
            assert!(key_ok(ok), "{ok}");
        }
        for bad in [
            "",
            "alt-",
            "hyper-h",
            "alt-alt-h",
            "alt-f21",
            "alt-Enter",
            "alt-hh",
        ] {
            assert!(!key_ok(bad), "{bad}");
        }
        let mut w = Windows::default();
        let s = |key: &str, run: &str| Shortcut {
            mode: "main".into(),
            key: key.into(),
            run: vec![run.into()],
        };
        w.shortcuts = Some(vec![s("alt-h", "focus left"), s("alt-h", "focus right")]);
        assert!(w.validate().is_err(), "twice in one mode");
        w.shortcuts = Some(vec![s("alt-h", "focuss left")]);
        w.validate().unwrap();
        assert_eq!(w.command_warnings().len(), 1, "a typo is a warning");
    }

    #[test]
    fn workspaces_get_their_shortcuts() {
        let (_h, paths) = home(STARTER);
        let (mut w, _) = import(&paths);
        w.workspaces = 7;
        prepare(&paths, &mut w);
        let keys: Vec<&str> = w
            .shortcuts
            .iter()
            .flatten()
            .map(|s| s.key.as_str())
            .collect();
        assert!(keys.contains(&"alt-7") && keys.contains(&"alt-shift-7"));
        // Settings from before shortcuts existed still load, from the file.
        let mut old = Windows {
            shortcuts: None,
            ..w.clone()
        };
        prepare(&paths, &mut old);
        assert!(old.shortcuts.unwrap().iter().any(|s| s.key == "alt-h"));
    }

    #[test]
    fn leaves_your_own_rules_and_bindings() {
        let mine = format!(
            "{STARTER}\n[[on-window-detected]]\nif.app-id = 'com.apple.Terminal'\nif.window-title-regex-substring = 'vim'\nrun = 'layout floating'\n"
        )
        .replace("alt-5 = 'workspace 5'", "alt-5 = 'exec-and-forget open -a Mail'");
        let (_h, paths) = home(&mine);
        let (mut w, _) = import(&paths);
        assert!(w.rules.is_empty(), "two conditions: not one of Palette's");
        w.workspaces = 5;
        let (files, warnings) = render(&paths, &w).unwrap();
        assert!(
            files[0]
                .text
                .contains("if.window-title-regex-substring = 'vim'")
        );
        assert!(
            files[0]
                .text
                .contains("alt-5 = 'exec-and-forget open -a Mail'")
        );
        assert!(warnings.iter().any(|w| w.contains("alt-5")), "{warnings:?}");
    }

    #[test]
    fn refuses_bad_values() {
        let bad = [
            Windows {
                layout: "spiral".into(),
                ..Default::default()
            },
            Windows {
                workspaces: 12,
                ..Default::default()
            },
            Windows {
                modifier: "hyper; rm".into(),
                ..Default::default()
            },
            Windows {
                rules: vec![Rule {
                    app_id: "a b".into(),
                    ..Rule::default()
                }],
                ..Default::default()
            },
        ];
        for w in bad {
            assert!(w.validate().is_err(), "{w:?}");
        }
    }
}
