# Strategy B: Dual-mode (terminal sessions + chat sessions)

- Date: 2026-09-30. Horizon: 8 weeks (M0 wk 1–2, M1 wk 3–4, M2 wk 5–6, M3 wk 7–8).
- Inputs: research reports R01–R19 in `docs/orchestrators/research/`, Pocket at `orchestrator-research` (b9d14a1), PO decisions D1–D13.
- Citations: `[R10 §12]` is a report section, `[R17 idea 17-1]` is an idea row, and `P path:L` points at Pocket code. "new" means the item is proposed here and has no report ID.
- **Assumed A.** Strategy A is not written yet. This doc assumes A is terminal-first: M0 fixes (D12) and trust floor (D10), LaunchSpec (D5), the Codex shared-thread driver (D4), the terminal surface (D9), attention and push (D2, D3, D11), and phone start and parity. **B = A + chat agents.** A-inherited epics are marked **A** below. B-only work is marked **B**.

## 1. Thesis

- **Pocket copies the best of Zeron and MonoCode only if it drives agents over their structured protocols.** The reports show why:
  - MonoCode: "Pocket runs real PTYs, so transcript-level features do not port directly" [R09 TL;DR].
  - Zeron's unit of work "is a chat, not a terminal" [R01 TL;DR].
  - The headline UX pieces need structured control:
    - Steer, Send next and Send now [R05 F6; R03 idea 03-5].
    - The question wizard, which needs `AskUserQuestion` as data [R05 F7].
    - Plan capture [R10 idea 10-16].
    - Undo/Keep, which needs tool events [R13 idea 13-1; R10 idea 10-14].
    - Per-turn model and effort, and reported context usage [R05 F8].
  - Over a PTY, all of these are approximations or impossible:
    - Prompts are typed into the TUI and fail with `errNotForeground` (P packages/pocketd/internal/daemon/presence.go:119-146).
    - Hooks only flip Needs you for `AskUserQuestion|ExitPlanMode` (P packages/pocketd/internal/daemon/plugin.go:13).
- **Terminals stay first-class.** Headless mode loses TUI-only features: dialogs, `/login`, and slash commands outside `initialize.commands` [R03 risks]. Terminal fidelity is one of Pocket's four edges [R15 TL;DR].
  - So B does not replace terminals. It adds a second session kind in the same fleet view, the same status model and the same worktree list.
- **Starts from the phone or an automation want chat, not a PTY.**
  - Nobody watches a TUI started from the phone.
  - A chat agent resumes by conversation id after a pocketd restart. PTYs die (P packages/pocketd/internal/timeline/timeline.go is in memory only), and restore is table stakes [R15 idea 15-1].
- **B degrades to A.** Chat support is gated per CLI version (03-12). If Claude drops the undocumented `--permission-prompt-tool stdio` [R03 risks], the chat kind turns off for Claude and terminal sessions keep working. A is B's fallback, not its rival.

**Against the field**

| Rival | Their model | Where B wins |
|---|---|---|
| Zeron | Chat-first headless engine with the terminal as a drawer [R01 TL;DR]. "Every signed-in device is fully trusted", no E2EE [R18 TL;DR]. Codex forced to yolo [R03 risks] | The same chat UX (E8–E10), plus real TUIs as sessions. Tailnet only, per-device tokens, explicit access on both axes [R17 F14] |
| MonoCode | Headless GUI over 10 CLIs [R09 TL;DR]. Remote access is an SSH host, "very early" [R09 TL;DR]. No phone push. Automations run only while the app runs [R11 idea 11-9] | The same headless kernel and Undo/Keep, plus an iPhone app with push and daemon automations that run with the desktop closed |
| Superset | Terminals plus wrappers. iPhone needs $20/mo Pro, through a vendor relay with no documented E2EE [R15 F1] | Free and self-hosted. The phone can start chat sessions |
| Happy | A `happy claude` wrapper that switches local/remote mode in one process; vendor server with E2EE [R15 F2] | The same terminal↔chat idea, but as a per-session kind in one fleet view, with no wrapper and no server |
| Claude Remote Control / ChatGPT "Control this Mac" | Single provider, vendor relay, Pro+. "Another connection took over this session" [R15 F5, F6] | Multi-provider, one fleet. The same exclusivity rule, made explicit (§3) |

**Cost, stated honestly**
- B adds E5, E7 and E8 plus the chat parts of E3, E9, E10, E13 and E14, about +2 weeks over A.
- It brings an undocumented Claude protocol and an experimental Codex app-server.
- There are two surfaces to keep coherent.
- The account terms for driving `claude --print` from a third-party GUI were not researched [R03 risks].

## 2. Target user and jobs

**User**
- A solo or indie developer on macOS plus iPhone (D13).
- Runs 2–6 Claude Code and Codex agents in parallel worktrees.
- Already logged into both CLIs on their own plans.
- Won't route code through a vendor relay [R15 TL;DR].
- The owner is the first user.

