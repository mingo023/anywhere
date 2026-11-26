# Orchestrator P3: Context Transfer, Titles, Schedules, Hosts

**Goal:** A session forks to another agent with its context. A schedule an agent made runs as an Automation. A delegation runs on a second machine with the desktop closed.

**Read first:**
- `docs/plans/2026-11-16-orchestrator.md`: Decisions → Group B and C.
- `packages/pocketd/internal/naming`, `internal/wsserver/retitle.go`.
- `packages/pocketd/internal/automation` (store, scheduler, launch).
- `packages/desktop/crates/hosts` (`link.rs`, `ssh.rs`, `install.rs`, `owner.rs`), `docs/plans/2026-10-12-remote-hosts.md`.

---

## PR 3.1: Context transfer

- `transfer.fork` across agents: a context block built from `items` (bounded, newest first, with tool calls summarized) is sent with the first prompt. `isolation` as for delegation, default shared.
- `transfer.merge_back`: a summary of the source since its split is sent to a related target as a context message.
- `anywhere_thread_transfer` with `fork`, `merge_back` and `status`.
- Tests: a fork Claude → Codex whose first prompt carries the block; merge_back refused for unrelated sessions.

## PR 3.2: Titles

- `anywhere_thread_update`: `rename`, and `regenerate_title` through `naming` on the first prompt plus the latest reply. Only the session title changes. Receipts per `clientRequestId`. The ceiling applies.

## PR 3.3: Schedules

- An Automation gains `createdBy` (session id), `bound` (session id), and a webhook trigger (`/hooks/<id>` on the MCP listener, optional HMAC-SHA256 header, local only).
- A bound run posts into its session through `message.send` and falls back to a new session when that one is closed.
- `schedule_task` (create or update, `enabled: false` pauses), `list_scheduled_tasks`, `delete_scheduled_task`, and `run_scheduled_task_now` (returns once started).
- The Automations page shows "Created by" and the webhook URL.
- Tests: a scheduled run in a bound session; a webhook with a bad signature is skipped.

## PR 3.4: Hosts in pocketd

- Port `hosts` link, SSH, known_hosts, sign-in and install from Rust to Go in `internal/hosts`. pocketd keeps one tunnel per host and reports state on the wire.
- The desktop's `hosts` crate becomes a view over pocketd's verbs: `host.list`, `host.add`, `host.remove`, `host.retry`, `host.update`, with `host.update` events. The wire contract lands in the index first.
- `delegate_task` and `anywhere_thread_launch` take `target.environmentId` (a host id). The engine dispatches to the host's pocketd through its tunnel and mirrors the remote session's status into the local delegation.
- `anywhere_environment_links`, `anywhere_environment_link` (a card that opens Settings › Hosts' add flow), `anywhere_environment_unlink`.
- Tests: a fake host pocketd behind a local tunnel; a delegation completes with the desktop disconnected. By hand: a real Linux host.
