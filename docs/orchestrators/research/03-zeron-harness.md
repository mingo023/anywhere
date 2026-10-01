# Zeron: how it drives each agent (harness)

Date: 2026-09-30.

Sources:
- Zeron: `zeronsh/comet` @ `ed3b1aae4a5189eef67143db7b8c5c3ee7a933c5` (shallow clone, one commit "Bump version to v0.2.99", 2026-09-29), MIT, at `/Users/mingo/tmp/orchestrators/zeron`.
- Pocket: this worktree @ `86deb13`.
- Installed CLIs on this Mac: Claude Code 2.1.285, codex-cli 0.159.0. Only `--help`, `--version` and flag-parse checks were run. No model calls.

Citation legend: `Z path:L` is a file in the Zeron clone. `P path:L` is a file in this Pocket worktree, relative to its root. `M path:L` is the monocode clone (not used in this report). `CLI claude 2.1.285` / `CLI codex 0.159.0` is output of the installed binary. A URL is an official page. Code beats docs; every place where they disagree is flagged **(code ≠ docs)**.

## TL;DR

- **Zeron never uses a PTY or TUI.** One Rust `Harness` trait turns every agent into a stream of `AgentEvent`s. Each provider uses its own native wire:
  - Claude: stream-json.
  - Codex: app-server JSON-RPC.
  - Cursor: a Node shim over `@cursor/sdk`.
  - Pi: JSONL RPC.
  - OpenCode: its own HTTP/SSE server protocol (Z crates/harness/src/lib.rs:6-7; Z crates/harness/src/opencode/mod.rs:1-10). Not covered in the matrices below.
  - ACP only for agents built on ACP: Devin, Grok, Hermes, Antigravity (Z crates/harness/src/lib.rs:3-15).
- **(code ≠ docs)** `docs/research/acp.md:16-31` says claude and codex moved to ACP adapters on 2026-08-08. The code retired that route: the adapters "held prompt turns open for background work… manufacturing done-status bugs" (Z crates/harness/src/lib.rs:11-15).
- **Claude recipe.** Zeron spawns this, then writes prompts and steers as stdin JSON lines (Z crates/harness/src/claude/mod.rs:167-243):

  ```
  claude --print --input-format stream-json --output-format stream-json --verbose --include-partial-messages --replay-user-messages --thinking-display summarized --permission-prompt-tool stdio [--permission-mode default] [--resume=<id>] [--model <m>] [--effort <e>] [--mcp-config <json>]
  ```

  Permissions arrive as `can_use_tool` control requests on stdout.
- **Codex recipe.** Spawn `codex app-server` (stdio JSON-RPC 2.0), then:
  1. `initialize` with `capabilities.experimentalApi`.
  2. `thread/start` or `thread/resume`.
  3. `turn/start` with `summary:"auto"`.
  4. `turn/steer` / `turn/interrupt` as needed.

  Approvals come as server requests (Z crates/harness/src/codex/mod.rs:1-37, 987-1155, 1493-1590, 1697-1773).
- **Zeron runs yolo.** Claude `can_use_tool` is always allowed. Codex runs with `approvalPolicy:"never"` and `danger-full-access`. ACP picks the preferred allow option. Only questions reach the user: AskUserQuestion, requestUserInput, Pi dialogs (Z crates/harness/src/claude/mod.rs:976-990; Z crates/harness/src/codex/mod.rs:970-976; Z crates/harness/src/acp/mod.rs:2949-2961). Pocket's Needs you needs the opposite, and the plumbing for it exists on both sides.
- **PO answer: yes, Pocket can add a GUI transcript + composer mode next to terminal mode.** Most parts already exist:
  - the `agent.Driver` interface (`Prompt`/`Interrupt`/`Compact`/`Close`);
  - the timeline event model;
  - `agent.stream` / `agent.timeline`;
  - the phone ChatScreen + Composer;
  - the permission broker;
  - a codex app-server client (P packages/pocketd/internal/agent/agent.go:16-21; P packages/pocketd/internal/codex/session.go:72-95).
- **Codex: no second process needed.** Pocket already joins the account daemon's socket as a second client (P packages/pocketd/internal/codex/rpc.go:44-68). Its spike verified that `turn/start` from pocketd shows up in the TUI (P docs/spike-remote-sessions.md:75). So a GUI composer and the terminal can drive the **same live thread**. The minimal path is to add `turn/start` / `turn/steer` / `turn/interrupt` to `codex.Session`.
- **Claude: a separate process with no PTY.** A stream-json process can't attach to a running TUI. GUI mode means pocketd owns a headless `claude --print …` child. Moving between modes happens at Idle via `--resume=<session_id>`, which is cwd-scoped (Z docs/research/harness.md:35).
- **Worth copying:**
  - Claude steer priority: `now` when no tool is open, `next` otherwise.
  - Held-Done: 5 s settle while steers are unconfirmed.
  - Codex `turn/steer` with `expectedTurnId`.
  - Queue actions Steer / Send next / Send now.
  - Context ring: amber at 75%, red at 90%.
  - Question panel that replaces the composer.

  Evidence: Z crates/harness/src/claude/wire.rs:219-230; Z crates/harness/src/claude/mod.rs:767-840; Z crates/ui/src/queue.rs:91-104; Z docs/context-usage.md:3-8.
- **Risks:**
  - `--permission-prompt-tool stdio` is undocumented. It is accepted by 2.1.285, and 2.1.285's new `--permission-prompts host|none` flag refers to it (CLI claude 2.1.285).
  - `codex app-server` is labelled `[experimental]` (CLI codex 0.159.0).
  - Versions differ: Zeron validated codex 0.153.4, Pocket targets ≥ 0.157, 0.159.0 is installed.

## Findings

### 1. Architecture: one trait, headless children, events

**Harness trait** (Z crates/harness/src/lib.rs:82-168):
- Identity and capabilities: `id`, `display_name`, `supports_steering`, `steering_mode` (StepBoundary | TurnBoundary), `reasoning_levels`, `installed`, `executable_path`, `deterministic_turn_end`, `authoritative_prompt_end`.
- Catalogs: `models` / `model_catalog` / `fallback_models`, `commands` / `commands_for(cwd)`, `skills(cwd)`.
- `run_title`.
- `run(RunRequest, RunControls) -> BoxStream<AgentEvent>`.

