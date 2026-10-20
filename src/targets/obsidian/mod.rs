//! Obsidian: a CSS snippet in every vault, on top of the vault's own theme.
//!
//! Catch: while Obsidian runs it keeps appearance settings in memory and
//! writes them back to appearance.json, undoing an outside edit. So with
//! Obsidian open, Palette only writes the snippet file and asks the user to
//! switch it on in Obsidian once (Obsidian then saves it itself). Once on,
//! Obsidian re-applies the file live whenever Palette rewrites it.
//! `init` adds "palette" to each vault's enabled snippets
//! (`.obsidian/appearance.json`) and writes `.obsidian/snippets/palette.css`;
//! each switch rewrites that file and Obsidian re-applies it live. Vaults
//! come from Obsidian's own list (`obsidian.json`). No plugin needed.

use std::fs;
use std::path::{Path, PathBuf};

use super::{BASE, Note, Output, Target};
use crate::paths::Paths;
use crate::system::{self, Outcome};
use crate::wiring::{Setting, Wiring};

pub const TARGET: Target = Target {
    name: "obsidian",
    output: Some(Output {
        file: "obsidian.css",
        template: include_str!("template.css"),
        // CSS can `@import` from the web; a theme mustn't make Obsidian phone home.
        overridable: false,
        finish: None,
    }),
    wiring: Wiring::Setting(Setting {
        configs,
        per_file: true,
        value: |_| SNIPPET.into(),
        read,
        write,
        prepare: Some(prepare),
    }),
    installed: |paths| super::has_app(paths, "Obsidian.app"),
    reload,
    unload: Some(|_| Outcome::Unchanged("snippet switched off in every vault".into())),
    doctor,
    ..BASE
};

const SNIPPET: &str = "palette";
const KEY: &str = "\"enabledCssSnippets\"";
/// First line of every snippet Palette writes, so it never touches a
/// `palette.css` the user made.
const HEADER: &str = "/* Palette: ";

// ---------------------------------------------------------------- vaults

/// Vault folders from Obsidian's list, that still exist.
pub fn vaults(paths: &Paths) -> Vec<PathBuf> {
    let list = paths
        .home()
        .join("Library/Application Support/obsidian/obsidian.json");
    let Ok(text) = fs::read_to_string(list) else {
        return Vec::new();
    };
    let mut vaults: Vec<PathBuf> = json_strings(&text, "\"path\"")
        .into_iter()
        .map(PathBuf::from)
        .filter(|p| p.join(".obsidian").is_dir())
        .collect();
    vaults.sort();
    vaults.dedup();
    vaults
}

/// Each vault's `appearance.json` (Obsidian writes it once any appearance
/// setting has been changed).
fn configs(paths: &Paths) -> Vec<PathBuf> {
    vaults(paths)
        .into_iter()
        .map(|v| v.join(".obsidian/appearance.json"))
        .collect()
}

fn snippet_file(vault: &Path) -> PathBuf {
    vault.join(".obsidian/snippets/palette.css")
}

fn is_ours(file: &Path) -> bool {
    fs::read_to_string(file).is_ok_and(|s| s.starts_with(HEADER))
}

// ---------------------------------------------------------------- appearance.json

/// String values of `key` anywhere in a JSON text. obsidian.json is written
/// by Obsidian itself, so a small scanner does; no JSON crate needed.
fn json_strings(text: &str, key: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find(key) {
        rest = &rest[i + key.len()..];
        let after = rest.trim_start();
        let Some(after) = after.strip_prefix(':') else {
            continue;
        };
        if let Some((value, _)) = json_string(after.trim_start()) {
            out.push(value);
        }
    }
    out
}

