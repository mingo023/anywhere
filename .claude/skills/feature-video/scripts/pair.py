"""pair.py <ours.png> <capture.png> <out.png> — the capture flattened on black above our `Window` still, both at 1440×900, to compare by eye.

Captures from drive.sh are translucent in dark (the window is 0x121212d9); flattening both on the same black is the only fair comparison.
"""
import sys
from PIL import Image

ours = Image.open(sys.argv[1]).convert("RGB").resize((1440, 900))
cap = Image.open(sys.argv[2]).convert("RGBA").resize((1440, 900))
bg = Image.new("RGBA", cap.size, (0, 0, 0, 255))
bg.alpha_composite(cap)
out = Image.new("RGB", (1440, 1800))
out.paste(bg.convert("RGB"), (0, 0))
out.paste(ours, (0, 900))
out.resize((1000, 1250)).save(sys.argv[3])
