# Research: desktop chat, for iOS parity

Covers every chat component and behaviour the desktop has, so the SwiftUI app can match it. Paths are relative to `packages/desktop/crates/` unless they say otherwise. `pocket/chat/X` means `pocket/src/chat/X.rs`.

---

## 0. Decision-relevant summary

1. **Port the fold and layout logic exactly.** Everything the chat shows comes from two pure Rust modules with no GPUI in them:
   - `chat/src/timeline.rs` folds pocketd items into a `Transcript` of turns and blocks.
   - `chat/src/rows.rs` lays that out as `Row`s.

   Together they're about 600 lines plus about 340 lines of tests. Port both to Swift as value types and port their tests as XCTests. Every view below is a dumb renderer of one `Row`.
2. **The wire sends whole items, not deltas.** `agent.stream` re-sends the full item under the same `id`/`seq` each time it grows (pocketd `internal/timeline/timeline.go:196-225`). Clients upsert by `id` and keep the list sorted by `seq` (`agents/src/agents.rs:508-519`). When `resets` changes on `AgentSummary`, drop the timeline (`agents.rs:552`).
3. **Markdown on desktop is not a stock renderer.** It is the `markdown` crate 1.0 (markdown-rs, mdast) with `ParseOptions::gfm()`, plus `math_text` and `math_flow`, frontmatter off and HTML on (`vendor/gpui-base/src/text/markdown_ext.rs:392`). The renderer is vendored and patched (`vendor/gpui-base/src/text/node.rs`). It adds GitHub alerts, footnotes, task checkboxes, scrollable tables, and inline-code path chips (from the chat plugin).
   - The chat splits each message into **top-level blocks** first (`rows.rs:375 markdown_blocks`), and each block is its own list row.
   - Fenced code blocks never go through the markdown renderer. The chat draws them itself (`pocket/chat/markdown.rs:221`), highlighted off the main thread with tree-sitter and Xcode colours.
   - On iOS, use swift-markdown (cmark-gfm) or a markdown-rs port per block, and draw code blocks natively.
4. **Mermaid is not rendered in chat.** It only renders in the file preview (`pocket/src/explorer/mermaid.rs`, `explorer/preview/markdown.rs:54`). In chat a mermaid fence is a plain code block labelled `mermaid`. Matching the desktop means no diagram in chat; a diagram would be a bonus.
5. **Streaming reveal is client-side.** New text is let out at 320 B/s, clamped to 0.3–1.4 s per chunk, cut at word ends, and each word fades in over 240 ms (`pocket/chat/reveal.rs`).
   - Code fences show whole once reached.
   - Only the last markdown block re-parses, because the block ids stay stable.
6. **The sent prompt glides a quarter down.** When the user sends, the prompt glides over 520 ms (cubic ease-out) to `min(viewport*0.25, 8 + heightAbove)` from the top. A bottom spacer holds it there until the reply fills the space; after that the list follows the tail (`pocket/chat/pin.rs`). Section 6 has the exact maths.
7. **The composer's outgoing state machine is client-side and large.** `chat/src/outgoing.rs` (1112 lines, about 50 tests) covers:
   - Send, Steer, Queue, Stop and Stopping states.
   - A 120 s delivery grace.
   - Echo matching to settle sends.
   - A paused queue after a stop, failure or refusal, with Resume.
   - Send now, and slash commands that never echo.

   Port it verbatim. The protocol has none of this.
8. **Permissions:** the desktop shows Deny, Reply (only when `feedback`) and "Allow once". It ignores `PermissionRequest.options`. AskUserQuestion becomes a paged wizard that sends `permission.resolve{decision:"allow", answers:{questionText: answer}}`.
9. **Account switching is not reachable from the phone today.** `accounts.v1` usage and switching go over pocketd's local **ops socket** (`daemon::request_to`, `pocket/src/accounts.rs:407`), not the WebSocket. iOS can only show `AgentSummary.account` and `switching` until pocketd exposes it.
10. **`@file` mentions have no wire message either.** The desktop lists files with a local `git ls-files` (`pocket/chat/composer/suggest.rs:378`). Image attachments are Mac file paths appended as a `# Attached images` block. **iOS needs new pocketd messages** for a file list and an image upload. Slash commands (`agent.commands`), MCP (`agent.mcp`), `/btw` (`agent.btw`), model/effort/access (`agent.settings`), compact, send_now and interrupt all exist on the wire.
11. **Test fixture:** `.ui-review/fixture/fixture.ts` is a fake pocketd with a phone WebSocket and 38 chat scenarios in `.ui-review/fixture/chat/*.json`. Point the iOS app at it.

---

## 1. Data pipeline

### 1.1 Wire → items
- **Item shape:** `TimelineItem` is defined in `packages/protocol/src/timeline.ts`. Its kinds are `user`, `assistant`, `thinking`, `tool`, `tasks`, `plan`, `compact`, `result` and `notice`. Common fields are `id`, `seq`, `ts` and `parentToolUseId?`.
- **Fetching:**
  - The desktop fetches `agent.timeline{limit:500}` once per agent as it first appears (`agents.rs:1109`), then applies `agent.stream` frames.
  - The response carries `epoch`, `items`, `hasOlder` and `maxSeq`. The desktop never pages older.
- **pocketd item rules** (`pocketd/internal/timeline/timeline.go Apply :181`):
  - **Drafts:** `assistant_draft` and `thinking_draft` append text to an open draft item (same id). The final `assistant_text` replaces the text.
  - **Tool calls:** `tool_start` creates a `tool` item with `status:"running"`. `tool_end` replaces `call` (status `ok`/`error`, `durationMs`, `output` clamped to 64 KiB plus "\n… truncated", per `proto.go:13`).
  - **Tasks and plans:** `TodoWrite` becomes a `tasks` item, not a tool. `ExitPlanMode` becomes a `plan` item with markdown text.
  - **Sub-agent metadata:** `tool_model` and `tool_tokens` update a `task` detail's `model` and `tokens`.
  - **Background calls:** `tool_background` keeps the call running past its turn and may add output.
  - **Retries:** `retried` rewrites a failed `result` into a `notice{error}`, because the agent went on after the error.
  - **Results:** a second `result` after an `interrupted` one is dropped.
- **Edit/Write diff previews:** at most `DiffPreviewLines = 24` lines, each capped at `DiffLineChars = 160` runes (`proto.go:14-15`). `additions` and `deletions` count the whole diff.
- **Tool detail mapping** (`timeline/detail.go:37`):

| Tool name | Detail kind | Fields |
|---|---|---|
| Bash, Monitor | `shell` | `command`, `description` |
| Read, NotebookRead | `read` | `path` |
| Edit, NotebookEdit | `edit` | `path`, `diff` |
| Write | `write` | `path`, diff of "" → content |
| Grep, Glob | `search` | `query`=pattern, `path` |
| Task, Agent | `task` | `description`, `model` |
| anything else | `other` | `name`, raw `input` |

  For TaskStop, TaskOutput, BashOutput and KillShell, pocketd copies the target task's command into a `shell` detail, or its description into `description`.
- **Caps:** sub-agent items (`parentToolUseId`) are only sent to clients that negotiate `subagents.v1`. The desktop's hello asks for `pair.v1 scopes.v1 summary.v2 host.v1 open.v1 names.v1 automations.v1 locals.v1 fork.v1 accounts.v1 subagents.v1 settings.v1 commands.v1 access.v1` (`agents.rs:1416`).

### 1.2 Items → Transcript (`chat/src/timeline.rs`)
Types are in `chat/src/chat.rs`:
- `Transcript{turns}`
- `Turn{id, prompt:Option<Message>, blocks, end:Option<End>}`
- `Block = Text | Thinking | Tool(ToolCall) | Todos | Plan | Compacted{auto} | Notice{message, error}`
- `ToolKind = Shell | Read | Edit{diff} | Write{diff} | Search | Fetch | Question(Vec<Question>) | Agent(SubAgent{description, transcript, model, tokens}) | Other{input pretty JSON, description}`
- `End = Done{duration_ms, usage} | Failed{error, duration_ms}`

Fold rules (`timeline.rs:9-17`, `step :121`, `block :190`, `kind :230`):
- **Turn boundaries:**
  - `user` opens a turn and `result` ends it.
  - Any other item while the last turn has ended opens a new turn with `prompt:None`. That is a turn the agent began itself: a hook, a background report, an auto compact.
- **Tool updates:**
  - A `tool` item with a known `toolUseId` replaces its block in place, even in an older turn.
  - An empty `toolUseId` falls back to the item id, so calls without ids never merge.
- **Remapping:** WebFetch becomes `Fetch{url}`. WebSearch becomes `Search{query}`. `search` with a path becomes `"{query} in {path}"`. AskUserQuestion becomes `Question`, parsed from `input.questions[{question, header, options[{label, description}], multiSelect}]`.
- **Dropped items:** blank `assistant`, `thinking` and `plan` items are dropped. A `notice` with an `error` becomes `Notice{error:true}`.
- **Sub-agent nesting:** items with `parentToolUseId` fold, recursively, into that `Agent` call's `SubAgent.transcript`. Items whose parent isn't an Agent call are dropped. Cycles are safe because `claim` removes children as it folds them.
- **Ordering:** items out of seq order are sorted first.
- **Incremental refold:** `Fold::refold` (`timeline.rs:49`) refolds only from the first changed turn, unless the change reaches back into an earlier turn's call or sub-agent. That is why 5000 items refold in under 2 ms (`session.rs:537,548` perf tests).

### 1.3 Transcript → Rows (`chat/src/rows.rs`)
`Row` (`rows.rs:6-30`) with its stable id (`:34`):

