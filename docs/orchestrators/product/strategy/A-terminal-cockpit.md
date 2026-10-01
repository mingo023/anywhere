# Strategy A: Terminal-native cockpit

- **Date:** 2026-09-30
- **Horizon:** 6–8 weeks
- **Platforms:** macOS desktop and iPhone (D13)
- **Decisions:** D1–D13 are applied as settled.
- **Citation keys:**
  - `[Rnn §x]` or `[Rnn Fx]`: a report section.
  - `[Rnn idea nn-x]`: a report idea.
  - `P path:L`: Pocket code at `5bc8ea8`.
  - "new": no source idea; proposed here.

## 1. Thesis

**The bet.** Every Agent stays a `claude` or `codex` process in a pocketd Terminal (CONTEXT.md "Agent"). Pocket becomes the cockpit around those Terminals:
- a Timeline view rendered from the timelines pocketd already builds;
- a composer that drives the live Agent: claude by typing into its PTY, codex by `turn/start` on the shared thread (D4);
- the missing basics: alerts, terminal surface, git review, restore, trust and install.

**Why Terminals should be the only runtime:**
1. **The GUI parts are cheap on top of what exists.**
   - pocketd already builds a timeline per Agent (P packages/pocketd/internal/timeline/timeline.go), and the desktop already downloads it (P packages/desktop/crates/agents/src/agents.rs:288).
   - What is missing is a renderer (14-1, L) and a composer (14-2, M). A structured driver would be 14-6 (XL) [R14 ideas].
2. **Codex gets structured turn control without a second runtime.**
   - pocketd is already a client of the account app-server (P packages/pocketd/internal/codex/rpc.go:59).
   - A `turn/start` sent from pocketd shows in the TUI (P docs/spike-remote-sessions.md:75) [R03 §9].
3. **Headless claude costs more than it returns.** stream-json can't attach to a running TUI [R03 §9]. Going headless therefore means:
   - a second process with a handoff;
   - a per-session exclusivity lock and hook double-asks;
   - the undocumented `--permission-prompt-tool stdio`;
   - plan terms for driving `claude --print` from a third-party GUI, which nobody has researched;
   - a CONTEXT and wire break, because `terminalId` becomes optional [R03 §9, R03 risks].
   - For comparison, MonoCode kills and respawns claude on every model, effort or mode change [R10 TL;DR].
4. **The TUI keeps what headless loses:** dialogs, slash commands and plugins [R03 risks].
5. **Pocket's worst gaps are not the chat view:**
   - No session restore, which the landscape treats as table stakes [R15 TL;DR].
   - No scrollback or selection [R16 ideas 16-1, 16-4].
   - A daemon listening on every interface (P packages/pocketd/cmd/pocketd/serve.go:46) and an agent self-approval path [R18 F1].
   - A non-author can't install Pocket [R19 TL;DR].
   - This plan spends the horizon on those gaps.

**What we accept:**
- Claude timeline items arrive as whole blocks from the JSONL tail. Only codex streams deltas [R14 F4].
- The claude composer injects keystrokes: text, 150 ms, then CR (P packages/pocketd/internal/terminal/terminal.go:276-287). It refuses once the Agent has left the foreground (P packages/pocketd/internal/daemon/presence.go:119-128).
- Answering AskUserQuestion for claude from a panel is unproven [R05 risks].
- The claude context window may stay hidden, per D7 [R05 risks].
- No Sessions without a Terminal, no cloud. Remote reach is Tailscale only.

**Against the field:**

| Rival | Their bet | Pocket wins on | Pocket loses on |
|---|---|---|---|
| Zeron | Headless structured protocols, its own transcript and composer, 9 agents [R01 TL;DR] | Real approvals: Zeron auto-allows every tool except AskUserQuestion, so its "Needs you" means questions only [R01 TL;DR]. TUI fidelity. | Claude token streaming; agent count; release pace of 162 releases in 70 days [R01 TL;DR] |
| MonoCode | Headless over 10 CLIs; SQLite transcripts [R09 TL;DR; R10 TL;DR] | Sessions live in an always-on daemon; a mode change never respawns the agent [R10 TL;DR]; iPhone | Breadth; automations; quick composer [R09 TL;DR] |
| Superset | Electron; 20+ agents; daemon terminals with Resume Args. The iPhone app needs Pro at $20/mo and iOS 26+, and runs through a relay with no documented E2EE [R15 F1] | Free phone app; no relay; status for any claude or codex started in a Pocket Terminal, where Superset reports status only for agents launched through its wrappers [R15 F1] | Agent breadth; PR and CI views; restore (E10 closes this) |
| Happy | MIT; E2EE relay; the `happy claude` wrapper [R15 F2] | No wrapper: a plain `claude` or `codex` is detected by a 250 ms foreground poll (P packages/pocketd/internal/daemon/watch.go:13); desktop fleet view plus a real terminal | Reach off the tailnet; Android; voice |
| First-party | `claude remote-control`: Pro+, a second client takes over the session, polls Anthropic [R15 F5]. ChatGPT "Control this Mac" [R15 F6] | Both providers in one fleet view; no vendor relay; terminal fidelity [R15 risks "Vendor squeeze"] | Included in the plan; nothing to install; push that the agent decides to send [R15 F5] |

