#!/bin/sh
# Sparkle's release tarball, pinned by version and checksum: the framework the app bundles and the tools that sign updates.
set -eu
VERSION=2.10.0
SHA256=c2bf58aa8387266ac179357b1415d6f2635f044da8be41042af32425dae6da0c
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIR="$ROOT/third_party/sparkle"
STAMP="$DIR/.version"
if [ -d "$DIR/Sparkle.framework" ] && [ "$(cat "$STAMP" 2>/dev/null)" = "$VERSION" ]; then
  echo "sparkle ok ($VERSION)"
  exit 0
fi
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
curl -fsSL -o "$TMP/sparkle.tar.xz" "https://github.com/sparkle-project/Sparkle/releases/download/$VERSION/Sparkle-$VERSION.tar.xz"
echo "$SHA256  $TMP/sparkle.tar.xz" | shasum -a 256 -c - >/dev/null
rm -rf "$DIR"
mkdir -p "$DIR"
tar -xJf "$TMP/sparkle.tar.xz" -C "$DIR"
echo "$VERSION" > "$STAMP"
echo "sparkle $VERSION"
