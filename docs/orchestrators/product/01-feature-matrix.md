# 01 — Feature matrix: Zeron, MonoCode, landscape vs Pocket

Date: 2026-09-30. Inputs: research reports R01–R19 (`docs/orchestrators/research/`), `CONTEXT.md`, PO decisions D1–D13. Pocket paths verified at `5bc8ea8`; main is now `f8f7293` and desktop paths moved — resolve them through 05-roadmap §0 Path map.

**Legend**
- Cells: ✓ ships · partial = some of it · – absent · n/r = no report covers it.
- `[R04 idea 04-3]` = idea 04-3 in report 04. `[R04 §F2]` = finding F2 (or numbered finding) in report 04. `[R15 §F16]` = the landscape product matrix.
- `[P path:L]` = Pocket at 5bc8ea8, verified; desktop paths → 05-roadmap §0 Path map (`pd/`, `app/` unchanged). `d/` = `packages/desktop/crates/`, `pd/` = `packages/pocketd/`, `app/` = `packages/app/src/`.
- Idea IDs: every source ID merged into the row. Effort: the source report's estimate; a range when reports differ. Value: H/M/L against the 6–8-week goal.
- **Fix now** = D12 milestone, ahead of features.
- Terms per CONTEXT.md. Report terms mapped: unread → Done, blocked/awaiting → Needs you, busy/running → Working, transcript → timeline.

## 1. Matrix

### 1.1 Sessions & status

| Feature | Zeron | MonoCode | Best landscape example | Pocket today | Idea IDs | Effort | Value |
|---|---|---|---|---|---|---|---|
| Status colours per D1 (Needs you amber, Working accent + spinner, Done green, Failed red, Idle faint); label "Working" | partial: own palette, amber = queued/offline [R05 idea 05-20; R07 §F6] | partial: dashed border + status word on Needs-you rows [R12 idea 12-3] | n/r | partial: Working green, label "Running" [P d/ui/src/ui.rs:457]; Done accent dot [R07 §F15] | 07-4, 05-20, 12-3 | S | H |
| Stable sidebar order; no re-sort on status change (D2) | ✓ recency order; status drives only the dot [R04 §F2] | n/r | n/r | – sorts by status [P d/pocket/src/view.rs:481,665] | 04-1 | S | H |
| Session row anatomy + compact density | ✓ compact 29 px rows [R04 §F3; R04 idea 04-13] | ✓ status word on Needs-you rows [R12 §F7] | n/r | partial: status sections, not-attached banner [R14 §F11] | 01-9, 04-13, 12-3 | S–M | M |
| Auto session title, rename, `claude -n <name>` | ✓ [R01 idea 01-17; R04 idea 04-5] | n/r | n/r | partial: prompt-named worktrees only [R06 §13] | 01-17, 04-5, 17-12 | S | M |
| Session context menu: Rename, Pin, Copy ▸ path / conversation ID / resume command, Close session… | ✓ [R04 §F6] | n/r | n/r | partial: project and worktree menus only [R14 §F11] | 04-5 | M | M |
| Pins | ✓ [R04 §F7] | ✓ pin, folders, filters [R09 idea 09-15] | n/r | – | 01-8, 04-6, 09-15 | M | M |
| Custom sections / session folders | ✓ [R04 §F7] | ✓ [R12 idea 12-14] | n/r | – | 04-15, 12-14 | L | L |
| Project monogram (8 hashed colours) + favicon | ✓ [R04 idea 04-12] | n/r | n/r | – | 01-19, 04-12 | S | L |
| Context usage: warn 75 %, danger 90 %, agent-reported window, hidden when unknown (D7); desktop + phone | ✓ 16 px ring, 75/90 [R05 §F8] | ✓ ring 75/90, "176K / 256K tokens" hover [R09 §F6] | n/r | partial: 32×4 bar, warns > 80 % [P d/ui/src/ui.rs:667]; window hardcoded 200 000 [R05 §F11]; phone – | 01-16, 03-6, 05-7, 08-17, 14-21 | S | H |
| Usage-limit detection, reset countdown, auto-resume +30 s | partial: plan usage [R01 idea 01-16] | ✓ [R09 idea 09-4] | n/r | – | 09-4 | M | M |
| State-report API for any agent CLI (`pocketd report --state --seq`, `pocketd release`) | n/r | n/r | herdr `pane report-agent` [R15 idea 15-2] | – state only from Claude hooks + Codex RPC [R15 §F16] | 15-2 | M | M |
| More providers | ✓ 10 harnesses incl. ACP [R01 §F2] | ✓ 10 CLIs [R09 §F2] | herdr via report API [R15 §F4] | – claude + codex [R14 §F8] | 14-22, 15-2 | M each | L |
| Named accounts (`CLAUDE_CONFIG_DIR` / `CODEX_HOME`) | n/r | ✓ [R09 idea 09-8] | n/r | – | 09-8, 10-18 | M | L |

### 1.2 Attention & notifications

| Feature | Zeron | MonoCode | Best landscape example | Pocket today | Idea IDs | Effort | Value |
|---|---|---|---|---|---|---|---|
| ⌘J next Needs you (D6) | – Mod+J = terminal drawer [R04 §F4] | n/r | n/r | ✓ [P d/pocket/src/main.rs:949,1065] | — | — | H |
| Jump to newest Done; clear all Done | n/r | n/r | cmux ⌘⇧U [R15 idea 15-15] | partial: ⌘J covers Needs you only [P d/pocket/src/main.rs:949] | 15-15 | S | M |
| Attention list sorted by urgency, with elapsed time (D2) | – recency only [R04 §F2] | ✓ needs input → working → unseen done [R09 idea 09-3] | cmux unread model [R15 §TL;DR] | partial: Inbox with j/k [R05 §F11] | 09-3, 08-11 | S | M |
| Banners: copy per status, body = the ask ≤ 240 chars, click focuses | ✓ [R01 idea 01-1; R04 §F9] | ✓ [R13 idea 13-20] | n/r | partial: banners dismissed once Seen, click focuses; fixed copy [P d/pocket/src/main.rs:339; R04 §F9] | 01-1, 13-20 | S | H |
| Sound cues (Needs you, Done, Failed); per-cue toggles default on; play on non-Seen transitions while focused (D11) | ✓ three cues, background-only default [R05 §F9; R07 §F12] | n/r | n/r | – no sounds [R05 §F11] | 01-1, 04-9, 05-6, 07-6, 14-10 | S–M | H |
| Notification settings: per-cue toggles, per-project mute 1/4/8 h, categories | ✓ Notifications section [R04 §F9] | ✓ [R09 idea 09-2; R11 §F9] | n/r | – [R04 §F9] | 04-9, 09-2, 11-12 | M | M |
| Detector rules: silent baseline on first sight, 250 ms coalesce, subagents silent, interrupt ≠ Failed | ✓ [R05 idea 05-5] | n/r | n/r | partial: Seen-aware Done; interrupt is not failure [CONTEXT.md §Status] | 05-5 | S | M |
| Dock badge = Needs-you count | n/r | ✓ [R09 idea 09-1; R11 §F9] | Superset dock badge [R15 §F16] | – | 09-1, 11-12, 14-10 | S | H |
| Resolve permissions on the desktop: approval toast stack, Allow/Deny on banners | – auto-approve [R08 §F15] | ✓ toasts [R13 idea 13-11] | n/r | – desktop sends only view/seen/close [P d/agents/src/agents.rs:208-218] | 13-11, 14-10, 14-2 | M | H |
| OSC 9/99/777 + `pocketd notify` raise Needs you from any program | n/r | n/r | cmux [R15 idea 15-3] | – vt shim has no OSC callback [R15 idea 15-3] | 15-3, 16-15 | S–M | M |
| Agent-requested push (`pocketd notify --push`) | n/r | n/r | Claude RC [R15 idea 15-4] | – | 15-4 | S | L |
| Floating status HUD over other apps | n/r | n/r | Codex pet [R15 idea 15-6] | – | 15-6 | M | L |
| Session reminders | n/r | ✓ [R09 idea 09-7] | n/r | – | 09-7 | M | L |

