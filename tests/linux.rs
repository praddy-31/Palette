//! Linux, tried on any machine: `PALETTE_PLATFORM=linux` (honoured only in a
//! fake home) gives the Linux list of targets, and `PALETTE_SESSION` picks the
//! desktop. Nothing runs: fake homes never apply effects.

mod common;

use common::Home;

const LINUX: (&str, &str) = ("PALETTE_PLATFORM", "linux");

fn on(session: &str) -> [(&'static str, &str); 2] {
    [LINUX, ("PALETTE_SESSION", session)]
}

#[test]
fn linux_renders_the_desktops_and_leaves_out_mac_apps() {
    let home = Home::new();
    let out = home.ok_env(&["set", "tokyo-night"], &on("gnome"));
    for file in [
        "kde.colors",
        "noctalia.json",
        "niri.kdl",
        "gnome.ini",
        "ghostty.conf",
        "konsole.colorscheme",
        "gnome-terminal.dconf",
        "fuzzel.ini",
    ] {
        assert!(out.contains(&format!("+ {file}")), "{file}: {out}");
    }
    for mac in ["borders.sh", "sketchybar", "chess-review.json", "habits"] {
        assert!(!out.contains(mac), "{mac} on Linux: {out}");
    }
    assert!(home.current_file("gnome.ini").contains("accent=#7aa2f7"));
    assert!(
        home.current_file("niri.kdl")
            .contains("active-color \"#7aa2f7\"")
    );
}

#[test]
fn the_mac_gets_no_linux_files() {
    let home = Home::new();
    let out = home.ok(&["set", "tokyo-night"]);
    assert!(
        !out.contains("kde.colors") && !out.contains("noctalia.json"),
        "{out}"
    );
    assert!(out.contains("borders.sh"), "{out}");
}

#[test]
fn kde_scheme_has_every_group_in_kde_format() {
    let home = Home::new();
    home.ok_env(&["set", "tokyo-night"], &on("kde"));
    let scheme = home.current_file("kde.colors");
    for group in [
        "[Colors:Button]",
        "[Colors:Complementary]",
        "[Colors:Header]",
        "[Colors:Selection]",
        "[Colors:Tooltip]",
        "[Colors:View]",
        "[Colors:Window]",
        "[General]",
        "[WM]",
    ] {
        assert!(scheme.contains(group), "{group} missing:\n{scheme}");
    }
    // KDE's `r,g,b`: accent #7aa2f7, no spaces.
    assert!(scheme.contains("DecorationFocus=122,162,247"), "{scheme}");
    // Every colour line is filled in.
    for line in scheme
        .lines()
        .filter(|l| l.contains('=') && !l.starts_with('#'))
    {
        let (key, value) = line.split_once('=').unwrap();
        assert!(!value.is_empty(), "{key} is empty");
    }
}

#[test]
fn noctalia_palette_is_valid_json_with_both_modes() {
    let home = Home::new();
    home.ok_env(&["set", "tokyo-night"], &on("niri-noctalia"));
    let text = home.current_file("noctalia.json");
    let json: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    for mode in ["dark", "light"] {
        let v = &json[mode];
        assert_eq!(v["mPrimary"], "#7aa2f7", "{mode}");
        for key in [
            "mOnPrimary",
            "mSecondary",
            "mOnSecondary",
            "mTertiary",
            "mOnTertiary",
            "mError",
            "mOnError",
            "mSurface",
            "mOnSurface",
            "mSurfaceVariant",
            "mOnSurfaceVariant",
            "mOutline",
            "mShadow",
            "mHover",
            "mOnHover",
        ] {
            let hex = v[key]
                .as_str()
                .unwrap_or_else(|| panic!("{mode}.{key} missing"));
            assert!(
                hex.starts_with('#') && hex.len() == 7,
                "{mode}.{key} = {hex}"
            );
        }
        for group in ["normal", "bright"] {
            for colour in [
                "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
            ] {
                assert!(
                    v["terminal"][group][colour].is_string(),
                    "{mode}.{group}.{colour}"
                );
            }
        }
    }
}

#[test]
fn apps_are_one_list_for_every_desktop() {
    let home = Home::new();
    let state = |apps: &str, name: &str| {
        apps.lines()
            .find(|l| l.split_whitespace().nth(1) == Some(name))
            .map(|l| l.split_whitespace().next().unwrap().to_string())
            .unwrap_or_else(|| panic!("{name} not listed:\n{apps}"))
    };
    let apps = home.ok_env(&["apps"], &on("niri-noctalia"));
    for app in ["ghostty", "foot", "neovim", "noctalia", "niri", "wallpaper"] {
        assert_eq!(state(&apps, app), "on", "{app}");
    }
    // Switched off on a niri day: off on a GNOME day too.
    home.ok_env(&["apps", "off", "foot"], &on("niri-noctalia"));
    let gnome = home.ok_env(&["apps"], &on("gnome"));
    assert_eq!(state(&gnome, "foot"), "off");
}

#[test]
fn doctor_names_the_desktop() {
    let home = Home::new();
    home.ok_env(&["set", "tokyo-night"], &on("kde"));
    let out = home.run_env(&["doctor"], &on("kde"));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Linux, KDE Plasma"), "{text}");
    let out = home.run_env(&["doctor"], &on("niri-noctalia"));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Noctalia can theme apps too"), "{text}");
}

#[test]
fn init_adds_an_include_line_to_niri() {
    let home = Home::new();
    let env = on("niri-noctalia");
    home.write(".config/niri/config.kdl", "layout {\n    gaps 16\n}\n");
    home.ok_env(&["set", "tokyo-night"], &env);
    home.ok_env(&["init"], &env);
    let config = std::fs::read_to_string(home.path(".config/niri/config.kdl")).unwrap();
    assert!(config.starts_with("layout {\n    gaps 16\n}\n"), "{config}");
    let generated = home.palette_dir().join("current/niri.kdl");
    assert!(
        config.contains(&format!("include \"{}\"", generated.display())),
        "{config}"
    );
    assert!(
        config.contains("// "),
        "marker comment uses KDL's //: {config}"
    );
}

#[test]
fn every_bundled_theme_renders_for_linux() {
    let home = Home::new();
    let themes = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/themes")).unwrap();
    for theme in themes.flatten().filter(|e| e.path().is_dir()) {
        let name = theme.file_name().to_string_lossy().to_string();
        home.ok_env(&["set", &name], &on("niri-noctalia"));
        let json = home.current_file("noctalia.json");
        serde_json::from_str::<serde_json::Value>(&json).unwrap_or_else(|e| panic!("{name}: {e}"));
        for file in ["kde.colors", "konsole.colorscheme", "fuzzel.ini"] {
            assert!(
                !home.current_file(file).contains("=\n"),
                "{name}: empty value in {file}"
            );
        }
        let terminal = home.current_file("gnome-terminal.dconf");
        assert_eq!(terminal.matches("'#").count(), 22, "{name}: {terminal}");
    }
}

#[test]
fn foot_gets_hex_without_hash_and_an_include_at_the_top() {
    let home = Home::new();
    let env = on("niri-noctalia");
    home.write(
        ".config/foot/foot.ini",
        "[main]\nfont=JetBrains Mono:size=11\n",
    );
    home.ok_env(&["set", "tokyo-night"], &env);
    let colours = home.current_file("foot.ini");
    assert!(
        colours.contains("\n[colors]\nforeground=a9b1d6\n"),
        "{colours}"
    );
    assert!(
        colours.contains("regular0=") && colours.contains("bright7="),
        "{colours}"
    );
    home.ok_env(&["init"], &env);
    let config = std::fs::read_to_string(home.path(".config/foot/foot.ini")).unwrap();
    let generated = home.palette_dir().join("current/foot.ini");
    let include = format!("include={}", generated.display());
    let first_section = config.find("[main]").unwrap();
    assert!(
        config.find(&include).is_some_and(|i| i < first_section),
        "{config}"
    );
}

#[test]
fn konsole_scheme_has_every_colour() {
    let home = Home::new();
    home.ok_env(&["set", "tokyo-night"], &on("kde"));
    let scheme = home.current_file("konsole.colorscheme");
    for i in 0..8 {
        for section in [
            format!("[Color{i}]"),
            format!("[Color{i}Intense]"),
            format!("[Color{i}Faint]"),
        ] {
            assert!(scheme.contains(&section), "{section}: {scheme}");
        }
    }
    assert!(
        scheme.contains("[Background]\nColor=26,27,38\n"),
        "{scheme}"
    );
}

#[test]
fn fuzzel_gets_an_include_at_the_top_and_doctor_names_own_colours() {
    let home = Home::new();
    let env = on("niri-noctalia");
    home.write(
        ".config/fuzzel/fuzzel.ini",
        "[main]\nfont=JetBrains Mono:size=14\n\n[colors]\nbackground=1a1b26f2\n",
    );
    home.ok_env(&["set", "tokyo-night"], &env);
    assert!(
        home.current_file("fuzzel.ini")
            .contains("\nbackground=1a1b26f2\n")
    );
    home.ok_env(&["init"], &env);
    let config = std::fs::read_to_string(home.path(".config/fuzzel/fuzzel.ini")).unwrap();
    let include = format!(
        "include={}",
        home.palette_dir().join("current/fuzzel.ini").display()
    );
    assert!(
        config
            .find(&include)
            .is_some_and(|i| i < config.find("[main]").unwrap()),
        "{config}"
    );
    let doctor = String::from_utf8_lossy(&home.run_env(&["doctor"], &env).stdout).to_string();
    assert!(
        doctor.contains("fuzzel.ini sets its own colours (background)"),
        "{doctor}"
    );
}
