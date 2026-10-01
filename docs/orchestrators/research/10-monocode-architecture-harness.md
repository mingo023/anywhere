# 10 — MonoCode: architecture and provider harness

Date: 2026-09-30.

Sources:
- MonoCode: `hardbeat920/monocode@cdc1441dc51e3709cd843e5c316608a123f323c6` (2026-09-30, MIT), clone `/Users/mingo/tmp/orchestrators/monocode`.
- Pocket: `anywhere@86deb13c` (worktree `orchestrator-research`).
- Codex release notes: https://github.com/openai/codex/releases/tag/rust-v0.157.0 (via `gh api`).
- Product-level MonoCode findings are in `09-monocode-product.md`. This report covers mechanism only.

Citation legend:
- `M path:L` is a line in the MonoCode clone. Paths are repo-relative, for example `M src-tauri/src/harness.rs:452`.
- `P path:L` is a line in the Pocket worktree, relative to `packages/`.
- `Z` would mean zeron; it isn't cited here.
- URLs are official upstream pages.
- `L-L` is an inclusive range.
- Every claim comes from code unless it cites `docs/`. Where docs and code differ, the text says so.

## TL;DR

- **Layering.** The Rust side is a dumb process pipe: spawn, line-write to stdin, line events from stdout and stderr, kill, and loopback HTTP/SSE. All provider protocol state machines are TypeScript adapters. On desktop they run in the webview. On a remote machine they run unchanged in a Node "host", by swapping one `ChildBackend` (`M src/integrations/harness/core/child.ts:10-24`, `M host/child-backend.ts:43-197`).
- **Claude.** A persistent `claude` process speaks stream-json over stdio, with `--permission-prompt-tool stdio`. It uses the SDK control protocol: `control_request` / `control_response` with subtypes `initialize`, `can_use_tool`, `interrupt`, `stop_task` and `list_models`. Any model, effort or mode change kills the process and respawns it with `--resume`. There is no live `set_*` (`M src/integrations/harness/providers/claude/claude.ts:374-514`).
- **Codex.** `codex app-server` runs over stdio as JSON-RPC lines without the `"jsonrpc"` field.
  - Handshake: `initialize{capabilities:{experimentalApi:true}}`, then `thread/start` or `thread/resume`.
  - `turn/start` carries the policy, model, effort and `collaborationMode` for each turn.
  - Control calls: `turn/interrupt`, `turn/steer`, `thread/revert`, `thread/compact/start` (`M …/codex/codex.ts:517-590`, `M …/codex/codexProtocol.ts:134-181`).
- **Runtime modes.** There are four: supervised, auto-accept-edits, auto and full-access. Each maps to Claude `--permission-mode` and to a Codex `approvalPolicy`/`sandbox`/`approvalsReviewer` triple. "Plan" is a separate per-turn intent (`M …/claude/claudeProtocol.ts:111-126`, `M …/codex/codexProtocol.ts:62-95`).
- **Process hygiene.**
  - Each child gets its own process group and a `MONOCODE_HARNESS_PARENT` marker env var.
  - Orphans are reaped at launch.
  - Kill is SIGTERM, then SIGKILL after 2 s.
  - The environment comes from the login shell (`$SHELL -lic printenv`, 5 s timeout).
  - On the host, a guard process kills the group when fd 3 hits EOF (`M src-tauri/src/harness.rs:1005-1045,1239-1270,2531-2544`, `M host/provider-guard.mjs:1-92`).
- **Checkpoints** are per-session file snapshots (`manifest.json` + blobs), not git refs.
  - An edit-tool start snapshots "before"; its completion captures "after".
  - Keep/Undo works per file and refuses files edited by others.
  - Only Codex, OpenCode, Pi and OMP can rewind the last turn. Claude cannot (`M src-tauri/src/checkpoint.rs:89-186,380-439`).
- **Storage.**
  - Desktop: SQLite `monocode.db`, with the whole rendered transcript in `sessions.blocks_json`.
  - Host: `node:sqlite` (WAL, `synchronous=FULL`) holding snapshots, events, receipts and devices.
  - Streamed output is batched every 120 ms.
  - Commands are idempotent (`commandId` + sha256 receipt).
  - After a crash, a turn is marked interrupted and is never replayed (`M src-tauri/src/session_store.rs:12-28`, `M host/store.ts:30-36`, `M host/engine.ts:38,264-289,417,704`).
- **Remote.** An SSH `-L` forward reaches the host's `127.0.0.1:3774`.
  - Every call is `POST /rpc` with a per-device 43-char Bearer token.
  - The client polls: 0.75 s while a turn runs, 3 s idle (10 s when hidden).
  - The host is process-per-turn: stop the provider after each turn, then `bind` its session id for the next turn (`M host/server.ts:193-231`, `M src/features/connections/ui/RemoteSession.tsx:414-418`, `M host/engine.ts:820-846`).
- **Pocket today observes and types; it does not drive headless.**
  - Agents are TUIs in pocketd PTYs.
  - Claude is followed through a hooks plugin plus a transcript tail. Codex is followed as a second client on the account's app-server daemon socket.
  - Input is typed into the PTY: text, then 150 ms, then CR. Interrupt is ESC. Compact is typed as `/compact` (`P pocketd/internal/daemon/presence.go:113-155`, `P pocketd/internal/codex/rpc.go:44-68`).
- **Minimal headless path for Pocket:**
  - Two new `agent.Driver` implementations in pocketd:
    - Claude over stream-json stdio.
    - Codex over `app-server` stdio, reusing the existing `codex.Session` item mapper.
  - An `agent.create` protocol message.
  - A desktop transcript pane.
  - Timeline, broker, hub and the phone ChatScreen are reused as-is (see §12).

## Findings

### 1. Process model

| Layer | Desktop session | Remote session |
|---|---|---|
| UI + adapter state machines | Webview (React), `src/integrations/harness/providers/*` | Desktop only renders; adapters run in the host |
| Child backend | Tauri commands `harness_spawn/write/kill/http/sse` (`M src/integrations/harness/core/child.ts:283-324`) | `HostChildBackend.invoke` exposes the same command names (`M host/child-backend.ts:43-197`) |
| Process owner | Rust `harness.rs` (`M src-tauri/src/harness.rs:452-579`) | Node spawns `process.execPath provider-guard.mjs <cmd> <args>`, detached, stdio `[pipe,pipe,pipe,pipe]`, `MONOCODE_HOST=1` (`M host/child-backend.ts:262-309`) |
| Provider CLI | `claude`, `codex app-server`, `cursor-agent acp`, `opencode serve`, `pi --mode rpc`, … | same binaries resolved on the host (`M host/process.ts:8-24,116-168`) |

- **Lifecycle.**
  - The adapter registry idle-parks a live child after `HARNESS_IDLE_PARK_MS = 5*60_000` and keeps its resume state (`M src/integrations/harness/core/registry.ts:110`).
  - Operations are serialized per session through operation and steer queues (`M …/core/registry.ts:116-160`).
  - `sendHarnessTurn` brackets each turn with `control_authorize_turn` / `control_turn_finished` (`M …/core/registry.ts:208-232`).
