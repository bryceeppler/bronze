#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build -q
display=:${XVFB_DISPLAY:-97}
data=$(mktemp -d)
pids=()
cleanup() { kill "${pids[@]}" 2> /dev/null || true; rm -rf "$data"; }
trap cleanup EXIT

Xvfb "$display" -screen 0 1280x800x24 -nolisten tcp > /dev/null 2>&1 &
pids+=($!)
export DISPLAY=$display VK_ICD_FILENAMES=${VK_ICD_FILENAMES:-/usr/share/vulkan/icd.d/lvp_icd.json}
for _ in $(seq 50); do xdpyinfo > /dev/null 2>&1 && break; sleep 0.1; done

launch() {
  target/debug/bronze --data "$data" > "out/real-window.log" 2>&1 &
  app=$!
  pids+=("$app")
  for _ in $(seq 100); do
    window=$(xdotool search --onlyvisible --name '^Bronze$' 2> /dev/null | head -1) && [ -n "$window" ] && return
    sleep 0.1
  done
  echo "real window: Bronze never mapped"; cat out/real-window.log; exit 1
}
map_state() { xwininfo -id "$window" | awk -F': ' '/Map State/ {print $2}'; }
corner() { xwininfo -id "$window" | awk -F': *' '/Absolute upper-left X/ {x=$2} /Absolute upper-left Y/ {y=$2} END {print x "," y}'; }
pass() { echo "real window: $1"; }
fail() { echo "real window: FAILED $1"; cat out/real-window.log; exit 1; }
wait_for() { for _ in $(seq 50); do eval "$1" && return 0; sleep 0.1; done; return 1; }

mkdir -p out
launch
pass "mapped at $(corner) on a 1280x800 screen"

python3 scripts/own-primary.py "Selected in xvfb, not copied" &
pids+=($!)
sleep 0.5
xdotool key ctrl+alt+c
wait_for 'grep -q "Selected in xvfb, not copied" "$data/notes.json" 2> /dev/null' \
  && pass "ctrl+alt+c captured the PRIMARY selection into notes.json" || fail "capture: notes.json has no captured note"

clipboard=$(python3 -c 'import tkinter; r = tkinter.Tk(); r.withdraw(); print(r.selection_get(selection="CLIPBOARD"))' 2>&1 || true)
case $clipboard in *"Selected in xvfb"*) fail "capture wrote the clipboard" ;; *) pass "the clipboard was left alone" ;; esac
import -window root out/real-window.png && pass "screenshot in out/real-window.png"

xdotool key ctrl+alt+space
wait_for '[ "$(map_state)" = IsUnMapped ]' && pass "ctrl+alt+space unmapped the window" || fail "hide: map state $(map_state)"
xdotool key ctrl+alt+space
wait_for '[ "$(map_state)" = IsViewable ]' && pass "ctrl+alt+space mapped it again" || fail "show: map state $(map_state)"

xdotool windowmove "$window" 140 90
wait_for '[ "$(corner)" = 140,90 ]' || fail "move: corner $(corner)"
xdotool key ctrl+alt+space
wait_for '[ "$(map_state)" = IsUnMapped ]' || fail "hide after move"
wait_for 'grep -q "\"x\": 140" "$data/preferences.json" 2> /dev/null' || fail "preferences.json has no moved frame"
kill "$app"; wait "$app" 2> /dev/null || true
launch
[ "$(corner)" = 140,90 ] && pass "relaunched where it was moved, 140,90" || fail "relaunch: corner $(corner)"

xdotool key ctrl+alt+space
wait_for '[ "$(map_state)" = IsUnMapped ]' || fail "hide before a second launch"
timeout 10 target/debug/bronze --data "$data" > out/real-window-second.log 2>&1 \
  && pass "a second launch on the same data folder exited by itself" || fail "second launch: kept running or failed"
wait_for '[ "$(map_state)" = IsViewable ]' && pass "and brought the running panel back" || fail "second launch: map state $(map_state)"
xdotool key ctrl+alt+space
wait_for '[ "$(map_state)" = IsUnMapped ]' && pass "which one press of the chord hides again" || fail "hide after a second launch"
