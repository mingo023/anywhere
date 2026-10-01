# Strategy C: Control plane (pocketd runs the fleet)

- Date: 2026-09-30. Horizon: 8 weeks. M0 is weeks 1–2, M1 weeks 3–4, M2 weeks 5–6, M3 weeks 7–8.
- Inputs:
  - research reports R02, R03, R05, R08, R10, R11, R14–R19 in `docs/orchestrators/research/`;
  - PO decisions D1–D13;
  - CONTEXT.md vocabulary.
- Citations:
  - `[R14 §F12]` is a report section.
  - `[R18 idea 18-8]` is an idea row.
  - `P path:L` is Pocket code as cited and verified by the reports. They were taken at 86deb13/b9d14a1, so re-check line numbers on main.
  - "new" marks an item proposed here that has no report ID.
- Sizes (solo dev with /implement agents): S ≤2 dev-days, M 3–5, L 6–10, XL >10.
- Work runs in two lanes:
  - **P**: pocketd and `packages/protocol`.
  - **C**: phone and desktop clients.

## 1. Thesis

- **Every feature on the roadmap needs one always-on owner of state. Today nobody owns it.**
  - pocketd persists only `config.json` and its plugin (P packages/pocketd/internal/config/config.go:60) [R15 TL;DR].
  - Timelines live in memory only (P packages/pocketd/internal/timeline/timeline.go:155-167).
  - The project registry and worktree ops exist only in the desktop (P packages/desktop/crates/store/src/store.rs:8-28; P packages/desktop/crates/git/src/git.rs:141,166-176) [R14].
  - The desktop exits when pocketd is down (P packages/desktop/crates/pocket/src/main.rs:1035-1038).
  - So phone spawn, agent-driven sessions, restore, automations and push each have nowhere to live.
- **pocketd is already the right process.**
  - It owns every PTY, runs 250 ms detection and holds the broker, hub and timeline [R14].
  - It already joins Codex's account daemon as a second client (P packages/pocketd/internal/codex/rpc.go:44-68).
  - It already exposes an ops socket to every terminal (P packages/pocketd/internal/ops/ops.go:95-158).
  - What's missing is persistence, policy and a few verbs, not a new runtime.
- **C makes pocketd the control plane.**
  - Sessions come back after restarts (15-1).
  - One `agent.create {LaunchSpec}` serves all four origins: desktop, phone, agent and schedule (D5).
  - A scoped agent CLI and MCP server lets agents drive Pocket but never approve (11-1, 03-16, 18-8).
  - Automations and lead/worker runs execute in the daemon with the app closed (11-9, 11-5).
  - Seen-aware push brings the human back only when needed (D3).
- **Security is the product, not a tax.**
  - Today a paired phone is a remote shell. An agent can approve its own permissions. Any PTY can type into a sibling [R18 TL;DR].
  - A control plane that lets agents drive agents is only shippable once scopes exist. C builds them in M0, so D10 and D12 become the foundation instead of a detour.
- **It avoids the collision zone.**
  - In-flight desktop work touches `pocket/src/{changes,diff,explore,overlay,main,capture,view}.rs`, `workspace.rs`, `theme.rs`, `ui.rs`, `git.rs` and `daemon.rs` [R14 §F12, git status].
  - pocketd, protocol, phone, `forms.rs`, `termview.rs` and the `agents` crate are untouched there. About 70% of C lands on the untouched side.
- **UI is built only where a feature needs it.** The terminal stays the desktop's drive surface (D9). No chat pane, no visual cloning.

### Why this wins for the target user

| Rival | Their model (cited) | Where C wins |
|---|---|---|
| Zeron | Headless chat engine on undocumented `--permission-prompt-tool stdio`. It runs yolo: Claude `can_use_tool` is always allowed, and Codex runs `never` / `danger-full-access` [R03 TL;DR]. "Every signed-in device is fully trusted", with no E2EE [R18 TL;DR] | Real TUIs plus human approvals for every tool. Per-device scopes. No undocumented CLI surface (§8) |
| MonoCode | The control plane lives in the app. The scheduler and inbox poll "run in the webview, so the app must be open", and events while it's closed never fire [R11 TL;DR]. Worker approvals go to the lead, "never to the user". The write-scope check runs after the fact [R11 TL;DR] | Daemon automations that run with the app quit (E11). Approvals always reach the human. A preventive `PreToolUse` deny (11-7) |
| Superset | iPhone needs Pro at $20/user/mo. Traffic goes through a vendor relay with no documented E2EE. Automations are Pro. ELv2 license [R15 §F1] | Free, self-hosted, tailnet-only, no relay |
| Happy | `happy claude` wrapper. Push via Happy's server. Vendor server with E2EE [R15 §F2] | No wrapper and no server beyond Expo's push hop, which carries fixed copy only (E9). Spawn from the phone into worktrees, schedules, orchestration |
| Claude Remote Control / ChatGPT "Control this Mac" | Single provider, vendor relay, Pro+/plan. "Another connection took over this session". Gives up after about 10 min offline [R15 §F5, §F6] | One fleet across claude and codex. Desktop and phone both attached. Survives restarts (E5). Vendors can't offer the other vendor's agent [R15 risks: "vendor squeeze"] |

### What C trades away (honest)

- **No GUI chat on the desktop.** A user who wants Zeron's or MonoCode's transcript UI won't get it (R05, R10 idea 10-12 deferred).
- **Claude stays a TUI.**
  - No live Claude deltas on the phone; the transcript tail updates every 100 ms per item [R03 §timeline].
  - Structured AskUserQuestion stays TUI-side (14-11 deferred).
- **Reach needs Tailscale.** Off the tailnet there's nothing: no relay, unlike Happy or Superset (18-16 deferred).
- **Codex workers get no preventive write scope.** There's no hook, so detection happens at integration only (E12).
- **Push depends on Expo and a paid Apple Developer account** [R19 idea 19-12].

## 2. Target user and jobs

- **User:** the owner.
  - Solo developer on macOS with an iPhone (D13).
  - Runs 3–8 claude and codex agents in parallel across worktrees of 2–5 projects.
  - On Tailscale. Often away from the desk. Builds Pocket with agents.
- **Secondary:** developers with the same profile building from source [R19 idea 19-15].

| # | Job to be done | Gap today | Epics |
|---|---|---|---|
| J1 | When an agent needs me and I'm away, notice and answer within minutes without exposing my Mac | No push [R14]. pocketd binds `:4517` on every interface (P packages/pocketd/cmd/pocketd/serve.go:46). The phone shows only Stop while Working (P packages/app/src/components/Composer.tsx:28-55) | E1, E2, E3, E8, E9 |
| J2 | When I think of a task away from the desk, start claude or codex on the right project in a fresh worktree from the phone | `agent.create` is rejected (P packages/pocketd/internal/proto/golden_test.go:102). Worktrees are desktop-only | E6, E7 |
| J3 | When pocketd or the Mac restarts, get every agent back on its Conversation | Everything is in memory. The desktop exits | E4, E5 |
| J4 | Let one agent fan work out to others and bring results back, approving risky steps myself | Ops socket is ungated (P packages/pocketd/internal/ops/ops.go:95-158). No orchestration | E3, E10, E12 |
| J5 | Recurring chores (dependency audit, test triage, PR review) run with the app closed | None | E11 |

