#!/usr/bin/env bash
# The phone app's end-to-end check: builds pocketd, the fake claude and anywhere-probe, starts pocketd in the
# foreground on a scratch POCKET_HOME and a free loopback port, registers a temp git repo (pushed to a bare origin)
# as a project, puts a fake gh on PATH and a fake push relay on loopback, then the probe pairs through the owner
# socket (as the desktop's "Pair a phone" does), starts a fake claude and runs its checks through the app's Client
# and Stores. Never touches the user's own pocketd.
#   scripts/mobile-e2e.sh [--keep]     --keep leaves the scratch home for a look
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
keep=${1:-}
export PATH="$HOME/.swiftly/bin:$PATH"
unset POCKETD_PTY POCKETD_SOCK POCKET_HOME CLAUDE_CODE_SANDBOXED

# Unix socket paths are capped at 104 bytes on macOS, so the home stays short.
# Resolved, since pocketd places worktrees by comparing the configured root with git's real paths.
home=$(cd "$(mktemp -d /tmp/pkm.XXXX)" && pwd -P)
bin="$home/bin"
mkdir -p "$bin/fake"
pid=""
relay=""
cleanup() {
  [ -n "$pid" ] && kill "$pid" 2>/dev/null && wait "$pid" 2>/dev/null || true
  [ -n "$relay" ] && kill "$relay" 2>/dev/null && wait "$relay" 2>/dev/null || true
  if [ "$keep" = "--keep" ]; then echo "kept $home"; else rm -rf "$home"; fi
}
trap cleanup EXIT

echo "== build"
(cd "$root/packages/pocketd" && go build -o "$bin/pocketd" ./cmd/pocketd && go build -o "$bin/fake/claude" ./e2e/fakeclaude)
scratch=${AK_SCRATCH:-/tmp/ak-e2e}
(cd "$root/packages/ios/Packages/AnywhereKit" && swift build --scratch-path "$scratch" --product anywhere-probe 2>&1 | tail -1)
probe="$scratch/debug/anywhere-probe"

free_port() { python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])'; }
port=$(free_port)
relay_port=$(free_port)

repo="$home/repo"
mkdir -p "$repo" "$home/claude"
repo=$(cd "$repo" && pwd -P)
git -C "$repo" init -q -b main
printf '# Probe\n\nA project the phone e2e reads.\n' >"$repo/README.md"
git -C "$repo" add README.md
git -C "$repo" -c user.name=e2e -c user.email=e2e@x -c commit.gpgsign=false commit -q -m init
git init -q --bare -b main "$home/origin.git"
git -C "$repo" remote add origin "$home/origin.git"
git -C "$repo" push -q -u origin main
git -C "$repo" config user.name e2e
git -C "$repo" config user.email e2e@x
git -C "$repo" config commit.gpgsign false
printf '\nA line the phone sees as a change.\n' >>"$repo/README.md"

cat >"$home/config.json" <<EOF
{"token":"e2e-token","port":$port,"listen":"loopback","restore":{"resumeAgents":false},"push":{"relay":"http://127.0.0.1:$relay_port"}}
EOF
cat >"$home/desktop.json" <<EOF
{"projects":["$repo"],"repos":{"$repo":{"name":"probe"}},"worktree":{"root":"$home"}}
EOF
echo "{\"projects\":{\"$repo\":{\"hasTrustDialogAccepted\":true}}}" >"$home/claude/.claude.json"

# A gh that answers from files (pocketd's own test double): signed in, with a PR and review threads.
fakegh="$home/fakegh"
mkdir -p "$fakegh"
cp "$root/packages/pocketd/internal/github/testdata/fakegh/gh" "$bin/fake/gh"
printf 'gh version 2.81.0 (2025-10-01)\n' >"$fakegh/version"
printf 'github.com\n  \xe2\x9c\x93 Logged in to github.com account probe (keyring)\n  - Active account: true\n' >"$fakegh/auth"
cp "$root/packages/pocketd/internal/github/testdata/cli.json" "$fakegh/view.json"
cp "$root/packages/pocketd/internal/github/testdata/threads.json" "$fakegh/threads.json"

# Before HOME moves: python3 may be a shim that finds itself under HOME.
python3 "$root/scripts/fake-push-relay.py" "$relay_port" "$home/relay.log" &
relay=$!
# pocketd runs gh with the login shell's env, so the fake gh reaches it as a user's own would: from the profile.
printf 'export PATH="%s:$PATH" FAKEGH="%s"\n' "$bin/fake" "$fakegh" | tee "$home/.profile" >"$home/.zprofile"

export FAKEGH="$fakegh" HOME="$home" POCKET_HOME="$home" POCKETD_SOCK="$home/pocketd.sock" CLAUDE_CONFIG_DIR="$home/claude"
export PATH="$bin/fake:$bin:$PATH" POCKETD_RESTORE_SHELL=/bin/sh

echo "== pocketd serve on 127.0.0.1:$port, home $home"
"$bin/pocketd" serve >"$home/pocketd.log" 2>&1 &
pid=$!
for _ in $(seq 100); do
  [ -S "$POCKETD_SOCK" ] && nc -z 127.0.0.1 "$port" 2>/dev/null && nc -z 127.0.0.1 "$relay_port" 2>/dev/null && break
  kill -0 "$pid" 2>/dev/null || { cat "$home/pocketd.log"; exit 1; }
  sleep 0.1
done

echo "== probe"
status=0
"$probe" --host "127.0.0.1:$port" --owner-socket "$POCKETD_SOCK" --project "$repo" --changed README.md --claude --relay-log "$home/relay.log" || status=$?
if [ "$status" -ne 0 ]; then
  echo "== pocketd log"
  tail -40 "$home/pocketd.log"
fi
exit "$status"
