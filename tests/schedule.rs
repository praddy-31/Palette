//! `palette schedule`: day and night themes, in a fake home at a fixed moment
//! (`PALETTE_NOW`, `PALETTE_UTC_OFFSET` replace the clock and time zone).

mod common;

use std::process::Command;

use common::Home;

/// 2026-10-15 at the given Singapore (UTC+8) local time, as unix seconds.
fn sg(h: i64, m: i64) -> i64 {
    // 2026-10-15 00:00 UTC
    let midnight = palette::schedule::days_from_civil(2026, 10, 15) * 86_400;
    midnight + (h * 60 + m) * 60 - 8 * 3600
}

fn run_at(home: &Home, now: i64, args: &[&str]) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_palette"))
        .args(args)
        .env("PALETTE_HOME", home.dir.path())
        .env_remove("XDG_CONFIG_HOME")
        .env("PALETTE_NOW", now.to_string())
        .env("PALETTE_UTC_OFFSET", "480")
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn ok_at(home: &Home, now: i64, args: &[&str]) -> String {
    let (good, out, err) = run_at(home, now, args);
    assert!(good, "{args:?} failed: {err}");
    out
}

fn current(home: &Home) -> String {
    home.ok(&["current"]).trim().to_string()
}

#[test]
fn off_by_default() {
    let home = Home::new();
    let out = home.ok(&["schedule"]);
    assert!(out.contains("Schedule: off"), "{out}");
    let out = home.ok(&["schedule", "run"]);
    assert!(out.contains("schedule is off"), "{out}");
    assert!(current(&home).starts_with("(none"));
}

#[test]
fn sun_mode_switches_by_phase_and_leaves_manual_choices_alone() {
    let home = Home::new();
    // Noon: the day theme straight away.
    let out = ok_at(
        &home,
        sg(12, 0),
        &[
            "schedule",
            "on",
            "--day",
            "nord",
            "--night",
            "tokyo-night",
            "--city",
            "singapore",
        ],
    );
    assert!(out.contains("It is day"), "{out}");
    assert!(
        out.contains("sunrise 06:4") && out.contains("sunset 18:5"),
        "{out}"
    );
    assert_eq!(current(&home), "nord");

    // Later the same day: nothing to do.
    let out = ok_at(&home, sg(15, 0), &["schedule", "run"]);
    assert!(out.contains("Nothing to do"), "{out}");

    // The user picks another theme by hand: the schedule doesn't fight it.
    home.ok(&["set", "gruvbox-dark"]);
    let out = ok_at(&home, sg(16, 0), &["schedule", "run"]);
    assert!(out.contains("Left gruvbox-dark alone"), "{out}");
    assert_eq!(current(&home), "gruvbox-dark");

    // After sunset the boundary is new: the night theme applies.
    let out = ok_at(&home, sg(20, 0), &["schedule", "run"]);
    assert!(out.contains("It is night"), "{out}");
    assert_eq!(current(&home), "tokyo-night");

    // Next morning: back to the day theme.
    ok_at(&home, sg(8, 0) + 86_400, &["schedule", "run"]);
    assert_eq!(current(&home), "nord");
}

#[test]
fn fixed_times_and_off() {
    let home = Home::new();
    let out = ok_at(
        &home,
        sg(22, 0),
        &[
            "schedule",
            "on",
            "--day",
            "nord",
            "--night",
            "tokyo-night",
            "--at",
            "06:30,21:15",
        ],
    );
    assert!(out.contains("day 06:30, night 21:15"), "{out}");
    assert!(out.contains("06:30 tomorrow -> nord"), "{out}");
    assert_eq!(current(&home), "tokyo-night");
    let out = ok_at(&home, sg(7, 0) + 86_400, &["schedule", "run"]);
    assert!(out.contains("It is day"), "{out}");
    assert_eq!(current(&home), "nord");

    let out = home.ok(&["schedule", "off"]);
    assert!(out.contains("Schedule off"), "{out}");
    let settings = std::fs::read_to_string(home.palette_dir().join("settings.toml")).unwrap();
    assert!(settings.contains("[schedule]") && settings.contains("on = false"));
    assert!(settings.contains("day = \"nord\""), "choices are kept");
    // Turning it on again with no options reuses them.
    ok_at(&home, sg(23, 0) + 86_400, &["schedule", "on"]);
    assert_eq!(current(&home), "tokyo-night");
}

#[test]
fn typed_coordinates_and_errors() {
    let home = Home::new();
    let out = ok_at(
        &home,
        sg(12, 0),
        &[
            "schedule",
            "on",
            "--day",
            "nord",
            "--night",
            "gruvbox-dark",
            "--lat",
            "51.5",
            "--lon",
            "-0.13",
        ],
    );
    assert!(out.contains("51.50, -0.13"), "{out}");

    let err = run_at(&home, sg(12, 0), &["schedule", "on", "--city", "Atlantis"]).2;
    assert!(err.contains("no city called `Atlantis`"), "{err}");
    let err = run_at(&home, sg(12, 0), &["schedule", "on", "--lat", "10"]).2;
    assert!(err.contains("both --lat and --lon"), "{err}");
    let err = run_at(&home, sg(12, 0), &["schedule", "on", "--at", "7:00,7:00"]).2;
    assert!(err.contains("same"), "{err}");
    let err = run_at(
        &home,
        sg(12, 0),
        &["schedule", "on", "--night", "no-such-theme"],
    )
    .2;
    assert!(err.contains("isn't installed"), "{err}");
}

#[test]
fn cities_list_and_search() {
    let home = Home::new();
    let all = home.ok(&["schedule", "cities"]);
    for city in [
        "Singapore",
        "Kuala Lumpur",
        "London",
        "San Francisco",
        "Bangalore",
    ] {
        assert!(all.contains(city), "{city}");
    }
    let hits = home.ok(&["schedule", "cities", "york"]);
    assert!(
        hits.contains("New York") && !hits.contains("Tokyo"),
        "{hits}"
    );
}
