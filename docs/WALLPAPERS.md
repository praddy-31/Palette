# Wallpaper sources

Collections that match Palette's themes. Checked 2026-10-12 (stars and sizes from GitHub).
Palette can now fetch from these on request: paste a link into **Settings → Wallpapers →
Get from a link**, or run `palette bg get <link>` (see "Wallpaper library and links"
below). Nothing is downloaded unless you ask. You can still copy files by hand with
`palette bg add --theme <name> <images…>`.

Most collections are gathered from many artists and have **no clear licence**:
fine for your own desktop, but don't bundle them into Palette or redistribute them.

## Grab one folder, not the whole repo

Several repos are gigabytes. `palette bg get` already avoids cloning them (see below). By hand, a sparse, shallow clone downloads only the folder you ask for:

```sh
git clone --depth 1 --filter=blob:none --sparse https://github.com/SleepyCatHey/CozyPixels
cd CozyPixels
git sparse-checkout set <folder>        # list folders first: ls, or browse on GitHub
palette bg add --theme catppuccin-mocha <folder>/*
```

## Per theme

| Theme(s) | Repo | Size | Notes |
| --- | --- | --- | --- |
| Tokyo Night | [tokyo-night/wallpapers](https://github.com/tokyo-night/wallpapers) | 22 MB | Made for the theme; MIT. Small, good first pick |
| Tokyo Night | [OldJobobo/omarchy-tokyo-night-bg-addon](https://github.com/OldJobobo/omarchy-tokyo-night-bg-addon) | 53 MB | Curated extras for Omarchy's Tokyo Night |
| Catppuccin | [orangci/walls-catppuccin-mocha](https://github.com/orangci/walls-catppuccin-mocha) | 777 MB | Large, popular (2.8k stars) |
| Catppuccin | [zhichaoh/catppuccin-wallpapers](https://github.com/zhichaoh/catppuccin-wallpapers) | 363 MB | Sorted into folders by style |
| Rosé Pine | [rose-pine/wallpapers](https://github.com/rose-pine/wallpapers) | 320 MB | Official Rosé Pine org |
| Gruvbox | [AngelJumbo/gruvbox-wallpapers](https://github.com/AngelJumbo/gruvbox-wallpapers) | 1.4 GB | Folders by category; actively updated |
| Nord | [linuxdotexe/nordic-wallpapers](https://github.com/linuxdotexe/nordic-wallpapers) | 623 MB | |
| Everforest | [Apeiros-46B/everforest-walls](https://github.com/Apeiros-46B/everforest-walls) | 552 MB | Archived on GitHub; moved to [box.apeiros.xyz](https://box.apeiros.xyz/public/everforest_walls/) |
| Kanagawa | [philikarus/Kanagawa-wallpapers](https://github.com/philikarus/Kanagawa-wallpapers) | 107 MB | Recoloured to the Kanagawa palette |

## Mixed collections (folders by theme or mood)

| Repo | Size | Notes |
| --- | --- | --- |
| [SleepyCatHey/CozyPixels](https://github.com/SleepyCatHey/CozyPixels) | 1.8 GB | The one you mentioned: Catppuccin, Nord, One Dark and more; updated Sept 2026 |
| [yukazakiri/themed-wallpapers](https://github.com/yukazakiri/themed-wallpapers) | 1.7 GB | 1100+ images pre-recoloured to 22 palettes (Gruvbox, Catppuccin, …): closest to "one folder per Palette theme" |
| [dharmx/walls](https://github.com/dharmx/walls) | 3.7 GB | The big one (9k stars); Everforest, Gruvbox, Nord folders among many |
| [D3Ext/aesthetic-wallpapers](https://github.com/D3Ext/aesthetic-wallpapers) | 630 MB | General aesthetic set |
| [makccr/wallpapers](https://github.com/makccr/wallpapers) | 2.7 GB | Mostly 4K and up |
| [michaelScopic/Wallpapers](https://github.com/michaelScopic/Wallpapers) | 700 MB | Has a `tokyo-night` folder |
| [FrenzyExists/wallpapers](https://github.com/FrenzyExists/wallpapers) | 650 MB | Personal picks |
| [mut-ex/wallpapers](https://github.com/mut-ex/wallpapers) | 424 MB | 2560×1440 |

## Wallpaper library and links

The **library** is a shared folder, `~/.config/palette/wallpapers/`, for pictures that
belong to no theme yet. Subfolders are fine (one per source repo, say). Items are named
by their path inside it, such as `CozyPixels/Nord/lake.png`. From the library you can
use a picture now, copy it into a theme (optionally recoloured to that theme), or make a
new theme from it.

```sh
palette bg library                      # list the pictures, the folder and its size
palette bg library add <images…> [--into nord]
palette bg library use <name>           # set it as the wallpaper until the next switch
palette bg library theme <name> [--theme <t>] [--recolor]
palette bg library new-theme <name> <theme-name> [--light|--dark] [--dry-run]
palette bg library remove <name>
palette bg library open                 # show the folder in Finder
```

**Get from a link** (`palette bg get <url>`, or Settings → Wallpapers → Get from a link):

- A picture link (any `https://` image, or a GitHub `blob`/`raw` link): that one file.
- A GitHub repo or folder (`github.com/o/r`, `…/tree/<branch>/<path>`): the pictures are
  *listed* through GitHub's git trees API (one unauthenticated request, so the repo is
  not cloned). Alone, the command lists the repo's folders. Then choose with
  `--folder <path>`, `--pick '*lake*'` (`*` and `?` match) or `--all`.
- `--into <subfolder>` picks the library subfolder (default: the repo's name, or
  `downloads`). `--theme <name>` also adds the pictures to that theme, and `--recolor`
  adds recoloured copies.

Limits and safety: https only; only on your request; at most 40 MB a picture, 200
pictures and 2 GB per download; each file must start with a real picture's header
(png, jpeg, webp, heic, tiff); names are cleaned so nothing lands outside the library.

## Recolour, generate, or start from a picture

Palette has its own wallpaper tools. Recolouring, generating and drafting work offline,
and pictures go through macOS `sips`, so there is no image library.

- **Recolour:** `palette bg recolor <image…> [--theme <name>] [--strength …] [--engine …] [--apply]`
  remaps an image to a theme's colours and adds the copy to the theme's wallpapers. Each
  pixel is blended from its nearest theme colours (judged in OKLab, so "near" means
  "looks near") while keeping some of its own light and shade, so photos stay
  recognisable. Strength (`--strength`, Palette's engine only):

  | Strength | Look |
  | --- | --- |
  | `subtle` | The picture's own colours mostly survive, nudged towards the theme (the original Phase 5d look) |
  | `balanced` (default) | Clearly in the theme's colours, with the picture's detail kept |
  | `strong` | Close to a flat, palette-mapped look, like gowall |

  The copy is named `<picture>-<theme>.<ext>` for the default, with `-subtle` or
  `-strong` added otherwise, so every variant sits side by side for comparing. With
  `--engine gowall` the name ends `-gowall` and strength is ignored.
  Optional: gowall ([Achno/gowall](https://github.com/Achno/gowall), MIT,
  `brew install gowall`, also listed under Settings → Get the tools). Palette runs only
  its offline `gowall convert … --preview false`, and only when you pick it. Other gowall
  features use the network, so they are never called.
- **Generate:** `palette bg generate [style] [--theme <name>] [--seed N] [--apply]` paints
  a 3200x2000 JPEG from the theme's colours. Useful for themes with no wallpapers. Six
  styles, calmest first:

  | Style | Look |
  | --- | --- |
  | `gradient` (default) | Soft blends of the theme's colours |
  | `mesh` | Big soft blobs of the theme's colours, gently warped (aurora style) |
  | `waves` | Layered wave bands |
  | `topo` | Contour lines of a height field in the accent, like a map |
  | `geometric` | Flat minimal shapes: a diagonal band, two circles, a ring and a dot |
  | `grain` | A halftone dot grid with film grain |

  Without `--seed` the file is `palette-<style>.jpg` and is replaced each time. With
  `--seed N` it is `palette-<style>-N.jpg`: the same N gives the same picture, so you can
  keep a variant you like. The app's Generate button picks seeds for you.
- **Theme from a picture:** `palette new <name> --from-wallpaper <image>` (or
  `palette bg library new-theme`) finds the ten main colours (k-means in OKLab). The
  background comes from the darkest large area and the accent from the most vivid
  colour. Terminal colours are *harmonised*: each usual hue (red stays red-ish, green
  green-ish) is turned part of the way towards the picture's overall colour cast, and
  leans to the picture's own nearby colours. A strongly tinted picture shifts them up to
  about 25 degrees, a grey one about 10, so two different pictures give visibly different
  themes. Every text colour is made readable (3:1), and anything missing is filled in.
  `--dry-run` shows the result without saving.
