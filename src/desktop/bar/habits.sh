#!/bin/bash
# Palette bar: Habit Tracker. Today's habits done / due, and a running timer.
# The Habit Tracker app writes native-model.json and sends habit_tracker_update.
export PATH="$PATH:/opt/homebrew/bin"
[ -f "$HOME/.config/palette/current/sketchybar.sh" ] && source "$HOME/.config/palette/current/sketchybar.sh"

ITEM=${NAME:-habits}
F="$HOME/Library/Application Support/local.habit.tracker/native-model.json"
get() { /usr/bin/plutil -extract "$1" raw -o - "$F" 2>/dev/null; }
hide() { sketchybar --set "$ITEM" drawing=off; exit 0; }

CHECK=$(printf '\xef\x81\x98')
TIMER=$(printf '\xef\x89\x94')

[ -f "$F" ] || hide
[ "$(get bar)" = "true" ] || hide

if [ "$(get date)" != "$(date +%Y-%m-%d)" ]; then
  sketchybar --set "$ITEM" drawing=on icon="$CHECK" icon.color=$GREY label="--"
  exit 0
fi

DONE=$(get done); DUE=$(get due)
case "$DONE" in ''|*[!0-9]*) DONE=0 ;; esac
case "$DUE" in ''|*[!0-9]*) DUE=0 ;; esac
if [ "$DUE" -gt 0 ] && [ "$DONE" -ge "$DUE" ]; then COLOR=$GREEN; else IC_VAR="C_${ICON_KEY:-blue}"; COLOR=${!IC_VAR:-$BLUE}; fi
LABEL="$DONE/$DUE"; [ "$DUE" -eq 0 ] && LABEL="none due"

T_NAME=$(get timer.name)
if [ -n "$T_NAME" ]; then
  START=$(get timer.start); MINS=$(get timer.mins); DOWN=$(get timer.down)
  NOW=$(date +%s); EL=$(( NOW - ${START%.*} / 1000 )); [ "$EL" -lt 0 ] && EL=0
  if [ "$DOWN" = "true" ]; then LEFT=$(( ${MINS%.*} * 60 - EL )); [ "$LEFT" -lt 0 ] && LEFT=0; T="$(( LEFT / 60 ))m left"
  else T="$(( EL / 60 ))m"; fi
  LABEL="$LABEL  $TIMER ${T_NAME:0:14} $T"
fi

sketchybar --set "$ITEM" drawing=on icon="$CHECK" icon.color=$COLOR label="$LABEL"
