#!/bin/bash
# Palette bar: the active app (icon and name). SketchyBar passes the app in $INFO.
export PATH="$PATH:/opt/homebrew/bin"
ICON_MAP="$HOME/.config/sketchybar/helpers/icon_map.sh"
[ -f "$ICON_MAP" ] && source "$ICON_MAP"
# Apps the icon font has no glyph for (your map says :default:): the nearest glyph it has
# (owner, 2026-11-15). Your own icon_map.sh wins wherever it knows the app.
# Each glyph must be one no other app uses: a workspace shows a glyph once (DECISIONS 2026-11-16).
palette_icon() {
  case "$1" in
    "Rex" | "Rex Beta") icon_result=":terminal:" ;;
    "Herdr") icon_result=":hyper:" ;;
    "Orca") icon_result=":code:" ;;
    "DeepSeek Harness") icon_result=":deepseek:" ;;
  esac
}

# Your choices (Bar page → App icons) win over the map (IDEAS 37b).
ICONS="$HOME/.config/palette/bar/icons.sh"
[ -f "$ICONS" ] && source "$ICONS"

APP=${INFO:-$(aerospace list-windows --focused --format '%{app-name}' 2>/dev/null)}
[ -z "$APP" ] && exit 0
icon_result=""
type user_icon >/dev/null 2>&1 && user_icon "$APP"
if [ -n "$icon_result" ]; then
  ICON="$icon_result"
elif type __icon_map >/dev/null 2>&1; then
  __icon_map "$APP"
  [ "$icon_result" = ":default:" ] && palette_icon "$APP"
  ICON="$icon_result"
else
  ICON="•"
fi
sketchybar --set "$NAME" icon="$ICON" label="$APP"
