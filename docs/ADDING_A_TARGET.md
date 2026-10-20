# Adding a target

A target is one app Palette themes. This guide is for contributors. Read
[ARCHITECTURE.md](ARCHITECTURE.md) first if you want the big picture.

## The rule

One target is one module: `src/targets/<name>/`. Adding an app must not touch any other
target. The only shared edits are one `mod <name>;` line and one `<name>::TARGET,` line in
`src/targets/mod.rs`.

## What a target is

A target (struct `Target` in `src/targets/mod.rs`) says:

| Field | Meaning |
| --- | --- |
| `name` | Short name, used by `palette apps on/off <name>` and in Settings |
| `output` | The file it writes into `~/.config/palette/current/`: `file`, `template`, `overridable`, optional `finish` |
| `wiring` | How `palette init` points the app at that file (see below) |
| `installed` | Is the app on this Mac? Used for reports only, never to decide what to write |
| `reload` | Make the running app pick up the new file. Only called when wired |
| `unload` | Optional. After `init --remove`, bring the app back to the user's own config. Default: reload |
| `env` | Shell lines this target adds to `shell.sh` (for tools configured by environment variables) |
| `doctor` | Extra `palette doctor` findings, as `Note::ok(..)` or `Note::fix(..)` |

Use `..BASE` for the parts you do not need. The defaults are: no output, `Wiring::Shell`,
"not installed", "nothing to reload", no env lines, no doctor notes.

## Files in the module

```
src/targets/<name>/
  mod.rs          the TARGET constant and its small functions
  template.<ext>  the minijinja template for the output file
```

Look at these for patterns:

- `habit_tracker`: the smallest. Output only, no wiring, the app reads the file itself.
- `fzf`: output plus an env line (`FZF_DEFAULT_OPTS_FILE`) and a version check in `doctor`.
- `ghostty`: an import line, config discovery for two locations, live reload by signal, a
  version check in `doctor`.
- `btop`: a one-setting change instead of an import line.

## The output file name

Pick the file name (`ghostty.conf`) and use it as `Output.file`. Then decide `overridable`:

- `true` only if the file is pure data (colours). A theme folder may then replace it with a
  hand-tuned copy.
- `false` if the file can run anything: shell, Lua, commands, `execute()` binds, CSS that
  can load from the web. Themes are data, and this keeps them so. A theme containing a
  non-overridable file is refused.

Use `finish: Some(fn)` only if the output must be built on the user's own file (Starship
merges your `starship.toml`).

## Template variables and filters

Templates are minijinja, in `template.<ext>`, included with `include_str!`. They are
checked strictly: a typo such as `{{ accnet }}` is an error, not an empty string. The
context is built in `src/render.rs`.

Variables:

- every `colors.toml` key as a lowercase `#rrggbb` string: `accent`, `selection`, `muted`,
  `background`, `dark_background`, `darker_background`, `lighter_background`, `foreground`,
  `dark_foreground`, `light_foreground`, `bright_foreground`, `red`, `yellow`, `green`,
  `cyan`, `blue`, `magenta`, `orange`, `brown`, `bright_red` and the other `bright_*`
- `ansi`: the 16 derived terminal colours, `ansi[0]` to `ansi[15]`
- `mood`: `colourful`, `muted` or `monochrome`
- `family`: six readable colours that belong together, for heading-like uses
- `pop`, `pop2`: readable colours that stand apart from the family
- `primary`, `secondary`, `tertiary`: `family[0]` to `family[2]`
- `mode`: `"dark"` or `"light"`
- `theme`: the theme name

Filters:

| Filter | Example result |
| --- | --- |
| `bare` | `{{ accent \| bare }}` gives `7aa2f7` |
| `argb` | `{{ accent \| argb }}` gives `0xff7aa2f7`; `{{ selection \| argb("f0") }}` sets the alpha |
| `hue`, `sat`, `light` | HSL numbers, for example `221`, `89`, `72` |
| `rgb` | `{{ background \| rgb }}` gives `26, 27, 38` |
| `ink` | `{{ primary \| ink(darker_background, bright_foreground) }}` gives whichever reads better on `primary` |

For colour choices, prefer `family`, `pop` and `ink` to hard-coding names such as
"red for errors": they are picked by how colours look (`src/colour.rs`), so odd themes stay
readable. Coloured text should keep at least 3:1 contrast on `background`.

