#!/bin/bash
# Battery percentage; red when low. Hidden on a Mac without a battery.
# These defaults are used until Palette's colours load.
RED=0xfff38ba8
FG=0xffcdd6f4

PERCENTAGE="$(pmset -g batt | grep -Eo '[0-9]+%' | head -1 | cut -d% -f1)"
if [ -z "$PERCENTAGE" ]; then
  sketchybar --set "$NAME" drawing=off
  exit 0
fi

LABEL="${PERCENTAGE}%"
pmset -g batt | grep -q 'AC Power' && LABEL="${LABEL} +"

COLOR=$FG
[ "$PERCENTAGE" -le 20 ] && COLOR=$RED

sketchybar --set "$NAME" drawing=on icon.drawing=off label="$LABEL" label.color=$COLOR
