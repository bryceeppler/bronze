#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build

server=(--manifest-path server/Cargo.toml)
cargo fmt --check "${server[@]}"
cargo clippy --locked "${server[@]}" --all-targets -- -D warnings
cargo test --locked "${server[@]}"

bin=target/debug/bronze
mkdir -p out/snaps out/trees
render() {
  local name=$1; shift
  "$bin" "$@" --scale 2 --snap "out/snaps/$name.png" --tree "out/trees/$name.json" > "out/trees/$name.log" 2>&1 \
    || { cat "out/trees/$name.log"; exit 1; }
}

for scene in $("$bin" --list-scenes); do
  for theme in light dark; do
    render "$scene-$theme" --scene "$scene" --theme "$theme"
  done
done

for theme in light dark; do
  render "core-loop-$theme" --script scripts/core-loop.json --at mark:end --theme "$theme"
  grep -qF 'Compare the two cache designs\n\nAsk for a rollout plan' "out/trees/core-loop-$theme.json" \
    || { echo "core loop did not end with the merged note ($theme)"; exit 1; }
done

desktop() {
  local mark=$1 theme=$2
  "$bin" --desktop scripts/backdrop.png --size 1280x800 --scale 2 --theme "$theme" \
    --script scripts/shell.json --selection "Selected in another app" --at "mark:$mark" \
    --snap "out/snaps/shell-$mark-$theme.png" --tree "out/trees/shell-$mark-$theme.json" > "out/trees/shell-$mark-$theme.log" 2>&1 \
    || { cat "out/trees/shell-$mark-$theme.log"; exit 1; }
}
expect() {
  jq -e "$2" "out/trees/shell-$1.json" > /dev/null || { echo "shell run at $1: expected $2"; exit 1; }
}
captures='[.elements[] | select(.text == "Selected in another app")] | length'
composer_focused='[.elements[] | select(.key == "composer" and .focused)] | length == 1'
for theme in light dark; do
  for mark in captured hidden captured-hidden end; do desktop "$mark" "$theme"; done
  expect "captured-$theme" ".windows[0].visible and ($captures) == 1"
  expect "hidden-$theme" "(.windows[0].visible | not) and ($composer_focused | not)"
  expect "captured-hidden-$theme" "(.windows[0].visible | not) and ($captures) == 2"
  expect "end-$theme" ".windows[0].visible and ($composer_focused)"
done

echo "check: ok, snaps in out/snaps, trees in out/trees"
