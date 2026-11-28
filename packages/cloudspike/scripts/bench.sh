#!/bin/bash
ulimit -n 20000
S=$(mktemp -d "${TMPDIR:-/tmp}/cloudspike-bench.XXXXXX")
(cd "$(git rev-parse --show-toplevel)/packages/cloudspike" && go build -o "$S/spike" ./cmd/spike) || exit 1
PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
"$S/spike" relay -listen 127.0.0.1:$PORT > "$S/relay.log" 2>&1 & RL=$!
trap 'kill $RL 2>/dev/null' EXIT
sleep 0.5
echo "== relay at rest: rss $(ps -o rss= -p $RL) KB"
for run in "$@"; do
  set -- $run
  echo "== links=$1 idle=$2"
  "$S/spike" bench -relay ws://127.0.0.1:$PORT -links $1 -idle $2 -dur 30s -pid $RL 2>&1 | grep -v "relay line"
  sleep 3
done