## 2. Target user and jobs

**Target user.** A solo developer on one Mac who:
- runs 3–8 concurrent claude and codex Sessions across Worktrees;
- lives in the terminal;
- checks in from an iPhone over Tailscale.

This is Zeron's target minus the multi-machine part [R01 TL;DR], limited by D13.

| # | Job | Today | Epics |
|---|---|---|---|
| J1 | When an Agent Needs you, know within seconds on any device and answer without hunting | Notifications have no actions (P packages/desktop/crates/pocket/src/main.rs:349). No dock badge, no sound [R14 idea 14-10]. No phone push [R14 idea 14-9] | E5, E7, E9 |
| J2 | Read what a turn did and reply without digging through scrollback | No desktop timeline or composer [R14 ideas 14-1, 14-2]. No scrollback [R16 F1] | E3, E4, E6 |
| J3 | Start an Agent in the right Worktree with explicit access, from the Mac or the phone | Codex gets `--full-auto` (P packages/desktop/crates/pocket/src/forms.rs:412). A failed spawn falls back to the shell with no notice (P packages/desktop/crates/daemon/src/daemon.rs:99-115). The phone can't spawn (P packages/pocketd/internal/proto/golden_test.go:102) | E1, E8 |
| J4 | Review what a turn changed and send comments back to the Agent | No latest-turn diff scope; comments can't become a prompt [R06 ideas 06-1, 06-2] | E11 |
| J5 | Keep going after a pocketd or Mac restart, or on a fresh install | No restore [R15 TL;DR]. Install takes 12 manual steps [R19 F7] | E10, E14 |

**Constraint on J1–J3:** no Agent can approve its own request, and no remote peer beyond Tailscale can reach pocketd (E2).

## 3. Rules every epic follows

- **Two drivers only:**
  - `termDriver` (P packages/pocketd/internal/daemon/presence.go:113-155);
  - the codex thread driver (E4).
  - No headless child processes.
- **New desktop surfaces are their own GPUI entities or crates.** Add no fields to `Desktop`, which already has 77 (P packages/desktop/crates/pocket/src/main.rs:101) [R14 F6].
- **Protocol changes update Go and TS together** and regenerate the goldens with `go test ./internal/proto -update` [R14 F4]. After E2, versions are negotiated as a range.
- **Any text that reaches a PTY** passes 18-9 sanitation.
- **Vocabulary follows CONTEXT.md.** The new surface is the "Timeline view", because CONTEXT.md avoids "transcript" for Conversation.
- **D1, D2, D6, D7 and D11 bind every UI change.**

## 4. Milestones

| M | Weeks | Epics | Exit criteria |
|---|---|---|---|
| M0 fix-now | 1–2 | E1, E2 | D12 bugs closed, each with a test. A process in a Terminal can't resolve its own permission. pocketd listens only on loopback and Tailscale. The phone pairs by QR |
| M1 | 3–4 | E3, E4, E5, E6 | A claude Session and a codex Session can be read and driven from the desktop Timeline view. Approvals work from notifications. Terminal scrollback, select, copy and paste work |
| M2 | 5–6 | E7, E8, E9, E10 | Questions and approvals are answered in panels. The phone starts a Session. Push works. Sessions come back after a pocketd restart |
| M3 | 7–8 | E11, E12, E13, E14 | Review comments reach the Agent. Palette, find and links work. The design system is tokenized. A signed app and a TestFlight build ship |

**Dependencies:**
- E1 → everything.
- E2 → E3, E4, E7, E8, E9.
- E3 → E4 → E7, E11.
- E5 → E9.
- E8 → E10.
- E6 → E12.
- E3, E4 and E5 → E13.
- E2, E9 and E10 → E14.

**Cut order if late:**
1. 07-2 dark theme
2. 19-8 and 19-9 updater
3. 17-4 and 17-7 canvas and model card
4. 14-13 `Desktop` split
5. 06-8 PR badge
6. 16-13 phone terminal

## 5. Epics

### E1 `fix-now`: Fix-now bugs on a clean base · M0 · M

