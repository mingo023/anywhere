#!/bin/sh
# Usage: capture.sh <dir> <name>=<steps> … — builds, then captures each screen against the design scenario it depicts.
set -e
root=$(cd "$(dirname "$0")/../.." && pwd)
~/.cargo/bin/cargo build -q --release -p pocket --features capture --manifest-path "$root/packages/desktop/Cargo.toml"
# The design renders with -webkit-font-smoothing: antialiased; CoreGraphics' stroke dilation would thicken every glyph.
defaults write pocket-desktop AppleFontSmoothing -int 0
trap 'defaults delete pocket-desktop AppleFontSmoothing' EXIT
out=$1; shift
mkdir -p "$out"
for spec in "$@"; do
  case ${spec%%=*} in
    compact-sidebar-open|repositories-worktrees) scenario=worktrees ;;
    *) scenario=sessions ;;
  esac
  fx=$(mktemp -d)
  log="$fx.log"
  bun "$root/.ui-review/fixture/fixture.ts" "$scenario" "$fx" > "$log" 2>&1 &
  pid=$!
  until grep -q ready "$log"; do kill -0 $pid; sleep 0.2; done
  fx=$(cd "$fx" && pwd -P)
  # Local 14:00, so the design's "Earlier today" group holds whatever hour the capture runs.
  h=$(date -u +%H)
  tz="FXT$(( 12 - (50 - 10#$h) % 24 ))"
  TZ=$tz HOME="$fx/home" POCKET_HOME="$fx/pocket" "$root/packages/desktop/target/release/pocket-desktop" --capture "$out" "$spec"
  kill $pid
done