**RunControls** (Z crates/harness/src/lib.rs:50-66):
- `request_input: Fn(Vec<UserInputQuestion>) -> oneshot::Receiver<Vec<UserInputAnswer>>`.
- `steering: mpsc::Receiver<SteerMessage{prompt,message_id}>`.
- `interrupt: CancellationToken`.
- `execution_lease`.

**RunRequest** (Z crates/proto/src/agent.rs:213-254):
- `prompt`, `harness?`, `model?`, `reasoning?`, a `model_options` map, `cwd`, `sandbox` (ReadOnly | WorkspaceWrite | DangerFullAccess), `auto_approve`, `resume?`.
- `attachments`: absolute paths, also repeated in the prompt text as "Attached images (local files …)".
- `worktree?`, `mcp? {name,command,args,env}`.

**AgentEvent** (Z crates/proto/src/agent.rs:484-601):

| Group | Events |
|---|---|
| Session | `SessionStarted{harness,model,tools,cwd,session_id,assistant_message_id}` |
| Streaming | `TextDelta`, `ReasoningDelta`, `GeneratedImage`, `AssistantMessageCompleted` |
| Tools | `ToolCall{id,call}`, `ToolResult{id,is_error,output?,diff?}` |
| Usage | `ContextUsage{tokens?,window?}`, `Usage{input_tokens,output_tokens}` |
| Commands / errors | `AvailableCommands`, `Error` |
| Input | `InputRequested`, `InputResolved` |
| Steering / end | `Steered{assistant_message_id,next_assistant_message_id}`, `Done{status Completed\|Interrupted\|Errored, result, error, session_id}` |
| Other | `UserMessage`, `Subagent{parent_tool_use_id, event}` |

- Tool kinds: Exec, ReadFile, WriteFile, EditFile, ApplyPatch, Search, Glob, WebFetch, WebSearch, Todo, Mcp, Unknown (Z crates/proto/src/agent.rs:300).
- Reasoning ladder: Minimal, Low, Medium, High, XHigh, Max, Ultra, Ultracode, Ultrathink.

**Shared child hygiene** (Z crates/harness/src/lib.rs:195-434):
- PATH = exe dir + own PATH + login-shell PATH.
- `StderrTail`: 6 lines × 700 bytes.
- Crash copy: "`<name>` exited unexpectedly (`<status>`): `<tail>`", redacted.
- `shutdown_child`: SIGTERM to the process group, then SIGKILL after `kill_grace`.

**Engine session model** (Z crates/engine/src/sessions.rs:1950-2110):
- **Persistent session.** A completed turn on a steerable harness parks: the child and its steering mailbox stay warm, and the next message starts the next turn with no respawn.
- **Idle reaper.** Ends a parked session after 30 min (`ZERON_SESSION_IDLE_MS`). A live subagent stretches the window to 8× (4 h).
- **No per-turn stall timeout**, by design ("agents may legitimately be quiet for >10min"). A 15 s live heartbeat feeds the UI's 45 s staleness gate.
- **Turn-quiesce watchdog.** 120 s, and 20 s for self-continued turns. **Retired for native drivers** (`deterministic_turn_end`); ACP keeps it (Z crates/engine/src/sessions.rs:1980-2031).

**Persistence** (Z crates/engine/src/run_journal.rs:1-11):
- One append-only JSONL journal per chat, `{"seq":n,"event":AgentEvent}`.
- It is the replay source for live subscribers.
- A journal whose last event is not `Done` is a crashed run: boot stamps it `aborted` and appends a synthetic `Done`.

**Transcript** lives in a Loro `SessionDoc` synced to devices (Z crates/doc/src/schema.rs:318-321). The queue is a movable list in the same doc (Z docs/reference/message-queue.md:9-15).

### 2. Code vs docs (code wins)

| Doc says | Code says |
|---|---|
| claude and codex run through ACP adapters `claude-agent-acp` 0.66.0 / `codex-acp` 1.1.14 (Z docs/research/acp.md:16-31) | Native stream-json and app-server drivers. ACP for claude/codex/cursor retired (Z crates/harness/src/lib.rs:3-15). Claude driver "resurrected from the pre-ACP driver… modernized against CLI 2.1.228" (Z crates/harness/src/claude/mod.rs:1-4) |
| Pi via ACP (Z docs/research/acp.md:33-35); context table lists "ACP (Devin, Grok, Hermes, pi)" (Z docs/context-usage.md:17) | Native `pi --mode rpc` JSONL driver, Pi ≥ 0.85.1 (Z crates/harness/src/pi/mod.rs:1, 122-139) |
| "Claude: Static model catalog; no model-discovery subprocess" (Z docs/research/model-discovery-reliability.md:44) | Live discovery through the `initialize` control request's `response.models`, merged over a curated fallback (Z crates/harness/src/claude/mod.rs:267-342; Z docs/research/model-catalog-validation.md round two, item 1) |
| Antigravity pinned 1.1.1 (ACP doc) | Archive pin is `agy-acp-server-1.2.1`. The 1.1.1 note survives only as the reason for TurnBoundary (Z crates/harness/src/acp/mod.rs:420-445, 1101-1104) |

### 3. Per-provider matrix: transport and turn control

