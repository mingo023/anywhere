# 00 PRD — Coding Pocket, next 8 weeks

- Date: 2026-09-30. Status: PO draft, revised after review (06-prd-review.md) — owner review. Strategy: "C spine + A cockpit" (04-strategy-decision.md).
- Plan: 05-roadmap.md (epics E01–E14, E16–E17; E15 stretch). UX: 02-ux-spec-desktop.md, 03-ux-spec-phone.md. Where they disagree with this PRD, the spec-overrides table (05-roadmap §7.1) wins.
- Citation keys:
  - `[Rnn §x]`, `[Rnn idea nn-x]`: research reports in `../research/`.
  - `[UXD §n]` = 02-ux-spec-desktop.md; `[UXP §n]` = 03-ux-spec-phone.md; `[C §n]` etc. = `strategy/*.md`.
  - `P path:L`: Pocket code at main `f8f7293`. Prefixes: `pk/` = `packages/desktop/crates/pocket/src/`, `pd/` = `packages/pocketd/`, `app/` = `packages/app/src/`. Older report paths resolve through 05-roadmap §0.
- Vocabulary: CONTEXT.md (Terminal, Project, Worktree, Login shell, Session, Agent, Attached, Conversation, Status: Needs you / Working / Done / Idle, Seen).
  - **Failed** = Done marked failed. It ranks between Needs you and Done in Up next.
  - **Up next** = the attention list ranked per D39.
  - Neither is in CONTEXT.md yet (Q5).

## 1. Problem

| # | Problem | Evidence |
|---|---|---|
| P1 | Reaching the Mac from the phone is unsafe. pocketd listens on every interface; origin checks are off; one shared token sits in `config.json` and the desktop reads it; `permission.resolve` has no scope check, so any process in a Pocket Terminal can approve its own ask | P pd/cmd/pocketd/serve.go:46; P pd/internal/wsserver/wsserver.go:53,123,156; P packages/desktop/crates/agents/src/agents.rs:197-200; P pd/cmd/pocketd/hook.go:14 [R18 TL;DR] |
| P2 | The phone can't start work | `agent.create` is rejected (P pd/internal/proto/golden_test.go:102) [R14 idea 14-19] |
| P3 | Nothing survives a restart. pocketd persists only `config.json`; timelines live in memory; the desktop exits when pocketd drops | P pd/internal/config/config.go:30,55; P pd/internal/timeline/timeline.go:156; P packages/desktop/crates/daemon/src/daemon.rs:172-183; P pk/main.rs:30 [R15 TL;DR] |
| P4 | Desktop attention is thin. Notifications carry no actions; lists re-sort on every status change; there is no Dock badge and no sound; ⌘J reaches Needs you only | P pk/desktop/alerts.rs:86; P pk/sidebar/sessions.rs:15; P pk/sidebar/rail.rs:12; P pk/palette.rs:84 [R09 TL;DR; R14 idea 14-10] |
| P5 | The Terminal lacks basics: no scrollback, selection, copy or paste. The shim exposes six calls | P packages/desktop/crates/term/src/term.rs:33-38 [R16 §F2] |
| P6 | Prompts fail silently. Typing into a TUI fails when it isn't in the foreground, and codex is never driven over its app-server | P pd/internal/daemon/presence.go:119,125,132; P pd/internal/terminal/terminal.go:278-287; P pd/internal/codex/session.go:139,160 [R03 §9; R14 idea 14-6] |
| P7 | Fix-now bugs (D12): codex Auto-edit passes `--full-auto`; ligatures render; the phone composer only offers Stop while Working; the phone keeps one permission slot, so concurrent asks overwrite each other | P pk/modals/new_session.rs:78; P pk/terminal_view/surface.rs:59; P app/components/Composer.tsx:51-52; P app/session.tsx:43,61-64 [R17 idea 17-1; R16 idea 16-3; R05 idea 05-2] |
| P8 | A phone Needs you for a question or plan dead-ends, because only permission replies exist | P pd/internal/daemon/plugin.go:13; P pd/internal/daemon/daemon.go:92,156 [R14 idea 14-11] |
| P9 | Both vendors now ship first-party phone control of their own CLI | [R15 TL;DR] |
| P10 | The look and shell lag the rivals the owner wants to clone. There are u32 literals and no token struct, no AA check and a substring palette, and phone and desktop disagree on words, order and colour | P packages/desktop/crates/theme/src/theme.rs; P pk/palette.rs:84; P app/screens/AgentsScreen.tsx:14 [R07 idea 07-1; R04 idea 04-5; UXD §2.0; UXP §9] |

## 2. Target user and jobs

- **User.** The owner: a solo developer on a Mac who runs several claude and codex Sessions in parallel Worktrees, steps away, and uses an iPhone [R01 TL;DR; C §2]. Runs Pocket from source, with no signed bundle [R19 idea 19-1].

| Job | "When I…, I want to…" | Epics |
|---|---|---|
| J1 | …have many Sessions, see which one Needs you and answer it in one action | E08, E11, E12 |
| J2 | …am away, know the moment an Agent Needs you or is Done, and answer from the phone | E11, E12, E17 |
| J3 | …have an idea on the couch, start a Session in a Project from the phone, safely | E02, E03, E06 |
| J4 | …restart the Mac or pocketd, get every Agent back on the same Conversation | E04, E09 |
| J5 | …read, select, copy and paste in the Terminal like in any Mac terminal | E07 |
| J6 | …send a prompt, know it was delivered or why it wasn't | E10 |
| J7 | …pair a phone, trust that nothing else on the network or in a Terminal can drive my Mac | E02, E03 |
| J8 | …read an Agent's history as a Timeline and drive it without the TUI | E13, E14 |
| J9 | …work in a cockpit with the Zeron/MonoCode look, and see the same words, order and colours on the phone | E16, E17, E08 |
| J10 | …let one Agent start or check another, within limits I set | E15 (stretch) |

## 3. Product principles

1. **The Terminal is the runtime.** Every Agent is a `claude` or `codex` process in a pocketd Terminal (D9) [C §10.1].
2. **PTY-backed over headless** when the value is equal. Codex is driven over the account app-server, not a private one (D4) [R03 §9].
3. **Security before reach.** No phone spawn or remote feature ships before E02/E03 (D10) [R18 §F6].
4. **pocketd owns state; clients render it.** Registry reads, worktree ops, launch, restore and push live in pocketd [C §1].
5. **Render costs what's visible.** Virtualize anything that grows with the Project (CLAUDE.md).
6. **Additive protocol behind `hello.caps`.** Old clients keep working [R02 idea 02-9; C §3].
7. **Nothing fails silently.** Every failable action returns a coded error [C §3 invariant 4].
8. **Stable order; urgency only on attention surfaces** (D2) [UXD §1].
9. **Local only.** No telemetry; no conversation text, env or raw argv on disk [C §3 invariant 3].
10. **The human approves.** Agents never hold `approve` [R18 R6; C §3 invariant 1].
11. **One look, two clients.** Desktop and phone share status words, tones and Up next ranking (D1, D39) [UXD §2.2; UXP §2.1].

## 4. Positioning

| Rival | What it is | Pocket's answer | Source |
|---|---|---|---|
| Zeron | Native desktop + engine + iOS; drives 9 CLIs over structured protocols with its own transcript; auto-allows every tool except `AskUserQuestion`; TLS + WorkOS, no E2EE | Real TUIs the owner can still type into; human approval kept; self-hosted over Tailscale. Clone its look, attention and composer UX, not its trust model | [R01 TL;DR; R18 §F4] |
| MonoCode | Free MIT Tauri GUI over 10 CLIs, headless; Dock badge, notification controls; ~1.5 releases/day | Terminal fidelity plus a phone. Clone the badge, sounds, stable order, access picker and launch flow | [R09 TL;DR; R17 §F14] |
| First-party (`claude remote-control`, ChatGPT "Control this Mac") | One provider each, vendor relay, free with the plan | Both providers in one fleet, one attention model across desk and phone, no relay | [R15 TL;DR, §F16] |
| Superset | Daemon with Resume Args; iPhone needs Pro at $20/user/mo; vendor relay; ELv2 | Restore (E09) and phone at no cost; no third-party hop except Expo push metadata | [R15 §F16] |
| Happy | MIT, E2EE relay, free; `happy claude` wrapper; iOS/Android/web (Expo) | pocketd owns the PTY (no wrapper to start through); desktop cockpit; codex driven structurally. No E2EE relay this horizon: Tailscale only | [R15 §F16; R18 R4] |

