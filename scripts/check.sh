#!/bin/sh
# Every gate a PR passes, stopping at the first failure. Tests get a scratch pocketd home,
# so a run from inside a Pocket terminal never reaches the pocketd that hosts it.
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
POCKET_HOME="$(mktemp -d)"
POCKETD_SOCK="$POCKET_HOME/pocketd.sock"
export POCKET_HOME POCKETD_SOCK
unset POCKETD_PTY
trap 'rm -rf "$POCKET_HOME"' EXIT
cd "$ROOT"

step() {
  name=$1
  shift
  echo "check: $name"
  status=0
  "$@" || status=$?
  if [ "$status" -ne 0 ]; then
    echo "check: FAILED $name"
    exit "$status"
  fi
}

pocketd_tests() (cd packages/pocketd && go vet ./... && go test -race -count=1 ./...)
desktop() (cd packages/desktop && "$@")

step ghostty scripts/build-ghostty.sh
step install pnpm install --frozen-lockfile --prefer-offline
step pocketd pocketd_tests
step "protocol test" pnpm --filter @pocket/protocol test
step "app typecheck" pnpm --filter @pocket/app typecheck
step "app test" pnpm --filter @pocket/app test
step "desktop build" desktop cargo build --workspace
step "desktop clippy" desktop cargo clippy --workspace --all-targets
step "desktop test" desktop cargo test --workspace
step probe scripts/probe-cli.sh
echo "check: ok"
