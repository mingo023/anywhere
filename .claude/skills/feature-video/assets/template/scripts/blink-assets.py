"""Splits public/AppIcon.png into the face without eyes and each eye on its own layer, so the end card can blink.
Run: python3 -I scripts/blink-assets.py
"""
from pathlib import Path
from PIL import Image, ImageFilter

PUB = Path(__file__).resolve().parent.parent / "public"
im = Image.open(PUB / "AppIcon.png").convert("RGBA")
W, H = im.size
px = im.load()
X0, X1, Y0, Y1, SPLIT = 400, 670, 370, 595, 536
lum = lambda p: 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2]

core = Image.new("L", im.size, 0)
for y in range(Y0, Y1):
    for x in range(X0, X1):
        if lum(px[x, y]) > 90 and max(px[x, y][:3]) - min(px[x, y][:3]) < 40:
            core.putpixel((x, y), 255)
mask = core.filter(ImageFilter.MaxFilter(9)).load()

face = im.copy()
fp = face.load()
for y in range(Y0 - 6, Y1 + 6):
    row = [x for x in range(X0 - 6, X1 + 6) if mask[x, y]]
    for x in row:
        l = next(i for i in range(x, 0, -1) if not mask[i, y])
        r = next(i for i in range(x, W) if not mask[i, y])
        t = (x - l) / (r - l)
        fp[x, y] = tuple(round(px[l, y][c] * (1 - t) + px[r, y][c] * t) for c in range(4))
# Rows interpolated one by one leave streaks; a blur inside the hole smooths them.
hole = core.filter(ImageFilter.MaxFilter(13))
face.paste(face.filter(ImageFilter.GaussianBlur(8)), (0, 0), hole)
fp = face.load()
face.save(PUB / "AppIcon-face.png")

white = sorted((px[x, y] for y in range(450, 520) for x in range(450, 480)), key=lum)[len(range(450, 520)) * 15][:3]
for side, (a, b) in {"l": (X0 - 6, SPLIT), "r": (SPLIT, X1 + 6)}.items():
    eye = Image.new("RGBA", im.size, (0, 0, 0, 0))
    ep = eye.load()
    for y in range(Y0 - 6, Y1 + 6):
        for x in range(a, b):
            if mask[x, y]:
                body, l = lum(fp[x, y]), lum(px[x, y])
                k = max(0.0, min(1.0, (l - body) / (lum(white) - body)))
                ep[x, y] = (*white, round(255 * k))
    eye.save(PUB / f"AppIcon-eye-{side}.png")
    bb = eye.getbbox()
    print(side, bb, "centre", ((bb[0] + bb[2]) / 2 / W, (bb[1] + bb[3]) / 2 / H))
