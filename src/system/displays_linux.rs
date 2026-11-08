//! Displays being connected: the Linux stand-in for `displays.rs`. GNOME, KDE
//! and Noctalia keep one wallpaper setting for every display, so a display
//! plugged in later already shows the current picture; nothing to watch.

/// Never called back on Linux.
pub fn on_added(_on_added: impl Fn() + Send + Sync + 'static) {}
