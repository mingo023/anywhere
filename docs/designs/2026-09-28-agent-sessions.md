# Design: agents started in Pocket terminals, and per-session status

Date: 2026-09-28. Charted in `docs/wayfinder/agent-sessions/map.md`; each decision's detail lives in its ticket there. Vocabulary is in `CONTEXT.md` (Terminal, Session, Agent, Attached, Conversation, Status, Needs you, Working, Done, Idle, Seen).

> **Status: implemented** in `9980d81`, amended by `b15b94e` (no last-agent fields) and `8a10124` (an agent that exits drops its session). Verified 2026-10-01 against a real `claude` through pocketd and the phone protocol; `codex` was not run live. The review note below still stands.

> **Review needed.** Tickets 01–07 were decided with the user. Tickets 08, 09 and every line marked **(agent-decided)** below were decided by the agent alone, on the user's delegation, while the user was away. Review them before implementing.

## Goal

When the user types `claude` or `codex` in a Pocket terminal (a pocketd-owned PTY), an agent appears in the session list by itself, with a herdr-like status: Needs you, Done (optionally failed), Working, Idle. Pocket-spawned agents take the same path. Only Pocket terminals; only claude and codex.

## Overview

```
PTY foreground poll ──► presence: agent appears / ends (both providers)
claude plugin hooks ──► ops `hook` ──► status + conversation binding (claude)
codex app-server broadcasts ──► watcher ──► status + thread binding (codex)
PTY input (Esc, Ctrl+C, Enter) ──► clear (claude) / thread mapping (codex)
agent.view / agent.seen (desktop, phone) ──► seen ──► Done vs Idle
```

pocketd computes every status and pushes it as `agent.update` (protocol v3). Desktop and phone only draw.

## 1. Presence: which agent runs in a terminal

Ticket 04, 07. **(agent-decided: polling instead of kqueue + OSC 133.)**

- One goroutine polls every terminal every **250 ms**:
  1. `pgid = ioctl(master, TIOCGPGRP)`, called through `pty.SyscallConn().Control`, never `Fd()`.
  2. Members of the group: sysctl `kern.proc.pgrp`, sorted by pid.
  3. For each member, argv and env from sysctl `kern.procargs2`; if that fails (root, zombie), fall back to `p_comm` from `kern.proc.pid`.
- **Foreground text** (terminal activity): empty when `pgid <= 0` or `pgid` is the terminal's own child pid (the shell at its prompt, or the agent Pocket spawned directly). Otherwise the leader's argv joined by spaces; the leader is the member whose pid equals pgid, else the first member.
- **Agent match**, first member in pid order:
  - claude: `basename(argv[0]) == "claude"`, and `argv[1]` is not a helper (`bg-*`) or a non-agent subcommand (`mcp`, `doctor`, `config`, `update`, `install`, `plugin`, `setup-token`, `migrate-installer`, `-v`, `--version`, `-h`, `--help`).
  - codex: `basename(argv[0]) == "codex"`, and `argv[1]` is not a non-agent subcommand (`login`, `logout`, `mcp`, `mcp-server`, `app-server`, `completion`, `sandbox`, `debug`, `apply`, `a`, `cloud`, `features`, `help`, `-h`, `--help`, `-V`, `--version`). This also covers npm-installed codex, whose leader is `node` with `codex` as a group member.
- An agent **is** one process: it starts when a matching pid enters the foreground and ends when it leaves (exit, crash, Ctrl-Z) or the terminal closes. Ending an agent denies its open permission requests and removes it (`closed`).
- Polling rather than kqueue + OSC 133: one code path for every shell, catches `fg`/Ctrl-Z and `exec claude`, and 250 ms is invisible next to agent turn times. Cost ≈ 25 µs per terminal per tick.

## 2. Agent model and status

Ticket 05, 06, 08.

- `AgentSummary.id` is pocketd's own UUID per agent process. `terminalId` points at its terminal. `providerSessionId` is the current conversation (Claude `session_id`, Codex thread id), empty until known.
- A conversation switch (`/clear`, `/resume`, codex `/new`) keeps the agent and restarts its timeline in a new epoch.
- pocketd holds, per agent: `phase` (idle | working | needsYou), `unseenEnd`, `failed`, `compacting`, `attached`, `seen`.

**Status machine** (every provider feeds the same methods):

