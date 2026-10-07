#!/bin/sh
# Builds a signed, notarized and stapled Anywhere.dmg in packages/desktop/target/release-mac (ADR 0004).
# Usage: scripts/release-mac.sh <version> [--publish]    e.g. 0.1.0
# --publish needs CHANGELOG.md's section for the version, tag v<version> at HEAD, a clean tree, gh logged in
# and the Sparkle private key (ANYWHERE_SPARKLE_KEY, or the login Keychain).
# Signs with ANYWHERE_SIGN_ID, default the keychain's first "Developer ID Application" identity.
# Notarizes with the notarytool keychain profile ANYWHERE_NOTARY_PROFILE (default "anywhere"),
# or, when ANYWHERE_NOTARY_KEY (path to a .p8) is set, with that App Store Connect API key,
# ANYWHERE_NOTARY_KEY_ID and ANYWHERE_NOTARY_ISSUER.
# ANYWHERE_SKIP_NOTARIZE=1 skips notarizing for a local check of the bundle.
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DESKTOP="$ROOT/packages/desktop"
TARGET="${CARGO_TARGET_DIR:-$DESKTOP/target}"
OUT="$DESKTOP/target/release-mac"
APP="$OUT/Anywhere.app"
DMG="$OUT/Anywhere.dmg"
REPO=mingo023/anywhere
# Sparkle's EdDSA public key (generate_keys -p). Every shipped build trusts it, so it never changes.
SPARKLE_PUBLIC_KEY=/NXXAgxXzG2c2Z1qs9ZuJ4g+xu/qRV7JzAJeFWIp+GA=

fail() {
  echo "release-mac: $*" >&2
  exit 1
}

build() {
  "$ROOT/scripts/build-ghostty.sh"
  for triple in aarch64-apple-darwin x86_64-apple-darwin; do
    (cd "$DESKTOP" && ANYWHERE_CHANNEL=release ANYWHERE_VERSION="$VERSION" cargo build --profile dist -p pocket --target "$triple")
  done
  lipo -create -output "$OUT/pocket-desktop" \
    "$TARGET/aarch64-apple-darwin/dist/pocket-desktop" "$TARGET/x86_64-apple-darwin/dist/pocket-desktop"
  for arch in arm64 amd64; do
    (cd "$ROOT/packages/pocketd" && CGO_ENABLED=1 GOARCH="$arch" go build -trimpath \
      -ldflags "-s -w -X main.version=$VERSION -X pocketd/internal/config.channel=release" -o "$OUT/pocketd-$arch" ./cmd/pocketd)
  done
  lipo -create -output "$OUT/pocketd" "$OUT/pocketd-arm64" "$OUT/pocketd-amd64"
  rm "$OUT/pocketd-arm64" "$OUT/pocketd-amd64"
}

assemble() {
  mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
  cp "$OUT/pocket-desktop" "$APP/Contents/MacOS/pocket-desktop"
  cp "$DESKTOP/assets/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"
  cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>dev.mingo.anywhere</string>
  <key>CFBundleName</key><string>Anywhere</string>
  <key>CFBundleExecutable</key><string>pocket-desktop</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
</dict>
</plist>
PLIST
  plutil -lint "$APP/Contents/Info.plist"
  helper="$APP/Contents/Library/Helpers/AnywhereDaemon.app"
  mkdir -p "$helper/Contents/MacOS" "$helper/Contents/Resources" "$APP/Contents/Library/LaunchAgents"
  cp "$OUT/pocketd" "$helper/Contents/MacOS/pocketd"
  cp "$DESKTOP/assets/AppIcon.icns" "$helper/Contents/Resources/AppIcon.icns"
  sed "s/@VERSION@/$VERSION/g" "$DESKTOP/assets/AnywhereDaemon-Info.plist.in" > "$helper/Contents/Info.plist"
  cp "$DESKTOP/assets/dev.mingo.anywhere.pocketd.plist" "$APP/Contents/Library/LaunchAgents/"
  plutil -lint "$helper/Contents/Info.plist" "$APP/Contents/Library/LaunchAgents/dev.mingo.anywhere.pocketd.plist"
  bundle_sparkle
}

