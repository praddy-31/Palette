# Palette: architecture

A map of how Palette works today (October 2026, after Phase 6 and the 1.1 round). It describes the code
as it is, not the plan. File references are `path:line` so you can jump straight there.
Weak spots and suggested improvements are in section 14.

---

## 1. Summary

Palette turns one theme (a `colors.toml` of about 25 hex colours) into a config file for
each app it themes. It writes them all at once into `~/.config/palette/current/`, then
tells the running apps to reload. Your own configs are never rewritten. `palette init`
adds one marked line to each of them, pointing at Palette's generated file. All the
logic lives in one Rust library crate (`palette`). There are two equal front doors:
- **the CLI:** `src/main.rs`, using `clap`;
- **the Tauri app:** `app/src/main.rs` plus plain HTML/JS in `app/ui/`.

Both only call the library.

```
 theme data                      core library (crate `palette`, src/)                  front doors          your Mac
 ───────────                     ─────────────────────────────────────                 ───────────          ────────
 bundled themes  ─┐              theme.rs   load + validate colours                    CLI                  SketchyBar, borders,
 (themes/*, in    │              colour.rs  how colours look (mood, family)            src/main.rs          Ghostty, kitty, ...
  the binary)     ├──► Theme ──► render.rs  minijinja templates → files    ─┐          (clap)               (reloaded)
 your themes      │              state.rs   generations + atomic swap      ├─ lib.rs ─┤                      macOS: wallpaper,
 ~/.config/       │              wiring.rs  `init`: one line per config     │  switch, │  Tauri app           light/dark
 palette/themes/* ┘              effects.rs reloads, appearance, wallpaper ─┘  init,   │  app/src/main.rs     (osascript)
 community themes ──► install.rs (download, data only) + catalog.rs            undo... │  + app/ui/*.html/js
 wallpapers ────────► library.rs + fetch.rs (links)    schedule.rs, favourites.rs, tools.rs, check.rs
                                 system.rs  the only place that runs programs          │
                                 targets/   one module per app (18)                    └─ both call the same functions
```

---

## 2. Folder and file map

### Core library (`src/`)

