# Making a theme

A Palette theme is a folder of plain data. Palette never runs anything inside it.
This guide is for people writing a theme. The exact file reference is
[THEME_SPEC.md](THEME_SPEC.md).

## The smallest theme

```
~/.config/palette/themes/my-theme/
  colors.toml
```

The folder name is the theme name: lowercase letters, digits and `-` only. Then:

```sh
palette list              # my-theme should appear
palette set my-theme
```

If a key is missing or malformed, `palette set` lists every problem at once with the file
path and changes nothing.

Try it in a fake home first if you like:

```sh
PALETTE_HOME=./tmp/home palette set my-theme
```

## Folder layout

```
my-theme/
  colors.toml      required
  light.mode       optional, empty file: marks a light theme
  backgrounds/     optional: wallpapers
  preview.png      optional: picker thumbnail
  <target file>    optional: static override of one app's file
```

## `colors.toml`

Every value is a string in `#RRGGBB` form: six hex digits, either case. No `#RGB`, no alpha,
no colour names.

| Key | Required | Used for |
| --- | --- | --- |
| `mode` | no | `"dark"` (default) or `"light"`. Same as a `light.mode` file |
| `accent` | yes | The highlight: active window border, focused workspace, links, cursor accents |
| `selection` | yes | Selected-text and pill background |
| `muted` | yes | Inactive borders, disabled text, ANSI bright black |
| `background` | yes | Main background of terminals, editors and the bar |
| `dark_background` | yes | A slightly darker surface; ANSI black |
| `darker_background` | yes | The darkest surface |
| `lighter_background` | yes | Raised surfaces such as cards and bars |
| `foreground` | yes | Main text; ANSI white |
| `dark_foreground` | yes | Comments and secondary text |
| `light_foreground` | yes | Slightly emphasised text |
| `bright_foreground` | yes | Strongest text; cursor; ANSI bright white |
| `red` `yellow` `green` `cyan` `blue` `magenta` | yes | The six normal colours (ANSI 1 to 6) |
| `orange` | no | Defaults to `yellow` |
| `brown` | no | Defaults to `orange` |
| `bright_red` `bright_yellow` `bright_green` `bright_cyan` `bright_blue` `bright_magenta` | yes | The bright colours (ANSI 9 to 14) |

Unknown keys give a warning, not an error.

The 16 terminal colours are derived, so you never write them: black is `dark_background`,
bright black is `muted`, white is `foreground`, bright white is `bright_foreground`, and the
rest are the colour keys above.

A short example:

```toml
mode = "dark"

accent = "#7aa2f7"
selection = "#283457"
muted = "#565f89"

background = "#1a1b26"
dark_background = "#16161e"
darker_background = "#101014"
lighter_background = "#24283b"

foreground = "#a9b1d6"
dark_foreground = "#565f89"
light_foreground = "#c0caf5"
bright_foreground = "#c0caf5"

red = "#f7768e"
yellow = "#e0af68"
green = "#9ece6a"
cyan = "#7dcfff"
blue = "#7aa2f7"
magenta = "#bb9af7"

bright_red = "#ff7a93"
bright_yellow = "#ff9e64"
bright_green = "#b9f27c"
bright_cyan = "#0db9d7"
bright_blue = "#7da6ff"
bright_magenta = "#bb9af7"
```

## Light themes

Set `mode = "light"` in `colors.toml`, or add an empty `light.mode` file, or both. Palette
then switches macOS to light mode with the theme.

## Wallpapers

Put images in `backgrounds/` (`png jpg jpeg heic webp tiff`; regular files only, not
symlinks). They are used in file-name order, so prefix names with `01-`, `02-` if order
matters. Switching to the theme sets the first one. A theme without wallpapers leaves the
desktop alone.

You can also add them from the command line:

```sh
palette bg add --theme my-theme ~/Pictures/one.jpg ~/Pictures/two.png
palette bg default --theme my-theme two.png    # start on this one
```

