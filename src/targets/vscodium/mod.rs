//! VSCodium has no include. Palette builds a tiny local theme extension
//! ("Palette"), installs it with VSCodium's own `--install-extension`, and
//! `init` sets `workbench.colorTheme` to it. On every switch the extension's
//! colour file is rewritten; `_watch` makes VSCodium re-read it live.

use std::fs;
use std::path::{Path, PathBuf};

use super::{BASE, Output, Target};
use crate::paths::Paths;
use crate::system::{self, Outcome};
use crate::wiring::{Setting, Wiring};

pub const TARGET: Target = Target {
    name: "vscodium",
    output: Some(Output {
        file: "vscodium.json",
        template: include_str!("template.json"),
        overridable: true,
        finish: None,
    }),
    wiring: Wiring::Setting(Setting {
        configs,
        per_file: false,
        value: |_| THEME_LABEL.into(),
        read,
        write,
        prepare: Some(prepare),
    }),
    installed: |paths| super::has_app(paths, "VSCodium.app") || super::has_program("codium"),
    reload,
    unload: Some(|_| Outcome::Done("switches back by itself".into())),
    doctor,
    ..BASE
};

const THEME_LABEL: &str = "Palette";
const EXTENSION_ID: &str = "palette.palette-theme";
const KEY: &str = "\"workbench.colorTheme\"";

fn configs(paths: &Paths) -> Vec<PathBuf> {
    vec![
        paths
            .home()
            .join("Library/Application Support/VSCodium/User/settings.json"),
    ]
}

fn extensions_dir(paths: &Paths) -> PathBuf {
    paths.home().join(".vscode-oss/extensions")
}

fn codium(paths: &Paths) -> Option<PathBuf> {
    system::find_program("codium").or_else(|| {
        [
            PathBuf::from("/Applications"),
            paths.home().join("Applications"),
        ]
        .into_iter()
        .map(|d| d.join("VSCodium.app/Contents/Resources/app/bin/codium"))
        .find(|p| p.is_file())
    })
}

// ---------------------------------------------------------------- settings.json

/// Byte range of the `"workbench.colorTheme"` value's string literal, quotes
/// included. settings.json is JSON with comments; VS Code keeps its settings
/// flat, so a plain search for the key is enough.
fn value_span(text: &str) -> Option<(usize, usize)> {
    let key = text.find(KEY)?;
    let after = &text[key + KEY.len()..];
    let colon = after.find(':')?;
    if !after[..colon].trim().is_empty() {
        return None;
    }
    let rest = &after[colon + 1..];
    let start = key + KEY.len() + colon + 1 + (rest.len() - rest.trim_start().len());
    if !text[start..].starts_with('"') {
        return None;
    }
    let mut escaped = false;
    for (i, c) in text[start + 1..].char_indices() {
        match c {
            '\\' if !escaped => escaped = true,
            '"' if !escaped => return Some((start, start + 1 + i + 1)),
            _ => escaped = false,
        }
    }
    None
}

fn read(text: &str) -> Option<String> {
    let (start, end) = value_span(text)?;
    let literal = &text[start + 1..end - 1];
    Some(literal.replace("\\\"", "\"").replace("\\\\", "\\"))
}

fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Set the value in place, add the setting at the top, or (`None`) delete
/// the line Palette added.
fn write(text: &str, value: Option<&str>) -> String {
    match (value, value_span(text)) {
        (Some(v), Some((start, end))) => format!("{}{}{}", &text[..start], quote(v), &text[end..]),
        (Some(v), None) => match text.find('{') {
            Some(brace) => {
                let rest = &text[brace + 1..];
                let comma = if rest.trim_start().starts_with('}') {
                    ""
                } else {
                    ","
                };
                format!("{}\n    {KEY}: {}{comma}{rest}", &text[..=brace], quote(v))
            }
            None => format!("{{\n    {KEY}: {}\n}}\n", quote(v)),
        },
        (None, Some((start, end))) => {
            let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
            let line_end = text[start..].find('\n').map_or(end, |i| start + i + 1);
            format!("{}{}", &text[..line_start], &text[line_end..])
        }
        (None, None) => text.to_string(),
    }
}

// ---------------------------------------------------------------- extension

const PACKAGE_JSON: &str = r#"{
  "name": "palette-theme",
  "displayName": "Palette",
  "description": "Colours from Palette, generated on this Mac",
  "publisher": "palette",
  "version": "1.0.0",
  "license": "MIT",
  "engines": { "vscode": "^1.60.0" },
  "categories": ["Themes"],
  "contributes": {
    "themes": [
      { "label": "Palette", "uiTheme": "vs-dark", "path": "./themes/palette.json", "_watch": true }
    ]
  }
}
"#;

const VSIX_MANIFEST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<PackageManifest Version="2.0.0" xmlns="http://schemas.microsoft.com/developer/vsx-schema/2011" xmlns:d="http://schemas.microsoft.com/developer/vsx-schema-design/2011">
  <Metadata>
    <Identity Language="en-US" Id="palette-theme" Version="1.0.0" Publisher="palette" />
    <DisplayName>Palette</DisplayName>
    <Description xml:space="preserve">Colours from Palette, generated on this Mac</Description>
    <Categories>Themes</Categories>
  </Metadata>
  <Installation>
    <InstallationTarget Id="Microsoft.VisualStudio.Code" />
  </Installation>
  <Dependencies />
  <Assets>
    <Asset Type="Microsoft.VisualStudio.Code.Manifest" Path="extension/package.json" Addressable="true" />
  </Assets>
