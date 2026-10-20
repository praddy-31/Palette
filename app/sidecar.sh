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
