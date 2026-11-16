# Orchestrator: Plan Index and Contracts

An agent in an Anywhere chat can start, message, wait on and stop other sessions through an MCP server that pocketd hosts. Sessions stay isolated, run on any agent, and report back to the session that started them. To make that durable, every session in pocketd moves onto one SQLite store of events, projections, command receipts and an effect outbox.

Background: T3 Code at `f8c8f30e` (`docs/orchestration-v2/orchestrator-mcp-server.md`, `docs/internals/overview.md`, `packages/contracts/src/orchestratorMcp.ts`), `.ui-review/prototypes/delegated-panel.html` (UI). The design re-implements T3's orchestrator MCP in Go. No T3 code is copied.

The work ships as five plans on one branch, `orchestrator`, merged to `main` once, when every e2e test in "Testing" passes. Each plan is several PRs into that branch. A plan that needs to change a contract changes it here first.

| # | Plan | Ships | Depends on |
|---|---|---|---|
| P0 | `2026-11-16-orchestrator-0-core.md` | SQLite store, command engine, effect worker, every session's events and timeline in the store, the message queue in pocketd, fork through the engine, upgrade from today's pocketd | nothing |
| P1 | `2026-11-16-orchestrator-1-mcp.md` | MCP HTTP endpoint, per-session tokens, injection into Claude, Codex and ACP chats, tool exposure | P0 |
| P2 | `2026-11-16-orchestrator-2-core-tools.md` | Group A tools, lineage, parent wake, question routing, the access ceiling, limits, the desktop panel, delivery rows and child view | P1 |
| P3 | `2026-11-16-orchestrator-3-group-b.md` | Context transfer (fork, merge_back), title tools, schedule tools on Automations, host links in pocketd and the environment tools | P2 |
| P4 | `2026-11-16-orchestrator-4-group-c.md` | `anywhere_background_wait`, `anywhere_workflow_stop`, `anywhere_thread_organize` | P2 |

## Decisions

Settled with the owner on 2026-11-16 (grilling, 28 questions).

- **Language and place.** Go, inside pocketd: `internal/orch` (engine, store, worker), `internal/mcp` (endpoint, tokens, tools). No TypeScript, no Effect. Model logic stays out of the desktop (ADR 0003).
- **Scope of the store: every session.** Chat and terminal sessions alike live in the store: hand-opened ones too, not only what MCP starts. A terminal session records launch, exit, turn start and end, and Needs you from its hooks, OSC 7501 and transcript watch. Commands to it (prompt, interrupt, close) go through the outbox; its worker types into the PTY. Raw PTY bytes never enter the store. An agent that isn't attached records only launch and exit.
- **Conversation text is stored.** Messages, tool calls and results go into the store, as T3 does. This drops PRD §9 invariant 3 ("no conversation text on disk", `00-prd.md:56`, `:310`, FR 09-2, 09-4). P0 rewrites those lines.
- **The store is the timeline's source.** Desktop, phone and `anywhere_thread_read` read it. Chats write to it from their streams, terminal Claude and Codex from the transcripts pocketd already watches. A session from before the upgrade imports its transcript once, the first time it's opened or restored. A restart reads the store, never the transcript.
- **Retention.** A closed session stays in the store, hidden from the sidebar but readable and searchable by the tools, and is deleted 15 days after it closed. Events are kept 1 day (reconnect and replay only; projections are the truth). Command receipts are kept 7 days. Removing a project or worktree doesn't delete history early. `orchestrator.db` is `0600`.
- **Wire stays.** Clients keep today's verbs (`agent.list`, `agent.timeline` with `epoch`/`seq`, …), now answered from the store. Additions only: `parent` and delegation fields, the queue verbs, delivery timeline items. No T3-style global event stream.
- **Message queue moves into pocketd.** The desktop's held sends (`chat/src/outgoing.rs`) become pocketd queue entries in the store: pending, sending, sent, cancelled. pocketd sends the next when a turn ends. On a user Stop, entries the user queued (`origin = user`) are held until the next send releases them; wake and question notices (`origin = wake | question`) are cancelled. Clients list, edit, cancel, reorder and send-now through new verbs.
- **Fork.** The sidebar's Fork menu stays as it is. Its backend becomes the engine's fork command, which records fork lineage, so a user fork can `merge_back` later. Same agent: the agent's own fork args, as today: Claude `--resume <id> --fork-session` runs as a chat and gets MCP; a Codex fork runs in the TUI (`launch/codex.go:51`), so it is a terminal session without tools. No "Fork to…" in the menu.
- **One release, one branch.** No feature flag. Every PR lands on `orchestrator`; `main` gets it in one merge.
- **Upgrade from today.** The new pocketd reads today's `handoff.json` and `terminals.json`, creates store sessions for running terminals, and imports history lazily. A chat running across the upgrade keeps running and gets MCP from its next launch or restore. For one release pocketd also keeps writing `terminals.json`, so reinstalling the old version restores terminals. A release-gating e2e test upgrades a real current pocketd in place.

