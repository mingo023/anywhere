# 05 — Zeron: transcript, composer, notifications UX

Date: 2026-09-30

Sources:
- Zeron: github.com/zeronsh/comet @ `ed3b1aa` (2026-09-29). Local clone: /Users/mingo/tmp/orchestrators/zeron. Includes the in-repo docs (`docs/*.md`) and the screenshots under `docs/screenshots/`.
- Pocket: this worktree @ `86deb13`.

Citation legend:
- `Z path:L` is a file and line in the Zeron clone, relative to its root.
- `P path:L` is a file and line in this Pocket worktree, relative to its root.
- `M` (monocode) is not cited.
- "shot:" cites a PNG under `Z docs/screenshots/`.
- Line numbers are for the SHAs above.
- When a Zeron doc or screenshot disagrees with its code, the code is treated as truth and the gap is flagged **[doc≠code]**.

## TL;DR

- **Zeron's desktop transcript is a virtualized row list.**
  - Per-row streaming polish: a stick-to-bottom spring, an own-send "runway", and a per-chunk opacity veil.
  - Markdown is "mended" during streaming (unclosed `**`, backticks and links are auto-closed for display).
  - Tool calls collapse into one summary line per group, e.g. "Ran 3 commands · edited 2 files · 1 failed" (Z crates/proto/src/view.rs:446).
