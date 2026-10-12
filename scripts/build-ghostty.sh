#!/bin/sh
# libghostty's C API has no stable version yet, so the commit is pinned.
# Usage: build-ghostty.sh [linux-arm64|linux-amd64]; no target builds the macOS lib.
set -eu
REV=4ae9f1a2de5484de3d6a13fe03676b8853b9c41c
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIR="$ROOT/third_party/ghostty"
TARGET="${1:-}"
case "$TARGET" in
  "")
    OUT="$DIR/zig-out"
    LIB="$OUT/lib/ghostty-vt.xcframework/macos-arm64_x86_64/libghostty-vt.a"
    ;;
  linux-arm64|linux-amd64)
    OUT="$DIR/zig-out-$TARGET"
    LIB="$OUT/lib/libghostty-vt.a"
    ;;
  *)
    echo "build-ghostty: unknown target $TARGET (want linux-arm64 or linux-amd64)" >&2
    exit 2
    ;;
esac
STAMP="$OUT/.rev"
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
  echo "build-ghostty: $(readlink "$ROOT/third_party")/ghostty isn't built at $SHORT; run scripts/build-ghostty.sh $TARGET in that checkout" >&2
  exit 1
fi
if [ ! -d "$DIR/.git" ]; then
  git clone https://github.com/ghostty-org/ghostty.git "$DIR"
fi
git -C "$DIR" checkout --quiet "$REV"
cd "$DIR"
case "$TARGET" in
  # The xcframework's macOS lib is universal (arm64 + x86_64); it needs xcodebuild.
  "") zig build -Demit-lib-vt -Demit-xcframework=true -Doptimize=ReleaseFast ;;
  linux-arm64) zig build -Demit-lib-vt -Doptimize=ReleaseFast -Dtarget=aarch64-linux-gnu --prefix "$OUT" ;;
  linux-amd64) zig build -Demit-lib-vt -Doptimize=ReleaseFast -Dtarget=x86_64-linux-gnu --prefix "$OUT" ;;
esac
echo "$REV" > "$STAMP"