- **Adapter contract:** `HarnessAdapter` covers send, steer, cancel, approve, answer, stop, forget, bind, compact, optional `rewindLastTurn`, and text-prompt helpers (`M …/core/registry.ts:47-102`).
- **Normalized event union** (`M …/core/types.ts:13-149`):
  - Session: `session.started/ended/error/providerBound`.
  - Turn and messages: `turn.started`, `message.delta`, `reasoning.delta`, `turn.metrics`.
  - Tools and steps: `tool.started/updated`, `agent.step`.
  - Prompts to the user: `approval.requested/resolved`, `question.asked/resolved`.
  - Other: `tasks.updated`, `plan`, `context`, `usage.limited`, `background.updated`.
- **Session input** is `{sessionId, cwd, model, modelSettings, providerAccountId, runtimeMode, intent, controlsAgents, appAccess, onEvent}` (`M …/core/types.ts:153-170`).
- **App startup and shutdown.**
  - Startup, in order: `harness::reap_orphaned_harness_processes()`, `session_store::init`, `control::init`, `checkpoint::init` (`M src-tauri/src/lib.rs:231-235`).
  - On `ExitRequested`/`Exit`, it runs `kill_all` on the harness and PTY hosts (`M src-tauri/src/lib.rs:571-597`).

### 2. Rust child pipe (`harness.rs`)

- **Events:**
  - `harness-stdout` and `harness-stderr`: one event per line.
  - `harness-exit {sessionId, code, pid}`.
  - `harness-sse`, `harness-sse-end`.
  - Sources: `M src-tauri/src/harness.rs:20-24,526-573`.
- **Spawn:** `harness_spawn(session_id, command, args, cwd, account, binary_provider, binary_path)`.
  - Only a binary the app itself resolved may be spawned (`:473`).
  - It reserves the worktree (`:477`) and pipes all three stdio streams (`:481-487`).
  - It then runs `prepare_child`, `apply_provider_account` and `control::configure_child` (`:488-491`).
  - A spawn cancelled mid-flight is reaped (`:514-524`).
- **stdin write:** `harness_write` appends `"\n"` and runs on the blocking pool (`:701-722`).
- **Loopback HTTP and SSE:**
  - HTTP has a 30 s default timeout (`:741-774`).
  - SSE connects within 10 s and reads for up to 6 h (`:776-828`).
  - `assert_loopback` rejects non-loopback URLs (`:888-898`).
  - Free ports come from binding `127.0.0.1:0` (`:441-447`).
- **One-shot exec allowlist:** `--version`, `--list-models`, `models --verbose|--json`, `models`, `status --json`, `agent list`. Timeout is 15 s (`:900-908,967`).
- **Kill and isolation:**
  - Each child gets `process_group(0)` and the marker env `MONOCODE_HARNESS_PARENT=<app pid>` (`:1012,1027-1045`).
  - Kill is SIGTERM to the group, escalating to SIGKILL after 2 s (`:1005,1161-1194`).
  - `kill_all` allows 300 ms grace (`:1009-1011`).
  - Spawn retries on `ETXTBSY` (`:1069-1082`).
- **Orphan reaper:** it reads the process table with env. It kills harness-looking argv whose marker parent is not us and not alive (`:1239-1270`).
- **GUI environment:**
  - `gui_search_path` puts the login-shell PATH ahead of fallbacks (`:2345-2394`).
  - Login env comes from `$SHELL -lic printenv` with a 5 s timeout (`:1017,2522-2561`).
  - Only `PATH` plus 5 credential vars are imported: `AI_GATEWAY_API_KEY`, `FX_AI_GATEWAY_API_KEY`, `VERCEL_OIDC_TOKEN`, `XAI_API_KEY`, `GROK_CODE_XAI_API_KEY`. `ANTHROPIC_API_KEY` and `OPENAI_API_KEY` are not among them (`:2474-2483`).
- **Binary resolution:**
  - Codex falls back to `/Applications/Codex.app/Contents/Resources/codex` (`:1729-1759`).
  - Claude prefers the login-shell `which` (`:1790-1811`).
- **Provider accounts:**
  - Profile dirs live at `app_data_dir/provider-accounts/{claude|codex}/{id}`, with id matching `[A-Za-z0-9_-]{≤80}` (`:581-625`).
  - Claude profiles set `CLAUDE_CONFIG_DIR` and `CLAUDE_SECURESTORAGE_CONFIG_DIR` and remove `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN` and `CLAUDE_CODE_OAUTH_TOKEN`.
  - Codex profiles set `CODEX_HOME` and remove `OPENAI_API_KEY`, `CODEX_API_KEY` and `CODEX_ACCESS_TOKEN` (`:676-692`).
- **TS side of the pipe:**
  - `watchChild` buffers up to 1000 early lines and replays them to a late subscriber (`M …/core/child.ts:86,243-255`).
  - `jsonRpc.ts` registers the pending entry before writing and has a 15 s write timeout (`M …/core/jsonRpc.ts:21,92-143`).
  - `includeJsonrpc:false` is used for Codex (`:33`).
  - Routing is: response by id, then server request, then notification (`:194-220`).

### 3. Claude adapter (stream-json + SDK control protocol)

- **Spawn args** (`M src/integrations/harness/providers/claude/claudeProtocol.ts:239-291`):
  ```
  claude --output-format stream-json --verbose --input-format stream-json
         --permission-prompt-tool stdio            # omitted for isolated helper spawns
         --include-partial-messages
         --setting-sources=user,project,local --settings '<json>'
         [--model M] [--effort E] --permission-mode <default|acceptEdits|auto|bypassPermissions|plan>
         [--allow-dangerously-skip-permissions]     # only with bypassPermissions
         (--session-id <new uuid> | --resume <claude session id>)   # mutually exclusive via launchOptions
  ```
  - `--session-id` is dropped when resuming (`M …/claude/claude.ts:1733`).
  - `--settings` JSON can carry `alwaysThinkingEnabled`, `fastMode`, `ultracode` and `disableAllHooks`. The last one applies when the user turned hooks off (`M …/claude/claude.ts:1697-1735`).
  - Isolated helper spawns (titles, catalog) add `--no-session-persistence --strict-mcp-config --mcp-config '{"mcpServers":{}}'` and force `disableAllHooks`.
- **Permission mode is always sent.** `default` is sent explicitly: the comment says otherwise the user's `permissions.defaultMode` could silently make "Supervised" run as `auto`. `manual` is an alias that needs CLI 2.1.200 (`M …/claudeProtocol.ts:105-126`).
- **Minimum CLI versions** are gated per feature and model. For example, Opus 5.5 needs 2.1.280 (`M …/claudeProtocol.ts:29-34`).
- **Handshake:**
  - After spawn it writes `{"type":"control_request","request_id":"monocode_N","request":{"subtype":"initialize"}}`.
  - It then waits up to `INIT_TIMEOUT_MS=8000` for `system/init` or a `control_response`.
  - The wait resolves on timeout rather than failing (`M …/claude/claude.ts:178,499-502,1659-1671`).
  - It then emits `session.providerBound` and `session.started`.
