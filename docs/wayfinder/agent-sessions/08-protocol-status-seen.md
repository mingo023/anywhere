---
id: 8
title: How the ops socket and protocol v2 carry agents, status and seen
labels: [wayfinder:grilling]
status: closed
assignee: mingo023
blocked_by: [7]
---

## Question

What changes on the ops socket and protocol v2 so that desktop and phone get agents, statuses and "seen"? Settle:

- the status field on `AgentSummary`: today it is `idle|running|compacting|closed`; it becomes Needs you/Done/Working/Idle plus the failed mark;
- agent identity on the wire, now that agent id ≠ terminal id (one terminal runs several agents over its life), and how an agent points at its terminal and session;
- how desktop and phone report "seen": pane visible in a focused window, timeline open on the phone;
- push events versus the 1s `list` poll for terminal and agent changes;
- activity (foreground command, exit) for agentless terminals and for agents that are not attached;
- how "not attached" and its reason (untrusted dir, codex without daemon) reach clients;
- the new `pocketd hook` ops for the claude plugin's status hooks, next to today's PermissionRequest `hook`.

## Resolution

Decided by the agent alone. The user delegated it ("tự research rồi tự quyết") and was not present, so review it before implementing. Sources: pocketd code (`internal/proto`, `internal/ops`, `internal/agent`, `internal/daemon`), desktop `agents`/`sessions`, phone `AgentsScreen`, herdr (`agent_status` + `seen` in the server, per-client focus), and superset (host-clock `seenAt`, visible pane marks seen).

**Wire version.** Protocol goes to 3. The status values change meaning, and pocketd, desktop and phone ship from one repo, so a clean break is cheaper than compatibility.

**`AgentSummary` (v3).**

| Field | Meaning |
|---|---|
| `id` | pocketd's own UUID for the agent (one process). No longer the Claude session id. |
| `terminalId` | The terminal the agent runs in. |
| `providerSessionId` | The current conversation (Claude `session_id`, Codex thread id). Empty until known; changes on `/clear`/`/resume`. |
| `status` | `needsYou` \| `done` \| `working` \| `idle` \| `closed`. Computed in pocketd, including seen. |
| `failed` | Set with `done` when the turn errored. |
| `attached` | False: no status from the agent; clients show terminal activity and ignore `status`. |
| `compacting` | Working because of a compaction (kept for the phone's chat header). |
| others | `title`, `cwd`, `provider`, `model`, `epoch`, `maxSeq`, `createdAt`, `updatedAt` as today. |

- On a conversation switch, the agent's timeline starts a new epoch and follows the new conversation. Clients already refetch on an epoch change.
- No `reason` for not attached: `provider` is enough to pick the hint (claude → trust, codex → no app-server).

**Seen.**
- New client message `agent.view {id, agentIds}`: the full set of agents this connection shows right now. Empty when the window is unfocused or the app is backgrounded. pocketd drops a connection's set when it disconnects.
- An agent is seen while any connection has it in its set. Becoming seen turns Done into Idle; a turn that ends while seen goes straight to Idle.
- New client message `agent.seen {id, agentIds}`: a one-shot "seen" for the inbox's "Mark all read". Done → Idle; no effect on other statuses.
- Rejected: seen timestamps like superset. One flag in pocketd was already decided, and a set per connection needs no clock agreement.

**Push, not poll, where it matters.**
- Agents: `agent.update` already pushes every change.
- Terminal activity: the ops attach stream gains `{ev:"foreground", id, text}` (command line of the foreground job, empty at the prompt). `session.Info` gains `foreground` so a fresh attach starts right.
- `session.Info` gains `lastProvider`/`lastTitle`: the most recent agent in that terminal, kept after it exits (the card's faded badge).
- The 1 s `list` poll stays for the terminal list itself. It is local and cheap; spawns by this window already trigger an immediate `list`.

**Hooks on the ops socket.**
- The `hook` op keeps its shape and gains `id` (the terminal, from `POCKETD_PTY`) and `pids` (the hook process's ancestor pids).
- pocketd dispatches on `hook_event_name`. PermissionRequest still waits for the broker; every other event returns at once.
- An event counts only if the terminal's foreground agent pid is among `pids`. This drops hooks from a nested `claude` started by Claude's Bash tool, which inherits `POCKETD_PTY`.

**Naming.** pocketd's `session` package and the ops `sessions` event are renamed to `terminal`/`terminals` in their own mechanical step, so code matches `CONTEXT.md`.