Edge to defend: multi-provider, terminal-first, self-hosted [R15 TL;DR].

## 5. Scope (8 weeks) and non-goals

**In scope** (05-roadmap.md):
- M0, wk 1–3: E01 fix-now, E02 reach-lockdown, E03 no-self-approval.
- M1, wk 2–7: E04 always-on, E05 registry-worktrees, E06 launchspec-create, E07 terminal-surface, E08 desktop-attention, E16 look, E17 phone-shell.
- M2, wk 5–8: E09 restore, E10 codex-drive, E11 push, E12 structured-answers.
- M3 (cut line, only if ahead): E13 timeline-view, E14 desktop-composer.
- Stretch, in order:
  - E15 agent-cli + grants;
  - S1 dark + Appearance + Settings surface; S2 the rest of phone connectivity;
  - S3 headless probe doc; S4 Tailscale Serve; S5 Devices sheet; S6 pocketd queue;
  - S7 phone read-only Terminal; S8 poll → events; S9 Undo/Keep; S10 new-session canvas + model card.
- Cut order if behind: E15 → S-items → E14 → E13. M0, E16 and E17 are never cut (D43).

**Non-goals** (this horizon):

| Non-goal | Why | Source |
|---|---|---|
| Headless claude / chat Sessions | Undocumented `--permission-prompt-tool stdio`; account terms unresearched | [R03 risks; R10 open questions] |
| Private codex app-server | D4 | [R10 idea 10-2] |
| On-disk conversation journal | Restore rebuilds from provider files | [R02 idea 02-11] |
| Relay, E2EE, LAN TLS | Tailscale covers the owner; L each | [R18 ideas 18-16, 18-17] |
| Lock-screen Allow/Deny, Live Activities | L; background reconnect timing is open | [R08 idea 08-19; R15 ideas 15-5, 15-6] |
| Automations, orchestrator, MCP server | Grants must soak first; plugin MCP loading unverified | [C §11; R03 idea 03-16; R11 idea 11-5] |
| Signed bundle, SMAppService, updater, TestFlight | Needs ADP; the owner runs from source (a dev bundle for notifications is in scope, FR 01-6) | [R19 ideas 19-1, 19-2, 19-8] |
| Per-Worktree ports (`POCKET_PORT`, `POCKET_ROOT_PATH`, `ports.json`) | Speculative; no consumer this horizon | [R15 idea 15-10] |
| Latest-turn diff, review comments → prompt | Needs turn snapshots | [R06 ideas 06-1, 06-2] |
| Phone key bar / raw input | Remote-shell power | [R16 idea 16-18; R18 TL;DR] |
| File viewer on the phone (`files` scope) | Path rules unimplemented | [R18 R10] |
| Providers beyond claude and codex; Android, iPad, Linux, Windows | D5, D13 | [R15 idea 15-2] |
| Agent-delegated approvals | The human stays in the loop | [R11 TL;DR] |

## 6. Functional requirements

Each FR is testable; the test named is the acceptance check. Idea IDs are in brackets. FR numbers match epic ids.

### E01 fix-now (M0)

| FR | Requirement | Test |
|---|---|---|
| 01-1 | Every desktop spawn passes both axes (P pk/modals/new_session.rs:72-79) [17-1, 17-2; R17 §F14]:<br>• Ask: claude `--permission-mode default`; codex `-s read-only -a on-request`.<br>• Auto-accept edits: claude `--permission-mode acceptEdits`; codex `-s workspace-write -a on-request`.<br>• Plan first: claude `--permission-mode plan`; codex disabled with a hint.<br>• `--full-auto` never appears. | argv unit test, provider × access (replaces P pk/modals/new_session.rs:432-438) |
| 01-2 | The Terminal font sets `liga`, `calt`, `dlig` = 0 (P pk/terminal_view/surface.rs:59) [16-3] | unit test on the font features; capture of `->`, `!=`, `--yolo` |
| 01-3 | Phone composer (P app/components/Composer.tsx:51-52) [05-2; UXP §4.4]:<br>• not Working → Send;<br>• Working + text → the per-provider label from step 0;<br>• Working + blank → Stop;<br>• Return never stops.<br>Step 0 is a recorded manual check that types a prompt mid-turn into claude 2.1.285 and codex 0.159. The label is "Queue" where the text queues and "Send" where it steers or is lost [R05 §Open questions; UXP §13] | `composer.test.mts`, three cases per provider; step 0 result in the PR |
| 01-4 | The phone keeps permission requests in a map keyed by `requestId`. The sheet shows "{tool} · 1 of N", and resolving one keeps the rest (P app/session.tsx:43,61-64) [08-13 data half] | map add / resolve / stale test |
| 01-5 | `scripts/check.sh` is the local gate every later PR runs [14-23; R14 §F10]:<br>• `go vet` + `go test -race` (pocketd), protocol tests, app typecheck + tests;<br>• `cargo build/clippy/test --workspace`, with clippy not using `-D warnings` and no `cargo fmt` check;<br>• libghostty built once via `scripts/build-ghostty.sh`, cached on the pin, for both cargo and pocketd cgo.<br>The CI workflow file follows once Q3 is answered. | a failing test makes `check.sh` exit non-zero |
| 01-6 | Two more scripts [R17 §F14]:<br>• `scripts/probe-cli.sh`, run by `check.sh`, checks the flags `claude --permission-mode default`, `codex -s read-only -a on-request` and `codex -c model_reasoning_effort`;<br>• `scripts/bundle-dev.sh` builds a dev `Pocket.app` (`Info.plist` with `CFBundleIdentifier`, `codesign -s -`) and launches it with `open`, because gpui-pre-macos drops notifications without a bundle (system_notifications.rs:118-127). | probe exits non-zero on a rejected flag; a test notification appears from the bundle |
| 01-7 | Window bounds and layout persist in `desktop.json`, saved 500 ms after the last change. The minimum window is 900×600 (P pk/main.rs:41-45) [04-7; UXD §3.1] | restore bounds on relaunch; resize below min refused |

### E02 reach-lockdown (M0)