- **User turn** (`M …/claudeProtocol.ts:188-217`):
  ```json
  {"type":"user","session_id":"","parent_tool_use_id":null,
   "message":{"role":"user","content":[{"type":"text","text":"…"},
     {"type":"image","source":{"type":"base64","media_type":"image/png","data":"…"}}]}}
  ```
  - Effort `ultrathink` is sent as an `"Ultrathink:\n"` prompt prefix.
  - The 1M-context variant uses a `[1m]` model suffix (`M …/claudeProtocol.ts:142-176`).
- **Steer** writes another `user` message while the turn is active (`M …/claude/claude.ts:265-278`).
- **Streaming:**
  - `stream_event` → `content_block_delta` with `text_delta`/`thinking_delta` becomes `message.delta`/`reasoning.delta`. `content_block_start` with `tool_use` becomes `tool.started` (`M …/claudeProtocol.ts:527-577`).
  - `parent_tool_use_id != null` marks a subagent message (`:595-598`).
  - `sessionIdFromMessage` skips `hook_*` and subagent lines (`:420-431`).
- **Turn end:**
  - `result` with subtype `success` means completed. `terminal_reason` `aborted_tools`/`aborted_streaming` means interrupted (`M …/claudeProtocol.ts:476-502`).
  - `rate_limit_event` with `rate_limit_info.status:"rejected"` becomes `usage.limited` (`:504-525`).
  - A result also yields context, metrics and usage (`M …/claude/claude.ts:891-919`).
  - The turn stays open until background tasks finish (`:1620-1629`).
- **Permission and question round-trip** (`M …/claude/claude.ts:921-1047`, `M …/claudeProtocol.ts:293-316,367-418`):
  - Inbound: `{"type":"control_request","request_id":R,"request":{"subtype":"can_use_tool","tool_name":T,"input":{…},"tool_use_id":U}}`. `sdk_control_request` is also accepted.
  - Reply: `{"type":"control_response","response":{"subtype":"success","request_id":R,"response":{"behavior":"allow","updatedInput":{…}}}}` or `{"behavior":"deny","message":"User declined tool execution."}`.
  - Subtype `permission` is handled like `can_use_tool`. Any other control subtype gets an empty success response (`:926-931`).
  - `control_cancel_request {request_id}` withdraws a pending prompt.
  - `AskUserQuestion` also arrives as `can_use_tool`. The answer is `allow` with `updatedInput:{questions, answers:{<question prompt>: "<label>[, <label>]"}}`; skip is `deny` (`:948-985`, `M …/claudeProtocol.ts:899-915`).
  - `ExitPlanMode` is always denied with "The client captured your proposed plan. Stop here and wait…". The plan is emitted as a `plan` event (`:987-1000`).
  - Plan intent allows only read and search tools. Full-access auto-allows. Everything else goes to the UI (`:1004-1047`).
- **Interrupt** (`M …/claude/claude.ts:302-334`):
  - Pending approvals resolve as deny and questions as skipped.
  - It sends `{"subtype":"stop_task","task_id":…}` for each background task, then `{"subtype":"interrupt"}`.
- **Compact** is sent as a `/compact` text turn and counts only when a `system` compact subtype confirms it (`:226-263`).
- **Respawn policy:**
  - The process is reused only if cwd, `settingsKey` (account, model, effort, fast, thinking, context, runtimeMode, hooks) and planning all match. Otherwise it is stopped and respawned with `--resume`.
  - A cwd change drops the resume because "Claude sessions are cwd-bound" (`:374-394`).
  - The resume grace is `RESUME_GRACE_MS=15000` (`:183`).
- **Catalog:** an isolated spawn sends `initialize`, then `{"subtype":"list_models"}`, and falls back to `--version`. The probe timeout is 15 s (`M …/claude/claudeCatalog.ts:207,248-320`).
- **No `rewindLastTurn`** in the Claude adapter (`M …/claude/claudeAdapter.ts:26-46`).

### 4. Codex adapter (`codex app-server` JSON-RPC over stdio)

- **Spawn:** `codex app-server` in the session cwd, with an account profile (`M …/codex/codex.ts:517-527`).
- **Handshake:**
  - `initialize {clientInfo:{name:"monocode",title:"MonoCode",version:"0.1.0"}, capabilities:{experimentalApi:true}}`. `experimentalApi` is required for `collaborationMode` and Plan. Then notify `initialized` (`:530-542`).
  - The server request `currentTime/read` can arrive before the thread exists. The reply is `{currentTimeAt:<unix s>}` (`:469-479`).
- **Thread:**
  - `thread/resume {threadId, …startParams}`. If that fails with a recoverable error (`M …/codexProtocol.ts:208-221`), it falls back to `thread/start {cwd, approvalPolicy, sandbox, sandboxPolicy:{type}, approvalsReviewer, model?, serviceTier?}` (`M …/codex/codex.ts:551-590`, `M …/codexProtocol.ts:97-119`).
  - Updates are muted while resuming.
- **Turn** (`M …/codexProtocol.ts:134-181`, `M …/codex/codex.ts:644-700`):
  ```json
  {"threadId":"…","input":[{"type":"text","text":"…"}],
   "approvalPolicy":"…","approvalsReviewer":"…","sandboxPolicy":{"type":"…"},
   "collaborationMode":{"mode":"default|plan","settings":{"model":"…","reasoning_effort":"…","developer_instructions":null}},
   "model":"…","effort":"…","serviceTier":"…"}
  ```
  - Images are sent as `{type:"image",url:"data:…;base64,…"}` or `{type:"localImage",path}` (`:184-206`).
  - The turn ends only on `turn/completed` or `turn/aborted`.
  - Because policy is carried per turn, settings changes need no respawn. A restart happens only when cwd, account or `controlsAgents` changes (`M …/codex/codex.ts:401-424`).
- **Mode map** (`M …/codexProtocol.ts:62-95`):

  | Mode | approvalPolicy | sandbox | approvalsReviewer | sandboxPolicy.type |
  |---|---|---|---|---|
  | supervised | untrusted | read-only | user | readOnly |
  | auto-accept-edits | on-request | workspace-write | user | workspaceWrite |
  | auto | on-request | workspace-write | auto_review | workspaceWrite |
  | full-access | on-request | danger-full-access | user | dangerFullAccess |
  | plan intent (per turn) | never | read-only | inherits | readOnly |

  - Full-access keeps `on-request` because, per the comment, "never" rejects escalations before the client can allow them.
  - `controlsAgents` adds `networkAccess:true` (`:43-53`).
  - Client-side auto-answers: auto-accept-edits allows file changes; full-access allows everything (`M …/codex/codex.ts:1352-1366`).
