//! Window borders (JankyBorders): `~/.config/borders/bordersrc`, written
//! whole from `desktop/borders.toml`. Colours are theme colour names, read
//! from Palette's `borders.sh` at run time, so a theme switch recolours them.

use std::fs;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use super::{File, Tool};
use crate::paths::Paths;
use crate::system::{self, Outcome};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Borders {
    /// `round` or `square`.
    pub style: String,
    /// Points, 1 to 20.
    pub width: f64,
    /// Sharper borders on Retina screens.
    pub hidpi: bool,
    /// `above` or `below` the window.
    pub order: String,
    /// Blur behind the border (0 = none).
    pub blur_radius: f64,
    /// Theme colour of the focused window's border.
    pub active: String,
    /// A second theme colour for a gradient (empty = a plain colour).
    pub active_to: String,
    /// Theme colour of the other windows' borders.
    pub inactive: String,
    /// How opaque the other borders are, 0 to 100 (0 hides them).
    pub inactive_opacity: u8,
    /// Apps that get no border.
    pub ignore: Vec<String>,
}

impl Default for Borders {
    fn default() -> Self {
        Self {
            style: "round".into(),
            width: 6.0,
            hidpi: true,
            order: "above".into(),
            blur_radius: 0.0,
            active: "accent".into(),
            active_to: String::new(),
            inactive: "muted".into(),
            inactive_opacity: 100,
            ignore: Vec::new(),
        }
    }
}

impl Borders {
    pub fn validate(&self) -> Result<()> {
        if !["round", "square"].contains(&self.style.as_str()) {
            bail!("style is round or square, not `{}`", self.style);
        }
        if !["above", "below"].contains(&self.order.as_str()) {
            bail!("order is above or below, not `{}`", self.order);
        }
        if !(1.0..=20.0).contains(&self.width) {
            bail!("width is 1 to 20, not {}", self.width);
        }
        if !(0.0..=100.0).contains(&self.blur_radius) {
            bail!("blur_radius is 0 to 100, not {}", self.blur_radius);
        }
        if self.inactive_opacity > 100 {
            bail!("inactive_opacity is 0 to 100");
        }
        for key in [&self.active, &self.inactive] {
            colour_name(key)?;
        }
        if !self.active_to.is_empty() {
            colour_name(&self.active_to)?;
        }
        for app in &self.ignore {
            if app.trim().is_empty() || app.contains(['"', ',', '\\', '$', '`', '\n']) {
                bail!("`{app}` can't be an app name here (no quotes, commas or $)");
            }
        }
        Ok(())
    }
}

fn colour_name(key: &str) -> Result<&'static str> {
    crate::theme::colour_key(key)
        .ok_or_else(|| anyhow::anyhow!("`{key}` is not a theme colour (accent, blue, muted, …)"))
}

/// Start from your bordersrc: style, width, hidpi, order, blur and the
/// ignored apps. Colours come from the theme.
pub fn import(paths: &Paths) -> (Borders, Vec<String>) {
    let mut b = Borders::default();
    let mut warnings = Vec::new();
    let Some(file) = Tool::Borders.files(paths).pop() else {
        return (b, warnings);
    };
    let Ok(text) = fs::read_to_string(&file) else {
        return (b, warnings);
    };
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        for word in line.split_whitespace() {
            let Some((key, value)) = word.split_once('=') else {
                continue;
            };
            let value = value.trim_matches(['"', '\'']);
            match key {
                "style" if ["round", "square"].contains(&value) => b.style = value.into(),
                "width" => b.width = value.parse().unwrap_or(b.width),
                "hidpi" => b.hidpi = value == "on",
                "order" if ["above", "below"].contains(&value) => b.order = value.into(),
                "blur_radius" => b.blur_radius = value.parse().unwrap_or(0.0),
                "blacklist" => {
                    b.ignore = value
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                "whitelist" => warnings
                    .push("your bordersrc only borders some apps (whitelist); Palette borders all apps except the ones you ignore".into()),
                _ => {}
            }
        }
    }
    b.width = b.width.clamp(1.0, 20.0);
    (b, warnings)
}

