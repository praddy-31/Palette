//! Font size and family per app (ROADMAP 22b, `palette font`).
//!
//! Terminals read theirs from Palette's own file (the `font` template variable, `render.rs`).
//! Editors keep fonts only in their settings.json, so for Zed and VSCodium Palette changes
//! that one key, as it does the theme key: a backup first, the rest of the file byte for
//! byte. Your own value is remembered in `fonts-yours.toml` the first time and put back when
//! the font is cleared. If you change the key yourself while Palette set it, your new value
//! becomes the one remembered.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::paths::Paths;
use crate::settings::AppFont;

/// An editor's font keys in its settings.json.
struct Editor {
    config: fn(&Paths) -> PathBuf,
    size: &'static str,
    family: &'static str,
}

fn editor(app: &str) -> Option<Editor> {
    match app {
        "zed" => Some(Editor {
            config: |p| p.home().join(".config/zed/settings.json"),
            size: "buffer_font_size",
            family: "buffer_font_family",
        }),
        "vscodium" => Some(Editor {
            config: |p| {
                p.home()
                    .join("Library/Application Support/VSCodium/User/settings.json")
            },
            size: "editor.fontSize",
            family: "editor.fontFamily",
        }),
        _ => None,
    }
}

/// Is this app's font written into its own settings (not Palette's file)?
pub fn is_editor(app: &str) -> bool {
    editor(app).is_some()
}

/// A family name Palette can write into every config format (no quotes, backslashes or
/// line breaks, which would need escaping in five different syntaxes).
pub fn check_family(family: &str) -> Result<()> {
    if family.trim().is_empty()
        || family
            .chars()
            .any(|c| matches!(c, '"' | '\\' | '\n' | '\r'))
    {
        bail!("a font family is a plain name, like JetBrainsMono Nerd Font Mono");
    }
    Ok(())
}

/// What Palette remembers per editor key: your value (JSON text, `None` = the key wasn't
/// there) and the value Palette last wrote, to notice a change you made yourself.
#[derive(Debug, Clone, PartialEq)]
struct Kept {
    yours: Option<String>,
    ours: String,
}

fn kept_file(paths: &Paths) -> PathBuf {
    paths.root().join("fonts-yours.toml")
}

/// `app.key` → what was kept.
fn load_kept(paths: &Paths) -> BTreeMap<String, Kept> {
    let Ok(text) = fs::read_to_string(kept_file(paths)) else {
        return BTreeMap::new();
    };
    let Ok(table) = text.parse::<toml::Table>() else {
        return BTreeMap::new();
    };
    table
        .iter()
        .filter_map(|(k, v)| {
            let t = v.as_table()?;
            Some((
                k.clone(),
                Kept {
                    yours: t.get("yours").and_then(|v| v.as_str()).map(String::from),
                    ours: t.get("ours")?.as_str()?.to_string(),
                },
            ))
        })
        .collect()
}

fn save_kept(paths: &Paths, kept: &BTreeMap<String, Kept>) -> Result<()> {
    let path = kept_file(paths);
    if kept.is_empty() {
        let _ = fs::remove_file(&path);
        return Ok(());
    }
    let mut text = String::from(
        "# Your own editor font settings, kept by Palette while it sets them (`palette font`).\n\
         # Put back when you clear the font in Palette. `yours` missing = you had none.\n",
    );
    for (key, k) in kept {
        text.push_str(&format!("\n[{}]\n", toml::Value::from(key.as_str())));
        if let Some(y) = &k.yours {
            text.push_str(&format!("yours = {}\n", toml::Value::from(y.as_str())));
        }
        text.push_str(&format!("ours = {}\n", toml::Value::from(k.ours.as_str())));
    }
    fs::create_dir_all(paths.root())?;
    fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
}

/// JSON text for a size: `14`, `13.5`.
fn size_json(size: f32) -> String {
    size.to_string()
}

