//! Shared test helper: a temporary fake home and a way to run `palette` in it.
#![allow(dead_code)] // each test file uses a different subset

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

pub struct Home {
    pub dir: TempDir,
}

impl Home {
    pub fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    pub fn palette_dir(&self) -> PathBuf {
        self.dir.path().join(".config/palette")
    }

    pub fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_palette"))
            .args(args)
            .env("PALETTE_HOME", self.dir.path())
            .env_remove("XDG_CONFIG_HOME")
            .output()
            .unwrap()
    }

    /// Run, assert success, return stdout.
    pub fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    /// Run, assert failure, return stderr.
    pub fn fails(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(!out.status.success(), "{args:?} unexpectedly succeeded");
        String::from_utf8(out.stderr).unwrap()
    }

    pub fn add_theme(&self, name: &str, colors: &str) -> PathBuf {
        let dir = self.palette_dir().join("themes").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("colors.toml"), colors).unwrap();
        dir
    }

    pub fn current_file(&self, name: &str) -> String {
        fs::read_to_string(self.palette_dir().join("current").join(name)).unwrap()
    }
}

pub fn bundled(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("themes")
        .join(name)
        .join("colors.toml");
    fs::read_to_string(path).unwrap()
}

impl Home {
    pub fn path(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    pub fn write(&self, rel: &str, text: &str) -> PathBuf {
        let path = self.path(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        path
    }

    pub fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.path(rel)).unwrap()
    }
}
