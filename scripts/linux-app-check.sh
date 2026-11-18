#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

tmp=$(mktemp -d)
display=:${XVFB_DISPLAY:-96}
pids=()
cleanup() { kill "${pids[@]}" 2> /dev/null || true; rm -rf "$tmp"; }
trap cleanup EXIT

export CARGO_HOME=${CARGO_HOME:-$HOME/.cargo} RUSTUP_HOME=${RUSTUP_HOME:-$HOME/.rustup}
export HOME=$tmp/home XDG_DATA_HOME=$tmp/data XDG_CONFIG_HOME=$tmp/config XDG_CACHE_HOME=$tmp/cache
mkdir -p "$HOME" "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME"
entry=$XDG_DATA_HOME/applications/ca.bryce.bronze.desktop
theme=assets/brand/icon/linux/dark/hicolor
log=$tmp/bronze.log

pass() { echo "linux app: $1"; }
fail() { echo "linux app: FAILED $1"; [ ! -f "$log" ] || cat "$log"; exit 1; }
wait_for() { for _ in $(seq 100); do eval "$1" && return 0; sleep 0.1; done; return 1; }
installed() { (cd "$tmp" && find home data config -path data/bronze -prune -o -type f -print0 | sort -z | xargs -0 -r sha256sum); }

./scripts/linux-app.sh --autostart > "$tmp/install.log" 2>&1 || { cat "$tmp/install.log"; exit 1; }
first=$(installed)
expected=$( (printf '%s\n' config/autostart/ca.bryce.bronze.desktop data/applications/ca.bryce.bronze.desktop home/.local/bin/bronze
  find "$theme" -type f -printf 'data/icons/hicolor/%P\n') | sort)
[ "$(awk '{print $2}' <<< "$first")" = "$expected" ] \
  && pass "installed exactly the binary, $(find "$theme" -type f | wc -l) icons, launcher entry and login entry" \
  || fail "installed files: $(awk '{print $2}' <<< "$first")"
diff -r "$theme" "$XDG_DATA_HOME/icons/hicolor" && pass "the icons are the brand's dark hicolor set, byte for byte" \
  || fail "the installed icons differ from $theme"
desktop-file-validate "$entry" "$XDG_CONFIG_HOME/autostart/ca.bryce.bronze.desktop" && pass "both entries pass desktop-file-validate" \
  || fail "desktop-file-validate"
[ -x "$HOME/.local/bin/bronze" ] || fail "the binary is not executable"

Xvfb "$display" -screen 0 1280x800x24 -nolisten tcp > /dev/null 2>&1 &
pids+=($!)
unset WAYLAND_DISPLAY
export DISPLAY=$display VK_ICD_FILENAMES=${VK_ICD_FILENAMES:-/usr/share/vulkan/icd.d/lvp_icd.json}
wait_for 'xdpyinfo > /dev/null 2>&1' || fail "Xvfb never started"
lookup=$(GDK_BACKEND=x11 python3 -I -c '
import gi
gi.require_version("Gtk", "4.0")
from gi.repository import Gdk, Gtk
Gtk.init()
theme = Gtk.IconTheme.get_for_display(Gdk.Display.get_default())
print(theme.lookup_icon("ca.bryce.bronze", None, 48, 1, Gtk.TextDirection.NONE, 0).get_file().get_path())
' 2> /dev/null)
[ "$lookup" = "$XDG_DATA_HOME/icons/hicolor/48x48/apps/ca.bryce.bronze.png" ] \
  && pass "GTK resolves Icon=ca.bryce.bronze at 48 px to the installed 48x48 icon" || fail "GTK icon lookup gave $lookup"

gio launch "$entry" > "$log" 2>&1 || fail "gio launch"
wait_for 'window=$(xdotool search --onlyvisible --name "^Bronze$" 2> /dev/null | head -1) && [ -n "$window" ]' \
  || fail "the launcher entry started no window"
app=$(xdotool getwindowpid "$window")
pids+=("$app")
[ "$(readlink "/proc/$app/exe")" = "$HOME/.local/bin/bronze" ] && pass "the launcher entry started the installed binary" \
  || fail "the window belongs to $(readlink "/proc/$app/exe")"
[ "$(xprop -id "$window" WM_CLASS)" = 'WM_CLASS(STRING) = "bronze", "bronze"' ] && pass "its WM_CLASS matches StartupWMClass=bronze" \
  || fail "WM_CLASS is $(xprop -id "$window" WM_CLASS)"

python3 scripts/own-primary.py "Selected after installing" &
pids+=($!)
sleep 0.5
xdotool key ctrl+alt+c
notes=$XDG_DATA_HOME/bronze/notes.json
wait_for 'grep -q "Selected after installing" "$notes" 2> /dev/null' \
  && pass "ctrl+alt+c captured into $(sed "s|^$tmp/||" <<< "$notes")" || fail "capture: no note in $notes"

./scripts/linux-app.sh --autostart > "$tmp/install.log" 2>&1 || { cat "$tmp/install.log"; exit 1; }
[ "$(installed)" = "$first" ] && pass "a second install over the running app left the same files" || fail "a second install changed files"
kill -0 "$app" 2> /dev/null && pass "and the running app kept running" || fail "the second install stopped the app"
kill "$app"
wait_for '! kill -0 "$app" 2> /dev/null' || fail "the app did not stop"

./scripts/linux-app.sh --uninstall > "$tmp/uninstall.log" 2>&1 || { cat "$tmp/uninstall.log"; exit 1; }
[ -z "$(installed)" ] && pass "uninstall removed every installed file" || fail "uninstall left $(installed)"
left=$(cd "$tmp" && find home data config -mindepth 1 -not -path 'data/bronze*' | sort)
[ -z "$left" ] && pass "and every folder it made" || fail "uninstall left $left"
grep -q "Selected after installing" "$notes" && pass "notes in data/bronze were kept" || fail "uninstall touched the notes"
./scripts/linux-app.sh --uninstall > /dev/null && pass "a second uninstall is a no-op" || fail "a second uninstall failed"