| # | Job | B's answer | Metric (§8) |
|---|---|---|---|
| J1 | When an agent needs me, answer it in seconds, wherever I am | Push (D3), inline approvals, question wizard, ⌘J | M-1 |
| J2 | Keep several agents busy in parallel worktrees and see the fleet at a glance | Stable sidebar (D2), one status model across both kinds, attention surfaces | M-7 |
| J3 | Redirect a running agent without killing it | Steer / Send next / Send now on every driver that supports it (E9) | M-4 |
| J4 | Start work from the phone or on a schedule, with no terminal waiting | `agent.create` with `kind: chat` (E3, E12), automations (E14) | M-5 |
| J5 | See and undo what an agent changed, or drop into the real TUI when chat isn't enough | "Changed N files" Undo/Keep (E13), "Open in terminal" (E7) | M-6 |

## 3. Coexistence decision (vocabulary)

**Rules**
- A session stays "one agent, from launch to exit" (P CONTEXT.md:23-25).
- An agent now comes in two kinds, terminal agent and chat agent, both in the same session list with the same Status.
- E3 applies the edits below and writes ADR 0003, "Chat agents: sessions without terminals", which amends ADR 0002 (P docs/adr/0002-sessions-are-agents-terminals-belong-to-worktrees.md).

**CONTEXT.md edits (verbatim, applied in E3)**

- **Agent** (replaces P CONTEXT.md:27-29): "A `claude` or `codex` process pocketd runs for the user, from launch to exit. A terminal agent runs the CLI's own interface in a terminal; a terminal runs at most one at a time and can run several over its life. A chat agent has no terminal." _Avoid_: provider session, thread
- **Chat agent** (new): "An agent pocketd drives over the CLI's structured protocol instead of a terminal: Claude in stream-json print mode, Codex on a private app-server. It is always attached and belongs to the worktree it was started in. pocketd may stop its process while Idle and resume the conversation on the next prompt; it stays the same agent until the user closes it." _Avoid_: headless session, chat session, SDK agent
- **Session** (amend the last two sentences): "Closing a running session closes its terminal, or ends its chat agent. It belongs to its terminal's worktree, or its chat agent's, wherever the agent `cd`s."
- **Conversation** (append): "A conversation has at most one live agent."
- **Seen** (amend): "…while its pane, terminal or chat, is visible in a focused desktop window, or its timeline is open on the phone."
- **Automation / Run** (new in E14, per [R11 idea 11-18]):
  - Automation: "A saved prompt and launch spec pocketd starts on a schedule."
  - Run: "One start of an automation; it creates one session."
  - Inbox keeps its current meaning.

**Behaviour**

| Aspect | Terminal agent | Chat agent |
|---|---|---|
| Desktop surface | Terminal pane in a worktree tab | Chat pane in a worktree tab: workspace leaf `Chat(agent_id)` next to `Terminal(id)` [R14 idea 14-1 "workspace::Tab variant"], so it can sit in a split beside a terminal |
| Phone surface | ChatScreen timeline (today) | ChatScreen timeline, which renders unchanged [R10 §12] |
| `AgentSummary.terminalId` | set | empty. `kind:"chat"` is added (E3) |
| Close | Closes the terminal | Ends the chat agent. Confirm if Working [R13 idea 13-13] |
| Status source | Hooks (Claude) / account app-server (Codex, D4) | Protocol frames. Always attached |
| Survives pocketd restart | Only via resume argv [R15 idea 15-1] (E7) | Yes, lazily resumed by conversation id (E7) |
| Default for | Desktop ⌘N first run, ⌘T, tab-menu quick rows | Phone new session, automations |

- **Kind is a LaunchSpec field.** B adds `kind: "terminal" | "chat"` to D5's LaunchSpec. The desktop draft canvas has a Kind chip whose last pick is remembered, like the other picks [R17 idea 17-5].
- **Exclusivity.**
  - pocketd holds one lock per conversation id.
  - A terminal agent that reports a `SessionStart` for a conversation a chat agent holds parks the chat agent, which shows "Continued in a terminal". This follows Claude's "Another connection took over this session" [R15 F5].
  - "Open in terminal" is disabled while Working [R03 risks].
- **Handoff (one direction in the horizon).**
  - "Open in terminal" runs at Idle. It ends the chat agent and opens a new terminal in the same worktree running `claude --resume <id>` or `codex resume <id>` [R03 §9; R03 idea 03-9].
  - This is a new session, because a session is one agent.
  - "Open as chat" is deferred (§6).

**Capabilities per kind × provider** (what E4–E13 must deliver; a dash means not offered)

| Capability | Claude terminal | Codex terminal (D4) | Claude chat | Codex chat |
|---|---|---|---|---|
| Prompt | PTY typing (P presence.go:113-155) | `turn/start` [R03 §9] | stdin user line [R10 §12] | `turn/start` |
| Steer while Working | – (queued in pocketd, drained at Idle) | `turn/steer {expectedTurnId}` | steer line, priority `now` with no open tool, else `next` [R03 §5] | `turn/steer` |
| Interrupt | Esc | `turn/interrupt` | `control_request interrupt` → SIGTERM 2 s → SIGKILL 3 s [R03 §5] | `turn/interrupt` |
| Structured questions | Hook reply for `AskUserQuestion` [R14 idea 14-11] (verify) | `requestUserInput` (verify on shared thread) | `can_use_tool` → `updatedInput.answers` | `item/tool/requestUserInput` |
| Plan capture | – | – | Deny `ExitPlanMode`, capture as plan [R03 §5] | `collaborationMode: plan` [R10 idea 10-16] |
| Model/effort/access change | TUI only | TUI only (turn/start omits policy) [R03 §9] | Respawn with `--resume` [R10 §12] | Per turn |
| Context % | transcript usage | `thread/tokenUsage/updated` | `result` usage | `thread/tokenUsage/updated` |
| Undo/Keep files | PreToolUse `Edit\|Write\|MultiEdit\|NotebookEdit` [R13 idea 13-1] | – | tool events | tool events |
| Conversation rewind | – | – | – (Claude has none) | `thread/revert` [R10 idea 10-15] |