# Sparkle's framework and its update settings. Not sandboxed, so its XPC services aren't needed.
bundle_sparkle() {
  "$ROOT/scripts/fetch-sparkle.sh"
  fw="$APP/Contents/Frameworks/Sparkle.framework"
  mkdir -p "$APP/Contents/Frameworks"
  ditto "$ROOT/third_party/sparkle/Sparkle.framework" "$fw"
  rm -rf "$fw/XPCServices" "$fw/Versions/B/XPCServices"
  /usr/libexec/PlistBuddy \
    -c "Add :SUFeedURL string https://github.com/$REPO/releases/latest/download/appcast.xml" \
    -c "Add :SUPublicEDKey string $SPARKLE_PUBLIC_KEY" \
    -c "Add :SUEnableAutomaticChecks bool true" \
    -c "Add :SUAutomaticallyUpdate bool true" \
    -c "Add :SUScheduledCheckInterval integer 21600" \
    "$APP/Contents/Info.plist"
}

# Inside out, as Sparkle's docs list it, before the app's own signature seals the bundle.
sign_sparkle() {
  fw="$APP/Contents/Frameworks/Sparkle.framework"
  codesign --force --timestamp --options runtime --sign "$SIGN_ID" "$fw/Versions/B/Autoupdate"
  codesign --force --timestamp --options runtime --sign "$SIGN_ID" "$fw/Versions/B/Updater.app"
  codesign --force --timestamp --options runtime --sign "$SIGN_ID" "$fw"
}

sign() {
  sign_sparkle
  codesign --force --options runtime --timestamp --sign "$SIGN_ID" "$APP/Contents/Library/Helpers/AnywhereDaemon.app"
  codesign --force --timestamp --options runtime \
    --entitlements "$DESKTOP/assets/Anywhere.entitlements" --sign "$SIGN_ID" "$APP"
  codesign --verify --deep --strict --verbose=2 "$APP"
}

make_dmg() {
  stage="$OUT/dmg"
  mkdir -p "$stage"
  ditto "$APP" "$stage/Anywhere.app"
  ln -s /Applications "$stage/Applications"
  # LZMA: about a quarter smaller than UDZO; mounts on macOS 10.15+.
  hdiutil create -volname Anywhere -srcfolder "$stage" -format ULMO -ov "$DMG"
  rm -rf "$stage"
  codesign --force --timestamp --sign "$SIGN_ID" "$DMG"
}

notarize() {
  if [ "${ANYWHERE_SKIP_NOTARIZE:-}" = 1 ]; then return; fi
  if [ -n "${ANYWHERE_NOTARY_KEY:-}" ]; then
    set -- --key "$ANYWHERE_NOTARY_KEY" --key-id "$ANYWHERE_NOTARY_KEY_ID" --issuer "$ANYWHERE_NOTARY_ISSUER"
  else
    set -- --keychain-profile "${ANYWHERE_NOTARY_PROFILE:-anywhere}"
  fi
  xcrun notarytool submit "$DMG" "$@" --wait
  # A rejected submission leaves no ticket, so stapling fails; `xcrun notarytool log <id>` says why.
  xcrun stapler staple "$DMG"
  spctl -a -vvv -t install "$DMG"
}

# This version's CHANGELOG section, for the GitHub release and the appcast. It runs first so a missing section fails before a long build.
release_notes() {
  mkdir -p "$OUT"
  awk -v v="## $VERSION" '$0 == v { on = 1; next } on && /^## / { exit } on' "$ROOT/CHANGELOG.md" 2>/dev/null | sed '/./,$!d' > "$OUT/Anywhere.md"
  if [ ! -s "$OUT/Anywhere.md" ]; then
    echo "release-mac: CHANGELOG.md has no ## $VERSION section; run scripts/changelog-draft.sh $VERSION and edit it" >&2
    exit 1
  fi
}

