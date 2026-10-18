#!/bin/sh
# Builds the Linux or Windows desktop: pocket-desktop with pocketd beside it, which installs itself as the
# user's service on first launch (crates/daemon/src/service/bundled.rs), packed as
# packages/desktop/target/release-<target>/Anywhere-<target>.tar.gz (.zip on Windows). Unsigned, no auto-update.
# Usage: scripts/release-desktop.sh <version> <linux-amd64|linux-arm64|windows-amd64> [--publish]    e.g. 0.1.0 linux-amd64
# Linux builds for the machine it runs on.
# Windows builds on Windows: gpui compiles its release shaders with the SDK's fxc.exe. It needs cargo-zigbuild.
# --publish needs tag v<version> at HEAD, a clean tree and gh logged in, and adds the archive to the release
# scripts/release-mac.sh creates, waiting up to two hours for it.
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DESKTOP="$ROOT/packages/desktop"
TARGET="${CARGO_TARGET_DIR:-$DESKTOP/target}"
REPO=mingo023/anywhere

fail() {
  echo "release-desktop: $*" >&2
  exit 1
}

# A published build is exactly the tagged commit.
check_tag() {
  if [ "$(git -C "$ROOT" rev-parse -q --verify "v$VERSION^{commit}")" != "$(git -C "$ROOT" rev-parse HEAD)" ]; then
    fail "tag v$VERSION must exist and point at HEAD"
  fi
  [ -z "$(git -C "$ROOT" status --porcelain --untracked-files=no)" ] || fail "commit or stash changes before publishing"
}

build() {
  "$ROOT/scripts/build-ghostty.sh" "$PLATFORM"
  case "$PLATFORM" in
    linux-*)
      (cd "$DESKTOP" && ANYWHERE_CHANNEL=release ANYWHERE_VERSION="$VERSION" cargo build --profile dist -p pocket)
      cp "$TARGET/dist/pocket-desktop" "$OUT/Anywhere/"
      cc=cc
      exe=
      ;;
    windows-amd64)
      (cd "$DESKTOP" && ANYWHERE_CHANNEL=release ANYWHERE_VERSION="$VERSION" cargo zigbuild --profile dist -p pocket --target x86_64-pc-windows-gnu)
      cp "$TARGET/x86_64-pc-windows-gnu/dist/pocket-desktop.exe" "$OUT/Anywhere/"
      # The GNU target links wry's WebView2 loader as a DLL, which Windows doesn't ship; without it the app won't start.
      cp "$(find "$TARGET/x86_64-pc-windows-gnu/dist/build" -path '*webview2-com-sys-*/out/x64/WebView2Loader.dll' | head -n 1)" "$OUT/Anywhere/"
      cc="zig cc -target x86_64-windows-gnu"
      exe=.exe
      ;;
  esac
  (cd "$ROOT/packages/pocketd" && CGO_ENABLED=1 GOOS="${PLATFORM%-*}" GOARCH="${PLATFORM#*-}" CC="$cc" go build -trimpath \
    -ldflags "-s -w -X main.version=$VERSION -X pocketd/internal/config.channel=release" -o "$OUT/Anywhere/pocketd$exe" ./cmd/pocketd)
}

pack() {
  case "$PLATFORM" in
    linux-*) ARCHIVE="$OUT/Anywhere-$PLATFORM.tar.gz"; tar -C "$OUT" -czf "$ARCHIVE" Anywhere ;;
    # Git Bash on the Windows runner has 7-Zip but no zip.
    windows-*)
      ARCHIVE="$OUT/Anywhere-$PLATFORM.zip"
      if command -v zip >/dev/null; then (cd "$OUT" && zip -qr "$ARCHIVE" Anywhere); else (cd "$OUT" && 7z a -tzip "$ARCHIVE" Anywhere >/dev/null); fi
      ;;
  esac
}

publish() {
  tries=0
  until gh release view "v$VERSION" --repo "$REPO" >/dev/null 2>&1; do
    tries=$((tries + 1))
    [ "$tries" -le 120 ] || fail "release v$VERSION never appeared; release-mac.sh creates it"
    sleep 60
  done
  gh release upload "v$VERSION" "$ARCHIVE" --repo "$REPO" --clobber
}

main() {
  usage="usage: scripts/release-desktop.sh <version> <linux-amd64|linux-arm64|windows-amd64> [--publish]"
  case "${1:-}" in
    [0-9]*.[0-9]*.[0-9]*) VERSION=$1 ;;
    *) echo "$usage" >&2; exit 2 ;;
  esac
  case "${2:-}" in
    linux-amd64|linux-arm64|windows-amd64) PLATFORM=$2 ;;
    *) echo "$usage" >&2; exit 2 ;;
  esac
  PUBLISH=0
  case "${3:-}" in
    --publish) PUBLISH=1 ;;
    "") ;;
    *) echo "$usage" >&2; exit 2 ;;
  esac
  OUT="$DESKTOP/target/release-$PLATFORM"
  rm -rf "$OUT"
  mkdir -p "$OUT/Anywhere"
  if [ "$PUBLISH" = 1 ]; then check_tag; fi
  build
  pack
  if [ "$PUBLISH" = 1 ]; then publish; fi
  echo "release-desktop: $ARCHIVE"
}

main "$@"
