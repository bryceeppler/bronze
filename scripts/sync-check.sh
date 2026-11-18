#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build -q
cargo build -q --release --locked --manifest-path server/Cargo.toml
base=${XVFB_DISPLAY:-93}
work=$(mktemp -d)
pids=()
server=
cleanup() {
  kill "${pids[@]}" $server 2> /dev/null || true
  wait 2> /dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT
export VK_ICD_FILENAMES=${VK_ICD_FILENAMES:-/usr/share/vulkan/icd.d/lvp_icd.json}

pass() { echo "sync-check: $1"; }
fail() {
  echo "sync-check: FAILED $1"
  tail -n 20 "$work"/*.log
  exit 1
}
now_ms() { echo $((${EPOCHREALTIME/./} / 1000)); }
wait_for() {
  local deadline=$(($(now_ms) + $1))
  until eval "$2"; do
    (($(now_ms) < deadline)) || return 1
    sleep 0.01
  done
}

start_server() {
  env -i BRONZE_SYNC_LISTEN="127.0.0.1:${port:-0}" BRONZE_SYNC_DATA="$work/server" \
    server/target/release/bronze-sync 2> "$work/server.log" &
  server=$!
  wait_for 5000 'grep -q "listening on" "$work/server.log"' || fail "bronze-sync did not start"
  port=$(sed -n 's/.*listening on 127\.0\.0\.1:\([0-9]*\),.*/\1/p' "$work/server.log")
}
stop_server() { kill "$server"; wait "$server" 2> /dev/null || true; server=; }

new_key() {
  local secret check
  secret=$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')
  check=$(printf %s "$secret" | xxd -r -p | sha256sum | cut -c1-8)
  printf 'bronze1-%s-%s' "$secret" "$check"
}

write_sync_json() {
  (umask 077 && printf '{ "version": 1, "server": "http://127.0.0.1:%s", "key": "%s" }\n' "$port" "$key" > "$1/sync.json")
}

declare -A display=([a]=":$base" [b]=":$((base + 1))") window=()
launch() {
  local name=$1
  shift
  DISPLAY=${display[$name]} "$@" target/debug/bronze --data "$work/$name" > "$work/$name.log" 2>&1 &
  pids+=($!)
  wait_for 20000 'window[$name]=$(DISPLAY=${display[$name]} xdotool search --onlyvisible --name "^Bronze$" 2> /dev/null | head -1) && [ -n "${window[$name]}" ]' \
    || fail "$name never mapped a window"
}

type_note() {
  DISPLAY=${display[$1]} xdotool windowfocus --sync "${window[$1]}" type --delay 2 "$2"
  DISPLAY=${display[$1]} xdotool key Return
}

outline() {
  jq -c 'def note: (if .done then "[x] " else "[ ] " end) + .text;
    [(.loose[] | note), (.sections[] | ("# " + .name), (.notes[] | note))]' "$work/$1/notes.json" 2> /dev/null
}
holds() { grep -qF "$2" "$work/$1/notes.json" 2> /dev/null; }
converged() { [ -n "$(outline a)" ] && [ "$(outline a)" = "$(outline b)" ] && [ "$(outline a | jq length)" = "$1" ]; }

for name in a b; do
  Xvfb "${display[$name]}" -screen 0 1280x800x24 -nolisten tcp > /dev/null 2>&1 &
  pids+=($!)
done
for name in a b; do
  wait_for 5000 "DISPLAY=${display[$name]} xdpyinfo > /dev/null 2>&1" || fail "Xvfb ${display[$name]} did not start"
done

mkdir -p "$work/a" "$work/b"
cat > "$work/a/notes.json" << 'EOF'
{ "version": 1, "active": null,
  "loose": [{ "text": "Ask for a rollout plan", "done": false }, { "text": "Check retry_after units", "done": true }],
  "sections": [{ "name": "Research", "notes": [{ "text": "Compare the two cache designs", "done": false }] }] }
EOF
cat > "$work/b/notes.json" << 'EOF'
{ "version": 1, "active": "Errands",
  "loose": [{ "text": "Summarize the failing test output", "done": false }],
  "sections": [{ "name": "Errands", "notes": [{ "text": "Buy milk", "done": false }] }] }
EOF
cp "$work/a/notes.json" "$work/a.seed.json"
cp "$work/b/notes.json" "$work/b.seed.json"

start_server
key=$(new_key)
write_sync_json "$work/a"
write_sync_json "$work/b"
launch a
launch b
wait_for 30000 'converged 7' || fail "first connect: a $(outline a), b $(outline b)"
cmp -s "$work/a/notes.v1.json" "$work/a.seed.json" && cmp -s "$work/b/notes.v1.json" "$work/b.seed.json" \
  || fail "first connect: a version 1 file was not kept"
pass "first connect: both machines hold all 5 notes in 2 sections, and each keeps its notes.v1.json"

live=10
pressed=()
synced=()
for n in $(seq $live); do
  text="Live edit $n from a"
  DISPLAY=${display[a]} xdotool windowfocus --sync "${window[a]}" type --delay 2 "$text"
  t0=$(now_ms)
  DISPLAY=${display[a]} xdotool key Return
  t1= t2=
  while [ -z "$t2" ]; do
    [ -z "$t1" ] && [[ $(< "$work/a/notes.json") == *"$text"* ]] && t1=${EPOCHREALTIME/./}
    [[ $(< "$work/b/notes.json") == *"$text"* ]] && t2=${EPOCHREALTIME/./} && t1=${t1:-$t2}
    (($(now_ms) - t0 < 5000)) || fail "live edit $n never reached b"
    sleep 0.002
  done
  pressed+=($((t1 / 1000 - t0)))
  synced+=($(((t2 - t1) / 1000)))
done
totals=($(for n in $(seq 0 $((live - 1))); do echo $((pressed[n] + synced[n])); done | sort -n))
median=${totals[$((live / 2))]} slowest=${totals[-1]}
((slowest < 2000)) || fail "a live edit took ${slowest}ms from Return on a to b's notes.json"
pass "live edit: $live notes from Return on a to b's notes.json took median ${median}ms, slowest ${slowest}ms (Return to a's notes.json: ${pressed[*]} ms; a's notes.json to b's: ${synced[*]} ms)"
wait_for 5000 "converged $((7 + live))" || fail "live edit: a $(outline a), b $(outline b)"

stop_server
type_note a "Typed on a while the server is down"
type_note b "Typed on b while the server is down"
wait_for 5000 'holds a "Typed on a while" && holds b "Typed on b while"' || fail "offline edits were not saved locally"
! holds b "Typed on a while" || fail "an offline edit reached b with the server down"
start_server
wait_for 30000 "converged $((9 + live))" || fail "offline edits: a $(outline a), b $(outline b)"
pass "server down: an edit on each machine was saved locally and both arrived once the server returned"

stop_server
rm -rf "$work/server"
start_server
type_note b "Typed on b after the wipe"
wait_for 30000 "converged $((10 + live))" || fail "wiped server: a $(outline a), b $(outline b)"
pass "wiped server: the machines created the vault again and a new note still reached a"

kill "${pids[@]:2}"
wait "${pids[@]:2}" 2> /dev/null || true
pids=("${pids[@]:0:2}")
traced() {
  display[$1]=${display[a]}
  launch "$1" strace -f -qq -I 1 --kill-on-exit -e trace=socket,connect -o "$work/$1.strace"
}
inet() { grep -E 'AF_INET6?' "$work/$1.strace" | grep -c . || true; }

mkdir -p "$work/off"
traced off
type_note off "Typed with sync off"
wait_for 5000 'holds off "Typed with sync off"' || fail "sync off: the note was not saved"
kill "${pids[-1]}"
wait "${pids[-1]}" 2> /dev/null || true
families=$(grep -o 'socket(AF_[A-Z0-9]*' "$work/off.strace" | sort | uniq -c | awk '{printf "%s %s, ", $1, substr($2, 8)}')
[ "$(inet off)" = 0 ] || fail "sync off: bronze opened an internet socket: $(grep AF_INET "$work/off.strace" | head -3)"
pass "sync off: with no sync.json bronze opened no AF_INET or AF_INET6 socket under strace (sockets: ${families%, })"

traced a
wait_for 5000 "grep -q 'connect(.*sin_port=htons($port)' '$work/a.strace'" || fail "sync on: no connection to the server under strace"
pass "sync on: the same trace on a synced folder shows $(inet a) internet socket calls, connecting to 127.0.0.1:$port"

echo "sync-check: ok"