- **Notifications consumed** (`M …/codexProtocol.ts:289-416`, `M …/codex/codex.ts:733-955`):
  - Items and deltas: `item/started`, `item/completed`, `item/agentMessage/delta`, `item/reasoning/summaryTextDelta`, `item/reasoning/textDelta`, `item/plan/delta`, `item/commandExecution/outputDelta`, `item/fileChange/patchUpdated`.
  - Turn and thread: `turn/started`, `turn/completed`, `turn/aborted`, `turn/plan/updated`, `thread/started`, `thread/tokenUsage/updated`.
  - Other: `account/rateLimits/updated`, `error`, `configWarning`, `serverRequest/resolved`.
- **Server requests** (`M …/codex/codex.ts:1139-1336`, `M …/codexProtocol.ts:248-261`):
  - `item/commandExecution/requestApproval` and `item/fileChange/requestApproval` → `{decision:"accept"|"decline"}`. The wire also allows `acceptForSession` and `cancel`; MonoCode uses one-shot accept.
  - `item/permissions/requestApproval` → `{scope:"turn"|"session", permissions}`, or `{permissions:{}}` to deny.
  - `item/tool/requestUserInput` → `{answers:{…}}`. Non-blocking questions auto-resolve after `QUESTION_AUTO_RESOLVE_MS=120000` (`:67`).
  - `mcpServer/elicitation/request` → `{action:"accept"|"decline"|"cancel", content, _meta:null}`.
  - Unknown methods → JSON-RPC error `-32601`. `serverRequest/resolved` cancels the matching UI prompt.
- **Controls:**
  - Interrupt: `turn/interrupt {threadId, turnId}` (`:340-362`).
  - Steer: `turn/steer {threadId, expectedTurnId, input}` (`:254-275`).
  - Compact: `thread/compact/start {threadId}` (`:704-724`).
  - Rewind: `thread/turns/list {threadId, limit:100, sortDirection:"desc", itemsView:"summary"}` → `thread/revert {threadId, beforeTurnId}` (`:196-252`).
- **Catalog:** paginated `model/list {cursor?}` (`M …/codex/codexCatalog.ts:124-137`).

### 5. Other providers (for completeness)

| Provider | Transport and args | Key methods |
|---|---|---|
| Cursor | `cursor-agent acp` (`M …/cursor/cursor.ts:301-304`) | ACP `initialize{protocolVersion:1}`, `session/new`\|`session/load`, `session/prompt`, `session/cancel`, `session/set_model`, `session/set_config_option`, inbound `session/update` and `session/request_permission`; steer via notify `session/steer`, falling back to `_session/steer` (`:158-160,311-503`) |
| Grok | `grok --no-auto-update [--permission-mode plan] agent --no-leader [--model] [--reasoning-effort] [--always-approve] stdio` | ACP (`M …/grok/grokProtocol.ts:89-105`) |
| fx, Hermes | `fx acp [--model]`, `hermes acp` | ACP (`M …/fx/fx.ts:443`, `M …/hermes/hermes.ts:253-256`) |
| Antigravity | spawns `agy_acp_server.par`, not `agy acp` | ACP (`M …/antigravity/antigravity.ts:133`) |
| OpenCode | `opencode serve --hostname=127.0.0.1 --port=<free>` | HTTP `/session/:id/prompt_async`, `/abort`, `/revert`, `/summarize`, `/fork`, SSE `/event` (`M …/opencode/opencode.ts:408-411`, `M …/opencode/opencodeClient.ts:94-185`) |
| Pi, OMP | `pi --mode rpc [--no-session] [--no-extensions] [--tools …] [<resumeFlag> id] [--model]` | RPC `prompt`, `steer`, `abort`, `compact`, `get_state`, `set_model` (`M …/pi/piProtocol.ts:150-166,215,245`, `M …/pi/piFamily.ts:270-1121`) |

- **`rewindLastTurn`** is implemented only by the Codex, OpenCode, Pi and OMP adapters (`rg -l rewindLastTurn providers`).
- **CLI self-update** runs `claude update`, `codex update` or `pi update --self`. Latest versions come from the npm registry for `@anthropic-ai/claude-code` and `@openai/codex` (`M src-tauri/src/harness_updates.rs:8-31`).

### 6. Checkpoints and rollback

- **Storage:**
  - Location: `app_data_dir/checkpoints/<sessionId>/`, holding `manifest.json` (atomic tmp+rename) and `files/` blobs (`M src-tauri/src/checkpoint.rs:563-568,1326-1342`).
  - Manifest fields: `{cwd, files(before), touched, tracked, prepared, after, stats, diverged}`.
  - A per-store mutex serializes operations (`:20-43`).
- **Git's role** is limited to listing dirty files at `ensure` and checking HEAD. Snapshots are plain file copies, capped at 500 files (`:18,49-87`).
- **Hook points** (`M src/app/App.tsx:6702,11263-11284`, `M src/features/sessions/model/checkpoint.ts:86-122`):
  - At turn start, `beginSessionTurn` runs `ensure`, which snapshots the currently dirty files.
  - On an edit tool's `tool.started` or `tool.updated` (not yet completed), `prepare(paths)` snapshots "before". The comment says "The first tool-start event owns the safe undo boundary" (`M src-tauri/src/checkpoint.rs:89-139`).
  - On completed or success, `capture(paths)` snapshots "after" and computes stats. A completion with no matching prepare is kept for review but is "deliberately not undoable" (`:141-186`).
- **Keep and Undo:**
  - Undo works per file or for all.
  - It refuses a file that changed outside this session, or one touched by another session (`foreign_touched_paths`).
  - Keep releases a path, or all of them (`:380-439`).
- **Worker integration:**
  - `apply` copies a worker worktree's verified delta into the lead checkout.
  - It requires both checkouts at the same HEAD and preflights every path before writing.
  - It is idempotent on retry (`:202-269`).

### 7. Storage

- **Desktop:**
  - `app_data_dir/monocode.db` is rusqlite in WAL mode with a separate `query_only` read connection (`M src-tauri/src/session_store.rs:37-55,92`).
  - The `sessions` table holds `id, cwd, harness, model, model_settings, runtime_mode, title, provider_session_id, blocks_json, created_at, updated_at`, plus later columns such as context, branch, worktree and linked work item (`:12-28,96-127`).
  - The transcript is MonoCode's own rendered blocks; provider transcript files are not re-read.
  - A quit snapshot (`session_set/list/take_in_flight`) records sessions to resume. `list` does not clear it, "so dev reloads must not consume the only copy" (`:552-584`).
- **Host** (`M host/store.ts:15,30-36,139-195`):
  - `node:sqlite` with `journal_mode=WAL; synchronous=FULL`.
  - Tables: `metadata`, `projects`, `sessions(snapshot)`, `receipts(id, signature, receipt)`, `events(session_id, revision, payload)`, `devices(hash)`.
  - Sync returns `unchanged`, `snapshot` or `delta`, based on per-block `blockRevisions`.
  - Events older than 2000 revisions are pruned. 32 sessions are cached.
