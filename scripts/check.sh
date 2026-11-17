#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build

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

echo "check: ok, snaps in out/snaps, trees in out/trees"
