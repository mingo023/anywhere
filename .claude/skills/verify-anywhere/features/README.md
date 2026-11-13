# Anywhere desktop verification map

The maintained source for verifying what a user can do in the Anywhere desktop app. Read this index first, then use the matching feature file as the recipe. Harness details (steps, argument order, evidence files) are in `../SKILL.md`.

## Baseline preconditions

- `../scripts/doctor.sh` exits 0.
- Every run goes through `../scripts/drive.sh <scenario> <out-dir> <specs…>`, which starts its own fake pocketd and scratch `HOME`. Never point the app at the user's pocketd or `~/.coding-pocket*`.
- Evidence goes to `.ui-review/shots/verify/<feature-id>-<YYYYmmdd-HHMMSS>/`, with `before/` and `after/` subdirs when a journey needs two runs.
- The app launches on the Sessions screen at 1440×900 points, light theme, with no session selected. The main pane shows the New session page ("What should we build next?").

## Driving conventions

- Start every recipe from launch; each spec is reset before it runs.
- Prefer key bindings and the Cmd+K palette over clicks. Clicks are in window points (screenshot px ÷ 2), measured from the screenshots named in each recipe; re-measure if the layout changed.
- Click a text input before `type`, except the palette, which focuses itself.
- Keep commands literal. `CAPTURE_*` arguments are consumed in order across all specs of one run.

## Proof and skip reporting

- Cite a screenshot of the state before the commit point and one after it, and read both.
- A send/create/approve is proved by its frame in `pocketd-frames.jsonl` with the exact text or ids, not by the screen alone.
- A git change is proved by `git-state.txt`.
- Record the feature ID, entry point and out-dir with every claim.
- If a precondition is missing from the fixture, report the journey as failed with the unmet precondition and add it to that feature's Gotchas. Don't swap in a named capture step to make it pass.
- Don't report an entry point as verified through a different one.

## Feature entry contract

Each feature file has an H1 title and one paragraph of user-visible behavior, then exactly these H2s in order: `Sub-features`, `How to get to it (user POV)`, `Driving it with drive.sh` (starts with `Preconditions:`, then labelled bullets pairing a user action with an exact command and the observable result), `Gotchas`. Keep implementation detail out except stable handles.

## Features

| ID | File | Last proved |
|---|---|---|
| `new-session` | [New session](./new-session.md) | 2026-11-13, keyboard + click entry, chat start |
| `chat` | [Chat with an agent](./chat.md) | not yet run as written |
| `palette` | [Command palette](./palette.md) | open-session sub-feature, 2026-11-13 |
| `changes` | [Changes panel](./changes.md) | open-panel sub-feature, 2026-11-13 |
| `settings` | [Settings](./settings.md) | open + toggle, 2026-11-13 |

Not mapped yet: Board (`click` the sidebar's Board row, 12 seeded tickets), Automations (`cmd-shift-a`, 4 seeded automations and 8 runs), Inbox (sidebar bell, 3 needs-you), Explorer, terminal splits and tabs, team mode (`team` scenario), phone pairing.