## 4. Milestones

| Milestone | Weeks | Epics | Exit demo |
|---|---|---|---|
| M0 fix-now | 1–2 | E1, E2 (+ the 1-day E5 probe) | Codex Auto-edit starts. `a --b` renders spaced. Phone sends while Working. A PTY child can't resolve permissions. pocketd is not on 0.0.0.0. Phone pairs by QR |
| M1 foundations | 3–4 | E3, E4, E5, E6 | Phone starts a Claude chat session in a new worktree and drives it end to end. Codex terminal prompts never hit `errNotForeground`. Terminal scrolls, selects and copies |
| M2 chat UX | 5–6 | E7, E8, E9, E10 | Desktop chat pane with tool summary, queue with Steer / Send next / Send now, question wizard, context ring. Codex chat works. Chat survives a pocketd restart |
| M3 reach | 7–8 | E11, E12, E13, E14 | Lock-screen push → answer. Undo/Keep. A daily automation opens a chat session in a fresh worktree |

**Lanes.** Each milestone's epics touch disjoint paths so /implement plans can run in parallel. Plans touching `pocket/src/{changes,diff,explore,overlay,main,capture,view}.rs`, `theme.rs`, `ui.rs`, `git.rs`, `daemon.rs` or `workspace.rs` must branch from `main`, where these files carry uncommitted work [R14 F12; git status at session start].

## 5. Epics

| ID | Slug | Title | A/B | Size | Milestone | Depends on |
|---|---|---|---|---|---|---|
| E1 | m0-fix-now | Fix now (D12) | A | S | M0 | – |
| E2 | m0-trust-floor | Trust floor + protocol v4 handshake (D10) | A | L | M0 | – |
| E3 | launchspec-v4 | Chat-ready wire, vocabulary and LaunchSpec | A+B | XL | M1 | E2 |
| E4 | codex-shared-thread | Codex terminal agents over the shared thread (D4) | A | M | M1 | – |
| E5 | claude-chat | Claude chat driver | B | L | M1 | E3 (wire), probe in M0 |
| E6 | terminal-surface | Terminal must-dos (D9) | A | M | M1 | – |
| E7 | codex-chat-restore | Codex chat driver, restore, handoff | B | L | M2 | E3, E4 (rpc error reply), E5 (headless pkg) |
| E8 | desktop-chat-pane | Desktop chat pane + draft canvas | B | XL | M2 | E3, E5 |
| E9 | composer-queue | Composer: queue, steer, context, per-turn settings | A+B | L | M2 | E4, E5, E8 |
| E10 | questions-approvals | Questions, plans, approvals | A+B | L | M2 | E3, E5, E8 |
| E11 | attention-push | Attention on desktop + phone push (D2, D3, D11) | A | L | M3 | E2 |
| E12 | phone-start-parity | Phone new session + parity | A+B | M | M3 | E3, E5 |
| E13 | checkpoints | Undo/Keep file checkpoints | A+B | L | M3 | E5, E7, E8 |
| E14 | automations | Automations that start chat sessions | B | M | M3 | E3, E7 |

### E1 m0-fix-now (M0, S, A)

- **Goal.** The four D12 bugs are gone before any feature lands.
- **Ideas and fixes**
  - 17-1: Codex Auto-edit `--full-auto` → `-s workspace-write -a on-request`. Today codex 0.159.0 rejects the flag (P packages/desktop/crates/pocket/src/forms.rs:412) [R17 F2].
  - 17-2: send both axes explicitly for every mode, per the [R17 F14] table. Ask becomes `--permission-mode default` / `-s read-only -a on-request`. Codex Plan is disabled with the hint "Codex plan needs a chat session". It is the same six lines, and it ends the silent `auto`.
  - 16-3: ligatures off (`liga`/`calt`/`dlig` = 0) for the terminal font (P packages/desktop/crates/theme/src/theme.rs:7,199). First confirm by typing `a --b` [R16 idea 16-3].
  - 05-2: phone primary button. Busy with text → Send. Busy and empty → Stop. Enter never stops. Today it is `onPress={busy ? onInterrupt : submit}` (P packages/app/src/components/Composer.tsx:51-52), Zeron #406 [R05 idea 05-2]. The send path stays `agent.prompt`; E9 defines queue semantics.
  - Agent self-approval (D12) is delivered by E2 (18-6 + 18-8) in the same milestone.
- **Done when**
  - `codex -s workspace-write -a on-request` starts from the sheet on 0.159.0.
  - `a --b` renders with its gap.
  - The phone can send while Working.
  - Unit tests cover the argv table.
- **Touches:** forms.rs, theme.rs or termview.rs (branch from main), app Composer.tsx.

### E2 m0-trust-floor (M0, L, A)