| Row | id | What it shows |
|---|---|---|
| `Prompt{turn}` | `{turn}` | the user bubble |
| `Notice{turn}` | `{turn}#notice` | "Audit deps is done" |
| `Note{turn,block}` | `{turn}#{block}` | a hook or error notice line |
| `Fold{turn}` | `{turn}#fold` | the reply header line |
| `Phase{turn,blocks}` | `{turn}#p{start}` | a run of thinking and tool calls |
| `Text{turn,block,range}` | `{turn}#{block}.{start}` | one markdown block |
| `Tasks` | `{turn}#{block}` | the todo card |
| `Question` | `{turn}#{block}` | the question marker |
| `Compacted` | `{turn}#{block}` | the compaction rule |
| `End{turn}` | `{turn}#end` | the reply footer or error |
| `Live{turn}` | `{turn}#live` | the "Opus 5.5 working for 7s" line |
| `Working` | `working` | "Sending…" |
| `Pending(i)` | `pending-{i}` | an outgoing send not yet echoed |

Layout order per turn (`rows_from :79`):
1. The prompt.
2. `Notice` if the turn reports background work.
3. The leading `Note`s.
4. `Fold`, if this turn heads a reply and isn't a failed-empty turn.
5. Then up to the last work block: work blocks group into one `Phase`, and any non-work block flushes the phase and emits its own rows (Text, Plan, Question, Compacted, Note). Todos are skipped here.
6. The **latest** `Todos` of the turn, as one `Tasks` row.
7. Blocks after the last work block (the answer).
8. `End`, if the turn ended.

After all turns come the trailer rows: `Pending(i)` for each echoed send, `Working` while one is `Sending`, and `Live{fold}` while the agent works (`live_tail :197`).

**Grouping rules:**
- **What counts as work:** `is_work` (`:144`) is Thinking, or any Tool except Question. Sub-agents are steps of the phase they run in. A Question ends the phase and stands alone (test `a_question_ends_the_phase_and_stands_alone`).
- **Narration vs answer:** text between steps is narration (`narrates :157`). Text after the last step is the answer.
- **Steering:** a steer is a prompt sent while the previous turn was still open. It carries the same reply: no new Fold and one footer for both turns (`steered :174`, `reply :278`).
- **Compactions:** a turn that only compacts gets no fold (`folds_at :167`).
- **Background reports:** `reported` (`:234`) picks calls that finished after their own turn ended, within the window [previous turn start, this turn start]. Those become the `Notice` heading the turn the agent began itself.
- **Background tasks:** `in_background` (`:210`) is the Agent or Shell calls still `Running` in ended turns. They feed the tray and keep `live_tail` on. A background shell alone does **not** keep the agent at work (`a_command_left_in_the_background_counts_but_leaves_the_agent_idle`).

**Phase title** (`phase_title :328`): parts are joined with " · ".
- Sub-agents: "Using/Used a subagent" or "N subagents".
- Edits: "Editing/Edited a.rs" (basename) or "N files".
- Reads, searches and fetches: "Exploring/Explored the project".
- Shell: "Executing a command"/"Ran a command" or "N commands".
- Only thinking: "Thinking"/"Thought".
- Otherwise: "Working".

**Gaps above each row** (`top_gap :314`):
- 16 above the first row.
- 14 before a Prompt or Pending, and between Pending and Live.
- 0 before a Phase or Working.
- 10 between blocks of one message.
- 12 before Tasks.
- 8 before Question, Compacted, Notice and Note.
- 6 otherwise.
- Rows hung on a fold's rail have 0.

### 1.4 Session and splicing (`chat/src/session.rs`)
- `Session.update(revision, items, working, now)` refolds only when the revision moved. It then settles outgoing sends and returns a `Change`: (rows to remeasure, row range to replace, count).
- Row ids hold while content changes, so the list re-measures in place instead of re-creating rows (`a_streamed_reply_grows_in_one_row_until_its_end_follows_it`).
- **iOS:** use the row id as the SwiftUI `id` in a lazy list or `UICollectionView` diffable data source. Reconfigure, don't reload, for content changes.
- `Folds` is a `HashMap<rowId, bool>` of user open/close choices that outlive defaults (`toggle :78`).

---

## 2. Components (renderers of one Row)

The reading column is 720 pt, clamped to 560–1200 in 16 pt steps (`transcript.rs:23,30`). Rows have 32 pt side padding. The list pads 8 above and 24 below (`pin.rs:5-6`).

### 2.1 User prompt bubble (`pocket/chat/user_bubble.rs`)
- **Look:** right-aligned, at most 78% of the column (`:16`). Background `BUBBLE`, corner radius 18, padding 16×9, 14.5/22 text (`:206-219`).
- **Marks:**
  - `/command` and `/skill` tokens are tinted `COMMAND` (`chat/src/marks.rs:15`). A command starts the text or follows whitespace, starts with a letter, and may end in punctuation. It isn't one when it runs into a path (`/usr/bin`, `/notes.md`).
  - `@path` mentions render as a file icon plus the basename (`mention_name`), tinted `MENTION`, with the full path in a tooltip (`:31-82`). `@mcp/...` tags are not mentions. Email addresses don't count.
- **Collapse:** past 5 wrapped lines the bubble clips with "..." and "Show more"/"Show less" plus a chevron (`:12,99,160-205`). The height tween takes `min(220 + 0.32*Δpx, 850)` ms, quint ease-out, or ease-in-out when Δ > 500 (`tween :104`).
- **Images:** `split_images` (`pocket/src/composer.rs:142`) strips a trailing `\n\n# Attached images\n\n- /path` block. The paths show as 56 pt thumbnail tiles, radius 8 (`:163`).
- **Under the bubble** (`actions :115`):
  - A copy button (28 pt; the icon swaps to a check for 1.2 s).
  - The send time as `H:MM` in 24-hour local time (`meta.rs:6`).
- **Selectable text:** drag to select and double- or triple-click; on iOS, use text selection (`selectable.rs`).
- **Pending sends:**
  - Show at 0.65 opacity.
  - A failed send adds "Not delivered — click to retry" in red (`working.rs:20`). Tapping it puts the text back in the composer (`transcript.rs:594`).
  - A steered send not yet taken shows a dot and "Claude reads this after its current step", or "reads these N" on the last one. "· Send now" appears when caps allow `send_now`; it reads "Sending…" while in flight (`user_bubble.rs:127-156`, `transcript.rs:788-812`).
- **Animations:**
  - The newest prompt rises 12 pt and fades in over 240 ms when sent (`transcript.rs:27,818`, `creating.rs:202`).
  - The first prompt of a new chat uses the "arriving" morph (`creating::BUBBLE` = 40 ms delay, 10 pt).

### 2.2 Fold (reply header) (`pocket/chat/fold.rs`)
- **Settled:** a provider glyph (or a down-chevron when open) plus "Claude worked for 1m 4s", in TEXT_3 at 14/20. A right-chevron shows when shut. Tap toggles. Default is **shut**: work hides, narration and the answer stay (`transcript.rs:316-323`).
- **Fade-in:** when a reply just settled, its label fades in over 240 ms (`transcript.rs:661`).
- **Duration:** `took` (`transcript.rs:272`) uses the turns' `durationMs` sum, else the time from start to the last block.
- **Who:** the label names the brand's short name ("Claude", "Codex", else "Agent").
- **Open while live:** a fold stays open while its reply is at work (`fold_live`). A phase holding a **failed sub-agent** always shows, even under a shut fold.
- **Live line:** the live tail's fold renders empty. The `Live` row at the bottom shows `glyph + shimmer(label)` instead.
- **Live labels** (`live_label :24`, `transcript.rs:253`):
  - "Stopping…"
  - "Thinking…", while all blocks are thinking and nothing runs in the background.
  - "Needs your approval", for a pending permission.
  - "Needs your answer", for a pending question.
  - "{model or who} working for {elapsed}[ · in the background | · N in the background]"
- **Rail:** rows inside an open fold hang on a 1 pt `RAIL` spine at x=7, with an 8 pt rounded elbow. The last row's spine stops at its elbow (`phase.rs:15-34`).

### 2.3 Phase (work group) (`pocket/chat/phase.rs`)
- **Lone step:** a phase with one non-agent step shows just that step, with a kind icon (`:120`).
- **Several steps:** otherwise a title row: kind icon (or a down-chevron when open) plus `phase_title`.
  - Live: the title shimmers and isn't clickable. The phase is forced open and shows only the **last 9 steps** (`LIVE_STEPS :11`).
  - Settled: default shut, tap toggles. Kept open if any sub-agent runs.
- **Kind icon** (`kind_icon :42`), from the first work step: bulb (thinking), pencil (edit/write), terminal (shell), bot (agent), search (read/search/fetch), tool (anything else).
- **Thinking step** (`:67`):
  - Header: "Thinking" (shimmer while live), "Thought for 3s" (time until the next block, at least 1 s, per `thought_for :63`), or "Thought".
  - Shut by default. The body uses faint 12/18 lines.
  - Wraps at 96 columns. A live thought keeps the **last** 24 lines; a settled one keeps the first 24 plus "… N more lines" (`thinking.rs`).

### 2.4 Tool row (`pocket/chat/tool_row.rs`)
- **Verb and argument** (`line :62`):

| Tool | Verb | Argument |
|---|---|---|
| Read | "Read" | file chip |
| Edit | "Edit" | file chip |
| Write | "Create" | file chip |
| Search | "Find" | the query, collapsed to one line |
| Fetch | "Fetch" | the URL |
| Shell | none | first non-empty command line in mono, else "Shell" |
| Shell named Monitor | "Watch" | as Shell |
| TaskStop, KillShell | "Stop" | the target's command or description |
| TaskOutput, BashOutput | "Read output of" | the target's command or description |
| Skill | "Skill" | `/skillname` |
| MCP tool | none | `mcp__server__tool` as "server · tool" |
| Agent | none | its description |
| Other | none | its raw name |

