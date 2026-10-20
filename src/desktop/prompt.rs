//! Prompt (Starship): `~/.config/starship.toml`. Palette builds the prompt
//! from coloured blocks of modules joined by a separator shape, and writes
//! it key by key like AeroSpace: `format`, `add_newline`, and the `style`,
//! `format` and options of the modules shown. Everything else in the file
//! (other modules' settings, palettes, comments) stays as written.
//! Colours are theme colour names, so Palette's coloured copy of the file
//! (the `starship` target) follows the theme.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item};

use super::File;
use crate::paths::Paths;
use crate::system::{self, Outcome};

pub fn config_file(paths: &Paths) -> PathBuf {
    paths.config_home().join("starship.toml")
}

/// A module Palette can place, with the text it shows inside a block.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Module {
    pub id: &'static str,
    pub name: &'static str,
    pub what: &'static str,
    /// Inside `[ … ]($style)`.
    #[serde(skip)]
    body: &'static str,
    /// Off unless turned on (`disabled = false` is written when shown).
    #[serde(skip)]
    off_by_default: bool,
    /// A language: shows its version when `versions` is on.
    pub language: bool,
}

const fn m(id: &'static str, name: &'static str, what: &'static str, body: &'static str) -> Module {
    Module {
        id,
        name,
        what,
        body,
        off_by_default: false,
        language: false,
    }
}

const fn lang(id: &'static str, name: &'static str) -> Module {
    Module {
        id,
        name,
        what: "Shown in a project that uses it",
        body: "$symbol( $version)",
        off_by_default: false,
        language: true,
    }
}

const fn off(mut module: Module) -> Module {
    module.off_by_default = true;
    module
}

pub const MODULES: &[Module] = &[
    off(m("os", "System logo", "Your Mac's logo", "$symbol")),
    m(
        "username",
        "User",
        "Your user name (over SSH or as root)",
        "$user",
    ),
    m(
        "hostname",
        "Computer",
        "The computer's name (over SSH)",
        "$hostname",
    ),
    m("directory", "Folder", "Where you are", "$path"),
    m(
        "git_branch",
        "Git branch",
        "The branch, in a git repo",
        "$symbol $branch",
    ),
    m(
        "git_status",
        "Git status",
        "Changes, ahead and behind",
        "$all_status$ahead_behind",
    ),
    m(
        "git_state",
        "Git state",
        "Rebasing, merging, …",
        "$state( $progress_current/$progress_total)",
    ),
    lang("nodejs", "Node.js"),
    lang("bun", "Bun"),
    lang("deno", "Deno"),
    lang("rust", "Rust"),
    lang("golang", "Go"),
    Module {
        body: "$symbol( $version)( \\($virtualenv\\))",
        ..lang("python", "Python")
    },
    lang("java", "Java"),
    lang("kotlin", "Kotlin"),
    lang("c", "C"),
    lang("php", "PHP"),
    lang("ruby", "Ruby"),
    lang("lua", "Lua"),
    lang("swift", "Swift"),
    lang("zig", "Zig"),
    lang("haskell", "Haskell"),
    lang("elixir", "Elixir"),
    lang("dotnet", ".NET"),
    m(
        "docker_context",
        "Docker",
        "The Docker context",
        "$symbol( $context)",
    ),
    m(
        "conda",
        "Conda",
        "The Conda environment",
        "$symbol( $environment)",
    ),
    off(m("time", "Time", "The time", "$time")),
    m(
        "cmd_duration",
        "Took",
        "How long the last command took",
        "$duration",
    ),
    m("jobs", "Jobs", "Background jobs", "$symbol$number"),
    off(m("memory_usage", "Memory", "Memory in use", "$symbol $ram")),
    off(m(
        "status",
        "Exit code",
        "When the last command failed",
        "$symbol$status",
    )),
];

pub fn find_module(id: &str) -> Option<&'static Module> {
    MODULES.iter().find(|m| m.id == id)
}

/// An option of a module the page shows: (module, key, kind, label).
/// `kind` is `int`, `text` or `bool`.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Opt {
    pub module: &'static str,
    pub key: &'static str,
    pub kind: &'static str,
    pub label: &'static str,
}

const fn opt(
    module: &'static str,
    key: &'static str,
    kind: &'static str,
    label: &'static str,
) -> Opt {
    Opt {
        module,
        key,
        kind,
        label,
    }
}