- **Goal.** Meet D10 before any phone spawn: a paired phone is a shell as the user [R18 TL;DR], and today any PTY child can approve its own permissions (P packages/pocketd/internal/wsserver/wsserver.go:156-161; P packages/pocketd/internal/daemon/plugin.go:81).
- **Ideas**
  - 18-1: bind to loopback + Tailscale IPs, re-checked every 30 s (P packages/pocketd/cmd/pocketd/serve.go:46).
  - 18-2: handshake hardening. Host allowlist, no `InsecureSkipVerify`, pre-auth limits.
  - 18-3: per-device 32-byte tokens stored as sha256 in `devices.json`, revoke with 4401, `pocketd devices`.
  - 18-4: QR pairing with a one-time 128-bit code, 5 min TTL. `pocketd pair` prints the QR. Supersedes 08-18/19-11.
  - 18-6: the desktop talks over the ops socket with owner scope and stops reading `config.json` (P packages/desktop/crates/agents/src/agents.rs:198-200,248).
  - 18-7: scopes observe/drive/approve/spawn (needed by 18-10 in E3).
  - 18-8: per-terminal agent grants with a peer-PID ancestry check. No approve, pair or devices calls from PTY descendants.
  - 18-9: prompt sanitation. Strip C0/ESC; agent-sourced prompts may not start with `!` or `/`.
  - 18-14: phone token stored `WHEN_UNLOCKED_THIS_DEVICE_ONLY`.
  - Transitions log (new, S): append `{ts, agentId, kind, provider, from, to, client}` to `~/.coding-pocket/transitions.jsonl` for the §8 baseline.
  - Protocol v4, bumped once here: `hello` carries `min`/`max` plus `capabilities[]` [R19 idea 19-10; R02 idea 02-9], plus the `pair` pre-auth message. Every later epic adds capabilities, never versions (P packages/protocol/src/constants.ts:1).
- **Done when**
  - `lsof -i :4517` shows no wildcard bind.
  - A process inside a Pocket terminal calling `permission.resolve` or ops approve is refused (test).
  - Revoking a device drops its socket.
  - The desktop runs with the legacy token deleted.
  - Goldens are regenerated (`go test ./internal/proto -update`).
- **Touches:** pocketd `wsserver`, `ops`, `daemon/plugin.go`, `config`, new `devices`; desktop `agents`; app ConnectScreen + new Pair screen; `packages/protocol`.

### E3 launchspec-v4 (M1, XL, A+B)

- **Goal.** One `agent.create` for every client and both kinds. The wire and the desktop model accept agents without terminals.
- **Ideas**
  - Vocabulary: apply §3 to CONTEXT.md and add ADR 0003. Update `docs/designs/2026-09-28-agent-sessions.md:138`, where terminalId is "always set".
  - 14-7 / 14-21: `AgentSummary` gains `kind`, an optional `terminalId`, `worktree`, `branch` and `usage{used,window}`. The desktop places cards by worktree, not terminal folder (P packages/desktop/crates/pocket/src/status.rs:63-64). Views key on agent id when there is no terminal (:113-117). The workspace gets a `Chat(agent_id)` leaf (P packages/desktop/crates/workspace/src/workspace.rs:3-6).
  - 14-3: decode the full timeline Item in Rust (P packages/desktop/crates/agents/src/agents.rs:29-90).
  - 14-5: `beforeSeq` paging.
  - 17-10 / 10-3 / D5: `agent.create {commandId, spec: LaunchSpec{project, checkout, provider, model, effort, access, plan, prompt, kind}}`. `agent.configure {agentId, model?, effort?, access?, plan?}`. `question.answer {requestId, answers|skip}`. Today `agent.create` is rejected (P packages/pocketd/internal/proto/golden_test.go:102).
  - 10-13 / 02-5: `commandId` dedupe for `agent.create` and `agent.prompt`.
  - argv moves into pocketd: one builder for the phone, the desktop sheet and the quick rows [R17 F13 Stage B]. Terminal kind → PTY argv via the F14 table. Chat kind → E5/E7 drivers.
  - 11-4: pocketd creates worktrees (branch from base, auto name), so the phone and automations can use `checkout: new` (P packages/desktop/crates/git/src/git.rs:166-173 today).
  - pocketd project registry: the desktop registers projects over ops. Needed by 18-10 (new; prerequisite named in [R18 idea 18-10]).
  - 18-10: phone limits (D5). claude|codex only, registered project only, no cmd/env/cwd. Auto and Full access only behind a desktop setting.
  - 17-8 / 09-9 / 10-6: the CLI availability probe runs in pocketd with login-shell env capture and a 30 s TTL. 03-12: version floor + capability probe, reported per provider and kind in `hello.ok`.
  - Desktop sheet, kept as a modal until E8: 17-3 access picker (Ask / Auto-accept edits / Auto / Full access + Plan first), 17-5 persisted picks (never Full access), 17-9 offline and spawn-failure states, 17-11 quick rows use the picks.
- **Done when**
  - Goldens cover every new message.
  - A terminal-kind `agent.create` from a test client opens a tab on the desktop.
  - A chat-kind create returns a clear "chat not available" error until E5 lands.
  - The phone cannot pass cwd or cmd (test).
- **Touches:** `packages/protocol`, pocketd `proto`/`wsserver`/`daemon`/new `git`, desktop `agents`/`status`/`workspace` (branch from main)/`forms.rs`/`store`.

### E4 codex-shared-thread (M1, M, A)