| Provider | Launch / transport | Resume | Steering | Interrupt | Turn end |
|---|---|---|---|---|---|
| Claude Code | `claude --print --input-format stream-json --output-format stream-json …` (full list in §4), JSONL on stdio (Z crates/harness/src/claude/mod.rs:167-243) | `--resume=<session_id>` (Z crates/harness/src/claude/mod.rs:219) | StepBoundary. Stdin user line with `uuid` + `priority:"now"\|"next"` (Z crates/harness/src/claude/wire.rs:219-230) | `control_request{subtype:"interrupt"}` → SIGTERM after 2 s → SIGKILL after 3 s (Z crates/harness/src/claude/mod.rs:884-899) | CLI `result` frame, eager. A background wake turn emits a second `result` (Z crates/harness/src/claude/mod.rs:16-20) |
| Codex | `codex app-server`, JSON-RPC 2.0 stdio, `experimentalApi` (Z crates/harness/src/codex/mod.rs:1-37, 213-217) | `thread/resume{threadId,…}`; on failure falls back to `thread/start` (Z crates/harness/src/codex/mod.rs:1072-1095) | StepBoundary. `turn/steer{threadId,expectedTurnId,input}` (Z crates/harness/src/codex/mod.rs:1493-1566) | `turn/interrupt{threadId,turnId}` → signals (Z crates/harness/src/codex/mod.rs:1576-1590) | `turn/completed` / `turn/failed` / `turn/aborted` |
| Cursor | `node shim.mjs` over pinned `@cursor/sdk@1.0.32`, JSONL (Z crates/harness/src/cursor/mod.rs:1-30, 60) | SDK `Agent.resume` (Z docs/mcp.md:68) | StepBoundary, SDK `Run.steer` (Z crates/harness/src/cursor/shim.mjs:502) | shim `interrupt` op | shim `turn` frame off `run.wait()` (Z crates/harness/src/cursor/mod.rs:21-22) |
| Devin | `devin acp`, ACP v1 (Z crates/harness/src/acp/mod.rs:282-345) | `session/load`, with a fresh-session fallback | StepBoundary via preempt: `session/cancel`, then re-prompt with the steer (Z crates/harness/src/acp/mod.rs:4656-4681) | `session/cancel` → signals (Z crates/harness/src/acp/mod.rs:4686-4700) | `session/prompt` response `stopReason` |
| Grok | `grok --no-auto-update agent --no-leader stdio`. Flags avoid a stale shared leader and the update stall (Z crates/harness/src/acp/mod.rs:205-265) | same | StepBoundary (preempt) | same | same, plus prompt-complete extension and a 30 s prompt-stall hint |
| Hermes | `hermes acp` (Z crates/harness/src/acp/mod.rs:358-414) | same | TurnBoundary (no `_session/steering`) | same | same |
| Antigravity | `agy_acp_server` (`--uid=` on Linux) from the pinned 1.2.1 archive, sha512-checked (Z crates/harness/src/acp/mod.rs:420-445, 1069-1118) | same | TurnBoundary: a cancel during reply wedges the server (Z crates/harness/src/acp/mod.rs:1101-1104) | same | same |
| Pi | `pi --mode rpc --no-themes`, JSONL, ≥ 0.85.1 (Z crates/harness/src/pi/mod.rs:122-139) | `--session <abs path>` (Z crates/harness/src/pi/sessions.rs:95) | StepBoundary. `prompt{streamingBehavior:"steer"}` (Z crates/harness/src/pi/mod.rs:491) | `clear_queue` + `abort` (Z crates/harness/src/pi/mod.rs:723-728) | `agent_settled`, not `agent_end` (Z crates/harness/src/pi/PROTOCOL.md:6-8) |

**ACP details:**
- The `_session/steering` extension is used when `initialize._meta.steering.supported` is set (Z crates/harness/src/acp/mod.rs:2497-2504).
- Permission ranking is allow_always > allow_once > first (Z crates/harness/src/acp/normalize.rs:486-500).
- The handshake timeout is 120 s, Antigravity discovery 90 s (Z crates/harness/src/acp/mod.rs:66-71).

### 4. Per-provider matrix: user-facing features

| Provider | Approvals | Questions | Models / effort | Context usage | Commands / skills | Subagents | MCP injection | Images |
|---|---|---|---|---|---|---|---|---|
| Claude | Every `can_use_tool` gets `{"behavior":"allow","updatedInput":input}` (Z crates/harness/src/claude/mod.rs:976-990) | `AskUserQuestion` → `request_input`; answers go back as `updatedInput.answers{<question>: label\|[labels]}` (Z crates/harness/src/claude/mod.rs:1013-1082) | `initialize` `response.models` + curated static list + settings/env ids. `--effort low\|medium\|high\|xhigh\|max`; xhigh clamps to max on older models; `<m>[1m]` for 1M; `--settings {"fastMode","alwaysThinkingEnabled","ultracode"}` (Z crates/harness/src/claude/mod.rs:190-243; Z crates/harness/src/claude/catalog.rs) | Last parent assistant `input + cache_read + cache_creation` tokens; window from `result.modelUsage[m].contextWindow` (Z docs/context-usage.md:14) | `initialize` `response.commands[{name,description,argumentHint}]` (Z crates/harness/src/claude/mod.rs:267-342) | Frames with non-null `parent_tool_use_id` → `Subagent`; `task_notification` settles | `--mcp-config '{"mcpServers":{…}}'` without `--strict-mcp-config`, so it merges (Z crates/harness/src/claude/mod.rs:79-92, 541-546) | base64 image blocks ahead of the text, ≤ 5 MiB, png/jpeg/gif/webp (Z crates/harness/src/claude/mod.rs:619-690) |
| Codex | `approvalPolicy:"never"` + `danger-full-access`. A stray `requestApproval` becomes a yes/no question → `{decision:"accept"\|"decline"}` (Z crates/harness/src/codex/mod.rs:696-707, 970-976, 1697-1741) | `item/tool/requestUserInput` → `{answers:{<id>:{answers:[labels]}}}` | `model/list{limit:20,includeHidden:false,cursor}`, paginated. Effort minimal→low … ultra. `serviceTier` Standard/Fast (Z crates/harness/src/codex/mod.rs:240-331; Z crates/harness/src/codex/catalog.rs:12-67) | `thread/tokenUsage/updated` `tokenUsage.last.totalTokens` / `modelContextWindow`, never the thread total (Z crates/harness/src/codex/normalize.rs:140-160) | `skills/list{cwds,forceReload}`. `/compact` → `thread/compact/start`, `/review` → `review/start`. All other TUI commands return an error (Z crates/harness/src/codex/mod.rs:872-922) | `collabAgentToolCall` (v1) / `subAgentActivity` (v2) child threads (Z docs/research/codex-subagents.md:5-17, 64-66) | `thread/start` `config` overrides `mcp_servers.<n>.{command,args,env}` (Z crates/harness/src/codex/mod.rs:91-98) | Paths as prompt text only |
| Cursor | SDK hardcodes `ignoreApprovals` (Z crates/harness/src/cursor/shim.mjs:625-629) | `askQuestion` disallowed: no answer channel (Z crates/harness/src/cursor/shim.mjs:658-661) | SDK `models` mode | Unavailable: SDK reports billed counts only (Z docs/context-usage.md:18) | None; workspace actions only (Z docs/harness-skill-completion.md:41) | SDK `taskUpdate`, tagged by the shim | Inline `mcpServers` + `settingSources` user/team/mdm/plugins (Z docs/mcp.md:68) | none found |
| ACP (Devin, Grok, Hermes, Antigravity) | `session/request_permission` → preferred allow option. Requests with no allow/reject options are treated as questions (Z crates/harness/src/acp/mod.rs:2949-3030) | Question-shaped permission requests | `session/set_config_option` `model` + `thought_level`. Devin: `devin models list`. Antigravity: effort in the model id (Z crates/harness/src/acp/mod.rs:3689-3711) | `usage_update` (Z crates/harness/src/acp/normalize.rs:430) | `available_commands_update` | Devin: `cognition.ai/subagentSupport` (Z crates/harness/src/acp/subagent_devin.rs:1-9). Grok: tails `~/.grok/sessions/…/chat_history.jsonl` (Z crates/harness/src/acp/subagent.rs:1-17) | `session/new` `mcpServers:[{name,command,args,env:[{name,value}]}]` (Z crates/harness/src/acp/mod.rs:2476-2494) | Text only (Z crates/harness/src/acp/mod.rs:2931-2947) |
| Pi | No approval channel. `select/confirm/input/editor` dialogs → question bridge (Z crates/harness/src/pi/ui.rs:13-30) | `extension_ui_request/response` | `get_available_models`, `get_available_thinking_levels`, `set_model`, `set_thinking_level` | Message usage; window from `get_state` `model.contextWindow` (Z crates/harness/src/pi/mod.rs:641, 762) | `get_commands`, including `skill:*` (Z docs/harness-skill-completion.md:39) | none found | Per-run `--extension` bridge (Z crates/harness/src/pi/mcp.rs:11) | Inline images (Z crates/harness/src/pi/mod.rs:650, 959) |

