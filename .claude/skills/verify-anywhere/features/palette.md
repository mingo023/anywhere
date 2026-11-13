# Command palette

Cmd+K opens a search over sessions, files and actions. Typing narrows the list; Enter opens the highlighted session, file or action. A leading `>` searches actions only.

## Sub-features

- `palette-open` opens the palette with its input focused.
- `palette-session` finds a session by title words and opens it in the main pane, selecting its sidebar row.
- `palette-action` runs an action from a `>` query (for example Board or Automations).
- `palette-empty` shows no rows for a query that matches nothing.

## How to get to it (user POV)

- Press Cmd+K.
- Click Search (⌘K) at the top of the sidebar.

## Driving it with drive.sh

Preconditions:

- Scenario `sessions`. Session "Fix stale terminal reveal" (Codex, `app-android · fix/restore-handoff`) exists.
- `S=.claude/skills/verify-anywhere/scripts; OUT=.ui-review/shots/verify/palette-$(date +%Y%m%d-%H%M%S)`.

- **Search.** Open the palette and type. Run `CAPTURE_KEYS="cmd-k" CAPTURE_TYPE="crash on resume" $S/drive.sh sessions $OUT/before query=key,type`. `impl-query.png` shows the dialog with "crash on resume", a Sessions group holding one row "Crash on resume after OTA · app-android · fix/ota-resume · Codex · Idle", and the footer "Navigate / Open / Tab to filter by project".
- **Open a session.** Run `CAPTURE_KEYS="cmd-k;enter" CAPTURE_TYPE="fix stale terminal" $S/drive.sh sessions $OUT/after open=key,type,key`. `impl-open.png` shows the tab "Fix stale terminal reveal · Codex", the terminal with "Allow command? $ pnpm test --filter app-android", and `fix/restore-handoff` selected in the sidebar.
- **Action query.** `cmd-k`, type `>board`, Enter. The Board screen opens (not yet run).
- **Proof.** Cite both screenshots. Opening a session doesn't send a create or prompt frame. Check `pocketd-frames.jsonl` has none.

## Gotchas

- Words match case-insensitively and must all appear. A typo shows nothing rather than a near match.
- The palette opens over whatever the main pane shows. Make sure the screenshot shows the palette before you cite it as the search state.