- **Goal.** Codex terminal agents are driven over pocketd's existing second-client connection (D4). They get steer and interrupt, and never fail on foreground.
- **Ideas:** 03-1 [R03 §9 steps 1–8].
  - Record `turnId` on `turn/started`.
  - Prompt: `turn/start` when Idle, `turn/steer {expectedTurnId}` while Working, and queue until `turn/completed` if rejected.
  - `turn/interrupt` instead of Esc.
  - Reasoning summaries → thinking.
  - `thread/tokenUsage/updated` → `usage`.
  - "Always allow" → `acceptForSession` (P packages/pocketd/internal/codex/session.go:272-279).
  - `turn/start` omits the policy fields, so the TUI's settings stand.
  - Add an error reply carrying `code` to `rpc.go` (P packages/pocketd/internal/codex/rpc.go:34-42,147-150). E7 needs it too.
- **Done when**
  - A prompt sent from the phone to a Codex terminal whose TUI is behind `less` still lands.
  - Steer during a turn shows in the TUI.
  - The originator side effects [R03 risks] have been checked and noted in the plan.
- **Touches:** pocketd `internal/codex` only.

### E5 claude-chat (M1, L, B)

- **Goal.** A Claude chat agent that the phone ChatScreen can drive end to end: prompt, stream, allow/deny, interrupt, compact.
- **Gate (M0 wk 2, 1 day).** Probe claude 2.1.285 and record golden fixtures:
  - `initialize`, `can_use_tool`, `AskUserQuestion`, `ExitPlanMode`, interrupt, `result`;
  - whether a `PermissionRequest` hook also fires with the stdio prompt tool.
  - No-go if the stdio prompt tool is rejected. Then B collapses to A for Claude, and E7's Codex chat keeps B alive.
- **Ideas:** 03-2 / 10-1, 03-3, 10-5, 03-11, 10-4 (Claude column), 03-10, 03-12 floor.
  - New `pocketd/internal/headless`. `Registry.AddFunc` with `TerminalID ""` (P packages/pocketd/internal/agent/agent.go:65) and a new `Driver` (P agent.go:16-21).
  - Spawn with the [R03 §9] recipe. `Setpgid`. Env stripped of `CLAUDE_CODE_PLUGIN_DIRS` and `POCKETD_*`. `POCKETD_HARNESS_PARENT` marker for the orphan reaper [R10 §12].
  - Handshake ≤8 s.
  - `can_use_tool` → broker → allow with `updatedInput`, or deny.
  - `AskUserQuestion` → `question.answer` → `updatedInput.answers`. The wire is from E3; the desktop and phone UI come in E10, and M1 shows the options in the existing PermissionSheet.
  - `ExitPlanMode` → deny + plan item.
  - Status straight from frames. Held-Done settles 5 s [R03 §5].
  - Settings change → respawn with `--resume`.
  - `--permission-mode default`, with `manual` if the probe says `default` is gone [R17 F11].
  - `claude.Map` reuse is verified against the fixtures [R10 risks].
- **Done when**
  - Fixture tests pass under `go test -race`.
  - A phone-created chat agent in a new worktree runs a tool behind an Ask permission, gets approved from the phone, and reaches Done.
  - No double permission prompt (M-8).
- **Touches:** new pocketd `internal/headless`, `internal/daemon` wiring only.

### E6 terminal-surface (M1, M, A)

- **Goal.** D9 must-dos, so terminal sessions stay a fair choice beside chat.
- **Ideas:** 16-1 scrollback with a 10k-line cap, 16-2 wheel, 16-4 selection, 16-5 ⌘C copy (never ^C with a selection), 16-6 ⌘V bracketed paste with a multi-line confirm [R16 TL;DR].
- **Done when:** you can scroll back 10k lines, select by 1/2/3 clicks, and copy and paste in a Pocket terminal.
- **Touches:** desktop `term` shim, `pocket/termview.rs`, `main.rs` (branch from main).

### E7 codex-chat-restore (M2, L, B)

- **Goal.** Codex chat parity with Claude chat. Chat agents survive pocketd restarts. One escape hatch to the TUI.
- **Ideas**
  - 10-2: private `codex app-server` over stdio.
    - `initialize {clientInfo:"coding-pocket", experimentalApi}`, then `thread/start|resume`.
    - `turn/start` carries the [R17 F14] Codex headless access columns, model, effort and `collaborationMode` per turn, with no respawn.
    - Handle server requests: approvals (`accept|acceptForSession|decline`), `permissions/requestApproval`, `requestUserInput`, elicitation decline, `currentTime/read`. Everything else gets `-32601` [R10 §12].
  - 10-4: Codex column. Re-verify `untrusted` on app-server [R17 open questions].
  - 10-7 + 14-4 / 02-11: persist `{agentId, provider, worktree, conversation, spec}` and append timelines to `~/.coding-pocket/timelines/{agent}.jsonl`.
    - On start, Idle chat agents come back and their process spawns on the next prompt. This is also 10-8's idle park after 5 min.
    - An in-flight turn is marked interrupted with "Inspect the work before continuing", with no replay [R10 risks].
  - 15-1: terminal agents are restored via resume argv in a new terminal.
  - Exclusivity lock and "Open in terminal" (03-9, §3).
  - `pocketd stats` reads the journal (new; §8).