- **File chip** (`:112`): rounded 5, `FILL_4`, mono 12.5. It has a 13 pt colour badge with two letters: RS #C2410C, TS #3178C6, MD #3E8EF7, TM (toml) #9C4221, JS (json) #8B8B94, SW #F05138. The basename truncates.
- **States:**
  - Running: the verb or argument shimmers.
  - Ok: dim, no body.
  - Failed: red text, an x icon, and a chevron that opens the error output. The error shows as mono 12/20, max height 240, first 24 lines (lines cut at 8192 chars) plus "… N more lines" (`tool_detail.rs`).
  - Running past the work: settled but still running reads as "background" in TEXT_4.
- **Output:** successful tool output is **never shown** in chat, only errors.
- **Edit/Write with a diff:** tap the chip to open the inline diff (`diff.rs`):
  - Lines are renumbered into git-style hunks with `@@ -old +new @@` headers; a gap in numbering starts a new hunk (`lines :12`).
  - Syntax and word highlighting runs off-thread on first open (`Tools.toggle :18`).
  - Rows are 12 pt mono and clip long lines.

### 2.5 Sub-agent row (`pocket/chat/sub_agent.rs`)
- **Status mark** (`mark :31`):
  - Starting: running with no blocks yet.
  - Running: a ring plus an arc spinning at 1 s per turn, capped at 20 fps.
  - Done.
  - Failed: red.
- **Head:** the mark, the name, and "doing" (the newest step in mono 12, while running and shut).
  - The name is the description's first line, cut past 56 chars to 55 + "…", or "Subagent" when empty (`name :41`).
  - Right side, in order: the model name, then the meta, then elapsed (while running), then a chevron (if openable). Meta is "starting", "failed", "done" or "N steps[, M failed]".
- **Open:** default is open if failed, else shut. Open shows:
  - The **brief**: the sub-agent's first prompt, 13/20, clipped to 3 lines with "Show more".
  - Its tool rows and text notes on a rail.
  - The **report**: the call output, else its last text. Markdown at 13.5/21, clipped to 12 lines with "Show more". A failed sub-agent's report shows as red mono.
- **Line estimates:** line counts are estimated at 90 chars per line (`lines :97`).
- **Nesting:** nested sub-agents are tool rows inside the rail. The full tree is in the agent map (2.15).

### 2.6 Text block (`transcript.rs:714-756`, `markdown.rs`)
- Rendering is covered in section 3. Body text is 14/24 TEXT_BODY.
- **Plan blocks:** the first block of a `Plan` gets a "Plan" header (list-checks icon), unless it starts with `#` (`:754`, `todos.rs:20`).
- **Narration:** text between work hangs on the rail while the fold is open.

### 2.7 Tasks card (`pocket/chat/todos.rs`)
- **Shape:** rounded 12, `FILL_1`, 1 pt border.
- **Header:** list icon, "Tasks" in mono 12 medium, and a pill reading "2 of 5" or "Complete".
- **Items:**
  - Done: a green check in a `SUCCESS_BG` circle; text TEXT_4 with strike-through.
  - Active: a spinner; text TEXT.
  - Pending: a hollow circle; text TEXT_2.
- **Which list:** only the **latest** `Todos` block of a turn shows, after its work. It's hidden when the fold is shut.

### 2.8 Question marker (`pocket/chat/question_marker.rs`)
- A 34 pt pill: comment tile, "Question", then the value.
- The value is "Awaiting your answer…" while running, else the first question's header (whitespace collapsed), else its question text.
- The questions themselves are asked in the composer (2.11).

### 2.9 Notes, notices, compaction, end (`pocket/chat/end_row.rs`, `transcript.rs:334-357`)
- **Note:** a redo icon (or a warning for errors) plus the first line, in TEXT_3 at 13 (`note :49`). Example: "Stop hook: tests fail".
- **Notice (background report):**
  - Labels: "{name} is done"; "Background command `cmd` is done/failed"; "Monitor `cmd` ended"; "N subagents are done"; "N background tasks are done".
  - Backticks mark a mono span. Code inside it may hold its own backticks, because the outermost pair bounds it (`code_span :352`).
  - Tap scrolls to the call's phase, opens it and flashes it (`FILL_4` fading over 900 ms, quint ease-out; `chat_show_block :484`).
- **Compacted:** rules on both sides of "Context compacted" or "Context compacted automatically", padded 14 above and below.
- **End rows:**
  - Interrupted: an error containing "interrupt" reads "Interrupted · What should Claude do instead?" (`interrupted :5`).
  - Failed: an error card with a warning tile, "Error", a copy button and the message (`error :14`).
  - Done: the reply footer (`meta.rs:36`):
    - "Copy answer" copies only the answer text: blocks after the last work in each turn of the reply, joined with blank lines (`answer :294`).
    - "Retry this turn" re-sends the reply's prompt.
    - Then the stats: elapsed and `H:MM`, separated by dots.
    - The footer shows only once no work continues in the reply, and once per steered reply.

### 2.10 Trailer rows
- "Sending…" shimmers. The shimmer is a 1.6 s sweep with a band of 0.25 width, capped at 20 fps (`shimmer.rs`).
- The live line: see 2.2.
- **Empty chat:** "Send a message to start." (`transcript.rs:432`).

### 2.11 Permission card (replaces the composer box) (`pocket/chat/permission.rs`)
- **Header:** an amber dot with a ring, "Allow {toolName}?", and "{Agent} is paused until you answer".
- **Body:** icon, verb and subject in mono, max height 120 (`subject :14`):

| Tool kind | Verb |
|---|---|
| Shell | "Run" |
| Read | "Read" |
| Edit | "Edit" |
| Write | "Write" |
| Search | "Search" |
| Fetch | "Fetch" |
| Agent | "Start agent" |
| Other | "Use" + toolName |

- **Buttons:**
  - **Deny** (esc) → `permission.resolve{decision:"deny"}`.
  - **Reply** (R; only if `feedback`) opens a textarea with the placeholder "Tell the agent what to do instead". "Send reply" sends `permission.resolve{deny, message}`, which pocketd types as the next prompt.
  - **Allow once** (⏎) → `allow`.
- **Read-only:** when the client is observe-only, the card shows without buttons (`composer.rs:51`).
- **Options:** `PermissionRequest.options` and `permission.resolve.option` exist on the wire, but the desktop doesn't use them.

### 2.12 Question wizard (replaces the composer box) (`pocket/chat/question.rs`)
- **Pages:** one page per question.
  - Head: the header as tracked uppercase 10.5 semibold. With more than one question, progress bars (14×3) plus "k of n".
  - Title: the question's first line, 15/21 semibold.
  - Body: the remaining lines as markdown, scrolling past 184 pt (`title_and_body :48`).
  - "Select one or more." when `multiSelect`.
- **Option cards:** radius 11.
  - Mark: a radio (radius 8) or a check box (radius 5).
  - Label (13.5 medium) and description (12, TEXT_3).
  - A number badge 1–9. Number keys pick when the free-text field is empty or unfocused.
- **Free text:** a pencil and a textarea, placeholder "Something else…", or "Something else, or leave blank to use your pick" once something is picked. Typed text wins over picks and dims the options to 0.55.
- **Picking:**
  - Single-select **auto-advances after 220 ms**, except on the last page (`AUTO_ADVANCE :16`, `select :86`).
  - Multi-select picks are joined with ", ".
- **Footer:** "Skip" (Deny), "Back" (esc; on the first page it skips) and "Next"/"Submit".
- **Answers:** `HashMap<questionText, answer>` → `permission.resolve{allow, answers}` (`answers :136`, `agents.rs:924`).
- **Page change:** a 220 ms slide of 14 pt from the side it came from, plus a fade.

### 2.13 Composer (`pocket/chat/composer.rs` and `composer/*`)
Column max width 744 (`:34`); box radius 16. Stacked top to bottom:
1. Agent map, when open.
2. Goal strip.
3. `/btw` card.
4. Background tray.
5. Queue tray.
6. Box: the attachment strip, the textarea (14.5/22, auto-grows 1 to 11 rows, then scrolls; `MAX_ROWS :39`), and a bar holding [+ attach] [access pill] [agents chip] … [model pill] [ghost Stop] [Send button].
7. Footer: the branch (worktree icon plus mono 11.5), the account chip, and the hint on the right.

- **Placeholders:**
  - "Ask, build, / for commands, @ for files"
  - Busy: "Send a follow-up into this turn…"
  - Stopping: "Queue a follow-up…" (`:31-33`, `placeholder :74`).
- **Hints** (`hint :62`):
  - "Answer the request above to continue"
  - "Stopping…"
  - "Working · esc to stop"
  - "⏎ send into this turn · ⌘⏎ queue for after"
- **Send button** (`send_button.rs:19`; 34 pt circle) follows `chat::action(working, stopping, empty)` (`outgoing.rs:369`):

| State | Action | Look |
|---|---|---|
| idle | Send | arrow; grey and inactive when empty |
| working, empty draft | Stop | white square with a spinning ring, 1 s per turn, 30 fps |
| working, typed | Steer | arrow, plus a ghost Stop beside it |
| stopping, typed | Queue | "Queue" pill |
| stopping, empty | Stopping | square, inactive |

  Opacity is 0.35 when the capability blocks the action (`Capabilities.allows`, `adapter.rs:29`).