pub fn render(paths: &Paths, b: &Borders) -> Result<Vec<File>> {
    let mut options = vec![
        format!("style={}", b.style),
        format!("width={:.1}", b.width),
        format!("hidpi={}", if b.hidpi { "on" } else { "off" }),
        format!("order={}", b.order),
    ];
    if b.blur_radius > 0.0 {
        options.push(format!("blur_radius={:.1}", b.blur_radius));
    }
    // Always, even empty: the running borders keeps its old list otherwise,
    // so an app taken off the list never got its border back.
    // borders matches the program's name, which isn't always the app's
    // (Ghostty's is `ghostty`): list both.
    let mut names: Vec<String> = Vec::new();
    for app in &b.ignore {
        let program = system::app_executable(app);
        for n in std::iter::once(app.clone()).chain(program) {
            if !names.contains(&n) && !n.contains(['"', ',', '\\', '$', '`', '\n']) {
                names.push(n);
            }
        }
    }
    options.push(format!("blacklist=\"{}\"", names.join(",")));
    let active = if b.active_to.is_empty() {
        format!("$C_{}", b.active)
    } else {
        format!(
            "gradient(top_left=$C_{},bottom_right=$C_{})",
            b.active, b.active_to
        )
    };
    // 0xffRRGGBB with the alpha byte swapped in.
    let alpha = ((u32::from(b.inactive_opacity) * 255 + 50) / 100) as u8;
    let inactive = format!("${{C_{}/#0xff/0x{alpha:02x}}}", b.inactive);
    let block = crate::wiring::palette_block(paths, "borders").unwrap_or_default();
    let text = format!(
        "#!/bin/bash
# JankyBorders, written by Palette from ~/.config/palette/desktop/borders.toml.
# Change it in Palette (Settings → Borders) or with `palette desktop borders`;
# edits made here are replaced on the next change. Your own file comes back
# with `palette desktop borders release`.

options=(
{}
)

{block}
borders \"${{options[@]}}\" active_color=\"{active}\" inactive_color=\"{inactive}\"
",
        options
            .iter()
            .map(|o| format!("  {o}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    Ok(vec![File {
        path: Tool::Borders.files(paths).remove(0),
        text,
        executable: true,
    }])
}

/// Run the bordersrc: `borders` with options updates the running borders.
pub fn reload(paths: &Paths) -> Outcome {
    if !system::is_running("borders") {
        return Outcome::Skipped("not running".into());
    }
    let rc = Tool::Borders.files(paths).remove(0);
    match system::run("bash", &[&rc.to_string_lossy()]) {
        Ok(_) => Outcome::Done("updated".into()),
        Err(e) => Outcome::Failed(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: &str = "#!/bin/bash\n\noptions=(\n    style=round\n    width=6.0\n    hidpi=on\n#    active_color=0xffcba6f7\n    active_color=0xff7aa2f7\n    inactive_color=0xff414868\n)\n\nborders \"${options[@]}\"\n";

    fn home(rc: &str) -> (tempfile::TempDir, Paths) {
        let h = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(h.path());
        let file = paths.config_home().join("borders/bordersrc");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, rc).unwrap();
        (h, paths)
    }

    #[test]
    fn imports_the_owners_bordersrc() {
        let (_h, paths) =
            home(&OWNER.replace("hidpi=on", "hidpi=off blacklist=\"Finder,Raycast\""));
        let (b, warnings) = import(&paths);
        assert_eq!(b.style, "round");
        assert_eq!(b.width, 6.0);
        assert!(!b.hidpi);
        assert_eq!(b.ignore, ["Finder", "Raycast"]);
        assert!(warnings.is_empty());
    }

    #[test]
    fn renders_options_colours_and_the_palette_line() {
        let (_h, paths) = home(OWNER);
        let b = Borders {
            active_to: "magenta".into(),
            inactive_opacity: 50,
            blur_radius: 8.0,
            ignore: vec!["Finder".into()],
            ..Borders::default()
        };
        b.validate().unwrap();
        let text = &render(&paths, &b).unwrap()[0].text;
        assert!(text.contains("  width=6.0\n"), "{text}");
        assert!(text.contains("blur_radius=8.0"));
        assert!(text.contains("blacklist=\"Finder\""));
        assert!(text.contains("gradient(top_left=$C_accent,bottom_right=$C_magenta)"));
        assert!(text.contains("${C_muted/#0xff/0x80}"));
        assert!(crate::wiring::has_block(
            text,
            crate::wiring::palette_block(&paths, "borders")
                .unwrap()
                .lines()
                .nth(1)
                .unwrap()
        ));
    }

    #[test]
    fn an_empty_ignore_list_is_still_sent() {
        // Without it the running borders keeps the old list, and an app taken
        // off it never gets its border back.
        let (_h, paths) = home(OWNER);
        let text = &render(&paths, &Borders::default()).unwrap()[0].text;
        assert!(text.contains("  blacklist=\"\"\n"), "{text}");
    }

    #[test]
    fn refuses_bad_values() {
        let bad = [
            Borders {
                style: "wavy".into(),
                ..Default::default()
            },
            Borders {
                width: 50.0,
                ..Default::default()
            },
            Borders {
                active: "sparkle".into(),
                ..Default::default()
            },
            Borders {
                ignore: vec!["a\"; rm".into()],
                ..Default::default()
            },
        ];
        for b in bad {
            assert!(b.validate().is_err(), "{b:?}");
        }
    }
}
