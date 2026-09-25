#!/bin/sh
# libghostty's C API has no stable version yet, so the commit is pinned.
set -eu
REV=4ae9f1a2de5484de3d6a13fe03676b8853b9c41c
DIR="$(cd "$(dirname "$0")/.." && pwd)/third_party/ghostty"
if [ ! -d "$DIR/.git" ]; then
  git clone https://github.com/ghostty-org/ghostty.git "$DIR"
fi
git -C "$DIR" checkout --quiet "$REV"
cd "$DIR"
zig build -Demit-lib-vt -Doptimize=ReleaseFast
