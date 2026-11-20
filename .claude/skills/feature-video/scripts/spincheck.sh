#!/bin/bash
# spincheck.sh <video> <name> <x> <y> <w> <h> <frame>… — crops one box from each frame (in the video's pixels) and prints how much it changes between neighbours.
# Run from the video project. 0 between frames means the animation is frozen; open /tmp/spincheck/<name>-strip.png to see it.
set -euo pipefail
v=$1; n=$2; x=$3; y=$4; w=$5; h=$6; shift 6
mkdir -p /tmp/spincheck
for f in "$@"; do
  npx remotion ffmpeg -y -loglevel error -ss "$(python3 -c "print(($f + 0.5) / 60)")" -i "$v" -frames:v 1 -vf "crop=$w:$h:$x:$y" "/tmp/spincheck/$n-$f.png"
done
python3 -I - "$n" "$@" <<'PY'
import sys
from PIL import Image, ImageChops, ImageStat
n, fs = sys.argv[1], sys.argv[2:]
ims = [Image.open(f"/tmp/spincheck/{n}-{f}.png").convert("RGB") for f in fs]
print(n, [round(sum(ImageStat.Stat(ImageChops.difference(a, b)).mean) / 3, 2) for a, b in zip(ims, ims[1:])])
o = Image.new("RGB", (ims[0].width * len(ims), ims[0].height))
for i, im in enumerate(ims):
    o.paste(im, (i * ims[0].width, 0))
o.save(f"/tmp/spincheck/{n}-strip.png")
PY
