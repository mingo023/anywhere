# Timing: one timeline drives picture and sound

## The beat

`src/timeline.json` is read by `kit.tsx` and by `scripts/audio.py`, so a click drawn on a frame and its sound are the same number. At 128 BPM and 60fps a beat is 28.125 frames. Components get `b` (the current beat, fractional) and `ms` (milliseconds, for the app's own spinner periods); never hand-convert to frames.

```json
{
  "bpm": 128, "fps": 60, "beats": 68, "drop": 5, "groove": [5, 53],
  "clicks": [7.5, 8.75, …],
  "typing": [{ "at": 9, "text": "gpu-box", "cps": 11 }, …],
  "jitter": [1.0, 0.72, 1.25, …],
  "steps": [13, 15, …], "ticks": [40], "ready": 21, "blink": 60,
  "hits": [5, 21, 46, 54], "whoosh": [4.4, 53.3]
}
```

| Key | Effect |
|---|---|
| `drop` | The window drops in and the groove starts |
| `groove` | The span where kick, clap, hats, bass and arp all play |
| `clicks` | The pointer presses and a click sounds |
| `typing` | Each character lands at `at` plus the summed `jitter[i] / cps` seconds. `typed(b, i)` returns the text so far; the audio adds one key sound per character |
| `steps`, `ticks` | Soft tick sounds; `steps` also completes the install checklist |
| `ready` | A chime |
| `blink` | The mascot blinks, with a blip |
| `hits` | Kick plus a crash-like boom: the drop, the payoff, the end card |
| `whoosh` | Swept noise into transitions |

Keep a loading state at least one beat. Keep typing at 20 cps or slower: at 24 cps the keys come 27ms apart and start to sound like a rattle. `checks.py` reports the closest gap.

## Cues inside screens

Each screen file has a `CUE` object of beats (`CUE.connect`, `CUE.send`…). Animate with `ramp(b, from, dur)` (0→1, ease-out-expo) and `press(b, clicks)` for buttons. A cue that is also a click must use the same number as `timeline.json`.

## Camera

`CAMERA` in `Video.tsx` is a list of `[beat, focusX, focusY, zoom]` in window points; `track()` eases between keys.
- **Full view:** `[720, 450, 1.15]`.
- **Macro typing shots:** zoom 1.9–2.1.

Hold a key for as long as the action takes, and move between keys in about a beat. At zoom z the frame shows `1920/z × 1080/z` points; keep the focus far enough in that the window's edge doesn't show unless that's intended.

## Pointer

`POINTER` holds `[beat, x, y]` keys in window points, converted through the camera so it stays on its target while the camera moves. Arrive about 0.3 beats before a click and hold until just after it. `pointerOpacity` fades it out when it isn't doing anything.

## Audio

`scripts/audio.py` writes two files:
- `public/music.wav`: a placeholder bed in F major colour (Fmaj7 G6 Am7 Em7). It has an intro with an arp, a riser into the drop, and the groove, which puts a kick on every beat. The end card plays half-time and fades over the last two beats.
- `public/sfx.wav`: keys, clicks, ticks, chime, whooshes and the blink blip.

The user can swap `music.wav` for a licensed track at the same BPM; the sfx stay in sync. Final loudness is set at export (−16 LUFS), so the files are only peak-normalised.
