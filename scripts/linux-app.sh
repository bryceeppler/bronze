#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

case $#:${1:-} in
  0: | 1:--autostart | 1:--uninstall) mode=${1:-} ;;
  *) echo "usage: scripts/linux-app.sh [--autostart | --uninstall]" >&2; exit 2 ;;
esac

data=${XDG_DATA_HOME:-$HOME/.local/share}
config=${XDG_CONFIG_HOME:-$HOME/.config}
bin=$HOME/.local/bin/bronze
theme=assets/brand/icon/linux/dark/hicolor
entry=$data/applications/ca.bryce.bronze.desktop
autostart=$config/autostart/ca.bryce.bronze.desktop
mapfile -t icons < <(find "$theme" -type f -printf '%P\n' | sort)

prune() {
  local dir
  dir=$(dirname "$1")
  while [ "$dir" != "$HOME" ] && [ "$dir" != "$data" ] && [ "$dir" != "$config" ] && [ "$dir" != / ] \
    && rmdir "$dir" 2> /dev/null; do
    dir=$(dirname "$dir")
  done
}

if [ "$mode" = --uninstall ]; then
  for file in "$bin" "$entry" "$autostart" "${icons[@]/#/"$data/icons/hicolor/"}"; do
    rm -f "$file"
    prune "$file"
  done
  echo "linux-app: removed Bronze; notes in $data/bronze are kept"
  exit 0
fi

case $bin in *[\"\`\$\\%]*) echo "linux-app: cannot write a desktop entry for $bin" >&2; exit 1 ;; esac

place() {
  mkdir -p "$(dirname "$2")"
  cp "$1" "$2.new"
  chmod "$3" "$2.new"
  mv -f "$2.new" "$2"
}

cargo build --release

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)
staged=target/linux-app
mkdir -p "$staged"
cat > "$staged/ca.bryce.bronze.desktop" <<ENTRY
[Desktop Entry]
Type=Application
Version=1.5
Name=Bronze
GenericName=Capture Panel
Comment=Capture selected text into a local to-do list, clipboard and scratchpad
Exec="$bin"
Icon=ca.bryce.bronze
Terminal=false
Categories=Utility;
Keywords=notes;clipboard;capture;prompts;
StartupWMClass=ca.bryce.bronze
ENTRY

place target/release/bronze "$bin" 755
for icon in "${icons[@]}"; do place "$theme/$icon" "$data/icons/hicolor/$icon" 644; done
place "$staged/ca.bryce.bronze.desktop" "$entry" 644
[ "$mode" != --autostart ] || place "$staged/ca.bryce.bronze.desktop" "$autostart" 644

echo "linux-app: installed $bin ($version) and Bronze in the app launcher"
[ "$mode" != --autostart ] || echo "linux-app: Bronze starts at login"
