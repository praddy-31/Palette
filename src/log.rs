//! `~/.config/palette/palette.log`: one line per switch, undo or refresh, when the
//! `log` setting is on (off by default; `palette log on`, Settings → General). For
//! finding out what happened when something looked odd. It stays on this Mac: nothing
//! reads it but you. At 200 KB it moves to `palette.log.1` (replacing the older one),
//! so the two files never pass 400 KB together.
//!
//! A line: `2026-11-05 17:02  app  set tokyo-night  14 ok, 1 failed: kitty: not running`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::{Context, Result};

use crate::effects::Effect;
use crate::paths::Paths;
use crate::schedule;
use crate::system::Outcome;

/// Size at which the log moves to `palette.log.1`.
pub const ROTATE_AT: u64 = 200 * 1024;

static SOURCE: OnceLock<&'static str> = OnceLock::new();

/// Who is switching, for the log: the app calls this once at start ("app"); the CLI
/// doesn't need to ("cli" is the default).
pub fn set_source(source: &'static str) {
    let _ = SOURCE.set(source);
}

pub fn file(paths: &Paths) -> PathBuf {
    paths.root().join("palette.log")
}

fn rotated(paths: &Paths) -> PathBuf {
    paths.root().join("palette.log.1")
}

/// What the effects of a switch came to: `14 ok, 1 failed: kitty: not running`.
pub fn summary(effects: &[Effect]) -> String {
    let failed: Vec<String> = effects
        .iter()
        .filter_map(|(name, o)| match o {
            Outcome::Failed(why) => Some(format!("{name}: {why}")),
            _ => None,
        })
        .collect();
    let ok = effects.len() - failed.len();
    if failed.is_empty() {
        format!("{ok} ok")
    } else {
        format!("{ok} ok, {} failed: {}", failed.len(), failed.join("; "))
    }
}

/// `YYYY-MM-DD HH:MM` in the Mac's time zone.
fn stamp() -> String {
    let now = schedule::now_unix();
    let offset = schedule::system_offset;
    let local = now + i64::from(offset(now)) * 60;
    let (y, m, d) = schedule::civil_from_days(local.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02} {}", schedule::local_hm(now, &offset))
}

/// Add one line, if the log is on. Never fails the caller: a log that can't be written
/// is reported on stderr and otherwise ignored.
pub fn record(paths: &Paths, what: &str) {
    let on = crate::settings::load(paths).is_ok_and(|s| s.log);
    if !on {
        return;
    }
    if let Err(e) = append(paths, what) {
        eprintln!("palette: log: {e:#}");
    }
}

fn append(paths: &Paths, what: &str) -> Result<()> {
    let path = file(paths);
    if fs::metadata(&path).is_ok_and(|m| m.len() >= ROTATE_AT) {
        fs::rename(&path, rotated(paths)).context("rotating palette.log")?;
    }
    fs::create_dir_all(paths.root())?;
    let source = SOURCE.get().copied().unwrap_or("cli");
    // One line, whatever the message holds.
    let what = what.replace(['\n', '\r'], " ");
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("opening {}", path.display()))?;
    writeln!(f, "{}  {source}  {what}", stamp())?;
    Ok(())
}

/// The last `n` lines, oldest first (from `palette.log.1` too if the current file is short).
pub fn tail(paths: &Paths, n: usize) -> Vec<String> {
    let read = |p: PathBuf| fs::read_to_string(p).unwrap_or_default();
    let text = read(rotated(paths)) + &read(file(paths));
    let lines: Vec<String> = text.lines().map(String::from).collect();
    lines[lines.len().saturating_sub(n)..].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(paths: &Paths) {
        let mut s = crate::settings::load(paths).unwrap();
        s.log = true;
        crate::settings::save(paths, &s).unwrap();
    }

    #[test]
    fn nothing_is_written_while_off() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        record(&paths, "set nord  3 ok");
        assert!(!file(&paths).exists());
        assert!(tail(&paths, 5).is_empty());
    }

    #[test]
    fn lines_are_added_and_rotated() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        on(&paths);
        record(&paths, "set nord  3 ok");
        record(&paths, "undo ash\nsecond line");
        let lines = tail(&paths, 5);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].ends_with("  cli  set nord  3 ok"), "{lines:?}");
        assert!(lines[1].ends_with("undo ash second line"), "{lines:?}");

        // Past the limit, the file moves aside and a new one starts.
        fs::write(file(&paths), "x\n".repeat(ROTATE_AT as usize / 2)).unwrap();
        record(&paths, "set ash  1 ok");
        assert!(rotated(&paths).exists());
        let now = fs::read_to_string(file(&paths)).unwrap();
        assert_eq!(now.lines().count(), 1, "{now}");
        assert!(tail(&paths, 1)[0].ends_with("set ash  1 ok"));
    }

    #[test]
    fn summary_names_what_failed() {
        let effects: Vec<Effect> = vec![
            ("ghostty".into(), Outcome::Done("reloaded".into())),
            ("kitty".into(), Outcome::Failed("not running".into())),
            ("bat".into(), Outcome::Skipped("off".into())),
        ];
        assert_eq!(summary(&effects), "2 ok, 1 failed: kitty: not running");
        assert_eq!(summary(&effects[..1]), "1 ok");
    }
}