| FR | Requirement | Test |
|---|---|---|
| 02-1 | pocketd binds `127.0.0.1:4517` plus each Tailscale address, found by enumerating interfaces for 100.64.0.0/10 and fd7a:115c:a1e0::/48, re-checked every 30 s. `tailscale ip` is only a fallback: it is absent on a LaunchAgent PATH and in App Store Tailscale. Without Tailscale it binds loopback only, and `host.tailnet = false` drives the "Phone access needs Tailscale" line (FR 16-5) (P pd/cmd/pocketd/serve.go:46,64-70) [18-1; R18 R3] | listener set from a fake interface list; never `0.0.0.0` |
| 02-2 | Handshake hardening (P pd/internal/wsserver/wsserver.go:53,123) [18-2; R18 R5]:<br>• no `InsecureSkipVerify`; Host allowlist (loopback, bound Tailscale IPs, MagicDNS);<br>• 4 KiB pre-auth read limit; ≤16 pre-auth sockets; 3 failed hello/pair per source per minute;<br>• coded errors (FR 02-7);<br>• constant-time legacy compare; random `requestId`. | table test per rule |
| 02-3 | Per-device tokens [18-3; R18 R1]:<br>• 32 random bytes, stored as sha256 in `devices.json` (0600, atomic);<br>• `pocketd devices [--json] / rename / revoke`; revoke closes sockets with 4401;<br>• the legacy token becomes device "Shared token (legacy)" with observe, drive, approve and no spawn. | revoke closes a live socket; plaintext never written |
| 02-4 | Pairing [18-4, 19-11; R18 R2]:<br>• owner verb `pair.begin` → `{url, code, expiresAt}`; `pocketd pair` prints the QR `codingpocket://pair?v=1&h=…&c=…&n=…` from a TTY;<br>• the code is 16 bytes, lasts 5 min, is single use; 5 failures lock 60 s;<br>• `pocketd pair` is refused from a PTY descendant (`internal/peer`);<br>• pre-auth `pair` → `pair.ok {deviceId, token}`; the QR never carries the token.<br>Desktop UI: FR 03-7. | expiry, reuse, lockout, descendant refusal |
| 02-5 | `hello` carries `caps[]` and `protocol {min, max}`; `hello.ok` returns the intersection; a mismatch returns a coded error naming the older side. Ships first, in its own PR [02-9, 19-10] | goldens; old client still connects |
| 02-6 | Phone pairing [19-11, 18-14, 18-18, 02-5; UXP §4.1]:<br>• the system Camera opens `codingpocket://pair`, then a confirm screen;<br>• token in SecureStore `WHEN_UNLOCKED_THIS_DEVICE_ONLY`;<br>• a per-install `clientId` replaces `"pocket-app"` (P app/session.tsx:76);<br>• trust copy "A paired phone can run commands on this Mac as you.";<br>• pair copy "On your Mac, press ⌘K → Pair phone". | pair URL parser test; clientId persisted across launches |
| 02-7 | Phone Revoked and version screens [UXP §4.1, §5.1]:<br>• 4401 or "not paired" → Revoked screen "This phone was removed. Pair again" → pair flow;<br>• version mismatch → "Update Pocket on your Mac" or "Update Pocket on this phone", naming the older side;<br>• no retry loop on either. | close-code tests for 4401, not-paired and both mismatch sides |

### E03 no-self-approval (M0)

| FR | Requirement | Test |
|---|---|---|
| 03-1 | The desktop talks to pocketd over the unix socket as owner (peer not a PTY descendant) and stops reading the token (P packages/desktop/crates/agents/src/agents.rs:197-200,243-248). In the same PR, the legacy token moves hashed into `devices.json` and is deleted from `config.json`, and `pocketd serve` stops printing it (P pd/cmd/pocketd/serve.go:61) [18-6] | desktop works with the token gone; `config.json` and serve output carry no token |
| 03-2 | WS and ops dispatch enforce the scopes observe, drive, approve, spawn and owner [R18 R6]. `permission.resolve` requires approve (P pd/internal/wsserver/wsserver.go:156) [18-7] | scope matrix: every handler × principal |
| 03-3 | PTY-peer rules [18-8; R18 R9]:<br>• a PTY peer = a descendant of a Terminal of this pocketd instance (not "has `POCKETD_PTY`");<br>• it may send hook events for its own Terminal only; the hook PID must descend from that Terminal's process (P pd/cmd/pocketd/hook.go:14; P pd/internal/ops/ops.go:118-158);<br>• every refusal carries a code.<br>Grants are E15 (stretch); the scope matrix keeps the grant row as design only. | forged hook refused; a scratch pocketd's own desktop is owner |
| 03-4 | Input rules (P pd/internal/terminal/terminal.go:278-287) [18-9; R18 R8]:<br>• PTY peers can't send input or prompts to any Terminal, their own included, whose Agent has an open permission or question;<br>• prompts are sanitized before typing: strip C0 except `\n` and `\t`, ESC, DEL; cap 64 KiB;<br>• agent-sourced prompts starting with `!` or `/` are refused. | sanitizer table test; prompt "1" into a Needs-you Terminal refused |
| 03-5 | The red-team script, run from inside a Pocket Terminal, refuses each case with a code: resolve, pair, devices, spawn, forged hook, input to another Terminal, `nohup`, `launchctl submit`, TIOCSTI on the own `/dev/tty`, and prompt "1" into a Needs-you Terminal. Reparent cases and kernel-allowed TIOCSTI are recorded as residual (S-10). After that, the legacy device's 7-day grace starts (phone only), and it is refused once the grace ends (D17) | script exits 0 with residuals listed; legacy refused after grace |
| 03-6 | A desktop launched from inside a Pocket Terminal is observe-only and shows the page-bar banner "Observe only — pocketd is managed elsewhere" (D20) [UXD §3.2] | peer classification test; banner copy test |
| 03-7 | A desktop palette action "Pair phone…" calls `pair.begin` over the owner channel and shows the QR and the one-time code (22 chars, for EnterCode, FR 02-4) in a dialog that closes on `pair.ok`. `pocketd pair` in Terminal.app is the fallback [UXD §3.11] | a legacy-token phone re-pairs within grace with no Terminal.app |

### E04 always-on (M1)

| FR | Requirement | Test |
|---|---|---|
| 04-1 | `pocketd daemon install / uninstall` (LaunchAgent, KeepAlive on failure), `--version` and `status`. A single instance via flock; a rotating log in `~/.coding-pocket/logs` [02-7, 02-8, 19-4] | second instance exits with a message; plist golden |
| 04-2 | Login-shell env capture uses sentinel markers + `env -0`, parsed between the sentinels, with a 5 s limit. Fish is special-cased as the desktop does (P packages/desktop/crates/daemon/src/daemon.rs:114). It feeds `LookPath`, so a LaunchAgent pocketd finds claude/codex (P pd/internal/terminal/terminal.go:87-101) [10-6] | zsh and fish fixtures with rc noise and multiline values |
| 04-3 | IOPM `PreventUserIdleSystemSleep` is held only while some Agent is Working or Needs you. It blocks idle sleep only (§7 Known limits). `hello.ok.host {tailnet, keepingAwake}` + `host.changed` (cap `host.v1`) [15-7] | assertion toggles with a fake; host goldens |
| 04-4 | `events.jsonl` (0600, 10 MiB × 3) records status transitions, Seen, answers by principal, prompts (origin, ack), creates (origin, code), restores, pushes and refusals, with no content. `pocketd stats [--since 7d]` prints §9 [C §9] | event schema golden; stats on a fixture |
| 04-5 | When pocketd is down, the desktop shows "Starting Pocket's terminal service…" and retries with backoff instead of exiting. On reconnect it re-attaches Terminals and resubscribes (P packages/desktop/crates/daemon/src/daemon.rs:172-183; P pk/main.rs:30; P pk/terminals.rs:90) [19-3] | kill pocketd; window stays; Terminals return |

### E05 registry-worktrees (M1)

| FR | Requirement | Test |
|---|---|---|
| 05-1 | Reading `desktop.json`:<br>• the desktop `store` saves it via temp file + rename, 0600 (P packages/desktop/crates/store/src/store.rs:38-42);<br>• pocketd reads it with mtime reload, and a parse failure keeps the last good copy and retries (P packages/desktop/crates/store/src/store.rs:8-31);<br>• `project.list` → Projects with Worktrees `{name, path, branch, isMain}` (cap `registry.v1`) [11-4; R18 R11] | fixture file; reload on change; truncated file keeps the last copy |
| 05-2 | `internal/worktree` list / create / remove, with the desktop's name rules (P packages/desktop/crates/git/src/git.rs:143,168) [11-4, 15-9; R18 R11]:<br>• `create` takes `base?` (default `RepoConfig.base`) and uses `RepoConfig.worktrees`;<br>• remove keeps the branch (D8) and refuses the main Worktree;<br>• copy list + `.worktreeinclude`: relative, inside the root, regular files only, no overwrite, symlinks skipped;<br>• `pocketd worktree` CLI (owner). | temp git checkout tests: create, base, copy, remove keeps branch, main refused |
| 05-3 | AgentSummary v2 adds project, worktree, branch, tokensUsed, contextWindow? and origin (cap `summary.v2`; P pd/internal/proto/proto.go:168-184). `contextWindow` is agent-reported, or taken from a per-model table in pocketd; it is absent when unknown [14-21, 03-6; R05 §Open questions] | goldens; unknown model → no window |