### 5. Claude Code driver in detail

**Executable resolution:**
- `CLAUDE_CODE_EXECUTABLE` overrides.
- Otherwise the first found of: PATH, login-shell PATH, `~/.claude/local/claude`, `~/.local/bin/claude`, `/opt/homebrew/bin/claude`, `/usr/local/bin/claude` (Z crates/harness/src/claude/mod.rs:68-77).

**Flags, and why** (Z crates/harness/src/claude/mod.rs:167-243):
- `--print --input-format stream-json --output-format stream-json --verbose`: keeps stdin open for multi-turn and steers.
- `--include-partial-messages`: `stream_event` text/thinking deltas.
- `--replay-user-messages`: the CLI echoes each stdin user line (with its `uuid`). This is the only steer receipt.
- `--thinking-display summarized`: "Newer Claude models emit no readable thinking text unless a summary is asked for" (Z crates/harness/src/claude/mod.rs:180-183). 2.1.285 allows `summarized|omitted|highlights` per its argument validator; `--help` does not list the flag (CLI claude 2.1.285).
- `--permission-prompt-tool stdio`: undocumented. Routes permission prompts to the stdio control channel, the same transport as the Agent SDK's `query()`. Validated live on 2.1.228 (Z crates/harness/src/claude/mod.rs:8-15, 184-189). 2.1.285 still accepts it. Its help doesn't list it but references it under `--permission-prompts host|none` (CLI claude 2.1.285).
- Permission mode:
  - `auto_approve` → `--permission-mode bypassPermissions --dangerously-skip-permissions`.
  - otherwise `--permission-mode default` (Z crates/harness/src/claude/mod.rs:209-216).
  - **Version drift:** 2.1.285 lists `acceptEdits|auto|bypassPermissions|manual|dontAsk|plan` and not `default`, but still accepts `default`; `bogus` is rejected (CLI claude 2.1.285).
- Title mode adds `--system-prompt … --tools "" --strict-mcp-config --mcp-config {"mcpServers":{}} --setting-sources ""` (Z crates/harness/src/claude/mod.rs:529-540).

**Stdin lines** (Z crates/harness/src/claude/wire.rs:210-230):

```json
{"type":"user","message":{"role":"user","content":"<text or blocks>"},"parent_tool_use_id":null}
{"type":"user","uuid":"<id>","priority":"now","message":{"role":"user","content":"…"},"parent_tool_use_id":null}
{"type":"control_request","request_id":"int_1","request":{"subtype":"interrupt"}}
{"type":"control_response","response":{"subtype":"success","request_id":"<id>","response":{"behavior":"allow","updatedInput":{…}}}}
```

- Deny shape: `{"behavior":"deny","message":…}` (Z docs/research/harness.md:30-31).
- Other client→CLI control subtypes: `set_permission_mode`, `set_model`, `get_context_usage`, `mcp_*`, `rewind_files`, `stop_task` (Z docs/research/harness.md:28-29).

**Steer priority:**
- `now` when no tool is open: stops streaming at once.
- `next` while a tool is open, because `now` also aborts in-flight MCP tools. Verified on CLI 2.1.280 (Z crates/harness/src/claude/wire.rs:219-230; Z crates/harness/src/claude/wire.rs:303-309).

**Held Done:**
- A `result` that lands while steers are unconfirmed is held until the steer's echoed user frame (matching `uuid`) arrives, or `HELD_DONE_SETTLE` = 5 s passes.
- Then `Steered{prev,next}` rotates the assistant message (Z crates/harness/src/claude/mod.rs:767-840).

**Stdout frames** (Z crates/harness/src/claude/wire.rs:12-22, 82-189):
- `system` (init: model, tools, cwd, session_id; `task_started` / `task_notification`).
- `stream_event` (`event.delta.text` / `.thinking`, `parent_tool_use_id`).
- `assistant` / `user` (`message.content` blocks, `uuid`, `parent_tool_use_id`, `error`).
- `rate_limit_event`.
- `result` (`subtype`, `result`, `errors`, `usage`, `modelUsage`, `session_id`).
- `control_request` (`request_id`, `request.subtype` / `tool_name` / `input`).
- Errors: `authentication_failed | oauth_org_not_allowed | billing_error | rate_limit | overloaded | invalid_request | model_not_found | server_error | max_output_tokens | unknown` → copy (Z crates/harness/src/claude/normalize.rs:13-24).

**Discovery with no model turn** (Z crates/harness/src/claude/mod.rs:267-342; Z crates/harness/src/claude/discovery.rs:37):
1. Spawn `--print --input-format stream-json --output-format stream-json --verbose`.
2. Write `{"type":"control_request","request_id":"zeron-command-probe","request":{"subtype":"initialize"}}`.
3. Read `response.commands` and `response.models[{value,resolvedModel,displayName,description,supportedEffortLevels}]`.

Timeout 10 s; cached 120 s per credential and binary.

