#!/bin/bash
# Palette bar: volume, battery, brightness, weather.
# SketchyBar calls: bash widgets.sh <volume|battery|weather|brightness>
# Weather settings arrive as LOCATION and UNITS (C or F). Weather asks
# wttr.in, the only pill that uses the network, and only if you add it.
export PATH="$PATH:/opt/homebrew/bin"
export LC_NUMERIC=C
[ -f "$HOME/.config/palette/current/sketchybar.sh" ] && source "$HOME/.config/palette/current/sketchybar.sh"
IC_VAR="C_${ICON_KEY:-blue}"; IC=${!IC_VAR:-$BLUE}     # the bar's icon colour (ICON_KEY)

VOL_UP=$(printf '\xef\x80\xa8');       VOL_DOWN=$(printf '\xef\x80\xa7');   VOL_OFF=$(printf '\xef\x80\xa6')
BAT_FULL=$(printf '\xef\x89\x80');     BAT_3Q=$(printf '\xef\x89\x81');     BAT_HALF=$(printf '\xef\x89\x82')
BAT_1Q=$(printf '\xef\x89\x83');       BAT_EMPTY=$(printf '\xef\x89\x84');  BOLT=$(printf '\xef\x83\xa7')
BRIGHT=$(printf '\xef\x81\x82')
SUN=$(printf '\xef\x86\x85');          MOON=$(printf '\xef\x86\x86')
PARTLY_DAY=$(printf '\xee\xbb\xb0');   PARTLY_NIGHT=$(printf '\xee\xbb\xaf')
CLOUD=$(printf '\xef\x83\x82');        FOG=$(printf '\xee\xbc\xa9')
RAIN=$(printf '\xee\xbc\x9c');         HEAVY_RAIN=$(printf '\xee\xbc\x9d')
SHOWER_DAY=$(printf '\xee\xbc\x9e');   SHOWER_NIGHT=$(printf '\xee\xbc\x9b')
STORM=$(printf '\xee\xbc\xac');        SNOW=$(printf '\xef\x8b\x9c')

ITEM=${NAME:-$1}

case "$1" in
volume)
  read -r VOL IS_MUTED < <(osascript -e 'set v to get volume settings' \
    -e '((output volume of v) as text) & " " & ((output muted of v) as text)')
  case "$VOL" in ''|*[!0-9]*) VOL=0 ;; esac
  if [ "$IS_MUTED" = "true" ] || [ "$VOL" -eq 0 ]; then ICON=$VOL_OFF; COLOR=$GREY
  elif [ "$VOL" -lt 34 ]; then ICON=$VOL_DOWN; COLOR=$IC
  else ICON=$VOL_UP; COLOR=$IC; fi
  sketchybar --set "$ITEM" icon="$ICON" icon.color=$COLOR label="${VOL}"
  ;;
battery)
  BATT=$(pmset -g batt)
  PCT=$(echo "$BATT" | grep -Eo '[0-9]+%' | head -1 | tr -d '%')
  case "$PCT" in ''|*[!0-9]*) exit 0 ;; esac
  if echo "$BATT" | grep -q 'AC Power'; then ICON=$BOLT; COLOR=$GREEN
  elif [ "$PCT" -ge 88 ]; then ICON=$BAT_FULL; COLOR=$IC
  elif [ "$PCT" -ge 63 ]; then ICON=$BAT_3Q; COLOR=$IC
  elif [ "$PCT" -ge 38 ]; then ICON=$BAT_HALF; COLOR=$IC
  elif [ "$PCT" -ge 13 ]; then ICON=$BAT_1Q; COLOR=$IC
  else ICON=$BAT_EMPTY; COLOR=$RED; fi
  sketchybar --set "$ITEM" icon="$ICON" icon.color=$COLOR label="${PCT}"
  ;;
brightness)
  [ -z "$INFO" ] && exit 0
  sketchybar --set "$ITEM" icon="$BRIGHT" icon.color=$YELLOW label="$(printf '%.0f' "$INFO")"
  ;;
weather)
  DATA=$(curl -fsS -m 8 "https://wttr.in/${LOCATION// /+}?format=j1" 2>/dev/null \
    | jq -r '.current_condition[0] | "\(.temp_C)|\(.temp_F)|\(.weatherCode)"' 2>/dev/null)
  [ -z "$DATA" ] && exit 0
  TEMP_C=${DATA%%|*}; REST=${DATA#*|}; TEMP_F=${REST%%|*}; CODE=${REST##*|}
  if [ "$UNITS" = "F" ]; then TEMP="${TEMP_F}°"; else TEMP="${TEMP_C}°"; fi
  HOUR=$((10#$(date +%H)))
  NIGHT=0; if [ "$HOUR" -ge 19 ] || [ "$HOUR" -lt 6 ]; then NIGHT=1; fi
  case "$CODE" in
    113) if [ $NIGHT = 1 ]; then ICON=$MOON; COLOR=$PURPLE; else ICON=$SUN; COLOR=$YELLOW; fi ;;
    116) if [ $NIGHT = 1 ]; then ICON=$PARTLY_NIGHT; COLOR=$PURPLE; else ICON=$PARTLY_DAY; COLOR=$YELLOW; fi ;;
    119|122) ICON=$CLOUD; COLOR=$GREY ;;
    143|248|260) ICON=$FOG; COLOR=$GREY ;;
    200|386|389|392|395) ICON=$STORM; COLOR=$PURPLE ;;
    176|353) if [ $NIGHT = 1 ]; then ICON=$SHOWER_NIGHT; else ICON=$SHOWER_DAY; fi; COLOR=$CYAN ;;
    302|305|308|314|356|359) ICON=$HEAVY_RAIN; COLOR=$IC ;;
    179|182|185|227|230|317|320|323|326|329|332|335|338|350|362|365|368|371|374|377) ICON=$SNOW; COLOR=$CYAN ;;
    *) ICON=$RAIN; COLOR=$CYAN ;;
  esac
  sketchybar --set "$ITEM" icon="$ICON" icon.color=$COLOR label="$TEMP"
  ;;
esac