- **Host engine** (`M host/engine.ts:38,78-248,417,704,868-883`):
  - Command types: `create/configure/compact/send/draft/removeDraft/cancel/approve/answer`, each with a `commandId`.
  - A sha256 signature detects replays that reuse an id with a different payload. The code states "A receipt means durable host acceptance, not provider completion".
  - Stream deltas (`message.delta`, `reasoning.delta`, `tool.updated`, `agent.step`, `status`) flush every 120 ms.
  - On restart, running turns become `interrupted` with the message "Host restarted. This turn was interrupted; inspect its work before continuing." (`:264-289`).

### 8. Remote sessions

- **Transport:**
  - The desktop runs OpenSSH with `-T -o ConnectTimeout=15 -o ServerAliveInterval=15 -o ServerAliveCountMax=3 -o ForwardAgent=no -o ControlMaster=no -o ExitOnForwardFailure=yes -o StrictHostKeyChecking=ask -o BatchMode=yes|no …`. It forwards a temporary local port to the host's `127.0.0.1:3774` (`M src-tauri/src/remote_ssh.rs:166-205`, `M host/cli.ts:52`).
  - Askpass prompts appear in Settings (`M src-tauri/src/ssh_askpass.rs`).
- **Server** (`M host/server.ts:190-231`):
  - Node `http` with `requestTimeout:20_000`, `headersTimeout:10_000` and 8 KiB headers.
  - Only `POST /rpc` is served. Any `Origin` header gets 403.
  - Auth is `Authorization: Bearer [A-Za-z0-9_-]{43}`, checked again after the body is read in case the device was revoked meanwhile.
  - Each request must carry `version == HOST_PROTOCOL_VERSION` and a matching `environmentId`.
  - Bodies are capped at 16 MiB.
- **Methods:**
  - Environment and projects: `environment.describe` (capabilities list), `projects.*`, `models.list`.
  - Sessions: `sessions.list/update/delete/sync/syncChunk/get`, `events.read {after}`.
  - Commands and attachments: `commands.dispatch`, `attachments.*`.
  - Devices: `devices.revokeSelf`.
  - Git and files: `git.*`, `files.*`, `workspace.run` (`:262-530`).
- **Tokens** are 32 random bytes in base64url; the store keeps only their sha256 (`M host/store.ts:279,312`).
  - The desktop saves credentials to `remote-machines.json` with mode 0600, not the keychain.
  - Plain HTTP is allowed only to loopback (`M src-tauri/src/remote.rs:76-82,115,208`).
- **Service:** launchd `com.monocode.host` with `RunAtLoad`/`KeepAlive`, a systemd user unit with `Restart=on-failure`, or Windows Task Scheduler.
  - A single owner holds a SQLite `BEGIN EXCLUSIVE` lock on `owner.db` (`M host/service.ts:13,72-93`, `M host/owner.ts:10-20`).
- **Execution:** the host is process-per-turn.
  - After each turn it awaits `provider.stop` (which is `forget*`), then `provider.bind(sessionId, providerSessionId, cwd)` so the next turn resumes (`M host/engine.ts:818-846`, `M host/providers.ts:57-69`).
  - Named provider accounts are unsupported on the host (`M host/child-backend.ts:262-309`).
  - Lines are capped at 8 MiB (`:311-339`).
  - Kill is SIGTERM, then SIGKILL after 3 s (`:353-400`).
- **Polling:**
  - Session sync: 750 ms while running, 3 s while visible, 10 s when hidden. Failures back off as `750·2^n` ms, capped at 10 s (`M src/features/connections/ui/RemoteSession.tsx:411-419`).
  - Session list: 3 s. Machine status: 15 s (`M src/features/connections/model/connections.ts:328,439`).
- **Docs vs code:** `docs/remote-access.md` matches the code on every claim checked: port 3774 (`:57`), 0.75 s/3 s polling (`:101`), 120 ms batches, no replay after a crash, and named accounts unsupported (`:103`). It says the Node host "proves the execution boundary without introducing the planned Rust daemon/worker IPC yet" (`:105`), so the host is a stopgap design.

### 9. Local control plane (cross-reference only)

- **Server:** `control.rs` runs a loopback TCP server with 8 workers and a 32-slot queue.
  - Requests are single-line JSON up to 256 KiB, with 3 s socket timeouts.
  - The server relays each request to the owning window as `monocode-control-request` and waits up to 35 s for the reply (`M src-tauri/src/control.rs:182-281`).
- **Children** receive `MONOCODE_CONTROL_ENDPOINT/TOKEN` and `MONOCODE_APP_ENDPOINT/TOKEN` only while they hold a grant (`:464-486`).
- **CLI:** the same executable acts as the CLI (`monocode control …`, `monocode app …`) (`M src-tauri/src/main.rs:7-15`, `M src-tauri/src/control_cli.rs:8-60`). Actions: `list/delegate/get/wait/respond/answer/steer/message/retry/cancel/review/finish`.
- Orchestration is out of scope here.

### 10. Pocket today (pocketd) vs MonoCode

| Concern | Pocket | MonoCode |
|---|---|---|
| Who owns the agent UI | Provider TUI in a pocketd PTY (`P pocketd/internal/terminal/terminal.go:112-156`) | MonoCode's own transcript and composer; the CLI is headless |
| Claude status | Plugin hooks injected via `CLAUDE_CODE_PLUGIN_DIRS` (`P pocketd/internal/daemon/plugin.go:10-22,65-82`) | stream-json events |
| Claude transcript | Tails `transcript_path` JSONL at 100 ms (`P pocketd/internal/claude/transcript.go:57-89,132-160`, `P pocketd/internal/daemon/claude.go:79-113`) | stream-json `assistant`/`user`/`stream_event` |
| Claude permission | `PermissionRequest` hook with 610 s timeout → broker → `hookSpecificOutput.decision` (`P pocketd/internal/daemon/plugin.go:28-31`, `P pocketd/internal/daemon/daemon.go:123-158`) | `can_use_tool` → `control_response` |
| Codex | Second WebSocket client on `$CODEX_HOME/app-server-control/app-server-control.sock` (`P pocketd/internal/codex/rpc.go:44-68,162-165`). Binds a thread by "Enter within 3 s, then a thread turned active" (`P pocketd/internal/daemon/codex.go:14-16,132-151`) | Owns a private `codex app-server` over stdio |
| Codex mapping | `codex.Session`: `thread/resume` replay, `item/*`, `turn/completed`, and the two `requestApproval` methods; answers `accept`/`decline` (`P pocketd/internal/codex/session.go:72-279`) | Same methods plus `permissions`, `requestUserInput`, `elicitation`, `currentTime/read`, and `-32601` |
| Prompt, interrupt, compact | Typed: text, 150 ms, `\r`; ESC; `/compact` (`P pocketd/internal/terminal/terminal.go:278-287`, `P pocketd/internal/daemon/presence.go:123-150`) | stdin JSON / RPC |
| Model, effort, mode | Not settable; model only observed (`P pocketd/internal/agent/agent.go:301-303`) | Per-spawn or per-turn |
| Kill | `cmd.Process.Kill()` on the PTY child only (`P pocketd/internal/terminal/terminal.go:310-312`); agent `SIGTERM` to its pid (`P pocketd/internal/daemon/presence.go:138-142`) | Process group, SIGTERM → 2 s → SIGKILL, orphan reaper |
| Persistence | In-memory timelines; the only file writers are config and plugin (`rg WriteFile pocketd`) | SQLite (desktop and host) |
| Wire | WS messages `hello`, `agent.list/prompt/interrupt/compact/close/view/seen/timeline`, `permission.resolve{option,message}` (`P pocketd/internal/proto/messages.go:62-83`) | Tauri events (local) / `POST /rpc` (remote) |

