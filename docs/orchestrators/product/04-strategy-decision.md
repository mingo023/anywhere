# 04 Strategy decision

- Date: 2026-09-30, revised after review (06-prd-review.md). Horizon: 8 weeks (M0 wk 1–3, M1 wk 2–7, M2 wk 5–8, M3 cut line); milestones overlap across lanes (05-roadmap §1). Solo dev + /implement agents.
- Inputs: strategies `strategy/A-terminal-cockpit.md`, `strategy/B-dual-mode.md`, `strategy/C-control-plane.md`; three judges (user value, engineering feasibility, differentiation/durability); research R01–R19; UX specs 02 (desktop) and 03 (phone); D1–D13.
- Citation keys:
  - `[A §n]`, `[B En]`, `[C §n]`: the strategy docs.
  - `[Rnn §x]`, `[Rnn idea nn-x]`: research reports.
  - `[UXD §n]` = 02-ux-spec-desktop.md, `[UXP §n]` = 03-ux-spec-phone.md.
  - `P path:L`: Pocket code at main `f8f7293`. Prefixes: `pk/` = `packages/desktop/crates/pocket/src/`, `pd/` = `packages/pocketd/`, `app/` = `packages/app/src/`.
- Sizes: S ≤2 dev-days, M 3–5, L 6–10, XL >10.

## 1. The three options

**A — Terminal-native cockpit.** Every Agent stays a `claude`/`codex` process in a pocketd Terminal; Pocket builds a cockpit around it: a desktop Timeline view rendered from the timelines pocketd already builds (P pd/internal/timeline/timeline.go:156), a composer that types into claude's PTY and sends `turn/start` to codex's shared Conversation (D4), plus alerts, terminal surface, git review, restore, trust and distribution [A §1]. Strength: the most visible UX value (Timeline, composer, alerts in M1). Weakness: ~95 dev-days against ~40 per lane, push gated behind Timeline (E9 → E4 → E3), an on-disk conversation journal (02-11), and citations taken at 5bc8ea8 that no longer resolve (`forms.rs`, `view.rs`, `termview.rs` are gone) [A §4–5].

**B — Dual-mode.** A plus a second Session kind: headless "chat" agents (Claude stream-json with `--permission-prompt-tool stdio`, a private Codex app-server) living in the same fleet view, so Steer/Queue, the question wizard, plan capture and Undo/Keep become exact instead of approximated over a PTY [B §1]. Strength: highest ceiling for Zeron/MonoCode parity. Weakness: rests on an undocumented Claude flag and unresearched account terms [R03 risks]; "B degrades to A" is false because B's chat epics are XL and consume A's budget; drops policy fields before verifying Codex inheritance; its ADR 0003 collides with the existing P docs/adr/0003-desktop-code-layout.md [B §3].

**C — Control plane.** pocketd becomes the always-on owner of state: device tokens and scopes, agent grants, a LaunchAgent, a project/worktree registry, `agent.create`, restore from provider files, coded prompt acks, push, an agent CLI/MCP, automations and an orchestrator [C §1, §3]. Strength: every later feature (phone spawn, restore, push, agents driving agents) gets one home; security first (D10); small additive protocol behind caps [C §3]. Weakness: no UI clone at all; phone Needs you for a question or plan dead-ends; desktop notifications "unchanged this horizon" [C §10.10]; restore claims no desktop change though the desktop exits when pocketd drops (P packages/desktop/crates/daemon/src/daemon.rs:172-183, P pk/main.rs:30, P pk/terminals.rs:90); legacy-token migration order undefined; keeps the orchestrator while dropping terminal selection; ~80 dev-days.

## 2. Judges' scores

Axes: value / feasibility / differentiation / risk. Higher is better; risk 10 = safest.

| Judge | A | B | C | Winner |
|---|---|---|---|---|
| User value | 8 / 5 / 6 / 5 = 24 | 6 / 3 / 8 / 3 = 20 | 6 / 7 / 7 / 7 = 27 | A |
| Engineering feasibility | 7 / 6 / 6 / 5 = 24 | 8 / 3 / 8 / 3 = 22 | 6 / 8 / 7 / 7 = 28 | C |
| Differentiation / durability | 7 / 5 / 5 / 5 = 22 | 7 / 3 / 3 / 3 = 16 | 6 / 5 / 8 / 7 = 26 | C |
| **Total** | **70** | **58** | **81** | **C** |

- C wins on feasibility and risk in all three rows; A wins on value in all three rows.
- B's feasibility and risk (3) in every row are disqualifying for a solo dev on an 8-week horizon.

