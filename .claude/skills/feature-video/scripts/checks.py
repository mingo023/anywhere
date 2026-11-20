"""checks.py <video.mp4> — the checks a cut must pass before it's shown, run from the video project.

- light: the brightest frame's mean luma (a Graphite Dark video has none above ~60)
- keys: per typing run in src/timeline.json, distinct key onsets in public/sfx.wav against its characters
- tail: the audio's peak over the last 0.1s (a hard cut shows here; it should be near silence)
"""
import json, math, struct, subprocess, sys, tempfile, wave
from pathlib import Path
from PIL import Image, ImageStat

video = sys.argv[1]
tl = json.loads(Path("src/timeline.json").read_text())
beat = 60 / tl["bpm"]
tmp = Path(tempfile.mkdtemp())

subprocess.run(["npx", "remotion", "ffmpeg", "-loglevel", "error", "-i", video, "-vf", "scale=480:-1", "-r", "4", str(tmp / "f%04d.png")], check=True)
luma = max((ImageStat.Stat(Image.open(f).convert("L")).mean[0], f.name) for f in tmp.glob("f*.png"))
print(f"light: brightest frame mean luma {luma[0]:.0f} ({luma[1]}, 4 per second)")

w = wave.open("public/sfx.wav")
sr, n = w.getframerate(), w.getnframes()
x = struct.unpack(f"<{n}h", w.readframes(n))
for t in tl["typing"]:
    s, times = t["at"] * beat, []
    for i in range(len(t["text"])):
        times.append(s)
        s += tl["jitter"][i % len(tl["jitter"])] / t["cps"]
    win = int(sr * 0.002)
    seg = x[int(times[0] * sr) : int((times[-1] + 0.1) * sr)]
    env = [max(abs(v) for v in seg[i : i + win]) for i in range(0, len(seg), win)]
    peak, ons, armed = max(env), 0, True
    for e in env:
        if armed and e > 0.35 * peak:
            ons, armed = ons + 1, False
        elif e < 0.12 * peak:
            armed = True
    gap = min(b - a for a, b in zip(times, times[1:])) if len(times) > 1 else 0
    print(f"keys: {t['text'][:24]!r} {len(times)} chars, {ons} onsets, closest {gap * 1000:.0f}ms apart")

subprocess.run(["npx", "remotion", "ffmpeg", "-loglevel", "error", "-i", video, "-vn", "-ac", "1", "-ar", "48000", str(tmp / "a.wav")], check=True)
a = wave.open(str(tmp / "a.wav"))
m = a.getnframes()
y = struct.unpack(f"<{m}h", a.readframes(m))
tail = max(abs(v) for v in y[-4800:]) or 1
print(f"tail: last 0.1s peaks at {20 * math.log10(tail / 32768):.0f} dBFS")
