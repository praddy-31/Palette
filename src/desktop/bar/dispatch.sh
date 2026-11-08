#!/bin/bash
# Palette bar: Dispatch. What Dispatch → Settings → Menu bar → "The pill shows" picks
# (papers read today unless you choose more). Dispatch writes status.json, with the text
# ready in `label`, and sends dispatch_update.
export PATH="$PATH:/opt/homebrew/bin"
[ -f "$HOME/.config/palette/current/sketchybar.sh" ] && source "$HOME/.config/palette/current/sketchybar.sh"

ITEM=${NAME:-dispatch}
F="$HOME/Library/Application Support/Dispatch/status.json"
get() { /usr/bin/plutil -extract "$1" raw -o - "$F" 2>/dev/null; }
hide() { sketchybar --set "$ITEM" drawing=off; exit 0; }

PAPER=$(printf '\xef\x87\xaa')   # nf-fa-newspaper

[ -f "$F" ] || hide
[ "$(get bar)" = "true" ] || hide

IC_VAR="C_${ICON_KEY:-blue}"; COLOR=${!IC_VAR:-$BLUE}
LABEL=$(get label)
[ "$(get failed_sources)" -gt 0 ] 2>/dev/null && COLOR=$RED   # a feed's last fetch failed
if [ "$(get date)" != "$(date +%Y-%m-%d)" ]; then
  # Dispatch hasn't written today (it isn't running, or it's just past midnight).
  COLOR=$GREY; LABEL="--"
fi
[ -z "$LABEL" ] && LABEL="--"
sketchybar --set "$ITEM" drawing=on icon="$PAPER" icon.color=$COLOR label="$LABEL"