### E06 launchspec-create (M1)

| FR | Requirement | Test |
|---|---|---|
| 06-1 | `agent.create {requestId, spec: LaunchSpec}` → `agent.created {requestId, agentId, terminalId}` or `error {requestId, code}` [17-10, 14-19]:<br>• `checkout` = existing Worktree, or `new {name, base?}` (D41);<br>• codes: `unknown_project, unknown_worktree, worktree_exists, provider_unavailable, access_not_allowed, folder_not_trusted, spawn_failed, duplicate`;<br>• `agent.providers`; cap `launch.v1`, advertised only by FR 06-3 (flips P pd/internal/proto/golden_test.go:102). | goldens in Go and TS |
| 06-2 | pocketd builds argv from the §F14 table [R17 §F14] [17-2, 17-8, 17-9, 17-12, 11-15, 03-12]:<br>• `claude -n <worktree or slug>`; the prompt as the last positional;<br>• codex + plan → `access_not_allowed`;<br>• effort only where `probe-cli.sh` shows the CLI accepts it;<br>• a new Worktree uses the Project's `RepoConfig` base, copy list and setup;<br>• setup runs as today (`|| exit`, P packages/desktop/crates/daemon/src/daemon.rs:111), and its failure → `spawn_failed` + output tail;<br>• the wrapper records the agent's exit status (`"$@"; s=$?; pocketd hook exit $s; exec $SHELL -l`, fish variant), so a non-zero exit before Attached → `spawn_failed` + the last 20 lines;<br>• `requestId` receipts dedupe; a provider probe in the Login shell, 30 s TTL.<br>Owner principal only until FR 06-3. | argv golden per provider × access × plan; bogus model → `spawn_failed` without timeout; duplicate returns the first result |
| 06-3 | The phone principal may create only within these limits (D5, D18) [18-10; R18 R7]:<br>• claude or codex; a registered Project; an existing Worktree or a new name, with no `base`;<br>• access ≤ `phone.maxAccess` (default Ask); Plan first allowed; Full refused;<br>• no grants; no cmd/env/cwd.<br>Phone-created Worktrees run the owner-configured setup. This PR advertises `launch.v1` and accepts non-owner principals. | policy table test; phone create refused before this PR |
| 06-4 | The desktop New session sheet sends `agent.create`. Argv and file copy leave the desktop (P pk/modals/new_session.rs:72-79,280-290), and base, worktrees dir, copy list and setup keep working [17-9, 17-3, 17-5; UXD §6]:<br>• access chips "Ask" · "Auto-accept edits" · "Auto" · "Full access" (desktop only, amber) · "Plan first";<br>• picks are remembered per Project, except Full access;<br>• errors show inline and keep the prompt. | modal test: error keeps the draft; remembered picks per Project |
| 06-5 | The phone New session sheet [08-21 reduced; UXP §4.8 + 05 §7.1]:<br>• Project, Worktree (existing / new name), provider, access (allowed set only), Plan first, prompt (required in UI);<br>• opens the Session on `agent.created`;<br>• Codex hint "Codex can't plan first in a terminal session". | LaunchSpec builder test from chip state |
| 06-6 | Before a claude spawn, pocketd checks Claude's recorded folder trust for the cwd. It pre-seeds trust for a new Worktree of a trusted Project, and otherwise returns `folder_not_trusted`. The phone then shows "Trust this folder in Claude on your Mac first" [R18:341; P pd/internal/daemon/presence.go:53-54] | a phone create in a fresh Worktree of a trusted Project reaches Working with no Mac input |
| 06-7 | `pocketd config set phone.maxAccess <ask\|edits\|auto>`, plus an owner-only WS `config.set` and a desktop palette action "Phone access level…". A locked chip on the phone reads "On your Mac: ⌘K → Phone access level" (D18) | owner sets, phone refused; hint copy test |

### E07 terminal-surface (M1)

| FR | Requirement | Test |
|---|---|---|
| 07-1 | 10k-line scrollback; wheel/trackpad with remainder accumulation; snap to bottom on input; a "Jump to bottom ↓" pill when scrolled up [16-1, 16-2; UXD §3.5]. The shim uses `scroll_viewport` (libghostty 4ae9f1a2 terminal.h:2384) | wheel routing table; capture with 10k lines |
| 07-2 | Selection by 1/2/3 clicks, Shift-click extend, drag autoscroll. ⌘C copies plain text only when there is a selection, and never sends ^C. ⌘A [16-4, 16-5; selection.h, formatter.h] | selection unit tests; ⌘C without selection is a no-op |
| 07-3 | ⌘V goes through `ghostty_paste_encode` (bracketed paste); a multi-line paste without bracketed mode asks first (paste.h:187,209,241) [16-6] | paste-safety classifier test |
| 07-4 | ⌥←/→ → ESC b/f; ⌘←/→ → ^A/^E; ⌘⌫ → ^U; resize fit debounced 80 ms trailing [13-12, 16-11] | `key_bytes` tests |
| 07-5 | Cursor: DECSCUSR shape, blink 530/530 (off under Reduce Motion), hollow when unfocused. IME preedit at the cursor [16-16, 16-10] | cursor state test; manual IME check |
| 07-6 | Closing a Terminal whose Agent is Working asks first [13-13; UXD §6 Close confirm] | confirm shown for Working, not for Idle |

### E08 desktop-attention (M1)

| FR | Requirement | Test |
|---|---|---|
| 08-1 | A permission ask posts a notification whose actions resolve over the owner channel (P pk/desktop/alerts.rs:86) (D30) [14-10, 13-20]:<br>• "Allow" = allow once; "Deny and stop" = deny + interrupt (P pd/internal/daemon/daemon.go:148); longer options only in the Inbox panel (FR 12-4);<br>• body line 1 "{project} · {worktree}", line 2 the ask, ≤240 chars;<br>• a click focuses the Session.<br>Tested through the dev bundle (FR 01-6). | `Alerts` test: actions on permission asks only; Deny → deny + interrupt → Idle; notification fires from the dev bundle |
| 08-2 | Dock badge = the count of Needs you Sessions, cleared at 0 (objc FFI) [09-1] | count function test |
| 08-3 | Sounds per [UXD §2.9] [05-5, 05-6, 07-6, 04-9]:<br>• Needs you / Done (fresh ≤45 s, not an interrupt) / Failed; 250 ms coalesce, highest priority wins;<br>• a silent baseline on launch and reconnect; non-Seen only, even when focused (D11);<br>• per-cue palette toggles in `desktop.json`, default on (D25);<br>• `afplay` off the UI thread;<br>• Zeron's WAVs with the MIT notice in `THIRD_PARTY.md`. | sound gate test (coalesce, priority, baseline, Seen) |
| 08-4 | Order and Up next [04-3; UXD §3.12]:<br>• Sessions and the rail sort by `created_at` desc, and status never reorders (remove P pk/sidebar/sessions.rs:15, P pk/sidebar/rail.rs:12, P pk/palette.rs:84);<br>• a palette "Up next" group (D39);<br>• Inbox sections NEEDS YOU / FAILED / DONE, each oldest transition first (P pk/inbox.rs:45), with "Mark all seen". | order test (status change keeps order); Up next ranking |
| 08-5 | D1 on the desktop [12-3; UXD §2.2, §3.3]:<br>• "Running" → "Working" (P packages/desktop/crates/ui/src/ui.rs:457);<br>• Done = success green + check; Needs you = amber `WAITING`; Failed = red ×; Idle faint;<br>• the Needs you row gets a `WAITING_BG` tint + a "Needs you" label + its glyph shape. | status → glyph/colour test; AA contrast test |
| 08-6 | The UXD §6 C-tagged desktop strings ship, including "{worktree name}" (was "Workspace"), "Mark all seen" and "Go to next Needs you" (P pk/sidebar/column.rs:61; P pk/inbox/list.rs:34; P pk/palette.rs:125) | a test greps UI strings for the CONTEXT.md avoid list |
| 08-7 | Keyboard (D6, D24, D40) [04-1, 04-2]:<br>• ⌘J = the next Needs you Session, oldest transition first, wrapping;<br>• ⌘⇧J and palette "Go to Up next" = the top-ranked Up next item other than the current Session, recomputed at each press;<br>• ⌘n = the nth Session in the visible sidebar list after filters, across expanded Worktrees (Compact: the nth rail item), with chips after 280 ms;<br>• ⌃Tab / ⌃⇧Tab wrap. | keymap uniqueness; ⌘⇧J with 3 Done visits all 3; ⌘n with filters and Compact |