/// A JSON string literal at the start of `s`: (value, length in bytes).
fn json_string(s: &str) -> Option<(String, usize)> {
    let mut chars = s.char_indices();
    if chars.next()?.1 != '"' {
        return None;
    }
    let mut out = String::new();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Some((out, i + 1)),
            '\\' => match chars.next()?.1 {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'u' => {
                    let hex: String = (0..4)
                        .filter_map(|_| chars.next().map(|(_, c)| c))
                        .collect();
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

/// Byte range of the `enabledCssSnippets` array, brackets included.
fn array_span(text: &str) -> Option<(usize, usize)> {
    let key = text.find(KEY)?;
    let mut i = key + KEY.len();
    let rest = &text[i..];
    let colon = rest.find(':')?;
    if !rest[..colon].trim().is_empty() {
        return None;
    }
    i += colon + 1;
    i += text[i..].len() - text[i..].trim_start().len();
    if !text[i..].starts_with('[') {
        return None;
    }
    let start = i;
    let mut in_string = false;
    let mut escaped = false;
    for (j, c) in text[start..].char_indices() {
        match (in_string, c) {
            (true, '\\') if !escaped => escaped = true,
            (true, '"') if !escaped => in_string = false,
            (true, _) => escaped = false,
            (false, '"') => in_string = true,
            (false, ']') => return Some((start, start + j + 1)),
            _ => {}
        }
    }
    None
}

fn names(array: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = array.trim_start_matches('[');
    while let Some(i) = rest.find('"') {
        let Some((name, len)) = json_string(&rest[i..]) else {
            break;
        };
        out.push(name);
        rest = &rest[i + len..];
    }
    out
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// `Some("palette")` when Palette's snippet is on; otherwise the array as
/// written (remembered, so `--remove` restores it exactly), or `None` when
/// the vault has no snippet list at all.
fn read(text: &str) -> Option<String> {
    let (start, end) = array_span(text)?;
    let array = &text[start..end];
    if names(array).iter().any(|n| n == SNIPPET) {
        Some(SNIPPET.into())
    } else {
        Some(array.to_string())
    }
}

/// `Some("palette")`: add the snippet. `Some("[…]")`: put a remembered array
/// back. `None`: delete the list Palette added.
fn write(text: &str, value: Option<&str>) -> String {
    let span = array_span(text);
    match (value, span) {
        (Some(raw), Some((start, end))) if raw.starts_with('[') => {
            format!("{}{raw}{}", &text[..start], &text[end..])
        }
        (Some(name), Some((start, end))) => {
            let mut list = names(&text[start..end]);
            if !list.iter().any(|n| n == name) {
                list.push(name.to_string());
            }
            let array = format!(
                "[{}]",
                list.iter().map(|n| quote(n)).collect::<Vec<_>>().join(", ")
            );
            format!("{}{array}{}", &text[..start], &text[end..])
        }
        (Some(value), None) => {
            let Some(brace) = text.find('{') else {
                return format!("{{{KEY}: [{}]}}\n", quote(value));
            };
            let rest = &text[brace + 1..];
            let entry = format!("{KEY}: [{}]", quote(value));
            if rest.trim_start().starts_with('}') {
                format!("{}{entry}{rest}", &text[..=brace])
            } else {
                format!("{}\n  {entry},{rest}", &text[..=brace])
            }
        }
        (None, Some((_, end))) => {
            let key = text.find(KEY).unwrap_or(0);
            // From the end of whatever precedes the key...
            let mut start = text[..key].trim_end().len();
            let mut stop = end;
            let after = &text[stop..];
            if after.trim_start().starts_with(',') {
                // ...through its comma, if it isn't the last member,
                stop += after.len() - after.trim_start().len() + 1;
            } else if text[..start].ends_with(',') {
                // ...or back over the previous member's comma if it is.
                start -= 1;
            }
            format!("{}{}", &text[..start], &text[stop..])
        }
        (None, None) => text.to_string(),
    }
}

// ---------------------------------------------------------------- snippet files

/// Write Palette's CSS into every vault (before the setting is switched on),
/// or delete it again (after it's switched off). Never touches a user's own
/// `palette.css`.
fn prepare(paths: &Paths, remove: bool) -> Result<String, String> {
    let vaults = vaults(paths);
    if remove {
        for vault in &vaults {
            let file = snippet_file(vault);
            if is_ours(&file) {
                fs::remove_file(&file).map_err(|e| e.to_string())?;
            }
        }
        if obsidian_running(paths) {
            return Ok("snippet removed; Obsidian is open, so also switch off \
                       Settings > Appearance > CSS snippets > palette (or it may come back)"
                .into());
        }
        return Ok("snippet removed".into());
    }
    for vault in &vaults {
        let file = snippet_file(vault);
        if file.exists() && !is_ours(&file) {
            return Err(format!(
                "{} is your own snippet; rename it so Palette can use that name",
                file.display()
            ));
        }
    }
    let n = copy_into(paths, &vaults)?;
    if obsidian_running(paths) {
        // Obsidian would overwrite appearance.json with its in-memory copy.
        return Err(format!(
            "snippet written to {n} vault(s), but Obsidian is open and would undo the setting: \
             in each vault switch on Settings > Appearance > CSS snippets > palette \
             (or quit Obsidian and run this again)"
        ));
    }
    Ok(format!("snippet written to {n} vault(s) and switched on"))
}

/// Never true for a fake home: tests don't depend on the real Obsidian.
fn obsidian_running(paths: &Paths) -> bool {
    !paths.is_fake() && system::is_running("Obsidian")
}

fn copy_into(paths: &Paths, vaults: &[PathBuf]) -> Result<usize, String> {
    let css = fs::read(paths.current().join("obsidian.css")).map_err(|e| e.to_string())?;
    for vault in vaults {
        let file = snippet_file(vault);
        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        // Write a temporary file, then rename it over the snippet: Obsidian
        // reacts to the first change it sees, and an in-place write could be
        // read half-done (top variables applied, headings etc. not, until a
        // restart). A rename swaps the whole file at once. The temp name
        // doesn't end in .css, so Obsidian ignores it.
        let tmp = file.with_file_name(".palette.css.tmp");
        fs::write(&tmp, &css).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &file).map_err(|e| e.to_string())?;
    }
    Ok(vaults.len())
}

/// Vaults whose appearance.json has Palette's snippet switched on.
fn wired_vaults(paths: &Paths) -> Vec<PathBuf> {
    vaults(paths)
        .into_iter()
        .filter(|v| {
            fs::read_to_string(v.join(".obsidian/appearance.json"))
                .is_ok_and(|t| read(&t).as_deref() == Some(SNIPPET))
        })
        .collect()
}

/// Every vault with Palette's snippet file gets the new colours, including
/// ones where it isn't switched on (yet): it's ready when the user does.
fn reload(paths: &Paths) -> Outcome {
    let wired = wired_vaults(paths);
    let vaults: Vec<PathBuf> = vaults(paths)
        .into_iter()
        .filter(|v| is_ours(&snippet_file(v)) || wired.contains(v))
        .collect();
    match copy_into(paths, &vaults) {
        Ok(0) => Outcome::Skipped("no vault has the snippet on".into()),
        Ok(n) => Outcome::Done(format!("updated {n} vault(s)")),
        Err(e) => Outcome::Failed(e),
    }
}

fn doctor(paths: &Paths) -> Vec<Note> {
    let mut notes = Vec::new();
    let wired = wired_vaults(paths);
    for vault in vaults(paths) {
        let name = vault
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if is_ours(&snippet_file(&vault)) && !wired.contains(&vault) {
            notes.push(Note::fix(format!(
                "vault {name}: the palette snippet is there but switched off (Obsidian may have \
                 rewritten its settings while open): Settings > Appearance > CSS snippets > palette"
            )));
        } else if !vault.join(".obsidian/appearance.json").is_file() {
            notes.push(Note::fix(format!(
                "vault {name}: no appearance.json yet; change any setting in Settings > Appearance once, then `palette init`"
            )));
        }
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_vault_paths() {
        let json = r#"{"vaults":{"a1":{"path":"/Users/me/Notes","ts":1,"open":true},"b2":{"path":"/Users/me/Work \"x\"","ts":2}}}"#;
        assert_eq!(
            json_strings(json, "\"path\""),
            ["/Users/me/Notes", "/Users/me/Work \"x\""]
        );
    }

    #[test]
    fn adds_to_an_existing_list_and_restores_it() {
        let src = "{\n  \"cssTheme\": \"Catppuccin\",\n  \"enabledCssSnippets\": [\n    \"wide\"\n  ],\n  \"x\": 1\n}";
        let before = read(src).unwrap();
        assert!(before.starts_with('['));
        let wired = write(src, Some("palette"));
        assert_eq!(read(&wired).as_deref(), Some("palette"));
        assert!(wired.contains("[\"wide\", \"palette\"]"), "{wired}");
        assert_eq!(write(&wired, Some(&before)), src);
    }

    #[test]
    fn adds_and_deletes_a_missing_list() {
        let src = "{\n  \"cssTheme\": \"Osaka Jade\",\n  \"baseFontSize\": 18\n}";
        assert_eq!(read(src), None);
        let wired = write(src, Some("palette"));
        assert_eq!(read(&wired).as_deref(), Some("palette"));
        assert_eq!(write(&wired, None), src);

        let wired = write("{}", Some("palette"));
        assert_eq!(wired, "{\"enabledCssSnippets\": [\"palette\"]}");
        assert_eq!(write(&wired, None), "{}");
    }

    #[test]
    fn last_member_removal_keeps_json_valid() {
        let src = "{\n  \"a\": 1,\n  \"enabledCssSnippets\": [\"palette\"]\n}";
        assert_eq!(write(src, None), "{\n  \"a\": 1\n}");
    }
}