## 3. Chosen: "C spine + A cockpit"

- **Spine (C).** Security → always-on → registry/launch → restore/drive/push, all in pocketd and `packages/protocol`, additive behind `hello.caps` [C §3 Protocol additions; R02 idea 02-9].
- **Cockpit (A).** Visible value every milestone: fix-now in M0; in M1 the Zeron/MonoCode look (E16: tokens, AA test, palette v2, menus, context meter), the phone shell (E17: copy, stable order, Up next, connectivity), terminal surface and attention (Allow/Deny notifications, badge, sounds, stable order, ⌘1–9); structured answers in M2; Timeline view and composer at the M3 cut line [A E3–E6; UXD §2.0; UXP §9].
- **Terminal stays the runtime.** No headless mode this horizon. Claude is a TUI driven by documented flags and hooks; Codex is driven over the account app-server (D4) [C §10.1; R03 §9].
- **Two lanes.**
  - Lane P: pocketd, `packages/protocol`, goldens and WS dispatch, one golden-touching PR at a time. Holds the pocketd halves of E02–E06 and E09–E12.
  - Lane C: desktop + phone (E01, E07, E08, E16, E17, and the client halves of E02–E04, E06, E09–E14). Lane C starts E16/E17 in week 2 once E01 merges.
  - Atomic exceptions: E03 PR1 and E05 PR1 change pocketd and a desktop crate together and run in lane P (05-roadmap §2).
- **Budget.** ~91 dev-days against 8 weeks × 2 lanes = 80. M0–M2 ≈ 81 (lane C ≈ 42, the critical path; lane P ≈ 39). M3 (≈10) is the cut line. Cut order: E15 (stretch) → S-items → E14 → E13; never cut M0, E16 or E17 (D43).
- **Risk accepted.** One reviewer for two lanes. Mitigated by the `scripts/check.sh` gate from E01 PR1 (D33; CI after owner Q3), ≤5 PRs per plan, and caps so each PR ships alone.

Why not pure C: judge 1 scored C lowest on value; the owner's prior is "ship visible UX value early" and the brief is to clone Zeron/MonoCode UI/UX. C's "desktop notifications unchanged" [C §10.10] and "no timeline" [C §6] leave the desktop exactly as it is for 8 weeks.

Why not pure A: A is ~95 dev-days, its push waits on the Timeline, and its journal writes conversation text to disk, violating C's invariant 3 [C §3 Invariants].

## 4. Grafted

| Graft | From | Lands in | Reason |
|---|---|---|---|
| Always-on (LaunchAgent, flock, log, stats) moved into M1 | A (judge 1) | E04 | Push, restore and phone reach all need pocketd alive [R02 idea 02-7; R19 idea 19-4] |
| Restore from provider files replaces A's journal | C E5 | E09 | No conversation text on disk [C §3 invariant 3; R15 idea 15-1] |
| Push decoupled from the Timeline | C E9 | E11 | A's chain E9 → E4 → E3 delays the top phone value [A §4] |
| LaunchSpec details: error codes, receipts, probe TTL, phone policy | C E7 | E06 | [R17 idea 17-10; R18 idea 18-10] |
| Hardening (bind, handshake, device tokens, pairing) | C E2/E3 | E02, E03 | D10 |
| Coded acks, dedupe, never type into a non-Attached agent | C E8 | E10 | [R02 idea 02-5; C §8] |
| `hello.caps` + protocol range | B | E02 | [R02 idea 02-9; R19 idea 19-10] |
| Capability matrix + conformance tests per CLI version | B | E10 | [R03 idea 03-12] |
| Acceptance test: codex TUI behind `less` | B | E10 | Proves `turn/start` needs no foreground [R03 §9] |
| Kill signal: low Timeline use after M3 → don't expand | B | metrics, E13 | Cheap guard on the most expensive UI bet |
| One-day headless probe doc | B | stretch S3 | Keeps the B door open with evidence, not code |
| `code` field on `frame.Error` | B | E06, E10 | Coded errors [C §3 invariant 4] |
| Undo/Keep | B | stretch S9 | Needs turn snapshots [R13 idea 13-1] |
| Timeline as a `Tab` variant in its own feature module | A E3 (judge 3) | E13 | Fits P packages/desktop/crates/workspace/src/workspace.rs:10-13 and ADR 0003 |
| Turn/start policy spike before the codex driver | judge 2 | E10 | Inheritance and originator side effects are unverified [R03 risks] |
| Notification Allow / Deny and stop as its own PR; badge + sounds in one PR | judge 1 | E08 PR4, PR5 | Allow/Deny needs no FFI (P pk/desktop/alerts.rs:86) but needs the dev bundle (E01); the badge needs objc FFI |
| Phone permissions map `Record<requestId>` | judge 1 | E01 | One slot overwrites concurrent asks (P app/session.tsx:43,61-64) |
| Claude answer ladder: PreToolUse reply → keystrokes → "Answer in the terminal" | judge 1 | E12 | Fixes the phone dead-end for questions and plans (P pd/internal/daemon/plugin.go:13) |
| Terminal S ideas + ⌘1–9 | judge 1 | E07, E08 | [R16 ideas 16-1…16-16; R04 idea 04-2] |
| Desktop reconnect | judge 3 | E04 | [R19 idea 19-3] |
| Re-anchored paths (forms.rs → modals/new_session.rs, etc.) | judge 2 | 05-roadmap §Path map | A/B/C cite 5bc8ea8/b9d14a1/86deb13 |
| 08-13 approval panel, `check.sh` (14-23 local half), 16-10, 16-11, 13-12 | judge 3 | E01, E07, E12 | S-sized, high value |