pub const OPTIONS: &[Opt] = &[
    opt(
        "directory",
        "truncation_length",
        "int",
        "Folders shown (0: all)",
    ),
    opt("directory", "truncation_symbol", "text", "Shortened with"),
    opt(
        "directory",
        "truncate_to_repo",
        "bool",
        "Start at the repo's folder",
    ),
    opt("directory", "home_symbol", "text", "Home shown as"),
    opt("git_branch", "symbol", "text", "Symbol"),
    opt(
        "git_branch",
        "truncation_length",
        "int",
        "Longest branch name",
    ),
    opt(
        "time",
        "time_format",
        "text",
        "Format (%R 14:05 · %T with seconds · %I:%M %p 2:05 PM)",
    ),
    opt(
        "cmd_duration",
        "min_time",
        "int",
        "Show after (milliseconds)",
    ),
    opt(
        "cmd_duration",
        "show_milliseconds",
        "bool",
        "With milliseconds",
    ),
    opt(
        "character",
        "success_symbol",
        "text",
        "After a command worked",
    ),
    opt(
        "character",
        "error_symbol",
        "text",
        "After a command failed",
    ),
    opt(
        "username",
        "show_always",
        "bool",
        "Always, not just over SSH or as root",
    ),
    opt("hostname", "ssh_only", "bool", "Only over SSH"),
    opt("docker_context", "symbol", "text", "Symbol"),
    opt("conda", "symbol", "text", "Symbol"),
    opt("jobs", "symbol", "text", "Symbol"),
    opt("memory_usage", "symbol", "text", "Symbol"),
];

/// The options a module has on the page (languages: their symbol).
pub fn options_of(module: &str) -> Vec<Opt> {
    let mut out: Vec<Opt> = OPTIONS
        .iter()
        .filter(|o| o.module == module)
        .copied()
        .collect();
    if let Some(m) = find_module(module).filter(|m| m.language) {
        out.push(opt(m.id, "symbol", "text", "Symbol"));
    }
    out
}

/// A coloured block of modules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Block {
    /// Background (theme colour name or `#rrggbb`).
    pub colour: String,
    /// Text colour.
    pub text: String,
    pub modules: Vec<String>,
}

