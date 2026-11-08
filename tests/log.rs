//! `palette log`: off by default, one line per switch and undo when on (Phase 10a).

mod common;

use common::Home;

#[test]
fn switches_are_logged_only_when_on() {
    let home = Home::new();
    home.ok(&["set", "nord"]);
    assert!(home.ok(&["log"]).contains("Log: off"));
    assert!(home.ok(&["log", "show"]).contains("empty"));

    assert!(home.ok(&["log", "on"]).contains("Log: on"));
    home.ok(&["set", "tokyo-night"]);
    home.ok(&["undo"]);
    home.fails(&["set", "no-such-theme"]);
    let out = home.ok(&["log", "show"]);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3, "{out}");
    assert!(lines[0].contains("  cli  set tokyo-night  "), "{out}");
    assert!(lines[1].contains("  cli  undo to nord  "), "{out}");
    assert!(lines[2].contains("set no-such-theme failed"), "{out}");
    assert_eq!(home.ok(&["log", "show", "-n", "1"]).lines().count(), 1);
    // `-n` alone means show (it was silently ignored until 2026-11-08).
    let two = home.ok(&["log", "-n", "2"]);
    assert_eq!(two.lines().count(), 2, "{two}");
    assert!(two.contains("undo to nord"), "{two}");

    home.ok(&["log", "off"]);
    home.ok(&["set", "nord"]);
    assert_eq!(home.ok(&["log", "show"]).lines().count(), 3);
}
