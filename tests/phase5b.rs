//! Phase 5b: Obsidian vaults and export/import between Macs.

mod common;

use std::fs;

use common::Home;

const APPEARANCE: &str = "{\n  \"cssTheme\": \"Osaka Jade\",\n  \"accentColor\": \"#7ee687\",\n  \"baseFontSize\": 18\n}";
const APPEARANCE_WITH_LIST: &str =
    "{\n  \"cssTheme\": \"Catppuccin\",\n  \"enabledCssSnippets\": [\n    \"wide\"\n  ]\n}";

/// Two vaults registered the way Obsidian does it.
fn vaults(home: &Home) {
    let a = home.path("Notes/Observatory");
    let b = home.path("Notes/Grail");
    home.write("Notes/Observatory/.obsidian/appearance.json", APPEARANCE);
    home.write(
        "Notes/Grail/.obsidian/appearance.json",
        APPEARANCE_WITH_LIST,
    );
    home.write(
        "Library/Application Support/obsidian/obsidian.json",
        &format!(
            "{{\"vaults\":{{\"1\":{{\"path\":\"{}\",\"ts\":1,\"open\":true}},\"2\":{{\"path\":\"{}\",\"ts\":2}}}}}}",
            a.display(),
            b.display()
        ),
    );
}

#[test]
fn obsidian_snippet_in_every_vault() {
    let home = Home::new();
    vaults(&home);
    home.ok(&["set", "tokyo-night"]);
    let out = home.ok(&["init"]);
    assert!(
        out.contains("snippet written to 2 vault(s) and switched on"),
        "{out}"
    );

    for vault in ["Observatory", "Grail"] {
        let css = home.read(&format!("Notes/{vault}/.obsidian/snippets/palette.css"));
        assert!(css.starts_with("/* Palette: tokyo-night"), "{css}");
        assert!(css.contains("--background-primary: #1a1b26 !important;"));
        assert!(css.contains("--accent-h: 221 !important;"));
        // Heading colours on the headings themselves, and Catppuccin's palette.
        assert!(css.contains(".HyperMD-header-1"), "{css}");
        assert!(css.contains("--ctp-base: 26, 27, 38 !important;"), "{css}");
        let appearance = home.read(&format!("Notes/{vault}/.obsidian/appearance.json"));
        assert!(appearance.contains("\"palette\""), "{appearance}");
    }
    assert!(
        home.read("Notes/Grail/.obsidian/appearance.json")
            .contains("[\"wide\", \"palette\"]")
    );

    // Each vault's own list comes back exactly; the snippet file goes.
    home.ok(&["init", "--remove"]);
    assert_eq!(
        home.read("Notes/Observatory/.obsidian/appearance.json"),
        APPEARANCE
    );
    assert_eq!(
        home.read("Notes/Grail/.obsidian/appearance.json"),
        APPEARANCE_WITH_LIST
    );
    assert!(
        !home
            .path("Notes/Grail/.obsidian/snippets/palette.css")
            .exists()
    );
}

#[test]
fn obsidian_never_overwrites_a_users_own_snippet() {
    let home = Home::new();
    vaults(&home);
    home.write("Notes/Grail/.obsidian/snippets/palette.css", "/* mine */\n");
    home.ok(&["set", "tokyo-night"]);
    let out = home.ok(&["init"]);
    assert!(out.contains("is your own snippet"), "{out}");
    assert_eq!(
        home.read("Notes/Grail/.obsidian/snippets/palette.css"),
        "/* mine */\n"
    );
    assert_eq!(
        home.read("Notes/Grail/.obsidian/appearance.json"),
        APPEARANCE_WITH_LIST
    );
}

/// One Mac: a theme, a wallpaper, settings, configs, wired.
fn source_mac() -> Home {
    let home = Home::new();
    home.write(".tmux.conf", "set -g mouse on\n");
    home.write(".aerospace.toml", "gaps = 8\n");
    home.write(
        ".config/sketchybar/sketchybarrc",
        "#!/bin/bash\nPILL=0xf0292e42\n",
    );
    home.write(".config/sketchybar/plugins/clock.sh", "date\n");
    home.write(".zshrc", "export SECRET=hunter2\n");
    let img = home.write("pic.png", "png");
    home.ok(&["set", "nord"]);
    home.ok(&["bg", "add", "--theme", "nord", img.to_str().unwrap()]);
    home.ok(&["apps", "off", "btop"]);
    home.ok(&["init"]);
    home
}

