# 14. Pocket today: capabilities, architecture, extension points

Date: 2026-09-30.

Sources:
- Pocket worktree `/Users/mingo/.worktrees/coding-pocket/orchestrator-research` @ `86deb13` ("Desktop polish: top bar layout, motion, flat selection").
- Pocket main checkout `/Users/mingo/Developer/self/coding-pocket` @ `b9d14a1` plus uncommitted work (F12).
- Competitors, for context only: zeronsh/comet @ `ed3b1aae4a5189eef67143db7b8c5c3ee7a933c5`, hardbeat920/monocode @ `cdc1441dc51e3709cd843e5c316608a123f323c6`. This report reads only Pocket code. It cites competitor facts through the earlier reports 04, 05 and 07.
- In-repo docs: `CONTEXT.md`, `docs/adr/*`, `docs/designs/*`, `docs/plans/*`, `docs/wayfinder/*`, `docs/spike-*.md`, `docs/research-*.md`, `docs/orchestrators/research/0*.md`.

Citation legend:
- `P path:L` is a file and line (or range) in the Pocket worktree, path relative to its root.
- `P@main path:L` is the same path in the main checkout's working tree, which is ahead of the worktree (F12).
- `Z` / `M` would be zeron / monocode paths. This report cites none directly; report numbers like "report 05" point to sibling files in `docs/orchestrators/research/`.
- "code≠doc" marks a place where code and docs disagree. The code wins.
- Line counts come from `wc -l` and include tests.

## TL;DR

- Pocket has three parts:
  - `pocketd`, a Go daemon: PTYs, libghostty-vt, agent detection, timeline, WebSocket v3.
  - A GPUI desktop on gpui-kit 0.6.6, light theme only.
  - An Expo phone app, dark theme.
- It is terminal-first. An "agent" is a `claude` or `codex` process that pocketd spots in the foreground of a Pocket PTY (P packages/pocketd/internal/daemon/detect.go:11-38).
  - The only driver types keystrokes into that PTY (P packages/pocketd/internal/daemon/presence.go:123-155).
- **Desktop has no transcript view and no chat composer.** It still downloads every agent's timeline (up to 500 items each) over WS (P packages/desktop/crates/agents/src/agents.rs:284-291).
  - It uses that data only for the inbox, context %, and "touched" files.
  - Its WS outbox can send only `agent.view`, `agent.seen` and `agent.close` (P packages/desktop/crates/agents/src/agents.rs:207-223).
- **The phone has the chat surface:** timeline, composer and permission sheet.
  - Four buttons are dead: diff, raw terminal, attach, mic (P packages/app/src/screens/ChatScreen.tsx:147-159; P packages/app/src/components/Composer.tsx:28-46).
  - No push notifications.
  - Pairing is manual host + token over plain `ws://`, meant for Tailscale (P packages/app/src/session.tsx:76; P packages/pocketd/cmd/pocketd/serve.go:46-59).
- Protocol v3 has 10 client and 9 server message types.
  - No `agent.create`; pocketd rejects it (P packages/pocketd/internal/proto/golden_test.go:102).
  - Timelines are in memory only.
  - Paging is forward-only (`sinceSeq`), so only the newest ≤500 items can be fetched (P packages/pocketd/internal/timeline/timeline.go:155-167).
- **The status pipeline is Pocket's strongest part**, and it is tested:
  - A 250 ms foreground poll.
  - Claude plugin hooks and a Codex app-server watcher.
  - The states Needs you > Done/Failed > Working > Idle, plus a shared Seen flag.
  - macOS system notifications with click-to-focus (P packages/desktop/crates/pocket/src/main.rs:314-328, :1103-1106).
- **The one seam for a GUI-driven agent mode** is `agent.Driver {Prompt, Interrupt, Compact, Close}` (P packages/pocketd/internal/agent/agent.go:16-21).
  - Codex already streams deltas into the timeline (P packages/pocketd/internal/codex/session.go:157-159).
  - Nothing calls `turn/start` yet.
- **Tech debt that blocks parallel cloning:**
  - The `Desktop` god-struct has 77 fields (84 on main) (P packages/desktop/crates/pocket/src/main.rs:99-180).
  - The desktop polls `list` every 1 s and git every 2 s (P packages/desktop/crates/pocket/src/main.rs:1091-1100).
  - Layout is not persisted.
  - The phone has two token sets.
  - No CI, no app bundle, no pocketd launch agent.
- **The main checkout is ahead of this worktree.**
  - `b9d14a1` rewrites the Changes panel: `changes.rs`, 664 lines covering staged/unstaged, commit/push/amend, discard, and an AI commit message.
  - Uncommitted work vendors Material Icon Theme file icons.
  - Plans touching Changes, explorer, palette, `theme` or `ui` must branch from main (F12).
- Report 04 agrees (its line 271): desktop notifications exist (P packages/desktop/crates/pocket/src/main.rs:314-328); sound does not.

## Findings

### F1. Vocabulary and decisions

- Terms (P CONTEXT.md:7-60):
  - Terminal: a Pocket PTY.
  - Project: a repo.
  - Worktree.
  - Login shell.
  - Session: one agent from launch to exit.
  - Agent.
  - Attached: pocketd can see its status.
  - Conversation: provider session id.
  - Status: Needs you / Working / Done / Idle, plus Seen.
- ADR 0001: desktop terminals run the user-record login shell. Agents run as `<shell> -l -c '<agent…>; exec <shell> -l'` (P docs/adr/0001-desktop-terminals-run-login-shell.md:1-3; P packages/desktop/crates/daemon/src/daemon.rs:65-128).
- ADR 0002: sessions are agents; terminals belong to worktrees (P docs/adr/0002-sessions-are-agents-terminals-belong-to-worktrees.md:1-3). A card is an agent, and a workspace is keyed by worktree (P packages/desktop/crates/pocket/src/status.rs:63-81; P packages/desktop/crates/workspace/src/workspace.rs:1-13).
- Design doc `2026-09-28-agent-sessions` (P docs/designs/2026-09-28-agent-sessions.md):
  - Presence poll, status table, Claude hooks and Codex watcher.
  - Seen via `agent.view` / `agent.seen`.
  - Rendering: system notifications (:155-167).
  - Out of scope: phone push, dock badge, rollout tailer, OSC 133.
- Wayfinder map `agent-sessions` is closed, with tickets 01-09: herdr study, claude/codex launch injection, PTY foreground, card model, status set, detection, protocol, rendering (P docs/wayfinder/agent-sessions/map.md). Out of scope there: terminals outside Pocket, and CLIs other than claude/codex.

### F2. Plans: implemented or not (checked against code)

