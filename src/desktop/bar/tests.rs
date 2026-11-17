use super::*;

const OWNER: &str = include_str!("../../../tests/fixtures/sketchybarrc-owner");

fn home(rc: Option<&str>) -> (tempfile::TempDir, Paths) {
    let h = tempfile::tempdir().unwrap();
    let paths = Paths::fake_home(h.path());
    if let Some(rc) = rc {
        let dir = paths.config_home().join("sketchybar/plugins");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.parent().unwrap().join("sketchybarrc"), rc).unwrap();
        fs::write(
            dir.join("widgets.sh"),
            "LOCATION=\"Springfield\"   # a city\nUNITS=\"F\"\n",
        )
        .unwrap();
    }
    (h, paths)
}

#[test]
fn the_default_is_valid() {
    Bar::default().validate().unwrap();
}

#[test]
fn imports_the_owners_bar_as_the_default() {
    let (_h, paths) = home(Some(OWNER));
    let (bar, warnings) = import(&paths);
    assert!(warnings.is_empty(), "{warnings:?}");
    let kinds = |b: &Bar| -> Vec<(String, Vec<String>)> {
        b.pills
            .iter()
            .map(|p| {
                (
                    p.side.clone(),
                    p.items.iter().map(|i| i.kind.clone()).collect(),
                )
            })
            .collect()
    };
    assert_eq!(kinds(&bar), kinds(&Bar::default()));
    let weather = bar
        .pills
        .iter()
        .flat_map(|p| &p.items)
        .find(|i| i.kind == "weather")
        .unwrap();
    assert_eq!(
        (weather.location.as_str(), weather.units.as_str()),
        ("Springfield", "F")
    );
    // Clicks equal to the defaults stay default, so they follow later improvements.
    assert!(
        bar.pills
            .iter()
            .flat_map(|p| &p.items)
            .all(|i| i.click.is_empty()),
        "{bar:?}"
    );
    assert_eq!(
        (bar.height, bar.y_offset, bar.pill.corner_radius),
        (36, 4, 14)
    );
}

#[test]
fn unknown_items_are_listed() {
    let rc = format!("{OWNER}\nsketchybar --add item mail right --add bracket mail_pill mail\n");
    let (_h, paths) = home(Some(&rc));
    let (_, warnings) = import(&paths);
    assert!(
        warnings.iter().any(|w| w.contains("`mail`")),
        "{warnings:?}"
    );
}

#[test]
#[cfg(target_os = "macos")] // the Mac's targets / CoreText fonts
fn renders_a_bar_that_reads_well() {
    let (_h, paths) = home(None);
    let mut bar = Bar::default();
    bar.pills.push(Pill {
        side: "right".into(),
        items: vec![Item {
            kind: "link".into(),
            label: "Mail".into(),
            click: "open -a 'Mail'".into(),
            ..Item::default()
        }],
        ..Pill::default()
    });
    bar.validate().unwrap();
    let files = render(&paths, &bar).unwrap();
    let rc = &files[0].text;
    assert!(files[0].executable);
    assert!(files.iter().any(|f| f.path.ends_with("bar/stats.sh")));
    // Colours by theme name; the theme file is sourced first.
    assert!(
        rc.contains("background.color=${C_selection/#0xff/0xf0}"),
        "{rc}"
    );
    assert!(rc.contains("current/sketchybar.sh"));
    // Counts as wired for both Palette targets.
    for target in ["sketchybar", "sketchybar-item"] {
        let block = crate::wiring::palette_block(&paths, target).unwrap();
        assert!(
            crate::wiring::has_block(rc, block.lines().nth(1).unwrap()),
            "{target}"
        );
    }
    // Events before their subscribers; one stats script for all eight.
    assert!(
        rc.find("--add event aerospace_workspace_change").unwrap()
            < rc.find("space_dispatcher").unwrap()
    );
    assert!(rc.contains("STATS=\"cpu gpu temp pwr mem disk vpn net\""));
    // Right side, right to left: the clock first, the link (rightmost pill) before it.
    assert!(
        rc.find("--add item link.1 right").unwrap() < rc.find("--add item clock right").unwrap()
    );
    assert!(rc.contains("click_script='open -a '\\''Mail'\\'''"), "{rc}");
    assert!(rc.contains("pill pill.left.0 space.1 space.2 space.3 space.4 space.5"));
    assert!(rc.trim_end().ends_with("sketchybar --update"));
    // bash can read it.
    let check = std::process::Command::new("bash")
        .arg("-n")
        .arg("-c")
        .arg(rc)
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
}

