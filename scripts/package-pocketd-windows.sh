#!/bin/sh
# Cross-builds pocketd for Windows with zig cc into pocketd-windows-<arch>.zip and its .sha256,
# which the desktop's bootstrap.ps1 downloads.
# Usage: scripts/package-pocketd-windows.sh <version> [amd64|arm64 ...]    default both; out: packages/pocketd/dist
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/packages/pocketd/dist"
VERSION="${1:?usage: package-pocketd-windows.sh <version> [amd64|arm64 ...]}"
shift
[ $# -gt 0 ] || set -- amd64 arm64
mkdir -p "$OUT"
for arch in "$@"; do
  case "$arch" in
    amd64) triple=x86_64-windows-gnu ;;
    arm64) triple=aarch64-windows-gnu ;;
    *) echo "package-pocketd-windows: unknown arch $arch (want amd64 or arm64)" >&2; exit 2 ;;
  esac
  "$ROOT/scripts/build-ghostty.sh" "windows-$arch"
  stage=$(mktemp -d)
  (cd "$ROOT/packages/pocketd" && GOOS=windows GOARCH="$arch" CGO_ENABLED=1 CC="zig cc -target $triple" go build -trimpath \
    -ldflags "-s -w -X main.version=$VERSION -X pocketd/internal/config.channel=release" -o "$stage/pocketd.exe" ./cmd/pocketd)
  zip="pocketd-windows-$arch.zip"
  rm -f "$OUT/$zip"
  (cd "$stage" && zip -q "$OUT/$zip" pocketd.exe)
  rm -r "$stage"
  (cd "$OUT" && shasum -a 256 "$zip" > "$zip.sha256")
  echo "$OUT/$zip"
done
