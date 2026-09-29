---
id: 7
title: Choosing how pocketd detects agents in a shell
labels: [wayfinder:grilling]
status: closed
assignee: mingo023
blocked_by: [1, 2, 3, 4]
---

## Question

Given the research, how does pocketd detect and attach to a `claude`/`codex` the user types by hand? Candidates: `PATH` shim, global hooks, PTY foreground-process tracking, shell integration, or a mix. Settle:

- the primary and fallback mechanisms, per status in "The session status set";
- how to avoid stuck state: herdr dropped claude/codex hooks as a state source because stale reports left panes stuck in blocked or working. Do screen rules back up or correct hook and app-server state?
- which capabilities (approvals, timeline, sending prompts) are lost versus a Pocket-spawned agent, and when;
- where the plugin or hooks live, and how shells and terminals outside Pocket stay untouched;
- claude in an untrusted dir, where hooks are skipped silently: fall back, warn, or both;
- codex modes that bypass the daemon (`exec`, `-p`, `-c`, `--no-daemon`): is a rollout-only session acceptable;
- how to map a codex thread to its PTY when two start in the same cwd at once;
- the codex "originator" label that pocketd's connection stamps on the user's own threads;
- account switching via `CLAUDE_CONFIG_DIR`/`CODEX_HOME` (trust per account, one daemon per `CODEX_HOME`).

## Resolution

Decided live with the user. superset (github.com/superset-sh/superset) was also compared: it uses a PATH wrapper and global hooks, and clears status on Esc/Ctrl+C.

**Three layers.**

| Layer | Source | Role |
|---|---|---|
| Presence | PTY foreground process (`TIOCGPGRP` + kqueue, argv match), see [Knowing which process a PTY runs on macOS](04-pty-foreground-process.md) | The only authority on "an agent runs in this terminal". Covers both providers, attached or not. When the process leaves the foreground, the agent and all its status end. |
| State, claude | Plugin hooks: SessionStart, UserPromptSubmit, PreToolUse/PostToolUse(+Failure), PermissionRequest, Notification, Stop, StopFailure, SessionEnd | Authority on status. SessionStart binds `session_id` and `transcript_path`, and rebinds on `/clear`, resume and fork. |
| State, codex | App-server `thread/status/changed` (and `turn/*` once subscribed), see [Attaching pocketd to a `codex` the user types in a shell](03-codex-launch-injection.md) | Authority on status. The state is held by the server, so it is not inferred from events. |

**Stuck state (superset style, no screen rules).**
- Presence resets everything on exit or crash.
- pocketd sees all PTY input. Esc or Ctrl+C while a claude agent is Working or Needs you sets it to Idle.
- The next hook (PostToolUse, PermissionRequest, UserPromptSubmit) re-asserts the real state, so a wrong clear heals itself.
- Codex needs neither fix: interrupts arrive as `turn_aborted`.

**Where hooks live.**
- pocketd ships a plugin dir. It exports `CLAUDE_CODE_PLUGIN_DIRS` (appended to any value the user already has), `POCKETD_SOCK` and `POCKETD_PTY` into the env of Pocket PTYs only.
- No global config is edited, and no PATH shim is used. This works for every `CLAUDE_CONFIG_DIR`.
- The hook exits 0 at once when `POCKETD_PTY` is unset or the socket is down.
- State is keyed by `session_id`, not by PTY alone, because a claude started from Claude's Bash tool inherits the env.
- Strip `CLAUDE_CODE_CHILD_SESSION` and `CLAUDECODE` from the PTY env.
- Pocket-spawned claude takes the same path: `--settings` and `--session-id` are dropped.
- If tmux in the PTY loses the env, the agent is detected but not attached.

**Not attached.** New glossary term in `CONTEXT.md`.
- A foreground claude with no SessionStart within a few seconds is not attached: untrusted dir, `--safe-mode`, or hooks disabled.
  - It shows terminal activity only, with no Needs you, Done or roll-up.
  - It shows a hint to trust the folder.
  - Pocket never writes `hasTrustDialogAccepted`.
- Codex modes that bypass the daemon (`exec`, `-p`, `-c`, `--oss`, `--no-daemon`, …) are also not attached. No rollout tailer is built.

**Codex thread → PTY.**
- pocketd keeps one persistent client per `CODEX_HOME` socket. The socket is read from the foreground process env (`KERN_PROCARGS2`) and connected lazily. After a daemon restart, pocketd reconnects and re-runs `thread/loaded/list` + `thread/resume`.
- On `thread/started`, keep root user threads only. Match `thread.cwd` to the process cwd or its `-C` flag, and `createdAt` to the process start time.
- Resume emits no `thread/started`: take the id from argv, or from `thread/status/changed` timing.
- Ambiguous case (same cwd, near-simultaneous start): defer mapping until the first `turn/started`, then assign the thread to the PTY that received Enter just before. An unprompted thread is Idle anyway, so nothing is lost.
- `codex.Dial` sends a `clientInfo.name` from `NON_ORIGINATING_CLIENT_NAMES`, so the user's threads keep their own originator. Pick the exact name from codex source when writing the plan.

**Capabilities.**
- Attached: same as Pocket-spawned.
  - Approvals work from phone, desktop or the terminal, and the first answer wins (broker `Dismiss`).
  - The timeline works. For claude it comes from SessionStart's `transcript_path`, which fixes the stale-after-`/clear` bug.
  - Prompts can be sent.
- Not attached: terminal view, typing and activity only.

**Accounts.** Claude: the plugin env is independent of `CLAUDE_CONFIG_DIR`, and trust is per account, which not attached covers. Codex: one client per `CODEX_HOME`, as above.
