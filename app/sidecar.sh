#!/bin/sh
# Build the palette CLI and put it where Tauri's `externalBin` expects it
# (binaries/palette-cli-<target triple>), so the .app ships with the CLI inside
# as Contents/MacOS/palette-cli. Not `palette`: tauri-build also copies the
# sidecar next to the app's own build (target/<profile>/<name>), which would
# overwrite the freshly built CLI the tests run with this older copy.
# Run automatically (from the repo root) before `cargo tauri build`/`dev`; run by hand once after a
# fresh clone, because `cargo build` of the app checks the file exists.
set -e
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
cargo build --release -p palette
triple=$(rustc -vV | sed -n 's/^host: //p')
mkdir -p app/binaries
cp target/release/palette "app/binaries/palette-cli-$triple"

# The beat lines' helper (ROADMAP 20): Swift, macOS only, copied into Contents/MacOS by
# tauri.conf.json's bundle.macOS.files. 14.2 is the first macOS with audio process taps; signed
# ad hoc here, so the app's own signature seals a signed binary.
if [ "$(uname)" = Darwin ]; then
  swiftc -O -target "$(uname -m)-apple-macos14.2" app/helpers/palette-beats.swift -o app/binaries/palette-beats
  codesign -s - -f app/binaries/palette-beats
  # Play/pause for the now-playing item, loaded by /usr/bin/perl (app/helpers/palette-media.m).
  clang -dynamiclib -fobjc-arc -framework Foundation -Wno-objc-method-access -mmacosx-version-min=13.0 \
    app/helpers/palette-media.m -o app/binaries/libpalette-media.dylib
  codesign -s - -f app/binaries/libpalette-media.dylib
fi
