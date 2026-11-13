# Changes panel

For the selected session's checkout, the right panel lists uncommitted files with their status, a commit message box with Commit All, and a commit graph. Clicking a file opens its diff.

## Sub-features

- `changes-open` opens the panel for the selected session's worktree, listing each changed file.
- `changes-diff` opens a file's diff in the main pane.
- `changes-commit` commits with a message; the repo gains the commit and the list empties.
- `changes-graph` shows the branch's commits and refs.

## How to get to it (user POV)

- Select a session, then click the branch icon (with its change-count badge) in the right rail.
- Not reached from the palette.

## Driving it with drive.sh

Preconditions:

- Scenario `sessions`. "Fix stale terminal reveal" on `fix/restore-handoff` has 3 uncommitted files (`components/terminal-view.tsx`, `hooks/index.ts`, `hooks/use-restore-preview.ts`).
- `S=.claude/skills/verify-anywhere/scripts; OUT=.ui-review/shots/verify/changes-$(date +%Y%m%d-%H%M%S)`.

- **Open the panel.** Open the session, then click the rail's branch icon. Run `CAPTURE_KEYS="cmd-k;enter" CAPTURE_TYPE="fix stale terminal" CAPTURE_CLICKS="1417,96" $S/drive.sh sessions $OUT/open panel=key,type,key,click`. `impl-panel.png` shows "fix-restore-handoff", "Changes · fix/restore-handoff", Uncommitted, a Message (⌘↩ to commit) box, Commit All, Changes 3 with the three files marked M, and a Graph with "Start fix/restore-handoff" and "Initial commit".
- **Commit.** Click the message box, type a message, press Cmd+Enter (not yet run; measure the box from `impl-panel.png`, around (1225,129)). `git-state.txt` for `code/app-android.worktrees/fix-restore-handoff` must show the new commit first and no status lines.
- **Proof.** Cite `impl-panel.png`, plus the before and after `git-state.txt` for any commit.

## Gotchas

- The panel follows the selected session. With none selected it has nothing to show, so open a session first.
- The git refresh is asynchronous. `drive.sh` settles on it between steps, but for a commit add `pause` steps before the after-screenshot.
- Git writes are real: they go to the fixture's repos under the scratch `HOME`, and the scratch is deleted at exit. `git-state.txt` is the only record.
