#!/bin/sh
# Uploads the iPhone app to TestFlight and submits it to the public external group (ADR 0007).
# Usage: packages/ios/scripts/release.sh <version>    e.g. 0.2.0-beta.1, uploaded as 0.2.0: App Store versions are numbers only.
# Needs Xcode, xcodegen, CHANGELOG.md's section for the version (its "What to Test"), and an App Store Connect API key
# with the Admin role, which cloud signing needs: ANYWHERE_ASC_KEY (path to its .p8), ANYWHERE_ASC_KEY_ID, ANYWHERE_ASC_ISSUER.
# ANYWHERE_TESTFLIGHT_GROUP names the group, default "Public".
set -eu
IOS="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${1:?usage: release.sh <version>}"
MARKETING="${VERSION%%-*}"
# The UTC minute, so every upload's build number is above the last one's.
BUILD="$(date -u +%y%m%d%H%M)"
OUT="$(mktemp -d)"
: "${ANYWHERE_ASC_KEY:?}" "${ANYWHERE_ASC_KEY_ID:?}" "${ANYWHERE_ASC_ISSUER:?}"

# TestFlight caps "What to Test" at 4000 characters.
awk -v v="## $VERSION" '$0 == v { on = 1; next } on && /^## / { exit } on' "$IOS/../../CHANGELOG.md" | sed '/./,$!d' | head -c 4000 > "$OUT/notes.md"
[ -s "$OUT/notes.md" ] || { echo "release-ios: CHANGELOG.md has no ## $VERSION section" >&2; exit 1; }

set -- -allowProvisioningUpdates -authenticationKeyPath "$ANYWHERE_ASC_KEY" \
  -authenticationKeyID "$ANYWHERE_ASC_KEY_ID" -authenticationKeyIssuerID "$ANYWHERE_ASC_ISSUER"
(cd "$IOS" && xcodegen generate)
xcodebuild archive -project "$IOS/Anywhere.xcodeproj" -scheme Anywhere -configuration Release \
  -destination 'generic/platform=iOS' -archivePath "$OUT/Anywhere.xcarchive" -skipPackagePluginValidation \
  MARKETING_VERSION="$MARKETING" CURRENT_PROJECT_VERSION="$BUILD" "$@"
xcodebuild -exportArchive -archivePath "$OUT/Anywhere.xcarchive" -exportOptionsPlist "$IOS/ExportOptions.plist" \
  -exportPath "$OUT/export" "$@"

swift "$IOS/scripts/testflight.swift" dev.mingo.anywhere "$MARKETING" "$BUILD" "${ANYWHERE_TESTFLIGHT_GROUP:-Public}" "$OUT/notes.md"