## `preview.png`

The picker thumbnail. Any size works: Palette makes a 480 px copy and caches it. Without a
`preview.png` the picker uses the first wallpaper, and without that it draws the colours.

## Static per-target overrides

For hand-tuned output of one app, put a file with the target's exact output name in the
theme folder. Palette copies it as it is instead of rendering its template.

Allowed: `ghostty.conf`, `kitty.conf`, `alacritty.toml`, `vscodium.json`, `btop.theme`.

Not allowed, because they can run programs or load from the web: `sketchybar.sh`,
`borders.sh`, `shell.sh`, `tmux.conf`, `fzf.opts`, `starship.toml`, `lazygit.yml`,
`neovim.lua`, `obsidian.css`. A theme containing one is refused with an error. Override
files must be regular files, not symlinks.

Installing a theme from a git repo ignores override files entirely (see below).

## Starting points

- **From a picture:** `palette new my-theme --from-wallpaper photo.jpg`. Palette finds the
  main colours, makes every text colour readable (3:1) and keeps the picture as the
  theme's wallpaper. Use `--dry-run` to see the colours first, `--light` or `--dark` to
  force the mode. Then edit the saved `colors.toml` to taste.
- **From an existing theme:** copy a bundled theme's `colors.toml` (they are in `themes/`
  in the repo) into a new folder and change it.
- **From Omarchy:** see the next section.

## Omarchy compatibility

The key names match Omarchy's, so an Omarchy `colors.toml` works unchanged. When you run
`palette install <url>`, Palette accepts three layouts:

1. A full `colors.toml` with the keys above.
2. A `colors.toml` with some keys missing, or the `color0` to `color15` plus
   `selection_background` style. Missing keys are derived from the others, and Palette
   says which it filled in. Named keys win where both exist. It needs at least
   `background` and `foreground`, and at least four of the six base colours.
3. An older Omarchy theme with only `alacritty.toml`: the colours are read from it.

A repo with none of these is refused.

## Contrast

Coloured text must be readable on `background`. Aim for at least **3:1** contrast for
`accent`, `red`, `yellow`, `green`, `cyan`, `blue`, `magenta` and their bright versions
(the WCAG level for large text and UI), and about 4.5:1 or more for `foreground`. Palette's
own generated themes (`palette new --from-wallpaper`) enforce 3:1 on every text colour.
Starship, Obsidian headings and similar targets pick their colours by this rule too, and
adjust if a colour is too faint.

Be honest about what `palette doctor` does: it checks that each app is installed, wired and
has its generated file, and that the theme loads. It does not measure contrast. To check
yours, use any WCAG contrast checker on `background` against each colour, or switch to the
theme and look at a terminal with `ls --color`, a `git diff` and your editor.

Quick ways a theme looks off: faint `dark_foreground` comments (below about 3:1), `red` and
`green` too close in lightness for a diff, or `selection` too close to `background` to see.

## Sharing a theme

Put the theme folder at the root of a git repo on GitHub:

```
my-theme/            (repo root)
  colors.toml
  light.mode         (if light)
  preview.png
  backgrounds/
    01-forest.jpg
```

Then anyone can run:

```sh
palette install https://github.com/you/my-theme
palette install https://github.com/you/my-theme --name forest    # under another name
palette update my-theme                                          # fetch the newest
```

GitHub repos are fetched as an archive with macOS's own `curl` and `tar`. Other hosts need
`git`. A local folder also works: `palette install ./my-theme`.

An install is data-only. It takes `colors.toml`, `light.mode`, `preview.png` or
`preview.jpg`, and images from `backgrounds/` (size limits apply, regular files only). Any
config files in the repo are ignored. If your theme needs a hand-tuned `ghostty.conf`, tell
users to copy it into the theme folder themselves.

To list yours in a README, link to this guide and to `palette install <url>`.
