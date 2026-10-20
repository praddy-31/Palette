//! Obsidian's active pane: an accent ring and fading the others, both on by
//! default, each switchable with `palette obsidian`.

mod common;

use common::Home;

const RING: &str = ".workspace-leaf.mod-active::after";
const DIM: &str = ".workspace-leaf:not(.mod-active) > .workspace-leaf-content { opacity";

#[test]
fn ring_and_dim_follow_the_setting() {
    let home = Home::new();
    home.ok(&["set", "nord"]);
    let css = home.current_file("obsidian.css");
    assert!(
        css.contains(RING) && css.contains(DIM),
        "both on by default"
    );
    assert!(
        css.contains("border: 2px solid #81a1c1"),
        "ring in nord's accent"
    );
    // Only for a split editor: every rule is scoped to a root with two panes.
    for line in css.lines().filter(|l| l.contains(".workspace-leaf")) {
        assert!(line.starts_with(".workspace-split.mod-root:has("), "{line}");
    }

    let out = home.ok(&["obsidian", "--dim", "off"]);
    assert!(out.contains("ring on, dim the others off"), "{out}");
    let css = home.current_file("obsidian.css");
    assert!(
        css.contains(RING) && !css.contains(DIM),
        "re-rendered straight away"
    );

    home.ok(&["obsidian", "--ring", "off"]);
    let css = home.current_file("obsidian.css");
    assert!(!css.contains(RING) && !css.contains(DIM));
    assert!(
        home.read(".config/palette/settings.toml")
            .contains("[obsidian]\nfocus_ring = false\ndim_inactive = false")
    );
    // Kept across switches.
    home.ok(&["set", "tokyo-night"]);
    assert!(!home.current_file("obsidian.css").contains(RING));

    assert!(
        home.ok(&["obsidian"])
            .contains("ring off, dim the others off")
    );
    assert!(
        home.fails(&["obsidian", "--ring", "maybe"])
            .contains("use on or off")
    );
}
