//! Displays being connected, heard from macOS's display system (CoreGraphics)
//! directly. macOS keeps a wallpaper per display, so a display that was
//! unplugged during a switch comes back with the old theme's picture; the app
//! sets the wallpaper again when one appears.
//!
//! An event from macOS rather than a timer, so an idle Palette stays at zero
//! wake-ups. Plain C calls into a framework every Mac app already loads.

use std::ffi::c_void;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

type Callback = extern "C" fn(display: u32, flags: u32, user_info: *mut c_void);

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGDisplayRegisterReconfigurationCallback(callback: Callback, user_info: *mut c_void) -> i32;
}

const BEGIN: u32 = 1 << 0; // kCGDisplayBeginConfigurationFlag: "about to change", sent first
const ADDED: u32 = 1 << 4; // kCGDisplayAddFlag: plugged in
const ENABLED: u32 = 1 << 8; // kCGDisplayEnabledFlag: switched back on (lid opened, mirroring off)

/// Quiet time after the last event before `on_added` runs: one plug-in sends
/// several events, and a brand-new display needs a moment before System
/// Events can set its picture.
const SETTLE: Duration = Duration::from_secs(2);

static HANDLER: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();
static EVENTS: AtomicU64 = AtomicU64::new(0);

/// Call `on_added` (on its own thread) about [`SETTLE`] after a display is
/// connected or switched on. Call once, from the main thread: macOS delivers
/// the events through that thread's run loop, which only an app has.
pub fn on_added(on_added: impl Fn() + Send + Sync + 'static) {
    if HANDLER.set(Box::new(on_added)).is_err() {
        return; // already watching
    }
    // SAFETY: `changed` is a plain function that lives for the whole program
    // and never uses `user_info`.
    unsafe { CGDisplayRegisterReconfigurationCallback(changed, std::ptr::null_mut()) };
}

extern "C" fn changed(_display: u32, flags: u32, _user_info: *mut c_void) {
    if flags & BEGIN != 0 || flags & (ADDED | ENABLED) == 0 {
        return;
    }
    let this = EVENTS.fetch_add(1, Ordering::SeqCst) + 1;
    std::thread::spawn(move || {
        std::thread::sleep(SETTLE);
        if EVENTS.load(Ordering::SeqCst) == this
            && let Some(handler) = HANDLER.get()
        {
            handler();
        }
    });
}