## 3. Control-plane model

### Principals and scopes

Scopes are observe / drive / approve / spawn / files / owner, as in [R18 R6]. C adds notify and orchestrate as grant-only scopes.

| Principal | Identified by | Scopes |
|---|---|---|
| Desktop | Unix socket peer, not a descendant of any pocketd PTY (`LOCAL_PEERPID` ancestry) [R18 idea 18-6] | owner (all) |
| Owner CLI (`pocketd pair`, `devices`, `grant`, `automation`, `config`) | Same peer check; refused from inside Pocket terminals | owner |
| Phone device | sha256-hashed device token over WS [R18 idea 18-3] | observe, drive, approve, spawn. Spawn is limited by D5 policy (E7) |
| Observe-only device | Device token | observe |
| Agent with a grant | PTY-descendant peer plus `POCKETD_GRANT` matching its terminal | A subset of observe, drive, spawn, notify, orchestrate. **Never** approve, owner, pair, devices, grant, automation or config [R18 idea 18-8] |
| Any PTY descendant without a grant | PTY-descendant peer | Hook events for its own terminal only (hook PID must descend from that terminal) |
| Automation | Internal | spawn via LaunchSpec |

### Invariants

Every epic's tests must hold these.

1. `approve` is held only by owner and off-Mac device tokens. That alone ends self-approval [R18 §permission.resolve].
2. Grants are fixed at launch by the owner (LaunchSpec `grants`, `pocketd grant`). Grants are never transitive: sessions an agent starts get no grants. The orchestrator's workers get none either.
3. pocketd never persists env, raw argv or conversation text. Timelines are rebuilt from provider files (E5) [R15 risks].
4. Every action that can fail returns a coded error. Nothing fails silently (today `agent_op` falls back to a shell prompt, P packages/desktop/crates/daemon/src/daemon.rs:98-112).
5. Agent-sourced prompts are sanitized and never start with `!` or `/` [R18 idea 18-9].

### State under `~/.coding-pocket/`

All files are 0600 and written atomically (tmp + rename).

| File | Epic | Content |
|---|---|---|
| `devices.json` | E2 | id, name, platform, scopes, sha256(token), createdAt, lastSeenAt, lastAddr, expoPushToken?, pushPrefs |
| `state/terminals.json` | E5 | terminalId, launchDir, title, provider?, conversationId?, transcriptPath?, launch {access, plan, model?, effort?}, grantScopes? |
| `state/ports.json` | E6 | worktree path → POCKET_PORT block base |
| `automations.json` | E11 | definitions, nextRunAt, history ≤100 |
| `runs/<runId>.json` | E12 | proposal, tasks, worktrees, status, results |
| `events.jsonl` | E4 | local metrics log (§9). Rotates at 10 MiB, keeps 3 |
| `logs/` | E4 | pocketd log, rotated |

### Protocol additions

All additions are additive and gated by `hello.caps` (E4).

| Message / field | Epic | Cap |
|---|---|---|
| `pair {code, name, platform}` → `pair.ok {deviceId, token}` (pre-auth) | E2 | `pair.v1` |
| `hello.caps[]` / `hello.ok.caps[]` | E4 | — |
| `agent.timeline.beforeSeq` | E5 | `timeline.before.v1` |
| `project.list` → projects with worktrees `{name, path, branch, isMain}` | E6 | `registry.v1` |
| `agent.create {requestId, spec: LaunchSpec}` → `agent.created {requestId, agentId, terminalId}` or `error {requestId, code, message}`; `agent.providers` | E7 | `launch.v1` |
| `agent.prompt` / `agent.interrupt` + `requestId` → `ack {requestId, result: delivered, queued or error, code?}` | E8 | `prompt.ack.v1` |
| AgentSummary gains project, worktree, branch, tokensUsed, contextWindow?, origin | E8 | `summary.v2` |
| `push.register {expoToken, prefs}` / `push.unregister` | E9 | `push.v1` |
| `automation.list` / `automation.run` / `automation.pause` | E11 | `automation.v1` |

## 4. Milestones

| Milestone | Weeks | Lane P | Lane C | Exit criteria |
|---|---|---|---|---|
| **M0 fix-now** | 1–2 | E2, E3 | E1, then the E2 phone half | D12 bugs closed (E1 + E3). D10 prerequisites in place (E2 + E3). Owner confirms paid ADP, EAS project and APNs key for E9 |
| **M1 always-on** | 3–4 | E4 → E6 → E7 → E5 | E7 phone sheet, E4 desktop reconnect | `launchctl kickstart -k` of pocketd brings back every agent on its Conversation. The phone creates a session in a new worktree |
| **M2 reach & drive** | 5–6 | E8, E10 | E9 phone half, E13 | Lock-screen push → tap → answer. A granted agent starts and drives a sibling |
| **M3 autonomy** | 7–8 | E11, E12 | E11 phone list | A daily automation runs with the app quit. One lead/worker run is confirmed on the phone and integrated |

- **Cut order if behind:**
  1. E13 selection
  2. E12 integrate and auto-wake
  3. E11 templates and `gh` triggers
  4. E10 MCP half
  5. E12 entirely

  Never cut M0.

## 5. Epics

### E1 `fix-now` (M0, S)

