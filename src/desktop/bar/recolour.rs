//! A theme switch without `sketchybar --reload` (ROADMAP 15a).
//!
//! `--reload` empties the bar and runs `sketchybarrc` again: every item is removed and
//! added back and every script restarts, so for about a second the pills are blank. For a
//! bar Palette manages, a switch only needs the colours. [`script`] turns the bar file on
//! disk (and Palette's item file) into a colour-only batch: each `--bar` / `--set` keeps
//! just its `…color=` settings, `--add` / `--subscribe` / `--move` / `--remove` go, and the
//! helpers (`pill()`) and loops (`for slot …`) stay, so `"$1"` and `$slot` still work. Then
//! `sketchybar --update` makes every item's script run once; they re-read Palette's colours.
//!
//! Made from the file at switch time, never stored, so it can't drift from the bar that's
//! running. A bar that's yours keeps `--reload`: Palette can't know its items.

/// The source line of Palette's colours, kept; the item file's source line is replaced by
/// its filtered text.
const COLOURS_FILE: &str = "current/sketchybar.sh\"";
const ITEM_FILE: &str = "current/sketchybar-item.sh\"";

/// The colour-only batch for `rc` (a Palette-managed sketchybarrc) and `item` (Palette's
/// theme item file, if it's wired).
pub fn script(rc: &str, item: Option<&str>) -> String {
    let mut out = String::from(
        "#!/bin/bash\n# Palette: the bar's colours only (a theme switch without --reload).\n",
    );
    for line in logical_lines(rc) {
        let t = line.trim_start();
        if t.starts_with('#') || t.is_empty() || t == "sketchybar --update" {
            continue;
        }
        if t.starts_with("[ -f ") {
            if t.contains(ITEM_FILE) {
                if let Some(item) = item {
                    out.push_str(&filter_file(item, true));
                }
            } else if t.contains(COLOURS_FILE) {
                out.push_str(&line);
                out.push('\n');
            }
            continue;
        }
        out.push_str(&filter_line(&line, false));
    }
    out.push_str("sketchybar --update\n");
    out
}

/// Every command of a file, filtered (the item file: its label too, for the theme's name).
fn filter_file(text: &str, item: bool) -> String {
    logical_lines(text)
        .into_iter()
        .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
        .map(|l| filter_line(&l, item))
        .collect()
}

/// One line of shell: a `sketchybar` command (alone or as a helper's body) filtered, anything
/// else (variables, `for … do`, `done`, calls of the helpers) as it is.
fn filter_line(line: &str, item: bool) -> String {
    let Some(at) = line.find("sketchybar --") else {
        return format!("{line}\n");
    };
    let (head, rest) = line.split_at(at);
    // A helper's body ends with `; }`.
    let (cmd, tail) = match rest.rfind("; }") {
        Some(end) => (&rest[..end], &rest[end..]),
        None => (rest, ""),
    };
    let kept = filter_command(cmd, item);
    let body = if kept.is_empty() {
        ":".to_string()
    } else {
        format!("sketchybar{kept} 2>/dev/null")
    };
    format!("{head}{body}{tail}\n")
}

/// The `--bar` / `--default` / `--set` parts of one `sketchybar …` command, colours only.
/// `--default` also becomes a `--set '/.*/'`: a default only reaches items added after it,
/// and the items already there got the old one.
fn filter_command(cmd: &str, item: bool) -> String {
    let words = split_words(cmd);
    let mut out = String::new();
    let mut i = 1; // past `sketchybar`
    while i < words.len() {
        let w = words[i].as_str();
        let (target, start) = match w {
            "--bar" => (Some("--bar".to_string()), i + 1),
            "--default" => (Some("--default".to_string()), i + 1),
            "--set" if i + 1 < words.len() => (Some(format!("--set {}", words[i + 1])), i + 2),
            _ => (None, i + 1),
        };
        let mut j = start;
        let mut props = Vec::new();
        while j < words.len() && !words[j].starts_with("--") {
            if target.is_some() && wanted(&words[j], item, w == "--set" && is_palette(&words, i)) {
                props.push(words[j].clone());
            }
            j += 1;
        }
        if let Some(target) = target
            && !props.is_empty()
        {
            let props = props.join(" ");
            if target == "--default" {
                out.push_str(&format!(" --default {props} --set '/.*/' {props}"));
            } else {
                out.push_str(&format!(" {target} {props}"));
            }
        }
        i = j;
    }
    out
}