- **Goal:** close the D12 bugs; E2 closes self-approval. Start measuring.
- **Ideas:** 14-24, 17-1, 17-2, 16-3, 05-2 (phone), 14-23 (CI half).
- **Scope:**
  - **14-24 rebase.** Plans target `5bc8ea8`. The main checkout has uncommitted edits in pocket/src/{capture,diff,explore,main,overlay,view}.rs and workspace/src/workspace.rs (git status, 2026-09-30) [R14 F12]. Land or shelve them before starting any epic that touches those files.
  - **17-1, 17-2.** Replace `--full-auto` (P packages/desktop/crates/pocket/src/forms.rs:410-412) with `-s workspace-write -a on-request`. Every mode sends both axes explicitly; codex Plan is disabled [R17 ideas 17-1, 17-2].
  - **16-3.** Turn ligatures off (`liga`/`calt`/`dlig` = 0). The live bug: `codex --yolo` renders as `codex--yolo` [R16 idea 16-3].
  - **05-2, phone half (Zeron #406).** Today the button is Stop whenever the Agent is Working (P packages/app/src/components/Composer.tsx:49-55). New rule: Send whenever there is text, Stop only when the field is empty and the Agent is Working; return never stops [R05 idea 05-2].
  - **Phone permission slot (not in D12; S).** A second request overwrites the first (P packages/app/src/session.tsx:61-62) [R08 F9]. Keep requests in a list keyed by `requestId`.
  - **14-23, CI only.** Actions run `go test ./...`, `cargo test`, clippy and the protocol golden decode. The bundle half moves to E14.
  - **Local journal (new).** pocketd appends `{ts, agentId, event, status, seen, origin}` for each transition, prompt and resolve to `~/.coding-pocket/journal.jsonl`. `pocketd stats` prints the metrics in §8. This gives a baseline before M1.
- **Deps:** none.
- **Done when:**
  - argv tests cover every provider and access pair;
  - the ligature fix has a render test;
  - the phone composer has a unit test;
  - CI is green;
  - the journal has been written for a week.

### E2 `trust-baseline`: D10 prerequisites; closes self-approval · M0 · L

- **Goal:** make drive, spawn and phone reach safe before anything widens them.
- **Ideas:** 18-1, 18-2, 18-3, 18-4, 19-11, 18-6, 18-8, 18-7, 18-9, 19-10.
- **Today:**
  - pocketd listens on every interface (P packages/pocketd/cmd/pocketd/serve.go:46).
  - The token sits in plaintext in `config.json` (P packages/pocketd/internal/config/config.go:52-61).
  - The origin check is off (`InsecureSkipVerify`) [R14 F4].
  - `POCKETD_SOCK` is set in every Terminal, and ops accepts spawn, prompt and input from any process [R11 TL;DR].
- **Order** [R18 F6]:
  1. **18-1** Bind loopback plus the Tailscale IPs, re-checked every 30 s.
  2. **18-2** Handshake hardening: Host allowlist, 4 KiB pre-auth limit.
  3. **18-3** Per-device 32-byte tokens, stored as sha256 hashes.
  4. **18-4 + 19-11** One-time QR code and the `codingpocket://pair` deep link.
  5. **18-6** The desktop talks over the ops socket as owner, verified by peer check, and stops reading the token.
  6. **18-8** `LOCAL_PEERPID` ancestry: processes that descend from a Terminal get no `approve` scope.
  7. **18-7** Scopes enforced in dispatch: observe, drive, approve, spawn and files.
  8. **18-9** Prompt sanitation:
     - strip C0, ESC and DEL;
     - cap at 64 KiB;
     - reject a leading `!` or `/` in prompts that come from an agent.
- **19-10 range negotiation.** Replace the exact `== 3` check (P packages/pocketd/internal/wsserver/wsserver.go:122) with min/max negotiation, because every later epic adds messages.
- **Deps:** E1.
- **Done when:**
  - An integration test starts a child in a Terminal. It fails to resolve its own permission over ops and over WS.
  - No plaintext token remains on disk.
  - `lsof` shows no listener on LAN interfaces.
  - The phone pairs by scanning the QR.

### E3 `timeline-view`: Desktop Timeline view · M1 · L

- **Goal:** read any Session's Conversation on the desktop without scrolling its Terminal. The Terminal keeps running.
- **Ideas:** 14-3, 14-1, 03-4 (view half), 03-15, 05-1, 05-10, 05-12, 05-17, 14-5, 02-3.
- **Today:** the desktop fetches the newest 500 items (P packages/desktop/crates/agents/src/agents.rs:288) and decodes only a subset. It drops tool status, output, diff, tasks and plan [R14 F5], and uses the rest only for context-left and last-edit (P agents.rs:155-179).
- **Scope:**
  - **14-3** Decode the full v3 timeline in Rust.
  - **14-1** The view: user bubbles, assistant markdown, thinking, tool groups, tasks, plan, compact and result rows.
  - **Tool groups:**
    - **03-15 / 05-1** Summary row, e.g. "Ran 3 commands · edited 2 files · 1 failed".
    - **05-10** Groups stay open while Working and fold once settled.
  - **05-12** Collapse long prompts.
  - **05-17** Jump pill.
  - **14-5** `beforeSeq` paging and a memory cap. `Page` is forward-only today (P packages/pocketd/internal/timeline/timeline.go:155-167).
  - **02-3** Frames bounded by bytes.
  - **Placement:** a Terminal ↔ Timeline toggle on the same Session. Toggling never resizes the PTY, because the last client to resize wins [R14 F4]. Proposed binding: ⌘⇧T. It is free in P main.rs:1062-1071 and outside D6's set.
- **Limit:** claude arrives as whole blocks, so the veil and mend (05-19) are not built for claude [R05 risks].
- **Deps:** E2, because 18-6 changes the desktop's transport.
- **Done when:**
  - claude and codex Sessions both render;
  - paging reaches item 1 of a 2k-item Session;
  - the newest page paints in under 100 ms (budget from [R02 idea 02-10]).

### E4 `live-composer`: Composer that drives the live Agent · M1 · L

- **Goal:** Send, Steer, Queue and Stop from the Timeline view and the phone, in the same Conversation the TUI shows.
- **Ideas:** 14-2, 03-1, 03-5, 05-3, 05-2 (desktop), 05-9, 02-5, 05-11, 03-6, 05-7, 14-21, 05-8, 03-10, 03-12.
- **Driver matrix** [R03 §9; R05 idea 05-3]:

| Action | claude (`termDriver`) | codex (thread driver, new) |
|---|---|---|
| Send, Idle | Type, 150 ms, CR (P terminal.go:276-287) | `turn/start {threadId, input, summary:"auto"}` |
| Steer, Working | PTY typing (TUI mid-turn behaviour to verify [R05 risks]) | `turn/steer {expectedTurnId}`; if rejected, hold until `turn/completed` |
| Send next | pocketd holds the text until the Stop hook, then Send | Hold until `turn/completed`, then `turn/start` |
| Send now | Esc (`0x1b`), then Send | `turn/interrupt`, then `turn/start` |
| Stop | Esc | `turn/interrupt {threadId, turnId}` |
| Compact | Types `/compact` | Types `/compact` (unchanged) |

- **Scope:**
  - **03-1 turnId.** Record `turnId` from `turn/started`, which `Notify` ignores today (P packages/pocketd/internal/codex/session.go:151-169).
  - **03-1 policy.** Omit `approvalPolicy`/`sandboxPolicy` only if the spike proves the thread inherits them. Otherwise send them from the Session's access mode.
  - **03-5 / 05-3 queue.** A per-Agent queue in pocketd, exposed as `agent.queue.*` messages. The phone UI for it is in E9.
  - **05-2** Desktop Send/Queue/Stop morph; Enter never stops.
  - **05-9 / 02-5 delivery.**
    - Optimistic bubble, confirmed by an ack or error on `agent.prompt`.
    - On failure: "Not delivered — retry".
    - pocketd dedupes on `(clientId, id)`.
  - **05-11** Drafts per Session; multi-line input.
  - **03-6 / 05-7 / 14-21 context ring.**
    - Codex window comes from `thread/tokenUsage/updated` [R03 §9].
    - The claude ring stays hidden until the Agent reports a window, per D7. JSONL carries the model, not the window [R05 risks].
    - Delete the hard-coded 200k (P packages/desktop/crates/agents/src/agents.rs:57).
    - `AgentSummary` gains branch, worktree, tokens and window.
  - **05-8** Working trailer with elapsed time.
  - **03-10** Codex reasoning summary rendered as thinking.
  - **03-12** Version gate. The codex thread driver runs only on verified versions; any other version falls back to `termDriver` typing.
- **First task (S spike),** run against the installed codex:
  - side effects of `turn/start` sent from `codex_app_server_daemon` (P packages/pocketd/internal/codex/rpc.go:59);
  - whether approvals fan out to the TUI;
  - whether policy is inherited [R03 risks];
  - on claude: whether text typed mid-turn is taken as a steer.
- **Deps:** E2 (18-7, 18-9), E3.
- **Done when:**
  - sending while Working never interrupts;
  - a codex turn sent from the composer shows in the TUI;
  - an undelivered prompt is visible and retryable;
  - the journal counts failed sends.

### E5 `alerts`: Needs you and Done alerts · M1 · M

- **Goal:** know within seconds which Session Needs you, and answer a permission from the notification.
- **Ideas:** 14-10, 13-20, 09-1, 05-5, 05-6, 07-6, 04-9, 04-1, 12-3, 04-2, 04-3, 16-15, 15-3, 15-7.
- **Scope:**
  - **14-10 + 13-20 notifications.**
    - Allow / Deny on permission notifications; today `actions` is empty (P packages/desktop/crates/pocket/src/main.rs:349).
    - Clicking jumps to the Session.
    - The body is the ask: `Approve: <tool>`, the question, or the reply's first paragraph (≤240 chars).
  - **09-1** Dock badge showing the count of Needs you.
  - **05-5 detector rules:**
    - first sight sets a silent baseline;
    - Done fires only if fresh (≤45 s) and not an interrupt;
    - alerts coalesce over 250 ms.
  - **D11:**
    - no banner for a Seen Session;
    - three sound cues (05-6 / 07-6 / 04-9) play on non-Seen transitions, even while the app is focused;
    - each cue has a toggle, default on;
    - check sound asset provenance [R05 risks].
  - **D1** colours on 12-3 rows.
  - **D2 / 04-1** Stable sidebar order. Urgency order only in ⌘J, the palette group, the dock badge and notifications.
  - **D6 / 04-2** ⌘1–9 select Sessions in visible sidebar order, with hint chips while ⌘ is held.
  - **04-3** Ctrl+Tab cycles Sessions.
  - **16-15 + 15-3 terminal signals.**
    - pocketd parses OSC title, bell (shown as a tab dot) and the OSC 9/99/777 notification forms. It ignores `OSC 9;4`, which is progress [R15 risks].
    - Adds `pocketd notify`.
  - **15-7** Keep-awake assertion while any Agent is Working or Needs you.
- **Deps:** E1.
- **Done when:**
  - a permission is approved from a banner;
  - the badge equals the number of Needs you;
  - a Seen Session never raises a banner.

### E6 `terminal-surface`: The Terminal can stay the runtime · M1 · M

- **Goal:** fix the terminal basics a cockpit can't live without.
- **Ideas:** 16-1, 16-2, 16-4, 16-5, 16-6, 16-10, 16-11, 16-16, 13-12.
- **Scope:**
  - **16-1** 10k-line scrollback on both VTs.
  - **16-2** Wheel and trackpad: remainder accumulation; snap to bottom on input.
  - **16-4** Selection: 1, 2 and 3 clicks; Shift-extend; edge autoscroll.
  - **16-5** ⌘C copies plain text and is never sent as ^C.
  - **16-6** ⌘V pastes through `ghostty_paste_encode`, with a confirm for unsafe multi-line pastes.
  - **16-10** IME preedit.
  - **16-11** 80 ms resize debounce.
  - **16-16** Cursor shape and blink.
  - **13-12** Mac editing keys: ⌥←/→, ⌘←/→, ⌘⌫.
  - The libghostty-vt pin already provides scrollback, selection and paste encoding, so the work is Rust plumbing [R16 F2].
- **Deps:** E1 (16-3).
- **Done when:** each of the above has a test in termview or an input test in the keys crate.

### E7 `structured-answers`: Answer without the Terminal · M2 · M

- **Goal:** answer permissions, AskUserQuestion and ExitPlanMode in desktop and phone panels.
- **Ideas:** 14-11, 05-4, 01-13, 08-13.
- **Today:**
  - pocketd hooks PreToolUse `AskUserQuestion|ExitPlanMode` (P packages/pocketd/internal/daemon/plugin.go:13), but only PermissionRequest gets a reply [R14 F4].
  - Codex sends only accept or decline (P packages/pocketd/internal/codex/session.go:272-279).
- **Scope:**
  - **14-11** `question.request` built from the PreToolUse input.
  - **05-4 / 01-13 question wizard.** It replaces the composer slot:
    - keys 1–9 pick an option;
    - single choices auto-advance after 220 ms;
    - "Other…" takes free text.
  - **08-13 approval panel.**
    - One page per open request ("Bash · 1 of 2"), numbered options, feedback.
    - Replaces the phone's modal sheet (P packages/app/src/components/PermissionSheet.tsx:32-100) [R08 F9].
  - **Codex "Always allow"** replies `acceptForSession` [R03 §9].
- **Claude answer path** [R05 risks]. Try each in order and ship the first that passes on the gated version:
  1. A PreToolUse reply that carries the answers.
  2. Keystrokes into the TUI dialog through `termDriver`.
  3. A read-only question plus "Answer in terminal", which jumps to the Terminal.
- **Deps:** E2 (approve scope), E3, E4.
- **Done when:** a 3-question AskUserQuestion is answered from the phone, or the fallback is documented with evidence.

### E8 `launchspec`: Start Sessions from desktop and phone · M2 · L

- **Goal:** start the right Agent in the right checkout with explicit access, and never fail silently.
- **Ideas:** 17-10, 11-4, 18-10, 17-3, 17-5, 17-6, 17-8, 17-9, 17-11, 17-12, 08-21 (reduced), 14-19, 13-15.
- **Today:**
  - `agent.create` is rejected (P packages/pocketd/internal/proto/golden_test.go:102).
  - A failed spawn falls back to the shell with no notice (P packages/desktop/crates/daemon/src/daemon.rs:99-115) [R17 idea 17-9].
  - Worktree operations exist only in the desktop [R11 TL;DR].
- **Scope:**
  - **17-10 / D5 LaunchSpec.**
    - `agent.create` carries {project, checkout, provider, model, effort, access, plan, prompt}.
    - pocketd builds argv from the access table [R17 F14].
    - Full access is never persisted.
  - **11-4 worktrees in pocketd.** pocketd creates, lists and removes Worktrees; a new one gets a new branch of the same name.
    - Deleting a Worktree keeps its branch (D8). 06-14's `branch -D` is not ported.
    - **13-15** The delete dialog states any unpushed commits.
  - **Project registry in pocketd,** a prerequisite for 18-10 [R18 idea 18-10].
  - **18-10 phone limits:**
    - claude or codex only;
    - a registered project;
    - prompt;
    - Ask or Plan only;
    - no cmd, env or cwd.
  - **Desktop sheet:**
    - **17-3** Access picker.
    - **17-5** Picks persist.
    - **17-6** Checkout chip.
    - **17-9** Offline and spawn-failure states shown before the sheet closes.
  - **Launch argv:**
    - **17-8** CLI probe in the login shell.
    - **17-11** Quick rows in the tab menu.
    - **17-12** `claude -n`.
  - **08-21 (reduced)** Phone sheet: project, provider and checkout chips, plus the prompt.
  - **Deferred inside the epic:** 17-4 canvas, 17-7 model card, 17-14.
- **Deps:** E1, E2 (spawn scope).
- **Done when:**
  - the phone starts a codex Session in a new Worktree;
  - a bad argv shows an error on the device that sent it.

### E9 `phone-reach`: Phone buzzes and answers · M2 · L

- **Goal:** Needs you and Done reach the phone, which can follow a Session and reply without the Mac.
- **Ideas:** 19-12, 08-2, 08-3, 08-4, 08-5, 19-13, 18-13, 15-4, 08-6, 08-7, 08-8, 08-9, 08-10, 08-11, 16-13, 14-17, 14-18, 18-5, 18-14.
- **Scope:**
  - **Push:**
    - **D3 / 19-12** Expo push from pocketd on Seen-aware transitions (08-4).
    - **08-3** One collapse id per Session.
    - **08-2** `push.register`.
    - **08-5** `pocketd push status`, which also prunes dead tokens.
    - **19-13** Pre-prompt card before asking for permission.
    - **18-13** Lock-screen privacy.
    - **15-4** Agent-requested push through `pocketd notify`, gated by 18-8.
  - **Composer and connection:**
    - The queue panel on E4's semantics.
    - **08-8** Outbox with retry.
    - **08-9** Drafts.
    - **08-6 / 08-7** Offline pill after a 4 s grace; redial when the app becomes active.
  - **Session list:**
    - **D2** An urgency-sorted Needs you list.
    - **08-10 / 08-11** Row redesign and "2 Working · 1 Needs you", in D1 colours.
  - **Terminal:** 16-13 read-only phone terminal (`vt.Frame` + `terminal.watch`, ≤4 Hz, never resizes).
  - **App hygiene:**
    - **14-18** One token set and a navigation stack.
    - **14-17** Dead buttons get wired or removed (P packages/app/src/screens/ChatScreen.tsx:147-159).
  - **Devices and storage:**
    - **18-5** Desktop Devices sheet.
    - **18-14** Keychain `WHEN_UNLOCKED_THIS_DEVICE_ONLY` and Unpair.
- **Prereq:** a paid ADP account and an EAS project [R19 F6].
- **Deps:** E2, E4, E5.
- **Done when:**
  - a locked phone buzzes once per transition;
  - nothing is pushed for a Seen Session;
  - a reply sent while Working is queued, not an interrupt.

### E10 `session-restore`: Survive restarts · M2 · L

- **Goal:** after a pocketd or Mac restart, each Session returns to its Worktree and its Conversation, with the layout intact.
- **Ideas:** 15-1, 14-4, 02-11, 14-15, 04-7, 19-3, 19-4.
- **Today:**
  - pocketd persists only `config.json` and its plugin [R15 TL;DR].
  - Timelines live in memory only [R14 F4].
  - The desktop exits when pocketd is missing (P packages/desktop/crates/pocket/src/main.rs:1052).
- **Scope:**
  - **15-1 restore.**
    - Persist {terminalId, cwd, argv, provider, providerSessionId}.
    - Relaunch with resume argv built by E8's argv builder.
    - Allow only claude and codex; never persist env [R15 risks].
  - **14-4 via 02-11** A timeline journal per Agent, reloaded for Sessions that have ended.
  - **14-15 / 04-7** Persist widths, window geometry, layout mode and selection.
  - **19-3** The desktop waits and reconnects: "Starting Pocket's terminal service…".
  - **19-4** pocketd adds `--version`, `status`, a single-instance flock and a rotating log.
- **Deps:** E8.
- **Done when:** after `kill pocketd`, every claude and codex Session with a `providerSessionId` is back in its Conversation.

### E11 `git-review`: Review a turn, send comments to the Agent · M3 · L

- **Goal:** see what a turn changed and send line comments back as a prompt.
- **Ideas:** 06-1, 06-2, 13-3, 06-13, 06-8, 06-7, 13-6, 06-6, 06-5, 14-14 (git half).
- **Scope:**
  - **06-1** "Latest turn" scope: snapshot the tree (temp index + `write-tree`) on UserPromptSubmit for claude and on `turn/started` for codex.
  - **06-2 / 13-3 / 06-13** Batched comments become one prompt through E4's queue (Send next).
  - **06-8** PR badge per Worktree, from `gh`.
  - **Safety:**
    - **06-7** Discard refuses if the tree changed after the confirm opened.
    - **13-6** Git safety confirms (push from the default branch, amend of a pushed head).
    - **06-6** Snapshot and diff caps are shown as notices.
  - **06-5** fs-watch refresh with a 500 ms debounce replaces git polling [R14 idea 14-14].
- **Collision:** this epic touches the uncommitted changes, diff and explore files [R14 F12]. It runs only after E1 lands them.
- **Deps:** E1, E4.
- **Done when:** three comments on two files arrive as one prompt when the turn ends.

### E12 `shell`: Palette, find, links, less polling · M3 · L

- **Goal:** finish the shell around the Terminal and stop `Desktop` from growing.
- **Ideas:** 14-12, 12-12, 16-8, 16-14, 16-9, 16-7, 16-12, 16-17, 04-5, 04-11, 13-13, 14-14, 14-13 (scoped).
- **Scope:**
  - **Palette (14-12 / 12-12):**
    - fuzzy matching;
    - `>` for commands;
    - an action registry;
    - worktrees and projects;
    - a Needs you group (D2).
  - **Terminal (second wave):**
    - **16-8** Links (OSC 8, URLs, `path:line`) with ⌘-click.
    - **16-14** ⌘F find.
    - **16-9** Mouse reporting.
    - **16-7** Scrollbar.
    - **16-12** Tab drag and middle-click close.
    - **16-17** Resizable split rows.
  - **Session chrome:**
    - **04-5** Session menu, with Copy resume command.
    - **04-11** Titlebar names the Session.
    - **13-13** Close-tab confirm.
  - **14-14** Terminal-list push events replace the 1 s poll (P packages/desktop/crates/pocket/src/main.rs:1117).
  - **14-13 (scoped)** Move the sidebar and palette out of `Desktop`.
- **Deps:** E5, E6.

### E13 `design-system`: One token source · M3 · L

- **Goal:** desktop and phone share semantic tokens, and D1 is encoded once.
- **Ideas:** 07-1, 07-3, 07-4, 12-2, 07-7, 07-10, 07-11, 07-12, 12-16; 07-2 is a stretch.
- **Scope:**
  - **07-1** A runtime `Palette` replaces the `u32` consts.
  - **07-3** A contrast unit test.
  - **07-4** Status semantics, in D1 colours.
  - **12-2 / 07-7** Motion tokens that zero out under reduced motion.
  - **07-10** ANSI 16 and terminal colours.
  - **07-11** Diff wash and change bar.
  - **07-12** Phone palette merge, on top of 14-18.
  - **12-16** Empty states.
  - **Cut first:** 07-2 dark theme and a System/Light/Dark setting.
- **Deps:** E3, E4 and E5 surfaces exist, so tokenization happens once.

### E14 `distribution`: A non-author can install it · M3 · L

- **Goal:** download, pair and run on a Mac and an iPhone, with no toolchains.
- **Ideas:** 19-16, 19-1, 14-23 (bundle half), 19-2, 19-5, 19-6, 19-7, 19-17, 19-18, 19-19, 19-20, 19-14, 19-15; 19-8 and 19-9 are stretch.
- **Today:** 12 manual steps [R19 F7].
- **Scope:**
  - **19-16** MIT licence and third-party notices (author decision).
  - **19-1** A signed, notarized `Pocket.app` embedding `pocket` and `pocketd`. libghostty-vt is static [R19 TL;DR].
  - **19-2** An SMAppService LaunchAgent that never boots out a running pocketd.
  - **19-5** A gate for translocated and DMG locations.
  - **19-6** Agent check, with install in a visible Terminal.
  - **19-7** The attach explainer.
  - **19-17** Pairing detects Tailscale and warns on LAN fallback.
  - **19-18 / 19-19 / 19-20** TCC purpose strings, dev isolation, and "Stop terminal service".
  - **19-14** TestFlight through EAS.
  - **19-15** BUILD.md.
  - **Stretch:** 19-8 updater and 19-9 restart policy (automatic only at 0 Terminals).
- **Prereq:** paid ADP, a Developer ID and an ASC API key [R19 idea 19-1].
- **Deps:** E2, E9, E10.
- **Done when:** a clean macOS user goes from download to a Session with Needs you on the phone in ≤10 min.

## 6. What we deliberately don't build

| Out | Ideas | Why |
|---|---|---|
| Headless Sessions, either provider; the private stdio app-server | 03-2, 03-3, 03-9, 10-1, 10-2, 10-7, 10-8, 10-12, 14-6 (claude half), 14-7, 17-13, 17-17 | The thesis; D4's headless branch is not adopted |
| Features that need headless | 03-7 (claude discovery), 03-13, 03-14, 10-14, 10-16 | They need 03-2 [R03 ideas]. Plan first = `--permission-mode plan` argv only [R17 F14] |
| Claude streaming polish | 05-18, 05-19, 05-21, 01-20, 13-10 | Claude arrives as whole blocks [R05 risks] |
| Relay, E2EE, LAN mode | 02-1, 08-22, 15-12, 18-16, 18-17 | Reach is Tailscale (18-1). 18-16 is L |
| Providers beyond claude and codex | 14-22, 15-2 | Depth over breadth in the horizon |
| Orchestration, automations, MCP control | 01-14, 03-16, 09-13, 09-14, 09-17, 11-1, 11-5–11-11 | Needs 18-8 grants in production first [R18 F7] |
| External inbox, notes, kanban | 11-16, 11-17 | Low fit [R11 TL;DR] |
| Live Activity, HUD, iPad, lock-screen Allow/Deny, Android/Windows/Linux | 15-5, 15-6, 08-20, 08-19 | L each; D13 |
| Browser, previews, editor, history graph | 06-10, 01-23, 06-9, 06-19, 01-21, 06-11 | Outside the cockpit |
| Projectless, host picker, multi-host, SSH hosts | 17-15, 17-16, 02-13, 09-18, 10-19 | One Mac (D13) |
| Live pocketd upgrade; CRDT/HLC | 15-14, 02-14, 02-15 | 15-1 restore is enough [R15 risks] |
| Account profiles, plan-usage meters | 09-4, 09-5, 09-8, 10-18 | Not a top-5 job |
| Cosmetics | 05-24, 05-25, 07-8, 07-13, 07-15, 12-18, 13-19 | No job served |
| Direct APNs | 08-1, 19-22 | D3 chose Expo |

## 7. Risks and de-risking

| Risk | Evidence | De-risk | Epic |
|---|---|---|---|
| The codex app-server is `[experimental]` and versions drift (0.153.4 / ≥0.157 / 0.159.0) | [R03 risks] | 03-12 gate; per-version schema generated in CI (`codex app-server generate-json-schema`); unknown version → `termDriver` typing | E4, E1 |
| `turn/start` from `codex_app_server_daemon` may have side effects; approval fan-out and policy inheritance are unverified | [R03 §9 item 7; R03 risks] | The spike is E4's first task. If policy is not inherited, send it from the access mode. If the turn is rejected, keep typing | E4 |
| Claude PTY typing: a 150 ms heuristic, foreground refusal, unverified mid-turn behaviour; merging with text half-typed in the TUI is also unverified | P terminal.go:276-287; P presence.go:119; [R05 risks] | Ack/err with retry (05-9); the journal counts failures; the existing foreground check stays | E4 |
| Undocumented or hidden flags | `--permission-prompt-tool stdio` [R03 risks]; `--permission-mode default` hidden in claude 2.1.285, where `manual` is listed [R03 risks] | The first is avoided (no headless). For the second, 03-12 reads `--help` to pick the accepted name, with an argv test per version | E1, E8 |
| No known way to answer claude AskUserQuestion from a panel | [R05 risks] | E7's three-step fallback down to "Answer in terminal" | E7 |
| Claude context window unknown | [R05 risks] | D7: hide it; no guessed model table | E4 |
| Vendor squeeze | [R15 risks] | Compete on the multi-provider fleet, terminal fidelity and no relay; don't chase chat polish | all |
| Collision with work in flight | Uncommitted edits in pocket/src and workspace (git status); [R14 F12] | E1 lands or shelves them; new surfaces are their own crates; E11 waits until M3 | E1 |
| New drive and spawn paths widen attack surface | [R18 F7] | D10 in M0; 18-7 scopes; the self-approval test runs in CI | E2 |
| Restore auto-runs argv | [R15 risks] | Allowlist claude and codex; never persist env | E10 |
| Push needs paid ADP and EAS | [R19 F6] | Owner decides in M0; fallback is desktop-only alerts | E9 |
| Claude skips hooks in untrusted folders, so the Agent is not attached | P packages/desktop/crates/pocket/src/status.rs:106 [R14 F4] | The Timeline view shows "not attached" with the trust hint (19-7) | E3, E14 |
| 14 epics in 8 weeks for one developer | — | Milestone exit gates; cut order in §4 | all |

## 8. Success metrics (local only)

Source: E1's `journal.jsonl` plus pocketd logs; no telemetry service. Baselines are taken in M0 week 2.

| Metric | Measured by | Baseline | Target by end of M3 |
|---|---|---|---|
| Time from Needs you to answer, p50/p90 | journal: Needs you → cleared | measured in M0 | p50 halved |
| Needs you answered without focusing that Terminal | journal `origin`: notification, panel, composer, phone | ≈0% on desktop (no actions, P main.rs:349) | ≥50% |
| Needs you expired by the broker's 10 min timeout (P packages/pocketd/internal/broker/broker.go:16) | journal | measured | 0/week with a paired phone |
| Prompt delivery failures | ack/err in the journal | measured | <1% of composer sends |
| Duplicate alerts per transition | detector log; `pocketd push status` (08-5) | — | 0 |
| Push latency, transition → Expo accepted, p90 | push log | — | <2 s |
| Sessions restored after a pocketd restart | restore log | 0% [R15 TL;DR] | 100% of those with a `providerSessionId` |
| Silent spawn failures | `agent.create` results | present (P daemon.rs:99-115) | 0 |
| Timeline view first paint, newest page | debug timing | — | <100 ms |
| Open D12 bugs | test suite | 4 | 0, each with a regression test |
| Trust invariants: no self-approval, no plaintext token, no LAN bind | CI integration tests | failing | passing on every commit |
| Fresh install to first phone alert | stopwatch, clean macOS user | 12 manual steps [R19 F7] | ≤10 min, no toolchains |