</PackageManifest>
"#;

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension=".json" ContentType="application/json" /><Default Extension=".vsixmanifest" ContentType="text/xml" /></Types>
"#;

/// Build `~/.config/palette/vscodium/palette-theme.vsix` (a zip) from the
/// active theme. Uses macOS's own `zip`; no archive crate needed.
pub fn build_vsix(paths: &Paths) -> Result<PathBuf, String> {
    let theme = fs::read(paths.current().join("vscodium.json"))
        .map_err(|e| format!("set a theme first ({e})"))?;
    let dir = paths.root().join("vscodium");
    let build = dir.join("build");
    let vsix = dir.join("palette-theme.vsix");
    let _ = fs::remove_dir_all(&build);
    let _ = fs::remove_file(&vsix);
    let files: [(&str, &[u8]); 4] = [
        ("extension/package.json", PACKAGE_JSON.as_bytes()),
        ("extension/themes/palette.json", &theme),
        ("extension.vsixmanifest", VSIX_MANIFEST.as_bytes()),
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
    ];
    for (name, contents) in files {
        let path = build.join(name);
        fs::create_dir_all(path.parent().unwrap_or(&build)).map_err(|e| e.to_string())?;
        fs::write(&path, contents).map_err(|e| e.to_string())?;
    }
    let vsix_arg = vsix.to_string_lossy();
    system::run_in(Some(&build), "zip", &["-q", "-r", "-X", &vsix_arg, "."])?;
    let _ = fs::remove_dir_all(&build);
    Ok(vsix)
}

/// Before `workbench.colorTheme` is set: install the extension. After it is
/// restored (`remove`): uninstall it.
fn prepare(paths: &Paths, remove: bool) -> Result<String, String> {
    if remove {
        if paths.is_fake() {
            return Ok("extension left alone: fake home".into());
        }
        let codium = codium(paths).ok_or("codium not found")?;
        system::run(
            &codium.to_string_lossy(),
            &["--uninstall-extension", EXTENSION_ID],
        )?;
        return Ok("Palette theme extension removed".into());
    }
    let vsix = build_vsix(paths)?;
    if paths.is_fake() {
        return Ok("extension built, not installed: fake home".into());
    }
    let codium =
        codium(paths).ok_or("codium not found, so the theme extension can't be installed")?;
    system::run(
        &codium.to_string_lossy(),
        &["--install-extension", &vsix.to_string_lossy(), "--force"],
    )?;
    Ok("Palette theme extension installed; if VSCodium is open, run \"Developer: Reload Window\" once".into())
}

/// The installed extension's folder (`palette.palette-theme-1.0.0`).
fn installed_extension(paths: &Paths) -> Option<PathBuf> {
    fs::read_dir(extensions_dir(paths))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.is_dir()
                && p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with(&format!("{EXTENSION_ID}-")))
        })
}

/// Copy the new colours into the installed extension. Written in place (not
/// renamed over) so VSCodium's watcher sees a change to the same file.
fn reload(paths: &Paths) -> Outcome {
    let Some(ext) = installed_extension(paths) else {
        return Outcome::Failed("Palette theme extension not installed: run `palette init`".into());
    };
    match copy_theme(paths, &ext) {
        Ok(()) => Outcome::Done("updated".into()),
        Err(e) => Outcome::Failed(e),
    }
}

fn copy_theme(paths: &Paths, ext: &Path) -> Result<(), String> {
    let theme = fs::read(paths.current().join("vscodium.json")).map_err(|e| e.to_string())?;
    fs::write(ext.join("themes/palette.json"), theme).map_err(|e| e.to_string())
}

fn doctor(paths: &Paths) -> Vec<super::Note> {
    if crate::wiring::is_wired(paths, &TARGET) && installed_extension(paths).is_none() {
        return vec![super::Note::fix(
            "theme is set to Palette but the extension is missing: run `palette init --remove` then `palette init`",
        )];
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETTINGS: &str = "{\n    // my settings\n    \"editor.fontSize\": 14,\n    \"workbench.colorTheme\": \"Tokyo Night\",\n    \"x\": true\n}\n";

    #[test]
    fn reads_and_replaces_the_value() {
        assert_eq!(read(SETTINGS).as_deref(), Some("Tokyo Night"));
        let wired = write(SETTINGS, Some("Palette"));
        assert!(wired.contains("\"workbench.colorTheme\": \"Palette\",\n"));
        assert_eq!(write(&wired, Some("Tokyo Night")), SETTINGS);
    }

    #[test]
    fn adds_and_deletes_a_missing_setting() {
        let src = "{\n    \"editor.fontSize\": 14\n}\n";
        let wired = write(src, Some("Palette"));
        assert_eq!(read(&wired).as_deref(), Some("Palette"));
        assert_eq!(write(&wired, None), src);

        let empty = write("{}", Some("Palette"));
        assert_eq!(empty, "{\n    \"workbench.colorTheme\": \"Palette\"}");
        assert_eq!(read(&empty).as_deref(), Some("Palette"));
    }

    #[test]
    fn escapes() {
        let text = write("{}", Some("a \"b\" \\c"));
        assert_eq!(read(&text).as_deref(), Some("a \"b\" \\c"));
        assert_eq!(read("{\"workbench.colorTheme\" : 7}"), None);
    }

    #[test]
    fn theme_template_is_valid_json_shape() {
        let t = include_str!("template.json");
        assert_eq!(t.matches('{').count(), t.matches('}').count());
        assert!(PACKAGE_JSON.contains("\"_watch\": true"));
    }
}