### E09 restore (M2)

| FR | Requirement | Test |
|---|---|---|
| 09-1 | Each Terminal runs in its own process group. Kill = SIGTERM → 2 s → SIGKILL to the group (P pd/internal/terminal/terminal.go:311); a `POCKETD_PARENT` marker; orphans reaped on start [10-5] | child of child dies on kill |
| 09-2 | `state/terminals.json` (0600, atomic) stores terminalId, launchDir, provider?, conversationId?, transcriptPath? and launch {access, plan, model?, effort?}. It never stores title, env, raw argv or conversation text. On restore the title is re-derived from the provider session files, falling back to the Worktree name. Terminals return with the same id [15-1] | round-trip; forbidden fields absent |
| 09-3 | Agents with a Conversation resume through the E06 builder [15-1, 10-7 marker; R17 §F14]:<br>• Full resumes as Ask (D28); argv validated by herdr rules; `resume_agents_on_restore` (default on);<br>• a Working Agent at shutdown gets an "Interrupted by restart" marker and goes Idle;<br>• each Agent reports `restore: resumed \| interrupted \| access_lowered \| failed` (cap `restore.v1`);<br>• codex `resume -s/-a` is probed first, with app-server `thread/resume` as the fallback. | restore fixture per provider and outcome |
| 09-4 | Timelines rebuild from provider files: claude `transcriptPath`, codex `thread/resume` [14-4 re-derived] | rebuilt timeline equals the live one on a fixture |
| 09-5 | The desktop re-adopts restored Terminals without user action | e2e: restart pocketd with 2 Agents → both Attached on the same Conversation ≤30 s |
| 09-6 | Desktop and phone show the restore outcome as a row notice: "Resumed" · "Interrupted by restart" · "Full access resumed as Ask" · "Couldn't resume" [UXD §3.14; UXP §5.4] | one render test per outcome, both clients |

### E10 codex-drive (M2)

| FR | Requirement | Test |
|---|---|---|
| 10-0 | A spike doc answers [R03 risks; UXP §13]:<br>• does `turn/start` without policy params inherit the TUI's policy?<br>• originator side effects for `codex_app_server_daemon` (P pd/internal/codex/rpc.go:46,59);<br>• approval fan-out across clients;<br>• what claude's TUI does with a prompt typed mid-turn (cross-check E01 step 0). | doc merged before code |
| 10-1 | `codexDriver` implements `Driver` (P pd/internal/agent/agent.go:16-21) [03-1, 03-5 steer]:<br>• prompt → `turn/start` (or `turn/steer` mid-turn, per the spike);<br>• interrupt → `turn/interrupt` with the tracked turnId;<br>• any RPC error falls back to typing;<br>• `frame.Error` carries `code`. | fakecodex fixtures pinned per CLI version |
| 10-2 | `agent.prompt` / `agent.interrupt` carry `requestId` → `ack {requestId, result: delivered / queued / error, code: not_foreground / not_attached / rejected}`. pocketd dedupes on `(clientId, requestId)` and never types into a non-Attached Agent (cap `prompt.ack.v1`) [02-5] | ack table test; duplicate delivered once |
| 10-3 | A capability matrix of provider × CLI version (prompt, steer, interrupt, compact, resume, effort, plan), with conformance tests [03-12] | matrix test per fixture |
| 10-4 | Moved: AgentSummary v2 → FR 05-3; context meter → FR 16-4 | — |
| 10-5 | The phone outbox [05-9, 08-8; UXP §5.2]:<br>• queue, ack by id, retry;<br>• "Offline — sends are saved";<br>• sending / delivered / queued / "Not delivered — tap to retry". | outbox state machine test |
| 10-6 | A phone prompt reaches a codex TUI that has `less` in the foreground and shows up in the TUI | acceptance script |

### E11 push (M2)

| FR | Requirement | Test |
|---|---|---|
| 11-1 | `internal/notify` fires on seen-aware transitions (D3) [08-1, 08-4]:<br>• Needs you on entry; Done / Failed when fresh, where fresh = a transition this pocketd process observed live;<br>• the baseline on start, restore and client hello is silent;<br>• never for a Session Seen at send time;<br>• `collapseId = agentId`. | transition table test; Seen suppresses; restart with 3 Done → 0 pushes |
| 11-2 | `push.register {expoToken, prefs}` / `push.unregister`, stored per device. The phone re-sends on each `hello.ok` (cap `push.v1`) [08-2] | goldens |
| 11-3 | The Expo sender (D29) [18-13, 19-12]:<br>• body "Needs you" / "Done" / "Failed"; title = the Project name;<br>• payload = agentId + a random requestId + category;<br>• receipts polled; `DeviceNotRegistered` prunes. | payload golden; prune test |
| 11-4 | `pocketd push status` lists devices and the last 30 sends with receipts [08-5] | CLI golden |
| 11-5 | Phone [08-3, 19-13; UXP §4.9]:<br>• a pre-prompt card before the OS prompt; an alert when denied;<br>• a tap opens the Session, held until FR 17-2 reconnects;<br>• foreground suppression only for the open Session. | handler test |
| 11-6 | Moved to FR 17-3 | — |

### E12 structured-answers (M2)

| FR | Requirement | Test |
|---|---|---|
| 12-1 | pocketd emits `question.request {requestId, agentId, kind: question / plan, …}` from the PreToolUse input for `AskUserQuestion` / `ExitPlanMode` (P pd/internal/daemon/plugin.go:13) (cap `question.v1`) [14-11] | golden from a recorded hook payload |
| 12-2 | The claude answer ladder: hook reply (if the spike proves it carries the answer) → keystrokes when Attached and in the foreground → a rung 3 that depends on the client (P pd/internal/daemon/daemon.go:92,156):<br>• desktop: "Answer in the terminal" + "Open terminal";<br>• phone: "Answer on your Mac", with no button until S7.<br>Every rung acks with a code. | ladder test per rung; the phone never renders "Open terminal" without S7 |
| 12-3 | The phone ApprovalPanel replaces PermissionSheet [08-13, 05-4 reduced; UXP §3, §4.5]:<br>• one page per open request, "1 of N", numbered options, a feedback field;<br>• a plan variant: rendered plan markdown, "Approve" / "Keep planning", optional feedback sent with Keep planning;<br>• it clears only on ack or resolved;<br>• the back badge counts Needs you in other Sessions. | panel state test; pending ask in another Session shows on the badge |
| 12-4 | The desktop Inbox detail answer panel (P pk/inbox/detail.rs) [01-13, 05-4; UXD §3.12]:<br>• reuses the §3.7 card: 1–9 choose, Enter sends, Esc closes;<br>• covers permissions (incl. "Allow for this session"), questions and plans. | key test per kind |
| 12-5 | Codex approvals offer "Allow for this session" (`acceptForSession`) beside accept / decline (P pd/internal/codex/session.go:275,277) | fakecodex reply golden |

