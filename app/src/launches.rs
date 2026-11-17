//! Apps starting, heard from macOS (NSWorkspace's "did launch application" notification).
//! Orca and Rex take a new theme only while they run, so a switch made while one was closed
//! leaves it on the older theme; when it starts, `palette::catch_up` sends it the current one.
//!
//! An event from macOS rather than a timer, like `displays.rs`: an idle Palette stays at
//! zero wake-ups. Rust term: Objective-C calls through the `objc2` crates (`Retained` is an
//! owned Objective-C object, `RcBlock` a closure macOS can call).

use std::time::Duration;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSRunningApplication, NSWorkspace, NSWorkspaceApplicationKey,
    NSWorkspaceDidLaunchApplicationNotification,
};
use objc2_foundation::NSNotification;
use palette::Outcome;

/// How long a starting app gets to answer its CLI: 8 tries, 4 s apart (Orca takes ~3 s).
const TRIES: u32 = 8;
const WAIT: Duration = Duration::from_secs(4);

/// Start listening. Call once, from the main thread (macOS posts these on it).
pub(crate) fn watch() {
    let block = RcBlock::new(|note: std::ptr::NonNull<NSNotification>| {
        // SAFETY: macOS hands the block a valid notification for the duration of the call.
        let note = unsafe { note.as_ref() };
        let Some(name) = launched_name(note) else {
            return;
        };
        if !palette::TOLD_WHILE_RUNNING.iter().any(|(a, _)| *a == name) {
            return;
        }
        // Off the main thread: catching up waits for the app to answer.
        std::thread::spawn(move || {
            let Ok(paths) = crate::paths() else { return };
            match palette::catch_up(&paths, &name, TRIES, WAIT) {
                Some(Outcome::Failed(e)) => {
                    eprintln!("palette: {name} started, theme not sent: {e}")
                }
                Some(Outcome::Done(note)) => eprintln!("palette: {name} started: {note}"),
                _ => {}
            }
        });
    });
    // SAFETY: the notification name is a constant AppKit exports; the observer token is kept
    // for the life of the app (leaked on purpose: the watch never stops).
    unsafe {
        let center = NSWorkspace::sharedWorkspace().notificationCenter();
        let token = center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceDidLaunchApplicationNotification),
            None,
            None,
            &block,
        );
        std::mem::forget(token);
    }
}

/// The started app's name as macOS shows it ("Orca", "Rex Beta").
fn launched_name(note: &NSNotification) -> Option<String> {
    let info = note.userInfo()?;
    // SAFETY: AppKit documents this key as holding the NSRunningApplication that started.
    let app = unsafe { info.objectForKey(NSWorkspaceApplicationKey) }?;
    let app: Retained<NSRunningApplication> = app.downcast().ok()?;
    Some(app.localizedName()?.to_string())
}
