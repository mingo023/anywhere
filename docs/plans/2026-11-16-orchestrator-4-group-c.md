# Orchestrator P4: Background Wait, Workflow Stop, Organize

**Goal:** A Claude session stays Working while background work it declared runs. An agent stops a Claude workflow. Sessions are pinned, archived or marked unread by a tool.

**Read first:**
- `docs/plans/2026-11-16-orchestrator.md`: Decisions → Group B and C.
- `packages/pocketd/internal/timeline/timeline.go` (`Lasting`, `LastingAgents`), `internal/daemon/stream.go:435` (`interrupt`, `stop_task`).

---

## PR 4.1: Background wait and workflow stop

- `background.hold` command: adds agent-declared task ids to the session's held set. The set joins `LastingAgents` in deciding Working. A task's end notification or a 6-hour cap releases it. `waiting: false` clears it. The result returns `matched` and `unmatched`.
- `anywhere_workflow_stop`: a `stop_task` effect on the coordinator id from `anywhere_thread_read` activity.
- Agents other than Claude (and OpenCode for background wait) get `unsupported`.
- Tests: held stays Working past turn end and releases on the notification; the cap releases.

## PR 4.2: Organize

- `session.organize`: `pin` and `unpin` (today's pin), `mark_unread` (status back to Done), `archive` (close; history kept 15 days). `snooze` and `settle` return `unsupported`.
- `anywhere_thread_organize`. The ceiling applies.
- Tests: each action; archive revokes the token.