### E13 timeline-view (M3)

| FR | Requirement | Test |
|---|---|---|
| 13-1 | `agent.timeline` gains `beforeSeq`; replies are ≤512 KiB; the client has a memory cap (cap `timeline.before.v1`; P pd/internal/timeline/timeline.go:156) [14-5, 02-3] | byte cap test; backward paging |
| 13-2 | Rust decodes the full v3 timeline (tool status/output/diff, tasks, plan) [14-3] | decode goldens shared with TS |
| 13-3 | Agent Tabs switch Terminal ⇄ Timeline [14-1; UXD §3.4]:<br>• a page-bar segmented control "Timeline" \| "Terminal", shown only for Attached Sessions with a Conversation;<br>• ⌘⇧T (D23) and the palette actions "Show Timeline" / "Show Terminal";<br>• the switch never resizes the PTY; the default is Terminal (D22). | PTY size unchanged across switches; control hidden without a Conversation |
| 13-4 | The Timeline renders with `list`, max width 760 [03-15, 05-1, 05-10, 05-12, 05-17, 05-8; UXD §3.6]:<br>• tool groups with a summary line, folded when settled;<br>• long prompts fold (>5 lines / 400 chars);<br>• a jump pill at 320 px;<br>• the working trailer shows elapsed time only. | summary grammar test; capture of a 2k-item Timeline |

### E14 desktop-composer (M3)

| FR | Requirement | Test |
|---|---|---|
| 14-1 | The Outbox gains `prompt` and `interrupt` with acks (P packages/desktop/crates/agents/src/agents.rs:208-216) [14-2] | ack handling test |
| 14-2 | A plain multiline composer under the Timeline, with no pickers, morphs Send / Queue / Stop (labels per FR 01-3 step 0; Steer after E10). Enter never stops; per-Session drafts [05-2, 05-11] | morph table test |
| 14-3 | The approval panel replaces the composer while the Session Needs you (reuses FR 12-4) | state test |
| 14-4 | An optimistic bubble with "Not delivered" retry [05-9]. The context ring is FR 16-4 | delivery state test |

### E16 look (M1)

| FR | Requirement | Test |
|---|---|---|
| 16-1 | Tokens (D26) [07-1 light half, 07-8; UXD §2.0, §2.5]:<br>• a `Palette` struct + `p(cx)` with light values; WHITE split;<br>• radii popover 12, dialog 16, palette 16; `MENU_IN` on every menu;<br>• the sites this epic and E08 touch read tokens. | AA contrast test: text ≥4.5:1, glyphs ≥3:1 |
| 16-2 | Palette v2 [04-5; UXD §3.11]:<br>• every-word match with accent highlight; a `>` prefix = actions only;<br>• groups Up next (FR 08-4) / Sessions / Worktrees / Files / Actions;<br>• a pointer guard. | match and ranking test; `>` filter test |
| 16-3 | Menus and seams [04-6, 04-7 widths; UXD §3.1, §3.2]:<br>• the Session context menu "Copy path" · "Copy resume command" · "Close session…", opened by right-click at the pointer;<br>• seams: 20 px hit area, double-click reset, persisted column widths. | menu item test; widths persist |
| 16-4 | Context meter (D7) [05-7; UXD §3.4; UXP §4.3]:<br>• warn 75 %, danger 90 % on the desktop bar, rail and usage card (P packages/desktop/crates/ui/src/ui.rs:665-668; P pk/sidebar/usage.rs:21) and the phone chip;<br>• a page-bar ring with a hover card "Context window" / "{used} / {window} tokens" / "{left} tokens remaining";<br>• the 200k constant is gone (P packages/desktop/crates/agents/src/agents.rs:57,160), and the meter is hidden when the window is unknown. | thresholds 74/75/89/90/unknown on both clients |
| 16-5 | The aside foot shows the host state from FR 04-3 [15-7; UXD §3.2]:<br>• "Keeping Mac awake" while the assertion is held;<br>• "Phone access needs Tailscale" (docs link) when `tailnet = false`. | host state → line test |

### E17 phone-shell (M1)

| FR | Requirement | Test |
|---|---|---|
| 17-1 | Dead controls and copy [UXP §1 principle 7, §9]:<br>• "Open raw terminal", "Attach" and "Dictate" are gone; the diff pill "Review changes" stays hidden until FR 17-5 (P app/screens/ChatScreen.tsx:147,155; P app/components/Composer.tsx:28,44);<br>• strings "Sessions", "Needs you", "Back to sessions" (P app/screens/AgentsScreen.tsx:14; P app/components/TimelineView.tsx:83; P app/screens/ChatScreen.tsx:131). | a test greps UI strings for the CONTEXT.md avoid list |
| 17-2 | Minimal graced connectivity [08-6, 08-7 minimal; UXP §5.1]:<br>• a drop never ejects to ConnectScreen;<br>• redial with backoff on drop and on AppState / NetInfo change; resync on `hello.ok`;<br>• offline copy "Can't reach {host}. Is your Mac awake (lid open) and on Tailscale?". | reconnect state test; pocketd restart keeps the Session screen |
| 17-3 | The Sessions list (was FR 11-6) [08-11; UXP §3, §4.2]:<br>• stable order (D2); an Up next section (D39); D1 tones;<br>• a summary line in urgency order with zero counts omitted, e.g. "1 needs you · 1 failed · 2 done · 3 working";<br>• a back badge = the Needs you count in other Sessions. | `order.test.mts`; summary grammar test; tones in `status.test.mts` |
| 17-4 | Chat [05-11, 05-1; UXP §4.3]:<br>• per-Session drafts;<br>• tool-row summary grammar;<br>• settled groups fold. | draft persistence test; summary grammar test |
| 17-5 | The Changes screen [UXP §4.6], virtualized, is opened by the "Review changes" pill. No protocol change | render test with 1000+ files |

### Stretch: E15 agent-cli

| FR | Requirement | Test |
|---|---|---|
| 15-0 | Grants [11-2; R18 R9]:<br>• owner-set per Terminal (`pocketd grant`, `POCKETD_GRANT`);<br>• scopes ⊆ {observe, drive, spawn, notify}, never approve/owner, never transitive. | granted drive allowed; approve always refused |
| 15-1 | `pocketd agent list / read / start / prompt / wait` and `pocketd agent worktree create` over ops, each gated by the caller's grant [11-1, 11-15]:<br>• `start` uses LaunchSpec with origin agent and no grants;<br>• prompts are sanitized and labelled "From agent {name}";<br>• mutations take `requestId`. | scope matrix rows; non-transitive grant test |
| 15-2 | `pocketd notify <text>` (notify grant) raises a seen-aware notice on the desktop and via push [15-3 notify verb] | notify gate test |
| 15-3 | No verb approves; attempts are refused with a code; the red-team suite is extended | red-team script |

## 7. Non-functional requirements

**Performance**

| NFR | Requirement | Source |
|---|---|---|
| P-1 | Changes list frame ≤3 ms at 1137 files; click → diff ≤100 ms. Every desktop epic runs capture mode in release before merge | CLAUDE.md |
| P-2 | Render cost scales with what's visible: the Timeline uses `list`; no per-frame work proportional to history or scrollback | CLAUDE.md; [R14 §F12] |
| P-3 | No git, file IO, sound playback or reconnect on the UI thread | CLAUDE.md |
| P-4 | Timeline reopen: first paint <100 ms; live deltas ≤200 ms after arrival | [R02 §F13] |
| P-5 | Timeline replies ≤512 KiB | [R02 idea 02-3] |
| P-6 | Desktop steady RSS 150–250 MB over a workday with 10 Agents (measured, reported per milestone) | [R02 §F13] |
| P-7 | No new polls. The 1 s list poll and the 2 s git poll stay until S8 | [R14 idea 14-14] |
| P-8 | Spinners: measure the whole-window redraw with 3 Working spinners and 1137 files. If over budget, share one ~12 fps clock | [UXD §9 Q2; R07 idea 07-14] |

