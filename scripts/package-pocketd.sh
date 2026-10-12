#!/bin/sh
# Builds the standalone pocketd a desktop installs on a host (crates/hosts/src/bootstrap.sh):
# packages/pocketd/dist/pocketd-<os>-<arch>.tar.gz holding pocketd-<version>-<os>-<arch>/pocketd, plus a sha256sum-format .sha256.
# Usage: scripts/package-pocketd.sh <version> [darwin-arm64 darwin-amd64 linux-arm64 linux-amd64]; no target builds all four.
# Linux cross-compiles with zig cc.
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST="$ROOT/packages/pocketd/dist"
VERSION="${1:?usage: scripts/package-pocketd.sh <version> [target...]}"
shift
[ $# -gt 0 ] || set -- darwin-arm64 darwin-amd64 linux-arm64 linux-amd64
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-13.0}"
mkdir -p "$DIST"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

for target in "$@"; do
  os=${target%-*}
  arch=${target#*-}
  case "$target" in
    darwin-arm64|darwin-amd64)
      "$ROOT/scripts/build-ghostty.sh"
      cc=cc
      ;;
    linux-arm64|linux-amd64)
      "$ROOT/scripts/build-ghostty.sh" "$target"
      case "$arch" in arm64) cc="zig cc -target aarch64-linux-gnu" ;; amd64) cc="zig cc -target x86_64-linux-gnu" ;; esac
      ;;
    *)
      echo "package-pocketd: unknown target $target" >&2
      exit 2
      ;;
  esac
  name="pocketd-$VERSION-$target"
  mkdir -p "$work/$name"
  (cd "$ROOT/packages/pocketd" && CGO_ENABLED=1 GOOS="$os" GOARCH="$arch" CC="$cc" go build -trimpath \
    -ldflags "-s -w -X main.version=$VERSION -X pocketd/internal/config.channel=release" -o "$work/$name/pocketd" ./cmd/pocketd)
  archive="pocketd-$target.tar.gz"
  # macOS tar would add ._ files and xattr headers that GNU tar warns about.
  COPYFILE_DISABLE=1 tar --no-xattrs -C "$work" -czf "$DIST/$archive" "$name"
  if command -v sha256sum >/dev/null; then sum="sha256sum"; else sum="shasum -a 256"; fi
  (cd "$DIST" && $sum "$archive" > "$archive.sha256")
  echo "package-pocketd: $DIST/$archive"
done
