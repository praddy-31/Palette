# Theme spec

A theme is a folder of plain data. Palette never executes anything inside it.

## Folder layout

```
~/.config/palette/themes/<name>/
  colors.toml      # required
  backgrounds/     # optional: wallpapers (used from Phase 2)
  preview.png      # optional: picker thumbnail (used from Phase 4)
  light.mode       # optional: empty file = light theme (Omarchy convention)
  <output file>    # optional: static override of one target, see below
```

`<name>` is the folder name: lowercase letters, digits and `-` only (`tokyo-night`).

Palette also ships bundled themes inside the binary. A user theme with the same
name as a bundled one wins. A folder for a bundled theme's name *without*
`colors.toml` keeps the bundled colours and only adds wallpapers and overrides
(that is what `palette bg add` creates).

### Wallpapers

Images in `backgrounds/` (`png jpg jpeg heic webp tiff`, regular files only),
used in file-name order. Switching to a theme sets its first wallpaper (or keeps
the current one if it already belongs to the theme). A theme without wallpapers
leaves the desktop alone and says so. `palette bg next` cycles,
`palette bg add <images…> [--theme <name>]` copies images in.

## `colors.toml`

The keys match Omarchy's `colors.toml` exactly, so an Omarchy theme's
`colors.toml` can be dropped in unchanged.

Every value is a string in `#RRGGBB` form (six hex digits, either case). No
`#RGB`, no alpha, no colour names: targets add alpha themselves where they need it.

| Key | Required | Meaning |
| --- | --- | --- |
| `mode` | no | `"dark"` (default) or `"light"` |
| `accent` | yes | Highlight colour: active border, focused workspace, links |
| `selection` | yes | Selected-text / pill background |
| `muted` | yes | Inactive border, disabled text, ANSI bright black |
| `background` | yes | Main background |
| `dark_background` | yes | Slightly darker surface; ANSI black |
| `darker_background` | yes | Darkest surface |
| `lighter_background` | yes | Raised surface (cards, bars) |
| `foreground` | yes | Main text; ANSI white |
| `dark_foreground` | yes | Comments, secondary text |
| `light_foreground` | yes | Slightly emphasised text |
| `bright_foreground` | yes | Strongest text; cursor; ANSI bright white |
| `red` `yellow` `green` `cyan` `blue` `magenta` | yes | Normal colours |
| `orange` | no | Defaults to `yellow` (some Omarchy themes leave it out) |
| `brown` | no | Defaults to `orange` |
| `bright_red` `bright_yellow` `bright_green` `bright_cyan` `bright_blue` `bright_magenta` | yes | Bright colours |

Unknown keys produce a warning, not an error, so newer Omarchy themes still load.
`mode = "light"` in `colors.toml` and a `light.mode` file mean the same thing.

### Derived ANSI palette

Terminals need the 16 ANSI colours. Palette derives them so theme authors don't
repeat themselves:

| Index | Name | Key |
| --- | --- | --- |
| 0 | black | `dark_background` |
| 1 | red | `red` |
| 2 | green | `green` |
| 3 | yellow | `yellow` |
| 4 | blue | `blue` |
| 5 | magenta | `magenta` |
| 6 | cyan | `cyan` |
| 7 | white | `foreground` |
| 8 | bright black | `muted` |
| 9-14 | bright red..cyan | `bright_*` |
| 15 | bright white | `bright_foreground` |

## Validation

`palette set` refuses a theme with any missing key or malformed value, and lists
**every** problem at once, with the file path:

```
error: theme `my-theme` is invalid (~/.config/palette/themes/my-theme/colors.toml):
  - missing key `bright_red`
  - `accent` = "#7aa2f" is not #RRGGBB
```

## Static overrides

Each target renders one output file (`ghostty.conf`, `kitty.conf`, ...). If a
theme folder contains a file with that exact name, Palette copies it verbatim
instead of rendering the template. Use this when a theme wants hand-tuned
output for one app.

Not allowed for files that can run programs: `sketchybar.sh`, `borders.sh`,
`shell.sh`, `tmux.conf`, `fzf.opts`, `starship.toml`, `lazygit.yml`,
`neovim.lua` and `obsidian.css` (CSS can load from the web). Allowed: `ghostty.conf`, `kitty.conf`, `alacritty.toml`,
`vscodium.json`, `btop.theme`. A theme containing one is
refused with an error. Override files must be regular files, not symlinks.

## Template context

Templates (`src/targets/<name>/template.*`, minijinja syntax) see:

- every `colors.toml` key, as `#rrggbb` strings (lowercased)
- `ansi`: list of the 16 derived colours, `ansi[0]`..`ansi[15]`
- Derived by how colours look, not their names (`src/colour.rs`, OKLCH + WCAG contrast):
  - `mood`: `colourful`, `muted` or `monochrome`
  - `family`: six readable colours that belong together (accent first, then its nearest
    distinct hues; shades fill in for muted and monochrome themes). Obsidian headings H1–H6
  - `pop`, `pop2`: readable colours that stand apart from the family (italic, bold, highlight)
  - `primary`, `secondary`, `tertiary`: `family[0..3]`
- `mode`: `"dark"` or `"light"`; `theme`: the theme name

Filters:

- `{{ accent | bare }}` → `7aa2f7` (no `#`)
- `{{ accent | argb }}` → `0xff7aa2f7` (SketchyBar / JankyBorders format)
- `{{ selection | argb("f0") }}` → `0xf0292e42` (custom alpha, two hex digits)
- `{{ accent | hue }}`, `| sat`, `| light` → `221`, `89`, `72` (HSL numbers, for Obsidian)
- `{{ background | rgb }}` → `26, 27, 38` (for themes that use `rgb(var(--x))`)
- `{{ primary | ink(darker_background, bright_foreground) }}`: whichever of the two reads better on it

## Output

`palette set` renders into a new generation folder, then atomically repoints
`~/.config/palette/current` at it:

```
~/.config/palette/
  current -> generations/1760190000000-tokyo-night
  generations/<millis>-<theme>/   # last 5 kept
    theme                         # the theme name
    sketchybar.sh  borders.sh  ghostty.conf  kitty.conf  alacritty.toml
    tmux.conf  vscodium.json  neovim.lua  btop.theme  shell.sh
    starship.toml  fzf.opts  lazygit.yml  sketchybar-item.sh  obsidian.css
    habit-tracker.json
  themes/<name>/                  # user themes (or wallpapers for bundled ones)
  backup/<UTC time>/              # originals saved by `palette init` (last 5 kept)
  wiring/<target>.previous        # settings `init` changed (btop, VSCodium), for --remove
  vscodium/palette-theme.vsix     # the local theme extension `init` installs
  settings.toml                   # hotkey, Dock, [apps] switched off
  sync.toml                       # what `palette export` takes along
  cache/thumbs/                   # picker thumbnails, safe to delete
  wallpaper                       # path of the wallpaper Palette last set
```

User configs point at `~/.config/palette/current/<file>`, which always exists
and is never half-written.

## Starship colour names

Palette's `starship.toml` is a copy of yours with `palette = "palette"` and a
`[palettes.palette]` table holding every `colors.toml` key (plus `purple` =
`magenta`). Write colours as those names to follow the theme, e.g.
`style = "bg:blue fg:background"`. Hex colours stay as written.