- **Keys:** Enter sends or steers; Enter never stops. ⌘Enter queues. Esc on an empty input stops (`chat_escape :385`).
- **Send pipeline** (`chat_send :298`):
  1. Refuse unless the mode is Prompt.
  2. Refuse if busy and `!send_while_working`.
  3. Build the text with `Draft.prompt(typed)`, which appends the image paths.
  4. Parse the slash command:
     - `Kind::Unsupported` → a footer notice: "/x runs only in Claude's terminal. Run it there.", or "/x isn't available in the chat." for chat-only agents.
     - `Kind::Local` → run it locally (below).
  5. Otherwise add the MCP context line.
  6. `/goal clear` skips the queue.
  7. If not steering → `Outgoing.push` (Sending, or Queued if busy) and pin the prompt.
  8. If steering → `agent.prompt` now, and mark `Delivery::Steered`.
- **Local commands** (`chat_local :347`; per adapter, `chat/src/adapter/claude.rs:27`, `codex.rs:27`):
  - `/model` and `/effort` without an argument open the model menu on that side. With an argument they send `agent.settings`.
  - `/compact` (no argument on Claude) → `agent.compact`. `/compact <instructions>` is sent as a prompt.
  - `/new` → a new chat tab.
  - `/btw q` → `agent.btw`.
  - `/mcp [q]` → types `@mcp/q` to open the server picker.
- **Slash menu** (`chat/src/commands.rs:56 menu`): the adapter builtins first, then the CLI's `agent.commands` list, deduped, without unsupported ones. Filtered by substring; names starting with the query sort first.
  - Claude builtins: model, effort, compact, btw, goal, mcp.
  - Codex builtins: goal, compact, model, effort, new, mcp.
  - Claude panel commands are hidden. Codex TUI-only commands are hidden.
  - Commands are re-requested each time a `/` starts a word (`suggest.rs:155`). The list shows 8 rows, scrolls and wraps the selection (`SHOWN :15`).
  - `slash()` treats `/path/...` and `/x.y` as prompts, not commands (`commands.rs:43`).
- **@ file mentions** (`suggest.rs:164,199,378`):
  - Files come from `git ls-files` of the agent's cwd, run locally.
  - Ranking: files the agent touched (latest first, `locate.rs:13`), then changed files, then git order.
  - Picking replaces the word with the path. The input paints icons over the mentions (`mention.rs`).
  - **Not on the wire yet.**
- **@mcp/ server tags** (`chat/src/mcp.rs`, `suggest.rs:227`):
  - The picker comes from `agent.mcp` and lists usable servers first, then NeedsAuth, Failed and Disabled.
  - Row status reads "Available", "Connecting", "Needs authentication" ("Finish signing in in your browser" once started), "Failed to connect" or "Disabled".
  - Actions: "Sign in" → `agent.mcp{action:"auth"}` plus `authUrl`; "Reconnect"; "Enable" (Claude only).
  - It rechecks while a server is settling.
  - On send, tagged servers prepend `MCP context: Use the configured server(s) "a" (claude), … when relevant to this request.\n\n` (`with_context :62`).
- **Images** (`attachments.rs`, `pocket/src/composer.rs:54-245`):
  - The "+" button opens a file picker. Paste and drag also work. Accepted types: png, jpg, jpeg, gif, webp.
  - Pasted images are written to `~/.anywhere/attachments/{id}.{ext}` with mode 0700.
  - The strip shows 56 pt tiles with a hover × to remove.
  - **Sent as text:** `typed\n\n# Attached images\n\n- /abs/path`. iOS needs an upload message.
- **Model pill** (`model_pill.rs`):
  - Read-only shape: provider icon, model label and effort label (for example "Opus 4.5", "Extra high").
  - When `settings.v1` is offered: a two-row menu (Model, Effort) with a side list. Effort reads "Set by Claude" when the model has no levels.
  - Picks send `agent.settings{model|effort}`.
  - `runs()` matches a Claude alias against its full id.
- **Access pill** (`access_pill.rs`): a shield plus the access label. It's only a picker for chat-only agents with `access.v1`; picks send `agent.settings{access}`. The access values are ask, edits, auto and full.
- **Queue tray** (`composer/queue.rs:38`):
  - Header: "N queued" plus "send in order after this turn", or "paused" with a "▶ Resume" button.
  - Numbered rows, each with **Steer** (send it into the turn now), **Edit** (pull it back into the composer, below any typed text) and **Remove**.
  - The queue pauses after a stop, a failed turn, a stop that beat a steer, or a refused send (`outgoing.rs:263 paused`).
- **Background tray** (`composer/background.rs:49`):
  - Header: "N in background". When folded it shows the newest step of any task in mono. It may offer "Stop"; tap folds.
  - Rows: glyph, name, doing ("Starting…" or the newest step), elapsed, and "Show ↑". Tapping a row scrolls to its phase.
  - Stop shows only for chat-only, interruptible, not-busy agents with a sub-agent in the background. It sends `agent.interrupt` (`stops :42`).
- **Goal strip** (`composer/goal.rs`):
  - Sparkle icon, objective, status and spent tokens.
  - Statuses: Active, Paused, Stuck (blocked), "Usage limit hit", "Over budget", Met (complete), "Gave up" (failed).
  - Spent reads "16,510 / 50,000 tokens", or "16,510 tokens" without a budget.
  - Buttons: Pause (Codex only, while active) sends `/goal pause`; Clear sends `/goal clear`. Both go as an immediate `agent.prompt`, not queued.
  - The data is `AgentSummary.goal`.
- **/btw card** (`composer/btw.rs`): "/btw" plus the question and ×. The body reads "Thinking…", then the answer, or the error in TEXT_3; max height 220, scrolls. Answers arrive in `agent.btw{answer}`.
- **Account chip** (`composer/account_chip*.rs`): chat-only agents, when the host has several accounts for the provider.
  - Chip: provider icon, account name, and a peak usage bar plus %.
  - Popover page 1, Usage: "Claude usage", "Checked …", reload, "In use: name · plan · email", then 5-hour and weekly blocks (bar, "% used", "% remaining", "Resets in …"), then "Switch account · N more".
  - Page 2, Switch: account rows (check, name, plan tag, "Signed out", or a back-at tag; "5h x% · wk y%" and a bar). Then "Only Claude accounts on {machine}" and "Manage…".
  - Pages slide 48 pt over 240 ms.
  - The switch op is `accounts.switch` over the ops socket. **Not on the WebSocket.**
- **Drafts:** one input is shared, with a parked draft (text plus images) per agent (`drafts.rs`). When a chat closes, its unsent text goes into an error toast with Copy: "Fix CI closed before 2 messages went out" (`chat.rs:44`).

### 2.14 Context meter
- `AgentSummary.tokensUsed` and `contextWindow`. `context()` returns None until the window is above 0 (`agents.rs:149`).
- Drawn as a 12 pt ring on the pane's **tab**, not in chat (`pocket/src/panels.rs:181`, `terminal_view/context.rs`).
- Colour levels: warn at 75%, critical at 90% (`store/src/prefs/sidebar.rs:77`); TEXT_3, then WAITING, then FAILED.
- Popover: "Context window", "x / y tokens", "z tokens remaining".
- Per-turn usage (`result.usage`) is folded but not shown in the UI.

### 2.15 Agent map (`pocket/chat/agent_map*.rs`)
- **Chip** in the composer bar while sub-agents run or the map is open: a green dot plus "2 of 5 agents" (`chip_label :164`).
- **Map:**
  - Title "Agents" with a summary like "4 agents · 2 working · 1 failed".
  - A graph with one column per generation, rounded edges, and dashes flowing from parent to child (`graph.rs`).
  - The main node reads "Handed off to N agents", "Turn ended · agents still working", "Done", or its newest step.
  - Sub-agent node meta: "27s · 32.9k tokens", or "Failed after 27s".
- **Detail page:** "Working · Haiku 5.5", the prompt (Show more), the steps and the report.
- **Keys:** arrow keys, Enter and Esc.
- **Contents:** `chat::spawned` (`chat/src/spawned.rs`) holds the latest turn's sub-agents plus older ones still running, each followed by its own children.

### 2.16 Opening links and files
- `target()` (`markdown.rs:101`): a `://` URL that isn't `file://` opens the web browser. Anything else is a file under `cwd`, with `:line:col` and `#L` suffixes stripped.
- **Bare names:** `chat_open_file` (`locate.rs:48`) resolves a bare name to the latest file the agent touched with that name, else repo matches. Several matches open a picker menu.

---

## 3. Markdown (exhaustive)

### 3.1 Parser and options
- **Crate:** `markdown` 1.0.0 (markdown-rs → mdast), not comrak or pulldown (`vendor/gpui-base/Cargo.toml:98`).
- **Options** (`markdown_ext.rs:392`): `ParseOptions::gfm()`, which turns on GFM tables, strikethrough (`~`), task lists, footnotes, literal autolinks (www/http/email) and GFM tag filter. On top of that:
  - `math_text = true` and `math_flow = true`. Both are needed so a `$$` block isn't swallowed inline.
  - `frontmatter` off by default (chat).
  - MDX off.
  - HTML flow and text on, parsed by the HTML sub-parser (`format/html.rs`). Unsupported tags are logged and dropped; failed HTML falls back to its source text.
- **No smart punctuation.**
- **Unrendered math:** math isn't rendered. Inline math nobody claims keeps its literal `$…$` source (`markdown.rs:290`). Block math becomes a code block with no language (`:628`).

