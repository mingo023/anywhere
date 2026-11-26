# Orchestrator P0: Core

**Goal:** Every session in pocketd runs on one SQLite store: events, projections, command receipts and an effect outbox. The timeline, the message queue and fork go through it. Today's pocketd upgrades into it in place.

**Read first:**
- `docs/plans/2026-11-16-orchestrator.md`: Decisions, Contracts → Store, Engine, Wire additions.
- `packages/pocketd/internal/skills/store.go`: the SQLite pattern.
- `packages/pocketd/internal/agent/agent.go`: `Driver`, the optional driver interfaces, `Registry`, and `OnStatus`, which runs under the agent's lock (`:131`, `:352`).
- `packages/pocketd/internal/daemon/stream.go`, `codex_chat.go`, `acp_chat.go`, `pi_chat.go`: the chat drivers.
- `packages/pocketd/internal/daemon/claude.go`, `codex.go`, `watch.go`: terminal agents (hooks, OSC 7501, transcript watch).
- `packages/pocketd/internal/timeline/timeline.go`: `Timeline.Apply`, epochs and seqs.
- `packages/pocketd/internal/daemon/restore.go`, `internal/state`, `internal/handoff`: restart and in-place upgrade.
- `packages/pocketd/internal/launch/launch.go:105` (`Launcher.Create`) and `receipts.go`.
- `packages/desktop/crates/chat/src/outgoing.rs`: today's queue, which moves here.

**Toolset:** from `packages/pocketd`, run `env -u POCKETD_SOCK go test -race -count=1 ./internal/orch/...` per package and `scripts/check.sh` at every PR boundary. Regenerate goldens with `go test ./internal/proto -update`, then `pnpm --filter @pocket/protocol test`.

---

## PR 0.1: Store

