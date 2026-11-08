# Wallpapers

Palette doesn't ship or recommend wallpaper collections: find your own, or use the ones
that come with a theme from **Get themes**. Everything below works with pictures you
already have, a link you paste, or wallpapers Palette paints itself. Nothing is
downloaded unless you ask.

Many wallpaper collections online are gathered from many artists and have **no clear
licence**: fine for your own desktop, but check before you share or bundle them.

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
  | `subtle` | The picture's own colours mostly survive, nudged towards the theme |
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
