#!/bin/bash
# Palette bar: CPU, GPU, temperatures, power and memory from ONE long-running macmon
# (ROADMAP 16a). Started once by stats.sh, which replaces it when the settings change.
# Each sample: one awk line, one `sketchybar --set` batch. No `top`, nothing started per
# item. Ends itself when the bar has gone (a `--set` fails: SketchyBar quit, reloaded or the
# pill was removed); macmon then ends on its next write (a broken pipe). stats.sh starts a
# new one on its next run.
#   FEED   the items to set (cpu gpu temp gputemp pwr mem)
#   EVERY  seconds between samples
#   GPU    busy (macmon's busy share, steady) or raw (the GPU driver's figure, jumps)
#   ICON_KEY  the theme colour of the icons
export PATH="$PATH:/opt/homebrew/bin:/usr/sbin:/sbin"
export LC_ALL=C
COLOURS="$HOME/.config/palette/current/sketchybar.sh"

macmon pipe -i "$(( ${EVERY:-5} * 1000 ))" 2>/dev/null | awk \
  -v want=" $FEED " -v gpu="${GPU:-busy}" -v ic="\$C_${ICON_KEY:-blue}" -v colours="$COLOURS" '
# A number by its key: macmon prints one JSON object per line, not always strictly valid,
# so the keys are found by pattern (as the old script did with grep).
function num(key,   m) {
  if (match($0, "\"" key "\": *[0-9.eE+-]+")) {
    m = substr($0, RSTART, RLENGTH); sub(/.*: */, "", m); return m + 0
  }
  return -1
}
function has(item) { return index(want, " " item " ") > 0 }
# Colour by level: the icon colour, yellow from `warm`, red from `hot`; grey when unknown.
function level(v, warm, hot) { return v >= hot ? "$RED" : v >= warm ? "$YELLOW" : ic }
function set(item, v, text, warm, hot) {
  if (!has(item)) return ""
  if (v < 0) return " --set " item " label=-- icon.color=$GREY"
  return " --set " item " label=" sprintf(text, v) " icon.color=" level(v, warm, hot)
}
function raw_gpu(   cmd, line, v) {
  v = -1; cmd = "ioreg -r -d 1 -w 0 -c IOAccelerator"
  while ((cmd | getline line) > 0)
    if (v < 0 && match(line, /"Device Utilization %"=[0-9]+/)) {
      v = substr(line, RSTART, RLENGTH); sub(/.*=/, "", v); v += 0
    }
  close(cmd)
  return v
}
{
  cpu = num("cpu_active_ratio"); if (cpu >= 0) cpu *= 100
  if (gpu == "raw") g = raw_gpu()
  else { g = num("gpu_active_ratio"); if (g >= 0) g *= 100 }
  total = num("ram_total"); used = num("ram_usage")
  mem = (total > 0 && used >= 0) ? used * 100 / total : -1
  p = num("sys_power"); if (p < 0) p = num("all_power")
  args = set("cpu", cpu, "%.0f%%", 60, 85) set("gpu", g, "%.0f%%", 60, 85) \
    set("mem", mem, "%.0f%%", 60, 85) \
    set("temp", num("cpu_temp_avg"), "%.0f\302\260", 70, 85) \
    set("gputemp", num("gpu_temp_avg"), "%.0f\302\260", 70, 85) \
    set("pwr", p, "%.0fW", 15, 25)
  if (args == "") next
  # The colours file is read again each time, so a theme switch shows on the next sample.
  if (system(". \"" colours "\" 2>/dev/null; sketchybar" args) != 0) exit 1
}'
