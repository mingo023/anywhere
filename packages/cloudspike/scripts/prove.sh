#!/bin/bash
set -u
ROOT=$(git rev-parse --show-toplevel)
S=$(mktemp -d "${TMPDIR:-/tmp}/cloudspike-prove.XXXXXX")
unset POCKETD_SOCK
export POCKET_HOME="$S/home" POCKETD_RESTORE_SHELL=/bin/sh
mkdir -p "$POCKET_HOME"
free() { python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])'; }
PORT=$(free); OWNER=$(free); RELAY=$(free); LOCAL=$(free)
echo "{\"token\":\"t\",\"port\":$PORT,\"listen\":\"loopback\",\"ownerListen\":\"127.0.0.1:$OWNER\",\"restore\":{\"resumeAgents\":false}}" > "$POCKET_HOME/config.json"
(cd "$ROOT/packages/pocketd" && go build -o "$S/pocketd" ./cmd/pocketd) || exit 1
(cd "$ROOT/packages/cloudspike" && go build -o "$S/spike" ./cmd/spike) || exit 1
"$S/pocketd" serve > "$S/pocketd.log" 2>&1 & PD=$!
"$S/spike" relay -listen 127.0.0.1:$RELAY > "$S/relay.log" 2>&1 & RL=$!
trap 'kill $PD $RL $DM 2>/dev/null' EXIT
for i in $(seq 50); do nc -z 127.0.0.1 $OWNER 2>/dev/null && break; sleep 0.2; done
TOKEN=$("$S/pocketd" pair --json | python3 -c 'import json,sys;print(json.load(sys.stdin)["token"])')
"$S/spike" demo -relay ws://127.0.0.1:$RELAY -target 127.0.0.1:$OWNER -listen 127.0.0.1:$LOCAL > "$S/demo.log" 2>&1 & DM=$!
for i in $(seq 50); do nc -z 127.0.0.1 $LOCAL 2>/dev/null && break; sleep 0.2; done
echo "== owner hello through relay (good token)"
bun "$ROOT/packages/cloudspike/scripts/probe.ts" ws://127.0.0.1:$LOCAL/ "$TOKEN"; echo "exit $?"
echo "== owner hello through relay (bad token must be refused by pocketd)"
bun "$ROOT/packages/cloudspike/scripts/probe.ts" ws://127.0.0.1:$LOCAL/ "not-a-token"; echo "exit $?"
echo "== relay log"; cat "$S/relay.log"
echo "== demo log"; cat "$S/demo.log"
kill $DM $RL $PD 2>/dev/null; wait 2>/dev/null
echo "scratch $S"
