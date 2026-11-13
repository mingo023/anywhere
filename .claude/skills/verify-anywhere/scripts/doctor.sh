#!/bin/sh
# Usage: doctor.sh [evidence-root]   Read-only: is this checkout ready to drive? Exit 1 on any FAIL.
root=$(cd "$(dirname "$0")/../../../.." && pwd)
bin="$root/packages/desktop/target/release/pocket-desktop"
ev=${1:-$root/.ui-review/shots/verify}
bad=0
ok() { echo "ok    $1"; }
fail() { echo "FAIL  $1"; bad=1; }
warn() { echo "warn  $1"; }
command -v bun >/dev/null && ok "bun $(bun --version)" || fail "bun not on PATH (the fake pocketd needs it)"
[ -x "$HOME/.cargo/bin/cargo" ] || command -v cargo >/dev/null && ok "cargo present" || fail "cargo not on PATH"
[ -f "$root/.ui-review/fixture/fixture.ts" ] && ok "fixture at .ui-review/fixture/fixture.ts" || fail "fixture.ts missing"
if [ -x "$bin" ]; then
  # The string only exists in a build without the capture feature, where screenshots fail.
  if grep -q "rebuild with --features capture" "$bin"; then fail "binary built without --features capture"; else ok "binary has capture: $bin"; fi
  newer=$(find "$root/packages/desktop/crates" -name '*.rs' -newer "$bin" | head -3)
  [ -z "$newer" ] && ok "binary newer than every crate source" || warn "sources changed since build (drive.sh rebuilds): $(echo $newer | tr '\n' ' ')"
else
  warn "no binary yet; drive.sh builds it (~4 min cold)"
fi
[ -z "${POCKETD_SOCK:-}" ] && ok "POCKETD_SOCK unset" || warn "POCKETD_SOCK=$POCKETD_SOCK points at a live pocketd; drive.sh unsets it"
for f in $(find "$ev" -name run.pid 2>/dev/null); do
  set -- $(cat "$f")
  for p in $1 ${3:-}; do kill -0 "$p" 2>/dev/null && warn "run still alive: pid $p from $f (cleanup.sh $ev)"; done
  [ -d "${2:-/nonexistent}" ] && warn "scratch dir left: $2 (cleanup.sh $ev)"
done
exit $bad
