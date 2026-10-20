# Homebrew cask

`palette.rb.template` is the cask for the tap `praddy-31/homebrew-tap`.
`scripts/release.sh` fills in `{{VERSION}}` and `{{SHA256}}` and writes `dist/palette.rb`.

Users install with:

```
brew install --cask praddy-31/tap/palette
```

## Create the tap (once)

1. On GitHub, create a public repo named exactly `homebrew-tap` under `praddy-31`
   (Homebrew maps `praddy-31/tap` to `praddy-31/homebrew-tap`).
2. Clone it and add a `Casks/` folder:
   ```
   git clone https://github.com/praddy-31/homebrew-tap.git
   mkdir homebrew-tap/Casks
   ```

## Each release

1. Set the same version in `Cargo.toml`, `app/Cargo.toml` and `app/tauri.conf.json`; commit.
2. `sh scripts/release.sh` (checks, build, signature checks, `dist/`).
3. Tag, push and create the GitHub release with `dist/Palette-<version>-arm64.dmg`
   (the script prints the exact commands).
4. Copy the cask and push the tap:
   ```
   cp dist/palette.rb ../homebrew-tap/Casks/palette.rb
   cd ../homebrew-tap && git commit -am "palette <version>" && git push
   ```
5. Check: `brew update && brew upgrade --cask palette` (or `brew install --cask …`).
   Optional lint: `brew audit --cask --online praddy-31/tap/palette`.

The URL in the cask must match the uploaded file name, `Palette-<version>-arm64.dmg`,
and the tag `v<version>`.