#[test]
fn the_scripts_parse() {
    for (name, text) in SCRIPTS {
        if name.ends_with(".js") && !cfg!(target_os = "macos") {
            continue; // osacompile is macOS's
        }
        // JXA (nowplaying.js) compiles with osascript's own compiler, without running it.
        let check = if name.ends_with(".js") {
            let dir = tempfile::tempdir().unwrap();
            let src = dir.path().join(name);
            fs::write(&src, text).unwrap();
            std::process::Command::new("osacompile")
                .args(["-l", "JavaScript", "-o"])
                .arg(dir.path().join("x.scpt"))
                .arg(&src)
                .output()
                .unwrap()
        } else {
            std::process::Command::new("bash")
                .arg("-n")
                .arg("-c")
                .arg(text)
                .output()
                .unwrap()
        };
        assert!(
            check.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&check.stderr)
        );
    }
}

#[test]
fn refuses_bad_settings() {
    let item = |i: Item| Bar {
        pills: vec![Pill {
            side: "left".into(),
            items: vec![i],
            ..Pill::default()
        }],
        ..Bar::default()
    };
    let bad = [
        Bar {
            height: 4,
            ..Bar::default()
        },
        Bar {
            font: "x\"; rm -rf".into(),
            ..Bar::default()
        },
        Bar {
            icon_colour: "sparkle".into(),
            ..Bar::default()
        },
        item(Item {
            kind: "clock".into(),
            format: "$(rm)".into(),
            ..Item::default()
        }),
        item(Item {
            kind: "weather".into(),
            location: "x'; rm".into(),
            ..Item::default()
        }),
        item(Item {
            kind: "link".into(),
            ..Item::default()
        }),
        item(Item {
            kind: "nope".into(),
            ..Item::default()
        }),
        Bar {
            pills: vec![Pill {
                side: "left".into(),
                items: vec![Item::of("clock"), Item::of("clock")],
                ..Pill::default()
            }],
            ..Bar::default()
        },
    ];
    for b in bad {
        assert!(b.validate().is_err(), "{b:?}");
    }
}

#[test]
fn the_theme_pill_goes_where_you_put_it() {
    let (_h, paths) = home(None);
    let rc = |bar: &Bar| render(&paths, bar).unwrap().remove(0).text;
    let palette = || Pill {
        side: "left".into(),
        items: vec![Item::of("palette")],
        ..Pill::default()
    };
    // After the workspaces, on the left: moved after the last item added.
    let mut bar = Bar::default();
    bar.pills
        .retain(|p| !p.items.iter().any(|i| i.kind == "palette"));
    bar.pills.insert(1, palette());
    bar.validate().unwrap();
    let text = rc(&bar);
    assert!(text.contains("--set palette position=left"), "{text}");
    assert!(
        text.contains("--move palette_gap after space_dispatcher --move palette after palette_gap"),
        "{text}"
    );
    // First on the left: before the next pill's gap, with no gap of its own.
    bar.pills.remove(1);
    bar.pills.insert(0, palette());
    let text = rc(&bar);
    assert!(text.contains("--move palette before gap.left.1"), "{text}");
    assert!(text.contains("palette_gap width=0"), "{text}");
    // Off: removed.
    bar.pills.remove(0);
    assert!(rc(&bar).contains("--remove palette_pill"));
}

#[test]
#[cfg(target_os = "macos")] // the Mac's targets / CoreText fonts
fn fonts_that_dont_exist_are_named() {
    let bar = Bar {
        font: "Helvetica".into(),
        font_style: "Heavy".into(),
        ..Bar::default()
    };
    let w = font_warnings(&bar);
    assert!(
        w[0].contains("no Heavy style") && w[0].contains("Bold"),
        "{w:?}"
    );
    let ok = Bar {
        font_style: "Bold".into(),
        ..bar
    };
    assert!(font_warnings(&ok).is_empty());
}

#[test]
fn workspaces_follow_their_display() {
    let (_h, paths) = home(None);
    let text = render(&paths, &Bar::default()).unwrap().remove(0).text;
    assert!(text.contains("ALL_DISPLAYS=0"), "{text}");
    assert!(text.contains("front_app_switched display_change"), "{text}");
}

#[test]
fn pills_can_sit_in_the_centre() {
    let (_h, paths) = home(None);
    let mut bar = Bar::default();
    // The habits pill and the theme pill move to the centre, habits first.
    for p in bar.pills.iter_mut() {
        if p.items
            .iter()
            .any(|i| i.kind == "habits" || i.kind == "palette")
        {
            p.side = "center".into();
        }
    }
    let at = |bar: &Bar, kind: &str| {
        bar.pills
            .iter()
            .position(|p| p.items.iter().any(|i| i.kind == kind))
            .unwrap()
    };
    let palette = bar.pills.remove(at(&bar, "palette"));
    bar.pills.insert(at(&bar, "habits") + 1, palette);
    bar.validate().unwrap();
    let rc = render(&paths, &bar).unwrap().remove(0).text;
    assert!(rc.contains("--add item habits center"), "{rc}");
    assert!(rc.contains("pill pill.center.0 habits"), "{rc}");
    assert!(rc.contains("--set palette position=center"), "{rc}");
    assert!(rc.contains("--move palette after palette_gap"), "{rc}");
    bar.pills[0].side = "middle".into();
    assert!(bar.validate().is_err());
}