| Signal | Effect |
|---|---|
| `Working()` | phase = working; clears Done and failed |
| `NeedsYou()` | phase = needsYou |
| `TurnEnded(failed)` | only if phase ≠ idle: phase = idle; if not seen, Done (with failed) |
| `Clear()` | only if phase ≠ idle: phase = idle, no Done (user interrupt) |
| `SetCompacting()` | compacting = true; phase = working if idle, remembering it started from idle |
| `Compacted()` | compacting = false; a compaction that started from idle ends like a turn |
| seen becomes true | Done and failed clear |
| `MarkSeen()` | Done and failed clear (one-shot) |

Wire status: `closed` if removed, else `needsYou` / `working` by phase, else `done` if unseen end, else `idle`. `failed` rides with `done`. A new agent starts Idle; its first idle is not Done.

**Deviation from ticket 06 (agent-decided):** a crash (exit ≠ 0) no longer marks Done ✕. An agent is its process, so when it exits the agent is gone. A Pocket-spawned agent that crashes still shows red, because its terminal exits non-zero (terminal activity). If accepted, drop "or the agent crashed" from `CONTEXT.md` › Done.

## 3. Claude: plugin hooks

Ticket 02, 07.

- pocketd writes a plugin at startup, `$POCKET_HOME/plugin/`:
  - `.claude-plugin/plugin.json`: `{"name": "anywhere", "version": "1.0.0", "description": "..."}`.
  - `hooks/hooks.json`: every event below runs `"<pocketd exe>" hook`.
- Every terminal's env (shell or agent, Pocket-spawned or not) gets:
  - `CLAUDE_CODE_PLUGIN_DIRS` = existing value + `:` + plugin dir (`:`-separated, Claude ≥ 2.1.280);
  - `POCKETD_SOCK`, `POCKETD_PTY=<terminal id>`;
  - `CLAUDECODE` and `CLAUDE_CODE_CHILD_SESSION` removed.
- Pocket-spawned claude is spawned plainly: no `--settings`, no `--session-id`, no per-session settings file.
- **Hooks are synchronous (agent-decided, against the research's `async: true`).** Separate async processes can reach pocketd out of order (a late PostToolUse after Stop would leave the agent Working). A sync hook costs ~10 ms per event; pocketd answers every non-permission event at once, with empty output (stdout of SessionStart/UserPromptSubmit would be injected into Claude's context). Timeouts: 5 s for status events, 610 s for PermissionRequest.
- `pocketd hook` exits 0 printing nothing when `POCKETD_PTY` is unset or the socket is down.
- **Which claude sent it (agent-decided refinement of ticket 08).** The hook walks its own ancestors (`kern.proc.pid` ppid chain) to the nearest process that matches the claude rule of §1, and sends that `pid` with the terminal id. pocketd counts the event only if `pid` is the terminal's current agent pid. This drops a nested `claude -p` run by Claude's Bash tool, whose nearest claude ancestor is itself. (Ticket 08 said "ancestor pids"; the nested claude's chain also contains the outer claude, so membership alone can't tell them apart.)

| Hook (matcher) | Effect |
|---|---|
| SessionStart | bind: `providerSessionId = session_id`, attached = true, model, cwd; when `transcript_path` changes, new epoch and tail the new file; `source == "compact"` → `Compacted()` |
| UserPromptSubmit | `Working()` |
| PreToolUse (`AskUserQuestion\|ExitPlanMode`) | `NeedsYou()` |
| PermissionRequest | `NeedsYou()`, then the broker as today; allow → `Working()`; deny with interrupt → `Clear()` |
| Notification (`permission_prompt\|elicitation_dialog\|elicitation_url_dialog\|agent_needs_input`) | `NeedsYou()` |
| PostToolUse, PostToolUseFailure, PermissionDenied | `Working()` |
| Stop | `TurnEnded(false)` |
| StopFailure | `TurnEnded(true)` |
| PreCompact | `SetCompacting()` (agent-decided addition) |

