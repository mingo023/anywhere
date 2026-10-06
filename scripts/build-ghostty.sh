#!/bin/sh
# libghostty's C API has no stable version yet, so the commit is pinned.
set -eu
REV=4ae9f1a2de5484de3d6a13fe03676b8853b9c41c
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIR="$ROOT/third_party/ghostty"
LIB="$DIR/zig-out/lib/ghostty-vt.xcframework/macos-arm64_x86_64/libghostty-vt.a"
STAMP="$DIR/zig-out/.rev"
SHORT=$(echo "$REV" | cut -c1-7)
if [ -f "$LIB" ] && [ "$(cat "$STAMP" 2>/dev/null)" = "$REV" ]; then
  echo "ghostty ok (cached $SHORT)"
  exit 0
fi
# A linked third_party belongs to another checkout whose lib others link against: read it, never rebuild it.
if [ -L "$ROOT/third_party" ]; then
  if [ -f "$LIB" ] && [ "$(git -C "$DIR" rev-parse HEAD 2>/dev/null)" = "$REV" ]; then
    echo "ghostty ok (shared $SHORT)"
    exit 0
  fi
  echo "build-ghostty: $(readlink "$ROOT/third_party")/ghostty isn't built at $SHORT; run scripts/build-ghostty.sh in that checkout" >&2
  exit 1
fi
if [ ! -d "$DIR/.git" ]; then
  git clone https://github.com/ghostty-org/ghostty.git "$DIR"
fi
git -C "$DIR" checkout --quiet "$REV"
cd "$DIR"
# The xcframework's macOS lib is universal (arm64 + x86_64); it needs xcodebuild.
zig build -Demit-lib-vt -Demit-xcframework=true -Doptimize=ReleaseFast
echo "$REV" > "$STAMP"
