#!/bin/bash
# Palette bar: CPU GPU TEMP POWER MEM DISK VPN NET. One run updates the ones in
# STATS (item names). DISK_AT: the disk shows from this % full.
export PATH="$PATH:/opt/homebrew/bin:/usr/sbin:/sbin"
export LC_ALL=C
[ -f "$HOME/.config/palette/current/sketchybar.sh" ] && source "$HOME/.config/palette/current/sketchybar.sh"
IC_VAR="C_${ICON_KEY:-blue}"; IC=${!IC_VAR:-$BLUE}     # the bar's icon colour (ICON_KEY)
has() { case " $STATS " in *" $1 "*) return 0 ;; *) return 1 ;; esac; }
ICON_NET=$(printf '\xef\x80\x99')      # download arrow: shown when you are not on Wi-Fi
ICON_WIFI=$(printf '\xef\x87\xab')
DEG=$(printf '\xc2\xb0')

is_num() { case "$1" in ''|*[!0-9]*) return 1 ;; *) return 0 ;; esac; }

color_for() {      # blue normally, yellow from 60%, red from 85%
  if [ "$1" -ge 85 ]; then echo "$RED"
  elif [ "$1" -ge 60 ]; then echo "$YELLOW"
  else echo "$IC"; fi
}

# ---- TEMP + POWER: macmon (brew install macmon), no sudo. Started now in the background so
#      its half-second sample overlaps with the top sample below instead of adding to it ----
MM_OUT=$(mktemp -t sketchybar_macmon)
MM_PID=""
if { has temp || has pwr; } && command -v macmon >/dev/null 2>&1; then
  macmon pipe -s 1 -i 500 > "$MM_OUT" 2>/dev/null &
  MM_PID=$!
fi

# ---- CPU: 100 - idle, from the second of two one-second top samples ----
CPU=$(top -l 2 -n 0 -s 1 2>/dev/null \
  | awk '/^CPU usage/ { idle = $7 } END { if (idle != "") printf "%.0f", 100 - idle }')

# ---- GPU: device utilisation from the GPU driver (works without sudo) ----
GPU=$(ioreg -r -d 1 -w 0 -c IOAccelerator 2>/dev/null \
  | grep -o '"Device Utilization %"=[0-9]*' | head -1 | grep -o '[0-9]*$')

