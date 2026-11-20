# Fidelity: where the truth lives

The rebuilt window should match a capture of the real app closely enough that nobody can tell it apart. Take every value from source; measure only geometry from captures.

## Colours

`packages/desktop/crates/theme/src/theme.rs`, the `themed!` block. Each token is `Token::new(light, dark)` as `0xRRGGBBAA`; Graphite Dark is the second value. `D` in `kit.tsx` holds the ones used so far; add more the same way.

- The window is translucent in dark (`WINDOW` = `0x121212d9`). In the video it sits over the wallpaper with `backdrop-filter`, as macOS glass does.
- Captures from `drive.sh` carry that alpha. Flatten them on black before comparing (`pair.py` does), and render the `Window` still on black too. Never sample colours from a capture's pixels.

## Icons

`packages/desktop/crates/theme/assets/icons/*.svg` (lucide plus the app's own). `scripts/icons.mjs` turns them into `ICONS`; use `<Icon name="server" size={16} color={D.t2} />` with the file's base name. The Claude mark is `claude`, the 8-spoke spinner is `spinner`.

## Animations

Copy the timing exactly. The user notices a spinner that doesn't turn, or turns wrong.

| In the app | Source | In `kit.tsx` |
|---|---|---|
| `spinner`: 8-spoke icon turned a whole spoke at a time, 800ms, 10fps | `theme.rs` | `Spinner` |
| `gradient_spinner`: 3×3 dots, diagonal colours, pulse from the top-left, 800ms, 20fps | `theme.rs` | `GradientSpinner` |
| `sweep`: a band 0.6 labels wide crossing a working label, 2800ms, 20fps | `ui/src/ui.rs` (`lit`) | `Sweep` |
| Space switch: the sidebar list slides 24px with opacity, 220ms ease-out-quint | `pocket/src/spaces.rs` (`SLIDE`) | `SLIDE` in `Video.tsx` |

Which glyph a state uses is mapped by `tone()` in `ui/src/ui.rs`:
- **Working:** `gradient_spinner` at 11px, with the label sweeping.
- **Needs you:** amber halo dot.
- **Done:** green halo dot.
- **Failed:** `x-bold`.

A live chat turn's last row is the agent's glyph plus "Opus 5.5 working for Ns" sweeping from `TEXT_3` (`chat/fold.rs`).

## Names and labels

- **Session rows** show the session's title (`tree_label` in `sidebar.rs`); the branch is only the tooltip. Take titles from `.ui-review/fixture/fixture.ts` (`title:` fields) or write realistic ones.
- **Project badges** use `initials()` in `pocket/src/util.rs`: the last dash-separated word. The sidebar shows its first letter (`app-android` → A, `app-ios` → I, `ml-trainer` → T).
- **Host status lines** come from `settings/ssh_hosts.rs`:
  - `Connecting…` with an amber dot, and no Reconnect button while connecting;
  - `Connected · version {v} · up {uptime}`, where a fresh host shows `up 0m`.
- **Install checklist steps:** Check the host, Download pocketd, Verify the download, Unpack pocketd, Install pocketd, Set up the service, Start pocketd. Then the panel shows "{host} is ready." with "Open {host}" and "Done".

## Behaviour

Before animating any transition, find the handler and read it.

- `switch_space` (`pocket/src/spaces.rs`) closes Settings, slides the sidebar, and opens New session in the target space's first project.
- The fake pocketd logs many requests but doesn't answer them with new state: a created ticket or a started board run. No capture shows what comes after. Build those states from the code that renders them (the card, the column, the row) and the fixture's existing data, and tell the user which beats weren't seen in the real app.
- The bottom switcher (`spaces/switcher.rs`) shows a host's link dot, a colour bar under the current space, and a count of sessions waiting on you. It has no "working" mark; don't add one.

## Geometry

Window points = 2x capture pixels ÷ 2 = 1200px preview pixels × 1.2. The template's files carry the measurements for:
- the sidebar (271pt wide, search at y 58, space header at y 193, project rows 29pt apart, switcher at y 872);
- the Hosts card (x 536, w 640);
- the New session composer (534–1133 × 421–541);
- the chat column (394–1273).

Re-measure from a fresh capture whenever a screen differs.

## Captures worth having before a new video

Each one comes from a `drive.sh` run with the `dark` step first.
- **Home:** the main view of the space the story starts in.
- **Every screen the story passes through, in its end state:** for example the empty space, the folder picker, a working chat, the Tasks card.
- **One state per long transition,** to check the in-between frames against.
