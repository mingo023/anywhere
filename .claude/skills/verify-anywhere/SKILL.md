---
name: verify-anywhere
description: Drive the Anywhere desktop app (pocket-desktop, Rust/GPUI) like a user and capture proof — real keystrokes and clicks against a disposable fake pocketd, with screenshots, the frames the app sent to pocketd, and the fixture repos' git state. Use to prove a desktop change works end to end (new session, chat, palette, Changes, settings, board), not just that it compiles or unit-tests pass.
---

# Verify Anywhere (desktop)

The primary surface is the macOS desktop app in `packages/desktop` (binary `pocket-desktop`). It is driven through its own **capture mode** (`--capture`, `crates/pocket/src/capture.rs`) against the **fake pocketd** in `.ui-review/fixture/fixture.ts`, which serves the daemon's socket and websocket, seeds git repos and agents, and answers prompts like an agent would. Nothing touches the user's real pocketd, `~/.coding-pocket*`, or `$HOME`.

Not covered here: the Go daemon on its own (`packages/pocketd`, has its own e2e under `packages/pocketd/e2e`), the `anywhere` CLI (`pocketd anywhere …`), and the iOS app (`packages/ios`).

Read `features/README.md` before driving; each feature file is the recipe.

## Helpers

All three are in `scripts/` and run from any directory.

| Script | Does |
|---|---|
| `scripts/doctor.sh [evidence-root]` | Read-only readiness check. Exit 1 on any `FAIL`. |
| `scripts/drive.sh <scenario> <out-dir> <name>=<step>,… …` | Build (incremental), start a fresh fixture, drive the app, write evidence to `<out-dir>`, stop what it started, delete scratch. |
| `scripts/cleanup.sh [evidence-root]` | Stops runs that `drive.sh` left alive (it was killed mid-run) and deletes their scratch dirs. Evidence stays. |

`evidence-root` defaults to `.ui-review/shots/verify` (gitignored).

## Launch

There is no long-lived server: each `drive.sh` call is one isolated launch.

```sh
S=.claude/skills/verify-anywhere/scripts
OUT=.ui-review/shots/verify/new-session-$(date +%Y%m%d-%H%M%S)
CAPTURE_KEYS="cmd-n" CAPTURE_CLICKS="792,451" CAPTURE_TYPE="Fix the parser test" \
  $S/drive.sh sessions "$OUT" 1-typed=key,click,type
```

What it does, in order:

1. `cargo build -q --release -p pocket --features capture` in `packages/desktop`. Cold build is about 4 minutes; then seconds. Judge speed only in release.
2. Unsets `POCKETD_SOCK`: inside an Anywhere terminal it points at the user's live pocketd and would win over `POCKET_HOME`.
3. Starts `bun .ui-review/fixture/fixture.ts <scenario> <scratch>` with `FIXTURE_LOG=<out>/pocketd-frames.jsonl`, and waits for its `ready` line (30s cap). The scratch dir is `$TMPDIR/verify-anywhere.XXXXXX`, holding `home/code/<repo>` git repos and worktrees plus `pocket/` (socket, config, `desktop.json`).
4. Runs `HOME=<scratch>/home POCKET_HOME=<scratch>/pocket pocket-desktop --capture <out> <specs…>`. The window is off-screen and unfocused, 1440×900 points, light theme. It quits after the last screen.
5. Reads git state, then kills the fixture and deletes the scratch dir.

Ready means the app has printed a screenshot path per screen and exited 0. A non-zero exit or a missing `impl-<name>.png` means the run failed; `desktop.log` says why (an unknown step name lists every known one).

Scenarios (fixture data):

- `sessions`: five repos, `app-android` with six worktree sessions. "Fix stale terminal reveal" has 3 uncommitted files and an Allow-command prompt; a needs-you badge shows 3.
- `worktrees`: like `sessions`, with extra sessions per worktree and a merged branch.
- `chat`: one Claude session, "Refactor the transcript fold", with a full transcript.
- `team`: a lead with two worker sessions.
- `agents` and `agents-chat`: all ten providers installed.
- `restored`: sessions after a pocketd restart (resumed, interrupted, failed).

Every scenario also has automations, runs, and 12 board tickets.

## Doctor