# ---- MEM: like Activity Monitor: app memory + wired + compressed ----
MEM_TOTAL=$(sysctl -n hw.memsize 2>/dev/null)
MEM=$(vm_stat 2>/dev/null | awk -v total="$MEM_TOTAL" '
  /page size of/                  { ps = $8 }
  /^Anonymous pages/              { anon = $NF + 0 }
  /^Pages purgeable/              { purg = $NF + 0 }
  /^Pages wired down/             { wired = $NF + 0 }
  /^Pages occupied by compressor/ { comp = $NF + 0 }
  END { if (total > 0 && ps > 0) printf "%.0f", (anon - purg + wired + comp) * ps * 100 / total }')

# ---- DISK: used % of the data volume ----
DISK=$(df -k /System/Volumes/Data 2>/dev/null | awk 'NR == 2 { gsub("%", "", $5); print $5 }')

# ---- NET: download speed, from the byte counter change since the last run ----
IFACE=$(route -n get default 2>/dev/null | awk '/interface:/ { print $2 }')
OFFLINE=0
if [ -z "$IFACE" ]; then IFACE=en0; OFFLINE=1; fi      # no default route = offline
read -r RX TX < <(netstat -ibn -I "$IFACE" 2>/dev/null \
  | awk 'NR == 2 { if (NF >= 11) print $7, $10; else print $6, $9 }')
NOW=$(date +%s)
STATE="/tmp/sketchybar_net_state"
NET_DOWN=0
if is_num "$RX" && is_num "$TX"; then
  if [ -f "$STATE" ]; then
    read -r P_T P_RX P_TX P_IF < "$STATE"
    if [ "$P_IF" = "$IFACE" ] && is_num "$P_T" && is_num "$P_RX"; then
      DT=$((NOW - P_T)); [ "$DT" -lt 1 ] && DT=1
      NET_DOWN=$(( (RX - P_RX) / DT ))
      [ "$NET_DOWN" -lt 0 ] && NET_DOWN=0       # counter reset or interface change
    fi
  fi
  echo "$NOW $RX $TX $IFACE" > "$STATE"
fi
NET_LABEL=$(awk -v b="$NET_DOWN" 'BEGIN {
  if (b >= 1048576) printf "%.1fM", b / 1048576
  else if (b >= 1024) printf "%.0fK", b / 1024
  else printf "0K" }')

# ---- Wi-Fi: the NET icon shows the state. It stays the Wi-Fi icon while a VPN is the
#      default route, because it asks the Wi-Fi interface itself rather than the route ----
WIFI_IF=$(networksetup -listallhardwareports 2>/dev/null \
  | awk '/Hardware Port: (Wi-Fi|AirPort)/ { getline; print $2; exit }')
WIFI_IP=""
[ -n "$WIFI_IF" ] && WIFI_IP=$(ipconfig getifaddr "$WIFI_IF" 2>/dev/null)
if [ "$OFFLINE" = 1 ]; then
  NET_ICON=$ICON_WIFI; NET_COLOR=$RED; NET_LABEL="off"
elif [ -n "$WIFI_IP" ]; then
  NET_ICON=$ICON_WIFI; NET_COLOR=$IC
else
  NET_ICON=$ICON_NET; NET_COLOR=$IC                    # ethernet, tethering, ...
fi

# ---- VPN: any VPN service in System Settings (ProtonVPN, Tailscale, ...) that is connected ----
if scutil --nc list 2>/dev/null | grep -q '(Connected)'; then VPN=on; else VPN=off; fi

# ---- TEMP + POWER: collect macmon's sample (give up after about 3 seconds) ----
TEMP=""; PWR=""
if [ -n "$MM_PID" ]; then
  for _ in 1 2 3 4 5 6; do kill -0 "$MM_PID" 2>/dev/null || break; sleep 0.5; done
  kill "$MM_PID" 2>/dev/null
  # grep instead of a JSON parser: macmon's sample is not always strictly valid JSON
  mm_val() { grep -o "\"$1\": *[0-9.]*" "$MM_OUT" | head -1 | grep -o '[0-9.]*$'; }
  TEMP=$(mm_val cpu_temp_avg)
  PWR=$(mm_val sys_power)
  [ -z "$PWR" ] && PWR=$(mm_val all_power)
fi
rm -f "$MM_OUT"
[ -n "$TEMP" ] && TEMP=$(awk -v t="$TEMP" 'BEGIN { printf "%.0f", t }')
[ -n "$PWR" ] && PWR=$(awk -v p="$PWR" 'BEGIN { printf "%.0f", p }')

# ---- Push everything to the bar in one call ----
args=()
add_pct() {        # $1 = item, $2 = percent (or empty if unavailable)
  has "$1" || return 0
  if is_num "$2"; then
    args+=(--set "$1" label="$2" icon.color="$(color_for "$2")")
  else
    args+=(--set "$1" label="--" icon.color=$GREY)
  fi
}
add_pct cpu  "$CPU"
add_pct gpu  "$GPU"
add_pct mem  "$MEM"

# disk only shows from DISK_AT % full
if ! has disk; then :
elif is_num "$DISK" && [ "$DISK" -ge "${DISK_AT:-80}" ]; then
  add_pct disk "$DISK"; args+=(--set disk drawing=on)
else
  args+=(--set disk drawing=off)
fi

# temperature: blue, yellow from 70, red from 85 (degrees C)
if ! has temp; then :
elif is_num "$TEMP"; then
  if [ "$TEMP" -ge 85 ]; then C=$RED; elif [ "$TEMP" -ge 70 ]; then C=$YELLOW; else C=$IC; fi
  args+=(--set temp label="${TEMP}${DEG}" icon.color=$C)
else
  args+=(--set temp label="--" icon.color=$GREY)
fi

# power: whole-system watts; blue, yellow from 15 W, red from 25 W
if ! has pwr; then :
elif is_num "$PWR"; then
  if [ "$PWR" -ge 25 ]; then C=$RED; elif [ "$PWR" -ge 15 ]; then C=$YELLOW; else C=$IC; fi
  args+=(--set pwr label="${PWR}W" icon.color=$C)
else
  args+=(--set pwr label="--" icon.color=$GREY)
fi

has vpn && args+=(--set vpn drawing=$VPN)
has net && args+=(--set net label="$NET_LABEL" icon="$NET_ICON" icon.color=$NET_COLOR)

[ ${#args[@]} -gt 0 ] && sketchybar "${args[@]}"