## Wiring

`wiring` tells `palette init` how to connect the app to `~/.config/palette/current/<file>`.
The code is in `src/wiring.rs`. There are three kinds.

**`Wiring::Import(Import)`**: add one marker comment and one import line to the app's
config. Build it with `Import::new(configs, line, insert)`:

- `configs(paths)` returns the config files to wire. Use `wiring::first_config(&[..])` to
  pick the first that exists. Always build paths from `paths.home()` or
  `paths.config_home()`, never from a hard-coded `~`, so fake homes work.
- `line(paths, generated)` returns the text of the import line, for example Ghostty's
  `config-file = ?/path/ghostty.conf`. Make it harmless if Palette is gone (a `?` or
  `[ -f ... ] &&` guard), as `wiring::source_line` does.
- `insert` places the block in the file. `wiring::at_end` is right when later lines should
  win. `wiring::after_colours` and the SketchyBar item's own function show other placements.
- Set `comment: "--"` (or another prefix) for non-`#` languages, and `create: true` if the
  app has no config yet and `init` should create it. Do this with struct update syntax:
  `Import { comment: "--", ..Import::new(..) }` (see Neovim).

The marker line is `Added by palette init (undo: palette init --remove)`. `init --remove`
finds the block by it and takes it out again.

**`Wiring::Setting(Setting)`**: change one setting to point at Palette, and remember the old
value in `~/.config/palette/wiring/<target>.previous` so `--remove` can restore it. You
supply `configs`, `per_file`, `value`, `read`, `write` and an optional `prepare` step (btop,
VSCodium and Obsidian use this).

**`Wiring::Shell`**: nothing to wire in the app. It is configured through environment
variables (the target's `env` function adds them to `shell.sh`), or it reads the generated
file itself. It counts as wired when the `shell` target is, or has nothing to wire at all.

Whatever you choose, these hold:

1. `init` backs up every config it will change to `~/.config/palette/backup/<time>/` first.
2. It never rewrites anything but its own block or one setting.
3. `--remove` and `--dry-run` must work, and wiring twice must change nothing the second time.

## Reload and doctor

`reload` returns an `Outcome`: `Done`, `Unchanged` (for apps that reread the file on their
own), `Skipped` (not running) or `Failed`. Run programs only through `src/system.rs`
(`system::run`, `is_running`, `signal`, `find_program`). It is the only place that touches
macOS processes. Check an app's version before sending a signal that older versions treat
as fatal.

`doctor` returns extra notes, such as a minimum version. The common "installed" and
"wired" findings come for free.

## Testing

Never test against real configs. `tests/common/mod.rs` provides `Home`: a temporary fake
home, with `run`, `ok`, `fails`, `add_theme`, `current_file`, `write`, `read` and `path`.
It runs the real binary with `PALETTE_HOME` set and `XDG_CONFIG_HOME` removed.

Copy `tests/habit_tracker.rs` as a model. It checks that every bundled theme renders the
file with valid values, and that a theme cannot replace a non-overridable file. For a
wired target, also test, in a fake home with a fake config:

- `palette init` adds the marker and line once, and a second `init` changes nothing
- `palette init --remove` restores the config byte for byte
- a backup exists after `init`

By hand:

```sh
PALETTE_HOME=./tmp/home cargo run -- set tokyo-night
PALETTE_HOME=./tmp/home cargo run -- init --dry-run
PALETTE_HOME=./tmp/home cargo run -- doctor
```

## Checklist

- [ ] `src/targets/<name>/mod.rs` and `template.<ext>`, with `mod` and `ALL` entries in
      `src/targets/mod.rs`
- [ ] `overridable` chosen on purpose (false if the file can run code)
- [ ] Wiring, `reload` and (if needed) `unload` and `doctor`
- [ ] Paths come from `Paths`, so fake homes work
- [ ] A test in `tests/<name>.rs` like `tests/habit_tracker.rs`
- [ ] A row in `docs/THEME_SPEC.md`: the output file in the generated-files list, and in the
      allowed or not-allowed override list
- [ ] A note in the pull request on any choice worth explaining (and a justification for any new dependency)
- [ ] The app's name and group in the Settings list in `app/ui/app.js` (`LABELS` and `GROUPS`)
- [ ] `docs/ARCHITECTURE.md` updated, and the README's list of what Palette themes
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test`
