use super::*;

const OWNER: &str = include_str!("../../../tests/fixtures/starship-owner.toml");
const CATPPUCCIN: &str = include_str!("../../../tests/fixtures/starship-catppuccin.toml");

fn home(text: Option<&str>) -> (tempfile::TempDir, Paths) {
    let h = tempfile::tempdir().unwrap();
    let paths = Paths::fake_home(h.path());
    if let Some(text) = text {
        let file = config_file(&paths);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }
    (h, paths)
}

fn modules(p: &Prompt) -> Vec<(String, Vec<String>)> {
    p.blocks
        .iter()
        .map(|b| (b.colour.clone(), b.modules.clone()))
        .collect()
}

#[test]
fn reads_the_owners_prompt() {
    let (p, warnings) = from_text(OWNER);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(p.shape, "round");
    assert_eq!(p.start, "fade");
    assert!(p.second_line);
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert_eq!(
        modules(&p),
        vec![
            ("foreground".into(), s(&["os"])),
            ("blue".into(), s(&["directory"])),
            ("muted".into(), s(&["git_branch", "git_status"])),
            (
                "lighter_background".into(),
                s(&["nodejs", "bun", "rust", "golang", "php"])
            ),
            ("background".into(), s(&["time"])),
        ]
    );
    // Text colours from each block's first module.
    assert_eq!(p.blocks[0].text, "darker_background");
    assert_eq!(p.blocks[1].text, "bright_foreground");
    assert_eq!(p.blocks[2].text, "blue");
    // A hex text colour is kept as you wrote it.
    assert_eq!(p.blocks[4].text, "#a0a9cb");
    assert_eq!(
        p.options["directory"]["truncation_length"],
        toml::Value::Integer(3)
    );
    assert_eq!(
        p.options["time"]["time_format"],
        toml::Value::String("%R".into())
    );
    assert!(p.versions);
    p.validate().unwrap();
}

#[test]
fn writes_key_by_key_and_reads_back() {
    let (_h, paths) = home(Some(OWNER));
    let (mut p, _) = import(&paths);
    p.shape = "arrow".into();
    p.blocks.swap(1, 2); // git before the folder
    p.blocks[4].modules.push("cmd_duration".into());
    p.options
        .entry("directory".into())
        .or_default()
        .insert("truncation_length".into(), toml::Value::Integer(2));
    p.validate().unwrap();
    let text = render(&paths, &p).unwrap().remove(0).text;
    // The format, one piece per line, arrows between blocks.
    assert!(
        text.starts_with("\"$schema\""),
        "the first line stays first:\n{text}"
    );
    for piece in [
        "format = \"\"\"\n[░▒▓](foreground)\\\n$os\\\n",
        "[\u{e0b0}](fg:foreground bg:muted)\\\n$git_branch\\\n$git_status\\\n",
        "[\u{e0b0}](fg:muted bg:blue)\\\n$directory\\\n",
    ] {
        assert!(text.contains(piece), "{piece:?} in\n{text}");
    }
    assert!(
        text.contains(
            "$time\\\n$cmd_duration\\\n[\u{e0b0} ](fg:background)\\\n$line_break$character\"\"\""
        ),
        "{text}"
    );
    // Modules restyled for their block; options in place.
    assert!(text.contains("style = \"bg:muted fg:blue\""), "{text}");
    assert!(
        text.contains("format = \"[ $symbol $branch ]($style)\""),
        "{text}"
    );
    assert!(text.contains("truncation_length = 2"), "{text}");
    // Untouched: your folder icons, the OS symbols.
    assert!(text.contains("\"Documents\" = \"󰈙 \""), "{text}");
    assert!(text.contains("Macos = \"󰀵\""));
    // And it reads back as the same prompt.
    fs::write(config_file(&paths), &text).unwrap();
    let (back, _) = import(&paths);
    assert_eq!(modules(&back), modules(&p));
    assert_eq!(back.shape, "arrow");
    assert_eq!(back.blocks[0].text, p.blocks[0].text);
    assert_eq!(
        back.options["directory"]["truncation_length"],
        toml::Value::Integer(2)
    );
    // Writing the same prompt again changes nothing.
    assert_eq!(render(&paths, &back).unwrap().remove(0).text, text);
}

#[test]
fn plain_prompts_and_no_config() {
    let plain =
        "format = \"$directory$git_branch$character\"\n[directory]\nstyle = \"bold cyan\"\n";
    let (p, _) = from_text(plain);
    assert_eq!(p.shape, "plain");
    assert_eq!(
        modules(&p),
        vec![
            ("cyan".into(), vec!["directory".to_string()]),
            ("foreground".into(), vec!["git_branch".to_string()])
        ]
    );
    let (_h, paths) = home(None);
    let text = render(&paths, &p).unwrap().remove(0).text;
    assert!(
        text.contains("$directory\\\n$git_branch\\\n$character"),
        "{text}"
    );
    assert!(text.contains("style = \"fg:cyan\""));
    // Starship's default prompt (no format): Palette's default to start from.
    assert_eq!(
        from_text("add_newline = false\n").0.blocks,
        Prompt::default().blocks
    );
}