- **Tool groups open while streaming and fold once settled.** Every chip expands to its invocation and output. Diffs and output past the doc clamp are fetched lazily ("Show full output (12 KB)").
- **The composer morphs Send/Queue/Stop.**
  - Send: no live run.
  - Queue: live run with content.
  - Stop: live run with an empty composer.
  - Enter never stops a run (issue #406; Z crates/ui/src/composer.rs:495,7566).
- **The queue is a first-class, CRDT-shared panel** above the composer. Per-row actions are Steer / Send next / Send now, with drag-reorder and device edit leases (Z crates/ui/src/queue.rs:100-102).
- **Zeron has no tool-permission UI.**
  - The Claude harness auto-allows every `can_use_tool` (Z crates/harness/src/claude/mod.rs:967-990).
  - Only AskUserQuestion is surfaced, as a paged wizard that replaces the composer: number keys, 220 ms auto-advance, free-text override (Z crates/ui/src/composer.rs:104,8610-8810).
  - Pocket's permission sheet is ahead here.
- **One notification detector drives both chimes and banners.**
  - Semantics: seed the baseline silently; Done only if fresh (≤45 s) and not an interrupt; 250 ms attention coalesce; banners off while focused (default); side chats silent (Z crates/ui/src/shell.rs:2585-2675; Z crates/ui/src/sound.rs:319-356).
  - Chimes still play when the app is focused.
- **Context ring:** 16 px, amber ≥75%, red ≥90%. Click opens a card: "184,000 / 200,000 tokens · 16,000 tokens remaining". The window comes from the harness; nothing is shown if the harness doesn't report one (Z crates/ui/src/context_usage.rs). **[doc≠code]** The doc says hover.
- **Pocket's phone composer has the #406 bug.**
  - While busy, the only button is Stop, so a typed follow-up can only be sent via the keyboard return key; tapping the button interrupts (P packages/app/src/components/Composer.tsx:50-55).
  - The input is also single-line and has no draft persistence.
- **Highest-value ports:**
  - Tool summary line.
  - Send/Queue/Stop morph.
  - Queue with Steer / Send-next (Pocket's PTY typing already steers).
  - AskUserQuestion wizard (pocketd already hooks PreToolUse `AskUserQuestion`).
  - Notification-detector rules plus three sound cues.
  - Context ring with a reported window instead of the hardcoded 200k.

## Findings

### F1. Transcript model and row types
- **Rows are built per entry by `rows_for_entry`** (Z crates/ui/src/transcript.rs:1286-1738). RowKind enum at Z crates/ui/src/transcript.rs:1000. Row kinds:
  - user bubble
  - assistant markdown
  - tool group
  - working trailer
  - notes/errors
- **The list is virtualized** with OVERDRAW_PX 320 (Z crates/ui/src/transcript.rs:64).
- **Tool items are Call / Thought / Note** (ToolItemKind; Z crates/ui/src/transcript.rs:333).
  - Thought and Note chips are UI-synthesized and ride inside the group.
  - Summary prefixes: "Thought process" (1) and "thought N times" (Z crates/ui/src/transcript.rs:1947-1990; Z crates/mobile/src/layout/tools.rs:103-122).
- **ToolCall enum** (Z crates/proto/src/agent.rs:300): Exec, ReadFile, WriteFile, EditFile, ApplyPatch, Search, Glob, WebFetch, WebSearch, Todo, Mcp, Unknown.
  - Subagents decode as `Unknown{name:"Agent: <desc>"}` and are labeled "Agent" (Z crates/proto/src/view.rs:428-437).
- **Chip label and detail per kind** (Z crates/proto/src/view.rs:400-438):

  | Kind | Label | Detail |
  |---|---|---|
  | Exec | Run | the command |
  | ReadFile | Read | path |
  | WriteFile | Write | path |
  | EditFile | Edit | path |
  | ApplyPatch | Patch | path, or "workspace" |
  | Search | Search | "pat in path" |
  | Glob | Glob | pattern |
  | WebFetch | Fetch | url |
  | WebSearch | Web | query |
  | Todo | Todo | "d/n done" |
  | Mcp | MCP | "server · tool" |
  | Unknown | Tool | name |

  Details are single-lined.
- **Group summary** (Z crates/proto/src/view.rs:446-515):
  - Segments, in order: `ran N commands`, `edited N files` (deduped by path), `read N files`, `searched N times`, `fetched N pages`, `updated todos`, `called N tools`, `N failed`.
  - Search, Glob and WebSearch all count as "searched"; Mcp and Unknown count as "called N tools"; with no segments it falls back to "N tools".
  - Joined with " · ". Only the first letter is capitalized.
  - One implementation serves desktop and iOS (Z crates/mobile/src/layout/tools.rs:112).
  - **[doc≠code]** shot: mobile-tool-disclosure/tool-group-expanded.png shows "1 search"; the code emits "searched 1 time".
- **Status dot palette**, oklch (Z crates/proto/src/view.rs:524-534):

  | State | oklch | Code comment |
  |---|---|---|
  | working | (0.718, 0.202, 349.761) pink | "Pink, not amber: the harsh yellow read as a warning, and running is routine" |
  | awaiting | (0.673, 0.182, 276.935) indigo | |
  | errored | (0.704, 0.191, 22.216) red-400 | |
  | completed-unseen | (0.765, 0.177, 163.223) emerald | |

- **User bubble:**
  - Max width 80%, px16 py10, 14/22 text.
  - Long prompts collapse past 5 lines or 400 chars (USER_COLLAPSED_LINES / USER_COLLAPSE_CHARS; Z crates/ui/src/transcript.rs:166-173). The toggle reads "Show more" / "Show less" (Z crates/ui/src/transcript.rs:5774).
  - An optimistic (pending) bubble renders at opacity 0.65 (Z crates/ui/src/transcript.rs:6472).
  - Hover reveals a timestamp and a copy button, tooltip "Copy message" (Z crates/ui/src/transcript.rs:6679).
  - Attachment thumbs are 112×80 (Z crates/ui/src/transcript.rs:179-180).
- **Send failure copy:**
  - "Not delivered — click to retry" (Z crates/ui/src/transcript.rs:6263).
  - The queued pending state reads "Queued — will send automatically" (Z crates/ui/src/transcript.rs:6314).

### F2. Tool group disclosure
- **Header** (Z crates/ui/src/transcript.rs:6990-7400):
  - 26 px tall (TOOL_GROUP_HEADER_HEIGHT; Z crates/ui/src/transcript.rs:112), gap 6, pr 4.
  - Color: text_muted, text on hover.
  - It stays quiet even on failure; failure appears only as the "· N failed" segment.
  - Chevron: ALT_ARROW_DOWN at 14 px, rotation −90°·(1−progress).
- **Open/close rules:**
  - Auto-opens while the group streams or chips are arriving.
  - Folds once settled.
  - Never auto-opens in compact mode.
  - Agent/spawn chips never fold, except in compact mode.
  - A collapsed, settled body is not built at all (render cost).
- **Tree layout:**
  - 48 px gutter, trunk at x 12.5, bend radius 6, branch end at x 28, icon at left 32 (16 px).
  - Rows 32 px, label 12/18.
  - Chips 38 px (card 30) (Z crates/ui/src/transcript.rs:91-114).
  - shot: tool-group-expanded shows icon + label rows over a mono detail line, with a vertical guide; a failed row's label is red ("Run Failed").
- **Motion** (Z crates/ui/src/transcript.rs:115-128):
  - Fold: 140 ms ease-out, height lerp within a 400 ms tween window.
  - Row reveal: 360 ms expo.
  - Connector reveal: 480 ms quint.
  - First row delay 90 ms, then a 65 ms stagger.
  - Header shimmer: 3400 ms sweep, half-width 0.36, strip width 2.
- **Every chip expands** to show its invocation and output/diff (ToolDetail; Z crates/ui/src/transcript.rs:751-935). A streaming Thought chip defaults open.
- **Sidecar blobs** (Z crates/ui/src/transcript.rs:6990-7400):
  - The doc clamps tool output to a 160-char summary (TOOL_OUTPUT_SUMMARY_MAX; Z crates/doc/src/parts.rs:18); the full diff/output is fetched on demand from a sidecar (Z crates/ui/src/transcript.rs:357-362).
  - Copy: "Show full {diff|output} ({kb})", "Loading full {what}…", "Couldn't load full {what} — tap to retry".
  - The most recent request wins.
- **Compact mode:** the header crossfades between the live summary and "Worked for {elapsed}" (Z crates/ui/src/transcript.rs:2252-2260).

### F3. Streaming and working state
- **Working trailer** (Z crates/ui/src/transcript.rs:6224):
  - A rotating flavour word. 21 words ("Zeroning", "Thinking", "Pondering", "Sifting", "Combobulating", …), rotated every 7 s from a per-chat FNV seed (Z crates/ui/src/transcript.rs:2187-2221).
  - Elapsed time, formatted to at most two units: `45s`, `3m 12s`, `2h 5m`, `1d 3h` (Z crates/ui/src/transcript.rs:2239-2250).
  - shot: send-pending shows "⁘ Sifting… 15s".
- **"Sending…" bridge:** shown while send_started is at or after the row's turn_started, so the timer doesn't count the round-trip and then restart (Z crates/ui/src/transcript.rs:2223-2236).
- **Finished turns** show "Worked for 3m 12s" (Z crates/ui/src/transcript.rs:2252).
- **Veil:** each streamed chunk fades in on opacity only, with zero translate (Z crates/ui/src/markdown/veil.rs).
  - Duration = clamp(EMA(inter-chunk)×3, 120, 400) ms, EMA seed 160 ms.
  - Curve α = 1−(1−p)^1.6.
- **Markdown streaming:**
  - The incremental parser re-parses from the last stable top-level block.
  - `mend` auto-closes `**`, `*`, backticks, `~~` and links (pending URL), with a setext guard. Display only (Z crates/markdown/src/lib.rs, mend.rs).
- **Loaders** (Z crates/ui/src/loaders.rs):
  - gradient_spinner: a 3×3 cell grid, 750 ms. Row tints #B6D3EF / #EDB185 / #F888A0, dim 0.1.
  - mini_glyph_spinner: a 2×3 perimeter snake.

### F4. Scroll behaviour
- **Stick-to-bottom:**
  - The stick threshold is 70 px (Z crates/ui/src/transcript.rs:62).
  - Spring: damping 0.7, stiffness 0.05, mass 1.25, 60 fps frames, max catch-up 8 frames, growth EMA 0.12, chase lead ≤32 px, settle grace 500 ms (Z crates/ui/src/transcript.rs:188-214).
  - "At bottom" means within 2 px (Z crates/ui/src/transcript.rs:203).
- **Jump pill:**
  - Appears past 320 px from the bottom (SCROLL_BUTTON_THRESHOLD_PX; Z crates/ui/src/transcript.rs:66).
  - Shape: "↓" + "Scroll to bottom", both 13 px (arrow muted). h30, rounded-full, hairline border, pl11 pr13, gap 6.
  - Placement: top −36 over the composer, right 10.
  - Frosted (blur 16), dialog_in animation (Z crates/ui/src/shell.rs:10162-10260).
- **Own-send runway** (`on_own_send`; Z crates/ui/src/transcript.rs:3944):
  - Resets pinned, the jump pill and the spring.
  - Anchors the just-sent prompt 48 px from the top (TITLEBAR 38 + 10; Z crates/ui/src/transcript.rs:220) and leaves blank runway below for the reply.
  - Glide retain 0.85, snap at 1 px.
  - A queued send defers until its row materializes.
  - Z docs/performance-runway-*.md reports fixing a 660→0 px jump.
- **Turn rail** (Z crates/ui/src/rail.rs):
  - A left minimap of prompts. Hidden below 768 px container width.
  - Ticks: at most 12 (larger counts are bucketed). Slot 10, gap 3, vertical margin 24. 2 px tall; 12 px wide at rest, 20 px on hover.
  - Colors: text·0.8 when active/hovered, ink 0.16 at rest.
  - Hover card: 280 px, p8, gap 6. Prompt at 12 px (≤160 chars), reply at 11 px muted (≤200). A bucket reads "{n} prompts".
  - Clicking glides there, 500 ms ease-in-out (SCROLL_GLIDE; Z crates/ui/src/motion.rs:299-415).

### F5. Composer
Z crates/ui/src/composer.rs, 14 174 lines. The header at L1-8 summarizes its scope: a hand-rolled input, the compact↔expanded flip, the Send/Queue/Stop morph, optimistic send with failure recovery, per-chat drafts, and the question wizard.

- **Geometry:**
  - Textarea 76–260 px. Actions row 42. Radius 26. Total height 120–304. Compact total 49.
  - Max width 768. Input text 14/22.75, with a 12 px fade band.
  - L55-104. Growth animation 180 ms (Z docs/performance-composer.md:39).
- **Flip rules:**
  - A newline always expands.
  - Width below 200 px always expands.
  - Collapse hysteresis 32 px; resize settles after 150 ms.
- **Button morph** (SendButtonMode, L495):
  - Send: no live run.
  - Queue: live run with content (attachments and review comments count as content).
  - Stop: live run with an empty composer.
  - Stop button: a 28 px circle on bg=text with an 11 px square (r3), tooltip "Stop" (L8820-8834).
- **Keys:**
  - Enter submits and never stops (#406, L7566; test L10634).
  - Cmd/Ctrl+Enter submits content, or with an empty composer activates the newest queued row (L7580-7592).
  - shift-enter inserts a newline. Tab accepts a mention; shift-tab outdents.
  - Word editing uses alt (mac) / ctrl.
  - Esc-stops-agent is off by default (Z crates/ui/src/settings.rs:945).
  - Send behavior: Enter (default) or ModEnter (Z crates/ui/src/settings.rs:569).
- **Placeholder:** "Do anything…" (L5469). shot: send-enabled shows the model chip "Mock 1 Medium", a paperclip and send, with "Demo workstation ⌄" and "No project ⌄" above.
- **@mentions** (L4785, L6624-6642):
  - File search via the SEARCH_FILES RPC: 80 ms debounce, one retry after 250 ms on transport error.
  - Popup rows: 16 px file icon, name + directory. Loading shows 3 skeleton rows. Empty: "No files available" / "No matching files".
  - Inserts `[basename](zeron-file:path)` (Z crates/proto/src/file_mentions.rs:50), rendered as a chip.
  - Drag-drop uses the same path. A pasted `@path` resolves only on an exact match.
- **Slash** (L4830):
  - `/` lists commands and `$` lists skills (LIST_COMMANDS, LIST_SKILLS).
  - Row: icon + "/name" + description · `<hint>`.
  - Empty copy: "No commands or skills available", "No matching commands", "This agent does not advertise skills".
- **Paste** (L2832):
  - An image or file beats text.
  - Clipboard metadata `zeronComposerV1` preserves chips.
  - Undo coalesces within 700 ms, limit 200 steps.
- **Attachments** (Z crates/ui/src/attachments.rs):
  - Limits: 24 MiB max. Upload chunks of 680 000 base64 chars (fits Cloudflare's 1 MiB frame), concurrency 3. Image cache 64 MiB.
  - Formats: png/jpg/gif/webp/svg/bmp/tiff.
  - An image-only message sends the body "See the attached image(s)."
  - The prompt appends "Attached images (local files — open them to view):\n- path". This is plain text, so it works for any CLI.
  - Strip thumbs 56 px, gap 8. Upload progress ring stroke 2.5.
- **Pickers** (Z crates/ui/src/pickers.rs):
  - Pickers: repo, branch (with an isolated-worktree toggle), harness+model (the harness locks once the chat exists), traits.
  - The traits summary reads "High · 1M · Fast".
  - Reasoning levels: Minimal…Max, plus Ultra/Ultracode/Ultrathink. A "Default" badge marks the default.
  - Footer hint: "↑↓ Navigate ↵ Select".

### F6. Queue and steering
- **Storage:** the queue lives on the session CRDT doc, so the phone sees the same queue (Z crates/ui/src/queue.rs).
- **Per-row primary action** (Z crates/ui/src/queue.rs:100-102):
  - "Steer (keep current work running)"
  - "Send at the next turn (this agent cannot steer mid-turn)"
  - "Send now (interrupt)": used for rows with attachments
  - Disabled: "Waiting for provider capabilities" (L1004)
  - The capability comes from `SteeringMode` StepBoundary vs TurnBoundary (Z crates/proto/src/agent.rs:165-178).
- **Geometry:**
  - Rows 36 (L76), text 12.5, pad 8, radius 8. Panel radius 16, pad 4, icon 13.
  - Button 72×28, r5, 11.5 px, tooltip delay 350 ms. Hint "⌘↵".
  - Max height 30% of the viewport (L349). Frosted.
  - It overlaps the composer by 18 px with a 16 px side inset (Z crates/ui/src/composer.rs:75,78).
- **Reorder and edit:**
  - Drag to reorder.
  - Editing takes a lease. Copy: "Editing in composer", "Save to queue" (L493), "Cancel", "Editing on {device}" (L432), "Needs review".
- **Execution:** SEND_QUEUED_MESSAGE_NOW and STEER_QUEUED_MESSAGE_NOW run host-side.
- **How Claude steers:** queued steer messages are written to stdin as user lines at any time; the CLI folds them in at its own step boundary (Z crates/harness/src/claude/mod.rs:28-30).

### F7. Permissions and questions
- **No approval UI.** The Claude harness passes the undocumented `--permission-prompt-tool stdio` (Z crates/harness/src/claude/mod.rs:8-16,186-188) and auto-allows every `can_use_tool` except AskUserQuestion; `auto_approve` adds `--permission-mode bypassPermissions --dangerously-skip-permissions`, else `--permission-mode default` (Z crates/harness/src/claude/mod.rs:209-217,966-990). SandboxLevel is ReadOnly / WorkspaceWrite / DangerFullAccess (Z crates/proto/src/agent.rs:165). The composer sets WorkspaceWrite with auto_approve false (Z crates/ui/src/composer.rs:8213).
- **Question wizard** (AskUserQuestion):
  - `pending_input_request` finds the unresolved Input part on the last assistant entry, even after the run died (L624-641).
  - Pure reducer (L667-720): paged "1/3". Single-select auto-advances after 220 ms. Keys 1–9 pick. Esc goes back when the input is empty or unfocused.
  - Render (L8610-8810):
    - Radius 26. Header uppercase-tracked 10.5 px. Counter chip h20.
    - Question 15/20 medium. Multi-select hint "Select one or more options." (L8761).
    - Option rows px14 py10 r12. Picked: border ink 0.16, bg 0.09. Hover bg 0.025→0.06. kbd chip 22 px r6.
    - A free-text field below a hairline. Placeholders: "Type your own answer, or pick an option above" / "Type your own answer, or leave this blank to use the selected option" (L8450-8452).
    - Buttons: "Back" (ghost) and "Next"/"Submit" (primary, 0.4 opacity when it can't advance).

### F8. Context usage
- **Chip** (Z crates/ui/src/context_usage.rs):
  - h24, px6, r6, 11 px, gap 5, hover ink 0.05.
  - Ring 16 px, radius 6, stroke 1.8. Track text_faint·0.25; starts at 12 o'clock.
- **Color:** ≥0.9 danger, ≥0.75 warning, else muted. Unknown shows text_faint "—".
- **Card:** "Context window", then "{used} / {window} tokens" and "{remaining} tokens remaining", with comma separators.
  - Other states: "Context limit not reported", "Waiting for context usage", "Context usage not reported by this harness yet".
  - If the harness reports no window, no chip is shown.
- **[doc≠code]** The card opens on click (toggle) (Z crates/ui/src/account_usage.rs:285,438-450); Z docs/context-usage.md says hover. shot: its tooltip shows "184000 / 200000" without commas (stale).
- **Plan-usage ring** beside it: re-probes on hover (≥30 s apart) and polls every 5 min.

### F9. Notifications and sound
- **Detector** (Z crates/ui/src/shell.rs:2585-2675):
  - A row's first sighting seeds the baseline silently. Side chats (`parent_chat_id`) never notify.
  - Sound plays if that cue is enabled; Attention passes through a 250 ms coalescing gate (AttentionSoundGate; Z crates/ui/src/sound.rs:412).
  - The banner is posted if `notifications_enabled && !(background_only && focused)`.
  - Banner title: the chat title, or "New session". Body: Done "Run finished", Request "Waiting on your input", Attention "Run failed" (L2644-2648).
- **Transition rules** (SessionNotificationState; Z crates/ui/src/sound.rs:319-356):
  - Errored → Attention.
  - AwaitingInput → Request.
  - Done fires only when all hold:
    - !send_pending
    - the row was updated within SESSION_STALE_MS 45 000
    - last_completed_turn changed
  - Interrupts never ring Done.
- **Connectivity banner:** "Connection unavailable" + "Your device is offline" / "Zeron is trying to reconnect" (L2673). The engine holds 4 s before reporting, with a 5 s startup quiet period (Z crates/ui/src/sound.rs:383).
- **Agent-update banner:** debounced 1 s. "{n} agent update(s) available" / "A coding agent update is ready" (L2729-2735). Clicking routes to Settings → Agents.
- **Sound playback** (Z crates/ui/src/sound.rs:29-38):
  - Cues: Done / Request / Attention, plus the appshot cue. Embedded WAVs, 48 kHz, 0.30–0.79 s.
  - Players: afplay / PowerShell / paplay|pw-play|aplay|ffplay|mpv.
  - Kill switches: env `ZERON_DISABLE_SOUND`, `ZERON_DISABLE_NOTIFICATIONS`.
  - **[doc≠code]** Z docs/sound-design/README.md auditions send/queued/undo cues; only done/request/attention/appshot ship. Doc rule: frequent actions stay silent (auditions manifest L22).
- **Banner delivery** (Z crates/ui/src/notify.rs): macOS NSUserNotification with a delegate (osascript fallback); Linux notify-send; Windows no-op. A click routes via userInfo `chatId`.
- **Settings copy** (Z crates/ui/src/settings/notifications.rs):
  - Page "Notifications" with sections "Desktop" and "Sounds".
  - Toggles: "Desktop notifications", "Only when in the background", "Agent updates" ("Show a banner when monitored agent CLIs have updates."), "Session sounds", "Task completed", "Input required", "Errors and disconnections".
  - Defaults, all on (Z crates/ui/src/settings.rs:932-951): sound, notifications, background_only, appshot sound.

### F10. Other surfaces (low transfer)
- **Appshots** (Z crates/ui/src/appshots.rs; Z docs/appshots.md):
  - Global hotkey ctrl-alt-space captures app windows plus AX text as untrusted context: "Applications mentioned by the user (untrusted observed content):".
  - Limits: 900 ms AX budget, 8192 px / 32 Mpx caps.
- **New-thread screen:**
  - The composer is centered (hero_limit = (vh−h)·0.5−8) and glides to the dock over 420/470 ms (Z crates/ui/src/composer_dock.rs).
  - Background effect: None/Dither/Ascii/Halftone/Scanlines (Z crates/ui/src/settings.rs:79).
- **Transcript width:** 560–1200 px, default 736, step 16 (Z crates/ui/src/settings.rs:214-217).

### F11. Pocket today (comparison)
- **Protocol** (P packages/protocol/src/timeline.ts:1-113):
  - Item kinds: user / assistant / thinking / tool / tasks / plan / compact / result.
  - ToolDetail: shell/read/edit/write/search/task/other.
  - Tool status: running/ok/error, plus durationMs. TurnUsage {input, output, cacheRead}.
- **pocketd folding** (P packages/pocketd/internal/timeline/timeline.go:1-208):
  - TodoWrite becomes tasks; ExitPlanMode becomes plan.
  - Output is clamped at 64 KiB + "… truncated" (P packages/pocketd/internal/proto/proto.go:12; P packages/pocketd/internal/timeline/timeline.go:177).
  - Claude events come from JSONL tailing, in whole blocks; sidechain lines are dropped (P packages/pocketd/internal/claude/transcript.go:37,59).
  - Codex streams `item/agentMessage/delta` (P packages/pocketd/internal/codex/session.go:157-159).
- **Prompting:** a PTY write, then 150 ms, then `\r` (P packages/pocketd/internal/terminal/terminal.go:278). Fails with errNotForeground (P packages/pocketd/internal/daemon/presence.go:123).
  - Typing while Claude works already queues/steers inside the TUI, so this is Zeron's "Steer" for free, but invisible to the user.
- **Phone timeline** (P packages/app/src/components/TimelineView.tsx:1-281):
  - Consecutive tools become groups (toRows L29-41).
  - Thinking collapses to its first line (≤90 chars).
  - Footer: "Working 12s · model" / "Waiting for approval" / "Compacting context Ns", with an 800 ms pulsing asterisk.
  - Result line: "Done in X.Xs." + "Nk in · Nk out".
  - Pin slack 32 px. Jump control: a 46 px glass circle.
  - The timer starts at `Date.now()` when status flips to working, so it resets on screen remount (P packages/app/src/screens/ChatScreen.tsx:107-114).
- **ToolGroup** (P packages/app/src/components/ToolGroup.tsx:1-120):
  - Header 40 px, "N actions · Ns", open by default.
  - Rows 36 px: glyph + mono name + text + DiffStat + status.
- **Composer** (P packages/app/src/components/Composer.tsx:15-58):
  - Single-line TextInput with local state, so no per-chat draft.
  - "+" and Mic are not wired.
  - The button is Stop whenever busy (the #406 bug); the return key still sends.
  - Placeholder "Message {provider}…". `/compact` is intercepted (P packages/app/src/screens/ChatScreen.tsx:51,177).
- **Permissions:** PermissionSheet with "1. Yes" / options / "N. No" + feedback "No, and tell Claude what to do differently" (P packages/app/src/components/PermissionSheet.tsx:1-126).
  - pocketd hooks PreToolUse for `AskUserQuestion|ExitPlanMode` (P packages/pocketd/internal/daemon/plugin.go:13), but there is no question UI.
- **Desktop:**
  - Terminal panes. System notifications keyed by agent id, dismissed once seen; a click focuses the agent (P packages/desktop/crates/pocket/src/main.rs:315-327,331-344,1103; P packages/desktop/crates/pocket/src/status.rs:119-120).
  - Sidebar sections: NeedsYou > Failed/Done (shared) > Working > Idle today > older Idle (P packages/desktop/crates/pocket/src/status.rs:82-90).
  - Inbox with j/k navigation (P packages/desktop/crates/pocket/src/inbox.rs:28-75).
  - Context bar: 32×4 px, warning past 80% used. CONTEXT_WINDOW is hardcoded to 200 000 (P packages/desktop/crates/agents/src/agents.rs:57,158-161; P packages/desktop/crates/ui/src/ui.rs:665).
  - No sounds.

## Ideas to clone into Pocket

| ID | Idea | User value | Evidence | Pocket mapping | Effort | Prereqs |
|---|---|---|---|---|---|---|
| 05-1 | Tool-group summary line ("Ran 3 commands · edited 2 files · 1 failed"), deduping edited paths; failure only as a segment | Scan a turn without expanding it | Z crates/proto/src/view.rs:446-515 | app `ToolGroup.tsx` header, replacing "N actions" (port); desktop inbox subtitle (adapt) | S | none; map ToolDetail kinds |
| 05-2 | Send/Queue/Stop morph; Enter/return never stops | Follow-ups while busy no longer interrupt by accident | Z crates/ui/src/composer.rs:495,7566; P packages/app/src/components/Composer.tsx:50-55 | app `Composer.tsx` (port) | S | none |
| 05-3 | Visible queue: Steer (type now) / Send next (hold until Stop hook) / Send now (interrupt + send); reorder, edit, delete | See and control what the agent will get next, from the phone | Z crates/ui/src/queue.rs:100-102,349; Z crates/proto/src/agent.rs:165-178 | pocketd per-agent queue + `agent.queue.*` WS msgs (new); app queue panel above composer (new) | M | Stop-hook turn boundary (exists); protocol schema |
| 05-4 | AskUserQuestion wizard: paged, 1–9 keys, 220 ms auto-advance, free-text override | Answer agent questions on the phone instead of "needs you → open terminal" | Z crates/ui/src/composer.rs:104,624-720,8610-8810; P packages/pocketd/internal/daemon/plugin.go:13 | pocketd parse PreToolUse input into `question.request` (new); app wizard replacing composer (new) | L | a way to feed answers back: hook response vs TUI keystrokes (open question) |
| 05-5 | Notification detector rules: silent baseline, Done only if fresh ≤45 s and not an interrupt, 250 ms coalesce, background-only banner, subagents silent | Fewer false or duplicate alerts after reconnects | Z crates/ui/src/sound.rs:319-356,412; Z crates/ui/src/shell.rs:2585-2675 | desktop `status.rs` alerts() + `main.rs` (adapt); phone push later | S | interrupt flag on agent summary |
| 05-6 | Three sound cues (done / request / attention) with per-cue toggles and an env kill switch; frequent actions silent | Hear when an agent needs you without watching | Z crates/ui/src/sound.rs:29-38; Z docs/sound-design/README.md | desktop crate: embedded WAV + afplay (new); settings toggles | M | sound assets (licensing, open question) |
| 05-7 | Context ring (16 px, 75/90 thresholds), click card "used / window tokens · remaining"; "—" if unknown | Know when to /compact before quality drops | Z crates/ui/src/context_usage.rs; P packages/desktop/crates/agents/src/agents.rs:57 | pocketd reports model window in AgentSummary (new); app header chip (new); desktop bar uses it (adapt) | S | window per model (JSONL `model` → table) |
| 05-8 | Working trailer: rotating flavour word (7 s, seeded), 2-unit elapsed, "Sending…" bridge, "Worked for Xm Ys" | Liveness plus an honest timer that survives remounts | Z crates/ui/src/transcript.rs:2187-2260,6224 | pocketd `turnStartedAt` on AgentSummary (new field); app `TimelineView` footer (adapt) | S | none |
| 05-9 | Optimistic user bubble (opacity 0.65) + "Not delivered — tap to retry" on errNotForeground or WS failure | Sent prompts are never silently lost | Z crates/ui/src/transcript.rs:6263,6472; P packages/pocketd/internal/daemon/presence.go:123 | app `session.tsx` pending items (new); pocketd ack/err on agent.prompt (adapt) | S | prompt ack frame |
| 05-10 | Tool groups fold when settled, open while streaming; per-kind label/detail vocabulary (Run/Read/Edit/Search/Fetch/Todo/MCP/Agent) | Long turns stay short; live work stays visible | Z crates/ui/src/transcript.rs:6990-7400; Z crates/proto/src/view.rs:400-438 | app `ToolGroup.tsx` (adapt) | S | none |
| 05-11 | Per-chat drafts + multiline growing input | Switching chats doesn't lose half-typed prompts | Z crates/ui/src/composer.rs:1-8,55-104 | app `Composer.tsx` + session store keyed by agentId (new) | S | none |
| 05-12 | Collapse long user prompts (>5 lines / 400 chars) with "Show more/less"; long-press copy + timestamp | Pasted logs don't bury the transcript | Z crates/ui/src/transcript.rs:166-173,5774,6679 | app `TimelineView` user bubble (port) | S | none |
| 05-13 | Image attachments: "+" picks photos, pocketd writes files under the agent cwd tmp, prompt appends "Attached images (local files — open them to view):\n- path" | Send screenshots from the phone to the TUI agent | Z crates/ui/src/attachments.rs | app `Composer` "+" (wire); pocketd `agent.attach` binary upload (new) | M | WS frame size limit / chunking |
| 05-14 | @file mention popup (80 ms debounce), inserting `@relative/path` that Claude/Codex resolve natively | Precise file references typed on a phone | Z crates/ui/src/composer.rs:4785,6624-6642 | pocketd `files.search` RPC (new; reuse desktop explore listing); app popup (new) | M | file index per cwd |
| 05-15 | Slash palette: list `/commands` + skills from `~/.claude/commands`, `.claude/commands`, skills dirs; send as typed text | Discover and run custom commands from the phone | Z crates/ui/src/composer.rs:4830 | pocketd `commands.list` (new); app popup (new) | M | per-provider command discovery |
| 05-16 | "Show full output (N KB)" lazy fetch beyond the 64 KiB clamp | Read full logs without bloating the timeline | Z crates/ui/src/transcript.rs:6990-7400; P packages/pocketd/internal/proto/proto.go:12 | pocketd keeps full output + `tool.output` RPC (new); app ToolGroup row (adapt) | S | output retention policy |
| 05-17 | Jump pill "↓ Scroll to bottom" at 320 px from bottom, hidden within 2 px; stick threshold 70 px | Predictable return to live output | Z crates/ui/src/transcript.rs:62,66,203; Z crates/ui/src/shell.rs:10162-10260 | app `TimelineView` thresholds (adapt) | S | none |
| 05-18 | Own-send runway: anchor the sent prompt near the top, reply streams into blank space below | The reader's eye stays on the new turn | Z crates/ui/src/transcript.rs:3944,220-233 | app `TimelineView` (new) | M | FlatList footer spacer math |
| 05-19 | Per-chunk opacity veil (clamp(EMA×3, 120, 400) ms) + markdown mend for Codex deltas | Smooth streaming without flicker or broken markup | Z crates/ui/src/markdown/veil.rs; Z crates/markdown/src/mend.rs | app markdown renderer (adapt) | M | only Codex streams deltas today |
| 05-20 | Status palette: working pink (not amber), awaiting indigo, errored red, done-unseen emerald | Running reads as routine; waiting pops | Z crates/proto/src/view.rs:524-534 | theme crate + app `status.ts` (adapt) | S | design sign-off |
| 05-21 | Turn rail minimap (≤12 ticks, bucketed, hover preview) | Jump between prompts in long sessions | Z crates/ui/src/rail.rs | phone: prompt index sheet (adapt); desktop: n/a without transcript | M | none |
| 05-22 | Connectivity banner after a 4 s hold, 5 s startup quiet ("Your device is offline" / "trying to reconnect") | No flashing error on brief blips | Z crates/ui/src/sound.rs:383; Z crates/ui/src/shell.rs:2673 | app ChatScreen error banner (adapt) | S | none |
| 05-23 | Desktop transcript pane (render the timeline next to the terminal) | Readable history on desktop | Z crates/ui/src/transcript.rs (whole) | desktop new crate (new) | XL | conflicts with terminal-first design |
| 05-24 | Appshots (hotkey window capture + AX text as context) | Show the agent another app's state | Z crates/ui/src/appshots.rs; Z docs/appshots.md | desktop (new) | L | macOS AX/screen permissions |
| 05-25 | New-thread animated backgrounds | Cosmetic | Z crates/ui/src/settings.rs:79 | n/a | M | none |

## UI/UX spec to copy

**Tool group (phone port of 05-1/05-10)**
- Layout:
  - Header row, 26 px (desktop) / 40 px (Pocket phone; keep 40 for touch).
  - Header content: summary text, then chevron (14 px, rotates −90° collapsed → 0° open).
  - Color: muted, primary on press.
- Summary grammar: `ran N command(s)` · `edited N file(s)` · `read N file(s)` · `searched N time(s)` · `fetched N page(s)` · `updated todos` · `called N tool(s)` · `N failed`.
  - Capitalize the first letter only.
  - If there are thoughts, prepend `thought process` / `thought N times`.
- States:
  - streaming: open, shimmer sweep 3400 ms.
  - settled: collapsed.
  - failed: summary only; the row label turns red.
- Rows: label (12/18) + mono detail on one line. Tree guide at x 12.5, bend radius 6.
- Motion:
  - fold 140 ms ease-out (0,0,0.58,1)
  - row reveal 360 ms expo (0.16,1,0.3,1)
  - stagger 65 ms, first row +90 ms
  - connector 480 ms quint (0.22,1,0.36,1)

**Composer button**
- Mode = !live ? Send : hasContent ? Queue : Stop.
- Stop: 28 px circle, fill = text color, 11 px rounded square (r3) glyph, a11y label "Stop".
- Return/Enter: submit only. Stop only via the button (Esc-to-stop is optional, default off).

**Queue panel**
- Position: above the composer, inset 16, overlapping it by 18. Radius 16, pad 4. Max height 30% of the viewport.
- Rows: 36 px, 12.5 px text, radius 8.
- Primary action: 72×28, r5, 11.5 px.
- Tooltip / a11y copy: "Steer (keep current work running)", "Send at the next turn (this agent cannot steer mid-turn)", "Send now (interrupt)", "Waiting for provider capabilities".
- Edit copy: "Editing in composer", "Save to queue", "Cancel".

**Question wizard**
- Container: radius 26, replaces the composer.
- Header: uppercase 10.5 px tracked + "1/3" chip (h20).
- Question: 15/20 medium. Multi-select hint "Select one or more options."
- Option row: px14 py10 r12, kbd chip 22 px r6 with its index.
  - Picked: border ink 0.16, bg ink 0.09. Hover/press bg 0.025→0.06.
- Behavior:
  - single-select auto-advance 220 ms
  - keys 1–9 pick
  - Esc/back steps back when the free text is empty
  - "Next"/"Submit" at opacity 0.4 while blocked
- Free text: "Type your own answer, or pick an option above" / "Type your own answer, or leave this blank to use the selected option".

**Working trailer**
- `{glyph} {Word}… {elapsed}`: the word rotates every 7 s over the 21-word list, seeded by chat id.
- Elapsed format: `Ns`, `Nm Ns`, `Nh Nm`, `Nd Nh`.
- Before the turn starts: "Sending…". After it ends: "Worked for {elapsed}".

**Context ring**
- 16 px ring, stroke 1.8, track muted·0.25, sweep from 12 o'clock.
- Color: <75% muted, ≥75% warning oklch(0.828 0.189 84.429), ≥90% danger oklch(0.704 0.191 22.216).
- Chip: h24, px6, r6, 11 px.
- Card: "Context window" / "184,000 / 200,000 tokens" / "16,000 tokens remaining".
- Fallback copy: "Context limit not reported", "Waiting for context usage".

**Scroll**
- Stick when ≤70 px from the bottom. The pill appears past 320 px; at bottom = ≤2 px.
- Pill: h30, full radius, hairline border, "↓" + "Scroll to bottom" 13 px, 36 px above the composer, right 10.
- Spring: damping 0.7, stiffness 0.05, mass 1.25.

**User bubble**
- Max 80% width, px16 py10, 14/22 (the Pocket phone uses 15; keep it).
- Collapse past 5 lines or 400 chars; "Show more" / "Show less".
- Pending opacity 0.65. Failure: "Not delivered — tap to retry".

**Notifications**
- Title: agent title, or "New session".
- Bodies: "Run finished" / "Waiting on your input" / "Run failed".
- Settings: "Desktop notifications", "Only when in the background" (default on), "Session sounds", "Task completed", "Input required", "Errors and disconnections".

**Motion tokens** (Z crates/ui/src/motion.rs:299-415)

| Token | Value |
|---|---|
| FADE_IN | 500 expo |
| FADE_QUICK | 150 |
| MENU_IN | 140 |
| MENU_OUT | 100 |
| DIALOG_IN | 180 |
| RESIZE | 200 |
| COLLAPSE | 180 |
| CHEVRON | 200 |
| SCROLL_GLIDE | 500 ease-in-out (0.42,0,0.58,1) |
| HOVER_FADE | 150 (0.4,0,0.2,1) |

**Dark tokens** (Z crates/ui/src/theme.rs)
- bg #060606, surface #0d0d0d, border white·0.08.
- Text neutral: 0.922 / muted 0.708 / faint 0.556.
- success oklch(0.765 0.177 163.223).
- User bubble wash: 0.08 dark / 0.04 light.
- Markdown block gap: 12.

## Open questions / risks

- **AskUserQuestion answer path.** Can a PreToolUse hook return the answers (deny with reason / updatedInput), or must pocketd drive the TUI dialog with keystrokes? The PTY route is fragile across Claude Code versions.
- **Queue semantics over the PTY.**
  - "Send next" needs a reliable turn-end signal. The Stop hook exists; for Codex, turn/completed.
  - "Steer" is just typing now; confirm Claude TUI mid-turn queue behavior and Codex TUI behavior.
- **Context window source.** Claude JSONL has the model but not the window; 1M-context variants break the hardcoded 200k (P packages/desktop/crates/agents/src/agents.rs:57). A model→window table must be maintained.
- **Zeron's no-approval posture** (auto-allow everything) is a different trust model. Pocket should keep its permission sheet; don't copy that.
- **Sound assets:** Zeron is MIT, but check the WAVs' provenance before reusing them. Otherwise commission or synthesize our own.
- **Streaming polish (veil/mend) only helps Codex.** Claude arrives as whole JSONL blocks (P packages/pocketd/internal/claude/transcript.go), so the value is limited until pocketd streams Claude deltas.
- **Zeron docs/screenshots lag the code:**
  - context card is click, not hover
  - token commas
  - "searched N times" vs "1 search"
  - unshipped sound cues
  - Re-verify against code before copying copy text.
- **Attachments over the WS need chunking.** Zeron chunks at 680k base64 chars to fit a 1 MiB relay frame; Pocket's WS limit is unverified here.
- **Turn rail and runway** are desktop-transcript features. Their value on a phone FlatList is unproven, and the runway risks scroll jumps (Zeron needed a perf doc to fix a 660→0 px jump).

## Verification

Date: 2026-09-30. Claims checked: 15. Corrected: 6.

Confirmed against code: tool summary grammar and status palette (Z view.rs), Send/Queue/Stop and #406 Enter rule, question-wizard constants and copy, queue tooltips and geometry, auto-allow of `can_use_tool`, notification detector and transition rules, context ring thresholds/click-toggle/no-window gating, scroll and spring constants, working-trailer words and elapsed format, Pocket Composer Stop-while-busy, AskUserQuestion PreToolUse hook with no question UI, hardcoded 200k CONTEXT_WINDOW, PermissionSheet copy.

- F6: Claude steering citation was the permissions header (L8-16); fixed to L28-30 and reworded to match (steers written as stdin user lines, folded at step boundary).
- F7: added citation for `--permission-prompt-tool stdio` (L186-188); `auto_approve` also passes `--dangerously-skip-permissions`; non-auto passes `--permission-mode default`.
- F2: the 160-char output clamp is `TOOL_OUTPUT_SUMMARY_MAX` in Z crates/doc/src/parts.rs:18, not in transcript.rs; citation fixed.
- F1: summary counts Glob/WebSearch as searches and Mcp/Unknown as "called N tools"; added.
- F11: Pocket desktop notification citation corrected to main.rs:315-327,331-344,1103 and status.rs:119-120; truncation string lives in timeline.go:177.
- F11: Pocket status ordering was wrong; Failed and Done share one section, Idle splits by today (status.rs:82-90).

