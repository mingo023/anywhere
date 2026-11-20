---
name: feature-video
description: Make a cinematic launch or feature video of the Anywhere desktop app with Remotion, by rebuilding the app's UI in React from its own source (theme tokens, icons, spinners) and driving it on a beat-locked timeline with music, typing and click sounds. Use whenever the user wants a promo, demo, showcase, launch, teaser or "tweet" video of an Anywhere feature, asks for a Remotion video of the app, or wants to show a feature "cinematic", even if they don't say Remotion or video template.
---

# Feature video (Anywhere)

The video is not a screen recording. Screenshots freeze every spinner, capture mode only shows one state per launch, and cuts between them read as choppy. Instead the app window is rebuilt in React, one component per screen, with colours, icons and animations copied from `packages/desktop`. Everything on screen is then a function of the current beat, so the whole flow plays as one continuous take that a camera can zoom into.

`assets/template/` is a finished 32-second video (Remote hosts: add a host, install pocketd, open a project on it, run an agent there, back to the Mac). Start every new video from it; most of the work is replacing its screens and timeline.

## Rules

- **Fake data only.** Reference screens come from `/verify-anywhere` against the fake pocketd and its fake repos. Never point the app, or a capture, at the user's real repos or real pocketd.
- **The project lives outside the repo,** at `~/video/<name>`. Renders, `node_modules` and audio never go into git.
- **Show only states the app can be in.** Before animating a transition, read what the code does (e.g. switching space runs `switch_space`, which opens New session in that space's first project rather than keeping the old chat). When the story needs two states at once, show them as an obviously editorial composite (two windows side by side), not as one impossible window.
- **Copy, don't invent.** Labels, names and glyphs come from source or the fixture. `references/fidelity.md` lists where each lives and the mistakes already made.

## Workflow

### 1. Agree the story

Ask what the feature is and which flow to show, end to end. The bar the user set: one continuous flow with no cuts inside the app, 30–40s, Graphite Dark, about 128 BPM, something changing on screen every one or two beats, typing with key sounds. Write the flow as beats before any code:

```
hook 0–5 · window drops in 5 · <action> 7.5 · <typing> 9–10.5 · … · tagline 49–53 · end card 54–68
```

Hold every loading state at least a beat (~0.47s at 128 BPM) so its spinner visibly turns.

Check the length before building: `beats × 60 / bpm` seconds. Under 30s, lengthen the payoff hold or the end card, not the setup.

### 2. Capture reference screens

Use `/verify-anywhere` with the `dark` step so captures are Graphite Dark. Set `TMPDIR=/tmp/va/` on `drive.sh`: the default macOS TMPDIR makes the fake host's unix socket path too long and the host shows offline. `FIXTURE_HOST_FILES=1` gives a host files for the folder picker. Keep captures in `.ui-review/shots/verify/<name>/` (gitignored).

### 3. Scaffold

```sh
.claude/skills/feature-video/scripts/new-video.sh <name>
```

It copies the template to `~/video/<name>`, installs pinned Remotion 4.0.545, generates `src/video/icons.gen.ts` from the app's icons, copies the app icon and splits it into face and eyes for the blink, writes the audio, and type-checks. Then `npx remotion studio` in that folder for live preview.

### 4. Rebuild the screens

Template map:

| File | What it is | Keep or replace |
|---|---|---|
| `src/video/kit.tsx` | Dark tokens from `theme.rs`, `Icon`, the app's `Spinner`, `GradientSpinner`, `Sweep`, `Btn`, typing helpers, `ramp`/`track` | Keep; add tokens from source as needed |
| `src/video/Sidebar.tsx` | The sidebar for This Mac and a host space, bottom switcher, space slide | Keep; change projects and sessions |
| `src/video/Hosts.tsx`, `Main.tsx` | The example's screens (Settings › Hosts, New session, folder picker, chat) | Replace with the feature's screens, reusing their pieces |
| `src/video/Video.tsx` | `AppWindow` (which screen shows at beat b), camera keys, pointer keys, wallpaper, hook, tagline, end card with blinking mascot | Keep the frame; rewrite `CAMERA`, `POINTER`, cues and copy |
| `src/timeline.json` | BPM, clicks, typing runs, step ticks, hits, whooshes | Rewrite per story; it drives picture and sound |

Every size is in window points (the window is 1440×900). Measure from a 2x capture by halving pixels. Gate each screen before building on it: render the `Window` still at a beat and compare with the capture.

```sh
npx remotion still Window /tmp/w.png --props='{"beat":21.6}'
python3 -I <skill>/scripts/pair.py /tmp/w.png <capture>.png /tmp/pair.png
```

Read `references/fidelity.md` before writing a screen.

### 5. Time it

`references/timing.md` covers the beat maths, the camera and pointer keys, and how `scripts/audio.py` turns `timeline.json` into `public/music.wav` and `public/sfx.wav`. Re-run `python3 -I scripts/audio.py` after every timeline change.

### 6. Check before showing anything

Render a quarter-scale frame sequence and read the contact sheets. Look for empty frames, cut-off framing and text too small to read.

```sh
npx remotion render Video out/seq --sequence --image-format=jpeg --scale=0.25
python3 -I <skill>/scripts/seqsheet.py out/seq /tmp/sheet 14 28.125   # half-beat steps at 128 BPM, 60fps
```

Then render 1080p and run:

```sh
python3 -I <skill>/scripts/checks.py out/<name>.mp4
<skill>/scripts/spincheck.sh out/<name>.mp4 <label> <x> <y> 30 30 <f> <f+3> <f+6>
```

- `checks.py` reports the brightest frame, key onsets against characters per typing run, and the audio tail.
- `spincheck.sh` crops one spinner across frames. Run it for every kind of animated icon in the cut, since frozen icons were the first complaint.

Audio can only be measured here, not heard: say so, and ask the user to listen to the typing.

### 7. Export

Read `references/export.md`:
- **1080p:** iterate here.
- **4K:** heavy, several times as long as 1080p; ask before starting one.
- **X:** the upload spec.

## Rules learned from corrections

| Rule | Enforced by |
|---|---|
| A sidebar session row shows the session's title, never its branch | `title()` in `Sidebar.tsx` throws on a branch-shaped label |
| A project badge letter comes from the name's last dash-separated word (`app-ios` → I) | `Project` in `Sidebar.tsx` derives it; never pass a letter |
| Switching space lands on New session in that space, not the previous chat | `Main`'s `mac` state in the template; check `switch_space` for other moves |
| Icons that load or work must animate in the cut | `spincheck.sh` on the render |
| The music fades with the picture; no hard cut | `audio.py` fades the last two beats; `checks.py` reports the tail |

When the user corrects something, fix it and add a row here, enforced by code in the template where possible.