| Plan | Status | Evidence |
|---|---|---|
| `2026-09-25-pocketd-remote-sessions` (PR1-8) | Implemented, except the per-tab bottom composer of Task 8.2 (code≠doc) | pocketd PTY/timeline/hub/broker/ws exist (P packages/pocketd/internal/*). The TS daemon is gone (`packages/` = app, desktop, pocketd, protocol). The ops `prompt` is used only by diff comments (P packages/desktop/crates/pocket/src/main.rs:904-911). No composer under terminal tabs (P packages/desktop/crates/pocket/src/view.rs:783-790). Plan text: P docs/plans/2026-09-25-pocketd-remote-sessions.md:8298-8306 |
| `2026-09-28-agent-sessions` (PR1-8) | Implemented | proc poller, hooks, codex watcher (P packages/pocketd/internal/daemon/{watch,plugin,codex}.go); v3 (P packages/pocketd/internal/proto/proto.go:10-15); notifications (P packages/desktop/crates/pocket/src/main.rs:314-328); inbox (P packages/desktop/crates/pocket/src/inbox.rs:29-45) |
| `2026-09-28-zed-explore-diff` (PR1-5) | Implemented | syntax + word diff (P packages/desktop/crates/pocket/src/diff.rs:21-66); read-only `Editor` (P packages/desktop/crates/pocket/src/explore.rs:10, :286-391); md preview (P packages/desktop/crates/pocket/src/explore.rs:141-152); image/large-file limits (P packages/desktop/crates/pocket/src/explore.rs:21-57); in-process diff with folds (P packages/desktop/crates/git/src/git.rs:198-245; P packages/desktop/crates/pocket/src/diff.rs:172, :407-415) |
| `2026-09-28-mermaid-preview` (PR1) | Implemented | `MarkdownPlugin` + merman + resvg, 64 MB budget (P packages/desktop/crates/pocket/src/mermaid.rs:12-16, :193-255) |
| `2026-09-28-monocode-markdown-style` (PR1) | Implemented. Research §11.1 describes the pre-plan state (code≠doc, expected) | `markdown_style` with heading sizes 22/18/16/14, bordered code card r10 pt36, table styles, `code_actions` language + copy (P packages/desktop/crates/pocket/src/explore.rs:132-161); `Theme.muted = WINDOW` (P packages/desktop/crates/theme/src/theme.rs:171-172) |
| `2026-09-29-sessions-are-agents` (PR1-4) | Implemented | `Registry.Close/Forget` (P packages/pocketd/internal/agent/agent.go:86-100); `Workspace::sync` (P packages/desktop/crates/workspace/src/workspace.rs:20-30); card = agent (P packages/desktop/crates/pocket/src/status.rs:63-81) |
| `2026-09-30-projects-sidebar` (PR1-4) | Implemented | Store order/collapse/move (P packages/desktop/crates/store/src/store.rs:44-70); `git::worktrees`/`remove_worktree` (P packages/desktop/crates/git/src/git.rs:141, :172); ⋯ menus + Confirm + drag (P packages/desktop/crates/pocket/src/view.rs:281-425; P packages/desktop/crates/pocket/src/overlay.rs:268-307); `setup_op` (P packages/desktop/crates/daemon/src/daemon.rs:103-121) |

### F3. Architecture map

```
 phone (Expo 57 / RN 0.86.3)          desktop (gpui-kit 0.6.6, bin pocket-desktop)
  App.tsx state switch                  Desktop entity (main.rs:99) under Root
  client.ts  ws://host:4517 ──┐          ├─ daemon crate ── unix sock, JSON lines ──┐
                              │          └─ agents crate ─ ws://127.0.0.1:4517 ─┐   │
                              ▼                                                 ▼   ▼
 pocketd serve (Go 1.27.1) ── wsserver (v3, token) ── hub (fan-out) ◄── agent.Registry
                           └─ ops server (~/.coding-pocket/pocketd.sock, 0600)
                                 ├─ terminal.Manager: creack/pty + libghostty-vt (cgo) snapshot
                                 ├─ daemon.Watch: 250 ms TIOCGPGRP → kern.proc → argv/env
                                 ├─ Claude: plugin hooks → `pocketd hook` → ops `hook`
                                 ├─ Codex: app-server JSON-RPC watcher (codex_app_server_daemon)
                                 ├─ timeline (in-memory, per agent) ← claude JSONL tail / codex items
                                 └─ broker (permissions, 10 min timeout)
```

- Sizes:
  - pocketd internal: daemon 2113, codex 1049, timeline 668, agent 639, wsserver 539, proto 519, terminal 475, ops 387, broker 322, claude 302, proc 233, vt 148, config 101, hub 90, envdir 47.
  - Desktop `pocket/src`: view 1181, main 1117, forms 1007, diff 777, explore 497, overlay 332, mermaid 328, inbox 313, status 228, capture 145, sessions 139, termview 89, syntax 84.
  - Desktop other crates: ui 829, git 533, agents 375, daemon 294, storybook 251, theme 195, workspace 158, keys 129, term 127, store 114.
  - Phone `src`: 2117.
- The desktop uses **both** transports:
  - Ops socket for terminals: spawn, attach, input, resize, prompt, close.
  - The phone WS for agents, timelines and permissions (P packages/desktop/crates/pocket/src/main.rs:1032-1041; P packages/desktop/crates/agents/src/agents.rs:242-250).
  - It exits when pocketd is unreachable (P packages/desktop/crates/pocket/src/main.rs:1035-1038).

### F4. pocketd

**CLI.**
- `pocketd serve | run <cmd> [args...] | attach <id> | hook` (P packages/pocketd/cmd/pocketd/main.go:10).
- `serve`:
  - Builds a Daemon with Terminals, Agents, Broker, Home, Exe and Sock, then writes the Claude plugin.
  - Listens on `:4517` on all interfaces.
  - Prints `phone: ws://<tailscale ip -4>:4517` and the token (P packages/pocketd/cmd/pocketd/serve.go:23-69).
- `run`: raw-mode local attach (P packages/pocketd/cmd/pocketd/run.go:14-87).
- `hook`: the Claude hook shim. It reads stdin and `POCKETD_PTY`, finds the nearest claude ancestor pid, and sends the ops `hook` (P packages/pocketd/cmd/pocketd/hook.go:14-65).

**Config and pairing.**
- Home is `$POCKET_HOME` or `~/.coding-pocket`.
- `config.json {token, port}` is written 0600. The first run creates a 24-byte base64url token and port 4517 (P packages/pocketd/internal/config/config.go:52-61).
- Pairing: the user types host and token into the phone (P packages/app/src/screens/ConnectScreen.tsx:36-46). No QR, no TLS, no relay.
- iOS allows arbitrary loads for this (P packages/app/ios/CodingPocket/Info.plist:41-43).

**Ops socket** (JSON lines, the desktop and CLI):
- `list`, `spawn`, `hook`, `attach` (snapshot, then events), `input`, `prompt`, `resize`, `screen`, `close` (P packages/pocketd/internal/ops/ops.go:117-163). Socket mode 0600 (P packages/pocketd/internal/ops/ops.go:76).
- Terminals:
  - Default size 80x24; 32 KB pump.
  - `Prompt` writes the text, sleeps 150 ms, then sends `\r` (P packages/pocketd/internal/terminal/terminal.go:276-287).
  - The last client to resize wins (P docs/spike-remote-sessions.md:108).

**WS server** (P packages/pocketd/internal/wsserver/wsserver.go):
- Hello within 10 s; ping every 20 s with a 10 s timeout; 1 MB read limit; origin check off (`InsecureSkipVerify`) (:22-57).
- The token and version must match. On success the server sends `hello.ok`, then `agent.list`, then the open permissions (:122-144).
- Dispatch is at :154-209.
- Hub fan-out buffers 1024 per subscriber and drops slow ones (P packages/pocketd/internal/hub/hub.go:37-52).

**Protocol v3 messages** (P packages/pocketd/internal/proto/messages.go:62-82, :98-165; P packages/protocol/src/messages.ts:4-65):

| Direction | Types |
|---|---|
| client → server | `hello`, `agent.list`, `agent.prompt`, `agent.interrupt`, `agent.compact`, `agent.close`, `agent.view`, `agent.seen`, `agent.timeline {sinceSeq?, limit 1-500}`, `permission.resolve {decision, option?, message?}` |
| server → client | `hello.ok`, `agent.list`, `agent.update`, `agent.stream {epoch, item}`, `agent.timeline {epoch, items, hasOlder, maxSeq}`, `permission.request`, `permission.resolved`, `ack`, `error` |

- `AgentSummary` fields (P packages/pocketd/internal/proto/proto.go:168-184):
  - Identity: id, terminalId, title, cwd, provider, model.
  - State: status, failed, attached, compacting.
  - Paging: epoch, maxSeq.
  - Also providerSessionId, createdAt, updatedAt.
  - Missing: branch, worktree, token usage, context window, cost.
- Item kinds: user, assistant, thinking, tool, tasks, plan, compact, result. ToolDetail kinds: shell/read/edit/write/search/task/other. Tool output is clamped to 64 KB; diff previews are 24 lines × 160 chars (P packages/pocketd/internal/proto/proto.go:10-15, :123-166).
- `PermissionRequest` has `options[]` (always/auto and suggestions) and `feedback` (P packages/pocketd/internal/proto/proto.go:186-200; P packages/pocketd/internal/daemon/permission.go).
- Contract: Go golden fixtures are rewritten with `go test ./internal/proto -update` (P packages/pocketd/internal/proto/golden_test.go:12). The TS package decodes them (`packages/protocol/test/golden.test.mjs`).

**Agent detection and status.**
- Poll every 250 ms (P packages/pocketd/internal/daemon/watch.go:13). Chain: `TIOCGPGRP` → `kern.proc.pgrp` members → argv/env via `kern.procargs2` (P packages/pocketd/internal/proc/proc_darwin.go).
- The provider comes from argv[0] basename `claude` / `codex`, skipping non-agent subcommands (P packages/pocketd/internal/daemon/detect.go:11-38). No other CLIs.
- Claude:
  - A generated plugin is loaded via `CLAUDE_CODE_PLUGIN_DIRS`, with `POCKETD_PTY` / `POCKETD_SOCK` in the env (P packages/pocketd/internal/daemon/plugin.go:10-22, :65-82).
  - Hooks: SessionStart, UserPromptSubmit, PreToolUse (`AskUserQuestion|ExitPlanMode`), PermissionRequest, Notification, PostToolUse(+Failure), PermissionDenied, Stop, StopFailure, PreCompact.
  - Only PermissionRequest gets a reply (P packages/pocketd/internal/daemon/daemon.go:90-120).
  - An agent with no SessionStart within 5 s is "not attached" (P packages/pocketd/internal/daemon/presence.go:55). Claude skips hooks in untrusted folders (P packages/desktop/crates/pocket/src/status.rs:106).
  - Esc or Ctrl+C clears the turn (P packages/pocketd/internal/daemon/claude.go:30-43).
- Codex:
  - pocketd joins the app-server as a second client (`codex_app_server_daemon`).
  - The thread binds to the codex that got Enter within 3 s (P packages/pocketd/internal/daemon/codex.go:16).
  - Deltas stream in; `serverRequest/resolved` dismisses asks (P packages/pocketd/internal/codex/session.go:150-168).
- Status machine: Working / NeedsYou / TurnEnded(failed) / Clear / Compacting (P packages/pocketd/internal/agent/agent.go:207-261).
  - Title = first user line, 80 chars (:305-332).
  - A conversation switch clears the timeline and title (:286-294).
- Permissions:
  - The broker times out after 10 min (P packages/pocketd/internal/broker/broker.go:16).
  - A deny sends "Denied from phone" plus an interrupt.
  - Feedback is typed as the next prompt after the turn, within 30 s (P packages/pocketd/internal/daemon/daemon.go:123-171).
- Driver: `termDriver` is the only implementation (P packages/pocketd/internal/daemon/presence.go:113-155):
  - Prompt → PTY typing.
  - Interrupt → `0x1b`.
  - Compact → types `/compact`.
  - Close → SIGTERM.
  - Every call fails with "Agent is no longer in the foreground" when the agent is not foreground (:119).

**Timeline** (P packages/pocketd/internal/timeline/timeline.go):
- In memory, never persisted, no item cap.
- `Apply` merges text. TodoWrite → tasks; ExitPlanMode → plan (:82-153).
- `Page(sinceSeq, limit)` returns the newest `limit` items after `sinceSeq` (:155-167). There is no `beforeSeq`, so `hasOlder` cannot be acted on.
- Claude events come from the JSONL tail, in whole blocks (no token streaming). Codex streams `item/agentMessage/delta` (P docs/spike-remote-sessions.md:42-50).

### F5. packages/protocol

- `PROTOCOL_VERSION = 3` (P packages/protocol/src/constants.ts).
- effect `Schema` unions for ClientMessage / ServerMessage (P packages/protocol/src/messages.ts:4-65).
- Timeline types: AgentStatus, ToolDetail, ToolCall, TaskItem, TurnUsage, TimelineBody, AgentSummary, PermissionRequest (P packages/protocol/src/timeline.ts:3-113).
- Test: `tsc -b && node --test test/` (P packages/protocol/package.json).
- Only the phone consumes it. The desktop hand-decodes a subset in Rust with `serde(default)` (P packages/desktop/crates/agents/src/agents.rs:12-104).
  - `Item` keeps id/seq/ts/kind/text/error/durationMs/usage/call.detail.
  - It drops tool status, output, diff, tasks, plan and output tokens.

### F6. Desktop crates

| Crate | Responsibility | Key items |
|---|---|---|
| `pocket` (bin `pocket-desktop`) | The app: one `Desktop` entity renders everything | `Desktop` 77 fields (P packages/desktop/crates/pocket/src/main.rs:99-180); actions (:32); enums Screen/Side/Layout/Column/Overlay (:34-81); `main()` (:1032-1117) |
| `agents` | WS v3 client on 127.0.0.1; `Agents {list, timelines, pending}` store | `connect` thread, 2 s reconnect, 100 ms read timeout (P packages/desktop/crates/agents/src/agents.rs:226-293); `Outbox` view/seen/close (:203-223); `context_left` with hard-coded 200k window (:56-57, :158-161); `last_edit` (:163-176); `model_label` (:183-195) |
| `daemon` | Ops-socket client; login-shell spawn ops | `Daemon::connect/send/input` (P packages/desktop/crates/daemon/src/daemon.rs:130-160); `login_shell`, `terminal_env`, `shell_op`, `agent_op`, `setup_op` (:65-128) |
| `git` | Shells out to `git`; in-process diff via imara-diff | `read` (P packages/desktop/crates/git/src/git.rs:59); `worktrees` (:141); `add/remove_worktree` (:166-176); `clone` (:177); `set_staged` (:182); `diff_texts` with 3 context lines and folds (:198-245); `words` (:267) |
| `keys` | Keystroke → PTY bytes | `CONTEXT="Terminal"`; tab/shift-tab bound to `NoAction` so gpui-kit Root focus cycling doesn't eat them (P packages/desktop/crates/keys/src/keys.rs:3-10) |
| `store` | `desktop.json` beside the socket | `Store {projects, repos: RepoConfig{name,color,base,worktrees,setup,copy}, collapsed}` (P packages/desktop/crates/store/src/store.rs:8-28) |
| `term` | C shim over static `libghostty-vt.a` | `Term::new/write/resize/app_cursor/frame` (P packages/desktop/crates/term/src/term.rs:41-72); build copies only the `.a` so the linker can't pick the dylib (P packages/desktop/crates/term/build.rs:1-15) |
| `theme` | Tokens, fonts, icons, gpui-kit theme hookup | Tokens (P packages/desktop/crates/theme/src/theme.rs:6-62); `highlight_theme` (:75-93); `PALETTE` (:96); 40 embedded icons (:143-147); `init` (:163-178) |
| `ui` | Stateless element builders (~64 fns) | `button`/`Variant`, `status`/`State`, `session_row`, `palette_row`, `modal`, `dropdown`, `segmented`, `tree_row`, `change_row`, `menu_row`, `trigger_field`… (P packages/desktop/crates/ui/src/ui.rs:5-829) |
| `workspace` | Per-worktree tabs | `Tab::Term(Vec<Vec<id>>) \| Tab::Changes`; `sync`, `add_tab`, `split(down)`, `open_changes`, `close_tab` (P packages/desktop/crates/workspace/src/workspace.rs:1-88) |
| `storybook` | Visual check of `ui` | Tabs Sessions/Explore/Changes, Unified/Split (P packages/desktop/crates/storybook/src/main.rs:6-60); `cargo run -p storybook` |

`pocket/src` modules:

| Module | Builds | Cites |
|---|---|---|
| `view.rs` | Sidebar (`aside`), 56 px nav rail, sessions column, page bar, term tabs + tab menu (New shell ⌘T / Claude / Codex with model hint), panes + pane header, Render root | `resizable` 5 px handle, Projects 200-420, Sessions 280-600, widths not saved (P packages/desktop/crates/pocket/src/view.rs:156-190); `aside` (:193-262); `nav` (:459-561); `column_view` (:581-634); `session_list` (:636-664); `main_view` (:680-691); `session_page` (:747-792); `term_tabs` (:822-919); `tab_menu_view` (:927-980); `panes`/`pane` (:982-1085); `render` (:1088-1138) |
| `overlay.rs` | ⌘K palette, More menu, Confirm sheet, overlay animation | `palette_sections`: Sessions/Files/Actions, ≤5 each, lowercase substring, Tab toggles all-projects (P packages/desktop/crates/pocket/src/overlay.rs:79-134); palette 660 px, r22, top 120 (:173-236); More: New terminal tab / Close session / Open in editor / Reveal in Finder (:237-266); `overlay_view` (:309-332) |
| `forms.rs` | New session (prompt, agent picker, permission mode, worktree name, base branch, copy env, setup) and Add/Edit repo | `Perm` Ask/AutoEdit/Plan → `--permission-mode acceptEdits\|plan`, `--full-auto`, `-s read-only` (P packages/desktop/crates/pocket/src/forms.rs:13-17, :402-415); worktree names = prompt slug or adjective-noun (:71-117); `start_session` (:402-460); `repo_view` (:822-965) |
| `inbox.rs` | Inbox of Needs you → Failed → Done with a live terminal pane | `notes` (P packages/desktop/crates/pocket/src/inbox.rs:29-45); detail with "Answer in the terminal · J K · ⌘↵ open session" (:206-258) |
| `status.rs` | Card model and alert diffing | `Status` order (P packages/desktop/crates/pocket/src/status.rs:4-42); `card` (:63-81); `roll_up` (:93-98); not-attached `banner` (:100-110); `alerts` (:119-124) |
| `diff.rs` | Changes diff (own `list` rows), unified/split, syntax + word tint, folds, line pick → comment composer → target session | `list(self.diff_list…)` (P packages/desktop/crates/pocket/src/diff.rs:379-395); composer (:501-550); `target_picker` (:606-683); comment → ops `prompt` `"{path} {lines}: {text}"` (P packages/desktop/crates/pocket/src/main.rs:904-911) |
| `explore.rs` | File tree, Go to file (⌘P), read-only Editor, markdown TextView + Mermaid, image preview | Limits: 512 KB text, 20 MB image (P packages/desktop/crates/pocket/src/explore.rs:21-23); `markdown_style` / `code_actions` (:141-161); `file_view` (:286-391) |
| `mermaid.rs` | Mermaid `MarkdownPlugin`, off-thread merman → usvg → raster cache | (P packages/desktop/crates/pocket/src/mermaid.rs:12-16, :119-255) |
| `termview.rs` | Terminal surface: canvas measures cells → `fit`; paints the visible frame as `StyledText` runs | MAIN 13/22, SMALL 12/19 (P packages/desktop/crates/pocket/src/termview.rs:11-12); `screen` (:41-89). No scrollback, selection, links or mouse reporting |
| `sessions.rs` | Terminal list sync + VT feed | (P packages/desktop/crates/pocket/src/sessions.rs:23-70) |
| `capture.rs` | `pocket-desktop --capture <dir> <name>=<step>,…`, 12 steps, needs feature `capture` | (P packages/desktop/crates/pocket/src/capture.rs:10-36, :38-48) |
| `syntax.rs` | tree-sitter per-line spans | (P packages/desktop/crates/pocket/src/syntax.rs:9-52) |

- View composition:
  - `Root(Desktop)` holds the lead, the column, the page, the panel and the overlay (P packages/desktop/crates/pocket/src/view.rs:1088-1138).
  - Lead is `aside` in Layout::Sidebars and the `nav` rail in Compact; Focus has none.
  - The page is `inbox_detail` / `diff_view` / `file_view` / `session_page` / `blank_page` (P packages/desktop/crates/pocket/src/view.rs:680-691).
  - `session_page` is a page bar plus either term panes or `diff_box` (:783-790).
  - Every pane is a function of `&mut Desktop`. There are no child entities besides gpui-kit states.
- gpui-kit 0.6.6 components used:
  - `Root`.
  - `Input`/`InputState`, `Textarea`/`TextareaState`, `EditorState`/`Editor` (read-only), `TextDecoration`, `Escape`.
  - `TextView`/`TextViewState`/`TextViewStyle`, `MarkdownPlugin`.
  - `Clipboard`, `HighlightTheme`, `SyntaxHighlighter`, `Rope`.
  - gpui `list`/`ListState`, `canvas`, `StyledText`, `svg`, `img`, animations.
  - `show_system_notification` (P packages/desktop/crates/pocket/src/main.rs:18-20; P packages/desktop/crates/pocket/src/explore.rs:7-11; P packages/desktop/crates/pocket/src/diff.rs:8; P packages/desktop/crates/pocket/src/mermaid.rs:1).
  - Everything else (buttons, menus, lists, tabs) is Pocket's own `ui` crate.
  - 15 tree-sitter features: bash, css, go, html, java, javascript, kotlin, markdown, python, rust, swift, toml, tsx, typescript, yaml (P packages/desktop/Cargo.toml:25-41).
- Keybindings (P packages/desktop/crates/pocket/src/main.rs:1046-1057):
  - ⌘K palette, ⌘P go to file.
  - ⌘N new session, ⌘J next waiting.
  - ⌘\ toggle rail, ⌘. cycle layout (Sidebars → Compact → Focus).
  - ⌘T new tab, ⌘⇧N new worktree.
  - ⌘, project settings, ⌘↵ open session.
- Window: 1440x900 centered, transparent titlebar, traffic lights at (14,14) (P packages/desktop/crates/pocket/src/main.rs:1059-1068). Bounds are not persisted.
- Reduced motion follows the NSWorkspace setting (P packages/desktop/crates/pocket/src/main.rs:1027-1030).

### F7. Phone app (packages/app)

- Stack: Expo 57, RN 0.86.3, React 19.2.3, Geist fonts, expo-glass-effect / expo-blur, expo-secure-store, react-native-svg (P packages/app/package.json:14-31). Style `dark`; bundle id `dev.mingo.codingpocket` (P packages/app/app.json).
- Navigation: no library. `App.tsx` switches ChatScreen when an agent is open, else AgentsScreen when online, else ConnectScreen. PermissionSheet is global (P packages/app/src/App.tsx:15-32).
- Client:
  - `ws://${host}`, clientId `pocket-app` (P packages/app/src/session.tsx:76).
  - Reconnect backoff 1 s doubling to 30 s (P packages/app/src/client.ts:7-12, :53-54).
- Connect: Host (placeholder `100.77.122.82:4517`) and token, stored in SecureStore (P packages/app/src/screens/ConnectScreen.tsx:36-46).
- Agents:
  - FlatList sorted by urgency.
  - Not-attached rows are dimmed to 0.5 with "Not attached · open on your Mac".
  - Empty text: "Start one on your Mac: pocketd run claude" (P packages/app/src/screens/AgentsScreen.tsx:24, :39, :74).
- Chat:
  - Glass header: back button, title pill, diff pill (+/−), terminal button.
  - The diff pill and terminal button have no `onPress` (P packages/app/src/screens/ChatScreen.tsx:131-159).
  - `/compact` is intercepted; `agent.view` is sent while the screen is active.
  - The timeline groups tools; details in report 05 F11.
- Composer:
  - Single-line.
  - Attach and Dictate have no handlers.
  - The button is Stop while busy, so a follow-up can only be sent with the return key (P packages/app/src/components/Composer.tsx:28-55; report 05 "#406").
- Permission sheet: Yes / provider options / No, plus a feedback field (P packages/app/src/components/PermissionSheet.tsx:30-80).
- Tokens: two sets.
  - `design.ts` `d` (bg `#0B0C0E`, text `#EDEBE6`, green `#5BE38A`, red `#F07167`, teal `#5CC8BE`), used by chat, composer and timeline (P packages/app/src/design.ts:1-40).
  - `theme.ts` (bg `#0b0d10`, accent `#7aa2f7`, radius 12), used by Connect, Agents and PermissionSheet (P packages/app/src/theme.ts:1-14).
- Tests: `node --test test/` covers `agents.test.mts` and `status.test.mts` (P packages/app/package.json:6-12).

### F8. Capability inventory

| Capability | Desktop | Phone |
|---|---|---|
| List agents with live status, urgency sort | yes, as cards, rail badges and roll-ups (P packages/desktop/crates/pocket/src/status.rs:83-98) | yes (P packages/app/src/screens/AgentsScreen.tsx:9) |
| Start an agent | yes: New session form, tab menu, ⌘N (P packages/desktop/crates/pocket/src/forms.rs:402-460) | no (`agent.create` rejected) |
| New worktree + setup script + copy env files | yes (P packages/desktop/crates/pocket/src/forms.rs:417-455) | no |
| Terminal tabs, splits, raw PTY | yes; no scrollback or selection (P packages/desktop/crates/pocket/src/termview.rs:41-89) | no (dead button) |
| Chat transcript view | **no** | yes |
| Composer / prompt | only as diff line comments → PTY (P packages/desktop/crates/pocket/src/main.rs:904-911) | yes, single-line |
| Interrupt / compact | in the terminal only | yes |
| Permission approve / deny / options / feedback | in the terminal only; inbox shows the ask text (P packages/desktop/crates/pocket/src/inbox.rs:35-37) | yes |
| Plan / tasks / tool cards | no | yes |
| File explorer + preview (code, md, mermaid, images) | yes | no |
| Git diff with syntax/word diff, stage | yes; commit/push/discard only on main (F12) | no (dead button) |
| Command palette | yes: substring, 3 sections | no |
| Notifications | macOS system notifications, click focuses (P packages/desktop/crates/pocket/src/main.rs:314-328, :1103-1106) | none (no push) |
| Seen sync across clients | yes (`agent.view` / `agent.seen`) | yes (`agent.view`) |
| Context % left | yes, 200k assumed (P packages/desktop/crates/pocket/src/view.rs:428-456) | no |
| Themes | light only | dark only |
| Remote access | local only | Tailscale IP + token |

### F9. Extension points

1. **Transcript view (desktop).**
   - The data is already in `Agents.timelines` (P packages/desktop/crates/agents/src/agents.rs:115-152), but `Item` must be widened to the full `TimelineBody` (P packages/protocol/src/timeline.ts:3-113).
   - Surface options:
     - A new `workspace::Tab` variant (P packages/desktop/crates/workspace/src/workspace.rs:3-6) rendered in `session_page` (P packages/desktop/crates/pocket/src/view.rs:783-790).
     - A pane mode beside the terminal.
   - Render pieces:
     - gpui `list` + `ListState`, as in `diff_box` (P packages/desktop/crates/pocket/src/diff.rs:379-395).
     - `TextView` + `markdown_style` + the `Mermaid` plugin for assistant text (P packages/desktop/crates/pocket/src/explore.rs:141-161).
   - Constraints:
     - Claude text arrives as whole blocks, not token deltas.
     - Only the newest 500 items can be fetched.
2. **Composer.**
   - Desktop, two paths:
     - (a) WS `agent.prompt`: add `prompt`/`interrupt`/`compact`/`resolve` to `Outbox` (P packages/desktop/crates/agents/src/agents.rs:207-223).
     - (b) The ops `prompt` already used by comments (P packages/desktop/crates/pocket/src/main.rs:911).
     - Both end in `termDriver.Prompt`, i.e. PTY typing (P packages/pocketd/internal/daemon/presence.go:123-128).
     - A `TextareaState` pattern exists: the comment box, 3 rows (P packages/desktop/crates/pocket/src/main.rs:187-294; P packages/desktop/crates/pocket/src/diff.rs:501-550).
   - Phone: `Composer.tsx` (P packages/app/src/components/Composer.tsx:15-58).
3. **Command palette.**
   - `Pick` enum + `palette_sections` + `activate` (P packages/desktop/crates/pocket/src/overlay.rs:12-18, :79-150).
   - Sources are hard-coded; there is no action registry. gpui actions are declared at P packages/desktop/crates/pocket/src/main.rs:32 and bound at :1046-1057.
4. **Notifications.**
   - `status::alerts` is pure and tested (P packages/desktop/crates/pocket/src/status.rs:119-124).
   - `sync_alerts` posts `SystemNotification { actions: Vec::new() }` (P packages/desktop/crates/pocket/src/main.rs:324). Action buttons such as Allow/Deny are a free slot.
   - The response handler only focuses (:1103-1106).
   - No sound, no dock badge; phone push and dock badge are out of scope (P docs/designs/2026-09-28-agent-sessions.md:155-167, :184).
   - The phone has no notification dependency (P packages/app/package.json:14-31).
5. **Relay.**
   - Clients speak v3 JSON over one WS.
   - Server side: `wsserver` + `hub` subscribers (P packages/pocketd/internal/wsserver/wsserver.go:122-209; P packages/pocketd/internal/hub/hub.go:37-52). A relay can be an outbound WS from pocketd carrying the same frames.
   - Phone side: a URL swap at P packages/app/src/session.tsx:76.
   - Auth is one static token (P packages/pocketd/internal/config/config.go:52-61), with no per-device keys.
6. **GUI-driven agent mode.**
   - Implement `agent.Driver` (P packages/pocketd/internal/agent/agent.go:16-21) and register it with `Registry.AddFunc` (:65).
   - Feed events through `timeline.Event`, as Codex does (P packages/pocketd/internal/codex/session.go:150-168).
   - Codex RPC plumbing exists (`internal/codex/rpc.go`), but `turn/start` is never sent (`rg turn/start packages/pocketd` hits only `turn/started` notifications in tests).
   - Blockers:
     - Cards are placed by the agent's terminal folder (P packages/desktop/crates/pocket/src/status.rs:63-64).
     - `view_set` keys on `terminal_id` (:113-117).
     - Workspaces hold terminal ids (P packages/desktop/crates/workspace/src/workspace.rs:3-6).
     - ADR 0002 ties sessions to terminals.

### F10. Constraints

gpui-kit 0.6.6 limits found so far:
- No backdrop blur (P packages/desktop/crates/pocket/src/view.rs:576).
- `list()` doesn't cascade text style; rows need `.w_full()` (P docs/spike-zed-preview.md:73).
- The Editor has no gutter-marker API (:78).
- `EditorState::set_value` resets scroll (:75).
- The Root binds tab/shift-tab to focus cycling (P packages/desktop/crates/keys/src/keys.rs:5-8).
- `use super::*` in tests shadows `#[test]` (P docs/spike-zed-preview.md:77).
- Markdown:
  - Component `TextView` colours are global `Theme` fields; per-view colours need the base `TextView` (P docs/research-monocode-markdown-styles.md:283-302).
  - Plugin `render` runs for every block on open (P docs/spike-mermaid.md:90).
  - The SVG fontdb has no Geist (:91).
- Windows:
  - Unfocused windows render at 30 fps (P docs/spike-zed-preview.md:72).
  - Occluded windows get no frames (P docs/spike-mermaid.md:98).
- Unknown, per report 06:450: native child views such as WKWebView.
- Dark-mode components need both modes set in `theme::init`, per report 07:782.

Other:
- libghostty's C API is unversioned; it is pinned to `4ae9f1a2…` and built with Zig 0.16.0 (P scripts/build-ghostty.sh:1-12).
- merman is an alpha pinned `=0.8.0-alpha.6` and adds +18 MB to the binary (P docs/spike-mermaid.md:85, :103-104).
- The 150 ms prompt delay was never measured (P docs/spike-remote-sessions.md:118).

Build and test commands:

| Package | Command | Source |
|---|---|---|
| all TS | `pnpm -r typecheck`, `pnpm -r build` | P package.json |
| protocol | `pnpm --filter @pocket/protocol test` | P docs/plans/2026-09-25-pocketd-remote-sessions.md:12 |
| app | `pnpm --filter @pocket/app typecheck && pnpm --filter @pocket/app test`; run `pnpm --filter @pocket/app ios` | P packages/app/package.json:6-12 |
| pocketd | `cd packages/pocketd && go vet ./... && go test -race -count=1 ./...`; goldens `go test ./internal/proto -update`; e2e with fakeclaude/fakecodex | P docs/plans/2026-09-25-pocketd-remote-sessions.md:10; P packages/pocketd/internal/proto/golden_test.go:12; P packages/pocketd/e2e/harness_test.go |
| libghostty | `./scripts/build-ghostty.sh`, once, ~1.5 min | P docs/plans/2026-09-25-pocketd-remote-sessions.md:8 |
| desktop | `cd packages/desktop && cargo test -p pocket -p workspace -p store -p agents -p ui -p daemon -p git && cargo build -p pocket -p storybook` | P docs/plans/2026-09-30-projects-sidebar.md:12 |
| desktop lint | `cargo clippy -p git -p theme -p pocket`; baseline has one `double_ended_iterator_last` warning; don't run `cargo fmt` | P docs/plans/2026-09-28-mermaid-preview.md:12; P docs/plans/2026-09-28-monocode-markdown-style.md:28 |
| run | `go run ./cmd/pocketd serve`, then `cargo run --release -p pocket`; storybook `cargo run -p storybook`; screenshots `--features capture` + `--capture` | P docs/plans/2026-09-30-projects-sidebar.md:13-14; P packages/desktop/crates/pocket/src/capture.rs:36-48 |
| CI | none (no `.github/`) | repo root listing |

### F11. Pocket already has (do not re-clone)

- A PTY daemon whose sessions survive client exit, with libghostty-vt snapshot on attach (P packages/pocketd/internal/terminal/terminal.go; P packages/pocketd/internal/vt/vt.go).
- Login-shell spawning with a clean env and setup-then-agent chaining (P packages/desktop/crates/daemon/src/daemon.rs:65-128).
- Agent detection for claude/codex typed in any Pocket shell; not-attached banners (P packages/pocketd/internal/daemon/detect.go:11-38; P packages/desktop/crates/pocket/src/status.rs:100-110).
- The status machine with Needs you / Done (failed) / Working / Idle and a shared Seen flag; inbox; system notifications with click-to-focus; ⌘J next waiting (P packages/desktop/crates/pocket/src/main.rs:934-943).
- Permission bridge with options and deny feedback, for Claude hooks and Codex app-server (P packages/pocketd/internal/daemon/daemon.go:123-171).
- Timeline folding: tool groups, TodoWrite → tasks, ExitPlanMode → plan, 64 KB clamp. Codex token streaming.
- Projects/worktrees sidebar: collapse, reorder, ⋯ menus, confirm delete, auto names, setup scripts, env-file copy, clone from URL.
- Tabs and splits per worktree; three layouts (Sidebars / Compact rail / Focus); resizable columns.
- Explorer with read-only Editor, markdown + mermaid + images; ⌘P.
- Diff: syntax + word tint, unified/split, folds, line-range comments sent to a chosen agent.
- ⌘K palette over sessions, files and actions, with project scoping.
- Phone: timeline, permission sheet, compact/interrupt, liquid-glass header.
- Screenshot capture harness and storybook.

### F12. In-flight work in the main checkout (collision map)

`git -C /Users/mingo/Developer/self/coding-pocket` shows HEAD `b9d14a1`, one commit ahead of this worktree's `86deb13`.

**Committed in `b9d14a1`** ("Add commit message composition; discard unstaged changes"; 12 files, +838/−116):
- New `P@main packages/desktop/crates/pocket/src/changes.rs` (664 lines). The Changes sidebar moves out of `diff.rs`:
  - Staged/unstaged sections with count pills.
  - A flat or tree list with folded folders (`tree()` :51-73).
  - Stage/unstage, and discard with a confirm (:506-531).
  - A commit box with Commit / Commit & Push / Amend (`CommitKind` :17-21, `commit` :533-574).
  - Push with `push.autoSetupRemote=true` (:627-630).
  - "Write message": `claude -p --model haiku "<PROMPT>"` fed with `git::commit_context` (:14, :597-624).
- `daemon::run_login(argv, cwd, stdin)` runs one-shot commands under the login shell (+48 lines in `daemon.rs`).
- `git.rs` (+80):
  - `FileStat.unstaged`.
  - Multi-path `set_staged`.
  - `discard`: `git clean -fq` for untracked files, `checkout -q` for tracked ones.
  - `commit_context`: last 10 subjects plus the diff, capped at 60,000 bytes.
- New icons: `discard`, `list-flat`, `list-tree`, `minus`.

**Uncommitted** (`diff --stat`: 7 files; the working tree is live, +96/−41 at verification, plus untracked):
- Material Icon Theme 5.38.1 vendored by `scripts/material-icons.mjs` into `theme/assets/material/`: 1128 files, including `icons.json` and `LICENSE`.
  - Embedded by the new `theme/build.rs`.
  - `theme::file_icon(path, folder, open, size)` matches like VS Code: full name, then extensions longest first.
  - Used in explorer `tree_row`, Changes rows and palette file rows (P@main packages/desktop/crates/theme/src/theme.rs:118-139; P@main packages/desktop/crates/ui/src/ui.rs:499; P@main packages/desktop/crates/pocket/src/overlay.rs:25, :189).
- Explorer drops the "All files / Touched by agents" chips, and `Desktop.touched_only` goes (P@main packages/desktop/crates/pocket/src/explore.rs:203-211).
- A `[DEBUG-perf]` frame benchmark under `POCKET_BENCH*` env in `capture.rs`. It looks temporary.
- `Desktop` now has 84 fields.

**Collision surfaces.** Plans must rebase on main before touching:
- `pocket/src/{changes,diff,explore,overlay,main,capture}.rs`
- `theme/src/theme.rs` (the `ICONS`/`Assets` loader)
- `ui/src/ui.rs` (`tree_row`)
- `git/src/git.rs`
- `daemon/src/daemon.rs`

Areas untouched there: pocketd, protocol, phone, `view.rs` (only 6 lines in b9d14a1), `inbox.rs`, `status.rs`, `workspace`.

### F13. Doc/code disagreements

- The remote-sessions plan Task 8.2 promised a composer under the selected tab (P docs/plans/2026-09-25-pocketd-remote-sessions.md:8298-8306). The code has none; the ops `prompt` is used only by diff comments.
- The spike's protocol mapping lists `agent.create` and `agent.stream` for Claude (P docs/spike-remote-sessions.md:84-99). v3 dropped `agent.create` (P packages/pocketd/internal/proto/golden_test.go:102), and Claude has no stream.
- Research §11.1 of the monocode-markdown-style doc describes the pre-plan style: inline code `FILL_2`, code block r8. The code now uses `HAIRLINE` and a bordered r10 card (P packages/desktop/crates/pocket/src/explore.rs:141-152). The doc is historical, not wrong.

## Ideas to clone into Pocket

These are the gaps and tech debt that block cloning competitor features. "Evidence" means Pocket code showing the gap.

| ID | Idea | User value | Evidence | Pocket mapping | Effort | Prerequisites |
|---|---|---|---|---|---|---|
| 14-1 | Desktop transcript view per agent (markdown, tool groups, plan/tasks, result line) | Read what the agent did without scraping the TUI | Timelines downloaded but unused (P packages/desktop/crates/agents/src/agents.rs:284-291); no view (P packages/desktop/crates/pocket/src/view.rs:680-691) | desktop `pocket/src/transcript.rs` (new) + `workspace::Tab` variant (adapt); reuse `markdown_style`, `Mermaid`, `list` | L | 14-3 |
| 14-2 | Desktop composer + WS actions (prompt, interrupt, compact, resolve permission) | Drive agents from the GUI; answer asks without the terminal | Outbox has only view/seen/close (P packages/desktop/crates/agents/src/agents.rs:207-223); inbox says "Answer in the terminal" (P packages/desktop/crates/pocket/src/inbox.rs:244) | `agents` crate Outbox (adapt); `pocket/src/composer.rs` (new) on `TextareaState` | M | 14-3 for permission payloads |
| 14-3 | Decode the full v3 timeline in Rust (tool status/output/diff, tasks, plan, output tokens, permission options/feedback) | Prerequisite for any desktop chat surface | `Item`/`Permission` keep a subset (P packages/desktop/crates/agents/src/agents.rs:29-90) | `agents` crate (adapt); golden tests against P packages/pocketd/internal/proto/testdata/golden | S | none |
| 14-4 | Persist timelines or rebuild them on restart | History survives a pocketd restart | In-memory only (P packages/pocketd/internal/timeline/timeline.go) | pocketd `timeline` + claude/codex readers (adapt) | M | decide store vs re-read JSONL/rollout |
| 14-5 | Backward paging (`beforeSeq`) and a memory cap | Scroll long sessions; bounded RAM | `Page` is forward-only (P packages/pocketd/internal/timeline/timeline.go:155-167) | pocketd `proto`, `timeline`, `wsserver`; protocol TS; phone (adapt) | S | protocol v4 or an additive field |
| 14-6 | Structured driver: Codex `turn/start`/`turn/interrupt`; Claude via SDK/stream-json | GUI-first agents; prompts don't fail when the TUI loses the foreground | Only `termDriver` (P packages/pocketd/internal/daemon/presence.go:113-155); `turn/start` unused | pocketd `agent.Driver` impls (new) | XL | ADR on terminal-less sessions; 14-7 |
| 14-7 | Terminal-less agents in cards, workspaces and view sets | Needed for 14-6 and for phone-created agents | Cards/views key on `terminal_id` (P packages/desktop/crates/pocket/src/status.rs:63-64, :113-117); workspace holds terminal ids (P packages/desktop/crates/workspace/src/workspace.rs:3-6) | desktop `status`, `workspace`, `view` (adapt); ADR 0002 update | M | ADR |
| 14-8 | Relay + TLS + QR pairing + per-device tokens | Reach the Mac off-Tailscale; safer pairing | Plain `ws://`, one static token, manual entry (P packages/app/src/session.tsx:76; P packages/pocketd/internal/config/config.go:52-61; P packages/pocketd/cmd/pocketd/serve.go:46-59) | pocketd outbound relay client (new); phone connect screen (adapt) | L | hosting decision |
| 14-9 | Phone push on Needs you / Done | Know when to look without the app open | No notification dependency (P packages/app/package.json:14-31); out of scope in design (P docs/designs/2026-09-28-agent-sessions.md:184) | phone (new) + relay/APNs (new) | L | 14-8 |
| 14-10 | Actionable desktop notifications (Allow/Deny), optional sound, dock badge | Answer asks from the banner | `actions: Vec::new()` (P packages/desktop/crates/pocket/src/main.rs:324) | desktop `main.rs` / `status.rs` (adapt) | S | 14-2 for resolve |
| 14-11 | Structured AskUserQuestion / ExitPlanMode prompts to clients | Answer questions and approve plans from the GUI or phone | The PreToolUse matcher only flips NeedsYou (P packages/pocketd/internal/daemon/plugin.go:13; P packages/pocketd/internal/daemon/daemon.go:108-109) | pocketd hook reply + proto message (new); phone/desktop sheets (new) | M | 14-3 |
| 14-12 | Palette: fuzzy ranking, action registry, worktrees/projects/commands, more than 5 rows | Keyboard-first navigation at scale | Substring match, 3 fixed sections, `take(5)` (P packages/desktop/crates/pocket/src/overlay.rs:79-134) | desktop `overlay.rs` (adapt) | M | none |
| 14-13 | Split the `Desktop` god-struct into entities (sidebar, page, palette, changes, explore) | Parallel feature work without merge conflicts; fewer re-renders | 77 fields (84 on main) (P packages/desktop/crates/pocket/src/main.rs:99-180) | desktop `pocket` (refactor) | L | land main's in-flight work first |
| 14-14 | Push updates instead of polling (terminal list events, fs watcher for git) | Lower latency and idle CPU | 1 s `list`, 2 s `refresh_git` (P packages/desktop/crates/pocket/src/main.rs:1091-1100) | pocketd ops events + desktop `git` watcher (new) | M | none |
| 14-15 | Persist layout: column widths, window bounds, layout mode, selection | App reopens as left | Widths not saved (P packages/desktop/crates/pocket/src/view.rs:156-190); fixed window (P packages/desktop/crates/pocket/src/main.rs:1059-1068); `Store` has no UI fields (P packages/desktop/crates/store/src/store.rs:20-28) | `store` + `pocket` (adapt) | S | none |
| 14-16 | Terminal: scrollback, selection/copy, links, mouse reporting, search | Baseline terminal UX expected in any orchestrator | `screen` paints only the visible frame (P packages/desktop/crates/pocket/src/termview.rs:41-89); `term` API has no scrollback (P packages/desktop/crates/term/src/term.rs:41-72) | `term` shim + `termview` (adapt) | L | libghostty scrollback API check |
| 14-17 | Phone: wire diff review and raw terminal; send while busy (queue/steer); multi-line; attach; mic | Close the obvious dead ends | P packages/app/src/screens/ChatScreen.tsx:147-159; P packages/app/src/components/Composer.tsx:28-55 | phone (adapt); pocketd attach over WS for terminal (new) | M | terminal-over-WS message |
| 14-18 | Phone: one token set; a navigation stack | Consistent look; deep links and back gestures | Two sets (P packages/app/src/design.ts:1-40; P packages/app/src/theme.ts:1-14); state switch (P packages/app/src/App.tsx:15-32) | phone (refactor) | S | none |
| 14-19 | Create agents from the phone (`agent.create` with cwd/worktree/provider/prompt) | Start work away from the Mac | Rejected (P packages/pocketd/internal/proto/golden_test.go:102) | pocketd `proto`/`wsserver`/`daemon.Spawn` (adapt); phone (new) | M | 14-7 if PTY-less |
| 14-20 | Desktop dark theme via a token struct | Parity with competitors; matches the phone | `pub const` light tokens only (P packages/desktop/crates/theme/src/theme.rs:9-62) | `theme` (refactor per report 07:550) | M | gpui-kit dark component check |
| 14-21 | AgentSummary: branch, worktree, token usage, context window | Accurate context rings and cards | Desktop hard-codes 200k (P packages/desktop/crates/agents/src/agents.rs:56-57); fields absent (P packages/pocketd/internal/proto/proto.go:168-184) | pocketd `proto`/`agent` + clients (adapt) | S | none |
| 14-22 | More providers (gemini, opencode, …) behind a provider table | Clone multi-harness support | `notAgent` has only claude/codex (P packages/pocketd/internal/daemon/detect.go:11-14) | pocketd `daemon` (adapt); `theme::provider_*` (adapt) | M per provider | status source per CLI |
| 14-23 | CI, app bundle/signing, pocketd launch agent | Installable product; regressions caught | No `.github/`; desktop exits without pocketd (P packages/desktop/crates/pocket/src/main.rs:1035-1038) | repo root (new) | M | none |
| 14-24 | Rebase research plans on main (`b9d14a1` + icons) | Avoid redoing Changes/commit/icon work | F12 | process | S | none |

## UI/UX spec to copy

Not applicable: this report documents the baseline and copies nothing. The current Pocket tokens that clones must map onto:

- Desktop (P packages/desktop/crates/theme/src/theme.rs:6-62):
  - Fonts: SANS `.SystemUIFont` 14, MONO `Geist Mono` 13.
  - Text: TEXT `#111113`, TEXT_2 `#6f6f78`, TEXT_3 `#8b8b94`, TEXT_4 `#a1a1aa`.
  - Surfaces: WINDOW `#f4f4f5`, SURFACE `#ffffff`, SURFACE_SUNKEN `#fafafa`. FILL_1..4 are `#111113` at alpha 08/0b/0e/11. SEPARATOR is `#111113` at alpha 17.
  - Accent: ACCENT `#5b5bd6`.
  - Status colours: WAITING `#ffb224`, RUNNING `#30a46c`, FAILED `#e5484d`.
  - Agents: AGENT_CLAUDE `#d97757`, AGENT_CODEX `#0f9d8a`.
  - Syntax: SYN keyword `#8e4ec6`, fn `#3e63dd`, string `#18794e`, comment `#a1a1aa`.
- Desktop metrics:
  - Bars and rows: top bar h42; tabs h28 r7; menu rows h32 r6.
  - Columns: default Projects 272 and Sessions 334; nav rail 56.
  - Overlays: palette 660 r22; row menu w210 r14.
  - Terminal text: 13/22 main and 12/19 small (P packages/desktop/crates/pocket/src/view.rs:193-262, :459-561, :822-980; P packages/desktop/crates/pocket/src/overlay.rs:173-236; P packages/desktop/crates/pocket/src/termview.rs:11-12).
- Phone: `d` tokens (P packages/app/src/design.ts:1-40); Geist 400/500/600 and Geist Mono.

## Open questions / risks

- Does GUI-driven mode (14-6) replace PTY typing or run beside it? ADR 0001/0002 assume every session has a terminal. A terminal-less agent needs a new ADR before 14-7.
- Timeline persistence (14-4): re-read Claude JSONL and Codex rollouts, or keep Pocket's own store? Re-reading keeps one source of truth but depends on formats that change between CLI versions (P docs/spike-remote-sessions.md:107).
- Relay (14-8): self-hosted, third-party, or Tailscale-only with TLS? A single shared token leaks on any device loss.
- Claude hooks are skipped in untrusted folders, so those agents stay "not attached". GUI-first flows must handle this, or pre-trust folders (P packages/desktop/crates/pocket/src/status.rs:106).
- gpui-kit unknowns: native child views, backdrop blur, per-view markdown colours, dark component theming. Any clone of Zeron's blur or embedded browser is blocked until checked.
- In-flight work: `[DEBUG-perf]` in `capture.rs` and the Material icons are uncommitted. Plans should confirm with the user what lands before touching `theme` / `ui` / `changes.rs`.
- The context window is assumed to be 200k for every model (P packages/desktop/crates/agents/src/agents.rs:56-57). This is wrong for larger-window models.
- `termDriver` fails whenever the agent leaves the foreground (P packages/pocketd/internal/daemon/presence.go:119). Queued prompts (report 05) need a retry or a structured driver.

## Verification

Date: 2026-09-30. Claims checked: 15. Corrected: 5.

Confirmed against source: claude/codex-only detection (`detect.go:11-38`); `termDriver` PTY typing, `0x1b` interrupt, `/compact`, SIGTERM, foreground guard (`presence.go:113-155`); Outbox view/seen/close only and 500-item timeline fetch (`agents.rs:207-223,284-291`); 10 client / 9 server v3 types and `agent.create` rejection (`messages.go:65-181`, `golden_test.go:102`); forward-only `Page` (`timeline.go:155-167`); `Driver` interface and `AddFunc` (`agent.go:16-21,65`); `Desktop` 77 fields here, 84 on main; 1 s `list` / 2 s git poll (`main.rs:1091-1100`); keybindings and 1440x900 window with traffic lights (14,14) (`main.rs:1046-1068`); notifications with `actions: Vec::new()` and click-to-focus (`main.rs:314-328,1103-1106`); `Perm` → `--permission-mode acceptEdits|plan`, `--full-auto`, `-s read-only` (`forms.rs:410-413`); desktop tokens, 40 icons, font sizes (`theme.rs`); metrics h42/h28 r7/h32 r6, 272/334/56, palette 660 r22 top 120, menu w210 r14, resize 200-420 / 280-600; phone dead buttons (`ChatScreen.tsx:147-159`, `Composer.tsx:28-46`), `ws://` URL, 1 s→30 s backoff; plugin hooks, 5 s attach, 3 s codex window, 10 min broker, 30 s feedback, 150 ms prompt delay; main's `changes.rs` 664 lines, `claude -p --model haiku`, `push.autoSetupRemote=true`, `commit_context` 10 subjects / 60,000 bytes, Material Icon Theme 5.38.1 with 1128 files.

Corrections:
- TL;DR and F13 said report 04:270 claims Pocket has no notification code. Report 04 (line 271) already says Pocket has notifications and lacks sound. Removed the "correction to report 04".
- 14-11: PreToolUse/Notification → `NeedsYou` is at `daemon.go:108-109`, not `:110-111`.
- 14-2: "Answer in the terminal" is at `inbox.rs:244`, not `:245`.
- F8: phone urgency sort is at `AgentsScreen.tsx:9`, not `:24` (that line is the empty text).
- F9.6: `turn/start` wording tightened; the only `rg` hits are `turn/started` notifications in tests. F12 uncommitted diff stat updated to the live value (+96/−41); the main working tree is still changing.
