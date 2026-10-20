# Palette

One click, one hotkey or one command restyles your whole Mac desktop from a single palette file.

<!--
DEMO GIF (owner to record): save as docs/media/demo.gif, about 10 seconds, under 5 MB.
Show: press the hotkey -> the quick switcher overlay opens -> pick a theme ->
the bar, window borders, terminal, editor and wallpaper all change at once.
Use a theme with a wallpaper so the change is obvious.
Then put this line back:
![Palette switching themes](docs/media/demo.gif)
-->

Palette is an open-source, Omarchy-style theme switcher for macOS. It has a menu bar app
and a `palette` command-line tool. Both do the same things. Themes are plain `colors.toml`
files, and Omarchy's community themes work.

## What it themes

Desktop

- SketchyBar (bar colours) and its "theme pill" item
- JankyBorders (window borders)
- Wallpaper, and macOS light or dark mode (switched with the theme)
- Optional, if you let it: the bar's layout, AeroSpace window gaps and shortcuts, border
  width, your Starship prompt, and the Dock and menu bar (see *Desktop pages* below)

Terminals

- Ghostty
- kitty
- Alacritty
- WezTerm
- tmux

Editors

- VSCodium (a local theme extension)
- Neovim
- Obsidian (a colour snippet in each vault, and a ring around the active pane when split)

Command-line tools

- btop
- Starship (merged with your own `starship.toml`)
- fzf
- bat
- lazygit
- a small shell file that loads the fzf, bat, starship and lazygit colours in new terminals

Apps

- Habit Tracker (a colours file the app can follow)

That is 18 targets in the code (`src/targets/`). `palette doctor` shows which ones are
installed on your Mac and which are wired. You can switch any of them off with
`palette apps off <name>` or in Settings. 23 themes are bundled, and
`palette catalog` lists more community themes.

## Install

Apple silicon Macs only for now.

```sh
brew install --cask praddy-31/tap/palette
```