#[test]
fn pills_move_as_a_whole() {
    // Default, on screen: 1 workspaces, 2 stats | 3 palette, 4 habits, 5 weather…
    let sides = |bar: &Bar| {
        bar.screen_order()
            .iter()
            .map(|&i| format!("{}:{}", &bar.pills[i].side[..1], bar.pills[i].items[0].kind))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut bar = Bar::default();
    assert_eq!(
        sides(&bar),
        "l:workspaces l:cpu r:palette r:habits r:weather"
    );
    assert_eq!(bar.find_pill("2").unwrap(), bar.find_pill("cpu").unwrap());
    assert!(bar.find_pill("9").is_err() && bar.find_pill("nope").is_err());

    // Workspaces before the stats: one step right swaps them.
    let i = bar.find_pill("workspaces").unwrap();
    bar.move_pill(i, Place::Step(1)).unwrap();
    assert_eq!(
        sides(&bar),
        "l:cpu l:workspaces r:palette r:habits r:weather"
    );
    // At the end of the left, the next step goes into the (empty) centre.
    let i = bar.find_pill("workspaces").unwrap();
    bar.move_pill(i, "next".parse().unwrap()).unwrap();
    assert_eq!(
        sides(&bar),
        "l:cpu c:workspaces r:palette r:habits r:weather"
    );
    // Habits to the centre's end, then the theme pill before it.
    let i = bar.find_pill("habits").unwrap();
    bar.move_pill(i, "centre".parse().unwrap()).unwrap();
    let (p, h) = (
        bar.find_pill("palette").unwrap(),
        bar.find_pill("habits").unwrap(),
    );
    bar.move_pill(p, Place::Before(h)).unwrap();
    assert_eq!(
        sides(&bar),
        "l:cpu c:workspaces c:palette c:habits r:weather"
    );
    // Stepping left from the centre's first goes to the left's end.
    let i = bar.find_pill("workspaces").unwrap();
    bar.move_pill(i, Place::Step(-1)).unwrap();
    assert_eq!(
        sides(&bar),
        "l:cpu l:workspaces c:palette c:habits r:weather"
    );
    // The screen's edges don't move anything.
    let i = bar.find_pill("cpu").unwrap();
    assert_eq!(bar.move_pill(i, Place::Step(-1)).unwrap(), i);
    // The file stays in screen order, so it reads left to right.
    assert_eq!(bar.screen_order(), (0..bar.pills.len()).collect::<Vec<_>>());
    bar.validate().unwrap();
    assert!("middle".parse::<Place>().is_err());
}

#[test]
fn the_dispatch_pill_listens_for_dispatch() {
    let (_h, paths) = home(None);
    let mut bar = Bar::default();
    bar.pills.push(Pill {
        side: "right".into(),
        items: vec![Item::of("dispatch")],
        ..Pill::default()
    });
    bar.validate().unwrap();
    let files = render(&paths, &bar).unwrap();
    let rc = &files[0].text;
    assert!(rc.contains("--add event dispatch_update"), "{rc}");
    assert!(
        rc.contains("--subscribe dispatch dispatch_update system_woke"),
        "{rc}"
    );
    assert!(rc.contains("dispatch://open/research"), "{rc}");
    assert!(files.iter().any(|f| f.path.ends_with("bar/dispatch.sh")));
}

/// 16a-b: the stats item passes the interval and the GPU reading to stats.sh, the feeder
/// ships with the scripts, the GPU temperature pill exists, and `gpu` only takes busy or raw.
#[test]
fn stats_pass_the_interval_and_gpu_reading() {
    let (_h, paths) = home(None);
    let mut bar = Bar {
        stats_every: 3,
        ..Bar::default()
    };
    let stats = bar
        .pills
        .iter_mut()
        .find(|p| p.items.iter().any(|i| i.kind == "gpu"))
        .unwrap();
    stats.items.push(Item::of("gputemp"));
    stats
        .items
        .iter_mut()
        .find(|i| i.kind == "gpu")
        .unwrap()
        .gpu = "raw".into();
    bar.validate().unwrap();
    let files = render(&paths, &bar).unwrap();
    let rc = &files[0].text;
    assert!(rc.contains("EVERY=3 GPU=raw"), "{rc}");
    assert!(rc.contains("--add item gputemp left"), "{rc}");
    assert!(
        rc.contains("STATS=\"cpu gpu temp pwr mem disk vpn net gputemp\""),
        "{rc}"
    );
    assert!(files.iter().any(|f| f.path.ends_with("bar/stats_feed.sh")));
    stats_gpu(&mut bar, "fast");
    assert!(bar.validate().is_err());
}

fn stats_gpu(bar: &mut Bar, mode: &str) {
    for p in &mut bar.pills {
        for i in &mut p.items {
            if i.kind == "gpu" {
                i.gpu = mode.into();
            }
        }
    }
}

/// A fake `macmon` (two samples) and `sketchybar` (writes its arguments down) on PATH.
fn fake_tools(dir: &std::path::Path, sketchybar_fails: bool) -> std::path::PathBuf {
    let bin = dir.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let sample = r#"{"all_power":3.0,"cpu_active_ratio":0.25,"gpu_active_ratio":0.9,"ram_total":1000,"ram_usage":500,"sys_power":20.5,"temp":{"cpu_temp_avg":72.4,"gpu_temp_avg":40.0}}"#;
    let exe = |name: &str, body: String| {
        let p = bin.join(name);
        fs::write(&p, format!("#!/bin/bash\n{body}\n")).unwrap();
        std::process::Command::new("chmod")
            .arg("+x")
            .arg(&p)
            .status()
            .unwrap();
    };
    exe("macmon", format!("echo '{sample}'; echo '{sample}'"));
    let log = dir.join("calls");
    exe(
        "sketchybar",
        format!(
            "echo \"$*\" >> '{}'{}",
            log.display(),
            if sketchybar_fails { "; exit 1" } else { "" }
        ),
    );
    bin
}

fn run_feeder(dir: &std::path::Path, bin: &std::path::Path) -> std::process::Output {
    let feed = dir.join("stats_feed.sh");
    fs::write(
        &feed,
        SCRIPTS
            .iter()
            .find(|(n, _)| *n == "stats_feed.sh")
            .unwrap()
            .1,
    )
    .unwrap();
    let colours = dir.join(".config/palette/current");
    fs::create_dir_all(&colours).unwrap();
    fs::write(
        colours.join("sketchybar.sh"),
        "RED=0xffff0000 YELLOW=0xffffff00 GREY=0xff777777 C_blue=0xff0000ff\n",
    )
    .unwrap();
    std::process::Command::new("bash")
        .arg(&feed)
        .env("HOME", dir)
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("FEED", "cpu gpu temp gputemp pwr mem")
        .env("EVERY", "1")
        .env("GPU", "busy")
        .env("ICON_KEY", "blue")
        .output()
        .unwrap()
}

#[test]
fn the_feeder_sets_every_item_from_one_macmon() {
    let h = tempfile::tempdir().unwrap();
    let bin = fake_tools(h.path(), false);
    let out = run_feeder(h.path(), &bin);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let calls = fs::read_to_string(h.path().join("calls")).unwrap();
    let first = calls.lines().next().unwrap();
    assert_eq!(calls.lines().count(), 2, "one batch per sample: {calls}");
    // Labels, and colours by level from the theme file read at each sample.
    assert!(
        first.contains("--set cpu label=25% icon.color=0xff0000ff"),
        "{first}"
    );
    assert!(
        first.contains("--set gpu label=90% icon.color=0xffff0000"),
        "{first}"
    );
    assert!(
        first.contains("--set mem label=50% icon.color=0xff0000ff"),
        "{first}"
    );
    assert!(
        first.contains("--set temp label=72\u{b0} icon.color=0xffffff00"),
        "{first}"
    );
    assert!(
        first.contains("--set gputemp label=40\u{b0} icon.color=0xff0000ff"),
        "{first}"
    );
    assert!(
        first.contains("--set pwr label=20W icon.color=0xffffff00"),
        "{first}"
    );
}

#[test]
fn the_feeder_stops_when_the_bar_is_gone() {
    let h = tempfile::tempdir().unwrap();
    let bin = fake_tools(h.path(), true);
    let out = run_feeder(h.path(), &bin);
    assert!(!out.status.success());
    let calls = fs::read_to_string(h.path().join("calls")).unwrap();
    assert_eq!(
        calls.lines().count(),
        1,
        "stopped after the first failed --set: {calls}"
    );
}

/// `palette desktop bar set gpu=raw` sets every GPU pill; a key for a pill the bar lacks fails.
#[test]
fn set_reaches_the_items_options() {
    let (_h, paths) = home(None);
    let mut bar = Bar::default();
    bar.pills
        .retain(|p| !p.items.iter().any(|i| i.kind == "habits"));
    super::super::set(&paths, Tool::Bar, toml::Value::try_from(&bar).unwrap()).unwrap();
    let pair = |k: &str, v: &str| vec![(k.to_string(), v.to_string())];
    super::super::set_keys(&paths, Tool::Bar, &pair("gpu", "raw")).unwrap();
    let saved: Bar = super::super::get(&paths, Tool::Bar)
        .unwrap()
        .try_into()
        .unwrap();
    let gpu = saved
        .pills
        .iter()
        .flat_map(|p| &p.items)
        .find(|i| i.kind == "gpu")
        .unwrap();
    assert_eq!(gpu.gpu, "raw");
    assert!(super::super::set_keys(&paths, Tool::Bar, &pair("gpu", "fast")).is_err());
}

/// stats.sh with macmon: starts the feeder once (the second run keeps it), never runs `top`,
/// and still sets the network itself.
#[test]
fn stats_start_the_feeder_once_and_skip_top() {
    let h = tempfile::tempdir().unwrap();
    let dir = h.path();
    let bin = fake_tools(dir, false);
    let exe = |name: &str, body: &str| {
        let p = bin.join(name);
        fs::write(&p, format!("#!/bin/bash\n{body}\n")).unwrap();
        std::process::Command::new("chmod")
            .arg("+x")
            .arg(&p)
            .status()
            .unwrap();
    };
    exe("macmon", "sleep 20");
    exe(
        "top",
        &format!("echo top >> '{}'", dir.join("top").display()),
    );
    for (name, text) in SCRIPTS {
        fs::write(dir.join(name), text).unwrap();
    }
    let run = || {
        std::process::Command::new("bash")
            .arg(dir.join("stats.sh"))
            .env("HOME", dir)
            .env("TMPDIR", dir)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", bin.display()),
            )
            .env("STATS", "cpu gpu mem net")
            .env("EVERY", "3")
            .output()
            .unwrap()
    };
    assert!(run().status.success());
    let pidfile = fs::read_to_string(dir.join("palette_stats_feed")).unwrap();
    let pid = pidfile.lines().next().unwrap().to_string();
    assert_eq!(pidfile.lines().nth(1), Some("cpu gpu mem|3|busy|blue"));
    assert!(run().status.success());
    let again = fs::read_to_string(dir.join("palette_stats_feed")).unwrap();
    assert_eq!(
        again.lines().next(),
        Some(pid.as_str()),
        "the feeder was kept"
    );
    assert!(!dir.join("top").exists(), "top ran");
    let calls = fs::read_to_string(dir.join("calls")).unwrap();
    assert!(calls.contains("--set net"), "{calls}");
    assert!(
        !calls.contains("--set cpu"),
        "the feeder sets the CPU: {calls}"
    );
    // The feeder's whole group goes (bash, macmon, awk).
    std::process::Command::new("kill")
        .args(["-TERM", "--", &format!("-{pid}")])
        .status()
        .unwrap();
}

