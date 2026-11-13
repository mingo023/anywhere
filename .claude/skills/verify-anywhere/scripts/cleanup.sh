#!/bin/sh
# Usage: cleanup.sh [evidence-root]   Stops runs drive.sh left behind (killed mid-run) and deletes their scratch dirs.
# Kills only pids recorded in run.pid files, and only if they are still the fixture or pocket-desktop. Evidence stays.
root=$(cd "$(dirname "$0")/../../../.." && pwd)
ev=${1:-$root/.ui-review/shots/verify}
for f in $(find "$ev" -name run.pid 2>/dev/null); do
  set -- $(cat "$f")
  for p in $1 ${3:-}; do
    ps -p "$p" -o command= 2>/dev/null | grep -qE 'fixture\.ts|pocket-desktop' && kill "$p" && echo "stopped $p"
  done
  case ${2:-} in "${TMPDIR:-/tmp}"*/verify-anywhere.*|/tmp/verify-anywhere.*|/private/*/verify-anywhere.*) rm -rf "$2" "$2.log" && echo "removed $2" ;; esac
  rm -f "$f"
done
