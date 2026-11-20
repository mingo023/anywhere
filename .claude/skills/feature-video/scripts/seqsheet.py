"""seqsheet.py <frames-dir> <out-prefix> <step> <frames-per-beat> — contact sheets of every `step`th frame, each labelled with its beat and second.

Render the frames first: npx remotion render Video out/seq --sequence --image-format=jpeg --scale=0.25
"""
import os, sys
from PIL import Image, ImageDraw

d, out, step, fpb = sys.argv[1], sys.argv[2], int(sys.argv[3]), float(sys.argv[4])
fs = sorted(os.listdir(d))
sel = list(range(0, len(fs), step))
cell, per = (320, 180), 48
for k in range(0, len(sel), per):
    s = Image.new("RGB", (cell[0] * 8, cell[1] * 6), "black")
    for j, i in enumerate(sel[k : k + per]):
        im = Image.open(os.path.join(d, fs[i])).convert("RGB").resize(cell)
        dr = ImageDraw.Draw(im)
        dr.rectangle([0, 0, 92, 14], fill="black")
        dr.text((3, 1), f"b{i / fpb:.1f} {i / 60:.1f}s", fill="yellow")
        s.paste(im, ((j % 8) * cell[0], (j // 8) * cell[1]))
    s.save(f"{out}_{k // per}.png")
    print(f"{out}_{k // per}.png")