### 1.3 Desktop shell

| Feature | Zeron | MonoCode | Best landscape example | Pocket today | Idea IDs | Effort | Value |
|---|---|---|---|---|---|---|---|
| ⌘1–9 = sessions in visible sidebar order; hint chips while ⌘ held (D6) | ✓ [R04 §F4] | ✓ ⌘1–9 tabs [R09 idea 09-12] | n/r | – [P d/pocket/src/main.rs:1062-1071] | 01-4, 04-2, 09-12 | S–M | H |
| Ctrl+Tab cycle with wrap; ⌘⇧↑/↓ next/prev session | ✓ [R04 idea 04-3] | ✓ [R09 idea 09-12] | n/r | – | 01-4, 04-3, 09-12 | S | M |
| ⌘K palette: every-word fuzzy match, cap after filter, wrap, highlight, `>` commands, action registry | ✓ [R04 §F5] | ✓ [R12 §F10] | n/r | partial: substring match, 5 rows [P d/pocket/src/overlay.rs:95,114] | 04-4, 12-12, 14-12 | S–M | H |
| Persist window geometry per display (min 900×600) and layout | ✓ [R04 idea 04-7] | n/r | n/r | – opens centred 1440×900 [P d/pocket/src/main.rs:1074] | 04-7, 14-15 | S | M |
| Resize seams: 20 px hitbox, double-click reset | ✓ [R04 §F8] | ✓ sash dbl-click reset [R16 §F4] | n/r | partial: 5 px handle, no reset [R12 §F16] | 04-8 | S | L |
| Titlebar names the selected session | ✓ [R04 idea 04-11] | n/r | n/r | – fixed "Anywhere" [P d/pocket/src/main.rs:1078] | 04-11 | S | L |
| Tabs: two-line with agent status stack; width / FLIP motion | n/r | ✓ [R12 §F8] | n/r | partial: tabs + splits, 3 layouts [R14 §F11] | 12-4, 12-5 | S–M | M |
| Splits with sizes: sash (min 160, max 55 %, dbl-click reset), edge-drop, ⌘⌥arrow focus | ✓ dock sizes [R16 idea 16-17] | ✓ split tree [R12 §F9] | n/r | partial: `Vec<Vec<String>>`, no sizes [P d/workspace/src/workspace.rs:4] | 12-6, 12-7, 12-8, 16-17 | S (rows) – L (tree) | M |
| Compact rail push-drawer | n/r | ✓ [R12 §F6] | n/r | partial: ⌘\ toggles rail [P d/pocket/src/main.rs:1066] | 12-9 | M | L |
| Settings surface: sections, search flash, keybindings page, rebindable shortcuts | ✓ 9 sections, rebindable [R01 §F3] | ✓ [R12 §F12] | n/r | partial: per-project sheet on ⌘, only [R12 §F16] | 01-18, 12-10, 12-11, 12-13 | M (L with rebinding) | M |
| Empty states | ✓ new-session canvas [R01 §F4] | ✓ [R12 §F15] | n/r | partial: blank page + start button [R17 §F15] | 12-16 | S | L |
| Close-session confirm while Working | n/r | ✓ [R13 idea 13-13] | n/r | – | 13-13 | S | M |
| Global quick composer ⌘⇧Space | n/r | ✓ [R11 §F9] | n/r | – | 09-6, 11-13 | L | L |
| Project actions (named commands) | ✓ [R01 idea 01-7] | n/r | Conductor scripts [R15 idea 15-10] | partial: per-project setup command [R01 §F9] | 01-7, 04-14 | L | L |
| Full-surface search | n/r | ✓ [R12 idea 12-17] | n/r | – | 12-17 | L | L |

Enablers (no surface): 14-13 split the Desktop god-struct (L). 14-24 rebase on main: done (main `f8f7293`; 05-roadmap §0).

### 1.4 Terminal (D9)

| Feature | Zeron | MonoCode | Best landscape example | Pocket today | Idea IDs | Effort | Value |
|---|---|---|---|---|---|---|---|
| **Fix now:** ligatures off (`liga`/`calt`/`dlig` = 0) | ✓ [R16 idea 16-3] | n/r | n/r | – Geist Mono with ligatures on [R16 §TL;DR] | 16-3 | S | H |
| Scrollback, 10k-line cap on both VTs, viewport scroll | ✓ 1 MiB replay [R01 §F3] | ✓ 5000 lines [R16 §F4] | n/r | – no scrollback view; ~1 page kept [R16 §TL;DR] | 14-16, 16-1 | S | H |
| Wheel/trackpad: remainder accumulation, TouchPhase reset, snap-to-bottom, mouse-mode routing | ✓ [R16 idea 16-2] | ✓ [R16 §F4] | n/r | – [R16 §F1] | 16-2 | S | H |
| Selection: 1/2/3 clicks, Shift+click extend, 2 px arm, 24 ms edge autoscroll | ✓ [R16 idea 16-4] | ✓ xterm selection [R16 §F4] | n/r | – [R16 §TL;DR] | 14-16, 16-4 | M | H |
| ⌘C copies selection; never sends ^C when a selection exists | ✓ [R16 idea 16-5] | ✓ [R16 §F4] | n/r | – | 16-5 | S | H |
| ⌘V bracketed paste; confirm unsafe multi-line | partial [R16 idea 16-6] | ✓ [R16 §F4] | n/r | – | 16-6 | S | H |
| Mac editing keys ⌥←/→, ⌘←/→, ⌘⌫ | n/r | ✓ [R13 idea 13-12] | n/r | – [R13 §F11] | 13-12 | S | H |
| Scrollbar rail (3→5 px, 1400 ms linger) | ✓ [R16 idea 16-7] | ✓ 14 px reserve [R16 §F4] | n/r | – | 16-7 | M | M |
| Links: OSC 8 + URL / `path:line`, ⌘-click | – [R16 §TL;DR] | – [R16 §F4] | n/r | – | 14-16, 16-8 | M | M |
| Mouse reporting for TUIs; Shift bypass | – [R16 §TL;DR] | ✓ [R16 §TL;DR] | n/r | – | 14-16, 16-9 | M | M |
| IME preedit + candidate bounds | – [R16 §TL;DR] | ✓ [R16 §TL;DR] | n/r | partial: `EntityInputHandler`, no preedit [P d/pocket/src/main.rs:997] | 16-10 | S | M |
| Find in terminal ⌘F | – [R16 §TL;DR] | – [R16 §F4] | n/r | – | 14-16, 16-14 | M | M |
| Resize debounce 80 ms | ✓ [R16 idea 16-11] | n/r | n/r | – | 16-11 | S | M |
| Terminal tabs: drag reorder, middle-click close, exited at 0.55, OSC title | ✓ [R16 idea 16-12] | ✓ reorder, middle-click [R16 §F4] | n/r | partial: tabs, no reorder [R16 idea 16-12] | 16-12 | M | M |
| OSC side channels: title, bell dot, OSC 52 (confirm), OSC 9/777 | partial [R16 idea 16-15] | partial: OSC 7 cwd, no bell [R16 §F4] | cmux [R15 idea 15-3] | – | 16-15, 15-3 | S | M |
| Cursor: DECSCUSR shape, blink (off under reduced motion), hollow unfocused | ✓ [R16 idea 16-16] | ✓ [R16 §F4] | n/r | – | 16-16 | S | L |
| Snapshot fidelity on re-attach (modes; primary history under alt screen) | n/r | n/r | n/r | – loses primary history [R16 §TL;DR] | 16-20 | S–M | M |
| Wide-glyph pinning, only if drift is observed | ✓ [R16 idea 16-19] | n/r | n/r | n/r | 16-19 | M | L |