**End of life:**
- Dropping the steering mailbox closes stdin; the CLI exits after the turn (Z crates/harness/src/claude/mod.rs:876-881).
- A crash with no `result` → `Done Errored` with the stderr tail.

### 6. Codex driver in detail

Validated against codex-cli 0.153.4 (Z crates/harness/src/codex/mod.rs:1-37).

**Handshake:**

```json
{"method":"initialize","params":{"clientInfo":{"name":"zeron-native","title":"Zeron","version":"…"},"capabilities":{"experimentalApi":true}}}
{"method":"initialized"}
```

**`thread/start`** (Z crates/harness/src/codex/mod.rs:987-1036):
- Params: `cwd`, `approvalPolicy`, `sandbox` (`read-only|workspace-write|danger-full-access`), `model`, `serviceTier` (omitted when default), `config` (dotted overrides).
- Title mode adds `baseInstructions`, `developerInstructions`, `ephemeral:true`, and disables each user MCP server read via `config/read`.

**`turn/start`** (Z crates/harness/src/codex/mod.rs:1131-1155):
- Params: `threadId`, `input:[{type:"text",text},{type:"skill",name,path}…]`, `approvalPolicy`, `sandboxPolicy{type:"readOnly"|"workspaceWrite"(+networkAccess)|"dangerFullAccess"}`, `summary:"auto"`, `model`, `effort`, `serviceTier`.
- Without `summary:"auto"`, codex "thinks in silence for minutes" and the UI's 45 s gate flips Working off (Z crates/harness/src/codex/mod.rs:1140-1144).

**Notifications → events** (Z crates/harness/src/codex/mod.rs:1243-1474):

| Notification | Event |
|---|---|
| `item/agentMessage/delta` | TextDelta |
| `item/reasoning/textDelta` \| `summaryTextDelta` \| `summaryPartAdded` | ReasoningDelta |
| `item/started` / `item/completed` | typed tools; `agentMessage` completed → `AssistantMessageCompleted` |
| `thread/tokenUsage/updated` | ContextUsage + Usage |
| `turn/completed` | Done; the session persists, and queued steers become the next `turn/start` |
| `turn/failed` | Errored |
| `turn/aborted` | Interrupted |

**Steer** (Z crates/harness/src/codex/mod.rs:1493-1566):
- `turn/steer{threadId,expectedTurnId,input}` → `Steered`.
- On rejection the steer is queued, or sent as a fresh `turn/start` if the turn already ended.
- Native commands wait for the turn boundary.

**Why yolo:** "on-request turned every command into a yes/no question" (user report). Also sidesteps a ≤ 0.144 workspace-write mount bug (Z crates/harness/src/codex/mod.rs:696-707, 970-976).

**Subagent flags** (Z docs/research/codex-subagents.md:48-66):
- v1: `-c features.multi_agent=true -c features.multi_agent_v2=false`.
- v2: `features.multi_agent_v2=true` + `agents.enabled=true`.

### 7. Cross-cutting UX mechanics

**Message queue** (Z docs/reference/message-queue.md:9-82):
- Ledger rows: id, text, attachments, hold-until-turn-end.
- Delivery:
  - Idle → the head starts a turn.
  - Working with mid-turn steering → steer a text-only head.
  - No steering → wait for the turn end.
  - Head with attachments → wait for a new turn.
- Explicit Steer never interrupts; Send now interrupts. Edit lease 60 s, renewed every 20 s.
- Defaults: desktop auto-steers, iOS holds.
- Capability-negotiated: `message-queue-v1`, `-actions-v1`, `-attachments-v1`, `-clean-attachment-text-v1`, `-edit-lease-v1`.

**Queue primary action** (Z crates/ui/src/queue.rs:91-120), checked in this order:

| Condition | Action | Tooltip |
|---|---|---|
| head has attachments | Send now | "Send now (interrupt)" |
| mid-turn steer supported | Steer | "Steer (keep current work running)" |
| otherwise (turn-boundary harness) | Send next | "Send at the next turn (this agent cannot steer mid-turn)" |

**Context ring** (Z docs/context-usage.md:3-26):
- Sits under the composer; hover shows tokens and remaining.
- Amber at 75%, red at 90%, a dash when unknown.
- Stored as one atomic `meta.contextUsage` in the doc. Subagents never move the parent meter.

**Completion** (Z docs/harness-skill-completion.md:11-79):
- `/` merges provider commands with workspace actions `/model /new /resume /settings /diff /files /terminal /rename /stop`. On a clash the Zeron action gets a `zeron:` prefix.
- `$` opens skills (default on for Codex only).
- Codex receives typed `{type:"skill"}` inputs.

**Zeron MCP server** (`zeron mcp`): stdio proxy to engine IPC `ws://127.0.0.1:$ZERON_IPC_PORT` (27654). Lets an agent list, create and drive other chats (`create_chat`, `send_message`, `wait_for_turn`, `interrupt_chat`, `respond_to_input`, …) (Z docs/mcp.md; Z crates/mcp).

### 8. Pocket today, against Zeron

| Concern | Pocket (terminal mode) | Zeron (headless) |
|---|---|---|
| Process | Agent TUI in a pocketd PTY, found by a 250 ms foreground poll (P docs/designs/2026-09-28-agent-sessions.md:25-36) | Headless child, no PTY |
| Claude status | Plugin hooks via `CLAUDE_CODE_PLUGIN_DIRS` → ops `hook` (P docs/designs/2026-09-28-agent-sessions.md:63-89) | Stream frames themselves |
| Claude timeline | Tails `transcript_path` JSONL, 100 ms poll (P packages/pocketd/internal/claude/transcript.go:55-89, 132-160). No live deltas: "Claude: không có" (P docs/spike-remote-sessions.md:91) | `stream_event` deltas |
| Codex status / timeline | Watcher + `codex.Session` on the daemon control socket (WebSocket over a unix socket, `clientInfo.name:"codex_app_server_daemon"`); `thread/resume` retry, item mapping (P packages/pocketd/internal/codex/rpc.go:44-68; P packages/pocketd/internal/codex/session.go:72-224) | Own `codex app-server` child |
| Prompt | Types text into the PTY, sleeps 150 ms, sends `\r` (P packages/pocketd/internal/terminal/terminal.go:278-287) | stdin JSON line / `turn/start` |
| Interrupt | Writes `Esc` (P packages/pocketd/internal/daemon/presence.go:130-135) | control_request / `turn/interrupt` |
| Compact | Types `/compact` (P packages/pocketd/internal/daemon/presence.go:144-150) | codex `thread/compact/start`; claude slash text |
| Approvals | Claude `PermissionRequest` hook → broker, with "always" / "switch to auto" options (P packages/pocketd/internal/daemon/permission.go:17-37). Codex `requestApproval` → broker → `accept`/`decline` (P packages/pocketd/internal/codex/session.go:251-279) | Auto-allow |
| Transcript UI | Phone only: `ChatScreen`, `TimelineView`, `Composer` (P packages/app/src/screens/ChatScreen.tsx; P packages/app/src/components/TimelineView.tsx; P packages/app/src/components/Composer.tsx). Desktop already subscribes to `agent.timeline` / `agent.stream` and keeps per-agent items (P packages/desktop/crates/agents/src/agents.rs:118, 274-275), but only derives inbox text, last result and context-left from them; no full transcript view found | Desktop, iOS and web transcript |
| Seams | `agent.Driver{Prompt,Interrupt,Compact,Close}` (P packages/pocketd/internal/agent/agent.go:16-21); `timeline.Event` kinds user, assistant_text, thinking, tool_start, tool_end, result, compacted, error (P packages/pocketd/internal/timeline/timeline.go:12-25) | `Harness` + `AgentEvent` |