Run `scripts/doctor.sh` first, and again whenever a run misbehaves. It checks: `bun` and `cargo` exist; the fixture exists; the binary exists **and was built with `--features capture`** (a build without it can't save screenshots); the binary is newer than every crate `.rs` file (warns if stale; `drive.sh` rebuilds); `POCKETD_SOCK` (warns; `drive.sh` unsets it); and leftover live runs or scratch dirs recorded in `run.pid` files.

## Drive

A spec is `<name>=<step>,<step>,…`. Each spec becomes one screenshot, `impl-<name>.png`. Before each spec the app is **reset** (overlays closed, back to the Sessions screen, light theme), so a spec must replay its whole journey from launch.

Prove things through the **input steps**. They dispatch real GPUI input events, so the input goes through the same key bindings, focus and hit-testing a user's does:

| Step | Takes its next argument from | Format |
|---|---|---|
| `key` | `CAPTURE_KEYS` | `;`-separated GPUI keystrokes: `cmd-n`, `enter`, `cmd-shift-j`, `escape` |
| `type` | `CAPTURE_TYPE` | `\|`-separated strings, typed one character at a time |
| `click` | `CAPTURE_CLICKS` | `;`-separated `x,y` in window points |
| `move` | `CAPTURE_MOVES` | `x,y` hover |
| `drop` / `drag` | `CAPTURE_DROPS` / `CAPTURE_DRAGS` | `x,y,/abs/file` (Finder drop / held drag) |
| `paste-image` | `CAPTURE_PASTE` | path to a PNG, pasted with Cmd+V |
| `pan` / `pinch` | `CAPTURE_PANS` / `CAPTURE_PINCHES` | `x,y,dx,dy` / `x,y,delta` |
| `pause` | — | one more 300ms settle |

**Arguments are consumed in order across the whole run, not per spec.** If spec 1 uses one `key` and spec 2 uses two, `CAPTURE_KEYS` holds three entries.

The reset does not clear everything. The New session composer keeps its draft from one spec to the next, so typing again doubles the text. When a journey types a draft, put its before and after states in **separate `drive.sh` runs** (`$OUT/before`, `$OUT/after`). Separate runs are cheap: about 10s each once built.

Stable handles, best first:

1. **Key bindings** (`crates/pocket/src/actions.rs`): `cmd-k` palette, `cmd-n` new session, `cmd-,` settings, `cmd-shift-a` automations, `cmd-b` sidebar, `cmd-.` focus, `cmd-j` next needs-you, `cmd-t` new tab, `cmd-d` split.
2. **The palette by name**: `cmd-k`, then type a session title, project, or a `>`-prefixed action, then `enter`. Its input focuses itself.
3. **Clicks**: window points = screenshot pixels ÷ 2. The screenshots are 2880×1800 for 1440×900 points. Measure from a fresh screenshot of the same state, and re-measure after any layout change. Text inputs other than the palette **need a click to take focus** before `type` lands.

The named, non-input steps in `capture.rs` (`session`, `chat`, `compose`, …) call internal methods directly. Use them only to reach a precondition the fixture can't provide, and never as the step that proves the feature.

Fixture knobs: prompt a chat with `/fixture permission|question|exit|clear|restart|stream-code|<chat/NAME>` to make the fake agent ask, quit or replay a transcript. Env: `FIXTURE_CHAT=<name>` (starting transcript), `FIXTURE_REFUSE=1`, `FIXTURE_REFUSE_STOP=1`, `FIXTURE_STOP_MS`, `FIXTURE_ECHO_MS`, `FIXTURE_CHUNKS`, `FIXTURE_SEND_NOW=1`, `FIXTURE_IDLE=1`, `FIXTURE_OBSERVE=1` (observe-only scopes). The header of `fixture.ts` documents each one.

## Evidence

`drive.sh` leaves everything in `<out-dir>`; use `.ui-review/shots/verify/<feature>-<timestamp>/`:

| File | Proves |
|---|---|
| `impl-<name>.png` | What the user saw after that spec's steps |
| `pocketd-frames.jsonl` | Every websocket frame the app sent to pocketd: the side effect of a send, approve, create or stop |
| `git-state.txt` | `git log -3` and `git status --short` for every fixture repo and worktree, after the app quit |
| `tree.txt` | Directories under `home/code` (worktrees created) |
| `steps.txt` | The specs and every `CAPTURE_*` / `FIXTURE_*` value, so the run can be replayed |
| `desktop.log` | The app's stdout/stderr |

Proof standards:

- Drive the user's path: key bindings, palette, clicks. Never prove a feature with a named capture step that calls an internal method.
- Capture the action *and* the result. Take one spec before the commit point (form filled, menu open) and one after it (sent, saved, opened).
- Check side effects next to pixels. A send or create must show up as its frame in `pocketd-frames.jsonl` (`agent.create`, `agent.prompt`, `permission.resolve`, `agent.interrupt`, …), with the exact text or ids. A git action must show in `git-state.txt`.
- The fake pocketd is the only mock, and it sits where production already has a process boundary: the pocketd protocol. Anything the desktop does itself (git, files, rendering) is real. The fixture never runs a real agent: it creates the worktree dir for `agent.create` but doesn't run `git worktree add`, so prove worktree creation by the request, not by `git worktree list`.
- Capture mode never writes `desktop.json` (`remember_chrome` returns early when capturing). Settings persistence across restarts can't be proved here; prove the toggle's visible state and its effect within the run.
- Read every screenshot you cite. A path printed doesn't mean the screen shows what you expect.

## Cleanup

`drive.sh` cleans up after itself on exit, Ctrl-C or failure. It kills the two pids it started (fixture, app) and deletes its scratch dir and `run.pid`. If it was killed with SIGKILL, or the session died, run:

```sh
.claude/skills/verify-anywhere/scripts/cleanup.sh            # or pass the evidence root
```

It kills only pids recorded in `run.pid` that are still `fixture.ts` or `pocket-desktop`, and removes only `verify-anywhere.*` scratch dirs. Never `pkill pocket-desktop` or `pkill bun`: the user's own Anywhere and other agents' fixtures share those names. Evidence directories are never deleted by cleanup; delete old ones by hand when no longer needed.

## Isolation

Runs are independent. Each gets its own scratch `HOME`/`POCKET_HOME`, a random fixture port (45000–49999), and its own unix socket, so parallel runs and the user's running Anywhere don't collide. Give parallel runs different `<out-dir>`s. `CAPTURE_HOLD=1` keeps the window shown and the app running after the last screen. `drive.sh` then waits forever, so don't use it with the helper.
