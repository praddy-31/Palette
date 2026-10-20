//! The switch lock, favourites/recents and the contrast report, end to end
//! with the real binary in a fake home.

mod common;

use std::fs;
use std::process::Command;

use common::Home;

/// Several `palette next` at once (scrolling the bar pill) must leave one
/// consistent state: `current` points at a real generation, whose theme
/// marker is the newest generation's theme, and each `next` stepped once.
#[test]
fn concurrent_nexts_leave_a_consistent_state() {
    let home = Home::new();
    home.ok(&["set", "catppuccin-latte"]);
    // Spawned all at once (not `output()`, which would wait for each).
    let spawned: Vec<_> = (0..6)
        .map(|_| {
            Command::new(env!("CARGO_BIN_EXE_palette"))
                .arg("next")
                .env("PALETTE_HOME", home.dir.path())
                .env_remove("XDG_CONFIG_HOME")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    for child in spawned {
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let root = home.palette_dir();
    let link = fs::read_link(root.join("current")).unwrap();
    let generation = root.join(&link);
    assert!(generation.is_dir(), "current -> {}", link.display());
    let mut names: Vec<String> = fs::read_dir(root.join("generations"))
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    assert!(
        names.iter().all(|n| !n.starts_with(".tmp-")),
        "staging left behind: {names:?}"
    );
    names.sort();
    assert_eq!(names.len(), 5, "{names:?}"); // KEEP_GENERATIONS
    let newest = names.last().unwrap();
    assert_eq!(link.file_name().unwrap().to_str().unwrap(), newest);
    let theme = home.current_file("theme").trim().to_string();
    assert!(
        newest.ends_with(&format!("-{theme}")),
        "{newest} vs {theme}"
    );

    // 1 set + 6 nexts, each one step on: the 7th theme alphabetically
    // counting from catppuccin-latte (the first bundled theme).
    let list = home.ok(&["list"]);
    let all: Vec<&str> = list
        .lines()
        .map(|l| l[2..].split_whitespace().next().unwrap())
        .collect();
    assert_eq!(theme, all[6], "{all:?}");
}

#[test]
fn favourites_and_recents() {
    let home = Home::new();
    let err = home.fails(&["next", "--favourites"]);
    assert!(err.contains("no favourites yet"), "{err}");
    home.ok(&["fav", "add", "nord"]);
    home.ok(&["fav", "add", "tokyo-night"]);
    assert!(home.ok(&["fav", "add", "nord"]).contains("already"));
    assert!(home.fails(&["fav", "add", "nope"]).contains("nope"));
    let list = home.ok(&["fav"]);
    assert!(
        list.contains("★ nord") && list.contains("★ tokyo-night"),
        "{list}"
    );
    assert!(list.contains("steps through: all"), "{list}");

    home.ok(&["set", "everforest"]);
    // Not a favourite: --favourites starts at the first one.
    home.ok(&["next", "--favourites"]);
    assert_eq!(home.ok(&["current"]).trim(), "nord");
    home.ok(&["next", "--favourites"]);
    assert_eq!(home.ok(&["current"]).trim(), "tokyo-night");
    home.ok(&["next", "--favourites"]);
    assert_eq!(home.ok(&["current"]).trim(), "nord");
    // Plain next follows the setting.
    home.ok(&["fav", "cycle", "favourites"]);
    home.ok(&["prev"]);
    assert_eq!(home.ok(&["current"]).trim(), "tokyo-night");
    assert!(
        home.read(".config/palette/settings.toml")
            .contains("next_cycles = \"favourites\"")
    );
    home.ok(&["next", "--all"]);
    assert_eq!(home.ok(&["current"]).trim(), "vantablack");

    let recent = home.ok(&["recent"]);
    let names: Vec<&str> = recent.lines().map(|l| &l[2..]).collect();
    assert_eq!(names, ["vantablack", "tokyo-night", "nord", "everforest"]);
    home.ok(&["undo"]);
    assert!(home.ok(&["recent"]).starts_with("* tokyo-night"));

    home.ok(&["fav", "remove", "nord"]);
    assert!(!home.ok(&["fav", "list"]).contains("nord"));
}

#[test]
fn contrast_report() {
    let home = Home::new();
    assert!(home.fails(&["check"]).contains("no theme is active"));
    let ok = home.ok(&["check", "tokyo-night"]);
    assert!(ok.contains("all") && ok.contains("readable"), "{ok}");
    home.ok(&["set", "white"]);
    let out = home.run(&["check"]);
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("dark_foreground"), "{text}");
    assert!(text.contains("try dark_foreground = \"#"), "{text}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("1 low-contrast pair"));
}