### 9. PO question: GUI transcript + composer mode next to terminal mode

**Answer: yes.** It has two shapes, because the two CLIs differ.

**Codex: hybrid, one live thread, both views.**

The codex daemon owns the thread; the TUI is only a client (P docs/spike-remote-sessions.md:105). pocketd is already a second client of that thread (P packages/pocketd/internal/codex/session.go:72-95). The spike showed `turn/start` from pocketd appears in the TUI (P docs/spike-remote-sessions.md:75). `codex app-server proxy` even exposes the daemon socket over stdio (CLI codex 0.159.0).

Minimal path, all in `P packages/pocketd/internal/codex/session.go` plus a new driver:
1. Handle `turn/started` to record `turnId`. Today's `Notify` ignores it (P packages/pocketd/internal/codex/session.go:151-169).
2. `Prompt(text)`:
   - Idle: `turn/start {"threadId":T,"input":[{"type":"text","text":…}],"summary":"auto"}`.
   - Active: `turn/steer {"threadId":T,"expectedTurnId":turnId,"input":[…]}`. On rejection, queue it until `turn/completed` (Z crates/harness/src/codex/mod.rs:1493-1566).
3. `Interrupt()`: `turn/interrupt {"threadId":T,"turnId":turnId}` instead of `Esc`.
4. Stream `item/reasoning/summaryTextDelta` as `thinking`.
5. Map `thread/tokenUsage/updated` to a context gauge.
6. GUI-only codex with no terminal: `thread/start {cwd,…}` on the same daemon; `codex resume <id>` in a terminal attaches later (to verify).
7. Omit `approvalPolicy` / `sandboxPolicy` on `turn/start`, so the thread keeps the TUI's settings. Zeron sends them every turn only because it forces yolo (assumption; verify it inherits).
8. "Always allow" for codex: reply `{"decision":"acceptForSession"}`. Valid decisions are `accept|acceptForSession|decline|cancel` (Z docs/research/harness.md:52); `acceptForSession` is present in the 0.159.0 schema (CLI codex 0.159.0). Pocket sends only `accept`/`decline` today (P packages/pocketd/internal/codex/session.go:272-279).

**Claude: separate headless agent, handoff at Idle.**

A stream-json process can't attach to a running TUI. The GUI-mode agent is a pocketd child with pipes, not a PTY. Minimal path:

1. Spawn in `cwd`, with env `USER` set (P docs/spike-remote-sessions.md:102) and `CLAUDECODE` removed (P docs/designs/2026-09-28-agent-sessions.md:73):

   ```
   claude --print --input-format stream-json --output-format stream-json --verbose --include-partial-messages --replay-user-messages --thinking-display summarized --permission-prompt-tool stdio --permission-mode default [--resume=<session_id>] [--model <id>] [--effort <level>]
   ```

2. `Prompt`: write the user line; while Working, write the steer line with `priority` `now`/`next` by open tools. `Interrupt`: write the `interrupt` control_request, then escalate to SIGTERM / SIGKILL. `Close`: close stdin.
3. Status straight from frames:
   - Working when a line is written.
   - Needs you on `can_use_tool`.
   - Done on `result`: `subtype == "success"` → ok, else failed.
   - Idle after an interrupt.

   No hooks needed.
4. Timeline:
   - `stream_event` deltas → `agent.stream`.
   - `assistant` / `user` frames carry the same `message.content` blocks Pocket's transcript mapper already parses (P packages/pocketd/internal/claude/transcript.go:57-111; Z crates/harness/src/claude/wire.rs:82-103). `claude.Map` is likely reusable; verify it on real frames.
   - `system/init.session_id` → `providerSessionId`.
5. Approvals: `can_use_tool {tool_name,input}` → broker → `{"behavior":"allow","updatedInput":input}` or `{"behavior":"deny","message":…}`.
   - `AskUserQuestion` → question card → `updatedInput.answers{…}` (Z crates/harness/src/claude/mod.rs:1013-1082).
   - "Always allow" needs `permission_suggestions` from the request (Z docs/research/harness.md:30). The reply field for persisting it is not exercised by Zeron (verify). Pocket's hook path already echoes the suggestions back as updates (P packages/pocketd/internal/daemon/permission.go:29-37).
6. Handoff:
   - GUI → terminal: at Idle, close stdin, then run `claude --resume <session_id>` in a new Pocket terminal in the same cwd.
   - Terminal → GUI: after the TUI exits, spawn with `--resume=<id>`.
   - Never run both on one session at once.

**Domain impact:**
- CONTEXT.md defines Agent as "A `claude` or `codex` process running in a terminal" (P CONTEXT.md:27-28).
- `AgentSummary.terminalId` is "always set" (P docs/designs/2026-09-28-agent-sessions.md:138).
- A headless claude agent breaks both. It needs a term (for example "chat agent") and an optional `terminalId`. That is a protocol bump.

**Suggested order:**
1. Codex composer on the shared thread: no new process, biggest win.
2. Desktop transcript view reading the existing `agent.timeline` / `agent.stream`.
3. Headless claude driver + approvals.
4. Steer/queue, context ring, model picker.

