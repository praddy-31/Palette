//! `palette list | head`: a reader that hangs up ends palette quietly, never with a panic.

mod common;

use std::io::Read;
use std::process::{Command, Stdio};

#[test]
fn a_closed_pipe_ends_palette_without_a_panic() {
    let home = common::Home::new();
    // A pipe whose reading end is already closed: palette's first write fails, every time
    // (what `head` does once it has its lines, without the timing).
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let mut child = Command::new(env!("CARGO_BIN_EXE_palette"))
        .arg("list")
        .env("PALETTE_HOME", home.dir.path())
        .env("PALETTE_PLATFORM", "macos")
        .env_remove("XDG_CONFIG_HOME")
        .stdout(writer)
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut err = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut err)
        .unwrap();
    child.wait().unwrap();
    assert!(!err.contains("panicked"), "{err}");
}