/// Put `app`'s font into its settings.json (an editor; terminals return `Ok(None)`). Keys
/// Palette no longer sets get your value back. `Some(path)` when the file changed.
pub fn apply_editor(paths: &Paths, app: &str, font: &AppFont) -> Result<Option<PathBuf>> {
    let Some(ed) = editor(app) else {
        return Ok(None);
    };
    let path = (ed.config)(paths);
    // No settings file and no folder: the editor isn't set up here; nothing to change.
    if !path.exists() && !path.parent().is_some_and(Path::is_dir) {
        return Ok(None);
    }
    let before = fs::read_to_string(&path).unwrap_or_default();
    let mut kept = load_kept(paths);
    let mut text = before.clone();
    let wants = [
        (ed.size, font.size.map(size_json)),
        (ed.family, font.family.as_deref().map(crate::jsonc::quoted)),
    ];
    for (key, want) in wants {
        let id = format!("{app}.{key}");
        let now = crate::jsonc::read(&text, key);
        match (want, kept.get(&id).cloned()) {
            (Some(v), k) => {
                let yours = match k {
                    // Changed by you since Palette wrote it: that's yours now.
                    Some(k) if now.as_deref() != Some(k.ours.as_str()) => now.clone(),
                    Some(k) => k.yours,
                    None => now.clone(),
                };
                text = crate::jsonc::write(&text, key, Some(&v));
                kept.insert(id, Kept { yours, ours: v });
            }
            (None, Some(k)) => {
                // Palette set it before: give yours back, unless you changed it meanwhile.
                if now.as_deref() == Some(k.ours.as_str()) {
                    text = crate::jsonc::write(&text, key, k.yours.as_deref());
                }
                kept.remove(&id);
            }
            (None, None) => {}
        }
    }
    save_kept(paths, &kept)?;
    if text == before {
        return Ok(None);
    }
    crate::wiring::change_in_place(paths, &path, &text)?;
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> (tempfile::TempDir, Paths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(dir.path());
        (dir, paths)
    }

    #[test]
    fn an_editor_font_is_set_and_yours_comes_back() {
        let (_d, paths) = home();
        let file = paths.home().join(".config/zed/settings.json");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        let mine = "{\n  // mine\n  \"buffer_font_size\": 15,\n  \"theme\": \"palette\"\n}\n";
        fs::write(&file, mine).unwrap();
        let font = AppFont {
            size: Some(13.5),
            family: Some("Menlo".into()),
        };
        assert!(apply_editor(&paths, "zed", &font).unwrap().is_some());
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.contains("\"buffer_font_size\": 13.5"), "{text}");
        assert!(text.contains("\"buffer_font_family\": \"Menlo\""), "{text}");
        assert!(text.contains("// mine"), "{text}");
        // Again: nothing to change.
        assert!(apply_editor(&paths, "zed", &font).unwrap().is_none());
        // Cleared: your 15 back, the family Palette added gone, the file as it was.
        apply_editor(&paths, "zed", &AppFont::default()).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), mine);
        assert!(!kept_file(&paths).exists());
    }

    #[test]
    fn a_value_you_change_meanwhile_becomes_yours() {
        let (_d, paths) = home();
        let file = paths
            .home()
            .join("Library/Application Support/VSCodium/User/settings.json");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "{\n    \"editor.fontSize\": 12\n}\n").unwrap();
        let size = |n| AppFont {
            size: Some(n),
            family: None,
        };
        apply_editor(&paths, "vscodium", &size(14.0)).unwrap();
        // You set 16 in VSCodium; Palette sets 13 later, then is cleared: 16 comes back.
        let text = fs::read_to_string(&file).unwrap().replace("14", "16");
        fs::write(&file, text).unwrap();
        apply_editor(&paths, "vscodium", &size(13.0)).unwrap();
        apply_editor(&paths, "vscodium", &AppFont::default()).unwrap();
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.contains("\"editor.fontSize\": 16"), "{text}");
    }

    #[test]
    fn no_editor_folder_means_nothing_written() {
        let (_d, paths) = home();
        let font = AppFont {
            size: Some(14.0),
            family: None,
        };
        assert!(apply_editor(&paths, "zed", &font).unwrap().is_none());
        assert!(!paths.home().join(".config/zed").exists());
        assert!(apply_editor(&paths, "ghostty", &font).unwrap().is_none());
    }

    #[test]
    fn family_names_stay_plain() {
        assert!(check_family("JetBrainsMono Nerd Font Mono").is_ok());
        assert!(check_family("a\"b").is_err());
        assert!(check_family("  ").is_err());
    }
}