## Ideas to clone into Pocket

| ID | Idea | User value | Evidence | Pocket mapping | Effort | Prerequisites |
|---|---|---|---|---|---|---|
| 03-1 | Codex GUI composer on the daemon thread (`turn/start` / `turn/steer` / `turn/interrupt`) | Type to codex from desktop/phone without keystroke injection; the TUI shows the same turn live | P docs/spike-remote-sessions.md:75; Z crates/harness/src/codex/mod.rs:1131-1155, 1493-1590 | `pocketd/internal/codex` session.go + new driver, adapt | M | Track `turnId` from `turn/started`; verify policy inheritance |
| 03-2 | Headless claude driver (stream-json, no PTY) | Chat-first claude agent with live deltas, structured tools, deterministic Done | Z crates/harness/src/claude/mod.rs:167-243, 730-959; Z crates/harness/src/claude/wire.rs:210-230 | new `pocketd/internal/claude/stream.go` implementing `agent.Driver`; reuse `claude.Map` | L | Protocol: optional `terminalId`; CONTEXT.md term |
| 03-3 | Route `can_use_tool` and AskUserQuestion to the broker (Needs you), not auto-allow | Keeps Pocket's approval promise in GUI mode | Z crates/harness/src/claude/mod.rs:967-1082; P packages/pocketd/internal/daemon/permission.go:17-37 | `pocketd/internal/broker` + new claude driver, adapt | M | 03-2 |
| 03-4 | Desktop transcript + composer view | GUI mode on the Mac, not only the phone | Z docs/screenshot.png; P packages/app/src/components/TimelineView.tsx `packages/desktop` new view over the timelines already held in `P packages/desktop/crates/agents/src/agents.rs:118`, new | L | 03-1 or 03-2 |
| 03-5 | Steer vs queue semantics (Steer / Send next / Send now) | Add guidance mid-turn without killing work | Z docs/reference/message-queue.md:26-36; Z crates/ui/src/queue.rs:91-120; Z crates/harness/src/claude/wire.rs:219-230 | `pocketd/internal/agent` queue + drivers, port | M | 03-1/03-2 |
| 03-6 | Context ring under the composer (75% amber / 90% red) | Know when to compact before it's too late | Z docs/context-usage.md:3-18; Z crates/harness/src/codex/normalize.rs:140-160 desktop already shows context-left from the last `usage` item with a hardcoded 200 000 window (P packages/desktop/crates/agents/src/agents.rs:57, 158-160; P packages/desktop/crates/pocket/src/view.rs:431); add real `window` from drivers + ring UI, adapt | S | 03-1/03-2 |
| 03-7 | Model/effort picker from live discovery (claude `initialize`, codex `model/list`) | Pick model and effort per chat without typing `/model` | Z crates/harness/src/claude/mod.rs:267-342; Z crates/harness/src/codex/mod.rs:240-331 | pocketd discovery op + desktop/phone chip, new | M | 03-2 for claude |
| 03-8 | Slash-command and skill completion from agent catalogs | Discoverable commands in the GUI composer | Z docs/harness-skill-completion.md:11-79; Z crates/harness/src/codex/mod.rs:176-234 | pocketd `commands` op + composer popover, new | M | 03-7 probe |
| 03-9 | Mode handoff: "Open in terminal" / "Open as chat" | Move one conversation between TUI and GUI | Z docs/research/harness.md:35; P docs/spike-remote-sessions.md:105 | `pocketd/internal/daemon` + desktop actions, new | M | 03-2; exclusivity lock per claude session |
| 03-10 | Streamed thinking (`--thinking-display summarized`, codex `summary:"auto"`) | Working never looks frozen | Z crates/harness/src/claude/mod.rs:180-183; Z crates/harness/src/codex/mod.rs:1140-1144 | drivers, port | S | 03-1/03-2 |
| 03-11 | Child hygiene: process-group SIGTERM→SIGKILL, stderr tail on crash → failed Done | Clear failure copy instead of silent vanish | Z crates/harness/src/lib.rs:239-434 | `pocketd/internal/proc` + claude driver, port | S | 03-2 |
| 03-12 | CLI version gate + capability probe | Fail loudly on unsupported CLI versions | Z crates/harness/src/pi/mod.rs:122-128; Z docs/research/harness.md:37-38 | pocketd startup check, new | S | none |
| 03-13 | Image attachments (claude base64 blocks ≤ 5 MiB) | Paste screenshots from phone/desktop | Z crates/harness/src/claude/mod.rs:619-690 | claude driver + upload op, port | M | 03-2 |
| 03-14 | Subagent folding by `parent_tool_use_id` | Nested agent work doesn't flood the transcript | Z crates/harness/src/claude/normalize.rs; Z docs/research/codex-subagents.md:5-17 | `pocketd/internal/timeline`, adapt | M | 03-2 |
| 03-15 | Tool-group summary row ("Ran 4 commands · read 1 file · called 1 tool", failures red) | Scannable transcript on a phone | Z docs/screenshot.png; Z docs/screenshots/mobile-tool-disclosure/tool-group-expanded.png | `packages/app` TimelineView + desktop view, adapt | S | none |
| 03-16 | Pocket MCP server so agents can drive other sessions | Agent orchestration | Z docs/mcp.md | new pocketd `mcp` command, new | L | 03-1/03-2 |

## UI/UX spec to copy

**Composer** (desktop, Z docs/screenshot.png):
- Rounded pill with the placeholder "Do anything…".
- Right side: provider icon + model + effort chip ("Fable 5 High"), paperclip, circular ↑ send button.
- Beneath it: checkout chip ("Local checkout") on the left, branch chip ("main ⌄") on the right.
- iOS placeholder: "Message" (Z docs/screenshots/mobile-tool-disclosure/tool-group-expanded.png).

**Transcript** (Z docs/screenshot.png; Z docs/screenshots/mobile-tool-disclosure/tool-group-expanded.png):
- User messages are right-aligned bubbles with image thumbnails above.
- Assistant text is plain full-width markdown.
- "Scroll to bottom" pill above the composer.
- A left-edge tick rail, probably one tick per user turn (unverified: inferred from the screenshot only).

**Tool group:**
- Collapsed: one muted row "Ran N commands · read N files · called N tool".
- Expanded: per tool, an icon + verb (Search / Run / Edit) + monospace argument, joined by a vertical line.
- Failure: verb and "Failed" in red, "· 1 failed" appended to the summary; the header stays muted.

