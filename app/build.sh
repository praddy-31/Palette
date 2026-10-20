#!/bin/sh
# Build the app (and its DMG), then tell macOS to forget the copy in the build folder, so
# palette:// links (the SketchyBar pill) only ever go to /Applications/Palette.app.
# Otherwise macOS may pick the build copy and start a second Palette for a link.
# Usage: sh app/build.sh   (any extra arguments go to `cargo tauri build`)
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/app"
cargo tauri build "$@"
LSR=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
APP="$ROOT/target/release/bundle/macos/Palette.app"
if [ -x "$LSR" ] && [ -d "$APP" ]; then "$LSR" -u "$APP" 2>/dev/null || true; fi
