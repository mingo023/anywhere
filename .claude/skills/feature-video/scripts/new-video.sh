#!/bin/bash
# new-video.sh <name> — a new Remotion project at ~/video/<name> from this skill's template, ready to render.
# The project lives outside the repo: renders, node_modules and audio stay out of git.
set -euo pipefail
name=${1:?usage: new-video.sh <name>}
skill=$(cd "$(dirname "$0")/.." && pwd)
repo=$(git -C "$skill" rev-parse --show-toplevel)
dest="$HOME/video/$name"
[ -e "$dest" ] && { echo "$dest exists; pick another name or work in it" >&2; exit 1; }
mkdir -p "$dest"
cp -R "$skill/assets/template/." "$dest/"
cd "$dest"
sed -i '' "s/__NAME__/$name/" package.json
mkdir -p public out
npm i --silent
node scripts/icons.mjs "$repo/packages/desktop/crates/theme/assets/icons"
cp "$repo/packages/desktop/assets/AppIcon.png" public/
python3 -I scripts/blink-assets.py >/dev/null
python3 -I scripts/audio.py
npx tsc --noEmit -p .
echo "ready: $dest  (npx remotion studio)"