- **Done when**
  - Kill -9 pocketd; on restart, the Idle Claude and Codex chat sessions accept a prompt and continue the same conversation.
  - "Open in terminal" continues in the TUI, and the chat card shows "Continued in a terminal".
  - Codex schema fixtures are generated from 0.159.0 (`codex app-server generate-json-schema`) and checked in CI.
- **Touches:** pocketd `internal/headless`, `internal/codex` (shared rpc), new `internal/journal`.

### E8 desktop-chat-pane (M2, XL, B)

- **Goal.** The Zeron/MonoCode chat pane on the desktop, for chat sessions. Terminal sessions keep their terminal on the desktop; the phone shows the timeline for both.
- **Ideas**
  - 14-1 / 03-4 / 10-12 / 05-23: the pane on workspace leaf `Chat`.
  - 05-1 / 03-15: tool-group summary "Ran 3 commands · edited 2 files · 1 failed".
  - 05-10 / 13-16: fold settled groups.
  - 13-9: tool-row grammar.
  - 05-8: working trailer and "Worked for Xm Ys".
  - 05-9: optimistic bubble at 0.65 opacity, "Not delivered — tap to retry".
  - 05-16: show full output.
  - 05-17: jump pill.
  - 03-10: streamed thinking.
  - 17-4: the draft "New session" canvas replaces the modal, with a Kind chip (§3) and the E3 chips.
  - 17-13: hero→dock glide (0.420 s) when a chat draft sends, skipped under reduced motion (P packages/desktop/crates/pocket/src/main.rs:1041-1044).
- **Constraint.** Status colours follow D1. The pane is its own entity in `pocket/src/chat.rs` holding its own state. It adds one field to `Desktop`, which already has 84 [R14 idea 14-13] (P packages/desktop/crates/pocket/src/main.rs:101).
- **Done when**
  - ⌘N → Kind Chat → prompt → the pane docks and streams.
  - Tool groups fold.
  - Seen works for chat panes (Done never appears for a chat you watched finish).
- **Touches:** desktop `pocket` (new `chat.rs`, `view.rs`, `forms.rs`, `overlay.rs`; branch from main), `agents`.

### E9 composer-queue (M2, L, A+B)

- **Goal.** One composer grammar on desktop and phone. Enter never stops. Follow-ups are never lost.
- **Ideas**
  - 05-2 on the desktop.
  - 05-3 / 03-5 / 10-9 / 08-12: a visible queue with row actions using Zeron's tooltips [R05 F6]: Steer "keep current work running", Send next, Send now "interrupt".
  - Row actions are disabled per the §3 capability table. Claude terminal has no Steer.
  - The queue lives in pocketd, so the phone and desktop see one queue and it survives client disconnects [R05 F6 "lives on the session doc"; R01 idea 01-12]. It drains at Idle for Claude terminals.
  - 05-7 / 03-6 / 08-17 / D7: 16 px context ring. Warning at 75%, danger at 90%, hidden when the window is unknown. Replaces the hard-coded 200k (P packages/desktop/crates/agents/src/agents.rs:56-57).
  - Chat-only per-turn chips via `agent.configure`: access, model (CLI aliases + last-used; no catalog, §6), effort. Footnote: "Access changes apply to the next turn. Stop and resend to apply them now." [R17 F9]
  - 05-11 / 08-9: drafts per session.
- **Done when**
  - A message queued on the phone while a Codex terminal turn runs can be steered from the desktop.
  - Enter on an empty busy composer does nothing.
  - The ring changes colour at 75 and 90 on a fixture.
- **Touches:** pocketd `daemon` (queue), desktop `chat.rs`, app Composer/session.

### E10 questions-approvals (M2, L, A+B)

- **Goal.** Answer permissions, questions and plans in place, without the TUI.
- **Ideas**
  - 05-4 / 10-10 / 01-13: question wizard. Paged, keys 1–9, 220 ms auto-advance, free-text field [R05 F7]. On desktop and phone.
  - 14-11: terminal Claude via a PreToolUse hook reply (verify the hook can return answers; otherwise "Answer in the terminal" stays for that cell).
  - 08-13: phone inline approval panel holding every open request. Fixes the single-request state (P packages/app/src/session.tsx:61-63).
  - 13-11: desktop approval toast stack. Inline Allow/Deny rows in the chat pane.
  - 10-16: plan card with Approve (continue in the chosen access) / Revise. Chat only.
  - "Always allow" uses `permission_suggestions` (Claude, verify) and `acceptForSession` (Codex) [R03 §9].
- **Done when**
  - A 3-question `AskUserQuestion` is answered with keys on the desktop and by taps on the phone.
  - A chat plan is approved and continues.
  - Two concurrent permission requests both reach the phone.
- **Touches:** pocketd broker + `daemon/permission.go`, desktop `chat.rs`/`inbox.rs`, app PermissionSheet → inline panel.

### E11 attention-push (M3, L, A)

- **Goal.** J1: Needs you reaches the user fast, and only when it isn't already Seen (D2, D3, D11).
- **Ideas**
  - Desktop:
    - 05-6 / 04-9: three sound cues on non-Seen transitions, even while focused. Per-cue toggles, default on.
    - 05-5 / 13-20 / 01-1: the notification body is the ask. Copy "Run finished" / "Waiting on your input" / "Run failed".
    - 14-10: Allow/Deny actions (P packages/desktop/crates/pocket/src/main.rs:324).
    - 09-1: dock badge = Needs-you count.
    - 15-15: ⌘⇧U jumps to the latest unread. It collides with none of ⌘K/⌘P/⌘N/⌘J/⌘T/⌘⇧N/⌘↵ (P main.rs:1060-1071); ⌘1–9 per D6.
  - Phone:
    - D3 / 19-12: pocketd sends through Expo Push Service on seen-aware transitions (08-4). One collapse id per session (08-3).
    - 08-2: `push.register` over the socket as a capability.
    - 08-5: `pocketd push status`.
    - 18-13: lock-screen privacy.
    - 19-13: pre-prompt.