### 1.5 Transcript & composer

| Feature | Zeron | MonoCode | Best landscape example | Pocket today | Idea IDs | Effort | Value |
|---|---|---|---|---|---|---|---|
| **Fix now:** Send stays available while Working; Enter never stops (Zeron #406) | ✓ Send/Queue/Stop morph [R05 idea 05-2] | n/r | n/r | – phone button is Stop while Working [P app/components/Composer.tsx:51-54] | 05-2 | S | H |
| Queue while Working: Steer / Send next / Send now (interrupt) | ✓ [R05 §F6; R08 §F15] | ✓ steer [R10 idea 10-9] | n/r | partial: typing into Claude's TUI steers, invisibly [R05 §F11] | 01-12, 03-5, 05-3, 08-12, 10-9 | M | H |
| Desktop timeline view | ✓ [R05 idea 05-23] | ✓ [R10 idea 10-12] | n/r | – desktop none; phone ✓ [R14 §F11] | 03-4, 05-23, 10-12, 14-1, 14-3 | L–XL | H |
| Desktop composer + actions (prompt, interrupt, compact, resolve) | ✓ [R03 idea 03-4] | ✓ [R10 idea 10-12] | n/r | – [P d/agents/src/agents.rs:208-218] | 14-2 | M | H |
| Codex turns via pocketd's existing app-server client: turn/start, turn/steer, turn/interrupt (D4) | ✓ own app-server [R03 §6] | ✓ private stdio [R10 §4] | n/r | partial: second client streams; prompts typed into the PTY [R03 §9; R05 §F11] | 03-1, 10-9 | M | H |
| Structured questions (AskUserQuestion wizard, 1–9 keys, 220 ms auto-advance) + ExitPlanMode review | ✓ [R01 idea 01-13; R05 idea 05-4] | ✓ [R10 idea 10-10; R13 §F3] | n/r | partial: hook flags Needs you, no question UI [R08 §F15] | 01-13, 03-3, 05-4, 10-10, 14-11 | M–L | H |
| Tool-group summary row, fold when settled, tool row grammar | ✓ [R05 §F2] | ✓ [R13 §F1] | n/r | partial: phone groups tools, open by default [R05 §F11] | 03-15, 05-1, 05-10, 13-9 | S–M | M |
| Working trailer: flavour word, elapsed, "Worked for Ns" | ✓ [R05 §F3] | n/r | n/r | partial: phone "Working 12s · model", timer resets on remount [R05 §F11] | 05-8, 08-16 | S | M |
| Per-session drafts, multiline | ✓ persisted [R08 §F15] | n/r | n/r | – single-line, lost on unmount [R08 §F15] | 05-11, 08-9 | S | H |
| Collapse long prompts; "Show full output (N KB)" | ✓ [R05 idea 05-12; R05 idea 05-16] | n/r | n/r | partial: output clamped at 64 KiB [R05 §F11] | 05-12, 05-16 | S | M |
| Scroll follow rules + jump pill | ✓ [R05 §F4] | n/r | n/r | partial: phone 46 px jump control [R05 §F11] | 05-17, 08-14 | S | M |
| Own-send runway | ✓ [R05 idea 05-18] | n/r | n/r | – | 05-18, 08-15 | M | L |
| Streaming polish: veil, markdown mend, paced reveal | ✓ [R05 idea 05-19] | ✓ [R13 §F2] | n/r | – | 05-19, 13-10 | M | L |
| Streamed thinking | ✓ [R03 idea 03-10] | n/r | n/r | partial: phone collapses to first line [R05 §F11] | 03-10 | S | L |
| Turn rail / minimap | ✓ [R01 idea 01-20; R05 idea 05-21] | n/r | n/r | – | 01-20, 05-21 | M | L |
| Image attachments | ✓ [R03 idea 03-13] | n/r | n/r | – "+" not wired [R05 §F11] | 03-13, 05-13 | M | M |
| @file mention | ✓ [R05 idea 05-14] | n/r | n/r | – | 05-14 | M | M |
| Slash / skill completion | ✓ [R03 idea 03-8] | n/r | n/r | partial: phone intercepts `/compact` [R05 §F11] | 03-8, 05-15 | M | M |
| Subagent folding | ✓ [R03 idea 03-14] | n/r | n/r | – sidechain lines dropped [R05 §F11] | 03-14 | M | L |
| Task list: cancelled state, "N of M" | n/r | ✓ [R13 idea 13-14] | n/r | partial: TodoWrite folds into tasks [R05 §F11] | 13-14 | S | L |
| Mode handoff "Open in terminal" / "Open as chat" | ✓ [R03 idea 03-9] | n/r | n/r | – | 03-9 | M | M |
| Hero → dock glide from draft to timeline | ✓ 0.420 s [R17 §F15] | ✓ 480 ms [R17 §F15] | n/r | – | 17-13 | M | L |

### 1.6 New-session flow

| Feature | Zeron | MonoCode | Best landscape example | Pocket today | Idea IDs | Effort | Value |
|---|---|---|---|---|---|---|---|
| **Fix now:** Codex Auto-edit uses `-s workspace-write -a on-request`, not `--full-auto` | n/r | n/r | n/r | – codex rejects the flag [P d/pocket/src/forms.rs:412; R17 §F2] | 17-1 | S | H |
| Explicit access flags on both axes for every mode | n/r | ✓ [R17 §F9] | n/r | – Ask sends no flag [R17 §F2] | 17-2, 10-4 | S | H |
| Access picker: Ask / Auto-accept edits / Auto / Full access (amber) + "Plan first" toggle | – sandbox fixed [R17 §TL;DR] | ✓ 4 modes + plan toggle [R17 §F9] | n/r | partial: one combined chip, 3 modes [R17 §F1] | 17-3, 10-4, 10-16 | M | H |
| Draft "New session" canvas in the main pane replaces the modal | ✓ hero composer [R17 §F3] | ✓ blank tab [R17 §F8] | n/r | – modal; prompt lost on git error [R17 §F15] | 17-4 | L | H |
| Persist picks: provider, model/effort per provider, access (never Full access) | ✓ [R17 §F6] | ✓ inherits active session [R17 §F10] | n/r | – nothing on disk [R17 §TL;DR] | 17-5 | S | H |
| Checkout chip (current checkout / current worktree / new worktree), base chip, inline name | ✓ [R17 §F4] | ✓ [R17 §F8] | n/r | partial: branch picker + footer prose [R17 idea 17-6] | 17-6, 06-16 | M | M |
| Agent / model / effort card, live catalog, dimmed missing CLIs, ⌘/ opens | ✓ [R17 §F4; R03 idea 03-7] | ✓ [R10 idea 10-11] | n/r | – model only via the CLI TUI [R17 §F13] | 17-7, 17-14, 03-7, 10-11 | M | M |
| pocketd-offline and git-failure states before the draft closes | ✓ [R17 idea 17-9] | n/r | n/r | – writes silently dropped [R17 §TL;DR] | 17-9 | S | H |
| `agent.create` with LaunchSpec; pocketd builds argv (D5) | n/r | ✓ agent.create/configure [R10 idea 10-3] | n/r | – argv built in desktop [R17 §F13] | 17-10, 10-3, 11-3, 14-19 | M | H |
| Tab-menu Claude Code / Codex rows use remembered picks | n/r | n/r | n/r | – ignore access and model [R17 idea 17-11] | 17-11 | S | M |
| LLM-named draft worktree | n/r | ✓ [R09 idea 09-16] | n/r | partial: prompt-named [R06 §13] | 09-16 | M | L |
| Prompt templates | n/r | ✓ [R11 idea 11-14] | n/r | – | 11-14 | S | L |
| Headless sign-in dialog | ✓ [R17 idea 17-17] | ✓ [R17 idea 17-17] | n/r | n/a: CLIs sign in inside their TUI [R17 §TL;DR] | 17-17 | M | L |

### 1.7 Git & review

| Feature | Zeron | MonoCode | Best landscape example | Pocket today | Idea IDs | Effort | Value |
|---|---|---|---|---|---|---|---|
| Latest-turn diff (temp-index write-tree at UserPromptSubmit) | ✓ [R06 idea 06-1] | n/r | n/r | – HEAD vs working tree only [R06 §13] | 06-1 | M | H |
| Batched review comments into the next prompt: header, `path:line (L/R): body`, quoted `> ±text` | ✓ [R06 §5] | ✓ [R13 idea 13-3] | Superset lines → agent [R15 §F16] | partial: sent at once, one line each [R06 §13] | 01-6, 06-2, 13-3 | S | H |
| Branch-changes scope + ref picker | ✓ [R06 §13] | n/r | n/r | – [R06 §13] | 06-3 | M | M |
| Multi-file continuous diff, sticky headers | ✓ [R06 §13] | n/r | n/r | – one file per view [R06 §13] | 06-4 | L | M |
| Live refresh: fs-watch, pushed updates instead of polling | ✓ [R06 §13] | n/r | n/r | – git polled 2 s, list 1 s [R06 §13; R14 idea 14-14] | 06-5, 14-14 | M | M |
| Snapshot caps + notices | ✓ [R06 idea 06-6] | n/r | n/r | n/r | 06-6 | S | L |
| Fold bar: expand 20 lines up/down, "N unmodified lines" | – [R06 §13] | ✓ [R13 idea 13-16] | n/r | partial: click-to-unfold, no steps [R06 §1] | 13-16 | S | L |
| Git safety: confirm push from default branch, warn amend of pushed head, staleness-guarded discard, cancellable AI message | partial: whole-tree discard only [R06 §13] | ✓ [R13 idea 13-6; R13 idea 13-17] | n/r | partial: per-path discard; push unconfirmed [R13 idea 13-6] | 06-7, 13-6, 13-17 | S | H |
| Hunk stage / revert | – [R06 §4] | ✓ [R13 idea 13-2] | n/r | partial: per path [R06 §13] | 13-2 | M | M |
| PR status badge (gh) + upstream sync bar | ✓ [R06 §7] | ✓ [R13 idea 13-4] | Claude desktop PR monitor [R15 §F5] | – [R06 §13] | 01-5, 06-8, 13-4 | M | M |
| Commit, Push & Create PR with LLM body | – no PR creation [R06 §7] | ✓ [R13 idea 13-5] | Claude desktop [R15 §F16] | partial: Commit/Amend/Push + AI message [R06 §13] | 13-5 | M | M |
| Auto-archive worktree when its PR merges | n/r | n/r | Claude desktop [R15 idea 15-11] | – | 15-11 | S after 06-8 | L |
| File checkpoints, end-of-turn Undo / Keep | n/r | ✓ [R10 §6; R13 §F6] | Conductor, Codex snapshots [R15 §F16] | – | 10-14, 13-1 | L | M |
| History lane graph | ✓ [R06 §6] | n/r | n/r | – [R06 §13] | 01-21, 06-11 | L | L |
| Delete-worktree dialog with unpushed commits; branch kept (D8) | partial: deletes its own branches [R06 §13] | ✓ [R13 idea 13-15] | n/r | partial: closes terminals, keeps branch [CONTEXT.md §Worktree] | 13-15 | S | M |
| Worktree create/remove inside pocketd (phone creates with a new worktree, D5) | ✓ host-created [R06 §13] | ✓ [R11 idea 11-4] | n/r | – desktop `crates/git` [R15 idea 15-8] | 11-4 | M | H |
| Worktree retention: cap 15, snapshot ref before delete | n/r | n/r | Codex [R15 idea 15-8] | – | 15-8 | M | L |
| `.worktreeinclude` copy source; skip symlinks; no overwrite | n/r | n/r | Claude, Codex [R15 idea 15-9] | partial: root `.env*` copied, overwrites [R15 idea 15-9] | 15-9 | S | M |
| Setup env (`POCKET_PROJECT_ROOT`, worktree path, `POCKET_PORT` block of 10), teardown script | ✓ setup env [R06 idea 06-15] | n/r | Conductor [R15 idea 15-10] | partial: setup command chained [R14 §F11] | 06-15, 15-10 | S | M |

### 1.8 Files & preview

| Feature | Zeron | MonoCode | Best landscape example | Pocket today | Idea IDs | Effort | Value |
|---|---|---|---|---|---|---|---|
| Explorer with md / mermaid / images; ⌘P | ✓ tree + preview [R06 §8] | n/r | n/r | ✓ [R14 §F11] | — | — | — |
| Virtualized keyboard file tree | ✓ [R06 §13] | n/r | n/r | – [R06 §13] | 06-12 | M | M |
| Line comments in Explore; selection "Add to chat" | ✓ [R06 idea 06-13] | ✓ [R13 §F7] | n/r | – diff comments only [R06 §13] | 06-13, 13-7 | S–M | M |
| Explore change navigation | n/r | ✓ [R13 idea 13-8] | n/r | – | 13-8 | M | L |
| Wrap toggle; persist split / wrap | ✓ [R06 idea 06-17] | n/r | n/r | n/r | 06-17 | S | L |
| Editable files | ✓ [R06 §13] | n/r | n/r | – read-only [R06 §13] | 06-19 | L | L |
| Dev-server discovery list | ✓ [R06 §10] | n/r | n/r | – [R06 §13] | 06-9 | M | M |

### 1.9 Design system

| Feature | Zeron | MonoCode | Best landscape example | Pocket today | Idea IDs | Effort | Value |
|---|---|---|---|---|---|---|---|
| Contrast fixes + WCAG test (TEXT_3 3.38, TEXT_4 2.56, FAILED 3.91, count badge 1.80; iOS-blue button shadow) | ✓ [R07 idea 07-3] | n/r | n/r | – [R07 §F15] | 07-3 | S | H |
| Dark theme; System / Light / Dark; runtime palette | ✓ [R07 §F1] | ✓ derived two-scheme tokens [R12 §F1] | n/r | – light-only u32 consts [R07 §F15] | 07-1, 07-2, 12-1, 14-20 | L | M |
| Motion tokens + catalog, reduce motion, FLIP glide 260 ms | ✓ [R07 §F11] | ✓ [R12 §F3] | n/r | partial: follows reduce motion [P d/pocket/src/main.rs:1060] | 04-10, 07-7, 12-2, 12-5 | M | M |
| Syntax palette (12 roles) + ANSI 16 | ✓ [R07 idea 07-10] | n/r | n/r | partial: diff syntax highlighting [R14 §F11] | 07-10 | S–M | M |
| Diff wash 0.055 + 3 px bar | ✓ [R07 §F7] | n/r | n/r | n/r | 07-11 | S | L |
| Phone: one token set (merge `theme.ts` into `design.ts`) + nav stack | ✓ iOS palette [R07 §F14] | n/r | n/r | – two token sets [R14 idea 14-18] | 07-12, 14-18 | S–M | M |
| Accent presets (7) | ✓ [R07 §F4] | n/r | n/r | – | 07-5 | M | L |
| Frosted window / blurred sidebar | ✓ α 0.80 [R07 §F5] | ✓ [R12 idea 12-15] | n/r | – | 07-8, 12-15 | S–M | L |
| UI font scale 12–20 | ✓ [R07 idea 07-9] | n/r | n/r | – | 07-9 | M | L |
| Pixel-glyph spinner 30 fps | ✓ [R07 idea 07-14] | n/r | n/r | partial: plain spinner [P d/ui/src/ui.rs:457] | 07-14 | M | L |

### 1.10 Phone

| Feature | Zeron | MonoCode | Best landscape example | Pocket today | Idea IDs | Effort | Value |
|---|---|---|---|---|---|---|---|
| Push via Expo from pocketd: Seen-aware transitions, one collapse id per session, tap opens session, pre-prompt card, diagnostics, fixed lock-screen copy (D3) | ✓ APNs from edge, no Seen suppression [R08 §F15] | n/r | Happy via Expo [R15 §F16] | – [R08 §F15] | 01-2, 02-2, 08-1, 08-2, 08-3, 08-4, 08-5, 11-12, 14-9, 18-13, 19-12, 19-13 | M | H |
| Graced connectivity: no eject, 4 s pill, redial on AppState / NetInfo, liveness lease | ✓ [R08 §F12; R02 idea 02-4] | n/r | n/r | – ejects to ConnectScreen [R08 §F15] | 02-4, 05-22, 08-6, 08-7 | M | H |
| Outbox: per-install clientId, idempotent prompts, "Not delivered — tap to retry" | ✓ [R02 idea 02-6; R08 idea 08-8] | ✓ commandId [R10 idea 10-13] | n/r | – drops sends; clientId constant [P app/session.tsx:76] | 02-5, 02-6, 05-9, 08-8, 10-13, 11-15 | M | H |
| Approval panel holding every open request | – auto-approve [R08 §F15] | n/r | n/r | partial: one-slot PermissionSheet [R08 §F15] | 08-13 | M | H |
| Attention list by urgency + live summary "2 Working · 1 Needs you" (D2) | ✓ summary [R08 idea 08-11] | ✓ [R09 idea 09-3] | n/r | – no phone attention list [R15 idea 15-15] | 08-11, 09-3, 15-15 | S | H |
| Session row redesign (62 pt) | ✓ [R08 §F5] | n/r | n/r | partial: agent list [R14 §F7] | 08-10 | M | M |
| Start a session: claude/codex, registered project, no cmd/env/cwd (D5, D10) | ✓ phone canvas [R08 §F10] | n/r | Claude app, ChatGPT app [R15 §F16] | – Mac only [R08 §F15] | 08-21, 14-19, 17-10, 18-10 | M | H |
| Read-only terminal: server-rendered frames, grid/log modes, no resize | n/r | n/r | n/r | – "Open raw terminal" has no handler [R16 §F6] | 14-17, 16-13 | M | H |
| Key bar + raw terminal input | n/r | n/r | n/r | – | 16-18, 14-17 | M | M |
| Read-only file / diff viewer; "This turn" diff | ✓ remote file access [R02 §F11] | n/r | n/r | – dead diff button [R14 idea 14-17] | 02-16, 06-18, 14-17, 18-11 | M | M |
| Tail cache (64 items) for instant open | ✓ [R02 idea 02-10] | n/r | n/r | – | 02-10 | M | M |
| Voice / dictation | n/r | n/r | Happy voice agent [R15 §F16] | – mic not wired [R05 §F11] | 14-17 | M | L |
| Lock-screen Allow/Deny (auth, no escalation options) | – [R08 §F15] | n/r | n/r | – | 08-19, 18-13 | L | M |
| Live Activity / Dynamic Island fleet tracker | n/r | n/r | Cursor iOS [R15 idea 15-5] | – | 15-5 | L | M |
| Multiple Macs on one phone | ✓ [R02 idea 02-13] | n/r | n/r | – | 02-13 | L | L |
| iPad split view | ✓ [R08 §F13] | n/r | n/r | – portrait only [R08 §F15] | 08-20 | L | L |

### 1.11 Daemon & durability

| Feature | Zeron | MonoCode | Best landscape example | Pocket today | Idea IDs | Effort | Value |
|---|---|---|---|---|---|---|---|
| PTYs outlive clients; libghostty snapshot on attach | ✓ daemon [R02 §F1] | – scheduler in webview [R11 §F6] | Superset daemon [R15 §F16] | ✓ [R14 §F11] | — | — | — |
| Restore agents after pocketd / Mac restart via resume argv | n/r | n/r | herdr, cmux, Superset [R15 idea 15-1] | – only config + plugin persist [R15 §TL;DR] | 15-1 | M | H |
| Persist timelines (JSONL journal) | ✓ Loro docs [R02 §F3] | ✓ [R10 idea 10-7] | n/r | – in memory [R14 idea 14-4] | 02-11, 10-7, 14-4 | M | H |
| Backward paging (`beforeSeq`) + frames ≤ 512 KiB | ✓ [R02 idea 02-3] | n/r | n/r | – forward-only; a page can reach ~12.5 MiB [R02 §F15] | 02-3, 14-5 | S | H |
| Protocol range + capabilities in hello | ✓ [R02 idea 02-9] | n/r | n/r | – exact version match [P pd/internal/wsserver/wsserver.go:123] | 02-9, 19-10 | S–M | H |
| `pocketd status`, `--version`, log file, single-instance lock | ✓ [R19 idea 19-4] | n/r | n/r | – [R19 §F7] | 02-8, 19-4 | M | H |
| Desktop waits and reconnects instead of exiting | n/r | n/r | n/r | – `exit(1)` [P d/pocket/src/main.rs:1052] | 19-3 | M | H |
| Restart policy: auto only at 0 terminals, once per version; else "Restart closes N terminals" | ✓ quiescence gate [R19 §F5] | n/r | n/r | – | 01-10, 19-9 | M | M |
| Live pocketd upgrade (PTY fds over SCM_RIGHTS) | n/r | n/r | herdr [R15 idea 15-14] | – | 15-14 | L | L |
| Keep the Mac awake while Working or a phone is connected | n/r | n/r | Codex, Superset [R15 idea 15-7] | – | 15-7 | S | M |
| Child hygiene: process group, reaper | ✓ [R03 idea 03-11] | ✓ [R10 idea 10-5] | n/r | n/r | 03-11, 10-5 | S | M |
| Login-shell env capture | n/r | ✓ [R10 idea 10-6] | n/r | ✓ login-shell spawn [R14 §F11] | 10-6 | S | L |
| Batch stream deltas 120 ms | n/r | ✓ [R10 idea 10-17] | n/r | partial: Codex token streaming [R14 §F11] | 10-17 | S | M |

### 1.12 Orchestration & automation

| Feature | Zeron | MonoCode | Best landscape example | Pocket today | Idea IDs | Effort | Value |
|---|---|---|---|---|---|---|---|
| Headless Claude driver (stream-json), only if headless is adopted | ✓ [R03 §5] | ✓ [R10 §3] | n/r | – terminal driver types into the PTY [R05 §F11] | 03-2, 10-1, 14-6, 14-7 | L | M |
| Headless Codex on a private stdio app-server (headless only, D4) | ✓ [R03 §6] | ✓ [R10 §4] | n/r | – | 10-2 | M | L |
| Idle-park headless children | n/r | ✓ [R10 idea 10-8] | n/r | n/a | 10-8 | S | L |
| Codex rewind | n/r | ✓ [R10 idea 10-15] | n/r | – | 10-15 | S | L |
| Agent control CLI / MCP server (`pocketd app sessions.*`) behind grants | ✓ stdio MCP [R01 idea 01-14] | ✓ `/operator` [R11 §F4] | Superset CLI, SDK, MCP [R15 §F16] | partial: `pocketd run/attach/hook`; ops socket ungated [R18 §F1] | 01-14, 03-16, 09-14, 11-1 | M–L | M |
| Orchestrator run: lead + workers, isolation, write-scope deny, integrate on review | n/r | ✓ [R11 §F1; R11 §F3] | n/r | – | 11-5, 11-6, 11-7, 11-8 | XL | M |
| Automations in pocketd: schedules, gh triggers, launch policy | n/r | ✓ in webview, app must be open [R11 §F6] | n/r | – | 09-13, 11-9, 11-10, 11-11 | L | M |
| CI repair | n/r | ✓ [R11 §F7] | Claude desktop, Conductor Checks [R15 §F16] | – | 09-17 | L | L |

Enabler: 11-18 add CONTEXT terms ("chat agent", optional terminalId) before headless work [R03 §9].

### 1.13 Remote & trust (D10)

| Feature | Zeron | MonoCode | Best landscape example | Pocket today | Idea IDs | Effort | Value |
|---|---|---|---|---|---|---|---|
| **Fix now:** no agent self-approval: desktop off the phone token, per-terminal grants, peer-PID check, no approve/pair/devices from PTY descendants | – [R18 §F4] | ✓ peer checks [R18 idea 18-8] | n/r | – token plaintext, any socket resolves [R18 §TL;DR] | 18-6, 18-8, 11-2 | M + M | H |
| Bind loopback + Tailscale only; `--listen lan` needs TLS | n/r | ✓ [R18 idea 18-1] | VibeTunnel [R15 idea 15-13] | – all interfaces [P pd/cmd/pocketd/serve.go:46] | 18-1, 15-13 | S | H |
| Handshake hardening: Host allowlist, Origin check, 4 KiB pre-auth, rate limit, split errors, random request IDs | n/r | ✓ 403/401 checks [R18 idea 18-2] | n/r | – `InsecureSkipVerify` [P pd/internal/wsserver/wsserver.go:53] | 18-2 | S | H |
| Per-device hashed tokens, revoke, `pocketd devices` | – every device trusted [R18 §F4] | ✓ [R18 §F3] | n/r | – one shared token [R18 §F1] | 18-3, 14-8 | M | H |
| QR pairing with a one-time code; `anywhere://pair`; Tailscale detection, LAN warning | ✓ [R18 idea 18-4] | ✓ [R18 idea 18-4] | Happy, Claude, Codex, Nimbalyst [R15 §F16] | – host + token typed [R19 §F7] | 08-18, 14-8, 18-4, 19-11, 19-17 | M | H |
| Prompt sanitation: strip C0/ESC/DEL, 64 KiB cap, agent-sourced `!`/`/` rejected | n/r | n/r | n/r | – raw write + `\r` [R18 §F1] | 18-9 | S | H |
| Scope enforcement (observe/drive/approve/spawn/files); observe-only devices | – [R18 §F4] | ✓ [R18 §F3] | n/r | – nothing past hello [R18 §F1] | 18-7 | S | M |
| Devices sheet: This Mac, Phones, Rename, Remove, Pair QR, legacy banner | ✓ [R18 §F4] | ✓ [R18 idea 18-5] | n/r | – | 18-5 | M | M |
| Repo-command import gating; copy lists regular files only; `desktop.json` 0600 | ✓ explicit import [R18 §TL;DR] | n/r | n/r | – default perms, symlinks followed [R18 §F1] | 18-12, 06-15, 01-7 | M (S: 0600 + symlink) | M |
| File-path guard for phone file access | partial: path grammar, but absolute reads [R18 §F4] | ✓ [R18 idea 18-11] | n/r | n/a: no file access [R18 §F1] | 18-11 | M | M |
| Phone at rest: this-device-only keychain, Face ID lock, Unpair | n/r | ✓ revokeSelf [R18 idea 18-14] | n/r | – default SecureStore [R18 §F1] | 18-14 | S | M |
| Tailscale Serve TLS (`wss://<mac>.<tailnet>.ts.net`) | n/r | n/r | VibeTunnel [R15 idea 15-13] | – plain `ws://` [R18 §F1] | 18-15, 15-13 | S | M |
| Plain trust copy: "A paired phone can run commands on this Mac as you." | n/r | ✓ [R18 idea 18-18] | n/r | – | 18-18 | S | M |
| Relay with E2EE (Noise IK, per-device keys via QR) | partial: relay, no E2EE [R18 §F4] | n/r | Happy, Nimbalyst [R15 §F16] | – tailnet only [R15 §F16] | 02-1, 14-8, 15-12, 18-16 | L | L |
| LAN mode with pinned self-signed cert | n/r | n/r | n/r | – | 18-17 | L | L |

### 1.14 Distribution & onboarding

| Feature | Zeron | MonoCode | Best landscape example | Pocket today | Idea IDs | Effort | Value |
|---|---|---|---|---|---|---|---|
| Signed, notarized `Anywhere.app` embedding pocket + pocketd; DMG; release CI | ✓ [R19 idea 19-1] | ✓ [R19 idea 19-1] | n/r | – no build, no CI [R19 §F7] | 19-1, 14-23 | L | H |
| pocketd LaunchAgent via SMAppService; never boot out a running pocketd | partial: boots out first [R19 §TL;DR] | ✓ [R19 §F4] | n/r | – `pocketd serve` in a terminal [R19 §F7] | 01-10, 02-7, 14-23, 19-2 | M | H |
| MIT LICENSE + third-party notices | ✓ [R19 §F8] | ✓ [R19 §F8] | Happy, Nimbalyst [R15 §F16] | – no LICENSE [R19 §F7] | 19-16 | S | H |
| TestFlight via EAS Build + Submit | ✓ [R19 idea 19-14] | n/r | cmux iOS beta [R15 §F16] | – Xcode + author's Team ID [R19 §F7] | 19-14 | M | H |
| First run (8 Mac + 4 phone screens), no account; location gate; attach explainer | ✓ no sign-in wall [R01 §F4] | – no wizard [R09 §F3] | n/r | – 12 manual steps [R19 §F7] | 19-5, 19-7 | M | H |
| Agent CLI check: login-shell probe, version gate, install / update hint | ✓ [R01 idea 01-15; R03 idea 03-12] | ✓ [R09 idea 09-9; R09 idea 09-10] | n/r | – dead Codex row when missing [R17 idea 17-8] | 01-15, 03-12, 09-9, 09-10, 17-8, 19-6 | S–M | M |
| In-app updater: manifest + sha256, staged swap, install on quit, codesign check, "What's new" | ✓ [R19 idea 19-8] | ✓ Tauri updater [R09 §F4] | n/r | – | 01-11, 02-12, 09-11, 19-8 | L | M |
| Self-build: team + bundle id from config; BUILD.md | n/r | n/r | n/r | – hardcoded Team ID [R19 §F7] | 19-15 | S | M |
| Dev isolation: "Pocket Dev" bundle, `POCKET_HOME`, port, label | ✓ [R19 idea 19-19] | n/r | n/r | – | 19-19 | S | M |
| Stop terminal service / uninstall | n/r | ✓ [R19 idea 19-20] | n/r | – | 19-20 | S | M |
| TCC purpose strings | n/r | ✓ [R19 idea 19-18] | n/r | – | 19-18 | S | L |
| Demo mode for App Review | ✓ [R19 idea 19-21] | n/r | n/r | – | 19-21 | M | L |

## 2. Pocket already ahead

- **Seen:** one flag for all clients; a turn that ends while Seen never becomes Done [CONTEXT.md §Status]. Zeron marks seen on select [R04 §F3] and never suppresses push [R08 §F15].
- **Permissions:** real bridge for Claude and Codex, with options and deny feedback [R14 §F11; R05 §F11]. Zeron auto-approves [R08 §F15].
- **Terminal-first:** agents run in real PTYs driven from desktop and phone; Happy's local/remote switch is unnecessary [R15 §Ideas, "Considered but not cloned"].
- **Durable PTYs:** pocketd outlives clients and snapshots via libghostty on attach [R14 §F11]. MonoCode's scheduler dies with its webview [R11 §F6].
- **⌘J next Needs you** [P d/pocket/src/main.rs:1065]; Zeron spends Mod+J on its terminal drawer [R04 §F4].
- **Git:** per-path stage/discard, Commit/Amend/Push, AI commit message, word diff, unfold. Zeron: whole-tree discard, no word diff, no PR creation [R06 §13].
- **Diff comments** go to a chosen agent [R14 §F11].
- **Explorer:** md, mermaid, images, ⌘P [R14 §F11].
- **Worktree creation:** setup chaining, env copy, clone from URL [R14 §F11; R06 §13].
- **Terminal IME:** wired through `EntityInputHandler` [P d/pocket/src/main.rs:997]; Zeron's terminal has none [R16 §TL;DR].
- **Ops socket 0600**; Zeron's local IPC has no token [R02 §F15].
- **No account, no relay, no hook consent:** plugin loads via env, no user files change [R19 §TL;DR]; zero per-user cost [R19 §F8]. Zeron needs WorkOS sign-in for sync [R08 §F15] and relays without E2EE [R18 §TL;DR].
- **Phone timeline:** task/plan folding, Codex token streaming, compact, interrupt [R14 §F11].
- **Screenshot harness + storybook** [R14 §F11].

## 3. Contradictions resolved

| D | Decision | Overrules | Keeps |
|---|---|---|---|
| D1 | Needs you amber, Working accent + spinner, Done green, Failed red, Idle faint | 05-20 (working pink, awaiting indigo); 07-4's amber = queued/offline; Pocket's Working green + "Running" [P d/ui/src/ui.rs:457] and Done accent dot [R07 §F15] | 12-3 shape cue, as built in UXD §3.3: `WAITING_BG` row tint + 7 px dot + "Needs you" label (no dashed border) |
| D2 | Sidebar stable; attention surfaces sort by urgency | Pocket's status sort [P d/pocket/src/view.rs:481,665] and status sections [R05 §F11]; settles R04's open question on recency vs urgency [R04 §Open questions] | 04-1 for the sidebar; 09-3, 08-11, 15-15, 09-1 sort by urgency |
| D3 | Expo Push from pocketd; Seen-aware transitions; one collapse id per session | 08-1 and 19-22 (direct APNs); 01-3 (45 s staleness gate, [R02 §F3]); 05-5's "Done fresh ≤ 45 s" rule for push; 15-4's presence-file suppression (Seen replaces it) | 02-2 = 19-12; 08-3 and 01-2 collapse/thread id = session |
| D4 | Codex in Pocket terminals via pocketd's existing app-server client | 10-2 for terminal sessions; 14-6's XL structured driver narrows to Claude headless | 03-1; 10-2 only for headless sessions |
| D5 | One `agent.create` with LaunchSpec; pocketd builds argv; phone limited per R18 | 11-3's separate `agent.start` / `terminal.spawn` for the phone (`terminal.spawn` stays owner-only, [R18 §F7]); desktop-side argv building [R17 §F13]; 08-21's open security review | 10-3, 14-19, 17-10, 18-10 as one message |
| D6 | Keep ⌘J, ⌘K, ⌘T, ⌘N, ⌘⇧N; ⌘1–9 = sessions in sidebar order | Zeron Mod+J drawer [R04 §F4]; MonoCode ⌘K clear and ⌘J hide terminal [R16 §F4]; 09-12's ⌘1–9 = tabs; 17-14's ⌘1–9 inside cards (use plain 1–9, as 01-13); 01-4 Mod+Shift+A | New, collision-free vs [P d/pocket/src/main.rs:1062-1071]: ⌘F (16-14), ⌘/ (17-14), Ctrl+Tab (04-3), ⌘⇧↑/↓ (09-12), ⌘⌥arrows (12-8), ⌘⇧U (15-15) |
| D7 | Warn 75 %, danger 90 %, reported window, hidden when unknown | 08-17 (≥ 50 % chip, warn ≥ 85 %); 05-7's "—" when unknown; Pocket's > 80 % warn and 200 000 window [P d/ui/src/ui.rs:667; R05 §F11] | 14-21 carries the window in AgentSummary |
| D8 | Deleting a worktree keeps its branch | 06-14 (`branch -D` when Pocket created it) | 13-15 dialog, minus any branch delete |
| D9 | Terminal surface in scope | R06's "little to copy" [R06 §9] | 16-1…16-20; 14-16 splits into 16-1/2/4/8/9/14 [R16 ideas note] |
| D10 | R18 fixes gate phone spawn and remote reach | 08-18's QR carrying the token (→ 18-4 one-time code); 15-12 shared-seed E2EE (→ 18-16 Noise); 11-2 (subsumed by 18-8); relay without E2EE (02-1, 08-22); Zeron "every device trusted" and absolute reads [R18 §F4] | Order 18-1 → 18-2 → 18-3 → 18-4 → 18-6 → 18-8 [R18 §TL;DR]; 16-18, 18-10, 08-19 wait on it |
| D11 | Banners suppressed for Seen; sounds on non-Seen transitions even when focused; per-cue toggles, default on | Zeron's background-only default (01-1, 04-9, 05-5) [R04 §F9]; 08-3's blanket foreground mute narrows to the Seen agent | Pocket's dismiss-on-Seen [R05 §F11] |
| D12 | Fix now: 17-1, 16-3, 05-2, 18-8 ahead of features | Ordering only. R18 lists 18-8 ← 18-6 ← 18-3 ← 18-2 as prerequisites, so the self-approval fix is that whole chain, not one S item | 16-3: confirm first by typing `a --b` [R16 idea 16-3] |
| D13 | macOS desktop + iPhone only | Zeron Windows/Linux builds [R01 §F5]; MonoCode Windows builds [R09 §F4]; Android: Happy, Nimbalyst [R15 §F16], 19-12's Android note, R16's Android native-module path [R16 §F6] | iPad (08-20) stays in the matrix at L value |

Other cross-report conflicts, settled by the later report:
- 10-4 vs R17: codex rejects `-a untrusted`; Ask = `-s read-only -a on-request` [R17 §F14].
- 14-17 raw VT attach vs 16-13: server-rendered frames, no VT on the phone [R16 §F6].
- 01-10 / 02-7 bootout vs 19-2: never boot out a running pocketd [R19 §TL;DR].
- R03's Claude headless handoff via `--resume` vs R10: don't respawn claude on every settings change [R10 §11].

## 4. Rejected

- 02-14 HLC LWW registry: Zeron's own "wont" [R02 idea 02-14]; one Mac owns state.
- 02-15 CRDT timelines: XL, "wont" [R02 idea 02-15]; pocketd is the sole writer, a journal suffices (02-11).
- 08-22 hosted relay: no-cloud stance; relay only as 18-16 with E2EE [R18 §F7].
- 08-1, 19-22 direct APNs: D3.
- 01-3 45 s staleness gate: D3.
- 05-20 Zeron status palette: D1.
- 06-14 `branch -D` on worktree delete: D8.
- 01-4 Mod+Shift+A archive: a session ends with its agent and stays listed until its terminal closes [CONTEXT.md §Session]; Close session covers it.
- 09-5 plan-usage meter: undocumented endpoint, breaks without notice [R09 idea 09-5].
- 09-18, 10-19, 17-16 SSH remote hosts / host picker: Mac + phone model [R18 §F7].
- 17-15 projectless sessions: Pocket is project/worktree-first [R17 idea 17-15].
- 17-18 model favourites: two providers, small catalogs [R17 idea 17-18].
- 09-19 Handoff / Second opinion / BTW: not portable [R09 idea 09-19].
- 11-16 external issue Inbox: out of scope [R11 idea 11-16].
- 11-17 Notes: skip [R11 idea 11-17].
- 13-18 dirty-tree branch switch: conflicts with worktree-first [R13 idea 13-18].
- 13-19 arcade games: no user value [R13 idea 13-19].
- 05-25 animated backgrounds: decoration only [R05 idea 05-25].
- 07-13 theme families / VS Code import: XL [R07 idea 07-13].
- 07-15 in-scene menu blur: needs a gpui fork [R07 idea 07-15].
- 12-18 interface scale: unit sweep, low demand [R12 idea 12-18].
- 15-12 shared-seed E2EE: can't revoke one device; replaced by 18-16 [R18 §F7].
- 01-23, 06-10 dev-server previews on `*.localhost` / embedded browser: XL; keep 06-9 discovery + 15-10 port block.
- 01-22, 05-24 Appshots: L, outside attention and terminal work.
- Zeron auto-approve / yolo default: Pocket's permission bridge is the edge [R08 §F15].
- Zeron trust model (every device trusted, absolute reads, TOFU room ownership, no E2EE) [R18 §TL;DR].
- Claude Squad `--autoyes`: presses Enter blindly [R15 §Ideas].
- Kanban boards, Warp share links, Superset resources/Design/Pages, cloud agents [R15 §Ideas].
- libghostty-vt or xterm.js webview on the phone: XL / new dependency [R16 §F6].
- Windows, Linux, Android items: D13.
