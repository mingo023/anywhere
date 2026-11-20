"""Writes public/music.wav and public/sfx.wav from src/timeline.json, so beats, clicks and keystrokes land on the frames that show them.

A placeholder bed; swap music.wav for a licensed 128 BPM track and keep sfx.wav.
Run: python3 -I scripts/audio.py
"""
import json, math, random, struct, wave
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
tl = json.loads((ROOT / "src/timeline.json").read_text())
BEAT = 60 / tl["bpm"]
TOTAL = tl["beats"] * BEAT + 1.5
rnd = random.Random(3)


def hz(m):
    return 440 * 2 ** ((m - 69) / 12)


class Track:
    def __init__(self, sr):
        self.sr = sr
        self.out = [0.0] * int(TOTAL * sr)

    def add(self, t0, dur, fn):
        a, b = max(0, int(t0 * self.sr)), min(len(self.out), int((t0 + dur) * self.sr))
        for i in range(a, b):
            self.out[i] += fn((i - int(t0 * self.sr)) / self.sr)

    def write(self, name, peak_db):
        peak = max(abs(v) for v in self.out)
        k = 10 ** (peak_db / 20) / peak
        with wave.open(str(ROOT / "public" / name), "w") as w:
            w.setnchannels(1)
            w.setsampwidth(2)
            w.setframerate(self.sr)
            w.writeframes(b"".join(struct.pack("<h", int(v * k * 32767)) for v in self.out))


def noise_fn(seed, lp):
    """White noise through a one-pole low-pass of coefficient `lp` (0–1, higher is brighter)."""
    r = random.Random(seed)
    s = [0.0]

    def f():
        s[0] += lp * (r.uniform(-1, 1) - s[0])
        return s[0]

    return f


# ---------- music ----------
M = Track(32000)
CHORDS = [[53, 57, 60, 64], [55, 59, 62, 64], [57, 60, 64, 67], [52, 55, 59, 62]]  # Fmaj7 G6 Am7 Em7
ROOTS = [41, 43, 45, 40]
g0, g1 = tl["groove"]
END = 54


def kick(t0, gain):
    def f(t):
        ph = 2 * math.pi * (48 * t + (130 - 48) * (1 - math.exp(-t * 32)) / 32)
        return gain * math.exp(-t * 8) * math.sin(ph)

    M.add(t0, 0.4, f)


def hat(t0, gain, dur=0.045):
    n = noise_fn(rnd.random(), 0.95)
    prev = [0.0]

    def f(t):
        x = n()
        y, prev[0] = x - prev[0], x
        return gain * math.exp(-t * 90) * y

    M.add(t0, dur, f)


def clap(t0, gain):
    n = noise_fn(rnd.random(), 0.5)
    M.add(t0, 0.25, lambda t: gain * (math.exp(-t * 22) + 0.6 * math.exp(-((t - 0.012) % 0.011) * 300) * (t < 0.03)) * n())


def bass(t0, note, gain, dur):
    def f(t):
        env = min(1, t / 0.005) * math.exp(-t * 4)
        x = 2 * math.pi * hz(note - 12) * t
        return gain * env * (math.sin(x) + 0.25 * math.sin(2 * x))

    M.add(t0, dur, f)


def pluck(t0, note, gain):
    M.add(t0, 0.35, lambda t: gain * math.exp(-t * 11) * (math.sin(2 * math.pi * hz(note) * t) + 0.3 * math.sin(4 * math.pi * hz(note) * t)))


def pad(t0, dur, notes, gain):
    def f(t):
        env = min(1, t / 0.3) * min(1, max(0, (dur - t) / 0.4))
        return gain * env * sum(math.sin(2 * math.pi * hz(m) * t) + 0.3 * math.sin(2 * math.pi * hz(m) * 1.004 * t + 1) for m in notes) / len(notes)

    M.add(t0, dur, f)


def boom(t0, gain):
    kick(t0, gain)
    n = noise_fn(rnd.random(), 0.6)
    M.add(t0, 1.6, lambda t: gain * 0.35 * math.exp(-t * 2.6) * n())


def riser(t0, dur, gain):
    n = noise_fn(11, 0.05)

    def f(t):
        p = t / dur
        return gain * p * p * (0.7 * n() + 0.3 * math.sin(2 * math.pi * (220 + 1300 * p * p) * t))

    M.add(t0, dur, f)


