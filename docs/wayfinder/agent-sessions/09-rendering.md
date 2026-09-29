---
id: 9
title: How each surface draws status
labels: [wayfinder:prototype]
status: closed
assignee: mingo023
blocked_by: [6, 7]
---

## Question

How do the desktop and the phone draw Needs you, Done, Working and Idle, the failed mark, terminal activity and "not attached"? Settle, with a rough mock per surface:

- the card, rail, worktree row and tab/split in the desktop (today `Status` in `pocket/src/main.rs:46-61`);
- the phone session list;
- the bell/inbox, and whether Needs you and Done notify (chime, native notification, dock badge), and when a notification is suppressed;
- where the "trust this folder" hint for a not-attached claude goes.

Reference: superset notifies only on Stop, permission and failure, suppresses the notification when the pane is visible and the window focused, and badges the dock with the count of workspaces that need attention.

## Resolution

Decided by the agent alone, on the user's delegation, without a live prototype. Review it before implementing. It is grounded in today's desktop views (`pocket/src/view.rs`, `ui/src/ui.rs`, `inbox.rs`), the phone's `AgentsScreen`, herdr's sidebar, and superset's notification rules. gpui 0.3.6 offers `show_system_notification` (tagged, dismissible, click handler) but no dock badge.

**Look of each status.** Theme colors exist: `WAITING` amber, `RUNNING` green, `FAILED` red, `ACCENT`.

| Status | Pill (card) | Dot/badge (rail, worktree, tab, nav) | Label |
|---|---|---|---|
| Needs you | amber dot | amber | "Needs you" (was "Waiting") |
| Done ✕ | red ✕ | red | "Failed" |
| Done | accent dot + diffstat | accent | "Done" |
| Working | green spinner | green | "Working" (was "Running") |
| Idle | diffstat only (today's Done look) | none | none |

- `ui::State` variants follow the glossary: `Waiting`→`NeedsYou`, `Running`→`Working`, today's `Done(a, r)`→`Idle(a, r)`, plus a new `Done(a, r)`.
- The desktop's `Status` becomes `NeedsYou, Failed, Done, Working, Idle`, in that order of urgency.

**Surfaces.**
- **Card list:** sections "Needs you", "Done" (failed first), "Working", then "Earlier today"/"Earlier" for Idle and agentless sessions.
- **Session card:** most urgent agent across the session's terminals. An agentless session shows its activity as the subtitle (`npm run dev`, `at prompt`, `exited 1`) and no pill. After the last agent exits, the card keeps `lastTitle` and shows the provider badge faded.
- **Rail row and worktree row:** roll up Needs you ("2 need you"), then Done ("1 done", failed counts), then Working. Merged still wins on worktree rows. Agentless and not-attached terminals never roll up.
- **Nav strip (compact layout):** shows sessions that are Needs you, Done or Working, plus the selected one. Top badge amber (Needs you) or accent/red (Done), bottom badge green (Working).
- **Tab/split lead:** an agent shows provider dot, name and status dot. An agentless terminal shows its foreground command as the label, with a green dot while a command runs, grey at the prompt, and red ✕ after a non-zero exit.
- **Not attached:** a muted "Not attached" chip where the pill would be, no roll-up. A one-line banner on top of the pane:
  - claude: "Claude skips hooks in folders it doesn't trust. Trust this folder in Claude to see status."
  - codex: "This codex runs without the app-server, so Pocket can't see its status."
- **Phone list:** one row per agent, sorted by urgency then `updatedAt`, with a status dot and label (`warn`, `error`, `ok`, `accent`, `muted`). A not-attached row is muted, reads "Not attached · open on your Mac", and doesn't open a timeline.

**Bell, inbox, notifications.**
- **Inbox:** Needs you and Done agents, not "today's finished turns". Opening one focuses its pane, which makes it seen. "Mark all read" sends `agent.seen` for the listed Done agents.
- **Bell count:** Needs you + Done.
- **Desktop notifications:** a system notification when an agent enters Needs you or Done while not seen.
  - Tag = agent id, so a later one replaces it. It is dismissed once the agent is seen or leaves the status.
  - Clicking it focuses the agent's pane.
  - Only the desktop notifies. No sound, no dock badge (no gpui API). Phone push is out of scope: there is no push infrastructure.