#[test]
fn export_takes_palette_and_configs_but_not_shell_rc() {
    let mac = source_mac();
    let out_dir = mac.path("Export");
    let out = mac.ok(&["export", out_dir.to_str().unwrap()]);
    assert!(
        out.contains("Created") && out.contains("sync.toml"),
        "{out}"
    );

    let exported = |rel: &str| out_dir.join(rel);
    assert!(exported("palette-export.toml").is_file());
    assert!(exported("README.txt").is_file());
    assert!(exported("palette/settings.toml").is_file());
    assert!(exported("palette/themes/nord/backgrounds/pic.png").is_file());
    assert!(exported("configs/.tmux.conf").is_file());
    assert!(exported("configs/.aerospace.toml").is_file());
    assert!(exported("configs/.config/sketchybar/plugins/clock.sh").is_file());
    assert!(
        !exported("configs/.zshrc").exists(),
        "shell rc files hold secrets"
    );
    // Rebuilt on any Mac, so not exported.
    assert!(!exported("palette/generations").exists());
    assert!(!exported("palette/backup").exists());

    let sync = mac.read(".config/palette/sync.toml");
    assert!(
        sync.contains("\".aerospace.toml\"") && sync.contains("# \".zshrc\""),
        "{sync}"
    );

    // Without wallpapers.
    let out_dir2 = mac.path("Export2");
    mac.ok(&["export", out_dir2.to_str().unwrap(), "--no-wallpapers"]);
    assert!(!out_dir2.join("palette/themes/nord/backgrounds").exists());

    // An unrelated non-empty folder is refused; an earlier export is refreshed.
    mac.write("Busy/file.txt", "x");
    assert!(
        mac.fails(&["export", mac.path("Busy").to_str().unwrap()])
            .contains("isn't empty")
    );
    mac.ok(&["export", out_dir.to_str().unwrap()]);
}

#[test]
fn import_sets_up_another_mac() {
    let old = source_mac();
    let out_dir = old.path("Export");
    old.ok(&["export", out_dir.to_str().unwrap()]);

    let new = Home::new();
    new.write(".tmux.conf", "set -g status off\n");
    let dir = out_dir.to_str().unwrap();

    let dry = new.ok(&["import", dir, "--dry-run"]);
    assert!(
        dry.contains("Dry run") && dry.contains("replace  ~/.tmux.conf"),
        "{dry}"
    );
    assert_eq!(new.read(".tmux.conf"), "set -g status off\n");

    let out = new.ok(&["import", dir]);
    assert!(out.contains("backed up to"), "{out}");
    assert!(out.contains("Theme: nord"), "{out}");
    assert_eq!(new.ok(&["current"]).trim(), "nord");
    assert_eq!(new.read(".aerospace.toml"), "gaps = 8\n");
    assert!(
        new.palette_dir()
            .join("themes/nord/backgrounds/pic.png")
            .is_file()
    );
    assert!(
        new.read(".config/palette/settings.toml")
            .contains("btop = false")
    );

    // The old Mac's home folder became this one's in the import lines.
    let tmux = new.read(".tmux.conf");
    let old_home = old.dir.path().to_string_lossy().to_string();
    let new_home = new.dir.path().to_string_lossy().to_string();
    assert!(
        !tmux.contains(&old_home) && tmux.contains(&new_home),
        "{tmux}"
    );
    assert!(new.ok(&["init"]).contains("already wired"));

    // The replaced original is in the backup.
    let backups: Vec<_> = fs::read_dir(new.palette_dir().join("backup"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    assert!(backups.iter().any(|b| {
        fs::read_to_string(b.join(".tmux.conf")).is_ok_and(|t| t == "set -g status off\n")
    }));
}

#[test]
fn import_refuses_what_isnt_an_export() {
    let home = Home::new();
    home.write("Random/x.txt", "x");
    assert!(
        home.fails(&["import", home.path("Random").to_str().unwrap()])
            .contains("isn't a Palette export")
    );
}

/// The owner's case: theme set by an older build (no obsidian.css yet),
/// then Obsidian set up on its own from Settings / `palette apps on`.
#[test]
fn setting_up_a_new_app_after_an_upgrade() {
    let home = Home::new();
    vaults(&home);
    home.ok(&["set", "tokyo-night"]);
    fs::remove_file(home.palette_dir().join("current/obsidian.css")).unwrap();
    let out = home.ok(&["apps", "on", "obsidian"]);
    let dbg = home.ok(&["init", "--dry-run"]);
    assert!(out.contains("wired"), "{out}\n---\n{dbg}");
    assert!(home.palette_dir().join("current/obsidian.css").is_file());
    assert!(
        home.read("Notes/Grail/.obsidian/snippets/palette.css")
            .starts_with("/* Palette:")
    );
    // Filling in a file isn't a theme switch.
    assert!(home.fails(&["undo"]).contains("nothing to undo"));
}
