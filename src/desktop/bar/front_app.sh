#!/bin/bash
# Palette bar: the active app (icon and name). SketchyBar passes the app in $INFO.
export PATH="$PATH:/opt/homebrew/bin"
ICON_MAP="$HOME/.config/sketchybar/helpers/icon_map.sh"
[ -f "$ICON_MAP" ] && source "$ICON_MAP"

APP=${INFO:-$(aerospace list-windows --focused --format '%{app-name}' 2>/dev/null)}
[ -z "$APP" ] && exit 0
if type __icon_map >/dev/null 2>&1; then
  __icon_map "$APP"; ICON="$icon_result"
else
  ICON="•"
fi
sketchybar --set "$NAME" icon="$ICON" label="$APP"