### MCP

- **Transport.** Streamable HTTP MCP at `http://127.0.0.1:<port>/mcp`. The port is picked free on first start and saved in config, so agents that outlive a restart or an in-place upgrade keep reaching it. A taken port fails loudly.
- **Credentials.** Before a chat agent launches, pocketd issues a token bound to the session, its agent kind and its launch. The store keeps the SHA-256 of the token, never the token. A token idle for 24 hours expires; closing the session revokes it. A token grants only the MCP tools, as that session. It never approves a permission ask and never reaches the WebSocket verbs. This reverses the no-self-approval rule that a process in a Terminal holds only `observe` (`docs/plans/2026-09-30-no-self-approval.md`, `peer.PTYScopes`) for the MCP tools alone: a token principal gets a new `delegate` scope, capped by the caller's access.
- **The token never goes on argv.** A chat's argv is visible in `ps` and is written to `handoff.json` (`handoff.Terminal.Args`). Codex and ACP get it over their JSON-RPC stdio. Claude gets it through a `0600` config file under `state/mcp/<session>.json`, passed by path to `--mcp-config` and removed on close, unless the P1 spike finds a stdin route. Agents run as the same macOS user, so one could still read another's file or memory; a stolen token can act only with its own session's access.
- **Injection.**
  - Claude: `--mcp-config <state/mcp/<session>.json>` (an `http` server with the `Authorization` header) and `--allowedTools mcp__anywhere__*`, added to `chatFlags` (`launch/claude.go:31`).
  - Codex chat: the server in the `thread/start` and `thread/resume` config (`daemon/codex_chat.go:206`), twice: `anywhere` (every tool but the two blocking ones) and `anywhere-direct` (only `anywhere_thread_wait` and `delegate_task`, `omit_tools_from = ["code_mode","deferred"]`).
  - ACP: an HTTP entry in `mcpServers` of `session/new` and `session/load` (`acp/session.go:289`, `:313`, `:1142`) when the agent advertises HTTP MCP. Otherwise the agent gets no tools. pocketd sends no `thread/fork` or `session/fork` today.
  - pi and omp: no tools this time. They can still be delegated to.
  - The user's own MCP servers stay.
