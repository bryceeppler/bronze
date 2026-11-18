#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build -q
cargo build -q --release --locked --manifest-path server/Cargo.toml
work=$(mktemp -d)
pids=()
server=
failed=
cleanup() {
  kill "${pids[@]}" $server 2> /dev/null || true
  wait 2> /dev/null || true
  [ -n "$failed" ] || rm -rf "$work"
}
trap cleanup EXIT
export VK_ICD_FILENAMES=${VK_ICD_FILENAMES:-/usr/share/vulkan/icd.d/lvp_icd.json}

pass() { echo "sync-check: $1"; }
fail() {
  failed=1
  echo "sync-check: FAILED $1"
  tail -n 20 "$work"/*.log 2> /dev/null || true
  echo "sync-check: kept $work, which holds a test key and test certificates; remove it when done"
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
  (umask 077 && printf '{ "version": 1, "server": "%s", "key": "%s" }\n' "$2" "$key" > "$1/sync.json")
}

declare -A display=() window=() app=()
launch() {
  local name=$1
  shift
  DISPLAY=${display[$name]} "$@" target/debug/bronze --data "$work/$name" >> "$work/$name.log" 2>&1 &
  app[$name]=$!
  pids+=($!)
  wait_for 20000 'window[$name]=$(DISPLAY=${display[$name]} xdotool search --onlyvisible --name "^Bronze$" 2> /dev/null | head -1) && [ -n "${window[$name]}" ]' \
    || fail "$name never mapped a window"
}

quit() { kill "${app[$1]}"; wait "${app[$1]}" 2> /dev/null || true; }

typing() { echo "$((${EPOCHREALTIME/./} / 1000)) $1 $2" >> "$work/typed.log"; }

type_note() {
  typing "$1" "$2"
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
  Xvfb -displayfd 3 -screen 0 1280x800x24 -nolisten tcp 3> "$work/$name.display" > /dev/null 2>&1 &
  pids+=($!)
done
for name in a b; do
  wait_for 5000 "[ -s '$work/$name.display' ]" || fail "Xvfb for $name did not start"
  display[$name]=":$(head -1 "$work/$name.display")"
  wait_for 5000 "DISPLAY=${display[$name]} xdpyinfo > /dev/null 2>&1" || fail "Xvfb ${display[$name]} does not answer"
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
write_sync_json "$work/a" "http://127.0.0.1:$port"
write_sync_json "$work/b" "http://127.0.0.1:$port"
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
  typing a "$text"
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

quit b
mkdir -p "$work/certs" "$work/tls"
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -days 1 -subj "/CN=sync-check authority" \
  -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign" \
  -keyout "$work/certs/authority.key" -out "$work/certs/authority.pem" 2> /dev/null
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -days 1 -subj "/CN=localhost" \
  -CA "$work/certs/authority.pem" -CAkey "$work/certs/authority.key" -addext "subjectAltName=DNS:localhost" \
  -addext "basicConstraints=critical,CA:FALSE" -addext "extendedKeyUsage=serverAuth" \
  -keyout "$work/certs/server.key" -out "$work/certs/server.pem" 2> /dev/null
python3 -I - "$port" "$work/certs/server.pem" "$work/certs/server.key" > "$work/tls-proxy.port" 2> "$work/tls-proxy.log" << 'EOF' &
import asyncio, ssl, sys

backend, cert, key = int(sys.argv[1]), sys.argv[2], sys.argv[3]

async def pipe(reader, writer):
    try:
        while data := await reader.read(65536):
            writer.write(data)
            await writer.drain()
    finally:
        writer.close()

async def serve(reader, writer):
    upstream_reader, upstream_writer = await asyncio.open_connection("127.0.0.1", backend)
    await asyncio.gather(pipe(reader, upstream_writer), pipe(upstream_reader, writer), return_exceptions=True)

async def main():
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(cert, key)
    server = await asyncio.start_server(serve, "127.0.0.1", 0, ssl=context)
    print(server.sockets[0].getsockname()[1], flush=True)
    await server.serve_forever()

asyncio.run(main())
EOF
proxy=$!
pids+=($proxy)
wait_for 5000 '[ -s "$work/tls-proxy.port" ]' || fail "the TLS proxy did not start"
tls=https://localhost:$(< "$work/tls-proxy.port")
display[tls]=${display[b]}
write_sync_json "$work/tls" "$tls"

launch tls env -u SSL_CERT_FILE -u SSL_CERT_DIR
wait_for 15000 'grep -q "bronze: sync offline: .*certificate" "$work/tls.log"' || fail "untrusted TLS: $(cat "$work/tls.log")"
[ ! -e "$work/tls/notes.json" ] || fail "untrusted TLS: bronze received notes through a server it should not trust"
quit tls
pass "untrusted TLS: with the system trust store bronze refused $tls ($(grep -m1 -o 'offline: .*' "$work/tls.log")) and received no notes"

launch tls env -u SSL_CERT_DIR SSL_CERT_FILE="$work/certs/authority.pem"
wait_for 15000 '[ "$(outline tls)" = "$(outline a)" ]' || fail "trusted TLS: tls $(outline tls), a $(outline a): $(cat "$work/tls.log")"
type_note tls "Typed behind TLS"
wait_for 5000 'holds a "Typed behind TLS"' || fail "trusted TLS: the note typed behind TLS never reached a"
grep -q "bronze: sync synced" "$work/tls.log" || fail "trusted TLS: bronze never said it synced"
quit tls
kill "$proxy"
pass "trusted TLS: with SSL_CERT_FILE naming the test authority bronze synced through $tls, received all $((10 + live)) lines and its typed note reached a"

quit a
traced() {
  display[$1]=${display[a]}
  launch "$1" strace -f -qq -I 1 --kill-on-exit -e trace=socket,connect -o "$work/$1.strace"
}
inet() { grep -E 'AF_INET6?' "$work/$1.strace" | grep -c . || true; }

mkdir -p "$work/off"
traced off
type_note off "Typed with sync off"
wait_for 5000 'holds off "Typed with sync off"' || fail "sync off: the note was not saved"
quit off
families=$(grep -o 'socket(AF_[A-Z0-9]*' "$work/off.strace" | sort | uniq -c | awk '{printf "%s %s, ", $1, substr($2, 8)}')
[ "$(inet off)" = 0 ] || fail "sync off: bronze opened an internet socket: $(grep AF_INET "$work/off.strace" | head -3)"
pass "sync off: with no sync.json bronze opened no AF_INET or AF_INET6 socket under strace (sockets: ${families%, })"

traced a
wait_for 5000 "grep -q 'connect(.*sin_port=htons($port)' '$work/a.strace'" || fail "sync on: no connection to the server under strace"
pass "sync on: the same trace on a synced folder shows $(inet a) internet socket calls, connecting to 127.0.0.1:$port"

quit a
address="http://127.0.0.1:$port"
cat > "$work/setup.json" << EOF
{"events": [
  {"t": 0.3, "type": "tap", "key": "more"},
  {"t": 0.6, "type": "mark", "name": "menu"},
  {"t": 0.7, "type": "tap", "key": "overflow-item2"},
  {"t": 1.0, "type": "mark", "name": "sheet"},
  {"t": 1.1, "type": "type", "text": "$address", "every": 0.01},
  {"t": 1.6, "type": "key", "name": "Enter"},
  {"t": 2.0, "type": "mark", "name": "code"}
]}
EOF
for mark in menu sheet code; do
  target/debug/bronze --data "$work/twin-$mark" --script "$work/setup.json" --at "mark:$mark" \
    --tree "$work/twin-$mark.json" > "$work/twin-$mark.log" 2>&1 || fail "the headless twin did not reach $mark"
done
rect() {
  jq -r --arg key "$2" '[.elements[] | select(.key == $key)][0].rect | select(. != null) | map(floor) | join(" ")' \
    "$work/twin-$1.json"
}
read -r more_x more_y more_w more_h <<< "$(rect menu more)"
read -r item_x item_y item_w item_h <<< "$(rect menu overflow-item2)"
read -r field_x field_y field_w field_h <<< "$(rect sheet sync-field-box)"
read -r copy_x copy_y copy_w copy_h <<< "$(rect code sync-copy)"
read -r composer_x composer_y composer_w composer_h <<< "$(rect menu composer)"
[ "$(jq -r '[.elements[] | select(.key == "overflow-item2")][0].label' "$work/twin-menu.json")" = "Sync…" ] \
  || fail "the menu's third item is not Sync…"
[ -n "$copy_w" ] || fail "the headless twin shows no Copy Code button"

keys() { DISPLAY=${display[$1]} xdotool windowfocus --sync "${window[$1]}" "${@:2}"; }
hint=("$field_x" $((field_y + field_h + 8)) "$field_w" 40)
region() {
  DISPLAY=${display[$1]} import -window "${window[$1]}" -crop "$4x$5+$2+$3" +repage rgb:- 2> /dev/null | md5sum
}
settled() {
  local now
  now=$(region "$@")
  [ "$now" != "$before" ] && sleep 0.15 && [ "$(region "$@")" = "$now" ]
}
after() {
  local name=$1 what=$2 before
  shift 2
  before=$(region "$name" "$@")
  "${act[@]}"
  wait_for 5000 'settled "$name" "$@"' || fail "$name: $what changed nothing at $*"
}
click() { act=(env DISPLAY="${display[$1]}" xdotool mousemove --window "${window[$1]}" $(($2 + $4 / 2)) $(($3 + $5 / 2)) click 1); }
open_sheet() {
  click "$1" "$more_x" "$more_y" "$more_w" "$more_h"
  after "$1" "opening the menu" "$item_x" "$item_y" "$item_w" "$item_h"
  click "$1" "$item_x" "$item_y" "$item_w" "$item_h"
  after "$1" "choosing Sync…" "${hint[@]}"
}
clipboard() {
  DISPLAY=${display[$1]} python3 -c 'import tkinter; r = tkinter.Tk(); r.withdraw(); print(r.selection_get(selection="CLIPBOARD"))' 2> /dev/null
}
hold_clipboard() {
  DISPLAY=${display[$1]} python3 -c 'import sys, tkinter
root = tkinter.Tk()
root.withdraw()
root.clipboard_clear()
root.clipboard_append(sys.argv[1])
root.mainloop()' "$2" &
  pids+=($!)
}
close_sheet() {
  act=(keys "$1" key Escape)
  after "$1" "Escape" "${hint[@]}"
  DISPLAY=${display[$1]} xdotool mousemove --window "${window[$1]}" $((composer_x + 20)) $((composer_y + composer_h / 2)) click 1
}
both_hold() { holds c "$1" && holds d "$1"; }

mkdir -p "$work/c" "$work/d"
display[c]=${display[a]} display[d]=${display[b]}
launch c
launch d
type_note c "Typed on c before setup"
type_note d "Typed on d before setup"
wait_for 5000 'holds c "Typed on c before setup" && holds d "Typed on d before setup"' || fail "setup: the first notes were not saved"

open_sheet c
act=(keys c type --delay 2 "$address")
after c "typing the address" "${hint[@]}"
act=(keys c key Return)
after c "Start New Vault" "$copy_x" "$copy_y" "$copy_w" "$copy_h"
[ -e "$work/c/sync.json" ] || fail "setup: Start New Vault wrote no sync.json"
DISPLAY=${display[c]} xdotool mousemove --window "${window[c]}" $((copy_x + copy_w / 2)) $((copy_y + copy_h / 2)) click 1
wait_for 5000 '[[ "$(clipboard c)" == "$address#bronze1-"* ]]' || fail "setup: Copy Code put no sync code on c's clipboard"
code=$(clipboard c)
pass "setup: on c the menu's Sync… started a vault on $address, and Copy Code put a ${#code}-character sync code on the clipboard"

hold_clipboard d "$code"
wait_for 5000 '[ "$(clipboard d)" = "$code" ]' || fail "setup: the code did not reach d's clipboard"
open_sheet d
act=(keys d key ctrl+v)
after d "pasting the code" "${hint[@]}"
keys d key Return
wait_for 5000 '[ -e "$work/d/sync.json" ]' || fail "setup: Join Vault wrote no sync.json on d"
grep -qF "${code#*#}" "$work/d/sync.json" || fail "setup: d joined with a different key"
wait_for 30000 'both_hold "Typed on c before setup" && both_hold "Typed on d before setup"' \
  || fail "setup: c and d did not merge: c $(outline c), d $(outline d)"
pass "setup: d joined by pasting the code into its field and pressing Return, and both hold both notes"

close_sheet c
close_sheet d
type_note c "Typed on c after setup"
type_note d "Typed on d after setup"
wait_for 10000 'both_hold "Typed on c after setup" && both_hold "Typed on d after setup"' \
  || fail "setup: a new note did not cross: c $(outline c), d $(outline d)"
pass "setup: a note typed in either window reached the other"

echo "sync-check: ok"
