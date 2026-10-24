#!/usr/bin/env bash
# The phone app's end-to-end check: builds pocketd, the fake claude and anywhere-probe, starts pocketd in the
# foreground on a scratch POCKET_HOME and a free loopback port, registers a temp git repo as a project, then the
# probe pairs through the owner socket (as the desktop's "Pair a phone" does), starts a fake claude and runs its
# checks through the app's Client and Stores. Never touches the user's own pocketd.
#   scripts/mobile-e2e.sh [--keep]     --keep leaves the scratch home for a look
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
keep=${1:-}
export PATH="$HOME/.swiftly/bin:$PATH"
unset POCKETD_PTY POCKETD_SOCK POCKET_HOME CLAUDE_CODE_SANDBOXED

# Unix socket paths are capped at 104 bytes on macOS, so the home stays short.
home=$(mktemp -d /tmp/pkm.XXXX)
bin="$home/bin"
mkdir -p "$bin/fake"
pid=""
cleanup() {
  [ -n "$pid" ] && kill "$pid" 2>/dev/null && wait "$pid" 2>/dev/null || true
  if [ "$keep" = "--keep" ]; then echo "kept $home"; else rm -rf "$home"; fi
}
trap cleanup EXIT

echo "== build"
(cd "$root/packages/pocketd" && go build -o "$bin/pocketd" ./cmd/pocketd && go build -o "$bin/fake/claude" ./e2e/fakeclaude)
scratch=${AK_SCRATCH:-/tmp/ak-e2e}
(cd "$root/packages/ios/Packages/AnywhereKit" && swift build --scratch-path "$scratch" --product anywhere-probe 2>&1 | tail -1)
probe="$scratch/debug/anywhere-probe"

port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')

repo="$home/repo"
mkdir -p "$repo" "$home/claude"
repo=$(cd "$repo" && pwd -P)
git -C "$repo" init -q -b main
printf '# Probe\n\nA project the phone e2e reads.\n' >"$repo/README.md"
git -C "$repo" add README.md
git -C "$repo" -c user.name=e2e -c user.email=e2e@x -c commit.gpgsign=false commit -q -m init
printf '\nA line the phone sees as a change.\n' >>"$repo/README.md"

cat >"$home/config.json" <<EOF
{"token":"e2e-token","port":$port,"listen":"loopback","restore":{"resumeAgents":false}}
EOF
cat >"$home/desktop.json" <<EOF
{"projects":["$repo"],"repos":{"$repo":{"name":"probe"}},"worktree":{"root":"$home"}}
EOF
echo "{\"projects\":{\"$repo\":{\"hasTrustDialogAccepted\":true}}}" >"$home/claude/.claude.json"

export HOME="$home" POCKET_HOME="$home" POCKETD_SOCK="$home/pocketd.sock" CLAUDE_CONFIG_DIR="$home/claude"
export PATH="$bin/fake:$bin:$PATH" POCKETD_RESTORE_SHELL=/bin/sh

echo "== pocketd serve on 127.0.0.1:$port, home $home"
"$bin/pocketd" serve >"$home/pocketd.log" 2>&1 &
pid=$!
for _ in $(seq 100); do
  [ -S "$POCKETD_SOCK" ] && nc -z 127.0.0.1 "$port" 2>/dev/null && break
  kill -0 "$pid" 2>/dev/null || { cat "$home/pocketd.log"; exit 1; }
  sleep 0.1
done

echo "== probe"
status=0
"$probe" --host "127.0.0.1:$port" --owner-socket "$POCKETD_SOCK" --project "$repo" --changed README.md --claude || status=$?
if [ "$status" -ne 0 ]; then
  echo "== pocketd log"
  tail -40 "$home/pocketd.log"
fi
exit "$status"
