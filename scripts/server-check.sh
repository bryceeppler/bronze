#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

usage="usage: scripts/server-check.sh binary | docker [image]"
mode=${1:?$usage}
image=${2:-bronze-sync}
work=$(mktemp -d)
max_bytes=64
max_vaults=3
random() { od -An -N32 -tx1 /dev/urandom | tr -d ' \n'; }
now_ms() { echo $((${EPOCHREALTIME/./} / 1000)); }
pid=
container=
volume=

fail() { echo "server-check: FAIL $*"; [[ -f $work/log ]] && cat "$work/log"; exit 1; }

cleanup() {
  [[ -n $pid ]] && kill "$pid" 2> /dev/null && wait "$pid" 2> /dev/null
  [[ -n $container ]] && docker rm -f "$container" > /dev/null
  [[ -n $volume ]] && docker volume rm "$volume" > /dev/null
  rm -rf "$work"
}
trap cleanup EXIT

case $mode in
  binary)
    (cd server && cargo build --release --locked --quiet)
    start() {
      env -i BRONZE_SYNC_LISTEN=127.0.0.1:0 BRONZE_SYNC_DATA="$work/data" BRONZE_SYNC_MAX_BYTES=$max_bytes \
        BRONZE_SYNC_MAX_VAULTS=$max_vaults server/target/release/bronze-sync 2> "$work/log" &
      pid=$!
      for _ in $(seq 50); do grep -q 'listening on' "$work/log" && break; sleep 0.1; done
      grep -qx "bronze-sync listening on 127.0.0.1:[0-9]*, data in $work/data" "$work/log" || fail "start line: $(cat "$work/log")"
      base="http://$(sed -n 's/.*listening on \([^,]*\),.*/\1/p' "$work/log")"
    }
    stop() { kill "$pid"; wait "$pid" || true; pid=; }
    ;;
  docker)
    volume=$(docker volume create --label fleet.worktree="$(git rev-parse --show-toplevel)" "bronze-sync-check-$$")
    start() {
      container=$(docker run -d --label fleet.worktree="$(git rev-parse --show-toplevel)" -p 127.0.0.1::8787 -v "$volume:/data" \
        -e BRONZE_SYNC_MAX_BYTES=$max_bytes -e BRONZE_SYNC_MAX_VAULTS=$max_vaults "$image")
      for _ in $(seq 150); do
        [[ $(docker inspect -f '{{.State.Health.Status}}' "$container") == healthy ]] && break
        sleep 0.2
      done
      docker logs "$container" > "$work/log" 2>&1
      [[ $(docker inspect -f '{{.State.Health.Status}}' "$container") == healthy ]] \
        || fail "container never became healthy: $(docker inspect -f '{{json .State}}' "$container")"
      grep -qx 'bronze-sync listening on 0.0.0.0:8787, data in /data' "$work/log" || fail "start line"
      [[ $(docker top "$container" -o pid,uid | awk 'NR == 2 { print $2 }') == 65532 ]] || fail "server is not running as uid 65532"
      base="http://$(docker port "$container" 8787/tcp | head -1)"
      echo "ok  container is healthy, runs as uid 65532, listens on 0.0.0.0:8787, keeps data in volume $volume"
    }
    stop() { docker stop "$container" > /dev/null; docker rm "$container" > /dev/null; container=; }
    ;;
  *) echo "$usage"; exit 2 ;;
esac

request() { curl -sS -o "$work/body" -D "$work/head" -w '%{http_code}' "$@"; }
etag() { grep -i '^etag:' "$work/head" | cut -d' ' -f2 | tr -d '\r'; }
expect() {
  local want=$1 what=$2 got
  shift 2
  got=$(request "$@")
  [[ $got == "$want" ]] || fail "$what: wanted $want, got $got $(cat "$work/body")"
  echo "ok  $want $what"
}
auth() { echo "Authorization: Bearer $1"; }

start
vault=$(random)
token=$(random)
url() { echo "$base/v1/vaults/$1"; }
board=$(head -c $max_bytes /dev/zero | tr '\0' 'a')
oversized="${board}a"

expect 200 "health" "$base/health"
expect 428 "a write without If-Match or If-None-Match" -X PUT -H "$(auth "$token")" --data-binary one "$(url "$vault")"
expect 413 "creating a board over the size cap" -X PUT -H "$(auth "$token")" -H 'If-None-Match: *' \
  --data-binary "$oversized" "$(url "$vault")"