**Context ring** (Z docs/context-usage.md:3-8; Z docs/screenshots/context-usage/near-capacity-light.png):
- Small ring + "92%" under the composer's right edge.
- Hover card: "Context window / 184000 / 200000 tokens / 16000 tokens remaining".
- Amber at ≥ 75%, red at ≥ 90%, "–" when unknown.

**Queue rows** (Z crates/ui/src/queue.rs:98-104, 1040-1064; Z docs/reference/message-queue.md:59-61):
- One trailing action per row: Steer / Send next / Send now, with the tooltips in §7.
- Compact mode shows ⌘↵ (macOS) or ⌃↵.
- Desktop auto-steers; iOS holds until the turn ends.

**Question panel** (Z apps/ios/Zeron/Composer/QuestionPanel.swift:1-5, 34, 144; Z crates/ui/src/composer.rs:8803):
- Replaces the composer while the agent asks.
- One card per question with large option rows and "Other…" free text.
- Single-select auto-advances after 220 ms. Buttons: Next, then Submit on the last page.
- In Pocket, the same slot fits permission cards: Allow / Deny / Always allow.

## Open questions / risks

- **`--permission-prompt-tool stdio` is undocumented.** Zeron validated it on 2.1.228 (Z crates/harness/src/claude/mod.rs:8-15). 2.1.285 still parses it, and its new `--permission-prompts host|none` references "the SDK host or --permission-prompt-tool" (CLI claude 2.1.285). Pin a minimum version, and probe with `initialize` at startup.
- **`--permission-mode default` is no longer a listed choice in 2.1.285** (`manual` appears). It is still accepted today. Decide which name Pocket sends, and whether `manual` equals the old `default`.
- **Hook overlap.** Pocket injects its hook plugin into every terminal env (P docs/designs/2026-09-28-agent-sessions.md:70-73). Unverified whether `PermissionRequest` hooks also fire for a `--print` child using the stdio prompt tool. If they do, the user is asked twice. Spawn headless claude without `CLAUDE_CODE_PLUGIN_DIRS`, or ignore its hooks.
- **Codex approvals with several clients.** Pocket already dismisses on `serverRequest/resolved` (P packages/pocketd/internal/codex/session.go:166-167). Confirm that a GUI-originated turn raises approvals to all subscribed clients, including the TUI.
- **Originator.** Pocket sends `clientInfo.name:"codex_app_server_daemon"` so it never stamps itself as a thread's originator (P docs/designs/2026-09-28-agent-sessions.md:100). Unverified whether `turn/start` from that name has side effects: history attribution, rate limits, a rejected turn.
- **Version drift.** The codex app-server is `[experimental]` (CLI codex 0.159.0). Zeron validated 0.153.4, Pocket's design targets ≥ 0.157 / tag `rust-v0.158.0`, and 0.159.0 is installed. Generate the schema per version (`codex app-server generate-json-schema --out`) in CI; Zeron recommends this too (Z docs/research/harness.md:54).
- **Claude exclusivity.** Nothing in Zeron guards against a TUI `claude --resume <id>` and a headless process on the same session at once. Pocket needs a per-session lock and a disabled "Open in terminal" while Working.
- **What headless claude loses.** Interactive-only TUI features such as dialogs and slash commands missing from `initialize.commands`. Which user commands matter is unmeasured.
- **Codex images.** Zeron sends only path text (Z crates/harness/src/codex/mod.rs, attachments). The 0.159.0 schema does define a `localImage` input item (`codex app-server generate-json-schema`, ClientRequest.json; CLI codex 0.159.0), but it is untested here. Verify before promising image paste for codex.
- **Don't copy Zeron's yolo defaults.** Codex `danger-full-access` + `never`, and claude `bypassPermissions` when auto-approve is on (Z crates/harness/src/codex/mod.rs:696-707; Z crates/harness/src/claude/mod.rs:209-213). Pocket's Needs you depends on real prompts.
- **Account/plan terms.** Terms for driving `claude --print` from a third-party GUI on a subscription were not researched here. Confirm before shipping.
- **Vocabulary and wire.** "Agent = process in a terminal" (P CONTEXT.md:27-28) and an always-set `terminalId` (P docs/designs/2026-09-28-agent-sessions.md:138) need a decision before 03-2.

## Verification

Date: 2026-09-30. Claims checked: 22. Corrected: 7.

Confirmed against source: Claude spawn flags and `--permission-mode default` / bypass branch (Z claude/mod.rs:167-243); steer `now`/`next` (Z claude/wire.rs:219-230); `HELD_DONE_SETTLE` 5 s; interrupt grace 2 s / 3 s; auto-allow of `can_use_tool`; codex `experimentalApi`, `summary:"auto"`, `approvalPolicy "never"` + danger-full-access, `turn/steer{expectedTurnId}`; session idle 30 min, subagent ×8, 15 s heartbeat, quiesce 120 s / 20 s; context ring 75% / 90%; queue tooltips; question panel 220 ms; image cap 5 MiB; MCP port 27654; CLI 2.1.285 permission-mode choices and `default` still accepted; codex app-server `[experimental]`; Pocket `agent.Driver`, daemon-socket client name, 150 ms prompt pause, `Esc` interrupt, `/compact`, 100 ms transcript poll, codex `accept`/`decline` only, spike `turn/start` (line 75), CONTEXT.md Agent term, always-set `terminalId`.

- TL;DR omitted the native OpenCode HTTP/SSE driver (Z crates/harness/src/lib.rs:6-7); added.
- §8 said desktop has no `agent.timeline` use and cited nonexistent `packages/desktop/src`. Desktop subscribes to `agent.timeline` / `agent.stream` (P packages/desktop/crates/agents/src/agents.rs:274-275); corrected.
- 03-6 mapping ignored that desktop already shows context-left with a hardcoded 200 000 window (P agents.rs:57, 158-160; P view.rs:431); remapped as adapt.
- 03-4 mapping now points at the existing desktop timeline store.
- Queue primary action table had wrong precedence: attachments → Send now wins first (Z crates/ui/src/queue.rs:108-120); reordered.
- Codex images risk said no evidence of image input; the 0.159.0 generated schema has a `localImage` input item; reworded, still to verify live.
- `--thinking-display` choices come from the validator, not `--help`; clarified. Tick-rail meaning marked unverified.