| File | Job | Key items |
| --- | --- | --- |
| `lib.rs` | Commands as functions, used by both front doors | `switch` / `switch_with_wallpaper` (`src/lib.rs:51`, `:57`), `undo` (`:95`), `current` (`:108`), `next_theme` / `prev_theme` (`:116`, `:121`), `fill_missing_outputs` (`:151`), `refresh` (`:175`), `init` (`:204`), `EXTRAS` (`:237`), `apps` / `set_app` (`:254`, `:290`), `import` (`:342`). Every one that changes state takes the switch lock (`lock::exclusive`) first |
| `paths.rs` | Every path Palette uses, and fake-home mode | `Paths` (`src/paths.rs:14`), `from_env` (`:41`, reads `PALETTE_HOME` / `XDG_CONFIG_HOME`), `fake_home` (`:24`), `is_fake` (`:63`), `themes()`, `current()`, `generations()`, `backups()`, `wallpaper_state()`, `for_shell` (`:95`, writes `$HOME/...` paths) |
| `theme.rs` | Parse and validate `colors.toml`; list and load themes; 23 bundled themes compiled in | `REQUIRED_KEYS` (`src/theme.rs:14`), `Colors::parse` (`:150`, reports every problem), `Colors::ansi` (`:218`), `Theme` (`:245`), `load` (`:293`, a user theme shadows a bundled one of the same name), `list` (`:339`), `is_valid_name` (`:230`) |
| `render.rs` | minijinja environment, filters, template context, theme overrides | `render` (`src/render.rs:27`), `render_one` (`:37`), `environment` (`:75`), `context` (`:101`), filters `bare`, `argb`, `ink`, `rgb`, `hsl` (`:126`–`:163`), `read_override` (`:194`) |
| `colour.rs` | Judges colours by how they look: OKLCH, WCAG contrast, mood, heading family, pop colours | `analyse` (`src/colour.rs:52`), `Mood` (`:20`), `READABLE` (`:50`), `readable` (`:205`), `mix` (`:222`), `contrast` (`:264`), `Lch` (`:275`, now `pub(crate)`), plus the shared maths `oklab` / `from_oklab` / `lab_distance` / `kmeans` (`:320`, `:334`, `:313`, `:350`) and `analyse_colors` (`:57`, for colours not yet saved as a theme) |
| `state.rs` | Writes generations and swaps the `current` symlink atomically; undo | `apply` (`src/state.rs:64`), `replace_file` (`:115`), `diff` (`:46`), `current_theme` (`:36`), `step_back` (`:179`), keeps 5 generations |
| `wiring.rs` | `palette init`: adds or removes the one line in each app's config, changes one-setting apps, takes backups | `Wiring` (`src/wiring.rs:27`), `Import` (`:38`), `Setting` (`:57`), `run` / `run_for` (`:341`, `:361`), `has_block` / `remove_block` (`:213`, `:222`), `Step` (`:242`), `is_wired` (`:285`), `new_backup_dir` / `remove_old_backups` (`:655`, `:669`), keeps 5 backups |
| `effects.rs` | Everything after the files are written: reload apps, set light/dark, choose and set the wallpaper | `apply` (`src/effects.rs:22`), `Reloads` (`:72`, dedupes shared reloads), `wallpaper_effect` (`:123`, checks the Wallpaper setting *before* anything is chosen; real and fake homes both use it), `theme_wallpaper` (`:141`), `default_wallpaper` / `set_default_wallpaper` (`:161`, `:169`), `set_wallpaper` (`:193`), `step_wallpaper` (`:216`), `add_wallpapers` / `remove_wallpaper` (`:236`, `:273`) |
| `system.rs` | The only module that runs programs or touches macOS | `Outcome` (`src/system.rs:9`), `run` / `run_in` (`:70`, `:75`, adds Homebrew folders to `PATH`), `find_program` / `find_all_programs` (`:63`, `:45`), `is_running` (`:17`), `signal` (`:143`), `set_dark_mode` / `set_wallpaper` (`:169`, `:178`, via `osascript` and System Events), `has_developer_tools` (`:133`), `copy_to_clipboard` (`:198`, `pbcopy`), `reveal` (`:215`, Finder), `utc_offset_minutes` / `time_zone_name` (`:102`, `:124`, for the schedule) |
| `settings.rs` | `settings.toml`: hotkey, Dock, welcome flag, per-app off list, default wallpapers, Obsidian look, `[schedule]`, favourites, `next_cycles` | `Settings` (`src/settings.rs:18`), `load` / `save` (`:69`, `:143`), `is_on` (`:60`), `normalise_hotkey` (`:203`) |
| `doctor.rs` | Read-only health check (`palette doctor`, Settings → Setup → Check). Target notes can be `Note::info` (`src/targets/mod.rs:87`): shown, but not counted as a problem | `run` (`src/doctor.rs:47`), `Row`, `Status`, `theme_row`, `target_row`, `command_row` (warns about several `palette` binaries) |
| `thumbs.rs` | 480 px JPEG previews made with macOS `sips`, cached by path + size + modification time | `thumbnail` (`src/thumbs.rs:30`), `image` (`:40`) |
| `sync.rs` | `palette export` / `import`: a plain folder you carry to another Mac | `export` (`src/sync.rs:228`), `import` (`:346`), `SyncConfig` (`:31`), `default_configs` (`:44`) |
| `install.rs` | Community themes: download, convert, install, update, remove. A theme you made yourself is moved to the Trash on remove; an installed one is deleted (it can be fetched again) | `install` (`src/install.rs:42`), `update` (`:70`), `installed` (`:92`), `Removal` (`:109`), `remove` (`:135`), `move_to_trash` (`:171`, a rename into `~/.Trash`, also used for starter configs), `slug` (`:200`), `complete` (`:517`, fills in missing colour keys) |
| `studio.rs` | Wallpaper tools: recolour a picture to a theme (three strengths), make a theme from a picture (hues harmonised with the picture), paint generated wallpapers in six styles. Painting is threaded (`par_map`) | `Engine` (`src/studio.rs:21`, Palette or Gowall), `has_gowall` (`:40`), `Strength` (`:89`, subtle / balanced / strong), `recolour_image` (`:158`, the 33³ OKLab lookup table), `recolour_name` (`:231`, the file name per engine and strength), `recolour` (`:251`, writes into the theme's backgrounds), `draft_from_image` (`:365`, k-means → colours), `new_from_wallpaper` (`:639`), `STYLES` (`:681`), `paint` (`:895`), `generate` (`:1157`, optional seed) |
| `pixels.rs` | Images without an image crate: `sips` converts to and from BMP, which is parsed and written by hand | `Image` (`src/pixels.rs:16`), `read` (`:34`, optional size cap), `write` (`:50`), `decode_bmp` (`:91`), `encode_bmp` (`:125`) |
| `catalog.rs` + `catalog.toml` | 112 curated Omarchy community themes, colours included, so browsing is offline | `entries` (`src/catalog.rs:31`, parsed once with `OnceLock`), `find` (`:59`), `Entry::theme` (`:21`) |
| `lock.rs` | One change at a time: an exclusive `flock(2)` on `~/.config/palette/.lock` around switch, undo, refresh, init, apps on/off, import and wallpaper changes. Waits up to 10 s, then a clear error. The kernel drops the lock when the process exits, so a stale file never blocks. Re-entrant within a thread (import → switch → init) | `Guard` (`src/lock.rs:35`), `exclusive` (`:50`), `exclusive_for` (`:54`), `TIMEOUT` (`:26`) |
| `schedule.rs` + `schedule/cities.rs` | Day and night themes: a day theme from sunrise, a night theme from sunset (NOAA solar equations; place from a built-in city list or typed latitude/longitude, no network, no Location permission) or from two fixed times. Each boundary is acted on once, so a theme you chose by hand stays until the next boundary | `Schedule` (`src/schedule.rs:60`), `Mode` (`:42`), `sun_day` (`:362`), `evaluate` (`:437`), `which_now` (`:550`), `apply_now` (`:627`), `enable` / `disable` / `save` (`:715`, `:762`, `:690`), `search_cities` / `suggest_city` (`:204`, `:261`), `CITIES` (`cities.rs:22`). Test hooks: `PALETTE_NOW`, `PALETTE_UTC_OFFSET` |
| `favourites.rs` | Up to ten favourite themes (in `settings.toml`) and the last eight switched to (`recent` file); which list `next` / `prev` step through | `Cycle` (`src/favourites.rs:24`), `list` / `add` / `remove` / `set` (`:48`–`:84`), `cycle_names` (`:105`), `recents` / `record` (`:128`, `:142`), `MAX_FAVOURITES`, `MAX_RECENTS` |
| `check.rs` | `palette check [theme]`: contrast report. Every colour pair a theme is used for is measured; pairs below the minimum are listed with what Palette does about them and a suggested colour. Separate from `doctor`, which checks wiring | `BODY` 4.5, `SECONDARY` 2.5, `COMMENTS` 1.6 (`src/check.rs:21`–`:25`), `Issue` (`:139`), `Report` (`:175`), `colors` (`:197`), `run` (`:232`) |
| `tools.rs` + `tools/starters/` | `palette tools`: the programs Palette themes but does not ship (SketchyBar, JankyBorders, AeroSpace, gowall), with the Homebrew commands to get them, and optional starter configs compiled into the binary (`sketchybar/`, `borders/`, `aerospace/`). Palette never runs Homebrew | `status` (`src/tools.rs:197`), `add_starter` (`:343`, refuses if the tool has any config), `remove_starter` (`:411`, to the Trash, only while the files are exactly what Palette wrote), `start_command` (`:248`), `open_terminal` (`:451`) |
| `library.rs` | The wallpaper library `~/.config/palette/wallpapers/`: pictures that belong to no theme. Use one now, copy it into a theme (optionally recoloured), or make a theme from it. Names are paths inside the folder; `list` walks regular files only and never follows symlinks | `dir` (`src/library.rs:25`), `list` (`:50`), `find` (`:125`), `add` (`:138`), `remove` (`:172`), `use_now` (`:187`), `add_to_theme` (`:195`), `new_theme` (`:216`), `sanitise` (`:239`) |
| `fetch.rs` | Wallpapers from a link: one picture, or a GitHub repo or folder, listed through the git trees API and downloaded file by file into the library. The only network code besides `install.rs`. Caps: 40 MB a file, 200 files and 2 GB a download | `Link` (`src/fetch.rs:41`), `list` (`:270`), `select` (`:397`), `check_caps` (`:453`), `download` (`:488`), `image_kind` (`:635`, checks magic bytes) |
| `edits.rs` | Your colour edits: `themes/<name>/edits.toml`, laid over a theme's own colours by `theme::load` (`theme::load_original` skips them). Set, reset one or all, or apply every `check` suggestion | `apply` (`Edit::Set` / `Reset` / `FixContrast`), `sheet`, `read`; `lib::edit_colours` re-renders the active theme |
| `desktop/` | Settings → Bar / Windows / Borders (`palette desktop`). Per tool: Palette writes the config only once you let it manage it (`manage` keeps your file as `desktop/backups/<tool>/original`; `release` restores it); every change backs up first (`KEEP` 5, `restore` goes back) and writes atomically. Settings in `desktop/<tool>.toml`. Reloads are skipped in fake homes | `mod.rs`: `Tool`, `status`, `get`, `set`, `set_keys`, `manage`, `release`, `backups`, `restore`. `macos.rs`: Dock and menu bar (`read`, `set`). `prompt.rs`: Starship (`Prompt`, `Block`, `MODULES`, `OPTIONS`, `from_text` reads a format into blocks, `render` key by key, `preset`, `preview` + `ansi_spans`). `bar/`: `Bar`, `Pill`, `Item`, `KINDS` (the pill library), `SLOTS` (workspace slots spaces.sh fills), `render` (sketchybarrc + scripts in `~/.config/palette/bar/`), `import` (reads your sketchybarrc), and the scripts (`spaces.sh`, `stats.sh`, `widgets.sh`, `habits.sh`, `front_app.sh`). `windows.rs`: `Windows`, `Shortcut`, `COMMANDS`, key-by-key `render` with `toml_edit` (shortcuts too), `import`, `prepare`, `running_apps`. Bar fonts: `bar::font_warnings` asks `system/fonts.rs` (CoreText: `families`, `styles`). `borders.rs`: `Borders`, `render`, `import` |
| `targets/mod.rs` | The `Target` type and the list of all 18 targets | `Target` (`src/targets/mod.rs:35`), `Output` (`:55`), `Note` (`:70`, `ok` / `info` / `fix`), `BASE` (`:104`), `ALL` (`:115`), `has_program` / `has_app` (`:142`, `:147`) |
| `targets/<name>/` | One app each: a template, its wiring, reload and doctor | see section 6 |
| `main.rs` | The CLI: clap subcommands and printing only | `Command` enum (`new --from-wallpaper`), `run`, `report` / `print_effects`, `init`, `bg` (`recolor --strength`, `generate --seed`, `get`, `library`), `catalog`, `schedule`, `tools`, `check`, `fav`, `recent` (1562 lines) |

### App (`app/`)

| File | Job |
| --- | --- |
| `app/src/main.rs` (2211 lines) | Tauri shell: 64 commands, the tray, the global hotkey, deep links, single-instance guard (`tauri_plugin_single_instance`), autostart, Dock policy, window placement, the schedule ticker (checks at launch, then every minute), the switch toast (`announce`) |
| `app/ui/picker.html`, `overlay.html`, `settings.html`, `toast.html` | The four windows. Plain HTML, no build step |
| `app/ui/app.js` (1612 lines) | The picker, quick switcher and Settings logic, picked by `document.body.dataset.view` |
| `app/ui/toast.js` (53 lines) | The "Switched to X · Undo" toast: painted in the new theme's colours, hides itself after 5 s (paused on hover) |
| `app/ui/style.css` | Shared styles, recoloured live in the active theme (`paintChrome`) |
| `app/tauri.conf.json` | Windows, CSP, bundle (app + dmg), ad-hoc signing (`signingIdentity "-"`), `externalBin` |
| `app/Info.plist` | Deliberately has no `LSUIElement` (see section 9) |
| `app/sidecar.sh` | Builds the CLI and copies it to `app/binaries/palette-<triple>`, so the `.app` ships the CLI |
| `app/build.sh` | `cargo tauri build`, then tells macOS to forget the build-folder `Palette.app`, so `palette://` links only go to /Applications |

### Other

`themes/` (23 bundled `colors.toml`), `tests/` (section 11), `examples/starship.toml`, `scripts/release.sh`, `packaging/homebrew/`, `.github/workflows/ci.yml`,
`docs/` (THEME_SPEC, THEME_AUTHORING, ADDING_A_TARGET, WALLPAPERS, this file). The
owner's working docs (ROADMAP, DECISIONS, IDEAS, the hands-on checklist `user_test.md`,
setup notes `CONFIG_INVENTORY.md`, PUBLISHING) stay local and are not in the repo.