- **Esc (`0x1b`, or kitty `ESC[27u`) or Ctrl+C (`0x03`, or kitty `ESC[99;5u`)** written to the terminal as a whole input chunk, while the claude agent is Working or Needs you → `Clear()`. This covers desktop typing, `pocketd run`, and `agent.interrupt` (which writes Esc). Claude fires no Stop on interrupt; the next hook heals a wrong clear.
- **Not attached:** a claude agent starts attached (optimistic, no flash of "not attached" at launch). If no SessionStart arrives within **5 s**, attached = false; a later SessionStart (trust accepted) sets it back. Pocket never writes `hasTrustDialogAccepted`.
- Timeline: tail `transcript_path` from SessionStart, mapping lines as today; `dismissAnswered` keeps closing the phone's permission card when the terminal's own dialog answers.

## 4. Codex: app-server watcher

Ticket 03, 07.

- Pocket-spawned codex is spawned plainly (codex ≥ 0.157 connects to, and starts, the account daemon itself). `--remote`, `codex app-server daemon start`, the loaded-list diff and `codexLocks` go.
- **Every pocketd connection to the app-server sends `clientInfo.name = "codex_app_server_daemon"`**, a name in codex's `NON_ORIGINATING_CLIENT_NAMES` (`app-server/src/request_processors/initialize_processor.rs:19`, tag `rust-v0.158.0`), so pocketd never stamps its name as the originator of the user's threads.
- One persistent **watcher** connection per socket (`$CODEX_HOME/app-server-control/app-server-control.sock`, with `CODEX_HOME` read from the codex process env), started when the first codex agent on it appears, reconnecting every 1 s. It reads the broadcasts:
  - `thread/started {thread: {id, parentThreadId, threadSource, ephemeral, …}}`: remember threads that are not root user threads (`parentThreadId != null`, `threadSource != "user"`, or `ephemeral`).
  - `thread/status/changed {threadId, status: {type, activeFlags}}`.
  - `thread/closed {threadId}`: unbind the thread; the agent stays.