This installs Palette.app and the `palette` command. Or download the DMG from
[GitHub Releases](https://github.com/praddy-31/palette/releases) and drag Palette to
Applications; then get the `palette` command from **Settings → Command-line tool**, which
links `~/.local/bin/palette` to the copy inside the app (`~/.local/bin` must be on your
`PATH`; it is on most setups).

Palette is ad-hoc signed, because there is no paid Apple developer account behind it.
On first open macOS may refuse it. Either:

- open System Settings, then Privacy & Security, and press **Open Anyway**; or
- run `xattr -dr com.apple.quarantine /Applications/Palette.app`.

Building from source needs Rust, Xcode Command Line Tools and the Tauri CLI
(`cargo install tauri-cli --version '^2'`); no Node. See [CONTRIBUTING.md](CONTRIBUTING.md).

## First run

```sh
palette init --dry-run    # show what would change, touch nothing
palette init
```

`palette init` adds one import line to each app's config so the app loads Palette's file.
Before it changes anything it backs up the config to `~/.config/palette/backup/<time>/`
(the last 5 are kept). The app offers the same step on first launch.

To undo it:

```sh
palette init --remove     # takes Palette's lines back out
```

`palette doctor` checks that each app is installed, wired and up to date.

## Everyday use

- **Menu bar picker.** Click the Palette icon for a grid of themes with previews.
- **Hotkey.** A global hotkey opens the quick-switch overlay. Change it in Settings, or
  with `palette hotkey alt+shift+t`. Run `palette hotkey` alone to see the current one.
- **Command line.**

  ```sh
  palette list                  # swatches; * marks the active theme
  palette set tokyo-night       # switch (add --dry-run to preview)
  palette next                  # or: palette prev
  palette undo                  # back to the previous theme; repeat to go further
  palette current
  palette refresh               # render again, after editing starship.toml say
  ```

- **Community themes.**

  ```sh
  palette catalog               # offline list of Omarchy's extra themes
  palette catalog aura          # search it
  palette install aura          # a name from the catalogue
  palette install https://github.com/someone/their-theme
  palette update                # download installed themes again
  palette remove aura
  ```

- **Wallpapers.** Each theme can have several.

  ```sh
  palette bg list
  palette bg next               # or: prev, set <name>, default <name>
  palette bg add --theme nord ~/Pictures/*.jpg
  palette bg recolor photo.jpg --theme nord --apply   # remap a picture to a theme's colours
  palette bg generate waves --apply                   # paint one from the theme's colours
  palette new my-theme --from-wallpaper photo.jpg     # a whole theme from a picture
  ```

  Recolour and generate work offline and need no extra tools. See
  [docs/WALLPAPERS.md](docs/WALLPAPERS.md) for where to find wallpapers.

  More pictures: `palette bg get <link>` downloads one picture, or lists a GitHub repo's
  folders to pick from. `palette bg library` holds pictures for any theme. `generate` has
  six styles: gradient, mesh, waves, topo, geometric and grain.

- **Favourites and recents.** `palette fav add nord` (up to ten), `palette next
  --favourites`, `palette recent`. The app's picker shows both.
- **Day and night.** Pick a light and a dark theme and a city in Settings; Palette switches
  at sunrise and sunset. `palette schedule` shows it, `on`/`off` toggles it, `cities` lists
  the built-in cities (no location services, no network).
- **Readable colours.** `palette check` lists colour pairs that are hard to read;
  `palette check --fix` applies the suggestions. Change any colour yourself with
  `palette colours --set accent=#7aa2f7` or in Settings → Colours; edits sit on top of the
  theme (`edits.toml`) and `palette colours --reset` brings the original back.
- **Desktop pages (optional).** Settings → Bar, Windows, Borders, Prompt and Dock & menu bar
  let Palette manage SketchyBar, AeroSpace, JankyBorders and Starship for you: pills, gaps,
  keyboard shortcuts, prompt blocks. For the bar, windows and borders nothing changes until
  you press **Let Palette manage**; your file is kept as the original, every change is
  backed up first, and **Give it back** restores yours. The Prompt page edits only the
  keys it shows in `starship.toml`; the Dock & menu bar page changes macOS settings, and
  switching an option back undoes it. Same from the terminal: `palette desktop`,
  `palette desktop bar manage`, `palette desktop bar release`.
- **Get the tools.** `palette tools` shows which of SketchyBar, JankyBorders, AeroSpace and
  gowall are installed and prints the Homebrew commands (it never runs Homebrew).
  `palette tools starter <tool>` writes a small themed config for a tool that has none.
- **Move to a new Mac.** `palette export <folder>` then `palette import <folder>`.

## Safety

- **Import, don't overwrite.** Palette writes only inside `~/.config/palette/`. Your own
  configs get one marked import line, added by `palette init` and removed by
  `palette init --remove`. Nothing else in them is rewritten. The one exception is opt-in,
  per tool: after **Let Palette manage** on a desktop page, Palette writes that tool's
  config (sketchybarrc, bordersrc, aerospace.toml key by key), backing it up before every
  change; **Give it back** restores your original. The Prompt page edits the keys it shows
  in your `starship.toml`, the rest of the file stays as written.
- **Atomic switches.** Each switch renders into a new folder and repoints the `current`
  symlink in one step, so an app never reads a half-written file.
- **Themes are data.** Nothing in a theme folder is ever run. Files that could run
  programs (shell scripts, tmux, fzf, starship, lazygit, Lua) cannot be replaced by a
  theme. `palette install` takes only colours, wallpapers and a preview image, never
  config files.
- **Fake homes for testing.** Set `PALETTE_HOME` to any folder and Palette uses it
  instead of your real home:

  ```sh
  PALETTE_HOME=./tmp/home palette set nord
  ```

- **Privacy.** No telemetry, no background service, no network access except when you
  explicitly run `palette install`, `palette update` or `palette bg get` (or press the same
  buttons in the app). The bar's weather pill fetches the weather only if you add it.

## Uninstall

1. If you used the desktop pages: **Give it back** on each one (or
   `palette desktop <tool> release`) to get your own configs back.
2. `palette init --remove` to take Palette's lines out of your configs.
3. Turn off Open at Login in Settings, then quit Palette from the menu bar.
4. `brew uninstall --cask praddy-31/tap/palette`, or drag Palette.app to the Bin and delete
   the `~/.local/bin/palette` link.
5. Optional: delete `~/.config/palette/` (themes, wallpapers, backups, settings).

## Guides

- [Making a theme](docs/THEME_AUTHORING.md)
- [Adding support for another app](docs/ADDING_A_TARGET.md)
- [Contributing](CONTRIBUTING.md)
- [Theme file reference](docs/THEME_SPEC.md)
- [How it is built](docs/ARCHITECTURE.md)
- [Wallpaper sources](docs/WALLPAPERS.md)

## Licence

MIT. See [LICENSE](LICENSE).

## Credits

Most bundled and catalogue colours come from [Omarchy](https://github.com/basecamp/omarchy)
(basecamp/omarchy, MIT), whose `colors.toml` format Palette follows. Thanks to the
authors of the Omarchy community themes.
