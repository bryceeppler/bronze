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

# xdotool sends a second release for a modifier it lets go of, which no keyboard does, so taps go through XTest directly.
shifts() {
  python3 - "$@" << 'PY'
import ctypes, sys, time
x11, xtest = ctypes.CDLL("libX11.so.6"), ctypes.CDLL("libXtst.so.6")
x11.XOpenDisplay.restype = ctypes.c_void_p
display = ctypes.c_void_p(x11.XOpenDisplay(None))
for step in sys.argv[1:]:
    key, _, state = step.partition(":")
    xtest.XTestFakeKeyEvent(display, {"left": 50, "right": 62}[key], state == "down", 0)
    x11.XFlush(display)
    time.sleep(0.05)
PY
}
select_text() {
  python3 scripts/own-primary.py "$1" &
  pids+=($!)
  sleep 0.5
}
captured() { wait_for "grep -q '$1' '$data/notes.json' 2> /dev/null"; }

xprop -id "$window" WM_CLASS | grep -q '"ca.bryce.bronze"' && pass "the window's class is ca.bryce.bronze" || fail "WM_CLASS $(xprop -id "$window" WM_CLASS)"
grep -q "listening for Shift gestures" out/real-window.log && pass "Bronze listens for Shift gestures on X11" || fail "no Shift listener"
shifts right:down left:down left:up right:up
wait_for '[ "$(map_state)" = IsViewable ]' && pass "Right Shift + Left Shift showed the panel" || fail "both Shifts: map state $(map_state)"
select_text "Picked by a double Left Shift"
shifts left:down left:up left:down left:up
captured "Picked by a double Left Shift" && pass "double-tap Left Shift captured the PRIMARY selection" || fail "double tap: no captured note"
shifts left:down left:up right:down right:up left:down left:up
shifts right:down left:down left:up right:up
wait_for '[ "$(map_state)" = IsUnMapped ]' && pass "Right Shift + Left Shift hid the panel" || fail "both Shifts: map state $(map_state)"
[ "$(grep -c 'Picked by a double' "$data/notes.json")" = 1 ] && pass "taps of different Shifts captured nothing more" || fail "mixed taps captured"

timeout 10 target/debug/bronze --data "$data" bronze://show-or-hide > out/real-window-link.log 2>&1 || fail "the show-or-hide link: launch failed"
wait_for '[ "$(map_state)" = IsViewable ]' && pass "bronze bronze://show-or-hide showed the running panel" || fail "link show: map state $(map_state)"
select_text "Picked by a bound key"
timeout 10 target/debug/bronze --data "$data" bronze://capture >> out/real-window-link.log 2>&1 || fail "the capture link: launch failed"
captured "Picked by a bound key" && pass "bronze bronze://capture captured the PRIMARY selection" || fail "capture link: no captured note"