impl Default for Block {
    fn default() -> Self {
        Self {
            colour: "lighter_background".into(),
            text: "foreground".into(),
            modules: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prompt {
    /// Between blocks: `arrow`, `round`, `slant`, `flat` (touching), or
    /// `plain` (no backgrounds: each block's colour is its text colour).
    pub shape: String,
    /// The left end: `fade` (░▒▓), `round`, `arrow` or `none`.
    pub start: String,
    pub blocks: Vec<Block>,
    /// The ❯ on a line of its own.
    pub second_line: bool,
    /// A blank line between prompts.
    pub add_newline: bool,
    /// Languages show their version.
    pub versions: bool,
    /// Module options the page shows (`OPTIONS`), by module then key.
    pub options: BTreeMap<String, toml::Table>,
}

impl Default for Prompt {
    /// The owner's layout: logo, folder, git, languages, time.
    fn default() -> Self {
        let block = |colour: &str, text: &str, modules: &[&str]| Block {
            colour: colour.into(),
            text: text.into(),
            modules: modules.iter().map(|s| s.to_string()).collect(),
        };
        Self {
            shape: "arrow".into(),
            start: "fade".into(),
            blocks: vec![
                block("foreground", "darker_background", &["os"]),
                block("blue", "bright_foreground", &["directory"]),
                block("muted", "blue", &["git_branch", "git_status"]),
                block(
                    "lighter_background",
                    "blue",
                    &["nodejs", "bun", "rust", "golang", "php"],
                ),
                block("background", "foreground", &["time"]),
            ],
            second_line: true,
            add_newline: true,
            versions: true,
            // Only the folder you're in: a full path crowds a small terminal.
            options: BTreeMap::from([(
                "directory".into(),
                toml::Table::from_iter([("truncation_length".into(), 1.into())]),
            )]),
        }
    }
}

const SHAPES: [&str; 5] = ["arrow", "round", "slant", "flat", "plain"];
const STARTS: [&str; 4] = ["fade", "round", "arrow", "none"];

/// Theme colour names, `#rrggbb`, or names Palette maps (`peach`).
fn colour_ok(c: &str) -> bool {
    crate::theme::colour_key(c).is_some() || crate::theme::is_hex_colour(c)
}

/// Starship's own module names: letters, digits, `_`, and `.` for
/// `custom.x` / `env_var.X`.
fn module_ok(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.".contains(c))
}

impl Prompt {
    pub fn validate(&self) -> Result<()> {
        if !SHAPES.contains(&self.shape.as_str()) {
            bail!("shape is {}, not `{}`", SHAPES.join(", "), self.shape);
        }
        if !STARTS.contains(&self.start.as_str()) {
            bail!("start is {}, not `{}`", STARTS.join(", "), self.start);
        }
        let mut seen = Vec::new();
        for b in &self.blocks {
            for c in [&b.colour, &b.text] {
                if !colour_ok(c) {
                    bail!("`{c}` is not a theme colour or #rrggbb");
                }
            }
            for id in &b.modules {
                if !module_ok(id) || id == "character" || id == "line_break" {
                    bail!("`{id}` can't go in a block");
                }
                if seen.contains(id) {
                    bail!("{id} is in the prompt twice");
                }
                seen.push(id.clone());
            }
        }
        for (module, table) in &self.options {
            for (key, v) in table {
                let Some(o) = options_of(module).into_iter().find(|o| o.key == key) else {
                    bail!("{module}.{key} is not an option the page knows");
                };
                let ok = match o.kind {
                    "int" => v.as_integer().is_some_and(|n| (0..=100_000).contains(&n)),
                    "bool" => v.is_bool(),
                    _ => v.as_str().is_some_and(|s| !s.contains('\n')),
                };
                if !ok {
                    bail!("{module}.{key}: {} expected", o.kind);
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- format

const ARROW: char = '\u{e0b0}';
const ARROW_LEFT: char = '\u{e0b2}';
const ROUND: char = '\u{e0b4}';
const ROUND_LEFT: char = '\u{e0b6}';
const SLANT_STR: &str = "\u{e0bc}";

/// Separator glyph between blocks and at the right end.
fn sep(shape: &str) -> &'static str {
    match shape {
        "arrow" => "\u{e0b0}",
        "round" => "\u{e0b4}",
        "slant" => SLANT_STR,
        _ => "",
    }
}

fn start_cap(start: &str) -> &'static str {
    match start {
        "fade" => "░▒▓",
        "round" => "\u{e0b6}",
        "arrow" => "\u{e0b2}",
        _ => "",
    }
}

/// The pieces of the top-level `format`, one per line in the file.
fn format_pieces(p: &Prompt) -> Vec<String> {
    let blocks: Vec<&Block> = p.blocks.iter().filter(|b| !b.modules.is_empty()).collect();
    let mut out = Vec::new();
    let mods = |b: &Block| {
        b.modules
            .iter()
            .map(|m| format!("${m}"))
            .collect::<Vec<_>>()
    };
    if p.shape == "plain" {
        for b in &blocks {
            out.extend(mods(b));
        }
    } else if let Some(first) = blocks.first() {
        let cap = start_cap(&p.start);
        if !cap.is_empty() {
            out.push(format!("[{cap}]({})", first.colour));
        }
        for (i, b) in blocks.iter().enumerate() {
            if i > 0 {
                let s = sep(&p.shape);
                out.push(format!(
                    "[{s}](fg:{} bg:{})",
                    blocks[i - 1].colour,
                    b.colour
                ));
            }
            out.extend(mods(b));
        }
        let last = blocks.last().map_or("background", |b| b.colour.as_str());
        out.push(format!("[{} ](fg:{last})", sep(&p.shape)));
    }
    out.push(if p.second_line {
        "$line_break$character".into()
    } else {
        "$character".into()
    });
    out
}

/// `format = """…"""` with one piece per line, like hand-written configs.
fn format_item(p: &Prompt) -> Result<Item> {
    let raw = format!("format = \"\"\"\n{}\"\"\"\n", format_pieces(p).join("\\\n"));
    let doc: DocumentMut = raw.parse().context("building the prompt format")?;
    Ok(doc["format"].clone())
}

/// The keys a module takes its style from: `username` has two (for you
/// and for root) and no `style`, which Starship rejects.
fn style_keys(id: &str) -> &'static [&'static str] {
    match id {
        "username" => &["style_user", "style_root"],
        _ => &["style"],
    }
}

/// A module's own `style` and `format` for its block.
fn module_look(p: &Prompt, b: &Block, module: &Module) -> (String, String) {
    let body = if module.language && !p.versions {
        "$symbol"
    } else {
        module.body
    };
    if p.shape == "plain" {
        (format!("fg:{}", b.colour), format!("[{body}]($style) "))
    } else {
        (
            format!("bg:{} fg:{}", b.colour, b.text),
            format!("[ {body} ]($style)"),
        )
    }
}

// ---------------------------------------------------------------- import

enum Tok {
    /// `[text](style)`
    Group(String, String),
    /// `$name`
    Var(String),
    Text(String),
}

fn tokens(format: &str) -> Vec<Tok> {
    let chars: Vec<char> = format.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut text = String::new();
    let flush = |text: &mut String, out: &mut Vec<Tok>| {
        if !text.is_empty() {
            out.push(Tok::Text(std::mem::take(text)));
        }
    };
    while i < chars.len() {
        match chars[i] {
            '\\' if i + 1 < chars.len() => {
                text.push(chars[i + 1]);
                i += 2;
            }
            '$' => {
                flush(&mut text, &mut out);
                let start = i + 1;
                let (name, next) = if chars.get(start) == Some(&'{') {
                    let end = chars[start..]
                        .iter()
                        .position(|c| *c == '}')
                        .map_or(chars.len(), |e| start + e);
                    (chars[start + 1..end].iter().collect(), end + 1)
                } else {
                    let end = chars[start..]
                        .iter()
                        .position(|c| !(c.is_ascii_alphanumeric() || *c == '_'))
                        .map_or(chars.len(), |e| start + e);
                    (chars[start..end].iter().collect::<String>(), end)
                };
                out.push(Tok::Var(name));
                i = next;
            }
            '[' => {
                // Up to the matching `](`, allowing nested groups inside.
                let mut depth = 0;
                let mut j = i;
                let mut close = None;
                while j < chars.len() {
                    match chars[j] {
                        '\\' => j += 1,
                        '[' => depth += 1,
                        ']' => {
                            depth -= 1;
                            if depth == 0 {
                                close = Some(j);
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
                let Some(close) = close.filter(|c| chars.get(c + 1) == Some(&'(')) else {
                    text.push('[');
                    i += 1;
                    continue;
                };
                let end = chars[close + 2..]
                    .iter()
                    .position(|c| *c == ')')
                    .map_or(chars.len(), |e| close + 2 + e);
                flush(&mut text, &mut out);
                out.push(Tok::Group(
                    chars[i + 1..close].iter().collect(),
                    chars[close + 2..end].iter().collect(),
                ));
                i = end + 1;
            }
            c => {
                text.push(c);
                i += 1;
            }
        }
    }
    flush(&mut text, &mut out);
    out
}

/// `fg:blue bg:muted bold` → the value after `key:`; a bare colour word
/// counts as `fg`.
fn style_colour(style: &str, key: &str) -> Option<String> {
    const WORDS: [&str; 6] = ["bold", "italic", "underline", "dimmed", "inverted", "none"];
    style
        .split_whitespace()
        .find_map(|w| match w.split_once(':') {
            Some((k, v)) if k == key => Some(v.to_string()),
            // `$style` points elsewhere: not a colour.
            None if key == "fg" && !WORDS.contains(&w) && !w.starts_with('$') => {
                Some(w.to_string())
            }
            _ => None,
        })
}

/// The first text colour in a format's groups, looking inside nested ones
/// (`[[ $branch ](fg:blue bg:muted)]($style)` → blue).
fn format_fg(format: &str) -> Option<String> {
    tokens(format).into_iter().find_map(|tok| match tok {
        Tok::Group(text, style) => format_fg(&text).or_else(|| style_colour(&style, "fg")),
        _ => None,
    })
}

/// Text that is only separator glyphs and spaces (not content).
fn is_separator(text: &str) -> bool {
    !text.trim().is_empty()
        && text.chars().all(|c| {
            c.is_whitespace() || "░▒▓".contains(c) || ('\u{e0b0}'..='\u{e0d7}').contains(&c)
        })
}

fn shape_of(text: &str) -> Option<&'static str> {
    text.chars().find_map(|c| match c {
        ARROW | '\u{e0b1}' => Some("arrow"),
        ROUND | '\u{e0b5}' => Some("round"),
        '\u{e0b8}'..='\u{e0bf}' => Some("slant"),
        _ => None,
    })
}

fn start_of(text: &str) -> &'static str {
    if text.contains('░') || text.contains('▓') {
        "fade"
    } else if text.contains(ROUND_LEFT) {
        "round"
    } else if text.contains(ARROW_LEFT) {
        "arrow"
    } else {
        "none"
    }
}

/// Start from your starship.toml.
pub fn import(paths: &Paths) -> (Prompt, Vec<String>) {
    match fs::read_to_string(config_file(paths)) {
        Ok(text) => from_text(&text),
        Err(_) => (Prompt::default(), Vec::new()),
    }
}

/// Read a config's prompt into blocks.
pub fn from_text(text: &str) -> (Prompt, Vec<String>) {
    read_prompt(text, false)
}

/// `keep`: leave colours as written (a preset's own names and hex, mapped
/// to theme colours afterwards) instead of replacing unknown ones.
fn read_prompt(text: &str, keep: bool) -> (Prompt, Vec<String>) {
    let mut p = Prompt::default();
    let mut warnings = Vec::new();
    let Ok(doc) = text.parse::<DocumentMut>() else {
        warnings.push("your starship.toml has a mistake: starting from Palette's default".into());
        return (p, warnings);
    };
    if let Some(b) = doc.get("add_newline").and_then(Item::as_bool) {
        p.add_newline = b;
    }
    // Options the page shows: yours, over Palette's defaults.
    for o in OPTIONS.iter().copied().chain(
        MODULES
            .iter()
            .filter(|m| m.language)
            .map(|m| opt(m.id, "symbol", "text", "Symbol")),
    ) {
        if let Some(v) = doc
            .get(o.module)
            .and_then(|t| t.get(o.key))
            .and_then(Item::as_value)
        {
            let v = match v {
                toml_edit::Value::String(s) => toml::Value::String(s.value().clone()),
                toml_edit::Value::Integer(n) => toml::Value::Integer(*n.value()),
                toml_edit::Value::Boolean(b) => toml::Value::Boolean(*b.value()),
                _ => continue,
            };
            p.options
                .entry(o.module.to_string())
                .or_default()
                .insert(o.key.to_string(), v);
        }
    }
    let Some(format) = doc.get("format").and_then(Item::as_str) else {
        return (p, warnings); // starship's default prompt: start from Palette's
    };

    // A module's text colour, from its style or its format's inner style.
    let text_of = |id: &str| -> Option<String> {
        let t = doc.get(id)?;
        let from_style = t
            .get(style_keys(id)[0])
            .and_then(Item::as_str)
            .and_then(|s| style_colour(s, "fg"));
        from_style.or_else(|| t.get("format").and_then(Item::as_str).and_then(format_fg))
    };
    let bg_of = |id: &str| -> Option<String> {
        let s = doc.get(id)?.get(style_keys(id)[0]).and_then(Item::as_str)?;
        style_colour(s, "bg").or_else(|| style_colour(s, "fg"))
    };

    let toks = tokens(format);
    let powerline = toks.iter().any(
        |t| matches!(t, Tok::Group(text, style) if is_separator(text) && style.contains("bg:")),
    );
    let mut blocks: Vec<Block> = Vec::new();
    let mut second_line = false;
    let mut start = "none";
    let mut shape: Option<&str> = None;
    let mut pending: Option<String> = None; // colour of the next block
    let mut vars: Vec<String> = Vec::new();
    for tok in &toks {
        match tok {
            Tok::Group(text, style) if is_separator(text) => {
                if let Some(s) = shape_of(text) {
                    shape.get_or_insert(s);
                }
                match style_colour(style, "bg") {
                    Some(bg) => {
                        blocks.push(Block {
                            colour: bg,
                            ..Block::default()
                        });
                    }
                    None if blocks.is_empty() && pending.is_none() => {
                        start = start_of(text);
                        pending = style_colour(style, "fg");
                    }
                    None => pending = None, // the end cap
                }
            }
            Tok::Group(text, _) => {
                for t in tokens(text) {
                    if let Tok::Var(v) = t {
                        vars.push(v.clone());
                        place(&mut blocks, &mut pending, &v, powerline, &bg_of);
                    }
                }
            }
            Tok::Var(v) if v == "line_break" => second_line = true,
            Tok::Var(v) if v == "character" || v == "fill" || v == "all" => {}
            Tok::Var(v) => {
                vars.push(v.clone());
                place(&mut blocks, &mut pending, v, powerline, &bg_of);
            }
            Tok::Text(t) if t.contains('\n') => second_line = true,
            Tok::Text(_) => {}
        }
    }
    blocks.retain(|b| !b.modules.is_empty());
    for b in &mut blocks {
        b.text = b
            .modules
            .first()
            .and_then(|m| text_of(m))
            .filter(|c| keep || colour_ok(c))
            .unwrap_or_else(|| "foreground".into());
        if !keep && !colour_ok(&b.colour) {
            warnings.push(format!(
                "`{}` isn't a theme colour: shown as lighter_background",
                b.colour
            ));
            b.colour = "lighter_background".into();
        }
    }
    if blocks.is_empty() {
        warnings
            .push("Palette couldn't read your prompt's format: starting from its default".into());
        return (p, warnings);
    }
    for v in &vars {
        if find_module(v).is_none() {
            warnings.push(format!(
                "${v}: Palette keeps it in the prompt but doesn't restyle it"
            ));
        }
    }
    p.shape = if powerline {
        shape.unwrap_or("flat").into()
    } else {
        "plain".into()
    };
    p.start = start.into();
    p.blocks = blocks;
    // `$line_break` in the format does nothing while the module is off.
    let line_break_off = doc
        .get("line_break")
        .and_then(|t| t.get("disabled"))
        .and_then(Item::as_bool)
        == Some(true);
    p.second_line = second_line && !line_break_off;
    p.versions = !MODULES.iter().filter(|m| m.language).any(|m| {
        doc.get(m.id)
            .and_then(|t| t.get("format"))
            .and_then(Item::as_str)
            .is_some_and(|f| !f.contains("$version"))
    });
    (p, warnings)
}

/// Put a module into the current block (a new one if there is none).
fn place(
    blocks: &mut Vec<Block>,
    pending: &mut Option<String>,
    id: &str,
    powerline: bool,
    bg_of: &dyn Fn(&str) -> Option<String>,
) {
    if !powerline {
        // Plain: one block per module, its colour the module's text colour.
        blocks.push(Block {
            colour: bg_of(id).unwrap_or_else(|| "foreground".into()),
            modules: vec![id.to_string()],
            ..Block::default()
        });
        return;
    }
    if let Some(colour) = pending.take() {
        blocks.push(Block {
            colour,
            ..Block::default()
        });
    }
    if blocks.is_empty() {
        blocks.push(Block {
            colour: bg_of(id).unwrap_or_else(|| "lighter_background".into()),
            ..Block::default()
        });
    }
    if let Some(b) = blocks.last_mut()
        && !b.modules.iter().any(|m| m == id)
    {
        b.modules.push(id.to_string());
    }
}

// ---------------------------------------------------------------- render

fn read(paths: &Paths) -> Result<DocumentMut> {
    let file = config_file(paths);
    let text = fs::read_to_string(&file).unwrap_or_default();
    text.parse::<DocumentMut>()
        .with_context(|| format!("{} is not valid TOML", file.display()))
}

/// Your starship.toml with this prompt in it.
pub fn render(paths: &Paths, p: &Prompt) -> Result<Vec<File>> {
    let mut doc = read(paths)?;
    apply(&mut doc, p)?;
    Ok(vec![File {
        path: config_file(paths),
        text: doc.to_string(),
        executable: false,
    }])
}

fn apply(doc: &mut DocumentMut, p: &Prompt) -> Result<()> {
    let format = format_item(p)?;
    let same_format = doc.get("format").and_then(Item::as_str) == format.as_str();
    if !same_format {
        // Placed first, like hand-written configs; keep a comment above it.
        let decor = doc
            .key("format")
            .and_then(|k| k.leaf_decor().prefix().cloned());
        doc.insert("format", format);
        if let (Some(decor), Some(mut key)) = (decor, doc.key_mut("format")) {
            key.leaf_decor_mut().set_prefix(decor);
        }
    }
    super::windows::put(&mut doc["add_newline"], p.add_newline.into());
    if p.second_line
        && doc
            .get("line_break")
            .and_then(|t| t.get("disabled"))
            .is_some()
    {
        super::windows::put(&mut doc["line_break"]["disabled"], false.into());
    }
    for b in &p.blocks {
        for id in &b.modules {
            let Some(module) = find_module(id) else {
                continue; // yours: its own style and format stay
            };
            let (style, format) = module_look(p, b, module);
            let table = &mut doc[id];
            if table.is_none() {
                let mut t = toml_edit::Table::new();
                t.set_implicit(false);
                *table = Item::Table(t);
            }
            for key in style_keys(id) {
                super::windows::put(&mut table[*key], style.as_str().into());
            }
            if !style_keys(id).contains(&"style")
                && let Some(t) = table.as_table_like_mut()
            {
                t.remove("style"); // written by the first Prompt page; Starship warns
            }
            super::windows::put(&mut table["format"], format.as_str().into());
            if module.off_by_default {
                super::windows::put(&mut table["disabled"], false.into());
            }
        }
    }
    for (module, opts) in &p.options {
        // Only modules in the prompt (the ❯ always is): no stray tables.
        let shown = module == "character" || p.blocks.iter().any(|b| b.modules.contains(module));
        if !shown {
            continue;
        }
        for (key, v) in opts {
            let new: toml_edit::Value = match v {
                toml::Value::String(s) => s.as_str().into(),
                toml::Value::Integer(n) => (*n).into(),
                toml::Value::Boolean(b) => (*b).into(),
                _ => continue,
            };
            if doc.get(module.as_str()).is_none_or(Item::is_none) {
                doc[module.as_str()] = Item::Table(toml_edit::Table::new());
            }
            super::windows::put(&mut doc[module.as_str()][key.as_str()], new);
        }
    }
    Ok(())
}

/// Palette's coloured copy of the file follows your file: render it again.
pub fn reload(paths: &Paths) -> Outcome {
    match crate::render_output(paths, "starship.toml") {
        Ok(()) => Outcome::Done("next prompt".into()),
        Err(e) => Outcome::Failed(format!("{e:#}")),
    }
}

// ---------------------------------------------------------------- presets

/// Starship's built-in presets (`starship preset --list`).
pub fn presets() -> Vec<String> {
    system::run("starship", &["preset", "--list"])
        .map(|out| {
            out.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// A preset's prompt, in the theme's colours: its colour names mapped by
/// Palette's starship palette (`peach` → `orange`), anything else (hex,
/// the preset's own palette names) to the nearest theme colour.
pub fn preset(paths: &Paths, name: &str) -> Result<(Prompt, String, Vec<String>)> {
    if !presets().iter().any(|p| p == name) {
        bail!(
            "`{name}` is not one of Starship's presets (`palette desktop prompt preset` lists them)"
        );
    }
    let text = system::run("starship", &["preset", name]).map_err(anyhow::Error::msg)?;
    let (mut p, warnings) = read_prompt(&text, true);
    let palette = preset_palette(&text);
    let theme = active_colours(paths)?;
    for b in &mut p.blocks {
        for c in [&mut b.colour, &mut b.text] {
            *c = to_theme(c, &palette, &theme);
        }
    }
    p.validate()?;
    Ok((p, text, warnings))
}

/// The preset's own selected palette: name → `#rrggbb`.
fn preset_palette(text: &str) -> BTreeMap<String, String> {
    let Ok(doc) = text.parse::<toml::Table>() else {
        return BTreeMap::new();
    };
    let selected = doc.get("palette").and_then(|p| p.as_str()).unwrap_or("");
    doc.get("palettes")
        .and_then(|p| p.get(selected))
        .and_then(|t| t.as_table())
        .map(|t| {
            t.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// The active theme's colours: key → `#rrggbb`.
fn active_colours(paths: &Paths) -> Result<Vec<(String, String)>> {
    let name = crate::state::current_theme(paths)?.context("no theme is active yet")?;
    let theme = crate::theme::load(paths, &name)?;
    Ok(theme
        .colors
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect())
}

/// A colour as a theme colour name.
fn to_theme(c: &str, palette: &BTreeMap<String, String>, theme: &[(String, String)]) -> String {
    if crate::theme::colour_key(c).is_some() {
        return c.to_string();
    }
    if let Some((_, key)) = crate::targets::starship::aliases()
        .into_iter()
        .find(|(name, _)| name == c)
        && crate::theme::colour_key(&key).is_some()
    {
        return key;
    }
    let hex = if crate::theme::is_hex_colour(c) {
        c.to_string()
    } else if let Some(h) = palette.get(c).filter(|h| crate::theme::is_hex_colour(h)) {
        h.clone()
    } else {
        return "foreground".into();
    };
    nearest(&hex, theme)
}

/// The theme colour that looks closest (OKLab).
fn nearest(hex: &str, theme: &[(String, String)]) -> String {
    let target = crate::colour::Lch::from_hex(hex);
    theme
        .iter()
        .min_by(|a, b| {
            let da = target.distance(&crate::colour::Lch::from_hex(&a.1));
            let db = target.distance(&crate::colour::Lch::from_hex(&b.1));
            da.total_cmp(&db)
        })
        .map_or_else(|| "foreground".into(), |(k, _)| k.clone())
}

// ---------------------------------------------------------------- preview

/// A run of prompt text in one style.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Span {
    pub text: String,
    /// `#rrggbb`, an ANSI colour name (`green`), or empty for the default.
    pub fg: String,
    pub bg: String,
    pub bold: bool,
}

/// The real prompt for these settings, as Starship would draw it in the
/// active theme (in your home folder, after a 4 s command).
pub fn preview(paths: &Paths, p: &Prompt) -> Result<Vec<Span>> {
    p.validate()?;
    let mut doc = read(paths)?;
    apply(&mut doc, p)?;
    let themed = crate::targets::starship::themed(paths, &doc.to_string())?;
    let file = paths.root().join("desktop/prompt-preview.toml");
    super::write_atomic(&file, &themed, false)?;
    let home = paths.home().to_string_lossy().into_owned();
    let out = system::run_env(
        "starship",
        &[
            "prompt",
            "--path",
            &home,
            "--logical-path",
            &home,
            "--cmd-duration",
            "4200",
            "--status",
            "0",
            "--terminal-width",
            "120",
        ],
        &[
            ("STARSHIP_CONFIG", Some(&file.to_string_lossy())),
            ("PWD", Some(&home)),
            ("STARSHIP_SHELL", None),
            ("STARSHIP_LOG", Some("error")),
        ],
    )
    .map_err(anyhow::Error::msg)?;
    Ok(ansi_spans(out.trim_start_matches('\n')))
}

/// SGR colour codes to spans (24-bit, 256 and the 16 named colours).
pub fn ansi_spans(text: &str) -> Vec<Span> {
    const NAMES: [&str; 8] = [
        "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
    ];
    let mut spans: Vec<Span> = Vec::new();
    let mut cur = Span {
        text: String::new(),
        fg: String::new(),
        bg: String::new(),
        bold: false,
    };
    let mut rest = text;
    while let Some(i) = rest.find('\u{1b}') {
        cur.text.push_str(&rest[..i]);
        rest = &rest[i..];
        // ESC [ codes m ; other escapes (shell wrappers) are skipped.
        let Some(end) = rest.find('m').filter(|_| rest[1..].starts_with('[')) else {
            rest = &rest[1..];
            continue;
        };
        let codes: Vec<u32> = rest[2..end]
            .split(';')
            .map(|c| c.parse().unwrap_or(0))
            .collect();
        rest = &rest[end + 1..];
        if !cur.text.is_empty() {
            spans.push(cur.clone());
            cur.text.clear();
        }
        let mut k = 0;
        while k < codes.len() {
            match codes[k] {
                0 => {
                    cur.fg.clear();
                    cur.bg.clear();
                    cur.bold = false;
                }
                1 => cur.bold = true,
                22 => cur.bold = false,
                39 => cur.fg.clear(),
                49 => cur.bg.clear(),
                c @ 30..=37 => cur.fg = NAMES[(c - 30) as usize].into(),
                c @ 90..=97 => cur.fg = format!("bright_{}", NAMES[(c - 90) as usize]),
                c @ 40..=47 => cur.bg = NAMES[(c - 40) as usize].into(),
                c @ 100..=107 => cur.bg = format!("bright_{}", NAMES[(c - 100) as usize]),
                38 | 48 if codes.get(k + 1) == Some(&2) && k + 4 < codes.len() => {
                    let hex = format!(
                        "#{:02x}{:02x}{:02x}",
                        codes[k + 2],
                        codes[k + 3],
                        codes[k + 4]
                    );
                    if codes[k] == 38 {
                        cur.fg = hex;
                    } else {
                        cur.bg = hex;
                    }
                    k += 4;
                }
                38 | 48 if codes.get(k + 1) == Some(&5) => {
                    let n = codes.get(k + 2).copied().unwrap_or(0);
                    let name = if n < 8 {
                        NAMES[n as usize].to_string()
                    } else if n < 16 {
                        format!("bright_{}", NAMES[(n - 8) as usize])
                    } else {
                        String::new()
                    };
                    if codes[k] == 38 {
                        cur.fg = name;
                    } else {
                        cur.bg = name;
                    }
                    k += 2;
                }
                _ => {}
            }
            k += 1;
        }
    }
    cur.text.push_str(rest);
    if !cur.text.is_empty() {
        spans.push(cur);
    }
    spans
}

#[cfg(test)]
mod tests;