- **Exposure.** Listed up front: `orchestrator_capabilities`, `delegate_task`, `task_status`, `tool_search`. `tool_search` loads others for the rest of the session (T3's `McpToolExposure`).
- **Bounded waits.** Every wait returns within 45 s with `waitTimedOut: true`. The caller calls again or ends its turn.
- **No `request_secret`.** Dropped. 27 tools.

### Delegation

- **Child = parent's access.** A child runs with exactly the parent's access and plan mode; the tools take no `runtimeMode` or `interactionMode`. A target agent that lacks that access steps down to the broadest narrower access it offers, and the result says which. An agent that always runs Full (`providers.NeverAsks`) can only be delegated to by a Full parent. Order: `settings` ≡ `ask` < `edits` < `auto` < `full`.
- **Ceiling on other sessions.** `anywhere_thread_send`, `anywhere_thread_configure`, `anywhere_thread_update` and `anywhere_thread_interrupt` refuse a target whose access is broader than the caller's.
- **Workspace.** `isolation: "worktree"` is the default: a new worktree and same-named branch from the parent's HEAD commit, named by `naming`. Uncommitted parent edits are not copied, and the result warns when there are some. `shared` runs in the parent's checkout. A parent outside a git repository gets `shared` only. The worktree is an ordinary one in the sidebar. Nothing auto-commits, merges or deletes it.
- **Completion wake (as T3).** The wake message carries no result: "Delegated task X reached a terminal state. Use task_status …". The result is the child's latest assistant text of its final turn, or its error. `mode: "async"` wakes the parent on completion. `mode: "wait"` returns the result through the wait; a wait that timed out after the parent's turn ended switches to waking. The wake steers into a running parent turn when the agent can steer without cutting tools (Claude, Codex), else queues; an idle parent starts a turn. Siblings that finish before the wake starts merge into one wake. A user Stop on the parent, or `task_cancel`, disposes pending wakes for good. A published result never changes; follow-ups via `anywhere_thread_send` are new turns.
- **Question routing (as T3's code).** Only questions are routed: Claude's `AskUserQuestion`, Codex user input, ACP questions. Permission asks never are. The parent gets a queued message (not steered): "Delegated task X is blocked on a question … read it with `anywhere_pending_request` … answer it when the answer follows from the task or the user's instructions; otherwise ask the user." The queued notice is cancelled when the question is answered first or the child settles. A parent in a `delegate_task` wait gets `hasPendingQuestion: true` instead. The question still shows to the user; the first answer wins.
- **Limits** (in Settings): 3 levels below a session the user opened; 6 children running per session (the 7th queues as `queued`); 12 MCP-started sessions running per Mac (user sessions don't count); `create_threads` 1–20 per call.
- **Who can be a parent.** Claude, Codex and ACP chats with HTTP MCP. Anyone can be a child.
- **`anywhere_thread_configure`** reads and sets model, effort and options of the same agent. Changing the agent is refused with a hint to fork; a Session stays one agent (CONTEXT.md). Access isn't set here.

### Group B and C

- **Context transfer.** `fork` copies a session up to a source point (default: its latest settled turn) into a new session, idle. Same agent with native fork: native. Otherwise a context block built from the store goes with the first prompt. `isolation` as for delegation, default shared. `merge_back` sends a summary of A's conversation since the split into related session B (parent–child or fork–source, same project) as a context message. It never touches git. `status` lists a session's transfers.
- **Titles.** `rename` as the app does. `regenerate_title` runs `naming` on the first prompt plus the latest reply and changes only the session's title.
- **Schedules on Automations.** No second scheduler. Agent-made schedules are Automations ("Created by <session>"), editable in the Automations page. New: bound to a session (posts into it; a closed one gets a new session), and a webhook trigger on `http://127.0.0.1:<port>/hooks/<id>` with optional HMAC, local only. `run_scheduled_task_now` returns once started.
- **Hosts move into pocketd.** pocketd keeps the SSH tunnel to each host's pocketd, so delegation to a host works with the desktop closed. The desktop goes through pocketd. `anywhere_environment_link` shows the Settings › Hosts flow as a card.
- **Organize.** `pin`/`unpin`, `mark_unread` (back to Done), `archive` (close; history kept 15 days). `snooze` and `settle` return `unsupported`.
- **`anywhere_background_wait`** adds agent-declared task ids to the held set behind `Timeline.LastingAgents()`, released on the task's end or after 6 hours. Claude and OpenCode only. **`anywhere_workflow_stop`** calls Claude's `stop_task`. Claude only.

### Desktop

- **Panel.** A floating glass panel at the chat's top right lists the chat's delegated sessions: agent logo, title, status, elapsed, and the question when one is pending. It is a capsule (logos and a summary) when collapsed. It opens on a new delegation or a question, collapses 3.5 s after all settle and when the user scrolls the chat, and offers Clear once all settle. The motion is a critically damped spring from the live value. It respects reduced motion and reduced transparency. It replaces in-chat cards.
- **Delivery rows.** Each thing pocketd told the parent is one quiet row in the tool-row voice: logo, title, `done` / `failed` (red) / `asks "…"`. Copy avoids the words in `pocket/tests/avoid_list.rs` ("finished", "running", "waiting", "unread"). Consecutive rows group. A click opens the child.
- **Child view.** Opening a child pushes its chat in from the right with a breadcrumb `‹ <parent> / <child>`; Back, `Esc` or `⌘[` returns. Its first message is the brief, marked "Delegated by <agent>". The composer and status line follow the child.
- **Sidebar.** A child is an ordinary session under its worktree. No nesting.
- **Alerts.** A child's Needs you alerts as usual, the phone included. A child's Done neither alerts nor enters Up next, since the parent consumes it.

## Contracts

### Store (`state/orchestrator.db`)

`modernc.org/sqlite` (already in `go.mod`), WAL, `SetMaxOpenConns(1)`, as `internal/skills/store.go`. Migrations are numbered SQL in `internal/orch/migrations/`.

| Table | Holds |
|---|---|
| `events` | `seq` (PK), `session_id`, `kind`, `payload` JSON, `at`. Pruned after 1 day; each session's newest kept |
| `sessions` | Projection: id, project, worktree/local, agent, account, access, plan, model, effort, options, conversation id, transcript path, chat, typed, status, failed, pinned, title, origin, `parent_id`, `relationship` (`subagent` \| `fork`), `depth`, `created_by` (`user` \| `agent` \| `automation` \| `board`), `closed_at` |
| `turns` | id, session, state (`queued` \| `starting` \| `working` \| `completed` \| `failed` \| `interrupted` \| `cancelled`), prompt message, started/ended, error |
| `items` | Timeline items (`proto.Item`) per session with `epoch`/`seq`, the text included |
| `items_fts` | FTS5 over item text and session titles, for `anywhere_thread_search` |
| `delegations` | task id, parent session, parent turn, child session, child turn, title, role, isolation, mode, status, work state, summary, error, completion wake (`settled_only` \| `always`), delivery state, client request id |
| `queue` | id, session, text, attachments, origin (`user` \| `wake` \| `question`), order, state (`pending` \| `sending` \| `sent` \| `cancelled` \| `held`), refs (task ids, request id) |
| `requests` | Pending questions and permission asks: id, session, kind (`question` \| `permission`), payload, state |
| `transfers` | id, source, target, kind (`fork` \| `merge_back`), source point, state |
| `receipts` | `command_id` (PK), spec hash, result JSON, `at`. Pruned after 7 days |
| `outbox` | id, session, kind, payload, state (`pending` \| `running` \| `done` \| `failed` \| `retired`), attempts, last error, causing command, created |
| `tokens` | SHA-256, session, agent, issued, last alive, expired, revoked |

Hosts (P3.4): `delegations` gains `environment_id` and `host_project` (`TEXT NOT NULL DEFAULT ''`). `sessions` holds a row per remote session under its tagged id, with its `items` and `turns`, written by the mirror through the Sink; its `terminal` is always empty.

### Engine

- `orch.Engine.Dispatch(cmd) (Receipt, error)`: lock the session, return an existing receipt for `cmd.ID`, call `decide(state, cmd) ([]Event, []Effect, error)`, a pure function with no `Context`, IO or clock, and commit events, projections, the receipt and outbox rows in one transaction. Then publish to the hub.
- Commands: `session.create`, `session.close`, `message.send` (mode `auto` \| `queue` \| `steer` \| `restart`), `turn.interrupt`, `session.stop`, `session.configure`, `session.rename`, `queue.edit` / `queue.cancel` / `queue.reorder` / `queue.release`, `request.answer`, `delegation.request`, `delegation.cancel`, `delegation.wake_policy`, `transfer.fork`, `transfer.merge_back`, `session.organize`, `background.hold`.
- Hosts (P3.4): `agent.launch`'s payload carries `environmentId` and `hostProject`. On a tagged lane, `turn.start`, `turn.interrupt`, `request.answer` and `agent.close` go to the host; other effects there are `unsupported`. There are no new commands or effect kinds: the remote path dispatches `delegation.started`, `delegation.launch_failed`, `delegation.settle`, `request.open`, `request.close`, `request.answer`, `delegation.cancel` and `message.send`.
- Ingest: drivers and terminal observers call `Engine.Ingest(sessionID, providerEvent)`, which normalizes into the same events (`turn.started`, `item.updated`, `turn.ended`, `request.opened`, `request.closed`, `status.changed`, …) and runs reactors (finalize delegation, deliver wake, route question, send next queued).
- Effects (each carries an idempotency key the agent sees, such as Claude's message `uuid` or Codex's client id): `agent.launch`, `turn.start`, `turn.steer`, `turn.interrupt`, `request.answer`, `pty.input`, `agent.close`, `worktree.create`, `fork.native`, `naming.run`. The worker claims the oldest `pending` row per session, with one `running` row per session at a time; sessions run in parallel. On start it retires `running` rows whose agent process is gone before claiming new work. Before an in-place upgrade hands over, the old worker stops claiming and drains `running` rows for up to 5 s; a row still `running` after the handoff goes back to `pending` only when the new pocketd hasn't ingested its echo (the prompt item, or the turn it started).

### Wire additions (Go `internal/proto`, TS `packages/protocol`, Swift AnywhereKit, same PR)

- `AgentSummary` gains `parent`, `relationship`, `createdBy`, and `delegation` (`{taskId, status, workState}`) for a child.
- `agent.delegations {id, agentId}` → `{type:"agent.delegations", id, items:[Delegation]}`; server event `delegation.update {agentId, delegation}`.
- Timeline item kind `delivery` (`{taskIds, childIds, outcome: done|failed|asks, question?}`).
- Queue verbs (scope `drive`): `queue.list`, `queue.edit`, `queue.cancel`, `queue.reorder`, `queue.send_now`; server event `queue.update {agentId, entries}`.
- Hosts (P3.4). Go `internal/proto/hosts.go`, TS `hosts.ts`, Swift `Hosts.swift`.
  - Cap `hosts.v1`. Owner only: a phone never gets the verbs or the events. `host.v1`, `host.changed` and `HelloOK.host` keep meaning this machine.
  - `HostTarget`: `{kind:"alias", name}` | `{kind:"address", addr, port, user?, keyFile?}`. An empty `user` signs in as this Mac's user. Neither `name`, `addr` nor `user` may start with `-` or hold whitespace.
  - `Host`: `{id, name, target, windows?, socket, link}`. `socket` is the tunnel's local end, `<home>/hosts/<id>.sock`, which clients dial for that host's terminals, agents and files.
  - `Link`: `{state:"connecting"}` | `{state:"online", version, uptime}` | `{state:"retrying", failure}` | `{state:"failed", failure}`. `retrying` tries again by itself; `failed` waits for `host.retry`.
  - `failure`: `auth`, `unknown_key`, `key_changed`, `no_such_host`, `refused`, `timed_out`, `dropped`, `throttled`, `not_running`, `unpaired`, `path_too_long`, `other`.
  - Verbs, all scope `owner` (`peer.Needs` `ws:host.*: Own`):
    - `host.list {id}` → `{type:"host.list", id, hosts:[Host]}`.
    - `host.add {id, target, name?}` → `{type:"host.update", id, host}`. Refuses `host_exists` (same target) and `invalid_host`. It connects at once but installs nothing.
    - `host.remove {id, hostId, uninstall?}` → `ack`. With `uninstall` it first removes pocketd's service over ssh; a failure is refused and the host stays. Removal stops the tunnel and deletes the host's `.known_hosts`, `.token`, `.key` and `.key.pub`. Running tasks on it settle `failed`: "The host was removed".
    - `host.retry {id, hostId}` → `ack`. Reconnects now, skipping a backoff or a `failed` wait.
    - `host.install {id, hostId, update?, archiveDir?}` → `ack`, then `host.progress` events. Without `update`, a pocketd already running is reused; with it, the archive is installed over it. The source is `archiveDir`'s `pocketd-<os>-<arch>.tar.gz` (or `.zip`) with its `.sha256`, else this pocketd's own release version. Refuses `host_busy` while one runs.
    - `host.sign_in {id, hostId, password}` → `ack` once the app's key is on the host, then the link reconnects. Refuses `host_ssh` with `detail` = the failure class. The password lives only in memory and in ssh's askpass environment; it is never stored, logged, echoed or sent to a client.
  - Server messages, to owner conns with the cap, as a per-conn feed like `automations`, not `hub.Publish`:
    - `host.list` without `id`: the whole list, on hello and after a removal or import.
    - `host.update {host}` without `id`: one host added or changed, link included.
    - `host.progress {hostId, update, progress:{kind, step?, text?}}`. `kind` is `step` (`step` is `detect`, `download`, `verify`, `unpack`, `link`, `service` or `start`), `warn` (`text`), `running` (reused, ends), `done` (ends) or `error` (`text`, ends).
  - Error codes: `unknown_host`, `host_exists`, `invalid_host` (the message says what's wrong), `host_busy`, `host_ssh` (`detail` = failure class).
  - Host-key trust has no verb. It stays trust on first use (`accept-new` into the app's `<id>.known_hosts`); `key_changed` is resolved by removing and re-adding the host.
  - Ids from a host are tagged `<hostId>/<theirId>`. A delegation's `childId`, a launched thread's id, and the request id of a remote child's question all carry the tag. pocketd's own ids never contain `/`. A client splits on the first `/` to route to that host's tunnel.
  - `Delegation` gains `environmentId?`: the host id of a task that runs on a host.
  - The `target` of `delegate_task` and `anywhere_thread_launch` (MCP inputs, not wire frames): `environmentId` is live, and `projectId?` is added. With `environmentId`, `projectId` names the host's project, by path on the host or by name; the default is the host project named like the caller's project. `delegate_task` returns `environmentId` and a tagged `childId`. `create_threads` and `anywhere_thread_transfer` with `environmentId` stay `unsupported`.
  - Timeline item kind `host_link` (`{environmentId?, hint?, reason?}`): the card `anywhere_environment_link` posts. Its button opens Settings › Hosts' add form with `hint` filled in.
  - `host.sign_in`, `host.remove` with `uninstall` and `host.install` run ssh, so they run off the conn's read loop and reply under their own `id` when done.
- Cap `orchestrator.v1`.

### Tools (27)

Names are T3's with `t3_` → `anywhere_`. Inputs and results follow `orchestratorMcp.ts` minus `runtimeMode`, `interactionMode` and environment fields where noted. Errors are T3's `OrchestratorMcpFailure` codes plus `unsupported` and `depth_exceeded`.

| Tool | Group | Engine path |
|---|---|---|
| `orchestrator_capabilities` | A | agents, models (short list + `search`), who can be a parent, limits |
| `delegate_task` | A | `delegation.request` |
| `task_status` | A | `delegations` projection |
| `task_cancel` | A | `delegation.cancel` (subtree depth first) |
| `create_threads` | A | `session.create` ×N in the caller's checkout, no lineage |
| `anywhere_thread_launch` | A | `session.create` with checkout strategy (`root` \| `worktree` \| `existing_worktree`), `scratch` |
| `anywhere_thread_list` | A | `sessions` projection, newest first, `cursor` |
| `anywhere_thread_search` | A | `items_fts` + titles, branches, PRs; skips children |
| `anywhere_thread_read` | A | `items`, `messages` \| `activity`, ~30k chars, positions |
| `anywhere_thread_send` | A | `message.send` |
| `anywhere_thread_wait` | A | turn state, ≤45 s |
| `anywhere_thread_interrupt` | A | `turn.interrupt` / `session.stop` |
| `anywhere_thread_configure` | A | `session.configure` (same agent) |
| `anywhere_pending_request` | A | `requests` (questions only), `request.answer` |
| `anywhere_queue` | A | `queue.*` |
| `anywhere_thread_transfer` | B | `transfer.fork` / `transfer.merge_back` / status |
| `anywhere_thread_update` | B | `session.rename`, `naming.run` |
| `schedule_task` | B | Automation save |
| `list_scheduled_tasks` | B | Automation list |
| `delete_scheduled_task` | B | Automation delete |
| `run_scheduled_task_now` | B | Automation run |
| `anywhere_environment_links` | B | `hosts.Manager.List`: `{links:[{environmentId, label, status: reachable\|unreachable, lastError?}], notListed}` |
| `anywhere_environment_link` | B | a `host_link` item in the caller's timeline → `{status:"pending"}`, or `{status:"linked", environmentId, label}` when the named host is already online |
| `anywhere_environment_unlink` | B | host remove (no uninstall) → `{environmentId, removed}`. Refuses `conflict` while a task runs there |
| `anywhere_background_wait` | C | `background.hold` |
| `anywhere_workflow_stop` | C | `stop_task` effect |
| `anywhere_thread_organize` | C | `session.organize` |

A session link is `anywhere://session/<id>`; the desktop opens it.

## Testing

1. **Decide.** Table tests on `decide`: ceiling, depth and concurrency limits, wake batching and disposal, subtree cancel, queue ordering, receipts.
2. **Store.** Temp-dir SQLite: a duplicate `commandId` creates nothing; killing pocketd between commit and effect resumes it; pruning at 1 / 7 / 15 days.
3. **e2e** with real pocketd, `e2e/fakeclaude`, `e2e/fakecodex` and a fake ACP agent, and an MCP client over HTTP: Claude delegates to Codex and gets the result; completion wakes the parent; a question reaches the parent and an earlier answer cancels the notice; `task_cancel` stops the tree; a restart mid-delegation resumes; in-place upgrade from the current release; bad, expired and over-ceiling tokens are refused.
4. **App.** `capture.sh` before and after for the panel, delivery rows and child view; `verify-anywhere`. Before merge, by hand: Claude delegating to real Codex, and a delegation to a host.

Golden tests: the Swift and protocol suites already fail on `main` on goldens never mirrored (PR #265) and CI runs neither; the gate is no new failures, and every wire change here is mirrored.

Toolset, as `scripts/check.sh`: from `packages/pocketd`, `env -u POCKETD_SOCK go vet ./... && env -u POCKETD_SOCK go test -race -count=1 ./...`; from `packages/desktop`, `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`; `pnpm --filter @pocket/protocol test`.

## Out of scope

- `request_secret`.
- pi and omp as parents.
- Snooze and settle.
- A T3-style event-stream wire.
- Webhooks reachable from the internet. The user tunnels them.

## Open questions

- MCP server: hand-roll the streamable HTTP subset pocketd needs, or add `github.com/modelcontextprotocol/go-sdk`. pocketd has five direct dependencies today. **Built hand-rolled** (`internal/mcp/server.go`: POST JSON-RPC, GET stream for `tools/list_changed`, bearer auth), so no dependency was added; the owner can still swap in the SDK.
- Codex user-input questions: pocketd doesn't handle Codex's user-input request today (only Claude's `AskUserQuestion` and ACP questions, `acp/request.go:231`, reach the broker as questions). P0.3 adds it after confirming the app-server request name against codex-cli 0.163. **Added** as `item/tool/requestUserInput` (`codex/session.go`), matching the schema codex-cli 0.163 generates (`codex app-server generate-json-schema`).
- Hosts, `host.update` (P3.4 lists it as a verb and as an event): **An event only.** Install and Update both go through `host.install {update}`.
- Hosts, verbs beyond the plan's list: **`host.install` and `host.sign_in` are added**, since pocketd now runs ssh for them.
- Hosts, capability name: **`hosts.v1`**, since `host.v1` already means this machine. It is defined in P3.4's wire stage and advertised with the handler.
- Hosts, host-key trust verb: **None.** Trust on first use stays as today.
- Hosts, "the desktop goes through pocketd" (Decisions): **For the list, links, install and sign-in.** Terminals, agents and files still go over the tunnel socket in `Host.socket`, which the desktop dials itself.
- Hosts, where a remote worktree starts (Delegation decisions say the parent's HEAD): **The host project's default branch**, since the parent's commit may not exist there. The result warns.
- Hosts, `anywhere_environment_link`: **Returns `pending` and no message comes back** when the user finishes the card. T3 posts the answer back; that is left out.
- Hosts, limits: **Remote tasks count against the Mac's 12; the host's own depth limit is not enforced.** A remote child is a depth-0 session to its host's pocketd and can delegate further.
- Restore from the store (P0.5): **`sessions.saved` (migration 004) holds the `terminals.json` entry each session was last saved with**, written with the file, rather than a column per launch field. Shells, which aren't sessions, still come back only from `terminals.json`. A `terminals.json` whose sessions differ from the store's was written by a pocketd without the store (the first start, or after a downgrade), so it wins and is taken into the store.
- `agent.list` (P0.5): **the store's open sessions, each as the registry has it live**; a start closes the sessions the store keeps open that came back neither restored nor handed down. `agent.view` only marks sessions seen and is unchanged.
- ACP and pi parents (P0.5): **a restore from the store drops the agent's replay**, so an OpenCode sub-agent's session isn't loaded again and its work reads as it did live. A pi turn cut by the restart is still read again from pi's history, the only place that says which calls it left open.
- Upgrade gate (P0.8): **`e2e/upgrade_main_test.go` builds `origin/main` from a detached worktree** (with the checkout's `third_party` linked in for libghostty) and upgrades it in place to this branch by replacing the binary. Its terminal session is one Anywhere launched: a terminal the user typed `claude` into comes back a shell after a cold start, on `main` as here.
- Writes as commands (P0.6): **`agent.close` (`session.close`), `agent.pin` (`session.organize`) and `permission.resolve` (`request.answer` with `user`) dispatch commands.** A user's answer reaches a permission ask too, and the broker alone says whether the ask is still open, as before. Three verbs stay direct:
  - `agent.create` (with `fork`): it streams `agent.progress` and `agent.creating` to the asking connection while setup runs, and its receipt is per requester (owner, phone, PTY); an effect has no way back to that connection and a global command id would let requesters' ids collide. So `launch/receipts.go` stays.
  - `agent.settings`: it sets access, which `session.configure` doesn't by design.
  - `agent.send_now`: it has the agent send what it already holds; no command does that (`queue.send_now` is for pocketd's own queue).
- Fork (P0.7): **a Codex fork is already a terminal session without tools**: `launch.Chat` is false for it, and only a chat gets `MCPFlags`.
- Held background tasks (P4): **a cold restart drops them**; its agents ended with the pocketd before, so none runs them. Only an in-place upgrade holds them again.
