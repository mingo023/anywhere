#!/bin/sh
# Usage: drive.sh <scenario> <out-dir> <name>=<step>,<step> …
# Starts the fake pocketd on a fresh fixture, drives pocket-desktop --capture through the steps, keeps evidence in <out-dir>,
# then stops the fixture it started and deletes its scratch dir. Input-step arguments (CAPTURE_KEYS, CAPTURE_TYPE, …) and
# FIXTURE_* knobs pass through from the environment.
set -eu
[ $# -ge 3 ] || { sed -n 2,5p "$0"; exit 2; }
scenario=$1 out=$2; shift 2
# Inside an Anywhere terminal this points at the user's live pocketd and would win over POCKET_HOME.
unset POCKETD_SOCK
root=$(cd "$(dirname "$0")/../../../.." && pwd)
bin="$root/packages/desktop/target/release/pocket-desktop"
cargo build -q --release -p pocket --features capture --manifest-path "$root/packages/desktop/Cargo.toml"
mkdir -p "$out"
out=$(cd "$out" && pwd -P)
fx=$(mktemp -d "${TMPDIR:-/tmp}/verify-anywhere.XXXXXX")
pid= app=
cleanup() {
  for p in $app $pid; do kill "$p" 2>/dev/null || true; done
  rm -rf "$fx" "$fx.log" "$out/run.pid"
}
trap cleanup EXIT INT TERM
FIXTURE_LOG="$out/pocketd-frames.jsonl" bun "$root/.ui-review/fixture/fixture.ts" "$scenario" "$fx" > "$fx.log" 2>&1 &
pid=$!
echo "$pid $fx" > "$out/run.pid"
i=0
until grep -q "^ready" "$fx.log"; do
  kill -0 "$pid" 2>/dev/null || { cat "$fx.log" >&2; exit 1; }
  i=$((i + 1)); [ $i -lt 150 ] || { echo "fixture not ready after 30s" >&2; exit 1; }
  sleep 0.2
done
fx=$(cd "$fx" && pwd -P)
printf '%s\n' "$@" > "$out/steps.txt"
env | grep -E '^(CAPTURE|FIXTURE)_' >> "$out/steps.txt" || true
HOME="$fx/home" POCKET_HOME="$fx/pocket" "$bin" --capture "$out" "$@" > "$out/desktop.log" 2>&1 &
app=$!
echo "$pid $fx $app" > "$out/run.pid"
wait "$app" || { cat "$out/desktop.log" >&2; exit 1; }
app=
cat "$out/desktop.log"
# Git side effects, read after the app quit: every repo and worktree the fixture made, plus any the run added.
for g in $(find "$fx/home/code" -maxdepth 3 -name .git 2>/dev/null | sort); do
  w=$(dirname "$g")
  echo "## ${w#"$fx/home/"}"
  git -C "$w" log --oneline -3
  git -C "$w" status --short
done > "$out/git-state.txt"
find "$fx/home/code" -maxdepth 2 -type d | sed "s|$fx/home/||" | sort > "$out/tree.txt"