# A published build is exactly the tagged commit.
check_tag() {
  tag="v$VERSION"
  if [ "$(git -C "$ROOT" rev-parse -q --verify "$tag^{commit}")" != "$(git -C "$ROOT" rev-parse HEAD)" ]; then
    echo "release-mac: tag $tag must exist and point at HEAD" >&2
    exit 1
  fi
  if [ -n "$(git -C "$ROOT" status --porcelain --untracked-files=no)" ]; then
    echo "release-mac: commit or stash changes before publishing" >&2
    exit 1
  fi
}

# The DMG goes up as a GitHub release; a stable one is also added to the appcast every installed copy polls.
# Sparkle signs the stapled DMG, so this runs after notarize.
publish() {
  tag="v$VERSION"
  case "$VERSION" in
    *-*)
      gh release create "$tag" "$DMG" --repo "$REPO" --verify-tag --prerelease --title "Anywhere $VERSION" --notes-file "$OUT/Anywhere.md"
      return
      ;;
  esac
  feed="$OUT/feed"
  rm -rf "$feed"
  mkdir -p "$feed"
  # generate_appcast adds to the items already in the folder's appcast, so start from the live one, if a stable release came before.
  if git -C "$ROOT" tag -l 'v*' | grep -v -- - | grep -vqx "$tag"; then
    gh release download --repo "$REPO" --pattern appcast.xml --dir "$feed"
  fi
  cp "$DMG" "$feed/Anywhere.dmg"
  cp "$OUT/Anywhere.md" "$feed/Anywhere.md"
  set -- --download-url-prefix "https://github.com/$REPO/releases/download/$tag/" --embed-release-notes --maximum-deltas 0 --link "https://github.com/$REPO/releases"
  # CI passes the private key in the environment; on the maintainer's Mac it's in the login Keychain.
  if [ -n "${ANYWHERE_SPARKLE_KEY:-}" ]; then
    printf '%s' "$ANYWHERE_SPARKLE_KEY" | "$ROOT/third_party/sparkle/bin/generate_appcast" --ed-key-file - "$@" "$feed"
  else
    "$ROOT/third_party/sparkle/bin/generate_appcast" "$@" "$feed"
  fi
  gh release create "$tag" "$DMG" "$feed/appcast.xml" --repo "$REPO" --verify-tag --latest --title "Anywhere $VERSION" --notes-file "$OUT/Anywhere.md"
}

main() {
  usage="usage: scripts/release-mac.sh <version> [--publish], e.g. 0.1.0"
  case "${1:-}" in
    [0-9]*.[0-9]*.[0-9]*) VERSION=$1 ;;
    *) echo "$usage" >&2; exit 2 ;;
  esac
  PUBLISH=0
  case "${2:-}" in
    --publish) PUBLISH=1 ;;
    "") ;;
    *) echo "$usage" >&2; exit 2 ;;
  esac
  SIGN_ID="${ANYWHERE_SIGN_ID:-$(security find-identity -v -p codesigning | awk '/"Developer ID Application/ { print $2; exit }')}"
  [ -n "$SIGN_ID" ] || fail "no Developer ID Application identity in the keychain; set ANYWHERE_SIGN_ID"
  # Matches LSMinimumSystemVersion; rustc, cc and cgo all read it.
  export MACOSX_DEPLOYMENT_TARGET=13.0
  rm -rf "$OUT"
  mkdir -p "$OUT"
  release_notes
  if [ "$PUBLISH" = 1 ]; then check_tag; fi
  build
  assemble
  sign
  make_dmg
  notarize
  if [ "$PUBLISH" = 1 ]; then publish; fi
  echo "release-mac: $DMG"
}

main "$@"