- **Mapping a thread to an agent (agent-decided simplification of ticket 07):** when an unbound thread that isn't known to be non-root turns `active`, bind it to the codex agent on the same socket that received Enter (`\r`, or kitty `ESC[13u`) most recently, within the last **3 s**, and whose bound thread (if any) isn't active. No match → ignore (another client's thread). This one rule covers new threads, `codex resume` (which emits no `thread/started`), `/new`, and two codex starting in one cwd. An unprompted thread is Idle anyway, so binding at first activity loses nothing.
- On bind: `providerSessionId = threadId`; new epoch if it replaces another thread; `codex.Open` follows the thread for timeline, title and approvals (as today, now fed status-free).
- Status for a bound thread:

| `status` | Effect |
|---|---|
| `active` + `waitingOnApproval` or `waitingOnUserInput` | `NeedsYou()` |
| `active`, no flags | `Working()` |
| `idle` | `TurnEnded(false)` |
| `systemError` | `TurnEnded(true)` |
| `notLoaded` | nothing |

- Known wart (agent-decided, accepted): the status can't tell an interrupted turn from a finished one, so an interrupted codex turn shows Done. Esc-clear is claude-only; for codex it would race the server's own status.
- **Not attached:** embedded launches that bypass the daemon: first argument `exec`/`e`; any of `--no-daemon`, `--oss`, `-p`/`--profile`, `-c`/`--config`, `--enable`, `--disable`, `--search`, `--strict-config`, `--dangerously-bypass-hook-trust`, `--remote`; or `CODEX_EXEC_SERVER_URL` in its env. No rollout tailer.

## 5. Seen

Ticket 06, 08.

- Client message `agent.view {id, agentIds}`: the full set of agents this connection shows now; empty when the window is unfocused or the app is in the background. pocketd drops a connection's set on disconnect. An agent is seen while any connection's set holds it.
- Client message `agent.seen {id, agentIds}`: one-shot, clears Done (the inbox's "Mark all read").
- Desktop sends `agent.view` = agents whose terminals are visible panes in the selected worktree's active tab, while its window is active and the Sessions screen shows; `[]` otherwise. Resent on change only.
- Phone sends `agent.view [id]` while ChatScreen is open and `AppState` is `active`; `[]` otherwise.

## 6. Wire

Ticket 08.

**Protocol v3** (`proto.Version = 3`, `PROTOCOL_VERSION = 3`, desktop hello `protocolVersion: 3`). `AgentSummary`:

| Field | JSON | Note |
|---|---|---|
| `id` | `id` | pocketd UUID |
| `terminalId` | `terminalId` | new, always set |
| `providerSessionId` | `providerSessionId` (omitempty) | now the conversation, not the agent id |
| `status` | `status` | `needsYou` \| `done` \| `working` \| `idle` \| `closed` |
| `failed` | `failed` (omitempty) | with `done` |
| `attached` | `attached` | false: clients show terminal activity, ignore `status` |
| `compacting` | `compacting` (omitempty) | phone chat header |
| unchanged | `title`, `cwd`, `provider`, `model`, `epoch`, `maxSeq`, `createdAt`, `updatedAt` | |

Client messages `agent.view` and `agent.seen`, both `{type, id, agentIds: string[]}`, answered with `ack`.

**Ops socket:**
- pocketd's `session` package becomes `terminal` (`Terminal`, `Manager`), and the `sessions` event becomes `terminals`. Mechanical, first PR.
- `Info` gains `foreground`.
- The attach stream gains `{ev: "foreground", id, text}`.
- The `hook` op gains `id` (terminal) and `pid` (nearest claude ancestor).
- The 1 s `list` poll stays.

## 7. Rendering

Ticket 09 (agent-decided). Summary; the ticket has the full list.

- Labels follow the glossary. `ui::State`: `Waiting` → `NeedsYou`, `Running` → `Working`, today's `Done(a, r)` → `Idle(a, r)`, new `Done(a, r)` (accent dot + diffstat). Failed stays red ✕.
- Desktop `Status`: `NeedsYou, Failed, Done, Working, Idle`, in urgency order. Card sections: Needs you, Done (failed first), Working, Earlier today, Earlier.
- A session card is one agent (ADR 0002): its title ("New session" until it has one), a status pill and a provider dot. No branch, tab chips or time. An exited agent's card goes with it. Agentless terminals have no card; they are tabs of their worktree.
- Worktree rows and the column title show the branch ("main" for the main worktree), never a session title.
- Rail and worktree rows roll up Needs you, then Done, then Working; agentless and not-attached terminals never roll up; Merged wins.
- Not attached: a muted "Not attached" chip, and a one-line pane banner: claude "Claude skips hooks in folders it doesn't trust. Trust this folder in Claude to see status."; codex "This codex runs without the app-server, so Pocket can't see its status."
- Inbox and bell: Needs you + Done. "Mark all read" sends `agent.seen`.
- Desktop system notifications (gpui `show_system_notification`) when an agent enters Needs you or Done while not seen; tag = agent id; dismissed when seen or when it leaves the status; click focuses the pane. No sound, no dock badge, no phone push.
- Phone: rows sorted by urgency then `updatedAt`; not-attached rows muted, "Not attached · open on your Mac", no timeline.

## 8. Shipping order

1. Rename `session` → `terminal` in pocketd and the ops event.
2. `proc` package + foreground poller + `foreground` event.
3. Protocol v3: status machine, seen, `agent.view`/`agent.seen`, TS protocol, golden files; desktop and phone bumped to v3 with a minimal status mapping.
4. Desktop: statuses, surfaces, inbox, notifications, `agent.view`, activity, not attached.
5. Phone: list, not attached, `agent.view`.
6. Presence: a claude or codex in any terminal becomes an agent (not attached until PR 7/8).
7. Claude hooks: plugin, env, hook op, binding, status, Esc clear, not-attached timer; Pocket-spawned claude goes plain.
8. Codex: non-originating name, watcher, Enter mapping, status; Pocket-spawned codex goes plain.

Clients ship before presence so they already group agents by `terminalId` when agent id and terminal id first differ.

## Out of scope

Terminals outside Pocket; other agent CLIs; phone push; dock badge; a codex rollout tailer; shell integration (OSC 133); renaming the desktop's internal `sessions` module.

## Risks

- The desktop files the plan touches have uncommitted edits by the user; the plan is written against the working tree.
- The Claude plugin manifest shape and `CLAUDE_CODE_PLUGIN_DIRS` behaviour were verified on Claude 2.1.283 only.
- Codex Enter mapping is a heuristic; a thread from another client that goes active within 3 s of an Enter in an idle Pocket codex would be mis-bound.
- After the codex app-server restarts, a bound thread's timeline is not followed again; its status still updates.
- Phone prompts to any agent, codex included, are typed into its terminal, as if the user typed them there.
