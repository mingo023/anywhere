#!/bin/sh
# Builds a signed Pocket.app and opens it: macOS drops notifications from a binary with no bundle id.
# The ad-hoc signature changes every build, so macOS may ask to allow notifications again.
# Usage: scripts/bundle-dev.sh [--no-open]
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ID=dev.mingo.codingpocket.desktop
OPEN=1
case "${1:-}" in
  --no-open) OPEN=0 ;;
  "") ;;
  *) echo "usage: scripts/bundle-dev.sh [--no-open]" >&2; exit 2 ;;
esac
TARGET="${CARGO_TARGET_DIR:-$ROOT/packages/desktop/target}"
(cd "$ROOT/packages/desktop" && cargo build --release -p pocket)
APP="$TARGET/release/Pocket.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp "$TARGET/release/pocket-desktop" "$APP/Contents/MacOS/pocket-desktop"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>$ID</string>
  <key>CFBundleName</key><string>Pocket</string>
  <key>CFBundleExecutable</key><string>pocket-desktop</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.0.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
</dict>
</plist>
PLIST
codesign --force --sign - "$APP"
if [ "$OPEN" = 0 ]; then
  echo "bundle-dev: built $APP"
  exit 0
fi
osascript -e "if application id \"$ID\" is running then tell application id \"$ID\" to quit" >/dev/null 2>&1 || true
LOG="${TMPDIR:-/tmp/}pocket-dev.log"
# `open` hands the app launchd's environment, not this shell's, so a scratch pocketd is passed on.
set --
for name in POCKETD_SOCK POCKET_HOME; do
  eval "value=\${$name:-}"
  if [ -n "$value" ]; then set -- "$@" --env "$name=$value"; fi
done
open -n --stdout "$LOG" --stderr "$LOG" "$@" "$APP"
echo "bundle-dev: open $APP · log $LOG"