## 5. Rejected

| Rejected | Source | Why |
|---|---|---|
| Headless Claude chat agents | B E5, E7, E8 | Undocumented `--permission-prompt-tool stdio`; unresearched account terms; XL [R03 risks; R10 open questions] |
| Private Codex app-server | B | D4: only if headless is adopted |
| On-disk conversation journal | A (02-11) | Conversation text on disk; restore rebuilds from provider files instead [R02 idea 02-11; C §3] |
| Vocabulary and ADR edits for chat sessions | B §3 | No second Session kind; ADR 0003 is taken |
| ⌘⇧U jump to latest unread | B (15-15) | ⌘⇧J and palette "Go to Up next" walk Up next, including Done; ⌘J stays next Needs you (D6, D24) [UXD §8] |
| Automations, orchestrator, MCP server | C E11, E12, E10 (MCP half) | Agents driving agents before grants have soaked; plugin-dir MCP loading unverified [C §11]. The agent CLI half survives as E15, now stretch (D43) |
| Distribution (signed bundle, updater) | A E14 | Needs a paid ADP; the owner runs from source [R19 ideas 19-1, 19-2] |
| Git review epic (latest-turn diff, comments → prompt) | A E11 | L; needs pocketd turn snapshots [R06 idea 06-1]; over budget. Comments-as-prompt needs E14's composer first |
| Shell epic (find, links, polling rework) | A E12 | Split: terminal pieces into E07, polling into stretch S8 [R14 idea 14-14] |
| Relay / E2EE | A, C | Tailscale covers the owner; L [R18 idea 18-16] |
| Lock-screen Allow/Deny | A | L; background reconnect timing open [R08 idea 08-19] |
| "Desktop notifications unchanged" | C §10.10 | Allow/Deny is S (14-10) and the desk is where the owner is most of the day |
| "No timeline" | C §6 | Kept as E13 at the cut line, with a kill signal |

## 6. Fatal flaws named by the judges, and the answer

| Flaw | Option | Answer in this plan |
|---|---|---|
| Push gated behind Timeline | A | E11 depends on E02, E04 and E17 connectivity, not on the Timeline |
| ~95 dev-days | A | ~91 incl. the M3 cut line (≈10); M0–M2 ≈ 81 |
| Journal on disk | A | Rejected; E09 rebuilds from `transcriptPath` / `thread/resume` |
| Cuts the agent CLI on a weak reason | A | E15 keeps it as the first stretch item, gated by grants, never approve |
| Stale citations | A, B, C | Re-anchored to main `f8f7293`; plans re-anchor again |
| Undocumented flag, unresearched terms | B | No headless; S3 probe doc only |
| "Degrades to A" false; XL epics | B | No XL epic; largest is L ≤5 PRs |
| Drops policy fields before verifying inheritance | B | E10 spike first; argv always sends both axes (E01) |
| ADR 0003 collision | B | No ADR in this roadmap; the next one is 0004 |
| No UI clone | C | E16 look + E17 phone shell in M1, never cut; E07, E08 in M1; E12 panels; E13/E14 at the cut line |
| Phone question/plan dead-end | C | E12 answer ladder + ApprovalPanel |
| Desktop notifications unchanged | C | E08 |
| Restore "no desktop change" | C | E04 PR4 desktop reconnect; E09 PR5 re-adopt e2e |
| Legacy-token migration order undefined | C | D17 in 00-prd |
| Drops terminal selection, keeps orchestrator | C | E07 in M1; orchestrator rejected |
| Phone permission slot never fixed | C | E01 PR4 |
