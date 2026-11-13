# New session

The user describes a task in the New session composer, picks an agent, project and checkout, and sends it. A session for that agent starts in the chosen project, opens as a chat showing the prompt, and the agent's reply streams in. A row for it appears in the sidebar.

## Sub-features

- `new-open` opens the New session page from each entry point.
- `new-type` takes a typed prompt and enables the send button.
- `new-send` sends the request (`agent.create`) with the project, checkout, provider and prompt.
- `new-chat` opens the new session as a chat with the prompt and the streamed reply.
- `new-sidebar` adds the new checkout's row under its project in the sidebar.

## How to get to it (user POV)

- Press Cmd+N anywhere.
- Launch the app with no session selected: the main pane already shows the page.
- Choose the `+` next to SESSIONS in the sidebar.
- Cmd+K, then pick the New session action.

## Driving it with drive.sh

Preconditions:

- `doctor.sh` exits 0. Scenario `sessions`: `app-android` is the default project and Claude Code the default agent.
- From the repo root: `S=.claude/skills/verify-anywhere/scripts; OUT=.ui-review/shots/verify/new-session-$(date +%Y%m%d-%H%M%S)`.

- **Open and type (before).** Press Cmd+N, click the composer, type the prompt. Run `CAPTURE_KEYS="cmd-n" CAPTURE_CLICKS="792,451" CAPTURE_TYPE="Fix the parser test" $S/drive.sh sessions $OUT/before typed=key,click,type`. `impl-typed.png` shows "What should we build next?", the prompt in the composer, and the send button turned dark. `pocketd-frames.jsonl` has no `agent.create`.
- **Send (after).** Same journey, then Enter. Run `CAPTURE_KEYS="cmd-n;enter" CAPTURE_CLICKS="792,451" CAPTURE_TYPE="Fix the parser test" $S/drive.sh sessions $OUT/after sent=key,click,type,key,pause,pause,pause`. `impl-sent.png` shows a tab titled with the prompt (Claude Code), the prompt bubble on the right, "Claude worked for 1s" and the reply. The sidebar has a new selected row under `app-android` (`Local 1` for a local checkout).
- **Side effect.** Run `rg '"agent.create"' $OUT/after/pocketd-frames.jsonl`. Exactly one frame, with `"provider":"claude"`, `"prompt":"Fix the parser test"`, `"chat":true`, `project` ending in `/home/code/app-android`, and a `checkout` (`{"local":…}` or `{"new":{"name":…}}`).
- **Proof.** Cite `$OUT/before/impl-typed.png`, `$OUT/after/impl-sent.png` and the `agent.create` line.

## Gotchas

- The composer only takes typing after a click on it at (792,451). Without it the keystrokes go nowhere and the screen shows the placeholder.
- The composer draft survives the reset between specs, so typing in two specs of one run sends the prompt twice ("Fix the parser testFix the parser test"). Use separate runs for before and after.
- The checkout chip under the composer can read `Local` or `Worktree · main` depending on what ran before in the same app run. Assert the `checkout` in the frame, not the chip.
- The fixture answers every create with the same canned reply about "the transcript jumped". The reply text proves streaming, not that the agent understood.
- Capture mode doesn't write `desktop.json`, so the new session is gone on the next run.