**Security** (R18, D10)

| NFR | Requirement | Source |
|---|---|---|
| S-1 | Never listen on `0.0.0.0` | [R18 R3] |
| S-2 | `approve` is held only by the owner and off-Mac device tokens; no PTY descendant resolves | [R18 R6] |
| S-3 | No plaintext token on the Mac's disk after E03 PR1 | [R18 R1] |
| S-4 | State files 0600, atomic; `desktop.json` written 0600 via temp + rename (FR 05-1) | [R18 R11; C §3] |
| S-5 | No env, raw argv, title or conversation text persisted by pocketd | [C §3 invariant 3] |
| S-6 | Prompt sanitation and a 64 KiB cap on every source | [R18 R8] |
| S-7 | Push carries fixed copy and ids only | [R18 R12] |
| S-8 | Phone token in SecureStore `WHEN_UNLOCKED_THIS_DEVICE_ONLY` | [R18 R13] |
| S-9 | Every new WS/ops handler has a scope-matrix row; the red-team script is green on every E03+ PR touching dispatch | [C §7] |
| S-10 | Residual risk, documented: reparented PTY descendants (`nohup`, `launchctl submit`, `open -a Terminal`) look external and get owner; TIOCSTI on the own tty, if the kernel allows it, bypasses pocketd. Optional hardening later: approve only for owner peers whose `proc_pidpath` is a Pocket executable | [R18 R9; R18:136,353] |

**Accessibility**

- Desktop:
  - status is never shown by colour alone (glyph shape per status; the Needs you row adds a label);
  - AA text 4.5:1 and glyphs 3:1, enforced by the contrast test (FR 16-1);
  - every new action is reachable from the keymap or the palette; keymap uniqueness test [UXD §2.2, §5, §8].
- Phone:
  - VoiceOver labels per [UXP §8.1]; Dynamic Type caps per [UXP §8.2];
  - ≥44 pt targets; solid surfaces under Reduce Transparency;
  - status is always a word; glyph α values meet 3:1 in the contrast test [UXP §2.1, §8.3].

**Reduced motion**

- Desktop follows macOS Reduce Motion (P pk/desktop.rs:81,352-353): the spinner is static, the cursor doesn't blink, tabs don't slide, one-shots snap [UXD §2.8]. The System / On / Off override waits for the Settings surface (S1).
- Phone: springs and fades snap; spinner and pulse are static; shimmer is off [UXP §6].

**Reliability**

- Every failable action returns a coded error, with no silent fallback. Today `agent_op` falls back to a shell (P packages/desktop/crates/daemon/src/daemon.rs:112); FR 06-2 records the exit status [C §3 invariant 4].
- Protocol additions are additive and cap-gated. An M0 phone keeps working against an M3 pocketd [R19 idea 19-10].

**Known limits**

- **Lid closed.** The IOPM assertion blocks idle sleep only. A closed MacBook lid sleeps the Mac, unless it runs in clamshell mode on power with an external display. Phone reach and push then stop, and the phone's offline copy names the lid (FR 17-2).
- **Reparent escape.** See S-10.

## 8. UX summary

| Area | Epic | Spec |
|---|---|---|
| Desktop fix-now, geometry | E01 | [UXD §3.1, §7 Phase 0] |
| Tokens, palette v2, menus, seams, context meter, host line | E16 | [UXD §2.0, §2.5, §3.1, §3.2, §3.4, §3.11, §7 Phases 1–2 light] |
| Status mapping, order, Up next, ⌘J / ⌘⇧J, ⌘1–9, sounds, notifications, Inbox | E08 | [UXD §2.2, §2.9, §3.3, §3.12, §3.13, §5, §6] |
| Observe-only banner, Pair phone dialog | E03 | [UXD §3.2, §3.11] |
| Terminal surface | E07 | [UXD §3.5, §6 Close confirm, §7 Phase 3] |
| Desktop New session (sheet, not canvas) | E06 | [UXD §6 Chips] |
| Restore notices | E09 | [UXD §3.14; UXP §5.4] |
| Inbox answer panel, plan variant | E12 | [UXD §3.7, §3.12] |
| Timeline view, composer | E13–E14 | [UXD §3.4, §3.6, §3.7, §7 Phase 6] |
| Phone composer morph, permissions map | E01 | [UXP §4.4, §10 Phase 0] |
| Phone copy, dead controls, connectivity, order, drafts, tool rows, Changes | E17 | [UXP §3, §4.2, §4.3, §4.6, §5.1, §9, §10 Phases 0–1c] |
| Phone Pair, Revoked, version screens | E02 | [UXP §4.1, §5.1] |
| Phone ApprovalPanel, plan variant | E12 | [UXP §4.5] |
| Phone push | E11 | [UXP §4.9, §10 Phase 2] |
| Phone New session | E06 | [UXP §4.8] |
| Phone outbox, delivery states | E10 | [UXP §5.2] |
| Deferred UX | S1, S2, S7, S10 | [UXD §3.9, §3.10, §7 Phase 1 dark; UXP §10 Phases 1a, 3] |

Every place where this PRD or the roadmap deviates from UXD/UXP is a row in 05-roadmap §7.1 "Spec overrides" (D42). Plans copy their rows verbatim.

## 9. Success metrics (local only)

Source: `~/.coding-pocket/events.jsonl` via `pocketd stats` (E04). The baseline is recorded at the end of M1, before push. Nothing leaves the Mac [C §9].

| Metric | Definition | Target by end of M3 |
|---|---|---|
| Needs you → answered | p50 / p90 from entering Needs you (not Seen) to resolve, split by principal | p50 ≤3 min; ≤ half the M1 baseline |
| Done → Seen | p50 | below the M1 baseline |
| Restore success | Agents with a Conversation Attached on the same Conversation ≤30 s after a pocketd restart | ≥95% |
| Create failures | `agent.create` errors / attempts, by origin and code | <2%; 0 silent |
| Prompt delivery | `ack.error` per 100 prompts, by provider | codex 0; claude <1 |
| Push precision | pushes for Sessions Seen at send time; pushes answered ≤10 min / sent | 0 Seen pushes; ≥50% answered |
| PTY refusals | refused ops/WS calls from PTY descendants | 0 unexpected in daily use; the red-team always refuses |
| Timeline use | Timeline opens per Attached Session per day, from the page-bar control, ⌘⇧T or the palette (kill signal, D34) | reported; low use → don't expand |
| Reachability | minutes pocketd was unreachable while an Agent was Working | trends down after E04 |
| Desktop perf | CLAUDE.md baselines | held every milestone |

## 10. Decisions log

Settled (do not reopen): D1–D13.

