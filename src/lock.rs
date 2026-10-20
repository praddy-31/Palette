//! One change at a time: an exclusive lock on `~/.config/palette/.lock`
//! around everything that changes Palette's state (switch, undo, refresh,
//! init, apps on/off, import, wallpapers).
//!
//! Scrolling the SketchyBar pill fires `palette next` several times a second,
//! and the app and a terminal can switch at the same moment. Without the lock
//! two switches interleave: the wallpaper from one, light/dark from another.
//! With it, the second caller waits for the first to finish (up to
//! [`TIMEOUT`], then a clear error).
//!
//! The lock is an advisory `flock(2)` lock (`File::lock` in Rust's std): the
//! kernel drops it when the process exits, even after a crash, so a stale
//! `.lock` file never blocks anything. It is re-entrant within a thread, so a
//! locked operation can call another (import → switch → init).

use std::cell::Cell;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::paths::Paths;

/// How long a second switch waits for the first before giving up. A switch
/// (render, reload apps, wallpaper) takes well under a second.
pub const TIMEOUT: Duration = Duration::from_secs(10);

thread_local! {
    /// Locks held by this thread (re-entry only counts, it doesn't lock again).
    static DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// Held while a change runs; dropping it releases the lock.
#[must_use = "the lock is released when this is dropped"]
pub struct Guard {
    /// `None` when this thread already held the lock (re-entry).
    file: Option<File>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
        if let Some(file) = self.file.take() {
            let _ = file.unlock();
        }
    }
}

/// Wait (up to [`TIMEOUT`]) for the lock, then hold it until the guard drops.
pub fn exclusive(paths: &Paths) -> Result<Guard> {
    exclusive_for(paths, TIMEOUT)
}

pub fn exclusive_for(paths: &Paths, timeout: Duration) -> Result<Guard> {
    if DEPTH.with(|d| d.get()) > 0 {
        DEPTH.with(|d| d.set(d.get() + 1));
        return Ok(Guard { file: None });
    }
    fs::create_dir_all(paths.root())
        .with_context(|| format!("creating {}", paths.root().display()))?;
    let path = paths.root().join(".lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .with_context(|| format!("opening {}", path.display()))?;
    let start = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => break,
            Err(TryLockError::WouldBlock) if start.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(TryLockError::WouldBlock) => bail!(
                "another Palette change is still running (waited {} s); try again in a moment",
                timeout.as_secs()
            ),
            Err(TryLockError::Error(e)) => {
                return Err(e).with_context(|| format!("locking {}", path.display()));
            }
        }
    }
    DEPTH.with(|d| d.set(d.get() + 1));
    Ok(Guard { file: Some(file) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_holder_waits_then_times_out() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        let held = exclusive(&paths).unwrap();
        // Re-entry on the same thread is fine.
        drop(exclusive(&paths).unwrap());
        let other = paths.clone();
        let err = std::thread::spawn(move || {
            exclusive_for(&other, Duration::from_millis(100))
                .err()
                .map(|e| e.to_string())
        })
        .join()
        .unwrap();
        assert!(err.unwrap().contains("still running"));
        drop(held);
        let other = paths.clone();
        let ok =
            std::thread::spawn(move || exclusive_for(&other, Duration::from_millis(100)).is_ok())
                .join()
                .unwrap();
        assert!(ok);
    }
}
