#!/bin/sh
# Build a Palette release on this Mac: checks, the .app + .dmg, signature checks,
# dist/Palette-<version>-arm64.dmg, its sha256 and a filled-in Homebrew cask.
#
# It never publishes anything: no git tag, no push, no `gh`. It prints those
# steps at the end for you to run by hand.
#
# Usage: sh scripts/release.sh [--dry-run] [--allow-dirty]
#   --dry-run      run the checks (versions, fmt, clippy, tests, cask template)
#                  but skip the app build, signing checks and dist/ files
#   --allow-dirty  don't require a clean git tree (for testing the script)
#
# Architecture: this builds for the host only (aarch64-apple-darwin on an
# Apple silicon Mac), so the DMG and cask are arm64-only. A universal build
# (`cargo tauri build --target universal-apple-darwin`) is possible but needs
# `rustup target add x86_64-apple-darwin`, a universal sidecar CLI and a cask
# without `depends_on arch: :arm64`; not done yet.
set -eu

dry_run=0
allow_dirty=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) dry_run=1 ;;
    --allow-dirty) allow_dirty=1 ;;
    -h | --help)
      sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "release.sh: unknown option '$arg' (try --help)" >&2
      exit 2
      ;;
  esac
done

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

step() { printf '\n==> %s\n' "$1"; }
fail() {
  printf 'release.sh: %s\n' "$1" >&2
  exit 1
}

# --- 1. Clean tree ----------------------------------------------------------
step "Git tree"
dirty=$(git status --porcelain)
if [ -n "$dirty" ]; then
  if [ "$allow_dirty" -eq 1 ]; then
    echo "Tree is dirty (allowed by --allow-dirty):"
    echo "$dirty"
  else
    echo "$dirty"
    fail "the git tree has uncommitted changes; commit them first (or --allow-dirty to test)"
  fi
else
  echo "Clean."
fi

# --- 2. Versions agree ------------------------------------------------------
step "Versions"
# First `version = "…"` line in each Cargo.toml is the [package] version.
v_core=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
v_app=$(sed -n 's/^version = "\(.*\)"/\1/p' app/Cargo.toml | head -n 1)
v_tauri=$(sed -n 's/^  "version": "\(.*\)",$/\1/p' app/tauri.conf.json | head -n 1)
echo "Cargo.toml            $v_core"
echo "app/Cargo.toml        $v_app"
echo "app/tauri.conf.json   $v_tauri"
[ -n "$v_core" ] && [ -n "$v_app" ] && [ -n "$v_tauri" ] ||
  fail "couldn't read a version from one of the files above"
if [ "$v_core" != "$v_app" ] || [ "$v_core" != "$v_tauri" ]; then
  fail "the three versions don't match; set them all to the release version"
fi
version=$v_core
echo "Releasing $version."

# --- 3. Architecture note ---------------------------------------------------
step "Architecture"
host=$(rustc -vV | sed -n 's/^host: //p')
echo "Host target: $host (the release is built for this target only)."
if [ "$host" != "aarch64-apple-darwin" ]; then
  fail "expected an Apple silicon Mac (aarch64-apple-darwin); the cask says arm64 only"
fi
if ! rustup target list --installed 2>/dev/null | grep -qx 'x86_64-apple-darwin'; then
  echo "Note: no Intel/universal build. To add one later: rustup target add x86_64-apple-darwin,"
  echo "      then cargo tauri build --target universal-apple-darwin (and drop 'depends_on arch')."
fi

# --- 4. Checks --------------------------------------------------------------
# The app crate's build script checks the bundled CLI exists, so clippy and the
# tests need it on a fresh clone.
if [ ! -e "app/binaries/palette-$host" ]; then
  step "Sidecar CLI (needed by the app crate)"
  sh app/sidecar.sh
fi
step "cargo fmt --check"
cargo fmt --all --check
step "cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings
step "cargo test"
cargo test --workspace

# --- 5. Cask template renders -----------------------------------------------
template=packaging/homebrew/palette.rb.template
[ -f "$template" ] || fail "missing $template"
render_cask() { # version sha256 out
  sed -e "s/{{VERSION}}/$1/" -e "s/{{SHA256}}/$2/" "$template" >"$3"
  if grep -q '{{' "$3"; then fail "unfilled placeholder left in $3"; fi
  if command -v ruby >/dev/null 2>&1; then
    ruby -c "$3" >/dev/null || fail "$3 isn't valid Ruby"
  fi
}

if [ "$dry_run" -eq 1 ]; then
  step "Cask template (dry run)"
  tmp=$(mktemp -d)
  render_cask "$version" "0000000000000000000000000000000000000000000000000000000000000000" "$tmp/palette.rb"
  echo "Template renders and parses ($tmp/palette.rb)."
  step "Dry run done"
  echo "Skipped: sidecar + cargo tauri build, codesign checks, DMG check, dist/ files."
  exit 0
fi

# --- 6. Build ---------------------------------------------------------------
cargo tauri --version >/dev/null 2>&1 ||
  fail "cargo tauri isn't installed (cargo install tauri-cli --version '^2')"
step "Sidecar CLI"
sh app/sidecar.sh
step "cargo tauri build"
(cd app && cargo tauri build)

bundle=target/release/bundle
app="$bundle/macos/Palette.app"
dmg="$bundle/dmg/Palette_${version}_aarch64.dmg"
[ -d "$app" ] || fail "no $app after the build"
[ -f "$dmg" ] || fail "no $dmg after the build"
[ -x "$app/Contents/MacOS/palette-cli" ] || fail "the CLI isn't at $app/Contents/MacOS/palette-cli"

# --- 7. Signatures ----------------------------------------------------------
step "codesign (app)"
codesign --verify --deep --strict --verbose=2 "$app"

step "codesign (app inside the DMG)"
mnt=$(mktemp -d)
hdiutil attach -nobrowse -readonly -mountpoint "$mnt" "$dmg" >/dev/null
# Detach even if the check fails.
trap 'hdiutil detach "$mnt" -quiet >/dev/null 2>&1 || true' EXIT
codesign --verify --deep --strict --verbose=2 "$mnt/Palette.app"
hdiutil detach "$mnt" -quiet
trap - EXIT

# --- 8. dist/ ---------------------------------------------------------------
step "dist/"
mkdir -p dist
out="dist/Palette-${version}-arm64.dmg"
cp "$dmg" "$out"
sha=$(shasum -a 256 "$out" | cut -d ' ' -f 1)
render_cask "$version" "$sha" dist/palette.rb
echo "$out"
echo "sha256 $sha"
echo "dist/palette.rb (cask, filled in)"

# --- 9. Next steps (printed, never run) -------------------------------------
cat <<EOF

==> Next steps (run these yourself; this script publishes nothing)

1. Tag the release commit:
     git tag -a v$version -m "Palette $version"
     git push origin main v$version

2. Create the GitHub release and upload the DMG, either:
     gh release create v$version "$out" --repo praddy-31/Palette --title "Palette $version" --notes "…"
   or on the web: https://github.com/praddy-31/Palette/releases/new
     (tag v$version, attach $out; the file name must stay Palette-$version-arm64.dmg)

3. Update the cask in the tap:
     cp dist/palette.rb <path to homebrew-tap>/Casks/palette.rb
     cd <path to homebrew-tap> && git commit -am "palette $version" && git push

4. Check it installs:
     brew update && brew install --cask praddy-31/tap/palette   (or brew upgrade --cask palette)
EOF
