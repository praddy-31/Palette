# Contributing

Thanks for helping. Palette is a Rust CLI (`palette`) plus a Tauri 2 menu bar app in `app/`.
Both only call the core library in `src/`.

## Build and test

You need an Apple silicon Mac with Rust (rustup), Xcode Command Line Tools
(`xcode-select --install`) and, for the app, the Tauri CLI:
`cargo install tauri-cli --version '^2'`. No Node: the app's UI is plain HTML/JS.

```sh
cargo test                                             # unit and CLI tests, all in fake homes
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings  # core and app
cargo build --release                                  # target/release/palette
sh app/sidecar.sh                                      # once after a fresh clone (app needs a CLI copy)
cd app && cargo tauri build                            # Palette.app and .dmg
```

Run `cargo fmt`, clippy and `cargo test` before you commit.

## Use a fake home

Never test against your real configs. Point Palette at a throwaway folder:

```sh
PALETTE_HOME=./tmp/home cargo run -- set tokyo-night --dry-run
PALETTE_HOME=./tmp/home cargo run -- doctor
```

Tests use a temporary fake home from `tests/common/mod.rs`.

## Ground rules

- Palette writes only to `~/.config/palette/`. User configs get one import line, only via
  `palette init`, which backs up first. Never overwrite a user's config.
- Themes are data. Never execute anything from a theme folder.
- Every feature has a library function, a CLI command and an app control. The CLI and the
  app contain no logic of their own.
- One app is one module under `src/targets/<name>/`. Adding one must not touch the others.
- Keep the binary small. Justify any new dependency in your pull request.
- No telemetry. No network except an explicit `palette install` or `palette update`.

## Commits

Small commits with a clear imperative message, for example `Add colors.toml parser`.
Commit before a large refactor so it can be rolled back. Explain decisions (a library
choice, a format change) in the pull request description.

## Guides

- [Adding support for another app](docs/ADDING_A_TARGET.md)
- [Making a theme](docs/THEME_AUTHORING.md)
- [Architecture](docs/ARCHITECTURE.md) and [theme file reference](docs/THEME_SPEC.md)

## Licence

By contributing you agree your work is released under the MIT licence.