/// `--set palette …` in the item file: its label is the theme's name.
fn is_palette(words: &[String], i: usize) -> bool {
    words.get(i + 1).is_some_and(|n| n == "palette")
}

/// A setting to keep: any colour (`color=`, `icon.color=`, `background.border_color=`); in the
/// item file also the theme pill's label.
fn wanted(word: &str, item: bool, palette: bool) -> bool {
    let Some((key, _)) = word.split_once('=') else {
        return false;
    };
    key.ends_with("color") || (item && palette && key == "label")
}

/// Join lines that end with `\` (one command written over several lines).
fn logical_lines(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for line in text.lines() {
        if let Some(part) = line.strip_suffix('\\') {
            cur.push_str(part);
            cur.push(' ');
        } else {
            cur.push_str(line);
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Split a command into shell words, keeping each word's text as written (quotes too), so
/// the words can be put back together unchanged.
fn split_words(cmd: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut chars = cmd.chars().peekable();
    let (mut single, mut double, mut brace) = (false, false, 0u32);
    while let Some(c) = chars.next() {
        match c {
            '\\' if !single => {
                cur.push(c);
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
                continue;
            }
            '\'' if !double => single = !single,
            '"' if !single => double = !double,
            '{' if !single && cur.ends_with('$') => brace += 1,
            '}' if !single && brace > 0 => brace -= 1,
            c if c.is_whitespace() && !single && !double && brace == 0 => {
                if !cur.is_empty() {
                    words.push(std::mem::take(&mut cur));
                }
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_keep_quotes_and_expansions() {
        let w = split_words(
            r#"sketchybar --set clock script='sketchybar --set $NAME label="$(date "+%a %d")"' background.color=${C_x/#0xff/0x80}"#,
        );
        assert_eq!(w.len(), 5, "{w:?}");
        assert_eq!(w[4], "background.color=${C_x/#0xff/0x80}");
    }

    #[test]
    fn commands_keep_only_colours() {
        let rc = "sketchybar --add item clock right --set clock icon='' icon.color=$C_accent update_freq=30 \\\n  script='sketchybar --set $NAME label.color=$X' click_script='open -a Calendar' \\\n  --subscribe clock system_woke\n";
        let s = script(rc, None);
        assert!(
            s.contains("sketchybar --set clock icon.color=$C_accent 2>/dev/null\n"),
            "{s}"
        );
        assert!(!s.contains("--add") && !s.contains("--subscribe") && !s.contains("script="));
    }

    #[test]
    fn helpers_loops_and_defaults_stay_working_shell() {
        let rc = "pill() { sketchybar --add bracket \"$1\" \"${@:2}\" --set \"$1\" background.color=${C_d/#0xff/0x80} \\\n  background.height=32 background.border_color=$C_accent; }\n\
gap() { sketchybar --add item \"$1\" \"$2\" --set \"$1\" width=6; }\n\
for slot in 1 2; do\n  sketchybar --add item space.$slot left --set space.$slot drawing=off icon.color=$GREY\ndone\n\
sketchybar --default icon.font=\"$FONT:Bold:13\" icon.color=$C_fg label.color=$C_fg\n\
pill pill.left.0 cpu\n";
        let s = script(rc, None);
        assert!(s.contains("pill() { sketchybar --set \"$1\" background.color=${C_d/#0xff/0x80} background.border_color=$C_accent 2>/dev/null; }"), "{s}");
        assert!(s.contains("gap() { :; }"), "{s}");
        assert!(s.contains("for slot in 1 2; do\n  sketchybar --set space.$slot icon.color=$GREY 2>/dev/null\ndone"), "{s}");
        assert!(s.contains("--default icon.color=$C_fg label.color=$C_fg --set '/.*/' icon.color=$C_fg label.color=$C_fg"), "{s}");
        assert!(s.contains("pill pill.left.0 cpu\n"));
        assert!(s.ends_with("sketchybar --update\n"));
    }

    #[test]
    fn item_file_replaces_its_source_line_and_keeps_the_name() {
        let rc = "[ -f \"$HOME/.config/palette/current/sketchybar.sh\" ] && source \"$HOME/.config/palette/current/sketchybar.sh\"\n\
[ -f \"$HOME/.config/palette/current/sketchybar-item.sh\" ] && source \"$HOME/.config/palette/current/sketchybar-item.sh\"\n";
        let item = "# comment\nsketchybar --add item palette right \\\n  --set palette icon=\"$I\" icon.color=$ACCENT label=\"nord\" click_script=\"open x\" \\\n  --add bracket palette_pill palette \\\n  --set palette_pill background.color=$PILL background.height=30\n";
        let s = script(rc, Some(item));
        assert!(s.contains("current/sketchybar.sh\"\n"), "{s}");
        assert!(!s.contains("sketchybar-item.sh"), "{s}");
        assert!(s.contains("--set palette icon.color=$ACCENT label=\"nord\" --set palette_pill background.color=$PILL"), "{s}");
    }

    /// Every colour a generated bar sets is in its batch (so the two can't drift), and the
    /// batch runs: each `--set` reaches SketchyBar with the colours filled in.
    #[test]
    #[cfg(target_os = "macos")] // the Mac's targets / CoreText fonts
    fn a_rendered_bar_keeps_every_colour_and_runs() {
        use crate::desktop::bar::{Bar, render};
        let h = tempfile::tempdir().unwrap();
        let paths = crate::paths::Paths::fake_home(h.path());
        let rc = render(&paths, &Bar::default()).unwrap().remove(0).text;
        // Both Palette files as a switch writes them (with the bar scroll, the longer item file).
        let theme = crate::theme::load(&paths, "tokyo-night").unwrap();
        let files = crate::render::render(&theme, &paths).unwrap();
        let text = |name: &str| {
            let f = files.iter().find(|f| f.name == name).unwrap();
            String::from_utf8(f.contents.clone()).unwrap()
        };
        let item = text("sketchybar-item.sh");
        let s = script(&rc, Some(&item));
        assert!(s.contains("label=\"tokyo-night\""), "{s}");
        for line in logical_lines(&rc) {
            if !line.contains("sketchybar --") {
                continue;
            }
            for w in split_words(&line) {
                let w = w.trim_end_matches(';'); // a helper's `…; }`
                if w.split_once('=').is_some_and(|(k, _)| k.ends_with("color")) {
                    assert!(s.contains(w), "{w} is missing from:\n{s}");
                }
            }
        }
        // Run it: a fake `sketchybar` that writes down its arguments, and a colours file.
        let bin = h.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let log = h.path().join("log");
        let fake = bin.join("sketchybar");
        std::fs::write(
            &fake,
            format!("#!/bin/bash\necho \"$*\" >> '{}'\n", log.display()),
        )
        .unwrap();
        std::process::Command::new("chmod")
            .arg("+x")
            .arg(&fake)
            .status()
            .unwrap();
        let current = h.path().join(".config/palette/current");
        std::fs::create_dir_all(&current).unwrap();
        std::fs::write(current.join("sketchybar.sh"), text("sketchybar.sh")).unwrap();
        let out = std::process::Command::new("bash")
            .arg("-c")
            .arg(&s)
            .env("HOME", h.path())
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let calls = std::fs::read_to_string(&log).unwrap();
        assert!(calls.contains("--set space.1 icon.color=0x"), "{calls}");
        assert!(calls.contains("--set palette icon.color=0x"), "{calls}");
        assert!(
            !calls.contains("--add") && !calls.contains("$C_"),
            "{calls}"
        );
        assert!(calls.trim_end().ends_with("--update"), "{calls}");
    }
}