- **Goal:** close three of D12's bugs. The fourth, agent self-approval, ships in E3 in the same milestone.
- **Ideas:** 17-1, 17-2, 16-3, 05-2 (the Send-while-busy subset of 14-17).
- **Depends:** none. Branch from main at 5bc8ea8 or later (14-24). None of these files are in main's uncommitted set.
- **Scope:**
  - Desktop `forms.rs:408-414`: emit the F14 argv on both axes for every access level [R17 §F14].
    - Codex Auto-accept edits: `-s workspace-write -a on-request`, replacing `--full-auto`, which codex 0.159.0 rejects (P packages/desktop/crates/pocket/src/forms.rs:412).
    - Claude Ask: `--permission-mode default`.
    - Codex Ask: `-s read-only -a on-request`.
    - Full access: claude `--permission-mode bypassPermissions --allow-dangerously-skip-permissions`; codex `-s danger-full-access -a never`.
    - Plan first: claude `--permission-mode plan`. Disabled for Codex, with the hint "Codex plan needs a chat session".
  - Desktop `termview.rs`: font features `liga`, `calt` and `dlig` set to 0 [R16 idea 16-3]. First confirm the bug by typing `a --b`.
  - Phone `Composer.tsx:28-55`: the button is Send when the input is non-empty and Stop only when the input is empty and the agent is Working. Return always sends and never stops [R05 idea 05-2; Zeron #406].
- **Done when:**
  - Codex Auto-accept edits starts without a CLI error.
  - Every desktop spawn carries both access axes.
  - `a --b` renders with its real spacing.
  - On the phone, typing while Working shows Send, and tapping it delivers the prompt.
- **Tests:**
  - `cargo test -p pocket`: an argv table test covering every provider × access × plan.
  - `pnpm --filter @pocket/app typecheck && pnpm --filter @pocket/app test`: a Composer state test.

### E2 `reach-lockdown` (M0, L)

- **Goal:** covers D10 items 1–3.
  - Only loopback and the tailnet can reach pocketd.
  - Each paired device holds its own hashed, revocable token.
  - Pairing uses a QR with a one-time code.
- **Ideas:** 18-1, 18-2, 18-3, 18-4, 18-14, 18-15, 18-18, 19-11. These supersede 08-18.
- **Depends:** none.
- **Scope:**
  - **Bind:** 127.0.0.1 plus the Tailscale interface IPs, re-checked every 30 s. Loopback only when there's no Tailscale. Refuse `--listen lan` without TLS (P packages/pocketd/cmd/pocketd/serve.go:46) [R18 idea 18-1].
  - **Handshake:** drop `InsecureSkipVerify` (P packages/pocketd/internal/wsserver/wsserver.go:53) [R18 idea 18-2]. Also:
    - a Host allowlist;
    - a 4 KiB pre-auth read limit;
    - at most 16 pre-auth sockets;
    - 3 auth failures per minute per address;
    - distinct error reasons.
  - **New package `internal/peer`:** the `LOCAL_PEERPID` check, plus ancestry against the PIDs of live pocketd PTYs. E3 reuses it.
  - **`devices.json`:** 32-byte base64url tokens, shown once and stored as sha256 [R18 idea 18-3].
    - Revoke closes the device's sockets with 4401.
    - `pocketd devices list|rename|revoke` is owner-only.
    - The legacy `config.json` token becomes a "Legacy device" for 7 days with no spawn scope, then is refused (§10 decision).
  - **`pocketd pair`** (owner-only) prints a terminal QR of `codingpocket://pair?host=<magicdns-or-ts-ip>:4517&code=<128-bit>` [R18 idea 18-4].
    - The code has a 5 min TTL, is single-use, and locks after 5 failures.
    - The pre-auth `pair` message returns the device token. The QR never carries a long-lived token.
  - **Phone:**
    - system Camera deep link `codingpocket://pair` [R19 idea 19-11];
    - SecureStore `WHEN_UNLOCKED_THIS_DEVICE_ONLY` [R18 idea 18-14];
    - a per-install clientId replacing the hardcoded `"pocket-app"` (P packages/app/src/session.tsx:76);
    - the copy "A paired phone can run commands on this Mac as you." [R18 idea 18-18].
  - **Optional `pocketd serve --tailscale-serve`:** loopback-only bind, with the MagicDNS host in the QR [R18 idea 18-15].
- **Done when:**
  - `lsof -iTCP:4517 -sTCP:LISTEN` shows no wildcard listener.
  - A bad token is rejected before 4 KiB is read.
  - Revoke drops the phone within 1 s.
  - A reused QR code is rejected.
  - `pocketd pair` run inside a Pocket terminal is refused with "Run from Terminal.app".
- **Tests:**
  - go: bind selection table (with and without a Tailscale interface), handshake limits, devices store, pair TTL, single use and lockout, peer ancestry with a fake process table.
  - `go test ./internal/proto -update` for `pair` goldens, then `pnpm --filter @pocket/protocol test`.
  - App test for deep-link parsing.

### E3 `agent-grants` (M0, L)

- **Goal:** covers D12 bug 4 and D10 items 4–5.
  - No process inside a Pocket terminal can approve, pair, or drive another terminal unless the owner granted it.
  - The desktop stops holding a phone-grade secret.
- **Ideas:** 18-6, 18-7, 18-8, 18-9, 11-2.
- **Depends:** E2 (device tokens, `internal/peer`).
- **Scope:**
  - **Scope enforcement in WS dispatch** (P packages/pocketd/internal/wsserver/wsserver.go:156-200) [R18 idea 18-7]. `permission.resolve` requires approve (today any socket can call it, wsserver.go:156-161).
  - **Desktop:** the agent protocol is served on the unix socket and the desktop dials it [R18 idea 18-6].
    - Owner scope comes from the peer check.
    - Remove the desktop's `config.json` token read (P packages/desktop/crates/agents/src/agents.rs:198-200,248).
    - Only the agents crate changes, so there's no collision.
  - **Ops socket:** classify every peer with `internal/peer`.
    - PTY descendants without a grant may only send hook events for their own terminal, and the hook PID must descend from the terminal named in `POCKETD_PTY` (today hooks are forgeable, P packages/pocketd/cmd/pocketd/hook.go:14-37).
    - `spawn`, `input`, `screen` and `close` (P packages/pocketd/internal/ops/ops.go:95-158) are refused with an error that names the fix.
  - **Grants:** a server-side per-terminal record {scopes, sha256(token)}.
    - The token goes into the PTY env as `POCKETD_GRANT` at spawn, and only when granted.
    - Issuers are LaunchSpec `grants` (E7) and `pocketd grant <terminal> <scopes>`, both owner-only.
    - A grant can't contain approve, owner, pair, devices, grant, automation or config [R18 idea 18-8].
  - **Sanitation** on every prompt and input path: strip C0 except `\n` and `\t`, plus ESC and DEL; cap at 64 KiB. Prompts sourced from agents or ops are refused if they start with `!` or `/` (P packages/pocketd/internal/terminal/terminal.go:278-287) [R18 idea 18-9].
- **Done when:** a red-team script run inside a Pocket terminal proves all of the following.
  - `cat ~/.coding-pocket/config.json` reveals no token.
  - `permission.resolve` with anything it can read is refused.
  - Ops `input` to a sibling is refused.
  - A forged hook for another terminal is refused.
  - The desktop still lists, views and approves.
- **Tests:**
  - go: ancestry classification, scope matrix per message, sanitation table.
  - e2e with fakeclaude: a PTY child attempts resolve and input and both are refused.
  - `cargo test -p agents` for the socket transport.

### E4 `always-on` (M1, M)

- **Goal:**
  - pocketd runs under launchd without the desktop.
  - It sees the same CLIs as the login shell.
  - It keeps the Mac awake while work runs.
  - The desktop survives its restarts.
  - The local metrics log exists before push lands, so there's a baseline.
- **Ideas:** 02-7, 02-8, 19-4, 19-3, 10-6, 15-7, 02-9 (with 19-10).
- **Depends:** E3 (desktop over the unix socket).
- **Scope:**
  - **`pocketd daemon install|uninstall`:** LaunchAgent with `KeepAlive{SuccessfulExit=false}` and `ThrottleInterval 30`; `bootout` to stop [R02 idea 02-7]. SMAppService and the app bundle (19-1, 19-2) come later.
  - **Service basics:** `pocketd --version`, `pocketd status` (connections, devices online, agents, keep-awake, last pushes), a single-instance flock, and a rotating log in `logs/` [R19 idea 19-4; R02 idea 02-8].
  - **Login env:** capture it at start with `$SHELL -lic printenv` (5 s timeout) and use it for LookPath (P packages/pocketd/internal/terminal/terminal.go:87-101) [R10 idea 10-6; R17 risks: launchd PATH]. Held in memory only.
  - **Keep-awake:** an IOPM `PreventUserIdleSystemSleep` assertion while any agent is Working or a device is connected [R15 idea 15-7].
  - **`hello.caps`:** a capability list replaces the fatal version check for additive features [R02 idea 02-9].
  - **Metrics:** `events.jsonl` plus `pocketd stats [--since 7d]` (§9).
  - **Desktop:** replace the `exit(1)` (P packages/desktop/crates/pocket/src/main.rs:1035-1038) with "Starting Pocket's terminal service…" and reconnect with backoff [R19 idea 19-3]. Land this after the in-flight `main.rs` work.
- **Done when:**
  - `launchctl kickstart -k gui/$UID/<label>` restarts pocketd and the desktop reconnects on its own.
  - A launchd-started pocketd resolves `claude` and `codex`.
  - `pmset -g assertions` shows Pocket's assertion while an agent is Working.
- **Tests:**
  - go: plist rendering, env parsing, caps golden (an old client without caps still connects), stats aggregation over a fixture log.

### E5 `restore` (M1, L)

- **Goal:** after a pocketd, desktop or Mac restart:
  - every Terminal reappears in its worktree;
  - every agent that had a Conversation resumes on it.
- **Ideas:** 15-1; 14-4 (the re-derive variant); 14-5; 10-5; 10-7 (the interrupted marker only).
- **Depends:** E4; E7 (argv builder).
- **Scope:**
  - **Record:** write `state/terminals.json` on every terminal, agent and conversation change. It holds a structured record only, never env or raw argv [R15 risks]. The provider session id is already tracked (P packages/pocketd/internal/agent/agent.go:170).
  - **Recreate on start:** each terminal is recreated with the **same id** in its launchDir. The desktop's `Workspace::sync` then re-adopts it with no desktop change (P packages/desktop/crates/workspace/src/workspace.rs:20-30).
  - **Resume agents:** agent records get resume argv from the E7 builder.
    - Commands: `claude --resume <id>` or `codex resume <id>`, plus the recorded access flags.
    - Full access is never restored as Full; it falls back to Ask [R17 §F14 rules].
    - Validated with herdr's rules: plain PATH command, no `'` or control characters, at most 64 args and 8 KiB [R15 idea 15-1].
    - Controlled by the config toggle `resume_agents_on_restore` (default on).
  - **Timelines are rebuilt, not journaled:**
    - Claude re-tails `transcriptPath` from offset 0 (P packages/pocketd/internal/claude/transcript.go:57-89).
    - Codex replays through `thread/resume` (P packages/pocketd/internal/codex/session.go:72-95).
    - No new on-disk copy of conversation text. herdr keeps screen history off because it "can include secrets" [R15 §F4].
  - **`agent.timeline.beforeSeq`:** backward paging plus a per-agent memory cap [R14 idea 14-5].
  - **Interrupted turn:** a turn in flight at shutdown adds the timeline item "Restored after restart. The last turn was interrupted; inspect its work before continuing." [R10 §7]. Status becomes Idle, not Done.
  - **Child hygiene:**
    - each terminal gets its own process group;
    - close sends SIGTERM to the group, then SIGKILL after 2 s (today only the child is killed, P packages/pocketd/internal/terminal/terminal.go:310-312);
    - marker env `POCKETD_PARENT=<pid>`;
    - a startup reaper kills orphans whose marker parent is dead [R10 idea 10-5].
  - **Not restored:** screen contents and scrollback.
- **Done when:**
  - Start 3 claude and 2 codex agents across 2 worktrees, then `launchctl kickstart -k`.
  - All 5 are Attached on the same Conversation within 30 s.
  - The desktop tabs show them.
  - Phone timelines show their history.
- **Tests:**
  - go: state round-trip, herdr validation table, argv rebuild, reaper with a fake process table.
  - e2e: fakeclaude and fakecodex honour `--resume` and `resume`, and the test asserts the conversation id.

### E6 `registry-worktrees` (M1, M)

- **Goal:** pocketd knows the projects and can create, list and remove worktrees. Phone, agents and automations can then start sessions without the desktop.
- **Ideas:** 11-4, 15-9, 15-10, R18 R7 (registry).
- **Depends:** E3.
- **Scope:**
  - **Registry:** pocketd reads projects from `desktop.json` (P packages/desktop/crates/store/src/store.rs:8-28) and reloads it when the mtime changes.
    - The desktop stays the only writer this horizon (§10 decision), so there are no desktop edits.
    - "Registered" means a project the user kept. Auto-added folders are excluded (CONTEXT Project).
  - **`internal/worktree`:**
    - list via `git worktree list --porcelain`;
    - create a new branch with the same name, from the main worktree's HEAD, using the desktop's name rules (P packages/desktop/crates/git/src/git.rs:141,166-176);
    - remove closes its terminals, removes the folder and **keeps the branch** (D8). Removing the main worktree is refused.
  - **Copy list on create:** the desktop.json per-repo list plus `.worktreeinclude`. Skip symlinks and never overwrite; today `std::fs::copy` overwrites [R15 idea 15-9] (P packages/desktop/crates/pocket/src/forms.rs:283-291,426-436).
  - **Port env:** `POCKET_PORT` (a block of 10, stored in `state/ports.json`) and `POCKET_ROOT_PATH` in every PTY env of that worktree [R15 idea 15-10].
  - **Interfaces:** WS `project.list` (observe) and owner CLI `pocketd worktree list|create|remove`.
  - The desktop's own worktree path in `git.rs` stays until the collision clears. New sessions go through pocketd from E7 on.
- **Done when:**
  - `pocketd worktree create <project> feat-x` creates the folder and branch and copies `.env`.
  - `remove` keeps the branch.
  - The phone's `project.list` shows the new worktree within 2 s.
- **Tests:** go tests against a temp git repo covering create, list, remove, copy rules, symlink skip and port allocation.

### E7 `launchspec-create` (M1, M)

- **Goal:** one `agent.create {LaunchSpec}` starts a session from any client.
  - Covers the desktop, the phone, agents (E10), automations (E11) and runs (E12).
  - pocketd builds the argv.
  - Failures come back as coded errors instead of a shell prompt.
- **Ideas:** 17-10, 17-2 (the table moves to pocketd), 17-8, 17-9, 17-12, 18-10, 14-19, 11-3 (subset), 11-15, 02-5.
- **Depends:** E3, E4, E6.
- **Scope:**
  - **Protocol** (`packages/protocol` + `pocketd/internal/proto`): `LaunchSpec {project, checkout: {worktree} | {newWorktree}, provider: "claude"|"codex", model?, effort?, access: "ask"|"acceptEdits"|"auto"|"full", plan: bool, prompt?, title?, grants?[]}` (D5).
    - Error codes: `unknown_project`, `unknown_worktree`, `worktree_exists`, `provider_unavailable`, `access_not_allowed`, `spawn_failed`, `duplicate`.
    - The golden at P packages/pocketd/internal/proto/golden_test.go:102 flips from rejected to accepted.
  - **`internal/launch` argv builder:**
    - the F14 table on both axes [R17 §F14];
    - Codex with `plan` returns `access_not_allowed`;
    - `claude -n <title|worktree>` [R17 idea 17-12];
    - the prompt is the last positional argument after E3 sanitation, not typed;
    - Codex `effort` is accepted only once `-c model_reasoning_effort=…` is verified on the installed CLI; otherwise `access_not_allowed` [R17 risks].
  - **Wrapper:** keep today's hand-back: run the argv, then `exec $SHELL -l` (P packages/desktop/crates/daemon/src/daemon.rs:98-112).
    - It must be shell-agnostic; the owner's login shell is fish.
    - It reports the exit status to pocketd as a hook for its own terminal.
    - A non-zero exit before Attached returns `spawn_failed` with the last 20 screen lines [R17 idea 17-9].
  - **Idempotency:** `(principal, requestId)` receipts kept for 10 min. A replay returns the same `agent.created`; a changed payload returns `duplicate` [R02 idea 02-5; R11 idea 11-15].
  - **`agent.providers`:** `command -v` in the captured login env with a 30 s TTL, plus a version floor → `[{provider, version, available}]` [R17 idea 17-8; R03 idea 03-12].
  - **Phone policy** (D5, [R18 idea 18-10]):
    - provider claude or codex;
    - a registered project only;
    - no cmd, env or cwd fields, by construction;
    - `access` at most `phone.maxAccess` (default `ask`; `plan` is allowed);
    - `grants` must be empty.
    - The owner raises the limit with `pocketd config set phone.maxAccess acceptEdits|auto`. Full access is never allowed from the phone.
  - **Desktop:** `forms.rs:402-459` sends `agent.create` over the unix socket instead of building argv. Errors show in the existing form.
  - **Phone:** a minimal create sheet off AgentsScreen (P packages/app/src/screens/AgentsScreen.tsx:24).
    - Fields: project, checkout (an existing worktree or a new name), provider, access (Ask / Plan first unless raised), multi-line prompt.
    - No model or effort pickers.
  - An empty prompt is allowed; the agent starts Idle (§10 decision).
- **Done when:**
  - The phone creates a claude session in a new worktree, and the desktop shows it in that worktree's tabs.
  - A bogus model returns `spawn_failed` with the screen tail on both clients.
  - A double tap creates exactly one session.
- **Tests:**
  - go: argv table for every provider × access × plan, the policy matrix, receipts.
  - `go test ./internal/proto -update` and `pnpm --filter @pocket/protocol test`.
  - e2e: fakeclaude asserts the argv.
  - App typecheck and test.

### E8 `reliable-drive` (M2, M)

- **Goal:**
  - Prompts, steers and interrupts reach agents, and every send is acknowledged.
  - Codex stops depending on keystrokes (D4).
  - Summaries carry what the CLI, the phone and the context indicator need (D7).
- **Ideas:** 03-1, 03-5 (steer only), 05-9, 02-5, 14-21, 03-6.
- **Depends:** E3, E4.
- **Scope:**
  - **`codex.Session`:** add `turn/start {threadId, input}`, `turn/steer {threadId, expectedTurnId, input}` and `turn/interrupt {threadId, turnId}` [R03 idea 03-1].
    - Track turnId from `turn/started`.
    - Send no policy params, so the thread's (TUI's) policy applies. Verify this first; it's R03's prerequisite.
    - The spike showed `turn/start` from pocketd appears in the TUI (P docs/spike-remote-sessions.md:75).
  - **New `codexDriver`** implementing `agent.Driver` (P packages/pocketd/internal/agent/agent.go:16-21):
    - Prompt steers when a turn is active and starts one otherwise.
    - A rejected steer is queued until `turn/completed`.
    - Any RPC error falls back to `termDriver` typing.
    - "Always allow" replies `acceptForSession` [R03 §codex].
  - **Claude keeps `termDriver`.** Its TUI queues text typed mid-turn [R05 TL;DR].
    - Never type into an agent that isn't Attached. This avoids answering the folder-trust dialog, since Claude skips hooks in untrusted folders (P packages/pocketd/internal/daemon/presence.go:53-55) [R10 open questions].
  - **Acks:** `agent.prompt` and `agent.interrupt` gain `requestId`. The `ack` result is `delivered`, `queued` or `error` with code `not_foreground`, `not_attached` or `rejected` (today this fails silently at P packages/pocketd/internal/daemon/presence.go:119). Dedupe by (principal, requestId) [R02 idea 02-5].
  - **Phone delivery state:** an optimistic bubble, then "Not delivered · Tap to retry" on error [R05 idea 05-9].
  - **AgentSummary** (P packages/pocketd/internal/proto/proto.go:168-184) gains project, worktree, branch, tokensUsed, contextWindow and origin [R14 idea 14-21].
    - contextWindow is sent only when the agent reports it, for example Codex `thread/tokenUsage/updated` [R10 §4]. Otherwise it's absent.
    - origin is one of `desktop`, `phone`, `agent:<id>`, `automation:<id>`, `run:<id>`.
  - **Context indicator (D7):** warn at 75%, danger at 90%, hidden when the window is unknown.
    - Desktop: replace the hardcoded 200k (P packages/desktop/crates/agents/src/agents.rs:56-57).
    - Phone: a chip in the ChatScreen header.
- **Done when:**
  - A phone prompt to a codex agent whose TUI shows a picker is delivered via `turn/start` and visible in the TUI.
  - A prompt to a claude agent sitting at a shell prompt shows `not_foreground` on the phone.
- **Tests:**
  - fakecodex e2e for start, steer, steer rejection with queueing, interrupt, and the fallback.
  - Golden for `ack`.
  - App test for retry state.

### E9 `push-attention` (M2, L)

- **Goal:** when a session the user isn't looking at becomes Needs you, Done or Done·failed, the iPhone gets one push per session. Tapping it opens that session (D3).
- **Ideas:** 19-12, 08-1, 08-2, 08-3, 08-4, 08-5, 18-13.
- **Depends:** E2, E4.
- **Owner prerequisites (check in M0):** paid Apple Developer Program, an EAS project id, the APNs key uploaded to Expo, and a dev or TestFlight build with push [R19 idea 19-12, 19-14].
- **Scope:**
  - **pocketd `internal/notify` transition detector.** Input is status plus Seen (P packages/pocketd/internal/agent/agent.go:133).
    - It emits on entering Needs you (once per requestId), Done, or Done·failed, and only if the session is not Seen at that moment.
    - The first sight of an agent sets a baseline. Working→Idle is silent. An interrupt is not a failure.
    - There is no 45 s staleness rule (D3) [R08 ideas 08-1, 08-4].
  - **Registration:** `push.register {expoToken, prefs {needsYou, done, failed}}` is stored on the device record and resent on every `hello.ok`. Revoking the device drops the token [R08 idea 08-2].
  - **Sender:** POST to the Expo Push API.
    - `collapseId` and thread id are the agentId, one per session (D3).
    - Title is the project name. Body is fixed: "Needs you", "Done" or "Failed" (§10 decision).
    - `data {agentId, requestId}`, where requestId is random.
    - No command, path or prompt text [R18 idea 18-13].
    - Poll receipts and prune `DeviceNotRegistered` [R08 idea 08-5].
  - **Phone:**
    - A tap opens that agent's ChatScreen, pending until connected. No banner while the app is in the foreground [R08 idea 08-3].
    - Ask for notification permission after the first successful pairing.
  - **`pocketd push status`:** devices and the last 30 deliveries with results [R08 idea 08-5].
  - **Desktop banners stay as they are** (P packages/desktop/crates/pocket/src/main.rs:314-328). Converging them on this detector is later work (§8).
- **Done when:**
  - With the phone locked, a claude permission prompt on an unseen session pushes "Needs you" within 5 s of the hook.
  - A session Seen on the desktop at the transition produces no push.
  - Two Done in a row on one session leave one lock-screen entry.
- **Tests:**
  - go: detector table (transition × Seen × baseline × interrupt), the sender against a fake HTTP server, receipt pruning.
  - App test for tap routing.

### E10 `agent-cli-mcp` (M2, L)

- **Goal:** an agent granted Pocket control can do the following through `pocketd agent …` or an MCP server, and can never approve:
  - list, read, start, prompt and wait on sessions;
  - create worktrees;
  - push a notification.
- **Ideas:** 11-1, 11-15, 03-16, 15-3 (the `notify` verb only), 15-4 (agent-requested push, without the presence rule), R18 R9 ("Driven by" label).
- **Depends:** E3, E6, E7, E8; E9 for `--push`.
- **Scope:**
  - **CLI:** `pocketd agent <verb> [--json] [--request-id ID] [--input -]`, with self-describing `--help` and errors that name the fix [R11 idea 11-1]:

    | Verb | Scope | Notes |
    |---|---|---|
    | `sessions list` | observe | AgentSummary v2 |
    | `sessions read <id> [--since seq] [--limit ≤200]` | observe | Assistant text and tool summaries. Each item is truncated to 4,000 chars [R11 TL;DR] |
    | `sessions wait <id> --until needs_you\|done\|idle [--timeout ≤25s]` | observe | Re-callable, like MonoCode's `wait` 0–25 s [R11] |
    | `sessions send <id> <text>` / `sessions interrupt <id>` | drive | Agent-sourced: sanitized, with no leading `!` or `/`. Returns the E8 ack |
    | `sessions start --input -` (LaunchSpec) | spawn | `grants` must be empty. `access` is at most the caller's own |
    | `worktrees list` / `worktrees create` | observe / spawn | E6 |
    | `notify <title> [--body] [--push]` | notify | A timeline marker, plus a phone push with fixed copy when `--push` |

  - **Never exposed:** permission, pair, devices, grant, automation or config verbs [R18 idea 18-8].
  - **Limits:** 3 s I/O, 256 KiB line, at most 24 pending per grant, receipts capped at 512 per grant [R11; R18 R9].
  - **Origin:** agent prompts carry `origin: agent:<id>`. The phone labels them "From <session title>". The desktop is unchanged.
  - **MCP:** `pocketd mcp`, a stdio server.
    - Tools: `list_sessions`, `read_session`, `start_session`, `send_message`, `interrupt_session`, `wait_for_status`, `create_worktree`, `notify`.
    - This mirrors `zeron mcp` without `respond_to_input` [R03 idea 03-16].
    - Auth is the inherited `POCKETD_GRANT`.
    - Claude registration: declare the server in Pocket's plugin dir (P packages/pocketd/internal/daemon/plugin.go). Verify plugin MCP support; if it's missing, document `claude mcp add`.
    - Codex: document the config snippet; don't auto-install.
  - **Grant UX:**
    - The desktop New Session form gets one "Pocket control" row, off by default. It sets `grants = [observe, drive, spawn, notify]`.
    - `pocketd grant` covers everything else.
    - No new key binding (D6).
- **Done when:**
  - A granted claude session is told "start codex in a new worktree fix-lint, have it run the linter, wait until it's done, summarize". It completes end to end; the only human input is the child's own permission prompts.
  - The same request from an ungranted session fails with an error naming the fix.
- **Tests:**
  - go: per-verb scope matrix, receipts, limits.
  - MCP JSON-RPC golden for `initialize`, `tools/list` and `tools/call`.
  - e2e: fakeclaude invokes the CLI.

### E11 `automations` (M3, M)

- **Goal:** scheduled agent jobs run from pocketd with the desktop and phone closed. Each run is an ordinary session.
- **Ideas:** 11-9, 11-10, 11-14 (subset), 11-11 (stretch).
- **Depends:** E5, E6, E7; E9 for push on Done or Failed through the normal detector.
- **Scope:**
  - **`automations.json`:** `{id, name, enabled, schedule {kind: hourly|daily|weekdays|weekly, time, weekday?}, spec: LaunchSpec (prompt required), policy: freshWorktree|reuseIdle, nextRunAt, history[≤100]}`. Times are local.
  - **Scheduler:**
    - a 30 s tick;
    - claim by compare-and-set on `nextRunAt` before launching;
    - a missed run within the 720-min grace fires once, on wake or restart;
    - an older miss is recorded as `missed` [R11 idea 11-9].
  - **Launch policy:**
    - Default is a fresh worktree `auto-<slug>-<yyyymmdd-hhmm>`.
    - `reuseIdle` picks this automation's last session if it's Idle with the same provider and checkout [R11 idea 11-10].
    - Worktrees are kept (D8).
  - **Access** defaults to `acceptEdits` at most. Full access is refused.
  - **Management:**
    - Owner CLI: `pocketd automation add --file|list|pause|resume|run|rm|history`.
    - Phone: `automation.list`, `automation.run` and `automation.pause` (spawn scope), on a list screen with Run now and Pause.
    - No desktop editor.
    - No agent verbs, so an injected agent can't plant a recurring job.
  - **Templates:** 3 prompts ported from MonoCode's 14: dependency audit, failing-test triage, PR review [R11 idea 11-14]. Attribution per [R19 idea 19-16].
  - **Stretch:** `gh` polling for new PRs, with a persistent seen set [R11 idea 11-11].
- **Done when:**
  - A daily automation fires with the desktop quit.
  - If the Mac sleeps through the slot, it fires once on wake.
  - A pocketd restart during a tick causes no double run.
  - The run appears as a session and pushes Done.
- **Tests:**
  - go: next-run computation across DST and weekdays, CAS claim, grace, history cap.
  - e2e with fakeclaude.

### E12 `orchestrator` (M3, XL)

- **Goal:** a Lead agent splits a goal into scoped tasks. After the human confirms:
  - pocketd runs 1–4 Workers in isolated worktrees;
  - it reports their results to the Lead;
  - it integrates the reviewed changes.
  - Approvals stay with the human.
- **Ideas:** 11-5, 11-6, 11-7, 11-8, 11-18.
- **Depends:** E6, E7, E8, E9, E10.
- **Scope:**
  - **Terms:** add Run, Lead, Worker, Assignment and Automation to CONTEXT.md before any code. "Inbox" keeps its current meaning [R11 idea 11-18].
  - **Grant:** `orchestrate` via LaunchSpec `grants`. It's owner-set, as a second option on the "Pocket control" row.
  - **`pocketd agent run propose --input -`:** `{goal, tasks[≤12] {title ≤160, prompt ≤30,000, scope[] (required), provider, access ≤ acceptEdits}, concurrency 1–4 (default 2)}`. MonoCode allows 40 tasks [R11 idea 11-5]; v1 caps at 12.
  - **Human gate:** pocketd raises a broker request on the Lead session, which shows as Needs you.
    - Existing permission UI renders it on desktop and phone: tool "Pocket run", detail = the task table, options Start / Cancel, plus feedback.
    - Answering needs approve. E9 pushes it.
    - No run card UI.
  - **On Start**, for each task:
    - create worktree `run-<runId>-<n>`, seeded with the Lead's uncommitted files (refuse symlinks and special files);
    - give it a private 0700 TMPDIR;
    - append an envelope to the prompt: no git, no spawning, stay inside scope [R11 idea 11-6];
    - spawn via E7 with the prompt in argv. Workers hold no grants.
  - **Claude Workers:**
    - Widen the plugin `PreToolUse` matcher (today it's only `AskUserQuestion|ExitPlanMode`, P packages/pocketd/internal/daemon/plugin.go:13).
    - Add a reply path (today only `PermissionRequest` replies, P packages/pocketd/internal/daemon/daemon.go:90-120).
    - Deny `Edit`, `Write`, `MultiEdit` and `NotebookEdit` on canonical paths outside scope, and report each denial to the Lead [R11 idea 11-7].
  - **Codex Workers:** scope is checked at integration only.
  - **Approvals:** Worker permission requests are ordinary Needs you plus push for the human. They never go to the Lead, which deviates from MonoCode [R11 TL;DR].
  - **Lead verbs:**
    - `run status`
    - `run wait [--timeout ≤25s]`
    - `run get <task>`: the last assistant text at Done, at most 4,000 chars
    - `run message <task> <text>`
    - `run cancel <task>`
    - `run integrate <task>`
    - `run finish`
  - **Auto-wake:** when a Worker reaches Done, Failed or Needs you and the Lead is Idle, pocketd sends the Lead one agent-sourced prompt, `[pocket] task <n>: <status>`. Capped at 20 per Run [R11 TL;DR].
  - **Integrate:** a per-file copy from Worker to the Lead checkout [R11 idea 11-8].
    - Refuse files the Lead changed since seeding, symlinks, and out-of-scope files.
    - Guard against a moved branch.
    - Keep the worktree on conflict.
    - The Lead commits.
  - **No new UI.** Workers are ordinary sessions in their worktrees on both clients.
  - **Cut line:** propose → confirm → spawn → wait/get. Integrate and auto-wake are cuttable.
- **Done when:**
  - From the phone, the owner asks a granted claude Lead to add tests for X and Y in parallel.
  - The owner confirms 2 tasks on the phone and both Workers finish.
  - The Lead integrates.
  - An out-of-scope Edit is denied and reported.
- **Tests:**
  - go: proposal validation, scope canonicalization (symlinks, `..`), `PreToolUse` deny reply, integration refusals, auto-wake cap.
  - e2e with two fakeclaude Workers.

### E13 `terminal-basics` (M2, L, lane C)

- **Goal:** under C the terminal is the only desktop drive surface (D9), so reading and moving agent output must work.
- **Ideas:** 16-1, 16-2, 16-4, 16-5, 16-6.
- **Depends:** none. `termview.rs` and the libghostty shim are outside the collision set.
- **Scope** [R16]:
  - **Scrollback:** a 10k-line cap on both VTs, with the shim wrapping `scroll_viewport` and the scrollbar [idea 16-1].
  - **Wheel:** remainder accumulation and snap-to-bottom on input [idea 16-2].
  - **Selection:** 1/2/3-click, Shift+click extend [idea 16-4].
  - **Copy:** ⌘C copies plain text, only when there's a selection, and is never sent as ^C [idea 16-5].
  - **Paste:** ⌘V through the bracketed-paste encoder [idea 16-6].
  - No new bindings beyond the standard ⌘C and ⌘V (D6).
- **Done when:**
  - Scrolling back 5k lines of agent output works.
  - A selected path copies exactly.
  - Pasting a multi-line prompt into claude arrives as one paste.
- **Tests:** `cargo test -p pocket` for the selection model and paste encoding.

## 6. What we deliberately don't build (this horizon)

| Not building | Why | Source |
|---|---|---|
| Headless Claude (stream-json + `--permission-prompt-tool stdio`) | Undocumented flag. Account-terms question. Claude stays a TUI | [R03 idea 03-2, risks; R10 idea 10-1, open questions] |
| Private Codex app-server | D4: only if headless is adopted | [R10 idea 10-2] |
| Desktop chat pane or composer, full timeline decode in Rust | D9 terminal surface. Collision with in-flight desktop work | [R14 ideas 14-2, 14-3; R05 idea 05-23; R10 idea 10-12] |
| Zeron/MonoCode visual cloning, dark theme, hero composer, model card | UI only where a feature needs it | [R14 idea 14-20; R17 ideas 17-4, 17-7] |
| Relay, E2EE relay, LAN TLS | Tailscale covers the owner. The relay is L | [R18 ideas 18-16, 18-17; R02 idea 02-1; R08 idea 08-22] |
| Lock-screen Allow/Deny, Live Activities, HUD | L each. Lock-screen auth timing is open | [R08 idea 08-19; R15 ideas 15-5, 15-6; R18 open] |
| Devices sheet, automation editor, run card | CLI plus the existing permission UI suffice | [R18 idea 18-5; R11 idea 11-5] |
| Report API, OSC detection, providers beyond claude and codex | D5 and D13 scope | [R15 ideas 15-2, 15-3; R14 idea 14-22] |
| Conversation journal on disk | Rebuilt from provider files instead (E5) | [R02 idea 02-11; R14 idea 14-4] |
| Live-upgrade fd handoff | Restore covers it | [R15 idea 15-14] |
| Signed bundle, SMAppService, updater | The owner runs from source; `pocketd daemon install` suffices | [R19 ideas 19-1, 19-2, 19-8] |
| Phone raw terminal and key bar | Remote-shell power | [R16 ideas 16-13, 16-18; R18 TL;DR] |
| Agent-delegated approvals | Keeps the human in the loop | [R11 TL;DR] |
| Structured AskUserQuestion and ExitPlanMode on the phone | Needs headless or hook-reply research | [R14 idea 14-11; R05 idea 05-4] |
| Desktop notification rework (actions, D11 sounds, dock badge, D2 ordering) | `main.rs` collision surface. Phone push first | [R14 idea 14-10] |
| Worktree retention, auto-archive on merge, checkpoints | Later | [R15 ideas 15-8, 15-11; R10 idea 10-14] |
| External inbox, notes, quick composer, iPad, Android/Windows/Linux | D13, scope | [R11 ideas 11-13, 11-16, 11-17; R08 idea 08-20] |

## 7. Execution notes for /implement agents

- **Branching:**
  - Branch from main.
  - Desktop edits in E4 (`main.rs`) and E8 (`agents.rs` + one `view.rs` line) wait until main's uncommitted desktop work is committed.
- **Commands:**
  - pocketd: `cd packages/pocketd && go vet ./... && go test -race -count=1 ./...`.
  - Goldens: `go test ./internal/proto -update`.
  - Protocol: `pnpm --filter @pocket/protocol test`.
  - Phone: `pnpm --filter @pocket/app typecheck && pnpm --filter @pocket/app test`.
  - Desktop: `cargo test -p pocket -p workspace -p agents` [R14].
- **Protocol changes** land in `packages/protocol` and `pocketd/internal/proto` together, with goldens, behind a cap (§3).
- **Every new ops or WS handler** gets a scope-matrix test row.
- **Vocabulary:** use CONTEXT.md terms: Terminal, Session, Agent, Worktree, Conversation, Needs you, Seen. Never "pane", "blocked" or "thread".

## 8. Risks and de-risking

| Risk | Evidence | De-risk in this plan |
|---|---|---|
| Undocumented Claude CLI surface breaks | `--permission-prompt-tool stdio` and SDK control requests are internals [R03 risks; R10 open questions] | Not used. Claude runs as a TUI with documented flags and hooks only |
| Claude flag drift | `--permission-mode default` is hidden in 2.1.285 [R17 risks] | Version floor plus probe (E7). `manual` fallback [R17 §F14 rules]. Argv golden table per CLI version |
| Codex app-server is experimental and drifts | `[experimental]`. Originator side effects of `turn/start` from `codex_app_server_daemon` are unverified. Approvals across clients [R03 risks] | Capability probe at attach. `codexDriver` falls back to typing on any RPC error. fakecodex fixtures pinned per CLI version. Spike evidence (P docs/spike-remote-sessions.md:75) |
| Resume argv runs something unintended | Restored commands run automatically [R15 risks] | Built from a structured record. Provider allowlist. herdr rules. No env. Full never restored. Toggle (E5) |
| PTY typing is fragile | `errNotForeground` (P packages/pocketd/internal/daemon/presence.go:119) | Coded acks (E8). Codex bypasses typing. Never type into an agent that isn't Attached |
| Agents driving agents amplify prompt injection | The ops socket is open today [R18 TL;DR]. MonoCode lets the Lead approve Workers [R11] | Grants default off and are owner-set. No approve for any agent. No transitive grants. Sanitation. Origin labels. Run caps (≤4 Workers, ≤12 tasks, 20 auto-wakes). `PreToolUse` scope deny. Worktree isolation |
| Reparenting escape | A daemonized PTY descendant looks like an outside process [R18 open] | Layered defenses: the bind limits reach to the tailnet, so a forged pairing code is useless outside it. Owner ops also check the peer's executable path. Residual risk documented |
| Claude folder-trust dialog answered by a remote `\r` | [R18 open] | Phone creates only in registered projects. Prompts go in argv. No typing into agents that aren't Attached |
| Mac asleep: phone unreachable, schedule missed | [R11 open; R15 idea 15-7] | IOPM assertion while Working or a phone is connected. 720-min grace. `pmset` check in E4 |
| Push prerequisites missing | Paid ADP, EAS and APNs key [R19 idea 19-12] | Checked in M0. If missing, E9 slips alone; nothing else depends on its sender |
| Expo sees push metadata | Third-party hop | Fixed copy, project-name title, random requestId, no content [R18 idea 18-13] |
| Collision with in-flight desktop work | [R14 §F12] | pocketd-first. Desktop edits limited to `forms.rs`, `termview.rs`, the `agents` crate, and one `main.rs` hunk after the rebase |
| Duplicate notification logic, desktop vs pocketd | The desktop computes its own (P packages/desktop/crates/pocket/src/main.rs:314-328) | Accepted for this horizon. Converge after M3 |
| Scope: about 80 dev-days in 8 weeks | Sum of the epic sizes | Two lanes. M3 is the cut line. Cut order in §4. Each milestone ships value on its own |
| Vendor squeeze | Claude RC and ChatGPT remote are free with the plan [R15 risks] | Compete where vendors can't: both providers in one fleet, several clients, no relay, daemon automations |
| No business model | Free-only orchestrators struggle [R15 risks] | Out of scope. The owner is the user |

## 9. Success metrics (local only)

- **Source:** `~/.coding-pocket/events.jsonl` (E4).
  - It records status transitions, Seen changes, answers (by which principal), prompts (origin, ack result), creates (origin, error code), restores, pushes (sent, receipt), grant refusals, automation runs, and run tasks.
  - `pocketd stats [--since 7d]` prints the table.
  - Nothing leaves the Mac.
- **Baseline:** recorded in M1, before push.

| Metric | Definition | Target by end of M3 |
|---|---|---|
| Needs you → answered | p50 and p90 from entering Needs you to resolve, for sessions not Seen at the transition. Split by answering principal | p50 ≤ 3 min. At least half of the M1 baseline |
| Done → Seen | p50 from Done to Seen | Below the M1 baseline |
| Restore success | Agents with a Conversation before a pocketd restart that are Attached on the same Conversation within 30 s | ≥ 95% |
| Create failure rate | `agent.create` errors / attempts, by origin and code | < 2%. 0 silent failures |
| Prompt delivery failures | `ack.error` per 100 prompts, by provider | Codex 0. Claude < 1 |
| Away-from-desk starts | Share of sessions with origin phone, agent, automation or run | Trend up; no fixed target |
| Push precision | Pushes for sessions Seen at send time, and pushes answered within 10 min / pushes sent | 0 Seen pushes. ≥ 50% answered |
| Grant refusals | Refused ops/WS calls from PTY descendants | 0 unexpected in daily use. The E3 red-team suite always refuses |
| Automation reliability | Due vs fired vs missed beyond grace, and double fires | 0 double fires. 0 silent misses |
| Run yield | Tasks integrated without a manual file edit / tasks confirmed. Scope denials per Run | ≥ 60% |
| Reachability | Minutes pocketd was unreachable while an agent was Working (gaps in phone socket plus Working) | Trend down after 15-7 |

## 10. Decisions this plan makes (need PO acknowledgement)

1. **No headless mode this horizon.** Claude stays a TUI. Codex is driven through the daemon (D4).
2. **`desktop.json` stays the project registry.** pocketd reads it (answers R18's open "registry owner" question for now).
3. **Push body copy uses Status words:** "Needs you", "Done", "Failed". R18's "Run finished" clashes with the Run term (11-18).
4. **LaunchSpec `prompt` is optional.** An empty prompt starts the agent Idle (R17 risk).
5. **Legacy token:** 7-day grace, no spawn scope (R18 open question).
6. **Phone access** defaults to at most Ask, with Plan first allowed. Only the owner raises it, and never to Full.
7. **Agents never approve.** Worker approvals go to the human.
8. **Grants are owner-set at launch only.** Phone-created sessions get none.
9. **Automations are created only by the owner CLI.** The phone can list, run and pause them.
10. **Desktop notifications are unchanged this horizon.** D11 sounds and the dock badge come after M3.

## 11. Open questions

- Does a Claude plugin dir load a bundled MCP server? If not, E10 falls back to documenting `claude mcp add`.
- Does Codex `turn/start` without policy params inherit the TUI's policy? What side effects does it have for the `codex_app_server_daemon` originator [R03 risks]?
- Should push be suppressed while the owner is active on the Mac but viewing another session (15-4 presence)? D3 says seen-aware only. PO call.
- Does `-c model_reasoning_effort` work for Codex effort [R17 risks]?
- Is the lead's result extraction (last assistant text at Done) enough, or does the Lead need a structured `result` block [R11 open]?