/// 17a: Performance mode slows stats 10×, every other timed pill 3× (at most an hour), never
/// the clock; the saved bar stays as it was.
#[test]
fn performance_mode_slows_by_fixed_multiples() {
    let (_h, paths) = home(None);
    let bar = Bar::default();
    let freq = |rc: &str, item: &str| -> u32 {
        let line = rc
            .lines()
            .find(|l| l.contains(&format!("--add item {item} ")))
            .unwrap_or_else(|| panic!("{item}"));
        let at = line.find("update_freq=").unwrap() + "update_freq=".len();
        line[at..]
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    let before = render(&paths, &bar).unwrap().remove(0).text;
    crate::performance(&paths, Some(true), &[]).unwrap();
    let after = render(&paths, &bar).unwrap().remove(0).text;
    assert_eq!(
        freq(&before, "stats_dispatcher") * 10,
        freq(&after, "stats_dispatcher")
    );
    assert_eq!(
        freq(&before, "space_dispatcher") * 3,
        freq(&after, "space_dispatcher")
    );
    assert_eq!(freq(&before, "battery") * 3, freq(&after, "battery"));
    assert_eq!(freq(&after, "weather"), 2700);
    assert_eq!(freq(&before, "clock"), freq(&after, "clock"));
    assert!(after.contains("EVERY=50"), "the feeder slows too");
    assert_eq!(bar, Bar::default());
}

/// 18d: an app switch sends two events at once; the workspaces are drawn once, by the last.
#[test]
fn one_app_switch_draws_the_workspaces_once() {
    let h = tempfile::tempdir().unwrap();
    let bin = h.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let calls = h.path().join("calls");
    let stub = |name: &str, body: &str| {
        let p = bin.join(name);
        fs::write(&p, format!("#!/bin/bash\n{body}\n")).unwrap();
        fs::set_permissions(&p, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    };
    stub(
        "aerospace",
        &format!(
            "echo \"$SENDER_TAG $*\" >> '{}'\ncase \"$*\" in *monitor*) echo '1|1';; *focused*) echo 1;; esac",
            calls.display()
        ),
    );
    stub("sketchybar", ":");
    let script = h.path().join("spaces.sh");
    fs::write(
        &script,
        SCRIPTS.iter().find(|(n, _)| *n == "spaces.sh").unwrap().1,
    )
    .unwrap();
    let run = |sender: &str| {
        std::process::Command::new("bash")
            .arg(&script)
            .env("HOME", h.path())
            .env("TMPDIR", h.path())
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .env("SENDER", sender)
            .env("SENDER_TAG", sender)
            .env("SLOTS", "12")
            .spawn()
            .unwrap()
    };
    let (mut a, mut b) = (run("front_app_switched"), run("aerospace_workspace_change"));
    a.wait().unwrap();
    b.wait().unwrap();
    let log = fs::read_to_string(&calls).unwrap();
    let windows = log.lines().filter(|l| l.contains("list-windows")).count();
    assert_eq!(windows, 1, "one run draws the bar: {log}");
    // A timer run (routine) never waits.
    let mut c = run("routine");
    c.wait().unwrap();
    let log = fs::read_to_string(&calls).unwrap();
    assert_eq!(
        log.lines().filter(|l| l.contains("list-windows")).count(),
        2,
        "{log}"
    );
}

/// Owner, 2026-11-16 (Ollama): a workspace whose windows are all off screen gets a faded
/// label; one with any window on screen stays as it was; no on-screen answer fades nothing.
#[test]
fn workspaces_with_only_off_screen_windows_fade() {
    let h = tempfile::tempdir().unwrap();
    let bin = h.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let drawn = h.path().join("drawn");
    let stub = |name: &str, body: &str| {
        let p = bin.join(name);
        fs::write(&p, format!("#!/bin/bash\n{body}\n")).unwrap();
        fs::set_permissions(&p, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    };
    stub(
        "aerospace",
        "case \"$*\" in\n\
         *list-windows*) printf '1|Zed|11\\n2|Ollama|22\\n3|Zed|33\\n3|Ollama|44\\n';;\n\
         *monitor*) printf '1|1\\n2|1\\n3|1\\n';;\n\
         *focused*) echo 9;;\nesac",
    );
    stub(
        "sketchybar",
        &format!("printf '%s\\n' \"$@\" > '{}'", drawn.display()),
    );
    let script = h.path().join("spaces.sh");
    fs::write(
        &script,
        SCRIPTS.iter().find(|(n, _)| *n == "spaces.sh").unwrap().1,
    )
    .unwrap();
    let draw = |onscreen: &str| {
        let ok = std::process::Command::new("bash")
            .arg(&script)
            .env("HOME", h.path())
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .env("FG", "0xffeeeeee")
            .env("PALETTE_ONSCREEN", onscreen)
            .status()
            .unwrap();
        assert!(ok.success());
        let out = fs::read_to_string(&drawn).unwrap();
        // The label colour each slot got, in order.
        out.lines()
            .filter_map(|l| l.strip_prefix("label.color="))
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    // Ollama's window 22 is off screen: workspace 2 fades; 3 has Zed on screen.
    assert_eq!(draw("11 33"), ["0xffeeeeee", "0x66eeeeee", "0xffeeeeee"]);
    // No answer (Linux, an older macOS): nothing fades.
    assert_eq!(draw(""), ["0xffeeeeee"; 3]);
}

/// 21g (owner, 2026-11-15: the nearest glyph): apps the icon font lacks get one from
/// Palette's small map; your icon_map.sh wins wherever it knows the app.
#[test]
fn apps_the_icon_font_lacks_get_the_nearest_glyph() {
    let h = tempfile::tempdir().unwrap();
    let map = h.path().join(".config/sketchybar/helpers/icon_map.sh");
    fs::create_dir_all(map.parent().unwrap()).unwrap();
    fs::write(
        &map,
        "__icon_map() { case \"$1\" in Ghostty) icon_result=\":ghostty:\";; Herdr) icon_result=\":mine:\";; *) icon_result=\":default:\";; esac; }\n",
    )
    .unwrap();
    for script in ["spaces.sh", "front_app.sh"] {
        let text = SCRIPTS.iter().find(|(n, _)| *n == script).unwrap().1;
        // Each script calls the fallback only after your map says :default:.
        assert!(
            text.contains("[ \"$icon_result\" = \":default:\" ] && palette_icon"),
            "{script}"
        );
        // Palette's fallback function as the script has it, after your map.
        let start = text.find("palette_icon() {").unwrap();
        let end = start + text[start..].find("\n}\n").unwrap() + 3;
        let probe = format!(
            "source \"$HOME/.config/sketchybar/helpers/icon_map.sh\"\n{}\n\
             for a in 'Rex Beta' Rex Herdr Ghostty Sonos Orca 'DeepSeek Harness'; do __icon_map \"$a\"; \
             [ \"$icon_result\" = :default: ] && palette_icon \"$a\"; echo \"$icon_result\"; done",
            &text[start..end]
        );
        let out = std::process::Command::new("bash")
            .args(["-c", &probe])
            .env("HOME", h.path())
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            ":terminal:\n:terminal:\n:mine:\n:ghostty:\n:default:\n:code:\n:deepseek:\n",
            "{script}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn your_apps_keeps_the_ones_on_this_mac_and_layouts_follow_the_theme_pill() {
    let mut bar = Bar::default();
    bar.pills
        .retain(|p| !p.items.iter().any(|i| i.kind == "habits"));
    let added = presets::add(&mut bar, "apps", "right", &|k| k == "habits").unwrap();
    let p = &bar.pills[added.pill.unwrap()];
    assert_eq!(
        p.items.iter().map(|i| i.kind.as_str()).collect::<Vec<_>>(),
        ["habits"]
    );
    assert_eq!(added.skipped, ["Dispatch (not on this Mac)"]);
    bar.validate().unwrap();
    // Every layout brings Palette's theme pill; Default leaves the Habit Tracker to Your apps.
    bar.pills
        .retain(|p| !p.items.iter().any(|i| i.kind == "palette"));
    presets::add(&mut bar, "default", "right", &|_| true).unwrap();
    let kinds: Vec<&str> = bar
        .pills
        .iter()
        .flat_map(|p| &p.items)
        .map(|i| i.kind.as_str())
        .collect();
    assert!(
        kinds.contains(&"palette") && !kinds.contains(&"habits"),
        "{kinds:?}"
    );
    bar.validate().unwrap();
    for l in presets::LOOKS {
        presets::with_look(&bar, l.id, Some(37))
            .unwrap()
            .validate()
            .unwrap();
    }
}

/// ROADMAP 20: the now-playing item, its beat lines left of the title on screen (right side: added
/// right to left), the click and the title's width/scroll, and no beat lines in "Slower bar".
#[test]
fn now_playing_renders_title_and_beat_lines() {
    let (_h, paths) = home(None);
    let mut bar = Bar::default();
    let mut np = Item::of("nowplaying");
    np.width = 30;
    np.scroll = false;
    np.action = "open".into();
    bar.pills.push(Pill {
        side: "right".into(),
        items: vec![np],
        ..Pill::default()
    });
    bar.validate().unwrap();
    let rc = &render(&paths, &bar).unwrap()[0].text;
    assert!(rc.contains("label.max_chars=30 scroll_texts=off"), "{rc}");
    assert!(rc.contains("WIDTH=30 SCROLL=0 BEATS=5 bash"), "{rc}");
    assert!(rc.contains("nowplaying.sh\" open'"), "{rc}");
    let at = |s: &str| rc.find(s).unwrap_or_else(|| panic!("{s} missing"));
    assert!(at("--add item nowplaying right") < at("--add item beat.5 right"));
    assert!(at("--add item beat.5 right") < at("--add item beat.1 right"));
    assert!(
        rc.contains("pill pill.right.0 nowplaying beat.5 beat.4 beat.3 beat.2 beat.1"),
        "{rc}"
    );
    assert!(wants_beats(&paths, &bar));
    // Slower bar: no beat lines, the title stays.
    let mut s = crate::settings::load(&paths).unwrap_or_default();
    s.performance.on = true;
    crate::settings::save(&paths, &s).unwrap();
    let rc = &render(&paths, &bar).unwrap()[0].text;
    assert!(
        rc.contains("--add item nowplaying") && !rc.contains("beat.1"),
        "{rc}"
    );
    assert!(rc.contains("BEATS=0"));
    assert!(!wants_beats(&paths, &bar));
    // Checked: width and click.
    bar.pills.last_mut().unwrap().items[0].width = 3;
    assert!(bar.validate().unwrap_err().to_string().contains("6 to 80"));
}

/// The item's script keeps one feeder and replaces it when the settings change.
#[test]
fn now_playing_keeps_one_feeder() {
    let h = tempfile::tempdir().unwrap();
    let bin = h.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let sb = bin.join("sketchybar");
    fs::write(&sb, "#!/bin/bash\n:\n").unwrap();
    fs::set_permissions(&sb, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let dir = h.path().join("bar");
    fs::create_dir_all(&dir).unwrap();
    for name in ["nowplaying.sh", "nowplaying.js"] {
        fs::write(
            dir.join(name),
            SCRIPTS.iter().find(|(n, _)| *n == name).unwrap().1,
        )
        .unwrap();
    }
    let run = |width: &str| {
        std::process::Command::new("bash")
            .arg(dir.join("nowplaying.sh"))
            .env("HOME", h.path())
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .env("WIDTH", width)
            .status()
            .unwrap();
        fs::read_to_string(h.path().join("Library/Caches/palette/nowplaying.pid")).unwrap()
    };
    let first = run("24");
    assert_eq!(run("24"), first, "still running: not started again");
    let second = run("30");
    assert_ne!(second, first, "new settings: a new feeder");
    let pid = second.lines().next().unwrap().to_string();
    let _ = std::process::Command::new("kill")
        .args(["-TERM", &format!("-{pid}")])
        .status();
}

#[test]
fn scripts_follow_the_apps_version_only_for_a_managed_bar() {
    let (_h, paths) = home(None);
    let dir = scripts_dir(&paths);
    fs::create_dir_all(&dir).unwrap();
    // An older version's script and a missing one.
    fs::write(dir.join("spaces.sh"), "#!/bin/bash\n# old\n").unwrap();

    // Not managed: your bar, nothing written.
    assert!(update_scripts(&paths).unwrap().is_empty());
    assert_eq!(
        fs::read_to_string(dir.join("spaces.sh")).unwrap(),
        "#!/bin/bash\n# old\n"
    );

    // Managed: the old and missing ones are rewritten, executable.
    let model = super::super::model_path(&paths, Tool::Bar);
    fs::create_dir_all(model.parent().unwrap()).unwrap();
    fs::write(&model, toml::to_string(&Bar::default()).unwrap()).unwrap();
    let updated = update_scripts(&paths).unwrap();
    assert_eq!(updated.len(), SCRIPTS.len(), "{updated:?}");
    for (name, text) in SCRIPTS {
        assert_eq!(fs::read_to_string(dir.join(name)).unwrap(), text, "{name}");
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(dir.join(name)).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "{name} executable");
    }
    // Up to date: nothing to do the second time.
    assert!(update_scripts(&paths).unwrap().is_empty());
    // One edited by hand comes back alone.
    fs::write(dir.join("stats.sh"), "edited").unwrap();
    assert_eq!(update_scripts(&paths).unwrap(), vec!["stats.sh"]);
}

#[test]
fn your_icons_win_in_a_safe_generated_file() {
    let icons = BTreeMap::from([
        ("Ollama".to_string(), ":ollama:".to_string()),
        ("Bob's App".to_string(), "★".to_string()),
    ]);
    let script = icons_script(&icons);
    let ask = |app: &str| {
        let out = std::process::Command::new("bash")
            .arg("-c")
            .arg(format!(
                "{script}\nicon_result=''; user_icon {}; printf %s \"$icon_result\"",
                super::super::sh_quote(app)
            ))
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap()
    };
    assert_eq!(ask("Ollama"), ":ollama:");
    assert_eq!(ask("Bob's App"), "★");
    assert_eq!(ask("Safari"), "");

    assert!(check_icon("Ollama", ":ollama:").is_ok());
    assert!(check_icon("Ollama", "AI").is_ok());
    assert!(check_icon("Ollama", "$(rm)").is_err());
    assert!(check_icon("Ollama", "a b").is_err());
    assert!(check_icon("Ollama", ":ll ama:").is_err());
    assert!(check_icon("", ":ollama:").is_err());

    // Kept in bar.toml and back.
    let bar = Bar {
        icons,
        ..Bar::default()
    };
    let back: Bar = toml::from_str(&toml::to_string(&bar).unwrap()).unwrap();
    assert_eq!(back.icons, bar.icons);
    assert!(
        !toml::to_string(&Bar::default())
            .unwrap()
            .contains("[icons]")
    );
}

#[test]
fn glyph_names_come_from_the_icon_map() {
    let (_h, paths) = home(None);
    assert!(glyphs(&paths).is_empty());
    let map = paths.config_home().join("sketchybar/helpers/icon_map.sh");
    fs::create_dir_all(map.parent().unwrap()).unwrap();
    fs::write(
        &map,
        "case \"$1\" in\n\"Ollama\") icon_result=\":ollama:\" ;;\n\"Zed\") icon_result=\":zed:\" ;;\n\"X\") icon_result=\":zed:\" ;;\n*) icon_result=\":default:\" ;;\nesac",
    )
    .unwrap();
    assert_eq!(glyphs(&paths), vec![":default:", ":ollama:", ":zed:"]);
}

#[test]
fn the_map_says_which_icon_each_app_gets() {
    let (_h, paths) = home(None);
    let apps = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
    assert!(
        map_icons(&paths, &apps(&["Zed"])).is_empty(),
        "no map, no answer"
    );
    let map = paths.config_home().join("sketchybar/helpers/icon_map.sh");
    fs::create_dir_all(map.parent().unwrap()).unwrap();
    // An older map without the "only when run directly" guard: it maps its own arguments,
    // which must not end up in the answer.
    fs::write(
        &map,
        "function __icon_map() {\n case \"$1\" in\n \"Zed\") icon_result=\":zed:\" ;;\n \"Adobe Bridge\"*) icon_result=\":adobe_bridge:\" ;;\n *) icon_result=\":default:\" ;;\n esac\n}\nfor a in \"$@\"; do __icon_map \"$a\"; printf 'LEAK %s\\n' \"$icon_result\"; done\n",
    )
    .unwrap();
    let got = map_icons(&paths, &apps(&["Zed", "Adobe Bridge 2026", "Rex Beta"]));
    assert_eq!(got.len(), 3, "{got:?}");
    assert_eq!(got["Zed"], ":zed:");
    assert_eq!(got["Adobe Bridge 2026"], ":adobe_bridge:");
    assert_eq!(got["Rex Beta"], ":default:");
    assert!(
        installed_apps(&paths).is_empty(),
        "a fake home lists no apps"
    );
}