for bar in range(0, tl["beats"], 4):
    k = (bar // 4) % 4
    g = 0.06 if bar < g0 else 0.09
    pad(bar * BEAT, 4 * BEAT + 0.3, CHORDS[k], g)

for b in range(tl["beats"]):
    t, k = b * BEAT, (b // 4) % 4
    arp = CHORDS[k] + [CHORDS[k][1] + 12]
    if b < g0:
        pluck(t, arp[b % len(arp)] + 12, 0.05)
        pluck(t + BEAT / 2, arp[(b + 2) % len(arp)] + 12, 0.04)
    elif b < g1 - 1:
        kick(t, 0.9)
        hat(t + BEAT / 2, 0.16)
        hat(t + BEAT / 4, 0.05)
        hat(t + 3 * BEAT / 4, 0.05)
        if b % 2 == 1:
            clap(t, 0.22)
        bass(t + BEAT / 2, ROOTS[k], 0.3, BEAT / 2)
        for j in range(2):
            pluck(t + j * BEAT / 2, arp[(2 * b + j) % len(arp)] + 12, 0.06)
    elif END <= b < END + 8:
        if b % 2 == 0:
            kick(t, 0.6)
        pluck(t, arp[b % len(arp)] + 12, 0.045)

riser((tl["drop"] - 2) * BEAT, 2 * BEAT, 0.22)
riser((END - 2) * BEAT, 2 * BEAT, 0.2)
for b in tl["hits"]:
    boom(b * BEAT, 1.0)
pad(tl["beats"] * BEAT - 4 * BEAT, 4 * BEAT + 1.4, CHORDS[0], 0.1)
# Fades out with the picture over the last two beats.
end = tl["beats"] * BEAT
for i in range(len(M.out)):
    t = i / M.sr
    if t > end - 2 * BEAT:
        M.out[i] *= max(0.0, (end - t) / (2 * BEAT)) ** 2
M.write("music.wav", -3)

# ---------- sfx ----------
S = Track(44100)


def charbeats(at, text, cps):
    s, out = at * BEAT, []
    for i in range(len(text)):
        out.append(s)
        s += tl["jitter"][i % len(tl["jitter"])] / cps
    return out


def key(t0):
    n = noise_fn(rnd.random(), 0.55)
    pitch = rnd.uniform(0.85, 1.2)
    gain = rnd.uniform(0.7, 1.0)
    S.add(t0, 0.05, lambda t: gain * (0.6 * math.exp(-t * 260) * n() + 0.5 * math.exp(-t * 120) * math.sin(2 * math.pi * 210 * pitch * t)))


def click(t0):
    n = noise_fn(rnd.random(), 0.9)
    S.add(t0, 0.03, lambda t: 0.9 * math.exp(-t * 400) * n() + 0.5 * math.exp(-t * 160) * math.sin(2 * math.pi * 2300 * t))
    S.add(t0 + 0.07, 0.02, lambda t: 0.45 * math.exp(-t * 500) * n())


def tick(t0):
    S.add(t0, 0.08, lambda t: 0.45 * math.exp(-t * 55) * math.sin(2 * math.pi * 1760 * t))


def chime(t0):
    for m, d in [(88, 0.0), (95, 0.09)]:
        S.add(t0 + d, 1.2, lambda t, m=m: 0.4 * math.exp(-t * 3.5) * (math.sin(2 * math.pi * hz(m) * t) + 0.2 * math.sin(2 * math.pi * hz(m) * 2.76 * t)))


def whoosh(t0):
    n = noise_fn(5, 0.1)
    dur = 0.8

    def f(t):
        p = t / dur
        return 0.5 * math.sin(math.pi * p) ** 2 * n()

    S.add(t0, dur, f)


for ty in tl["typing"]:
    for c in charbeats(ty["at"], ty["text"], ty["cps"]):
        key(c)
for c in tl["clicks"]:
    click(c * BEAT)
for b in tl["steps"] + tl["ticks"]:
    tick(b * BEAT)
chime(tl["ready"] * BEAT)


def blip(t0):
    S.add(t0, 0.12, lambda t: 0.3 * math.exp(-t * 30) * math.sin(2 * math.pi * (1400 - 500 * min(1, t / 0.06)) * t))


blip(tl["blink"] * BEAT - 0.05)
for b in tl["whoosh"]:
    whoosh(b * BEAT)
S.write("sfx.wav", -1)
print(f"{tl['beats']} beats, {TOTAL:.1f}s")