- **Prerequisite:** paid Apple Developer account and an EAS project [R19 idea 19-12].
- **Done when**
  - A Needs you on a phone-started chat reaches the lock screen within 5 s.
  - No banner is shown for a Seen session.
  - `pocketd push status` lists the last 30 deliveries.
- **Touches:** desktop `main.rs`/`status.rs` (branch from main), pocketd new `internal/push`, app `push.ts`.

### E12 phone-start-parity (M3, M, A+B)

- **Goal.** J4 on the phone. The phone has no dead ends.
- **Ideas**
  - 14-19 / 08-21: new session screen. Project (registry), checkout, provider, access within the 18-10 limits, Kind defaulting to Chat, prompt.
  - 14-17: wire or hide the dead buttons: mic and "Open raw terminal" (P packages/app/src/screens/ChatScreen.tsx:155-159).
  - 14-18: token set and navigation stack.
  - 08-6 / 08-7: graced connectivity and redial on active.
  - 08-8: outbox using `commandId`.
  - 08-11: fleet summary line "2 working · 1 needs you".
- **Done when**
  - Starting a chat in a new worktree from the phone is at most 4 taps plus the prompt.
  - A prompt typed offline is delivered once after reconnect.
- **Touches:** `packages/app` only.

### E13 checkpoints (M3, L, A+B)

- **Goal.** J5: MonoCode's "Changed N files" card with Undo/Keep.
- **Ideas**
  - 10-14 / 13-1: per-session file checkpoints. Snapshot before the first edit of a turn from tool events (chat) or a PreToolUse matcher for `Edit|Write|MultiEdit|NotebookEdit` (Claude terminal; widens P packages/pocketd/internal/daemon/plugin.go:13).
    - Limit 500 files.
    - Undo refuses files edited by someone else since the snapshot.
  - 10-15: Codex chat Undo also offers `thread/revert`. Claude Undo restores files only and adds a timeline note.
- **Done when**
  - Undo restores files byte-identical.
  - A foreign edit blocks Undo for that file with a reason.
  - Checkpoints sitting in a full-access turn race-test green or are refused [R10 risks].
- **Touches:** pocketd new `internal/checkpoint`, `daemon/plugin.go`; desktop `chat.rs`; app timeline card.

### E14 automations (M3, M, B)

- **Goal.** J4 without a human start: pocketd runs saved prompts on a schedule, as chat sessions, with the desktop and phone closed.
- **Ideas**
  - 11-9, minimal:
    - Schedules hourly / daily / weekdays / weekly in local time.
    - Run now, pause.
    - 720 min missed-run grace.
    - CAS on `next_run_at`, restart recovery.
    - 100-run history.
  - 11-10: default launch is a chat session in a fresh worktree via LaunchSpec + 11-4.
  - 11-18: vocabulary.
  - The access default is the user's pick, never Full access. A Run that needs you raises Needs you like any session.
- **UI:** a desktop sheet (list + form) and a phone list with Run now.
- **Done when:** a weekday 09:00 automation fires after a pocketd restart at 08:59, opens a chat session in a new worktree, and its Done pushes.
- **Touches:** pocketd new `internal/automation`; desktop sheet; app list.

## 6. What we deliberately don't build (horizon)

- **One conversation live in both TUI and chat at once.** It is locked instead (§3). "Open as chat" from a terminal agent is also out: a TUI must exit first, and the manual path is enough.
- **Chat replacing terminals.** Terminal stays the desktop default. The CLI's own UI is the escape hatch for slash commands, `/login` and dialogs [R03 risks].
- **Model catalog and discovery** (03-7, 10-11, 17-7, 17-14). The chips use CLI aliases and last-used values.
- **Slash-command completion** (03-8, 05-15), **@file** (05-14), **images and appshots** (03-13, 05-13, 05-24).
- **Providers beyond claude and codex** (14-22), and the herdr-style report API (15-2).
- **Relay or E2EE off the tailnet** (18-16, 15-12, 08-22, 02-1), LAN TLS (18-17), multiple hosts (02-13), SSH hosts (10-19, 09-18).
- **Orchestration** (11-5–11-8), agent CLI and MCP (11-1, 03-16, 01-14), event triggers and CI repair (11-11, 09-17), the external Inbox (11-16).
- **Lock-screen Allow/Deny** (08-19), Live Activities (15-5), HUD (15-6), quick composer (09-6, 11-13), iPad split (08-20), Android and other OSes (D13).
- **Desktop dark theme** (14-20, 12-1), usage meters (09-5), Zeron's amber = queued (D1), and Zeron's yolo defaults [R03 risks].
- **Phone read-only terminal and key bar** (16-13, 16-18). Chat sessions cover the phone use case.
- **Splitting the `Desktop` god-struct** (14-13). New surfaces bring their own entities instead.

