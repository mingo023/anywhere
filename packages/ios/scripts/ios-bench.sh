#!/bin/zsh
# Usage: scripts/ios-bench.sh [scenario…] — builds a BENCH release app, runs each scenario on the booted simulator
# (or $SIM, a UDID) and prints its IOSBENCH line. FRAMES sets the stream frames, UIBENCH_SCROLL the scroll frames.
set -e
cd ${0:a:h}/..
UDID=${SIM:-$(xcrun simctl list devices booted | grep -oE '[0-9A-F-]{36}' | head -1)}
[[ -n $UDID ]] || { echo "boot a simulator or set SIM"; exit 1; }
DD=${DERIVED:-/tmp/anywhere-ios-bench}
xcodebuild -project Anywhere.xcodeproj -scheme Anywhere -configuration Release -destination 'generic/platform=iOS Simulator' \
  -derivedDataPath $DD -skipPackagePluginValidation CODE_SIGNING_ALLOWED=NO 'SWIFT_ACTIVE_COMPILATION_CONDITIONS=$(inherited) BENCH' \
  build -quiet
xcrun simctl install $UDID $DD/Build/Products/Release-iphonesimulator/Anywhere.app
(( $# )) || set -- chat-5000 changes-1500 diff-20k file-10k markdown-258k
for s in "$@"; do
  xcrun simctl terminate $UDID dev.mingo.anywhere >/dev/null 2>&1 || true
  start=$(date "+%Y-%m-%d %H:%M:%S")
  SIMCTL_CHILD_UIBENCH_FRAMES=${FRAMES:-600} SIMCTL_CHILD_UIBENCH_SCROLL=${UIBENCH_SCROLL:-240} \
    xcrun simctl launch $UDID dev.mingo.anywhere -bench $s >/dev/null
  for i in {1..90}; do
    sleep 2
    xcrun simctl spawn $UDID launchctl list | grep -q dev.mingo.anywhere || break
  done
  xcrun simctl spawn $UDID log show --start "$start" --predicate 'eventMessage BEGINSWITH "IOSBENCH|"' --style compact 2>/dev/null \
    | grep -o 'IOSBENCH|.*' | tail -1 || echo "IOSBENCH|$s|no result"
done
