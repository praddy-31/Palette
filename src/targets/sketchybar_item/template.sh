# Palette: {{ theme }}. Theme switcher item for SketchyBar (generated; edits are overwritten)
# Sourced once from sketchybarrc. A small pill left of your right-hand pill:
# shows the theme, click opens what Palette's menu bar click opens (picker or quick switcher),
# scroll changes theme (unless turned off: `palette fav scroll off`).
PALETTE_ICON=$(printf '\xf3\xb0\x8f\x98')
sketchybar --add item palette_gap right \
  --set palette_gap width=6 padding_left=0 padding_right=0 icon.drawing=off label.drawing=off \
  --add item palette right \
  --set palette icon="$PALETTE_ICON" icon.color=$ACCENT icon.padding_left=12 \
    label="{{ theme }}" label.padding_right=12 \
    click_script="open -g palette://click" \
{%- if bar_scroll %}
    script='[ "$SENDER" = mouse.scrolled ] || exit 0; if [ "${SCROLL_DELTA:-0}" -gt 0 ]; then open -g palette://prev; else open -g palette://next; fi' \
  --subscribe palette mouse.scrolled \
{%- endif %}
  --add bracket palette_pill palette \
  --set palette_pill background.color=$PILL background.corner_radius=14 \
    background.height=30 background.border_width=0