| ID | Decision | Status |
|---|---|---|
| D1 | Status colours: Needs you amber; Working accent + spinner; Done success green; Failed danger red; Idle faint ink. Zeron's amber = queued is not adopted | settled |
| D2 | Stable order. Only attention surfaces (⌘J, palette Up next, Dock badge, phone Up next, notifications) sort by urgency | settled |
| D3 | Expo Push from pocketd, on seen-aware transitions (not the 45 s staleness rule), with one collapse id per Session | settled |
| D4 | Codex is driven over pocketd's second-client connection to the account app-server; a private stdio app-server only if headless is adopted | settled |
| D5 | `agent.create` carries LaunchSpec {project, checkout, provider, model, effort, access, plan, prompt}; pocketd builds argv. Phone: claude/codex, a registered Project, no cmd/env/cwd | settled |
| D6 | Keep ⌘J (next Needs you), ⌘K, ⌘T, ⌘N / ⌘⇧N; no collisions; ⌘1–9 = Sessions in visible sidebar order | settled |
| D7 | Context warn 75%, danger 90%; the window is agent-reported; hidden when unknown | settled |
| D8 | Deleting a Worktree keeps its branch | settled |
| D9 | The Terminal surface is in scope | settled |
| D10 | R18 fixes precede phone spawn and remote reach: bind, per-device hashed tokens, QR pairing, desktop off the phone token, no agent self-approval | settled |
| D11 | Banners suppressed for Seen Sessions; sounds on non-Seen transitions even when focused; per-cue toggles, default on | settled |
| D12 | Fix-now: codex `--full-auto`, ligatures, phone composer Stop-only, agent self-approval | settled |
| D13 | macOS + iPhone only | settled |
| D14 | Strategy "C spine + A cockpit"; no headless mode this horizon; claude stays a TUI [C §10.1] | PO-decided — review |
| D15 | `desktop.json` stays the Project registry. pocketd reads it (mtime reload); the desktop stays its only writer, now atomic [C §10.2] | PO-decided — review |
| D16 | LaunchSpec `prompt` is optional in the protocol (empty starts Idle); the phone UI requires one [C §10.4; UXP §12] | PO-decided — review |
| D17 | The legacy token, in three steps [R18 R1; C §10.5]:<br>(1) E02 keeps it as a device, with no spawn and no expiry;<br>(2) E03 PR1 moves the desktop to the unix socket, moves the token hashed into `devices.json`, deletes it from `config.json` and stops printing it;<br>(3) E03 PR5, after the Pair phone dialog exists, starts a 7-day phone grace, then refuses it. | PO-decided — review |
| D18 | `phone.maxAccess` defaults to Ask; Plan first is allowed. The owner raises it to Auto-accept edits or Auto via `pocketd config set` or ⌘K → Phone access level. Full access is never offered from the phone this horizon (tighter than [R17 §F14 rules] and [UXP §12]) | PO-decided — review |
| D19 | Agents never hold `approve`. Grants (E15, stretch) are owner-set at launch and never transitive; phone-created Sessions get none [C §10.7–8] | PO-decided — review |
| D20 | A desktop launched inside a Pocket Terminal is a PTY descendant: observe-only, plus a banner. The dev loop runs the desktop from Terminal.app, or against a scratch pocketd (05 §7) | PO-decided — review |
| D21 | The desktop keeps its own alert detector (P pk/status.rs:120-124); pocketd's push detector mirrors its table; they converge after M3. Desktop notifications change this horizon (rejects [C §10.10]) | PO-decided — review |
| D22 | The default agent view is Terminal; Timeline is opt-in; no setting (answers [UXD §9 Q1]) | PO-decided — review |
| D23 | ⌘⇧T switches Terminal ⇄ Timeline. [UXD §3.4] left it unbound, and ⌘⇧T is free in P pk/actions.rs:5-18 | PO-decided — review |
| D24 | ⌘J stays "Go to next Needs you" (D6). ⌘⇧J and the palette action "Go to Up next" walk Up next (Needs you, Failed, Done), each press going to the top-ranked item other than the current Session. ⌘⇧U (15-15) is not adopted [UXD §8] | PO-decided — review |
| D25 | Sound toggles ship as palette actions persisted in `desktop.json`; the Settings surface waits for S1 (deviates from the [UXD §7] order constraint 2) | PO-decided — review |
| D26 | The light `Palette` struct, AA contrast test, radii and `MENU_IN` ship in E16 PR1, before the D1 remap (E08 PR1). Dark mode and the Appearance actions wait for S1 [R07 ideas 07-1, 07-2] | PO-decided — review |
| D27 | No on-disk conversation journal; restore rebuilds from provider files [R02 idea 02-11] | PO-decided — review |
| D28 | Restore: Full resumes as Ask; `resume_agents_on_restore` defaults on; interrupted turns are marked and go Idle; every outcome is shown (FR 09-6) [R15 idea 15-1; R17 §F14 rules] | PO-decided — review |
| D29 | Push copy: body "Needs you" / "Done" / "Failed"; title = the Project name; no content [C §10.3; R18 R12] | PO-decided — review |
| D30 | gpui-pre 0.3.6 has no notification subtitle (platform.rs:441-453): body line 1 "{project} · {worktree}", line 2 the ask. "Allow" / "Deny and stop" appear only on permission asks | PO-decided — review |
| D31 | Codex Ask = `-s read-only -a on-request` (the CLI rejects `untrusted`). Codex Plan first is disabled in Terminal Sessions, with the hint "Codex can't plan first in a terminal session" [R17 §F14 deviations] | PO-decided — review |
| D32 | Ligatures off unconditionally; no toggle [R16 idea 16-3] | PO-decided — review |
| D33 | `scripts/check.sh` is the gate from E01 PR1. A macOS CI workflow with a cached libghostty follows once Q3 is answered [R14 idea 14-23] | PO-decided — review |
| D34 | Kill signal: if Timeline opens per Attached Session per day stay low two weeks after M3, stop Timeline/composer work | PO-decided — review |
| D35 | Worktree copy never overwrites and skips symlinks (today it overwrites, P pk/modals/new_session.rs:290) [R18 R11] | PO-decided — review |
| D36 | Scopes this horizon: observe, drive, approve, spawn, owner. `notify` arrives with grants (E15). `files` and `orchestrate` are deferred [R18 R6; C §3] | PO-decided — review |
| D37 | The Working + text label is chosen per provider by the E01 step-0 check: "Queue" where typing queues, else "Send". "Steer" waits for the E10 spike [UXP §13] | PO-decided — review |
| D38 | Push is not suppressed while the owner is active on the Mac but viewing another Session; seen-aware only (D3) [C §11 Q3] | PO-decided — review |
| D39 | Up next ranking on every surface: Needs you > Failed > Done (not Seen), then oldest transition first. The phone's newest-first tie-break (P app/status.ts:23-24) and the Inbox's (status, newest) sort change [UXD §3.11, §3.12] | PO-decided — review |
| D40 | ⌘n = the nth Session in the visible sidebar list after filters, across expanded Worktrees; in Compact, the nth rail item (refines D6) | PO-decided — review |
| D41 | LaunchSpec `checkout.new {name, base?}` is additive. pocketd reads `RepoConfig` base / worktrees / setup / copy from `desktop.json`. Only the desktop sets `base`. Phone-created Worktrees run the owner-configured setup, and a setup failure is `spawn_failed` | PO-decided — review |
| D42 | The spec-overrides table (05 §7.1) wins over UXD/UXP; plans copy their rows verbatim | PO-decided — review |
| D43 | The look (E16) and the phone shell (E17) are M1 and never cut. E15 moves to stretch. Cut order: E15 → S-items → E14 → E13 | PO-decided — review |

## 11. Open questions (owner calls)

1. **Licence** — closed. MIT only requires keeping the notice, so Zeron's WAVs ship with `THIRD_PARTY.md` (E08 PR4). Pocket's own licence folds into Q3.
2. **Apple Developer Program.** Push needs a paid ADP account, an EAS project and an APNs key [R19 idea 19-12]. Without them E11 isn't scheduled; nothing else depends on its sender.
3. **CI cost, visibility, licence.** Paid macOS runner minutes; whether the GitHub project (remote `mingo023/anywhere`) goes public; which licence it carries if so [R15 §F16]. Until answered, `scripts/check.sh` is the gate (D33).
4. **Paid tiers.** Stay free, or plan a paid tier later? Rivals charge for phone access (Superset Pro $20/user/mo) [R15 §F16, risks]. This roadmap assumes free.
5. **CONTEXT.md.** Add "Up next" (the D39 attention list) and the note "Failed = Done marked failed; ranks between Needs you and Done in Up next". Until then, the PRD header carries both.