### 3.2 Pre-split into blocks (`chat/src/rows.rs:375 markdown_blocks`)
- **Splitting:** blank lines end a block, except inside a list that carries on with an item or an indented line.
  - A heading always ends its block.
  - A fence (``` or ~~~, at least 3, indent ≤3) opens a block that runs until a closing fence of the same character with at least the same length and nothing after it.
  - **An unclosed fence runs to the end of the message.** That covers streaming.
  - A backtick fence whose info string contains a backtick is not a fence (inline ```` ```x``` ````).
- **Fence parsing** (`pocket/chat/markdown.rs:20 fence`):
  - The language is the first word of the info string.
  - The code is the body minus the closing fence. An unclosed fence's code is everything after the opening line.
  - An indented fence (under a list item) has that indent removed from every line and renders padded by `LIST_INDENT` = 26.6 (`transcript.rs:750`).
  - A fence indented 4+ stays inside its list's markdown block and is drawn by the TextView's own code block.

### 3.3 Element styling (`pocket/chat/markdown.rs:160 style()`, `node.rs`)
| Element | Rendering |
|---|---|
| Paragraph | 14 pt, 24 line height, TEXT_BODY. 16 pt gap between blocks inside one TextView. Soft breaks reflow to spaces; hard breaks (two spaces or `\`) keep the newline (tests `test_soft_break_*`). |
| Headings | Semibold, colour `strong`=TEXT. Sizes H1 19/27, H2 16/24, H3 15/22, H4–H6 14/22. |
| Strong / emphasis / strike | TEXT for strong; italic; `Delete` is a strikethrough mark. Nested marks merge (`test_nested_emphasis_merges_text_marks`). |
| Inline code | 0.9 em mono, padding 0.25em 0.5em, `FILL_4` background, TEXT colour. |
| Inline code that names a path | **A chip:** file icon (13 pt) plus the path in mono. Tap opens the file; folders aren't clickable. Detection (`path_in :85`): no whitespace, no `://`, holds a letter, and holds `/` or a name with a known file icon. `make app`, `v*`, `type:`, `Desktop`, `self.items`, `0.1/5` and URLs are not paths. |
| Links | Colour TEXT with a TEXT_2 underline. Tap → `target()`. Reference links and definitions are resolved. Autolinks come from GFM. |
| Images | `![alt](url)` loads an `img(url)`; data URLs are decoded inline. Inline HTML `<img>` with or without a size stays in the paragraph. |
| Unordered list | Accent-coloured 5 pt dot; indent 26.6; 8 pt gap between items; nests. |
| Ordered list | `"{start+n}."` in the accent colour, right-aligned in the indent. **Counts from its first number** (`an_ordered_list_counts_from_its_first_number`). |
| Task list | Box 0.9 em, radius 0.225 em, 1.5 pt border. When checked: link-colour fill, a check SVG, and the item text muted and struck through. |
| Blockquote | 2 pt left border in accent at 60%, accent background at 5%, right radius 6, padding 12/10/6, TEXT colour. |
| GitHub alerts | `> [!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]`, `[!CAUTION]`, any case. 4 pt left border plus an icon in the kind colour: #0969da, #1a7f37, #8250df, #9a6700, #d1242f. The title is the rest of the tag line, else the kind name. Padding 12×16, radius 6, aside background. An unknown tag stays a quote. |
| Code block (fenced) | Chat-drawn (`code_block :221`): radius 10, `ink(0.035)` background, border. 28 pt header with the language (11 pt TEXT_2), a **wrap toggle** ("Wrap long lines"/"Scroll long lines") and **Copy** ("Copied" plus a check for 1.2 s). Body padded 12×10, mono 12.5/18; it scrolls horizontally unless wrapped. Label: the language lowercased, else "code" for fenced or "text" for indented. |
| Table | Framed: radius 10, `SEPARATOR_STRONG` border, inner rules, cell padding 12, head in TEXT. Column widths come from measured content; columns grow to fill and shrink with wrapping down to a per-column floor (160–480 px, about 2 lines). Past that the table **scrolls horizontally**, with the scrollbar kept visible. Column **alignment** (`:--`, `:-:`, `--:`) is honoured. |
| Horizontal rule | A 1 pt line in the border colour. |
| Footnotes | Numbered in reference order; unreferenced definitions are dropped. A section at the end: a 1 pt rule, then an ordered list at 0.9× size with "↩" backrefs (one per reference). |
| HTML blocks | Parsed into blocks; `<br>` becomes a break. |
| Math | Literal text (inline) or a code block (flow). |
| Mermaid | **Not rendered in chat**: a plain code block. Diagrams only render in the file preview (`explorer/mermaid.rs`, via the `merman` crate to SVG to resvg). |

### 3.4 Syntax highlighting
- **Engine:** tree-sitter, through gpui-kit's `SyntaxHighlighter`, run with `cx.background_spawn` (`markdown.rs:74`, `transcript.rs:599`).
- **Grammars** (Cargo features, `packages/desktop/Cargo.toml:32-47`): bash, css, go, html, java, javascript, kotlin, markdown, python, rust, swift, toml, tsx, typescript, yaml.
- **Aliases:** sh, shell, zsh and console map to bash. An unknown language renders unhighlighted.
- **Theme:** Xcode's default colours (`theme/src/theme.rs:363`). Light/dark pairs:

| Token | Light | Dark |
|---|---|---|
| keyword | 9b2393 | fc5fa3 |
| string | c41a16 | fc6a5d |
| comment | 5d6c79 | 6c7986 |
| number | 1c00cf | d0bf69 |
| type | 3900a0 | d0a8ff |
| title/declaration | 0b4f79 | 5dd8ff |
| function | 0f68a0 | 41a1c0 |
| property/constant | 326d74 | 67b7a4 |
| preproc | 643820 | fd8f3f |
| attribute | 815f03 | bf8555 |
| url | 0e0eff | 5482ff |

  Everything else uses MD_TEXT. Emphasis is italic; strong is weight 600.
- **Caching** (`Highlights :41`): by row id and (code length, dark). A growing block keeps its previous colours, cut to fit, until the new run lands. Ranges never split a UTF-8 character. A late light run never fills the dark slot. An appearance flip re-highlights.

### 3.5 Streaming behaviour
- pocketd sends the full text each time. The desktop rebuilds rows. Only the last top-level block's row changes id content, so only that `TextView` reparses.
- **Reparse:** TextView `set_text` reparses the whole string (`state.rs:309`). It's synchronous when ≤4 KiB, otherwise on a background parser (`MAX_SYNC_FULL_REPLACE_BYTES :40`). Updates are coalesced, up to 64 per parse.
- **Fade:** the TextView's stream-fade compares *rendered* text, so `**bo` completing to **bold** fades only the changed glyphs (`stream_fade.rs`).
- **Client reveal** (`reveal.rs`):
  - Pace 320 B/s, clamped so a chunk shows within 300–1400 ms.
  - The cut lands on the next whitespace, at a char boundary.
  - Text that arrives mid-reveal continues from what shows, with no jump.
  - Shrunk or rewritten text shows at once.
  - Text already present when the chat opens (`primed`) shows at once.
  - Code fences appear whole and fade in over 240 ms.
- **Live text:** live turns render with the fade motion on (`transcript.rs:752`).

### 3.6 Selection and copy
- TextView is `.selectable(true)`. ⌘C copies the selection as markdown reconstructed from nodes. Tables re-serialize to GFM, and list and code markers are kept (`node.rs:889-1010`).
- The user bubble uses a selectable `StyledText`.
- Per-message copy: the prompt's "Copy message", the reply's "Copy answer" (the answer only), the code block's "Copy code", and the error's copy.
- **iOS:** use `textSelection(.enabled)` per block, plus explicit copy buttons. A long-press context menu on a bubble or answer should offer Copy and Retry.

### 3.7 Edge cases handled (fixture list)
- An unclosed fence while streaming.
- A `##` inside a fence is not a heading.
- `~~~~` containing ```` ``` ````.
- An inline triple-backtick is not a fence.
- A fence indented under a list item.
- Blank lines between list items keep one list.
- A list continued by an indented line.
- Very long code lines: horizontal scroll or the wrap toggle.
- Wide tables: framed horizontal scroll (`wide-table.json`).
- Long tool output: 24 lines and 8 KiB per line.
- Diff lines: 160 chars.
- Thinking wraps at 96 columns.
- Unicode cut safety in reveal and highlights.

### 3.8 Test names (ready-made fixtures)
Each test name below is a sentence describing a behaviour to reproduce.

- **`chat/src/rows.rs`:**
  - a_reply_lays_out_as_fold_phases_then_its_answer
  - text_between_steps_stands_whole_between_their_phases
  - sub_agents_are_steps_of_the_phase_they_run_in_and_the_latest_todos_follow_the_work
  - narration_before_a_sub_agent_stands_above_its_phase
  - a_reply_stopped_before_anything_came_has_no_fold
  - a_question_ends_the_phase_and_stands_alone
  - a_turn_the_agent_begins_itself_heads_its_own_reply
  - a_reply_is_at_work_while_its_turn_is_open_or_a_sub_agent_it_started_runs
  - a_phase_is_titled_by_its_work
  - a_long_message_becomes_one_row_per_markdown_block
  - a_fenced_code_block_is_never_split
  - inline_backticks_open_no_fence
  - row_ids_stay_the_same_when_a_tool_finishes
  - exchanges_get_fourteen_and_their_parts_six
  - splice_replaces_only_rows_after_the_shared_prefix
  - sending_shows_only_while_a_send_is_on_its_way
  - the_live_line_trails_only_the_last_reply_while_the_agent_works_on_it
  - a_live_tail_settles_once_its_fold_stops_carrying_the_live_line
  - a_command_left_in_the_background_counts_but_leaves_the_agent_idle
  - a_background_task_is_found_at_its_turn_and_block_and_shown_by_its_row
  - a_background_sub_agent_keeps_the_agent_at_work_after_its_turn_ends
  - the_live_line_follows_the_newest_exchange_while_a_sub_agent_runs_in_the_background
  - a_turn_the_agent_begins_after_a_background_sub_agent_finished_reports_it
  - each_turn_the_agent_begins_itself_is_a_reply_timed_on_its_own
  - what_set_the_agent_going_heads_its_turn_and_an_error_it_went_on_after_stands_in_place
  - a_command_that_finished_in_the_background_is_reported_like_a_sub_agent
  - a_call_that_finished_after_its_turn_failed_is_reported_by_the_turn_it_set_going
  - a_prompt_sent_mid_turn_carries_on_the_reply_it_came_into
  - a_reply_stays_at_work_while_the_agent_works_past_a_steer
  - a_step_up_lands_on_the_showing_prompt_before_the_one_above_it
- **`chat/src/timeline.rs`:**
  - a_user_item_starts_a_turn_and_a_result_ends_it
  - items_before_the_first_prompt_form_a_turn_without_one
  - a_tool_update_replaces_the_running_call_with_the_same_use_id
  - a_tool_end_after_a_new_prompt_updates_the_call_in_its_own_turn
  - tools_without_a_use_id_never_replace_each_other
  - a_failed_result_ends_the_turn_with_its_error
  - a_result_with_no_prompt_ends_a_turn_of_its_own
  - items_out_of_seq_order_fold_by_seq
  - blank_text_and_thinking_are_left_out
  - webfetch_and_websearch_become_fetch_and_search
  - another_tool_keeps_what_pocketd_says_it_is_about
  - ask_user_question_becomes_a_question_tool
  - a_task_call_becomes_a_sub_agent
  - a_task_call_carries_the_context_its_sub_agent_answered_from
  - items_with_a_parent_fold_into_their_sub_agents_transcript
  - a_sub_agents_own_sub_agent_nests_two_deep
  - a_claude_session_pocketd_paged_nests_each_sub_agent_under_its_own_agent_call (golden `pocketd/internal/proto/testdata/golden/server/timeline_subagents.json`)
  - items_whose_parent_is_no_sub_agent_are_left_out
  - a_tasks_item_becomes_todos_with_mapped_statuses
  - a_compact_item_becomes_a_compacted_marker
  - a_notice_after_an_ended_turn_heads_a_turn_the_agent_began_itself
  - an_error_the_agent_went_on_after_is_a_notice_in_its_turn
  - an_edit_keeps_its_diff
  - a_permission_maps_to_an_ask_with_its_tool
- **`chat/src/session.rs`:**
  - showing_a_row_opens_the_shut_fold_over_it_and_its_shut_phase_but_no_fold_work_holds_open
  - a_session_refolds_only_when_the_revision_moves
  - a_tool_finishing_replaces_its_turns_rows_though_their_ids_hold
  - streaming_text_replaces_only_the_markdown_block_that_grew
  - an_agent_with_no_items_shows_no_rows_and_no_working_row
  - a_streamed_reply_grows_in_one_row_until_its_end_follows_it
  - a_pending_send_adds_its_row_without_a_refold
  - a_send_not_delivered_drops_the_sending_trailer
  - a_queued_send_waits_off_the_transcript_until_its_turn_ends
  - a_new_conversation_keeps_queued_sends_and_fails_none
  - a_shown_transcript_splices_only_when_it_changes
  - toggling_a_row_flips_what_it_shows_from_its_default
  - refolding_each_fixture_scenario_as_it_streams_matches_folding_it_whole
  - refolding_a_five_thousand_item_stream_item_by_item_matches_folding_it_whole
  - a_reply_streaming_after_twelve_hundred_or_five_thousand_items_refolds_well_under_two_ms
  - folding_twelve_hundred_items_takes_well_under_two_ms
  - a_pocketd_restart_mid_turn_holds_a_queued_send
  - a_command_reads_as_work_until_the_timeline_moves
- **`chat/src/outgoing.rs`** (lines 398–1107): a_send_while_idle_is_sending_and_while_working_is_queued, a_send_settles_when_its_prompt_reaches_the_transcript, a_repeated_send_waits_for_its_own_prompt, a_send_not_seen_in_two_minutes_is_not_delivered, a_send_queued_while_working_is_held_until_the_turn_ends, queued_sends_go_one_per_turn_in_the_order_queued, a_send_settles_when_text_left_in_the_terminal_ran_into_its_prompt, a_bash_command_sent_from_the_chat_settles_by_its_echo, a_released_send_gets_two_minutes_from_its_release, withdrawing_drops_a_queued_send_unsent_and_hands_back_its_text, steering_sends_a_queued_send_mid_turn, retry_hands_back_a_failed_send, a_turn_holding_only_a_compaction_lets_the_queue_go, stopping_holds_the_queue_until_resumed, resuming_lets_the_next_queued_send_go, a_new_failure_after_resuming_pauses_again, a_slash_command_never_holds_the_queue, a_slash_command_never_turns_failed, a_refused_send_fails_at_once_and_releases_nothing, a_refusal_holds_the_queue_until_resumed, a_send_getting_through_after_a_refusal_lets_the_queue_go, a_prompt_starting_with_a_path_is_tracked_like_any_other, draining_returns_queued_and_failed_text_in_order, a_steered_send_waits_out_a_long_turn_without_failing, a_queued_send_waits_while_status_says_working_and_the_transcript_has_no_turns, a_command_just_sent_reads_as_work_before_status_shows_it, a_command_that_shows_nothing_holds_the_queue_only_until_status_could_have_shown_it, a_steer_waits_until_its_prompt_reaches_the_transcript_and_a_command_never_does, send_now_goes_once_for_everything_waiting_until_it_shows, a_send_now_nothing_came_of_can_be_pressed_again, a_stop_pauses_the_queue_before_the_turn_shows_it_ended, a_stop_the_turn_never_shows_lets_go_after_ten_seconds, a_stop_before_the_sent_prompt_echoes_reads_stopping_until_it_does, a_steer_the_stop_beat_goes_back_to_the_queue_paused, a_steered_command_reads_as_work_too, a_command_shows_as_sending_until_its_turn_echoes_it, a_send_queued_while_stopping_goes_once_the_stop_lands, a_sub_agent_in_the_background_leaves_the_chat_free_and_holds_no_queue, working_with_an_empty_draft_offers_stop, working_with_a_draft_offers_steer, idle_offers_send, a_turn_being_stopped_offers_no_second_stop_but_still_queues, and about 15 more.
- **`chat/src/marks.rs`:** a_command_that_opens_the_text_is_marked, every_skill_a_prompt_names_is_marked, a_skill_keeps_its_mark_before_punctuation, paths_and_numbers_stay_plain, mentions_start_the_text_or_follow_whitespace, an_mcp_tag_is_no_file, emails_and_a_lone_at_stay_plain, a_mention_shows_its_files_name_or_its_folders_name_and_slash
- **`chat/src/commands.rs`:** a_slash_command_is_its_name_and_argument, a_prompt_that_starts_with_a_path_is_no_command, only_clearing_the_goal_skips_the_queue, claude_runs_pickers_and_compact_itself_and_leaves_panels_out, codex_sends_goal_and_skills_and_leaves_its_tui_commands_out, the_menu_puts_the_agents_own_first_then_its_list_without_panels_or_twins, the_menu_keeps_names_that_hold_the_query_those_it_starts_first
- **`chat/src/mcp.rs`:** the_picker_lists_usable_servers_first_then_those_waiting_on_a_sign_in, only_a_connected_or_connecting_server_is_usable, a_server_the_agent_cant_use_is_signed_in_to_reconnected_or_enabled, a_tag_is_a_whole_word_naming_a_known_server, of_two_names_that_start_alike_the_longer_that_fits_is_the_tag, a_prompt_that_tags_servers_asks_the_agent_to_use_them
- **`chat/src/spawned.rs`:** each_sub_agent_follows_the_one_that_spawned_it, the_map_holds_the_latest_turn_that_spawned_and_earlier_agents_still_at_work, a_conversation_that_spawned_nothing_has_an_empty_map
- **`pocket/chat/markdown.rs`:** a_fenced_block_reads_its_language_and_code, a_fence_under_a_list_item_loses_its_indent_from_each_line, inline_code_naming_a_file_or_folder_is_a_path, a_link_opens_a_web_page_or_a_file_under_the_agents_folder, a_code_block_is_highlighted_again_when_the_appearance_flips, a_growing_code_block_keeps_its_old_colours_until_the_new_run_lands, kept_colours_stop_at_the_code_and_never_split_a_character
- **`vendor/gpui-base/src/text/format/markdown.rs`:** test_nested_emphasis_merges_text_marks, test_inline_html_image_stays_in_markdown_paragraph, test_inline_html_image_without_size_stays_in_markdown_paragraph, test_soft_break_reflows_to_space, test_soft_break_reflows_while_hard_break_survives, test_hard_break_renders_newline, yaml_frontmatter_is_disabled_by_default, non_mapping_yaml_frontmatter_falls_back_to_code_block, fenced_yaml_remains_a_code_block, inline_math_is_enabled_by_default_and_respects_code_and_escapes, block_math_parses_as_a_block_and_falls_back_to_a_code_block, unclaimed_inline_math_keeps_its_literal_source, an_alert_without_a_title_is_titled_by_its_kind, an_alert_takes_its_title_from_the_rest_of_the_tag_line, a_blockquote_with_an_unknown_tag_stays_a_quote, footnotes_are_numbered_in_reference_order_and_unreferenced_ones_dropped, a_code_block_without_a_language_is_labelled_by_how_it_was_written, a_code_block_is_labelled_by_its_language_in_lowercase, an_ordered_list_counts_from_its_first_number
- **`pocket/chat/transcript.rs`:** a_notice_names_the_command_that_finished_in_the_background_and_how, a_notice_keeps_a_command_with_backticks_in_one_code_span, only_text_that_grew_since_the_last_update_streams_in, content_width_clamps_between_560_and_1200_in_steps_of_16, a_live_fold_thinks_until_the_agent_acts_and_waits_on_the_user_first, a_live_fold_reads_stopping_from_the_stop_on, a_settled_fold_counts_the_turns_time_else_its_first_to_last_move, a_phase_is_live_until_its_turn_writes_or_works_on_past_it, a_live_fold_counts_the_sub_agents_working_on_in_the_background, a_phase_hangs_off_its_fold_only_once_the_live_line_leaves_it, a_closed_fold_hides_its_work_but_not_a_failed_sub_agent, a_closed_fold_still_shows_the_text_between_its_work, a_replys_answer_leaves_out_the_text_between_its_work
- **Other `pocket/chat` tests:**
  - `reveal.rs`: a_chunk_landing_at_once_shows_word_by_word_within_a_second_and_a_half, a_few_words_still_take_the_quickest_reveal, text_arriving_mid_reveal_carries_on_from_what_shows, a_reveal_moves_until_its_last_word_has_faded_in, text_already_there_or_rewritten_shows_at_once, a_cut_inside_a_wide_character_still_ends_on_a_word
  - `pin.rs`: 11 tests (section 6)
  - `jump.rs`: the_jump_pill_shows_past_320px_and_hides_at_two, a_glide_eases_the_whole_distance_and_a_far_end_jumps
  - `sticky.rs`: a_prompt_sticks_once_its_turn_scrolls_past_its_top, a_send_held_at_the_top_before_its_echo_sticks_nothing, nothing_sticks_above_the_first_prompt, at_its_end_the_top_row_is_the_first_that_reaches_the_top, the_next_prompt_or_a_pending_send_pushes_the_card, the_card_moves_up_only_once_the_next_prompt_reaches_it, a_pushed_card_loses_its_shadow_over_its_first_12px, a_folded_card_is_one_line_tall_and_an_opened_one_grows_to_160px, a_folded_prompt_reads_its_line_breaks_as_spaces, a_prompt_of_only_images_counts_them
  - `user_bubble.rs`: a_prompt_over_five_wrapped_lines_collapses, a_bubble_shows_each_mention_as_its_name_and_keeps_its_path, a_bubble_tints_each_skill_where_it_shows_after_a_mention, a_bubble_eases_longer_the_further_it_moves_up_to_850ms
  - `tool_row.rs`: each_tool_reads_as_a_verb_and_what_it_acted_on, a_file_badge_names_its_language
  - `sub_agent.rs`: a_sub_agents_name_is_its_tasks_first_line_cut_past_56_chars, a_sub_agent_starts_until_it_does_something_then_counts_its_steps_if_it_shows_any, a_sub_agent_reports_its_output_else_its_last_words_once_done, a_sub_agent_waiting_on_its_own_work_shows_what_it_said_so_far, a_sub_agents_brief_is_the_prompt_it_started_on, a_running_sub_agent_shows_its_newest_call_as_a_step, a_text_counts_the_lines_it_wraps_to
  - `thinking.rs`: live_thinking_keeps_its_tail_and_settled_thinking_its_head, a_long_thought_wraps_between_words_at_96_columns
  - `tool_detail.rs`: output_over_24_lines_keeps_the_head_and_counts_the_rest
  - `diff.rs`: diff_lines_map_onto_git_lines, a_diff_is_coloured_once
  - `todos.rs`: todos_count_done_of_total, the_tasks_card_reads_done_of_total_until_complete
  - `end_row.rs`: a_turn_the_user_stopped_reads_as_interrupted_not_failed
  - `fold.rs`: a_live_fold_says_who_works_and_for_how_long, a_settled_fold_says_how_long_the_agent_worked
  - `working.rs`: elapsed_reads_seconds_then_minutes_then_hours_then_days (format: `12s`, `3m 4s`, `1h 2m`, `1d 3h`)
  - `meta.rs`: timestamps_read_twenty_four_hour_time
  - `question.rs`: eyebrow_labels_are_capitals_spaced_by_hair_spaces, a_question_titles_with_its_first_line_and_shows_the_rest_below, a_single_select_pick_advances_after_220ms, the_last_page_offers_submit_and_waits_for_it, going_back_from_the_first_page_has_nowhere_to_go, number_keys_pick_the_first_nine_options, a_pick_over_typed_words_clears_them_and_the_picks_they_hid, answers_are_keyed_by_question_text
  - `question_marker.rs`: a_question_waits_until_answered_then_shows_its_header
  - `permission.rs`: a_permission_card_names_its_tool_and_path
  - `composer.rs`: a_send_right_after_a_stop_goes_out_even_if_status_still_says_working, sub_agents_left_in_the_background_leave_the_composer_free, a_chat_with_no_turns_yet_goes_by_status, a_queued_send_waits_while_the_agent_asks_and_its_transcript_has_no_turns, a_send_on_its_way_holds_the_next_one, the_footer_hints_at_what_enter_does_now, a_turn_being_stopped_reads_stopping_and_takes_sends_only_into_the_queue, a_terminal_only_command_points_at_the_terminal_only_when_the_agent_has_its_tui, an_observe_only_agent_gets_no_composer
  - `composer/*`: the_queue_lists_only_queued_sends, editing_a_held_send_keeps_what_was_typed, a_task_says_what_it_does_now_and_a_command_just_its_name, the_folded_tray_shows_the_newest_step_of_any_task, a_chat_left_with_only_background_work_still_offers_a_stop, the_tray_offers_no_stop_that_would_end_a_turn_or_reach_no_sub_agent, a_stuck_codex_goal_reads_as_stuck, a_goal_shows_its_tokens_against_a_budget_only_when_it_has_one, the_model_pill_reads_model_and_effort, a_claude_alias_is_the_model_its_full_id_names, plus about 19 in `suggest.rs` (lines 608–776)
  - `agent_map.rs`: escape_leaves_the_details_then_closes_the_map_which_stays_drawn_while_it_fades, reopening_the_map_starts_on_the_graph_with_the_main_agent_picked, the_arrow_keys_stop_at_the_first_and_last_node, enter_opens_the_picked_agent_and_left_goes_back_to_the_graph, a_node_says_how_long_its_agent_ran_and_the_context_it_used, the_chip_counts_the_agents_still_working_out_of_all, the_main_agent_hands_off_to_the_agents_its_turn_ends_with, a_nested_agent_shows_in_the_chat_at_the_call_its_ancestor_was_spawned_under
  - `locate.rs`: touched_files_come_latest_first_once_each_with_a_sub_agents, a_bare_name_means_the_latest_touched_file_so_named_before_any_in_the_repo, untouched_names_are_matched_whole_against_the_repo
- **Item-level fixtures** (`.ui-review/fixture/chat/*.json`, all replayed by `refolding_each_fixture_scenario_as_it_streams_matches_folding_it_whole`):
  - background-agents, background-agents-done, background-shell
  - code-blocks, code-wrap, inline-code, long-markdown, wide-table
  - compaction-after-result, empty, errors, every-tool-kind
  - notices-background, notices-hooks
  - permissions, queued, sticky-prompt
  - streaming, streaming-live
  - sub-agent, subagent-brief-report, subagent-waiting
  - repro-background-{monitor,stale,wake}
  - repro-ingestion-{cli-image-paste,handback,interrupt-then-done,orphan-stopped}
  - repro-steer-midturn{,-done}
  - repro-subagents-{report-handback,report-sync,taskstop}
  - repro-text-{error-then-result,list-fence-split,stop-hook-continuation,synthetic-no-response}
- **Fake pocketd** (`fixture.ts` header):
  - `/fixture <name>` plays a scenario.
  - Built-in scenarios: `perf-1200` and `perf-5000`, `permission` and `question`, `exit`, `clear`, `restart`, and `stream-code` (a code block streaming over 4.5 s). `/compact` works for 4 s, then compacts.
  - Environment flags: FIXTURE_REFUSE, FIXTURE_SEND_NOW, FIXTURE_ECHO_MS, FIXTURE_REFUSE_STOP, FIXTURE_STOP_MS, FIXTURE_CHUNKS.

---

## 4. Outgoing sends (`chat/src/outgoing.rs`)

- **Deliveries:**
  - `Sending`: idle when sent.
  - `Steered`: sent into the running turn; waits for its echo.
  - `Queued`: held locally while the agent works; released one per turn, in order, once the agent is idle and not paused (`settle :121`).
  - `Failed`: no echo within **120 s** of being sent or of the agent next going idle (`GRACE_MS :30`), or refused (pocketd answered `error` for that `agent.prompt` id; `fail :254`).
- **Echo matching** (`same_text :342`): whitespace is collapsed, and the echo **ends with** the sent text, because text left in the TUI runs into the prompt. `! cmd` is treated as `!cmd`. Matching looks back from 10 s of skew, and the 32 newest claimed prompt ids are remembered, so a repeated send waits for its own echo.
- **Slash commands** never echo. They read as "work" until pocketd status shows working→idle: 3 s lag, 60 s max (`:36-38`). A command is never marked Failed.
- **Stop:**
  - Pauses the queue at once.
  - Lasts at most 10 s if the turn never shows an end.
  - A steer the stop beat goes back to the queue, paused.
  - A queue made while stopping goes once the stop lands.
- **Send now:** at most one in flight, for 10 s at most. It applies to all waiting steers.
- **Busy:** `busy()` (`:323`) is true when any send is in flight, a command runs, or the transcript is at work. Status is used only when there are no turns. Background sub-agents **don't** count.
- **pocketd restart:** a queued send survives a restart mid-turn (`a_pocketd_restart_mid_turn_holds_a_queued_send`).

---

## 5. Capabilities (`chat/src/adapter.rs`)
- `Capabilities{interrupt, compact, send_while_working, sub_agents, send_now}` comes from `agent.providers` → `ProviderInfo.chat` (`ChatCaps` in `protocol/src/launch.ts`). If that is absent, the adapter defaults apply:
  - Claude: everything except send_now.
  - Codex: no sub_agents, no send_now.
  - Unknown providers: everything except send_now; every command is sent.
- Codex `pauses_goal` is true.
- **Chat-only agents** (`AgentSummary.chatOnly`): the chat is the only driver. That enables the access picker, the account chip, and Stop in the background tray.
- **View only:** when the client lacks the approve/drive scope (`observe_only`), the composer shows "View only" and asks render read-only.

---

## 6. Scroll and animation behaviours to reproduce

- **List:** bottom-aligned with `FollowMode::Tail`. The list follows new content only while the user is at the end (`transcript.rs:70`).
- **Prompt pinned a quarter down** (`pin.rs`):
  - On send: `pin()`.
  - `held = min(viewport*0.25, TOP(8) + heightAbove)`.
  - `room = viewport - held - BOTTOM(24) - heightOfPromptAndBelow`. A spacer of `room` pads the end. While a row below is unmeasured, the spacer is `viewport - held - 24 - 1`.
  - Once the prompt's row is first measured, it glides from its end position (`viewport - 24 - below`) to `held` over **520 ms**, eased `1-(1-t)^3`. With reduce-motion there's no glide.
  - The pin lets go when `room <= 0`, meaning the reply has filled the space. The list then follows the tail.
  - A user scroll stops the glide. The prompt stays anchored only if the user left the list at its end.
  - Steers keep the pin on the original prompt (`steered`).
  - The prompt is held by item anchor, not by tail follow, so a row growing beneath can't push it up for a frame.
- **Jump pill:** "↓ Scroll to bottom" appears when more than 320 pt from the end and hides at 2 pt or less. Hysteresis keeps it from disappearing mid-scroll (`jump.rs:30`).
  - Tapping glides 300 ms with quint ease-out, or jumps if the distance is more than 2.5 viewports.
  - The pill fades in and drops 2 pt over 180 ms.
- **Sticky prompt card** (`sticky.rs`):
  - Shows once the current exchange's prompt scrolls past the top. Not before the first prompt; not for an unechoed send.
  - A 34 pt pill on the popover colour with shadows: arrow, up to 2 image thumbnails of 20 pt (then "+N"), the prompt on one line (line breaks become spaces; image-only prompts read "N images"), "· H:MM", and a chevron when the prompt is long.
  - The chevron opens the full prompt, which scrolls past 160 pt.
  - Tapping jumps to the prompt.
  - The next prompt **pushes** the card up instead of sliding under it. The shadow fades over the first 12 pt of the push.
  - Enters over 220 ms, 6 pt up.
  - **iOS:** a safe-area overlay pinned to the top, with a pushed offset.
- **Prompt navigation:** ⌘↑ goes to the start of the current exchange, then to the previous one; ⌘↓ goes to the next (`step_prompt :222`). On iOS this could be a long-press menu or a scrubber.
- **Streaming reveal:** see 3.5.
- **Shimmers** are capped at 20 fps. Repeating animations redraw the whole window on desktop; on iOS, use `TimelineView` at a low rate or a gradient mask animation.
- **Flash** when a row is shown from the tray or a notice: `FILL_4` fading over 900 ms.
- **Reduce motion:** rises become 150 ms fades; the glide is off.
- **Question page slide:** 220 ms. Account popover pages: 240 ms slide of 48 pt. Composer trays and asks fade in.

---

## 7. Parity checklist

| Component | Desktop source | Protocol data | Notes for SwiftUI |
|---|---|---|---|
| Fold items → turns | `chat/src/timeline.rs` | `agent.timeline`, `agent.stream` `TimelineItem`, `parentToolUseId` | Port as a pure Swift struct plus tests; incremental refold for 5k items |
| Rows layout | `chat/src/rows.rs` | — | Stable string ids drive diffable updates |
| Session splicing | `chat/src/session.rs` | `AgentSummary.resets` | Reconfigure in place; drop the timeline on `resets` change |
| User bubble | `pocket/chat/user_bubble.rs` | `user.text` | 5-line collapse, `/cmd` tint, `@path` chips, image thumbs from `# Attached images` |
| Sticky prompt | `pocket/chat/sticky.rs` | — | Overlay with push-out |
| Fold header / live line | `pocket/chat/fold.rs`, `transcript.rs:253` | `result.durationMs`, `AgentSummary.model`, pending permission | Shimmer label; default shut |
| Phase | `pocket/chat/phase.rs` | `tool`, `thinking` | Last 9 steps while live; rail graphics |
| Thinking | `phase.rs:67`, `thinking.rs` | `thinking.text` | "Thought for Ns"; 24-line tail or head |
| Tool row | `pocket/chat/tool_row.rs` | `ToolCall{name, detail, status, output, durationMs}` | Verb table; error output only; background tag |
| Inline edit diff | `pocket/chat/diff.rs` | `detail.diff{lines[number, kind, text], additions, deletions}` (≤24 lines) | Hunk renumbering; highlight off-main |
| Sub-agent | `pocket/chat/sub_agent.rs` | `detail.kind=task{description, model, tokens}` plus nested items | Needs the `subagents.v1` cap |
| Agent map | `pocket/chat/agent_map*.rs`, `chat/src/spawned.rs` | as above | Graph view; a sheet on iOS |
| Markdown block | `pocket/chat/markdown.rs`, `vendor/gpui-base/src/text/*` | `assistant.text`, `plan.text` | GFM + alerts + footnotes + path chips; per-block parse |
| Code block | `markdown.rs:221` | fence in text | Language label, wrap toggle, copy; tree-sitter-like highlight with Xcode colours |
| Plan | `transcript.rs:754`, `todos.rs:20` | `plan.text` | "Plan" header + markdown |
| Tasks card | `pocket/chat/todos.rs` | `tasks.items[{text, status}]` | Latest per turn |
| Question marker | `pocket/chat/question_marker.rs` | AskUserQuestion tool | — |
| Question wizard | `pocket/chat/question.rs` | `permission.request` (toolName AskUserQuestion) → `permission.resolve{allow, answers}` | Paged; 220 ms auto-advance |
| Permission card | `pocket/chat/permission.rs` | `PermissionRequest{toolName, detail, feedback, options}` → `permission.resolve{decision, message?}` | Deny / Reply / Allow once; `options` unused on desktop |
| Notes / notices | `end_row.rs:49`, `transcript.rs:334` | `notice{text\|error}`, background calls | Tap to jump + flash |
| Compacted | `end_row.rs:56` | `compact{trigger}` | — |
| End: error / interrupted / footer | `end_row.rs`, `meta.rs` | `result{ok, error, durationMs, usage}` | Copy answer, Retry turn, elapsed + time |
| Pending / Sending / Not delivered | `transcript.rs:787`, `working.rs` | `agent.prompt` ack/error | Retry puts the text back |
| Outgoing state machine | `chat/src/outgoing.rs` | `agent.prompt`, `agent.interrupt`, `agent.send_now`, error frames | Port verbatim with tests |
| Composer box + send button | `pocket/chat/composer.rs`, `composer/send_button.rs` | — | Send/Steer/Queue/Stop/Stopping |
| Slash menu | `chat/src/commands.rs`, `composer/suggest.rs` | `agent.commands` → `{commands[name, description, argumentHint]}` | Builtins first; hide unsupported |
| @file mentions | `composer/suggest.rs:164-199`, `chat/src/marks.rs` | **none** (local git) | Needs a new pocketd file-list message |
| MCP picker / tags | `chat/src/mcp.rs`, `suggest.rs:227` | `agent.mcp{action?, server?}` → `{servers[name, status, scope], authUrl}` | Open `authUrl` in Safari |
| Image attach | `pocket/src/composer.rs:54-245`, `composer/attachments.rs` | **none** (paths in text) | Needs upload to the host |
| Model / effort pill | `composer/model_pill.rs` | `agent.settings{model, effort}`, `ProviderInfo.efforts`, `settings.v1` | Model catalog source: see `settings/models.rs` |
| Access pill | `composer/access_pill.rs` | `agent.settings{access}`, `access.v1`, `phoneMaxAccess` | Picker only when chat-only |
| Queue tray | `composer/queue.rs` | local | Steer / Edit / Remove / Resume |
| Background tray | `composer/background.rs` | running calls past their turn | Show ↑ / Stop |
| Goal strip | `composer/goal.rs` | `AgentSummary.goal{objective, status, tokensUsed, tokenBudget}` | Pause (Codex) / Clear via `agent.prompt` |
| /btw | `composer/btw.rs` | `agent.btw{text}` → `{answer}` | Card over the composer |
| /compact | `adapter/claude.rs:31` | `agent.compact`; `AgentSummary.compacting` | — |
| Account chip / switch | `composer/account_chip*.rs`, `pocket/src/accounts.rs` | **ops socket only**; `AgentSummary.account`, `switching` | Display only until pocketd adds a WebSocket op |
| Context meter | `terminal_view/context.rs`, `panels.rs:181` | `AgentSummary.tokensUsed`, `contextWindow` | Ring; 75/90% thresholds |
| Pin quarter-down | `pocket/chat/pin.rs` | — | 520 ms glide + spacer |
| Jump pill | `pocket/chat/jump.rs` | — | 320/2 hysteresis; 300 ms glide |
| Streaming reveal | `pocket/chat/reveal.rs` | full-item re-sends | 320 B/s, word cut, 240 ms fade |
| Links / file open | `markdown.rs:101`, `locate.rs` | file preview (other research) | URL vs file with `:line` stripped |
| Drafts per agent | `pocket/chat/drafts.rs`, `chat.rs:44` | — | Unsent toast on close |
| Mermaid | `pocket/src/explorer/mermaid.rs` (preview only) | — | Not in chat on desktop; optional on iOS |