- `internal/orch/store.go`: open `state/orchestrator.db` (`0600`, WAL, one connection), numbered migrations, every table in the index.
- `Store.Tx(func(*Tx) error)`: the only write path.
- Pruning on a 1-hour ticker: events older than 1 day (keep each session's newest), receipts older than 7 days, and closed sessions 15 days after `closed_at` with their items, turns, delegations, queue, requests and transfers.
- Tests: migrations from empty and from each earlier version; one test per pruning rule; a crash mid-transaction leaves nothing.

## PR 0.2: Engine and worker

- `internal/orch/decide.go`: `decide(state, cmd)` for `session.create`, `session.close`, `message.send` (all four modes), `turn.interrupt`, `session.stop`, `session.configure`, `session.rename`, `request.answer`. Pure: plain values in, `[]Event` and `[]Effect` out.
- `engine.go`: `Dispatch` takes the per-session lock, checks receipts (the same id with a different spec hash fails with `invalid_request`), runs decide, commits events, projections, the receipt and outbox rows in one transaction, then publishes.
- `worker.go`: claims the oldest `pending` outbox row per session, with at most one `running` row per session. Runs the effect through an `Effector` interface that the daemon implements. Records `done` or `failed` and feeds the result back as a command. On start, it retires `running` rows whose agent process is gone before claiming anything new.
- `agent.create`'s `requestId`, scoped by its requester, becomes the command id. `launch/receipts.go` (in memory, 10 minutes) keeps only `worktree.create`.
- Tests: table tests on `decide`; a duplicate command id commits nothing; kill after commit, before the effect, and the effect runs after reopen; at most one running effect per session.

## PR 0.3: Chat ingest

- Each chat driver reports what it hears through `Engine.Ingest` instead of only `agent.Record`: Claude stream-json lines, Codex `turn/*` and items, ACP `session/update`, pi RPC events. `Ingest` normalizes into `turn.started`, `item.updated`, `turn.ended`, `request.opened` and `request.closed`, writes `items` with the epoch and seq that `timeline.Apply` gives today, and updates the session projection.
- Requests: `broker` asks become `requests` rows. `kind = question` for `AskUserQuestion` and ACP questions (`acp/request.go:231`), `permission` otherwise. `permission.resolve` becomes `request.answer`.
- Codex user input: `codex_chat.go` handles no user-input request today. Confirm the app-server request name against codex-cli 0.163, map it to a `question` request, and answer it over JSON-RPC. Add a fakecodex fixture.
- The registry's in-memory `Timeline` stays as the hot cache for live drafts. Projections in the store are the truth on restart.
- Tests: for each driver, a recorded fixture produces the same `proto.Item`s from the store as the old live timeline did (the old FR 09-4 test, now against the store).

## PR 0.4: Terminal ingest

- Terminal Claude: hooks and OSC 7501 statuses, and the transcript tail (`internal/claude/transcript.go`) as items.
- Terminal Codex: the rollout watch (`internal/codex/watch.go`).
- Agents that aren't attached get only `session.created` and `session.closed`.
- Prompt, interrupt and close to a terminal session become outbox effects. `pty.input` types into the PTY (`claudeKind.typeIn` and the like).
- Raw PTY output is never written.
- Tests: a fake hook sequence produces turns and statuses; a transcript fixture produces items; an unattached agent records only its launch and exit.

## PR 0.5: Reads from the store

- `agent.list`, `agent.view` and `agent.timeline` read projections and `items`. The transcript rebuild (`claude.Transcript`, codex `thread/resume` replay) runs only for a session without items, once, then writes them (lazy import).
- Restart: `Daemon.Restore` reads sessions from the store. `terminals.json` is written alongside for one release.
- `items_fts` is filled on write.
- Tests: restart with a store and no transcripts shows the same timeline; lazy import runs once.

## PR 0.6: Writes as commands, queue in pocketd

- `agent.create`, `agent.prompt`, `agent.interrupt`, `agent.send_now`, `agent.close`, `agent.settings` and `agent.pin` dispatch commands. Their handlers only translate.
- The queue moves into the store, with queue commands and the verbs `queue.list`, `queue.edit`, `queue.cancel`, `queue.reorder` and `queue.send_now`, plus the `queue.update` event. A reactor sends the next entry on `turn.ended`. A user Stop holds `origin = user` entries until the next send and cancels `wake` and `question` entries.
- Desktop: `chat/src/outgoing.rs` keeps the optimistic row but no longer holds sends. The composer queue (`pocket/src/chat/composer/queue.rs`) renders `queue.update` and edits through the verbs. Same for the phone's wire mirror (decode only).
- Wire mirrors in Go, TS and Swift ship in the same PR.
- Tests: the e2e queue order across a turn end; Stop holds the queue; edit and reorder; desktop unit tests on the queue model.

## PR 0.7: Fork through the engine

- `transfer.fork` command with a `fork.native` effect: the agent's fork args as today (`launch/argv.go:116`). It records `relationship = fork` and `parent_id`. A Claude fork runs as a chat. A Codex fork runs in the TUI (`launch/codex.go:51-52`), so it's a terminal session: its events come from P0.4 and it gets no MCP tools.
- `agent.create` with `fork` stays a `session.start`, as its fork is a conversation id, not a session. The sidebar Fork menu (`sidebar/row_menu.rs`) is unchanged.
- Tests: a fork records lineage; the existing fork e2e tests still pass.

## PR 0.8: Upgrade from today

- On first start without `orchestrator.db`: read `handoff.json` (in-place upgrade) or `terminals.json`, and create a session per terminal with its conversation id, transcript path, access and launch. No history import up front.
- A chat that was running stays running without MCP until its next launch or restore.
- In-place upgrade with the outbox: the old worker stops claiming and drains `running` rows for up to 5 s before handing over; the new one sets a leftover `running` row back to `pending` only when its echo hasn't been ingested.
- e2e (release-gating): build the current `main` pocketd, start chat and terminal sessions, upgrade in place to this branch, then check every session is listed, takes a prompt and shows its history. Then reinstall the old binary and check terminals restore from `terminals.json`.
- `docs/orchestrators/product/00-prd.md`: rewrite §9 invariant 3, S-5, FR 09-2 and FR 09-4 to say conversation text is stored in `orchestrator.db`, kept 15 days after close. `CONTEXT.md` gains **Delegated session**, **Queue** and **Fork**.