---

## 3. Data on disk

Everything Palette owns lives under `~/.config/palette/` (`Paths::root`).

```
~/.config/palette/
  current -> generations/1791846443990-osaka-jade     symlink, swapped atomically on each switch
  generations/
    1791846443990-osaka-jade/                         one folder per switch, newest 5 kept
      theme                                           name of the theme it was rendered from
      sketchybar.sh  borders.sh  ghostty.conf  kitty.conf  alacritty.toml  tmux.conf
      vscodium.json  neovim.lua  obsidian.css  btop.theme  shell.sh  starship.toml
      fzf.opts  lazygit.yml  sketchybar-item.sh  wezterm.lua  habit-tracker.json
  themes/<name>/                                      your themes, installed themes, or just wallpapers for a bundled one
    colors.toml  light.mode  preview.png  backgrounds/  <target>.<ext> overrides
    .palette-source                                   only for `palette install`ed themes: url, date, files that came with it
  wiring/<key>.previous                               old values of settings Palette changed (btop color_theme, Obsidian snippets…)
  backup/<timestamp>-<label>/                         copies of configs before init / import changed them, newest 5 kept
  cache/thumbs/<hash>.jpg                             picker and wallpaper thumbnails
  cache/install-<millis>/                             temporary download, deleted afterwards
  wallpapers/[<folder>/]<picture>                     the wallpaper library: downloads from links and pictures you keep
  starters/<tool config path>                         a copy of each starter config Palette wrote (to tell if you changed it)
  settings.toml                                       hotkey, dock, welcomed, favourites, next_cycles, [apps] off list,
                                                      [wallpapers] defaults, [obsidian], [schedule]
  sync.toml                                           which of your configs `export` takes (created on first export)
  wallpaper                                           path of the last wallpaper Palette set
  recent                                              the last 8 themes switched to, newest first
  schedule.state                                      the day/night boundary (unix seconds) last acted on
  .lock                                               the switch lock (an empty file; the lock is held by the kernel)
```

