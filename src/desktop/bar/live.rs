//! What the Bar page shows next to each item (ROADMAP 18, S5 and S6): the value the real bar
//! shows right now (`sketchybar --query <item>`), and why an item can't work on this Mac
//! ("needs macmon", "Habit Tracker isn't set up here").

use std::collections::BTreeMap;

use super::{Bar, KINDS, find_kind};
use crate::paths::Paths;
use crate::system;
use crate::targets::has_program;

/// The label each item of `bar` shows on the running bar, by item kind. Only items the bar
/// holds once (they keep the kind as their SketchyBar name), not the workspaces (many slots).
/// Empty when SketchyBar isn't running, or in a fake home (tests never ask the real bar).
pub fn values(paths: &Paths, bar: &Bar) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if paths.is_fake() || !system::is_running("sketchybar") {
        return out;
    }
    for item in bar.pills.iter().flat_map(|p| &p.items) {
        let Some(kind) = find_kind(&item.kind) else {
            continue;
        };
        if !kind.single || kind.id == "workspaces" || out.contains_key(kind.id) {
            continue;
        }
        if let Ok(json) = system::run("sketchybar", &["--query", kind.id])
            && let Some(label) = label_of(&json)
            && !label.is_empty()
        {
            out.insert(kind.id.to_string(), label);
        }
    }
    out
}

/// `label.value` from SketchyBar's `--query` JSON, without a JSON library: the first
/// `"value": "…"` after `"label"`.
pub fn label_of(json: &str) -> Option<String> {
    let rest = &json[json.find("\"label\"")?..];
    let rest = &rest[rest.find("\"value\"")? + 7..];
    let rest = rest
        .trim_start()
        .strip_prefix(':')?
        .trim_start()
        .strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push(' '),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    out.extend(u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32));
                }
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

/// Why an item can't work (fully) on this Mac, by kind; kinds that work aren't listed.
pub fn needs(paths: &Paths) -> BTreeMap<String, String> {
    let macmon = has_program("macmon");
    let support = paths.home().join("Library/Application Support");
    let mut out = BTreeMap::new();
    for k in KINDS {
        let note = match k.id {
            "temp" | "gputemp" | "pwr" if !macmon => "needs macmon (Get the tools)",
            "gpu" if !macmon => "raw reading only: Busy needs macmon",
            "workspaces" if !has_program("aerospace") => "needs AeroSpace",
            "habits"
                if !support
                    .join("local.habit.tracker/native-model.json")
                    .is_file() =>
            {
                "Habit Tracker isn't set up on this Mac"
            }
            "dispatch" if !support.join("Dispatch/status.json").is_file() => {
                "Dispatch isn't set up on this Mac"
            }
            "nowplaying" if system::palette_app_file("Contents/MacOS/palette-beats").is_none() => {
                "play/pause and beat lines need the Palette app (the title works without it)"
            }
            _ => continue,
        };
        out.insert(k.id.to_string(), note.to_string());
    }
    out
}

/// Whether an app an item reads from is on this Mac (the Your apps preset keeps only those).
pub fn have(paths: &Paths, kind: &str) -> bool {
    !matches!(kind, "habits" | "dispatch") || !needs(paths).contains_key(kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_label_comes_out_of_sketchybars_json() {
        let json = r#"{ "name": "cpu", "icon": { "value": "x" }, "label": { "value": "12 °C \"hot\"", "color": "0xff" } }"#;
        assert_eq!(label_of(json).as_deref(), Some("12 °C \"hot\""));
        assert_eq!(label_of("{}"), None);
    }
}