expect 404 "reading a vault that does not exist" -H "$(auth "$token")" "$(url "$vault")"
expect 201 "creating the vault" -X PUT -H "$(auth "$token")" -H 'If-None-Match: *' \
  --data-binary "$board" "$(url "$vault")"
v1=$(etag)
expect 200 "reading the board" -H "$(auth "$token")" "$(url "$vault")"
[[ $(cat "$work/body") == "$board" && $(etag) == "$v1" ]] || fail "read returned other bytes or version"
expect 401 "reading with a wrong token" -H "$(auth "$(random)")" "$(url "$vault")"
grep -qi '^www-authenticate: Bearer' "$work/head" || fail "401 without WWW-Authenticate"
expect 401 "reading with no token" "$(url "$vault")"
expect 401 "replacing with a wrong token" -X PUT -H "$(auth "$(random)")" -H "If-Match: $v1" --data-binary evil "$(url "$vault")"
expect 404 "a malformed vault id" -H "$(auth "$token")" "$(url "${vault^^}")"
expect 204 "replacing the current version" -X PUT -H "$(auth "$token")" -H "If-Match: $v1" --data-binary two "$(url "$vault")"
v2=$(etag)
[[ -n $v2 && $v2 != "$v1" ]] || fail "replace did not answer a new version"
expect 412 "replacing a stale version" -X PUT -H "$(auth "$token")" -H "If-Match: $v1" --data-binary three "$(url "$vault")"
[[ $(etag) == "$v2" ]] || fail "412 did not carry the current version"
expect 412 "creating a vault that exists" -X PUT -H "$(auth "$token")" -H 'If-None-Match: *' \
  --data-binary four "$(url "$vault")"
[[ $(etag) == "$v2" ]] || fail "412 did not carry the current version"
expect 412 "replacing a vault that does not exist" -X PUT -H "$(auth "$token")" -H "If-Match: $v1" --data-binary x "$(url "$(random)")"
[[ -z $(etag) ]] || fail "412 for a missing vault carried a version"
expect 413 "replacing with a board over the size cap" -X PUT -H "$(auth "$token")" -H "If-Match: $v2" \
  --data-binary "$oversized" "$(url "$vault")"

for n in $(seq 2 $max_vaults); do
  expect 201 "creating vault $n of $max_vaults" -X PUT -H "$(auth "$token")" -H 'If-None-Match: *' \
    --data-binary x "$(url "$(random)")"
done
expect 403 "creating a vault over the cap" -X PUT -H "$(auth "$token")" -H 'If-None-Match: *' \
  --data-binary x "$(url "$(random)")"

began=$(now_ms)
expect 200 "a long-poll on an old version answers at once" -H "$(auth "$token")" -H "If-None-Match: $v1" \
  -H 'Prefer: wait=55' "$(url "$vault")"
(($(now_ms) - began < 1000)) || fail "long-poll on an old version waited"
[[ $(cat "$work/body") == two && $(etag) == "$v2" ]] || fail "long-poll returned other bytes"

began=$(now_ms)
expect 304 "a long-poll on the current version at its timeout" -H "$(auth "$token")" -H "If-None-Match: $v2" \
  -H 'Prefer: wait=1' "$(url "$vault")"
waited=$(($(now_ms) - began))
((waited >= 1000 && waited < 2000)) || fail "304 after ${waited}ms, wanted 1 to 2 seconds"
[[ $(etag) == "$v2" ]] || fail "304 did not carry the current version"

polls=50
pollers=()
for n in $(seq $polls); do
  curl -sS -o /dev/null -w '%{http_code}' -H "$(auth "$token")" -H "If-None-Match: $v2" -H 'Prefer: wait=55' \
    "$(url "$vault")" > "$work/poll-$n" &
  pollers+=($!)
done
sleep 1
began=$(now_ms)
expect 204 "a write while $polls long-polls wait" -X PUT -H "$(auth "$token")" -H "If-Match: $v2" --data-binary three "$(url "$vault")"
v3=$(etag)
wait "${pollers[@]}"
woke=$(($(now_ms) - began))
[[ $(grep -lx 200 "$work"/poll-* | wc -l) == "$polls" ]] || fail "not every long-poll answered 200: $(cat "$work"/poll-*)"
((woke < 1000)) || fail "the last of $polls long-polls woke ${woke}ms after the write"
echo "ok  all $polls long-polls answered 200 within ${woke}ms of the write"

stop
start
expect 200 "the board after a restart" -H "$(auth "$token")" "$(url "$vault")"
[[ $(cat "$work/body") == three && $(etag) == "$v3" ]] || fail "the restart lost the board"

echo "server-check: ok ($mode)"
