---
id: 3
title: Attaching pocketd to a `codex` the user types in a shell
labels: [wayfinder:research]
status: closed
assignee:
blocked_by: []
---

## Question

When the user types `codex …` in a shell whose PTY pocketd owns, how can pocketd attach to that session? When pocketd spawns codex itself, it adds `--remote` to the app-server and claims the new thread by diffing the loaded thread list (`packages/pocketd/internal/daemon/codex.go:23-95`). Find out:

- **Injection:**
  - can a `PATH` shim add `--remote`;
  - can any config or env (`CODEX_HOME`, `config.toml`, `notify`, hooks) make Codex report to pocketd;
- **User commands:** `codex resume`, `codex exec`, `-C`, a user-supplied `--remote`, and an app-server daemon that isn't running;
- **Without `--remote`:** is tailing rollouts (`$CODEX_HOME/sessions/**`) enough to infer state (no approvals);
- **State:** the values of `thread/status/changed` (active, idle, `waitingOnApproval`…) and whether they map to "needs attention", "waiting", "running", "done". pocketd ignores this event today.

Sources: Codex source, app-server protocol, the installed CLI.

## Resolution

Findings: `docs/research/codex-launch-injection.md` on branch `research/codex-launch-injection` (eb18773).

- **No injection needed on Codex 0.157+:** a plain `codex` connects to, or starts, the same app-server daemon socket pocketd uses. Tested: a second client saw `thread/started` (cwd, rollout path) before any prompt.
- **Listening:** pocketd keeps one connection per socket. `thread/started`, `thread/status/changed` and `thread/closed` reach every client without subscribing; `thread/resume` once the thread is mapped.
- **No PTY id on threads:** match the PTY's foreground `codex` process (argv, cwd, `CODEX_HOME`) to the thread's cwd and creation time. Tie-breakers: the first prompt, or a TUI debug log keyed by prompt id (untested).
- **Rejected:**
  - a PATH shim adding `--remote`: argv reparsing, and `exec` refuses `--remote`;
  - hooks: they run in the daemon's env, so they can't tell which PTY they belong to.
- **Daemon bypass:** `-p`, `-c`, `--oss`, `--no-daemon` and `codex exec` skip the daemon. Only the rollout is left: it shows turn start, done, failed and interrupted, never an approval wait.
- **Status:**
  - `active` + `waitingOnApproval`/`waitingOnUserInput` → needs attention;
  - `active` → running;
  - `idle` → waiting/done;
  - `systemError` → error;
  - `notLoaded` → closed.
- **Risk:** the first client to connect sets the daemon-wide "originator" label, so the user's own threads get labelled "pocketd".
