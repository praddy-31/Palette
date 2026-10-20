#!/bin/bash
# Highlights the focused workspace. $1 is this item's workspace number.
# These defaults are used until Palette's colours load.
ACCENT=0xff89b4fa
PILL=0xf0313244
FG=0xffcdd6f4
DARK=0xff1e1e2e

if [ -n "$SELECTED" ]; then
  # macOS Space event: SELECTED is "true" or "false"
  [ "$SELECTED" = "true" ] && focused=1 || focused=0
else
  # AeroSpace event: FOCUSED_WORKSPACE is sent with the trigger
  now="${FOCUSED_WORKSPACE:-$(aerospace list-workspaces --focused 2>/dev/null)}"
  [ "$1" = "$now" ] && focused=1 || focused=0
fi

if [ "$focused" = 1 ]; then
  sketchybar --set "$NAME" background.color=$ACCENT icon.color=$DARK
else
  sketchybar --set "$NAME" background.color=$PILL icon.color=$FG
fi