## 7. Risks and de-risking

| # | Risk | Evidence | De-risk in this plan |
|---|---|---|---|
| R-1 | `--permission-prompt-tool stdio` and the SDK control protocol are undocumented | [R03 risks; R10 risks] | M0 1-day probe with no-go (E5). Golden fixtures per CLI version. Version floor + `initialize` probe (03-12). Chat is disabled per provider on a failed probe, and terminal kind remains (B → A) |
| R-2 | `--permission-mode default` is unlisted in 2.1.285 (`manual` is listed) | [R17 F11] | The probe picks `default` or `manual`. Stored in the capability report |
| R-3 | Codex app-server is `[experimental]` and drifts (0.153.4 Zeron, 0.159.0 local) | [R03 risks] | `generate-json-schema` fixtures per version in CI. `-32601` for unknown requests so turns never hang. Tested-version range in `hello.ok` |
| R-4 | A private app-server and the account daemon share one `CODEX_HOME`; unverified | [R10 risks] | Spike at E7 start: two concurrent threads, check rollout files. Fallback, which needs PO sign-off because it reverses D4's split: `thread/start` on the daemon socket pocketd already dials (P packages/pocketd/internal/codex/rpc.go:162-165) |
| R-5 | Double permission ask: Pocket's or the user's hooks fire in headless Claude | [R03 risks; R10 risks] | Strip `CLAUDE_CODE_PLUGIN_DIRS`/`POCKETD_*`. The probe tests which wins. M-8 counts duplicates |
| R-6 | Same Claude conversation live twice (TUI `--resume` + chat) | [R03 risks] | Conversation lock. Parks the chat on a foreign `SessionStart`. Handoff disabled while Working |
| R-7 | Account terms for driving `claude --print` from a third-party GUI on a subscription | [R03 risks] | PO checks before sharing builds beyond personal use. Chat can be switched off per provider in settings (the same gate as R-1) |
| R-8 | Scope: B is A plus ~2 weeks, in 8 weeks, solo | §5 sizes | Parallel lanes by path (§4). Cut order in §9. The M1 exit gate decides whether M2 keeps both chat drivers |
| R-9 | Phone-started chat = remote code execution | [R18 TL;DR; R10 risks] | E2 before any spawn (D10). 18-10 limits. Full access never from the phone without a desktop setting and never persisted [R17 F14] |
| R-10 | Checkpoint race with auto-approved edits | [R10 risks] | Snapshot in the permission/PreToolUse path, before the reply. Refuse Undo when a snapshot is missing |
| R-11 | Collisions with uncommitted work on `main` (now also `workspace.rs`) | [R14 F12]; git status | Plans touching those files branch from `main`. pocketd-only lanes (E4, E5, E7) are safe |
| R-12 | No replay after a crash leaves half-done work | [R10 risks] | Mark interrupted, "Inspect the work before continuing". Never auto-retry |
| R-13 | Two surfaces drift apart (terminal vs chat semantics) | §3 matrix | The capability table is the contract. Each driver ships a conformance test against `agent.Driver` + queue semantics |
| R-14 | Headless in an untrusted folder behaves unknown | [R10 risks] | The probe covers it. Phone creates are limited to registered projects |

## 8. Success metrics (local, no telemetry)

- **Source.**
  - pocketd appends status transitions and client actions to `transitions.jsonl` (E2). E7's journal adds per-item timestamps.
  - `pocketd stats [--since 7d]` prints the metrics.
  - Nothing leaves the Mac.
- **Baseline.** Week 1 values come from a transitions-only logger shipped with E2. It is S and lives in the same file.

| ID | Metric | Computation | Target by wk 8 |
|---|---|---|---|
| M-1 | Needs you → answer latency, p50/p90 | `permission.resolve`/`question.answer` ts − Needs you ts, split by client (desktop/phone) | p50 ≤ 60 s for phone answers after E11. p90 improves ≥ 50% vs the wk-1 baseline |
| M-2 | Prompt delivery failures | `errNotForeground` + write errors / prompts, per kind | Codex terminal 0 after E4. Chat 0 |
| M-3 | Chat share (thesis test) | chat sessions / all sessions started, per week | ≥ 30% by wk 8. **Kill signal:** < 15% two weeks after E8 → freeze further chat work and finish A |
| M-4 | Redirects without Stop | Steer + Send next / (those + Stop-then-send within 30 s) | ≥ 70% |
| M-5 | Non-desktop starts | sessions created by phone or automation, per week | ≥ 5/week |
| M-6 | Undo/Keep use | Undo, Keep, refused-Undo counts | Refused-Undo with a data loss report = 0 |
| M-7 | Restore success | Idle chat agents that accept a prompt after a pocketd restart / all | 100% |
| M-8 | Driver health per CLI version | handshake failures, `-32601` sent, duplicate permission asks | Duplicate asks = 0. Every failure names the CLI version |

## 9. Cut order if behind

1. E14 automations: move past the horizon. The phone keeps J4.
2. E13 checkpoints for Claude terminal only: keep chat. The PreToolUse path waits.
3. E8 glide (17-13) and word-level polish.
4. E7 Codex chat: if R-4 fails, Codex stays terminal-only via E4. That is still a working D4 path.
5. Never cut: E1, E2, E3's wire, E5, E9's Enter-never-stops, E11 push. These are the thesis and the D10/D12 floor.
