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
        let check = std::process::Command::new("bash")
            .arg("-n")
            .arg("-c")
            .arg(text)
            .output()
            .unwrap();
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