#[test]
fn a_preset_reads_in_its_own_colours() {
    let (p, _) = read_prompt(CATPPUCCIN, true);
    assert_eq!(p.shape, "arrow");
    assert_eq!(p.start, "round");
    let colours: Vec<&str> = p.blocks.iter().map(|b| b.colour.as_str()).collect();
    assert_eq!(
        colours,
        ["red", "peach", "yellow", "green", "sapphire", "lavender"]
    );
    assert_eq!(p.blocks[0].modules, ["os", "username"]);
    // Named through Palette's starship palette, or the nearest theme colour.
    let theme = vec![
        ("orange".to_string(), "#ff9e64".to_string()),
        ("blue".to_string(), "#7aa2f7".to_string()),
    ];
    assert_eq!(to_theme("peach", &BTreeMap::new(), &theme), "orange");
    assert_eq!(to_theme("#7ba0f0", &BTreeMap::new(), &theme), "blue");
    // Gruvbox's names are in Palette's starship palette too.
    assert_eq!(to_theme("color_aqua", &BTreeMap::new(), &theme), "cyan");
    // A preset's own palette name: its hex, then the nearest theme colour.
    let palette = BTreeMap::from([("my_orange".to_string(), "#ffa060".to_string())]);
    assert_eq!(to_theme("my_orange", &palette, &theme), "orange");
}

#[test]
fn refuses_bad_values() {
    let p = Prompt {
        shape: "zigzag".into(),
        ..Prompt::default()
    };
    assert!(p.validate().is_err());
    let mut p = Prompt::default();
    p.blocks[0].colour = "sparkle".into();
    assert!(p.validate().is_err());
    let mut p = Prompt::default();
    p.blocks[1].modules.push("os".into());
    assert!(p.validate().is_err(), "twice");
    let mut p = Prompt::default();
    p.options
        .entry("directory".into())
        .or_default()
        .insert("truncation_length".into(), toml::Value::String("x".into()));
    assert!(p.validate().is_err());
}

#[test]
fn colour_codes_become_spans() {
    let spans = ansi_spans("\u{1b}[38;2;1;2;3;48;2;16;32;48m ~ \u{1b}[0m\u{1b}[1;32m❯\u{1b}[0m ");
    assert_eq!(spans[0].text, " ~ ");
    assert_eq!(
        (spans[0].fg.as_str(), spans[0].bg.as_str()),
        ("#010203", "#102030")
    );
    assert_eq!(spans[1].text, "❯");
    assert!(spans[1].bold);
    assert_eq!(spans[1].fg, "green");
    assert_eq!(spans[2].text, " ");
}

#[test]
fn a_preset_without_line_break_stays_on_one_line() {
    let (p, _) = read_prompt(CATPPUCCIN, true);
    assert!(!p.second_line, "catppuccin turns line_break off");
    let (_h, paths) = home(Some(CATPPUCCIN));
    let mut p = p;
    p.second_line = true;
    for b in &mut p.blocks {
        b.colour = "blue".into();
        b.text = "foreground".into();
    }
    let text = render(&paths, &p).unwrap().remove(0).text;
    assert!(text.contains("[line_break]\ndisabled = false"), "{text}");
}

#[test]
fn username_takes_its_own_style_keys() {
    let (_h, paths) = home(Some(
        "[username]\nstyle = \"bold red\"\nshow_always = true\n",
    ));
    let p = Prompt {
        blocks: vec![Block {
            colour: "orange".into(),
            text: "darker_background".into(),
            modules: vec!["username".into()],
        }],
        ..Prompt::default()
    };
    let text = render(&paths, &p).unwrap().remove(0).text;
    assert!(
        text.contains("style_user = \"bg:orange fg:darker_background\""),
        "{text}"
    );
    assert!(
        text.contains("style_root = \"bg:orange fg:darker_background\""),
        "{text}"
    );
    assert!(
        !text.contains("\nstyle ="),
        "Starship rejects username.style:\n{text}"
    );
    fs::write(config_file(&paths), &text).unwrap();
    assert_eq!(import(&paths).0.blocks[0].text, "darker_background");
}

#[test]
fn one_folder_by_default_but_yours_wins() {
    let (_h, paths) = home(None);
    let text = render(&paths, &Prompt::default()).unwrap().remove(0).text;
    assert!(text.contains("truncation_length = 1"), "{text}");
    let (p, _) = from_text("[directory]\ntruncation_length = 3\n");
    assert_eq!(
        p.options["directory"]["truncation_length"].as_integer(),
        Some(3)
    );
}
