#!/bin/sh
# Renders og.html into public/assets/og.png at 2400×1260 (2× the 1200×630 layout).
set -e
cd "$(dirname "$0")"
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless --hide-scrollbars --allow-file-access-from-files \
  --window-size=1200,630 --force-device-scale-factor=2 --virtual-time-budget=4000 \
  --screenshot="$PWD/../public/assets/og.png" "file://$PWD/og.html" 2>/dev/null
