---
title: Self-started agents in Pocket shells, and per-session status
labels: [wayfinder:map]
status: closed
---

## Destination

A spec, ready for `/write-plans`, that settles how a `claude`/`codex` the user starts by hand inside a Pocket terminal (a pocketd-owned PTY in some branch/worktree) appears in the session list automatically, and how every session shows a herdr-style status (needs attention, waiting, running, done).

## Notes

- Domain: pocketd (Go, `packages/pocketd`) owns the PTYs. Desktop (Rust gpui, `packages/desktop`) groups sessions into project/worktree by cwd. Phone speaks protocol v2.
- Read first: `docs/spike-remote-sessions.md`, `docs/plans/2026-09-25-pocketd-remote-sessions.md`.
- Current state:
  - pocketd only knows an agent it spawned itself (`pocketd run`, New Session form). It picks the provider from `filepath.Base(cmd)` (`internal/daemon/daemon.go:39`).
  - `claude` typed in a shell tab is invisible.
  - Desktop polls `list` every 1s.
  - Card status is NeedsYou/Working/Failed/Done, computed in `pocket/src/main.rs:327`.
- Status reference: herdr (herdr.dev; 0.9.0 installed at `/opt/homebrew/bin/herdr`). The user wants herdr-like statuses.
- Every session: call the Skill tool for "grilling" and "domain-modeling". Write code and docs in English.
- Tracker: local markdown, see `docs/wayfinder/README.md`.

## Decisions so far

- [How herdr detects agents and their state](01-herdr-agent-state.md): foreground pgrp + argv to detect; screen rules for state; idle/working/blocked/unknown, `done` = idle not yet seen.
- [Knowing which process a PTY runs on macOS](04-pty-foreground-process.md): `TIOCGPGRP` + kqueue proc events, ms latency, no permissions; OSC 133 for command/exit; match claude by argv/exe, not kernel name.
- [Attaching pocketd to a `claude` the user types in a shell](02-claude-launch-injection.md): env `CLAUDE_CODE_PLUGIN_DIRS` plugin hooks, bind via SessionStart; no PATH shim; hooks map to running/attention/waiting; untrusted dirs skip hooks.
- [Attaching pocketd to a `codex` the user types in a shell](03-codex-launch-injection.md): plain codex already uses the shared app-server; listen to thread/* and match thread to PTY by process cwd+time; daemon-bypass modes fall back to rollout.
- [How terminal, agent and card relate when an agent runs in a shell](05-terminal-agent-card-model.md): card = session (top terminal + panes), aggregated over its agents; keeps last agent title; worktree fixed at creation; agent = process, `/clear` switches conversation.
- [The session status set](06-session-status-set.md): Needs you > Done > Working > Idle; failed is a mark on Done; one global seen flag in pocketd; agentless terminals show activity, no roll-up.
- [Choosing how pocketd detects agents in a shell](07-detection-mechanism.md): foreground process = presence; claude plugin hooks via PTY env + codex app-server = state; Esc/Ctrl+C clears, next hook heals; no hooks → "not attached", activity only; codex same-cwd tie broken by Enter before first turn.
- [How the ops socket and protocol v2 carry agents, status and seen](08-protocol-status-seen.md) (agent-decided): protocol v3; agent has own id + `terminalId`; status computed in pocketd with `failed`/`attached`; `agent.view` sets per connection = seen; `foreground` event for activity; hook op gains terminal id + ancestor pids.
- [How each surface draws status](09-rendering.md) (agent-decided): glossary labels everywhere; Done gets an accent mark; roll-ups skip agentless/not-attached; inbox = Needs you + Done; desktop-only system notifications, suppressed when seen.

## Not yet specified

Nothing left; the remaining questions are open tickets.

## Out of scope

- Agents started in terminals outside Pocket (Ghostty, iTerm): Pocket owns no PTY there. Ruled out while charting.
- Agent CLIs other than claude/codex (gemini, amp, opencode…). The spec only leaves room to add providers later.