- **Codex daemon.** Codex 0.157.0 "Enable[d] automatic daemon startup by default" (#47179, https://github.com/openai/codex/releases/tag/rust-v0.157.0). That daemon is the socket Pocket rides.
- **Coverage gaps.** Pocket treats `exec`, `--no-daemon`, `-p`, `--remote` and similar as "embedded", with no attach (`P pocketd/internal/daemon/codex.go:37-52`). Pocket's `codex.Session.Request` ignores every other server request, which is safe only because the TUI answers them (`P pocketd/internal/codex/session.go:251-270`).

### 11. What MonoCode does that Pocket must not copy blindly

- **Claude respawn on every settings change** costs a cold start of about 1–3 s plus `--resume` (inference). MonoCode also accepts an 8 s init timeout silently (`M …/claude/claude.ts:1659-1671`).
- **Host process-per-turn** loses warm caches and background tasks between turns. MonoCode chose it for crash safety on the remote host, not on desktop (`M host/engine.ts:818-846` vs `M …/core/registry.ts:110`).
- **Snapshot-on-`tool.started`** depends on the edit tool reporting paths before it runs. Under full-access nothing blocks the tool, so the snapshot races the tool's own execution (inference; see Open questions).

### 12. Minimal path: headless-driven GUI sessions in Pocket (claude + codex)

Goal: pocketd owns a headless provider process, and the phone ChatScreen and a desktop pane are the only UI. Everything below uses messages MonoCode ships.

1. **Registry entry, no terminal.**
   - Call `d.Agents.AddFunc(id, cwd, provider, newDriver)` with `TerminalID ""` (`P pocketd/internal/agent/agent.go:65-74,168`).
   - `Record`, `Working`, `NeedsYou`, `TurnEnded`, `Clear`, `SetConversation`, `SetModel`, the broker, hub and timeline are reused unchanged (`P pocketd/internal/agent/agent.go:207-332`).
   - A new `Driver` implements `Prompt`, `Interrupt`, `Compact` and `Close` (`P pocketd/internal/agent/agent.go:16-21`).
2. **Spawn** (new `pocketd/internal/headless`):
   - `exec.Command(bin, args…)` with `Dir=cwd`, `SysProcAttr{Setpgid:true}`, and stdin/stdout pipes read by a line scanner (8 MiB cap, as in `M host/child-backend.ts:311-339`).
   - Env: login-shell env, minus `CLAUDE_CODE_PLUGIN_DIRS` pointing at Pocket's plugin and minus `POCKETD_SOCK`/`POCKETD_PTY`, so hooks don't double-report or grab permissions (`P pocketd/internal/daemon/plugin.go:65-82`).
   - Add a marker `POCKETD_HARNESS_PARENT=<pid>` so the orphan reaper can find leftovers (`M src-tauri/src/harness.rs:1012,1239-1270`).
   - Kill: `kill(-pgid, SIGTERM)`, then SIGKILL after 2 s (`M src-tauri/src/harness.rs:1005,1161-1194`).
3. **Claude driver** (`M …/claude/claudeProtocol.ts:239-316`, `M …/claude/claude.ts:302-334,499-502,921-1047`):
   - **Spawn:** `claude --output-format stream-json --verbose --input-format stream-json --permission-prompt-tool stdio --include-partial-messages --setting-sources=user,project,local --permission-mode <default|acceptEdits|auto|bypassPermissions|plan> [--allow-dangerously-skip-permissions] [--model M] [--effort E] (--session-id <uuid>|--resume <id>)`.
   - **Handshake:** write `{"type":"control_request","request_id":"pocket_1","request":{"subtype":"initialize"}}`. Wait ≤8 s for `{"type":"system","subtype":"init",…}` or a `control_response`, then `SetConversation(session_id)`.
   - **`Prompt(text)`:** write `{"type":"user","session_id":"","parent_tool_use_id":null,"message":{"role":"user","content":[{"type":"text","text":…}]}}` and call `Working()`. If a turn is active, the same write is a steer (`M …/claude/claude.ts:265-278`).
   - **Streaming:**
     - `stream_event.content_block_delta.text_delta` and `thinking_delta` become incremental `assistant_text`/`thinking`.
     - Full `assistant`/`user` lines become `tool_start`/`tool_end`. These have the same `message.content` shape `claude.Map` already parses from transcripts (`P pocketd/internal/claude/transcript.go:57-112`). This is inferred and needs a fixture test.
     - `result` → `Kind:"result"` plus `TurnEnded(subtype!="success")`. `aborted_*` counts as an interrupt, handled via `Clear()`.
   - **Permissions:** inbound `control_request` with `subtype:"can_use_tool"` → `NeedsYou()` and `broker.Ask(PermissionRequest{ToolName, Detail: timeline.Detail(tool_name,input)})`.
     - Reply `{"type":"control_response","response":{"subtype":"success","request_id":R,"response":{"behavior":"allow","updatedInput":<input>}}}` or `{"behavior":"deny","message":…}`.
     - Every other subtype gets the empty success response.
     - `control_cancel_request` → `broker.Dismiss`.
   - **`AskUserQuestion`:** `allow` with `updatedInput:{questions, answers:{prompt:label}}`.
   - **`Interrupt()`:** send a `stop_task` for each background task, then `{"type":"control_request","request_id":"pocket_N","request":{"subtype":"interrupt"}}`.
   - **`Compact()`:** `Prompt("/compact")` plus `SetCompacting()`. Completion is confirmed by a `system` compact subtype.
   - **Settings change:** `Close()`, then respawn with `--resume`.
4. **Codex driver** (`M …/codex/codex.ts:517-724,1139-1336`, `M …/codexProtocol.ts:62-181`):
   - **Spawn:** `codex app-server`. Frames are one JSON object per line with `id`/`method`/`params`/`result` and no `"jsonrpc"`. Pocket's `frame` struct matches for reading, but its `Error` has only `message` and `Client.Reply` sends only `result` (`P pocketd/internal/codex/rpc.go:34-42,147-150`). Replying `-32601` needs an error `code` field and an error-reply method. Otherwise only the transport changes, from WebSocket over unix socket to stdio lines.
   - **Handshake:** `initialize {clientInfo:{name:"anywhere",title:"Anywhere",version}, capabilities:{experimentalApi:true}}`, then notify `initialized`. Pocket currently sends `clientInfo.name:"codex_app_server_daemon"` and `capabilities:nil` (`P pocketd/internal/codex/rpc.go:59`).
   - **Thread:** `thread/start {cwd, approvalPolicy, sandbox, sandboxPolicy:{type}, approvalsReviewer, model?}`, or `thread/resume {threadId, …same}` falling back to start. `SetConversation(thread.id)`.
   - **`Prompt`:** `turn/start {threadId, input:[{type:"text",text}], approvalPolicy, approvalsReviewer, sandboxPolicy, collaborationMode:{mode:"default",settings:{model,reasoning_effort,developer_instructions:null}}, model?, effort?}`.
     - `turn/started` → `Working()`. `turn/completed`/`turn/aborted` → `TurnEnded`.
     - Existing `codex.Session.Notify`/`started`/`completed` build the timeline unchanged (`P pocketd/internal/codex/session.go:139-243`).
   - **Server requests:**
     - Keep `accept`/`decline` for the two approval methods (`P pocketd/internal/codex/session.go:251-279`).
     - Add `currentTime/read → {currentTimeAt}`, `item/permissions/requestApproval → {scope:"turn",permissions}|{permissions:{}}`, `item/tool/requestUserInput → {answers}`, and `mcpServer/elicitation/request → {action:"decline",content:null,_meta:null}`.
     - Anything else gets error `-32601`. Without a TUI, an unanswered request hangs the turn.
   - **`Interrupt()`:** `turn/interrupt {threadId, turnId}`. **`Compact()`:** `thread/compact/start {threadId}`. Steer is `turn/steer {threadId, expectedTurnId, input}`.
5. **Protocol:**
   - Add `agent.create {provider:"claude"|"codex", cwd, model?, effort?, mode}` → `ack` + `agent.update`.
   - Add `agent.configure {agentId, model?, effort?, mode?}`.
   - Add `question.answer {requestId, answers|skip}`. Today only `permission.resolve` exists (`P pocketd/internal/proto/messages.go:62-83`), in `packages/protocol` and `pocketd/internal/proto`.
   - `agent.prompt/interrupt/compact/close/timeline` work as-is.
6. **UI:**
   - The phone's ChatScreen already renders timelines.
   - The desktop needs a transcript pane for agents with an empty `terminal_id`. The `agents` crate already pulls `agent.timeline` (limit 500) and upserts stream items (`P desktop/crates/agents/src/agents.rs:242-290`).
7. **Robustness** (in order of value):
   - Persist `{agentId, provider, cwd, providerSessionId, settings}` so a pocketd restart can resume, marking an in-flight turn "interrupted" without replay (`M host/engine.ts:264-289`).
   - Idle-park after 5 min.
   - Idempotent `commandId` on `agent.prompt` for flaky phone links (`M host/engine.ts:417,704`).

## Ideas to clone into Pocket

| ID | Idea | User value | Evidence | Pocket mapping | Effort | Prerequisites |
|---|---|---|---|---|---|---|
| 10-1 | Headless Claude driver (stream-json + `can_use_tool` control protocol) | Start and drive Claude from the phone or desktop without a TUI; real approvals instead of hook plus typing | `M …/claude/claudeProtocol.ts:239-316`, `M …/claude/claude.ts:374-514,921-1047` | pocketd `internal/headless/claude` (new), implements `agent.Driver`; reuse `claude.Map` | L | 10-5, 10-6 |
| 10-2 | Headless Codex driver on private `codex app-server` stdio | Same for Codex; per-turn model, effort and mode with no respawn | `M …/codex/codex.ts:517-724`, `M …/codexProtocol.ts:97-181` | pocketd `internal/codex` (adapt): stdio transport for `Client`, reuse `Session`, extend `Request` | M | 10-5, 10-6 |
| 10-3 | `agent.create` / `agent.configure` / `question.answer` messages | Pick provider, folder, model, effort and mode when starting a session | `M host/engine.ts:78-248` (create/configure/answer commands) | `packages/protocol` + `pocketd/internal/proto` + wsserver (adapt) | M | 10-1 or 10-2 |
| 10-4 | Four-mode permission matrix, `default` sent explicitly | Predictable safety level per session; no silent `auto` from user config | `M …/claudeProtocol.ts:105-126`, `M …/codexProtocol.ts:62-95` | pocketd `internal/headless` (new table) | S | 10-1, 10-2 |
| 10-5 | Process group, SIGTERM→2 s→SIGKILL, parent-marker env and orphan reaper at startup | No stray `claude`/`codex` after a pocketd crash or quit | `M src-tauri/src/harness.rs:1005-1045,1161-1194,1239-1270` | pocketd `internal/proc` (adapt) + daemon startup | S | — |
| 10-6 | Login-shell env capture (`$SHELL -lic printenv`, 5 s) and binary fallbacks (`Codex.app` bundle) | Sessions started from the phone find the same CLIs as the user's shell under launchd (MonoCode imports only `PATH` plus 5 gateway/xAI vars, not Anthropic/OpenAI keys) | `M src-tauri/src/harness.rs:1729-1759,2345-2428,2522-2561` | pocketd `internal/envdir` / `terminal.LookPath` (adapt) | S | — |
| 10-7 | Persist headless sessions; resume after pocketd restart; mark in-flight turns interrupted, no replay | Conversations survive a daemon or Mac restart | `M host/store.ts:30-36`, `M host/engine.ts:264-289`, `M src-tauri/src/session_store.rs:552-584` | pocketd new store (JSON/SQLite) | M | 10-1, 10-2 |
| 10-8 | Idle-park children after 5 min; respawn with `--resume` / `thread/resume` | Lower RAM and battery with many idle sessions | `M …/core/registry.ts:110`, `M …/claude/claude.ts:374-394` | pocketd `internal/headless` | S | 10-7 |
| 10-9 | Steer a running turn | Correct a drifting agent without cancelling | `M …/claude/claude.ts:265-278`, `M …/codexProtocol.ts:121-132` | proto `agent.prompt` while working → steer (adapt) | S | 10-1, 10-2 |
| 10-10 | Structured questions (`AskUserQuestion` via `updatedInput`, Codex `requestUserInput`) | Answer agent questions on the phone as option pickers | `M …/claude/claude.ts:948-985`, `M …/claudeProtocol.ts:899-915`, `M …/codex/codex.ts:1146-1200` | broker + proto `question.*` (new) | M | 10-3 |
| 10-11 | Model catalog probe (Claude `list_models`, Codex `model/list`), cached 5 min and keyed by binary realpath+mtime | Accurate model picker without hardcoding | `M …/claude/claudeCatalog.ts:248-320`, `M …/codex/codexCatalog.ts:124-137`, `M host/server.ts:69-70,116-125` | pocketd new `models` op | S | 10-3 |
| 10-12 | Desktop transcript pane for terminal-less agents | Headless sessions visible and drivable on the Mac | `M src/integrations/harness/core/types.ts:13-149` (event model) | desktop `crates/pocket` new view over `crates/agents` Items | L | 10-1 or 10-2 |
| 10-13 | Idempotent prompt commands (`commandId` + sha256 receipt) | A phone retry after a network drop never double-sends a prompt | `M host/engine.ts:417,444-713` | proto `agent.prompt.commandId` + pocketd receipt map (new) | M | — |
| 10-14 | Per-session file checkpoints with per-file Keep/Undo and foreign-edit refusal | Undo one agent's edits without git stash or reset | `M src-tauri/src/checkpoint.rs:49-439`, `M src/app/App.tsx:11263-11284` | pocketd new `checkpoint` pkg + desktop `crates/pocket/src/changes.rs` (exists on `main` since b9d14a1; absent at this worktree's 86deb13) | L | 10-1 or 10-2 (tool events with paths) |
| 10-15 | Codex rewind last turn (`thread/turns/list` → `thread/revert`) | "Undo last message" for Codex conversations | `M …/codex/codex.ts:196-252` | pocketd `internal/codex` (adapt) | S | 10-2 |
| 10-16 | Plan intent (Claude `--permission-mode plan` + deny `ExitPlanMode`; Codex `collaborationMode.mode:"plan"`) | Review a plan on the phone before any edit | `M …/claude/claude.ts:987-1015`, `M …/codexProtocol.ts:150-172` | pocketd `internal/headless` + timeline `plan` item | M | 10-1, 10-2 |
| 10-17 | Batch streamed deltas every 120 ms | Less WS chatter and phone battery use during long outputs | `M host/engine.ts:37-45,868-883` | pocketd `hub` publish coalescing (adapt) | S | — |
| 10-18 | Provider account profiles (`CLAUDE_CONFIG_DIR`/`CODEX_HOME`, API-key env stripped) | Work and personal accounts side by side | `M src-tauri/src/harness.rs:581-699` | pocketd `internal/envdir` | M | 10-1, 10-2 |
| 10-19 | SSH-forwarded remote host with per-device tokens | Run agents on another machine | `M host/server.ts:193-231`, `M src-tauri/src/remote_ssh.rs:166-205` | n/a (Pocket's model is Mac + phone) | XL | — |

## Open questions / risks

- **Claude CLI drift.** `--permission-prompt-tool stdio`, `control_request initialize`, `list_models` and `stop_task` are undocumented SDK internals. MonoCode gates them by CLI version (`M …/claudeProtocol.ts:29-34`). Pocket needs a version floor and fixture tests.
- **Codex daemon coexistence.** A private `codex app-server` and the auto-started account daemon share one `CODEX_HOME` (0.157.0 notes). Unverified:
  - whether concurrent writers are safe;
  - whether Pocket could instead `thread/start` + `turn/start` on the daemon socket it already dials (`P pocketd/internal/codex/rpc.go:162-165`);
  - which client receives server requests in that case.
- **Hooks in headless Claude.** If the user's hooks, or Pocket's plugin through inherited `CLAUDE_CODE_PLUGIN_DIRS`, load, a `PermissionRequest` hook may decide before the stdio prompt tool. It is unverified which one wins. Pocket must strip its own plugin dir.
- **Stream-json vs transcript parity.** Reusing `claude.Map` on stream-json `assistant`/`user` lines is an inference. Needs a recorded fixture.
- **Glossary.** CONTEXT.md defines Agent as "a claude or codex process running in a terminal". Headless agents need a glossary change: Session without Terminal, and what "close" means.
- **Trust prompt.** Claude "skips hooks in a folder the user hasn't trusted" (`P pocketd/internal/daemon/presence.go:53-55`). Headless behavior in untrusted folders is unverified.
- **Full-access from the phone** is remote code execution with no dialog. Needs explicit per-session opt-in, plus `--allow-dangerously-skip-permissions`.
- **Checkpoint race.** Snapshotting on `tool.started` may lose the race against auto-approved edits, since nothing blocks the tool in full-access (`M src/app/App.tsx:11263-11284`). Unverified.
- **`approvalsReviewer:"auto_review"`** (Codex `auto` mode) depends on the CLI version. Behavior on older CLIs is unknown.
- **No replay after a crash** (MonoCode's choice) leaves half-applied work. Pocket should show "interrupted, inspect work", not auto-retry.

## Verification

Date: 2026-09-30. Claims checked: 34. Corrected: 9.

Confirmed against source: Claude spawn args, `default` permission mode and version gates; the `initialize`/`can_use_tool`/`stop_task`/`interrupt` control flow; the 8 s init wait that resolves on timeout; respawn on settings change and the cwd-bound resume. Also confirmed: Codex `experimentalApi`, the mode map including plan and full-access `on-request`, `turn/start` params and the control methods; harness kill, marker and reaper constants, the exec allowlist and `-lic printenv`. On the host: port 3774, `/rpc` with Origin 403, the 43-char Bearer token, 120 ms flush, the restart message, process-per-turn `bind`, SIGKILL after 3 s, and SQLite tables. Plus 500-file checkpoints, `rewindLastTurn` providers, and Codex 0.157.0 #47179. Pocket: 150 ms Enter, ESC interrupt, `/compact`, `capabilities:nil`, socket path, 3 s map window, embedded flags, 610 s hook timeout, 100 ms tail, `Driver`/`AddFunc`, proto message list, desktop timeline limit 500, `WriteFile` writers, and CONTEXT.md's Agent definition.

- Login-shell import: the 5 extra vars are gateway/xAI/Vercel credentials, not Anthropic/OpenAI API keys. Named them (`harness.rs:2474-2483`).
- Provider profiles strip more vars than stated (`ANTHROPIC_AUTH_TOKEN`, `CLAUDE_CODE_OAUTH_TOKEN`, `CODEX_API_KEY`, `CODEX_ACCESS_TOKEN`). Fixed the range to `:676-692`.
- `QUESTION_AUTO_RESOLVE_MS` is at `codex.ts:67`, not `:66`.
- Remote polling: 10 s also applies when the window is hidden. Failures back off as `750·2^n` ms, capped at 10 s. Fixed in the TL;DR and §8.
- Claude `permission` control subtype is handled like `can_use_tool`. Added.
- Pocket `frame.Error` has no `code` and `Client.Reply` sends only `result`, so the claim that it "already matches" was too strong for sending `-32601`. Qualified in §12.4.
- Pocket's Codex `clientInfo.name` is `codex_app_server_daemon`. Added.
- Idea 10-14: `changes.rs` is absent from this worktree (86deb13) and exists only on `main` (b9d14a1). Mapping now says so.
- Idea 10-6 user value claimed MonoCode imports "keys" from the shell. Narrowed to CLIs, per the first correction. The Pocket wire list also lacked `hello`; added.
