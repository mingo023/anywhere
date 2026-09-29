---
id: 6
title: The session status set
labels: [wayfinder:grilling]
status: closed
assignee: mingo023
blocked_by: [1, 2, 3]
---

## Question

What statuses does a session have, and what does each mean? The user wants herdr's: needs attention, waiting, running, done. Today's are NeedsYou/Working/Failed/Done (`packages/desktop/crates/pocket/src/main.rs:46-51`). Settle:

- the status list;
- each status's definition for claude and for codex;
- an urgency order, since a session card shows its most urgent agent's status;
- how "done" differs from "idle", and what clears it (seen? new prompt?);
- where Failed and exit fit;
- the status of a plain terminal (no agent);
- whether pocketd or desktop computes status.

## Resolution

Terms are in `CONTEXT.md` under Status.

- **Statuses, in order of urgency:** Needs you > Done > Working > Idle, like herdr. Done that failed ranks above plain Done. A session card and every roll-up (rail, worktree row) show the most urgent.
- **Definitions:**

  | | claude | codex |
  |---|---|---|
  | Needs you | a permission, question or dialog is pending | `active` + `waitingOnApproval`/`waitingOnUserInput` |
  | Working | from prompt sent until Stop or interrupt | `active`, no flags |
  | Done | turn ended, not seen | `active`→`idle`, not seen |
  | Done ✕ | API error, crash (exit ≠ 0) | `systemError`, turn failed |
  | Idle | at prompt, seen | `idle`, seen |

  - A new agent starts Idle, and its first idle is not Done.
  - Needs you clears only when answered, from any client. Focusing doesn't clear it.
- **Failed:** not its own status, but a mark on Done. It clears to Idle when seen. A user interrupt (Esc) is not a failure. After the agent exits, the terminal shows shell activity again.
- **Seen:** a single flag in pocketd, set while the agent's pane is visible in a focused desktop window or its timeline is open on the phone. A turn that ends while seen never becomes Done. A new prompt also clears Done.
- **Agentless terminal:** it has activity, not status: the foreground command (`npm run dev`), at prompt, or `exited N`. That activity shows on its tab or card and never rolls up; otherwise a dev server would keep a card Working forever.
- **Computed in pocketd,** then sent to desktop and phone. The signal source (hooks, app-server, screen) is left to "Choosing how pocketd detects agents in a shell".