| Path | Written by | Why |
| --- | --- | --- |
| `current`, `generations/` | `state::apply` (every switch), `state::replace_file` (in-place fixes that aren't a switch) | Your configs import from `current/...`, so one symlink rename switches every app at once. Old generations make `undo` free. |
| `themes/` | you; `effects::add_wallpapers`; `install::install` / `update`; `studio` (recolours, generated wallpapers, themes from pictures) | Themes are data folders |
| `wallpapers/` | `library::add`, `fetch::download` | Pictures that belong to no theme yet; `fetch.rs` is the only writer of downloaded pictures |
| `starters/` | `tools::add_starter` | `remove_starter` only trashes a config that still matches this copy |
| `recent` | `favourites::record` (every switch and undo; failure is ignored) | History only, so it is not in `settings.toml` (the bar pill scrolls many times a second) |
| `schedule.state` | `schedule::apply_now` | Each sunrise / sunset is acted on once, so a hand-picked theme stays until the next one |
| `.lock` | `lock::exclusive` | One change at a time |
| `wiring/` | `wiring::remember` | `init --remove` restores the exact old value of one-setting apps |
| `backup/` | `wiring::back_up`, `sync::import` | Safety copy before any user file changes |
| `settings.toml` | `settings::save` (CLI `hotkey`, `apps`, `bg default`, `obsidian`, `schedule`, `fav`; app Settings) | Plain TOML, editable by hand |
| `wallpaper` | `effects::set_wallpaper` | Lets "keep the current wallpaper if it belongs to this theme" and `bg next` work |

**Atomic swap.** `state::apply` (`src/state.rs:64`) works in three steps:
1. It writes all files into `generations/.tmp-<name>`.
2. It renames that folder to `generations/<name>`.
3. It points a new symlink `.current.tmp` at the folder and renames it over `current`.

`rename(2)` is atomic on the same filesystem, so an app reading at that moment sees either the old theme or the new one, never a mix.

---

## 4. Life of a theme switch

`palette set tokyo-night`, a picker click and a `palette://next` link all end in
`lib::switch_with_wallpaper` (`src/lib.rs:57`). It first takes the **switch lock**
(`lock::exclusive`, an `flock` on `.lock`), so two switches never interleave, then:

1. **Load.** `theme::load` reads `themes/tokyo-night/colors.toml` if it exists, otherwise
   the bundled copy, and validates it with `Colors::parse`: all 23 keys must be
   `#rrggbb`; `orange` and `brown` are optional. A `light.mode` file makes it a light
   theme.
2. **Render.** `render::render` (`src/render.rs:27`) builds one minijinja environment
   holding every target's template, and one context (`context`, `:101`). The context
   holds:
   - every colour, plus `ansi` (the 16 terminal colours), `mode` and `theme`;
   - the colour analysis from `colour::analyse`: `mood`, `family[0..6]`, `pop`, `pop2`,
     `primary` / `secondary` / `tertiary`.

   Then, for each target with an output:
   - if the theme folder holds an override file and the target allows overrides, that
     file is used as is;
   - an override for a non-overridable target is an error;
   - otherwise the template is rendered, then the target's `finish` step, if it has one.
     Starship's `finish` merges in your `starship.toml` and fixes unreadable text colours.
3. **Diff.** `state::diff` reports each file as new / changed / unchanged, for the CLI
   output and `--dry-run`.
4. **Write.** `state::apply` makes a new generation and swaps `current`, as above.
5. **Effects.** `effects::apply` (`src/effects.rs:22`). In a fake home it only records the
   wallpaper choice. Otherwise:
   - **Reloads:** for each target in `targets::ALL`, if it's switched on in
     `settings.toml`, run its `reload` (see section 6). The `Reloads` list (`:75`)
     remembers which reload functions already ran, compared with `std::ptr::fn_addr_eq`.
     SketchyBar and its theme pill share one reload, so the bar redraws once.
   - **Appearance:** unless switched off, `system::set_dark_mode` runs through osascript
     and System Events.
   - **Wallpaper:** unless switched off, `theme_wallpaper` (`:125`) picks in this order:
     1. the one asked for (`--wallpaper`, or the quick switcher's ←→);
     2. the current wallpaper, if it already belongs to this theme;
     3. the theme's default (`[wallpapers]` in settings.toml);
     4. its first wallpaper;
     5. if it has none, your wallpaper is left alone.
6. **Record.** The theme goes on the `recent` list (a failure here never fails the switch).
7. **Report.** The CLI prints changes and effects. The app sends a `theme-changed` event
   so every window re-renders, and `announce` shows the "Switched to X · Undo" toast
   (unless the picker is open or Settings has focus).

`undo` (`src/lib.rs:95`) calls `state::step_back`, which repoints `current` at the
previous generation and runs the effects again. It doesn't re-render: the old files are
still on disk.

---

## 5. `init` and wiring

`palette init` (`lib::init`, `src/lib.rs:204`) first calls `fill_missing_outputs`. This
renders any file the active generation lacks, for example `obsidian.css` after an
upgrade. Then `wiring::run` wires every switched-on target, and finally the apps whose
configs changed are reloaded.

**Three kinds of wiring** (`src/wiring.rs:27`):

| Kind | What it does | Used by |
| --- | --- | --- |
| `Import` | Adds a marker comment (`MARKER`) plus one import/source line, e.g. `[ -f "$HOME/.config/palette/current/sketchybar.sh" ] && source ...`. Where the line goes is the target's `insert` function: `at_end`, `after_colours` (SketchyBar: after your colour variables), or before `sketchybar --update` (the pill). | sketchybar, sketchybar-item, borders, ghostty, kitty, alacritty, tmux, neovim, shell |
| `Setting` | Changes one setting to point at Palette (`value`) and remembers the old value in `wiring/`. With `per_file`, it remembers one value per file (Obsidian vaults). An optional `prepare` step runs first (VSCodium installs its theme extension; Obsidian writes the snippet). | vscodium, btop, obsidian |
| `Shell` | Nothing in the app's config. The tool reads an environment variable from `shell.sh`, which is wired into your shell rc. | starship, fzf, bat, lazygit |

**Blocks are found by their import line** (`has_block`, `remove_block`), not by line
numbers. This means:
- running `init` twice changes nothing;
- removal takes out exactly the marker and the line, so the file comes back byte for
  byte (tested).

Several targets writing to the same file (`sketchybarrc` gets two blocks) are merged
before writing (`plan_write`).

**Backups.** Before the first write, every file about to change is copied into
`backup/<timestamp>-<label>/`. Only the newest 5 backup folders are kept
(`KEEP_BACKUPS`).

**Per-app on/off** (`lib::set_app`, `src/lib.rs:290`):
- **Off:** `name = false` goes into `[apps]`. That one target is unwired (backed up,
  restored), and `shell.sh` is rewritten in place without that tool's variables
  (`state::replace_file`, so it isn't an undo step).
- **On:** calls `fill_missing_outputs`, then wires just that target.

`wallpaper` and `appearance` are on/off "extras" (`EXTRAS`) with no wiring.

---

## 6. Targets

Each target is a `const TARGET: Target` built from `..BASE` defaults
(`src/targets/mod.rs:89`). Fields:

| Field | Meaning |
| --- | --- |
| `name` | Used in CLI, settings and doctor |
| `output: Option<Output>` | `file` (name in `current/`), `template` (minijinja), `overridable` (may a theme ship its own copy? never for files that can run programs), `finish` (post-process) |
| `wiring` | Import / Setting / Shell (section 5) |
| `installed` | Is the app on this Mac? Only for reports |
| `reload` | Make the running app pick up the change |
| `unload` | After `init --remove`: back to your own look (defaults to reload) |
| `env` | Lines this target adds to `shell.sh` |
| `doctor` | Extra checks beyond "wired / not wired" |

| Target | Writes | Wired how (into) | Reload | Override? | Extra doctor checks |
| --- | --- | --- | --- | --- | --- |
| sketchybar | `sketchybar.sh` (colour variables) | Import, after your colour lines (`sketchybarrc` + plugin scripts that set colours) | `sketchybar --reload` | no | |
| sketchybar-item | `sketchybar-item.sh` (the Palette pill) | Import, before `sketchybar --update` | shares sketchybar's reload (deduped) | no | |
| borders | `borders.sh` | Import at end of `bordersrc` | re-runs `borders` with the new colours; unload restores yours | no | |
| ghostty | `ghostty.conf` | Import (`config-file =`) | `SIGUSR2` (Ghostty 1.2+) | yes | version too old for live reload |
| kitty | `kitty.conf` | Import (`include`) | `SIGUSR1` | yes | |
| alacritty | `alacritty.toml` | Import (`import = [...]`) | reloads by itself (watches the file) | yes | |
| tmux | `tmux.conf` | Import (`source-file -q`) | `tmux source-file`; unload re-sources yours | no (tmux can `run-shell`) | |
| vscodium | `vscodium.json` | Setting `workbench.colorTheme` in `settings.json`, after installing a local "Palette" theme extension | updates the extension's theme file | yes | extension missing |
| neovim | `neovim.lua` | Import (`pcall(dofile, ...)`) in `init.lua`, created if missing | running Neovims watch the file | no (Lua) | init.vim-only setups |
| obsidian | `obsidian.css` (colours, plus the active-pane ring and dimming from `[obsidian]` in settings.toml, via `obsidian_ring`/`obsidian_dim` in the template context; `lib::obsidian_look`) → copied as `palette.css` into every vault's `.obsidian/snippets/` | Setting: adds `palette` to `enabledCssSnippets` per vault (not while Obsidian is open) | copies the snippet atomically (temp + rename) | no (CSS can `@import` the web) | snippet present but switched off |
| btop | `btop.theme` | Setting `color_theme = "palette"` plus a link in `~/.config/btop/themes/` | `SIGUSR2` (btop 1.4+) | yes | link missing |
| shell | `shell.sh` (env vars of switched-on tools) | Import, source line in `.zshrc` | none: new shells load it | no | this terminal predates `init` (only when run from a terminal) |
| starship | `starship.toml` = your config + `[palettes.palette]` + contrast fixes | Shell (`STARSHIP_CONFIG`) | next prompt | no (`custom` modules run commands) | your file changed since (run `refresh`); hex colours that won't follow; text colours Palette changed |
| fzf | `fzf.opts` | Shell (`FZF_DEFAULT_OPTS_FILE`) | next run | no | |
| bat | none | Shell (`BAT_THEME` follows the terminal's ANSI colours) | none | n/a | |
| lazygit | `lazygit.yml` | Shell (`LG_CONFIG_FILE`) | next run | no | |
| habit-tracker | `habit-tracker.json` (colours only; JSON, not CSS, because CSS can `@import` from the web) | none: the Habit Tracker app reads the file itself when its "Follow Palette" setting is on (off by default) | none: the app checks the file's date every couple of seconds | no | |
| wezterm | `wezterm.lua` (a Lua table returned as `colors`) | Import, one `pcall(dofile, …)` line before your `return config`; it also adds Palette's folder to WezTerm's reload watch list | none needed: WezTerm reloads when the `current` link changes | no (Lua runs inside WezTerm) | |

There are 18 targets in `ALL` (`src/targets/mod.rs:115`). Adding an app = a new folder
under `src/targets/` and one line in `ALL`. No other target changes.

---

## 7. Colour engine (`src/colour.rs`)

Omarchy themes name colours loosely: in Osaka Jade, `yellow` is green and `blue` is the
accent. Templates that pick colours by name therefore gave six identical headings.
`colour.rs` judges colours by how they look.

- **OKLCH** (`Lch`, `:279`): a colour space with perceptual lightness (L), chroma
  (C, colourfulness) and hue (H). Equal steps look equal, unlike HSL. Conversions use
  Björn Ottosson's published matrices.
- **WCAG contrast** (`contrast`, `:264`): 1 to 21. `READABLE = 3.0` is the floor for
  coloured text on the background. `readable(hex, bg)` (`:205`) lightens or darkens a
  colour in 0.03 L steps until it reaches 3:1.
- **Pool:** the accent plus the 13 normal and bright colours, each made readable, with
  near-duplicates (OKLab distance ≤ 0.06) dropped.
- **Mood** (`:105`): hue clusters are chromatic colours (C > 0.05) at least 35° apart.
  Thresholds were tuned on a contact sheet of all 23 bundled themes.
  - Monochrome: at most 1 cluster, or mean chroma < 0.035.
  - Colourful: mean chroma ≥ 0.085, plus either 5 or more clusters, or 4 clusters with
    chroma ≥ 0.12.
  - Muted: everything else.
- **Family** (`:123`): six colours that belong together, used for Obsidian H1–H6 and
  starship `family1..6`.
  - It starts with the accent, then adds the most distinct colours nearest in hue.
  - The arc starts at ±60° and widens by 30° until there are enough real hues: 6 for
    colourful themes, 4 for muted, 1 for monochrome.
  - Lighter and darker steps fill the rest.
- **Pop / pop2** (`standout`, `:172`): the readable colours furthest in hue from the
  family, weighted by chroma, used for italic, bold and highlight. If nothing stands
  apart (monochrome), it falls back to a much lighter or darker shade.
- **Signature** = `primary/secondary/tertiary` = the first three family colours.
- **OKLab maths** (`oklab`, `from_oklab`, `lab_distance`, `kmeans`) is shared with
  `studio.rs`: recolouring finds nearest theme colours in OKLab, and
  `new --from-wallpaper` clusters a picture's pixels with k-means (k = 10, fixed seed,
  so the result is repeatable).

**Template filters** (`src/render.rs`):

| Filter | Example | Used by |
| --- | --- | --- |
| `bare` | `#7aa2f7` → `7aa2f7` | configs that want hex without `#` |
| `argb` | `0xff7aa2f7`, or `argb("f0")` | SketchyBar, borders |
| `hue` / `sat` / `light` | `221`, `89`, `72` | Obsidian accent (HSL) |
| `rgb` | `122, 162, 247` | Obsidian Catppuccin `--ctp-*` |
| `ink(dark, light)` | whichever reads better on the colour | starship `on_*` names |

**Starship contrast guard** (`src/targets/starship/mod.rs`, `guard_contrast`):
- It walks your merged config. Any `style` naming both a text and a background colour
  whose contrast is below `UNREADABLE = 1.8` gets `darker_background` or
  `bright_foreground` as text, whichever reads better.
- Powerline separators (`[](fg:a bg:b)`, no letters or `$`) are skipped, because they're
  meant to blend.
- 1.8 is deliberately below WCAG: white on Tokyo Night blue is 2.5, and that look is
  intentional in starship's own presets.
- Changes go into the copy only. A header line records them, and doctor reports it.

---

## 8. Community themes (Phase 5c)

- **Catalogue:** `src/catalog.toml` holds 112 themes from the Omarchy manual's
  "Extra themes" page, 74 KB, baked into the binary.
  - Each entry has name, title, url, author, mode and colours. Browsing (`palette
    catalog`, Settings → Get themes) needs no network.
  - `catalog::entries` completes every entry once (`OnceLock`), so previews and mood
    analysis work. A unit test checks that all entries parse.
- **Install** (`install::install`):
  1. Resolve the source: a catalogue name becomes its URL; anything else is used as a
     link or a folder.
  2. Make a slug (`omarchy-aura-theme` → `aura`) and refuse names that clash with a
     bundled theme or one you have.
  3. Fetch:
     - **GitHub:** `curl` downloads `codeload.github.com/<owner>/<repo>/tar.gz/HEAD`
       (300 s, 500 MB caps) and `tar` unpacks it. bsdtar refuses `..` and absolute paths
       by default.
     - **Other hosts:** `git clone --depth 1`, only if the Xcode command-line tools are
       present.
     - **Local folders:** used directly (tests).
  4. `build` writes `themes/.<name>.partial`, then `swap_in` renames it into place.
- **Three colour layouts** are read:
  - named keys in `colors.toml` (often missing the derived `dark_background` …
    `bright_foreground` group);
  - `color0`–`color15` plus `selection_background` (`COLOURS16`);
  - older repos with only `alacritty.toml` (`from_alacritty`).

  `complete` derives any missing key by mixing background, foreground and the colours
  (`colour::mix`), and marks it `# filled`.
- **Safety:**
  - Only `colors.toml`, `light.mode`, `preview.png`/`.jpg` and image files in
    `backgrounds/` are copied (regular files only, never symlinks, ≤ 40 MB each,
    ≤ 80 images).
  - Config files (`ghostty.conf`, `kitty.conf`, `hyprland.conf`, scripts) are never
    imported, because several of their settings can start programs.
- **Update** re-downloads into a new partial folder. It keeps wallpapers you added: any
  file not listed in `.palette-source`.
- **Remove** works only for themes with a `.palette-source`, and never for the active
  theme.

---

## 9. The app (`app/`)

**Windows** (`app/tauri.conf.json`):

| Window | Size | Behaviour |
| --- | --- | --- |
| `picker` | 440×580, no frame, transparent | Under the tray icon (positioner plugin) or top-right; hides when it loses focus (with a 500 ms grace period, the `Shown` state). Theme grid, search, footer: theme ‹ ↶ ›, wallpaper ‹ n/m ›, gear. Ends with a "+ Get more themes" card. |
| `overlay` | 860×500, centred | The hotkey's quick switcher: ↑↓ themes with a mini-desktop preview, ←→ wallpapers, ⏎ applies, typing filters |
| `settings` | 560×680, normal window | Hides on close. Tabs: Apps, Get themes, Wallpapers (with the Library and Get from a link), Get the tools, Setup, Move to another Mac, General, Schedule; plus a hidden Welcome tour pane |
| `toast` | small, top right, no frame | "Switched to X · Undo". Shown without taking focus after a switch from the tray, the bar pill or the quick switcher; hides after 5 s |

**Commands** (`#[tauri::command]` in `app/src/main.rs`), by area:

| Area | Commands |
| --- | --- |
| Themes and switching | `themes`, `thumbnail`, `apply` (with optional wallpaper), `step` (next/prev/undo), `refresh` |
| Apps tab | `apps`, `set_app` |
| Get themes tab | `catalog`, `installed_themes`, `install_theme` (also update), `remove_theme` |
| Wallpapers | `wallpapers`, `wallpaper_thumb`, `set_wallpaper`, `step_wallpaper`, `set_default_wallpaper`, `remove_wallpaper`, `add_wallpapers` (native file dialog), `has_gowall`, `recolour_wallpapers`, `generate_wallpaper`, `draft_theme_from_wallpaper`, `save_theme_from_wallpaper` |
| Library and links | `library`, `library_thumb`, `library_add`, `library_remove`, `library_use`, `library_to_theme`, `draft_theme_from_library`, `reveal_folder`, `wallpaper_link` (list), `wallpaper_link_download` |
| Schedule | `schedule_get`, `schedule_save`, `schedule_cities` |
| Get the tools | `tools_status`, `add_starter`, `remove_starter`, `copy_text`, `open_terminal`, `open_homebrew_site` |
| Favourites and contrast | `set_favourite`, `recents`, `set_next_cycles`, `contrast_report` |
| Colours tab | `colour_sheet`, `edit_colours` (set / reset / fix) |
| Bar, Windows, Borders (`ui/desktop.js`) | `desktop_status`, `desktop_get`, `desktop_set`, `desktop_own` (manage / give back), `desktop_restore`, `bar_kinds`, `running_apps`, `font_families`, `font_styles`, `aerospace_commands`, `macos_get`, `macos_set` (Dock & menu bar tab), `prompt_meta`, `prompt_preview`, `prompt_presets`, `prompt_preset` (Prompt tab) |
| Setup tab | `setup_all` (init / init --remove), `check` (doctor) |
| Move tab | `sync_prefs`, `export_dialog`, `import_dialog` (dry-run plan), `import_apply` |
| General tab | `get_hotkey`, `set_hotkey`, `prefs`, `set_dock`, `set_login`, `set_welcomed`, `set_obsidian_look`, `cli_status`, `install_cli` |
| Windows | `hide`, `open_settings`, `open_settings_tab` |

Slow commands (switching, installing, downloading, painting, thumbnails) are `async` and use `spawn_blocking`,
so the UI thread never waits on `osascript` or `curl`.

**Events:**
- `theme-changed`: after any switch; every window re-renders.
- `themes-changed`: after install or remove.
- `open-tab`: tells Settings which tab to show.
- `switched`: after a switch from the app, with the new theme's colours; the picker and the toast show "Undo" from it.

The web view talks to Rust through `window.__TAURI__` (`withGlobalTauri`). File access is
never given to the page. Thumbnails come back as base64 data URLs, asked for by theme
and file name, never by path.

**Other pieces:**
- **Tray:** left click opens the picker. The right-click menu has Open Picker, Quick
  Switch, Next Theme, Undo, Settings… and Quit.
- **Hotkey:** global-shortcut plugin, default ⌥⇧T, stored in settings.toml.
  `register_hotkey` keeps the old one if the new one is taken.
- **Deep links:** `palette://picker`, `overlay`, `next`, `prev` (`open_url`). The
  SketchyBar pill uses them for click and scroll.
- **Autostart:** autostart plugin with an `--autostart` argument, so a login launch stays
  quiet.
- **Dock policy:** `setup` picks the activation policy once, from the `dock` setting:
  Accessory (menu bar only) or Regular. `Info.plist` has no `LSUIElement`: a UI-element
  app that turns Regular later gets a second Dock tile.
- **First launch:** opens Settings on the welcome tour until `welcomed = true`.
- **Bundled CLI:** `externalBin` ships `Contents/MacOS/palette-cli` (not `palette`, see
  DECISIONS 2026-10-17); Settings → Command-line tool links `~/.local/bin/palette` to it,
  and the Homebrew cask links it as `palette`.
- **Signing:** ad-hoc (`signingIdentity "-"`) signs the whole bundle, so
  `codesign --verify --deep --strict` passes. Gatekeeper still asks on first open
  (Open Anyway).

---

## 10. Export / import, settings, doctor

- **Export** (`sync::export`, `src/sync.rs:228`): a plain folder holding:
  - `palette-export.toml` (version 1, the exporting home path, the active theme);
  - `README.txt`;
  - `palette/`: settings, sync.toml, themes with wallpapers (optional), and `wiring/`;
  - `configs/<path-under-home>`: the configs listed in `sync.toml`. That defaults to
    every config Palette wires plus AeroSpace. `.zshrc` is commented out, because shell
    files hold secrets.
- **Import** (`sync::import`):
  1. Validates the manifest.
  2. Backs up what it will replace.
  3. Copies files, replacing the old home path with the new one in text and links.
  4. Switches theme and runs `init`.

  `--dry-run` lists the changes. Nothing from the folder is executed.
- **settings.toml keys:** `hotkey`, `dock`, `welcomed`, `favourites` (up to ten),
  `next_cycles` (`"all"` or `"favourites"`), `[apps] <name> = false`,
  `[wallpapers] <theme> = "<file>"`, `[obsidian] focus_ring / dim_inactive`, and
  `[schedule]` (`on`, `day`, `night`, `mode` = `sun` or `fixed`, `city`, `lat`, `lon`,
  `day_at`, `night_at`). A malformed `[schedule]` falls back to the defaults.
- **Doctor** (`doctor::run`): one row for the theme (files present?, wallpaper count and the one on screen, the `check` contrast report as info notes), one per target
  (installed / wired / off, plus its notes; `Note::info` lines are shown but not counted as problems), a `command` row (several `palette` binaries
  on PATH) and a `backups` row.

---

## 11. Testing

- **Fake homes:** `PALETTE_HOME=<dir>` makes `Paths` treat `<dir>` as the home folder,
  with configs under `<dir>/.config`. `is_fake()` then skips everything that would touch
  the real Mac: reloads, osascript, VSCodium extension install. Files are still written,
  so behaviour is testable.
- **`tests/common/mod.rs`** `Home` helper:
  - `Home::new()` makes a temporary folder;
  - `run`, `ok` and `fails` run the real compiled binary (`CARGO_BIN_EXE_palette`)
    against it;
  - `write`, `read`, `path`, `palette_dir` and `current_file` set up and inspect files.
- **Test files:** 209 passing tests (2 ignored), counted across the workspace with
  `cargo test --workspace`.

| File | Covers |
| --- | --- |
| `tests/cli.rs` | list / set / next, dry run, errors, generations |
| `tests/phase2.rs` | `init` on a realistic SketchyBar + borders + Ghostty setup, byte-exact `--remove`, undo, wallpapers, safety rules |
| `tests/phase3.rs` | tmux, VSCodium, Neovim, btop, shell tools, doctor, refresh, backup pruning, starship merge |
| `tests/phase4.rs` | SketchyBar item, prev, hotkey, thumbnails |
| `tests/phase5.rs` | per-app on/off, shared files, shell.sh rewrite without undo step |
| `tests/phase5b.rs` | Obsidian vaults, user snippets never overwritten, export / import between two fake Macs |
| `tests/phase5c.rs` | installing from Omarchy-shaped folders: only data copied (configs, scripts, symlinks ignored), name clashes, update keeps your wallpapers, remove rules, alacritty conversion, offline catalogue |
| `tests/wallpapers.rs` | default wallpaper, prev/next wrap, `set --wallpaper`, wallpaper off |
| `tests/habit_tracker.rs`, `tests/wezterm.rs`, `tests/obsidian_panes.rs` | the Habit Tracker colours file, WezTerm wiring, the Obsidian active-pane ring |
| `tests/schedule.rs` | day/night with fixed `PALETTE_NOW` and `PALETTE_UTC_OFFSET`, boundaries acted on once |
| `tests/library.rs`, `tests/studio.rs` | the wallpaper library, downloads from local `file://` fixtures, recolour strengths and generated styles |
| `tests/tools.rs` | tools status, starter configs only into an empty slot, removal to the Trash |
| `tests/next_five.rs` | the switch lock, favourites and recents, the contrast report |
| unit tests | in `colour.rs` (round trip, moods, families of all bundled themes), `render.rs`, `install.rs` (slugs, URLs, colour layouts), `catalog.rs`, `settings.rs`, `starship` (contrast guard), `system.rs`, `fetch.rs` (links, trees, caps, magic bytes), `schedule.rs` (sun times), `studio.rs` |

- **UI checks:** no automated UI tests in the repo. Screens are checked by copying
  `app/ui` into a scratch folder with a mock `window.__TAURI__` (canned command
  results), then screenshotting with headless Chrome. Real-app behaviour is checked by
  hand on a real Mac (the owner keeps that checklist locally).

---

## 12. Security and privacy model

**Never happens:**
- No network except on an explicit action: `palette install` / `update` (and the app's
  Install button), and getting wallpapers from a link (`palette bg get`, Settings →
  Wallpapers → Get from a link). No telemetry. The catalogue, the city list for the
  schedule and the sun times are all offline.
- **Network details (`fetch.rs`).** Through macOS `curl`, https only (redirects too, at
  most 5). A GitHub repo or folder is *listed* with one unauthenticated request to
  `api.github.com/…/git/trees/…?recursive=1` (no repo download), then the pictures you
  choose come one by one from `raw.githubusercontent.com`. A single picture link is
  downloaded as it is. Caps: 40 MB a file, 200 files and 2 GB per download. Each file
  must start with a real picture's magic bytes (png, jpeg, webp, heic, tiff), is saved
  under a sanitised name inside the library, and is opened only by `sips` and the desktop.
- **Starter configs are the one user config file Palette writes.** Only on request
  (`palette tools starter <tool>`, or the button), only when the tool has no config at
  all, only from text compiled into the binary, then wired like `init` (backup first).
  Removal moves the files to `~/.Trash` and only while they still match the copy in
  `starters/`. Homebrew is never run: Palette prints the commands.
- **Desktop configs are written whole, only once you ask.** "Let Palette manage" a tool
  (Bar, Windows, Borders) first copies your file to `desktop/backups/<tool>/original`;
  each change backs up again (last 10) and writes atomically; "Give it back" restores
  the original byte for byte. AeroSpace is edited key by key (`toml_edit`), so everything
  the page doesn't show stays as you wrote it. The bar runs only Palette's own pill scripts
  (compiled in) or a script path you entered; the weather pill, if you add it, calls
  wttr.in.
- **Trash, not delete.** Removing a theme you made yourself, or a starter config, moves
  it to `~/.Trash` (`install::move_to_trash`). Installed themes can be fetched again, so
  they are deleted.
- Nothing in a theme folder is executed. Non-overridable targets refuse override files.
  Installed themes contribute only colours and images.
- No `sudo`, no security settings changed.
- Your configs get exactly one marked block (or one changed setting), only via `init`,
  `set_app` or `import`, always after a backup. Removal is byte-exact.
- `osascript` receives the wallpaper path as an argument, never pasted into the script.
- The web view gets no file-system access, and the CSP allows only local scripts.

**Known gaps:**
- No signature or pinning for downloaded themes: you trust GitHub and the repo owner for
  the images. A malicious image could target an image decoder (`sips`, macOS), which is
  macOS's problem, but still a risk.
- The Gatekeeper first-run prompt remains: ad-hoc signing, no notarisation.
- `settings.toml` is rewritten whole by each saver. Comments you add by hand are lost.

---

## 13. Dependencies and sizes

| Crate | Where | Why |
| --- | --- | --- |
| `anyhow` | core | Error type with context chains (`with_context`) |
| `clap` (derive) | core (CLI) | Subcommands and help text |
| `minijinja` | core | Templates, Jinja syntax, small and fast |
| `toml` | core | `colors.toml`, settings, sync, catalogue, starship merge |
| `serde` (derive) | core | The desktop settings models (bar, windows, borders) to and from TOML |
| `toml_edit` | core | Edits `~/.aerospace.toml` key by key, keeping comments and layout |
| `tempfile` | dev only | Temporary fake homes in tests |
| `tauri` 2 (tray-icon, image-png, macos-private-api) | app | Windows, tray, IPC |
| `tauri-plugin-global-shortcut` | app | Hotkey |
| `tauri-plugin-autostart` | app | Open at login |
| `tauri-plugin-deep-link` | app | `palette://` links |
| `tauri-plugin-positioner` | app | Picker under the tray icon |
| `tauri-plugin-single-instance` (`deep-link` feature) | app | One running copy only; a second launch hands over its arguments and quits |
| `tauri-plugin-dialog` | app | Native folder and file pickers |
| `serde`, `serde_json` | app | Command results to JS |

No JSON, image, HTTP, git or colour crates: JSON is handled by small scanners, images by
`sips` (to BMP and back, `pixels.rs`), HTTP by `curl` (`install.rs`, `fetch.rs`), archives by `tar`, colour maths is hand-written.

**Release profile:** LTO, one codegen unit, stripped, `panic = "abort"`. The CLI binary
is about 2.8 MB (1.4 MB before the colour engine and the 74 KB catalogue; 2.34 MB before
the desktop pages, which added `serde` and `toml_edit`).
`Palette.app` is about 8 MB; the DMG about 3.6 MB.

---

## 14. Weak spots and suggested improvements

Ordered by how much they matter. Effort: S = an hour or two, M = a day, L = several days.

1. **No lock between concurrent switches (fixed in the 1.1 round).** Scrolling the
   SketchyBar pill can fire `palette://next` several times a second, and the app and a
   terminal can switch at once. Now `lock::exclusive` (an `flock` on `.lock`) wraps
   switch, undo, refresh, init, apps on/off, import and wallpaper changes. The second
   caller waits up to 10 s. The lock is re-entrant per thread and the kernel releases it
   on exit, so a crash leaves nothing stuck. *Still open:* a coalescing queue in the app
   (keep only the last request) would stop a burst of scrolls from queuing at all.
2. **"Wallpaper off" was ignored on a real Mac (fixed 2026-10-13).** The wallpaper was
   chosen, and set, as half of a `match` tuple before the setting was read. Now
   `effects::wallpaper_effect` checks first. Lesson: a function with side effects must not
   sit inside a tuple that is matched afterwards, because Rust evaluates both halves.
   (The same day: two menu bar icons, fixed by the single-instance plugin.)
3. **`app/src/main.rs` is 2211 lines, `app/ui/app.js` 1612 lines (M).** Growing with
   every tab (64 commands now). *Suggestion:* split the Rust side into modules per area
   (`themes.rs`, `wallpapers.rs`, `schedule.rs`, `tools.rs`, `setup.rs`, `sync.rs`,
   `tray.rs`), and the JS into `picker.js`, `overlay.js`, `settings/*.js` (plain ES
   modules, still no build step).
4. **Errors are strings (M).** The core uses `anyhow`; the app turns errors into
   `String`. The UI can't tell "not installed" from "permission denied" except by text.
   *Suggestion:* a small error enum for cases the UI acts on (`NotInstalled`,
   `ThemeActive`, `NameTaken`, `NeedsPermission`), serialised with a code; keep anyhow
   inside.
5. **No CI (fixed).** `.github/workflows/ci.yml` runs on a macOS runner: `cargo fmt
   --check`, `clippy --workspace -D warnings`, `cargo test --workspace`. *Still open:*
   rendering every catalogue theme in CI.
6. **No logging (S).** When something odd happens in the app there's no trace.
   *Suggestion:* append one line per switch / effect to `~/.config/palette/palette.log`
   (rotated at ~200 KB), behind a setting. Keep it local, no telemetry.
7. **Catalogue goes stale (S).** `catalog.toml` was built by a one-off script in a
   scratch folder. *Suggestion:* commit the script (`tools/build_catalog.py` or a Rust
   `xtask`), document how to refresh it, and record the fetch date in the file. Later,
   an explicit "Refresh list" button could fetch the manual page (network on click
   only).
8. **Thumbnail cache only grows (S).** `cache/thumbs/` keeps a JPEG for every image ever
   seen, including removed themes. *Suggestion:* prune entries not touched in 30 days on
   startup, or cap the folder at ~100 MB.
9. **Two "installed?" ideas (S).** `theme::Origin::User` covers both your hand-made themes
   and downloaded ones; `.palette-source` tells them apart. *Suggestion:*
   `Origin::Installed`, so the picker can badge them ("from Omarchy") and Remove appears
   in the picker too.
10. **Duplication between CLI and app (S–M).** Both build "theme or current" lookups,
    apply results and messages separately (`print_effects` vs `applied`). *Suggestion:*
    move shared summaries into `lib.rs`, e.g. one `Summary` type both print or
    serialise.
11. **Colour engine thresholds are hand-tuned (M).** Mood and family thresholds were
    tuned on 23 bundled themes. *Suggestion:* a test that analyses all 112 catalogue
    themes and snapshots mood counts, so a change shows its effect; your friends' theme
    inspection can feed corrections.
12. **macOS-only seams (L, for the Linux port).** `system.rs` is already the only place
    that touches the OS, which is the right seam. The wallpaper, appearance,
    `is_running`, `sips` and `has_app` calls need Linux versions. *Suggestion:* a
    `Platform` trait with a macOS implementation now, before more macOS calls spread.
13. **settings.toml loses hand-written comments (S).** `save` rewrites the whole file.
    *Suggestion:* `toml_edit` (keeps formatting) if hand-editing matters, or document
    that comments are not kept.
14. **Accessibility of the UI (M).** Buttons are mostly real `<button>`s, but cards lack
    `aria-label`s, focus outlines are faint, the picker grid isn't arrow-key navigable,
    and colour is the only cue in some places (★/☆ aside). *Suggestion:* keyboard
    navigation in the picker grid, visible focus rings, labels on icon buttons (‹ ↶ ›,
    gear, ×).
15. **No localisation (S to prepare, L to do).** All strings are inline English.
    *Suggestion:* only worth it after release. If so, collect UI strings into one
    `strings.js` first.
16. **Generated preview images for bundled themes (M).** Bundled themes without
    wallpapers show gradients. The roadmap item is open. The colour engine could render
    a small mock desktop (like the overlay preview) to PNG with `sips` from an SVG/BMP.

---

## 15. Glossary

| Term | Meaning |
| --- | --- |
| Activation policy | macOS setting per app: Regular (Dock icon + menu), Accessory (menu bar only), Prohibited. Tauri's `set_activation_policy`. |
| `LSUIElement` | `Info.plist` key that starts an app as Accessory. Removed here, because changing to Regular later made a second Dock tile. |
| Ad-hoc signing | Code signature without an Apple Developer ID (`codesign -s -`). Valid, but Gatekeeper can't vouch for who made it. |
| Gatekeeper / quarantine | macOS checks downloaded apps (the `com.apple.quarantine` attribute) on first open; unsigned or ad-hoc apps need "Open Anyway". |
| Notarisation | Apple scanning a Developer-ID-signed app so Gatekeeper opens it without asking. Needs a paid account. |
| Symlink swap | Make a new symlink under a temporary name, then `rename(2)` it over the old one: atomic replacement. |
| Generation | One rendered set of files in `generations/<millis>-<theme>/`; `current` points at the active one. |
| minijinja | Rust implementation of the Jinja2 template language (`{{ accent }}`, `{% for %}`, filters). |
| Filter | A template function applied with a pipe: `{{ accent \| argb }}`. |
| `OnceLock` | Rust standard-library cell initialised once, on first use, then shared (the parsed catalogue). |
| `fn` pointer / `fn_addr_eq` | Targets store plain function pointers; `std::ptr::fn_addr_eq` compares two to dedupe reloads. |
| `anyhow` | Error crate: one error type carrying a chain of context messages. |
| Tauri command | A Rust function marked `#[tauri::command]` that JavaScript calls with `invoke("name", args)`. |
| `spawn_blocking` | Runs slow, blocking work on a thread pool so the async runtime and UI stay responsive. |
| Sidecar / `externalBin` | An extra executable shipped inside the `.app` (here, the `palette` CLI). |
| Deep link | A custom URL scheme (`palette://next`) that macOS routes to the app. |
| Global shortcut | A hotkey registered system-wide, working in any app. |
| `osascript` / System Events | macOS's AppleScript runner, and the scriptable service that sets wallpaper and dark mode (needs Automation permission). |
| `sips` | macOS's built-in image tool, used to make thumbnails. |
| bsdtar | macOS's `tar`; by default rejects absolute paths and `..` in archives. |
| OKLCH / OKLab | Perceptual colour spaces (Ottosson 2020): equal numeric steps look equal. |
| WCAG contrast | Ratio of relative luminances, 1–21; 4.5 is the usual text standard, 3 for large text. |
| Chroma | How colourful a colour is (0 = grey). |
| `XDG_CONFIG_HOME` | Standard variable for where configs live (default `~/.config`). |
| `PALETTE_HOME` | Palette's fake-home switch for tests and safe manual runs. |
| `SIGUSR1` / `SIGUSR2` | User-defined Unix signals; kitty, Ghostty and btop reload their config on them. |
| CSP | Content Security Policy: which scripts and images the web view may load. |
