---
id: 2
title: Attaching pocketd to a `claude` the user types in a shell
labels: [wayfinder:research]
status: closed
assignee:
blocked_by: []
---

## Question

When the user types `claude …` in a shell whose PTY pocketd owns, how can pocketd attach to that session as it does when it spawns claude itself? Today it passes `--session-id` and `--settings` with a PermissionRequest hook, then tails the transcript (`packages/pocketd/internal/daemon/daemon.go:48-79`). Find out:

- **Injection:**
  - can a `PATH` wrapper (shim) add `--settings` and `--session-id`;
  - does any env var load settings or hooks;
  - what the `SessionStart` hook payload holds (`session_id`, `transcript_path`, `cwd`, `source`);
- **User flags:**
  - does a user `--settings` merge or override;
  - do `--resume`/`-c` conflict with `--session-id`;
  - what `-p` (no TUI) does;
  - how `CLAUDE_CONFIG_DIR` changes things;
- **Status hooks:** `Notification`, `Stop`, `UserPromptSubmit`, `PermissionRequest`, `SessionEnd`: when each fires, its payload, and whether it can drive "needs attention", "waiting" and "done".

Sources: Claude Code docs (hooks, settings, CLI flags), the installed CLI.

## Resolution

Findings: `docs/research/claude-launch-injection.md` on branch `research/claude-launch-injection` (b20929e).

- **Injection:** set `CLAUDE_CODE_PLUGIN_DIRS=<pocketd plugin>` (official, 2.1.280+) plus `POCKETD_SOCK`/`POCKETD_PTY` in the shell env. Plugin hooks merge with the user's `--settings` and settings files, and survive `-r`, `-c`, `--fork-session` and `-p`. Tested: plugin-only hooks covered a full turn, and PermissionRequest allow worked.
- **Binding:** bind a session to its PTY with SessionStart (`session_id`, `transcript_path`), not `--session-id`. `/clear` changes the id and transcript mid-PTY, so today's fixed-id tail already goes stale after `/clear`.
- **PATH shim rejected:**
  - a second `--settings` overrides the first entirely;
  - `--session-id` plus `--resume`/`-c` exits 1 unless `--fork-session` is given;
  - absolute paths and aliases bypass it.
- **Status hooks:**
  - running: UserPromptSubmit, PreToolUse, PostToolUse;
  - needs attention: PermissionRequest, which fires immediately (Notification `permission_prompt` comes about 6s later);
  - waiting: Stop, with `idle_prompt` at about 60s;
  - ended: SessionEnd, unless reason=clear;
  - an interrupt fires no Stop, so keep the transcript marker.
- **Blockers:**
  - In untrusted dirs every hook is skipped silently, and trust is stored per `CLAUDE_CONFIG_DIR`. `IS_DEMO` hides the trust dialog.
  - `--safe-mode` disables hooks.
  - pocketd must strip an inherited `CLAUDE_CODE_CHILD_SESSION`, or claude stops saving transcripts.
