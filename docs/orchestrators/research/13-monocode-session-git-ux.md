# 13 — MonoCode: session, composer, source-control UX

Date: 2026-09-30

Sources:
- hardbeat920/monocode@cdc1441dc51e3709cd843e5c316608a123f323c6
- https://usemono.dev. It gives only the tagline "A GUI for your coding agents", a provider list and downloads, with no UX detail.
- `M docs/screenshot.jpg` (v0.1.0). It is older than the code; see §F13.
- Pocket: coding-pocket `main`@b9d14a1. This research worktree is at 86deb13, which has no `changes.rs`, so every Pocket line number refers to `main`.

Citation legend:
- `M path:L` is a line in the MonoCode clone at the SHA above. Shorthands:
  - `f/` = `src/features/`
  - `css` = `src/styles/index.css`
  - `tauri/` = `src-tauri/src/`
  - Other paths are from the repo root.
- `P path:L` is a line in Pocket `main`@b9d14a1. Shorthands:
  - `d/<crate>/` = `packages/desktop/crates/<crate>/src/`
  - `app/` = `packages/app/src/`
  - `pocketd/` = `packages/pocketd/internal/`
- `Z` is not used.
- `NN-k` means idea k in research report NN.
- Tailwind v4 units: 1 spacing unit = 4 px, so `size-3.5` = 14 px. `content/50` means `--color-content` at 50% alpha.
- Where the code and the screenshot disagree, the code wins.

## TL;DR

- **Tool rows.** Each tool call renders as one "verb + mono file chip" row.
  - Running and successful calls get no trailing icon. A failed row expands to show the error.
  - An edit diff shows inline only while it awaits approval. Otherwise it opens as a 300 ms hover popover (460×280) (`M f/sessions/ui/AgentTranscript.tsx:3064-3584`, `M f/sessions/ui/ToolDiffPreview.tsx:67-166`).
  - The Pocket desktop has no transcript (14-1).
- **Streaming.** Text is revealed at ≥90 chars/s, catches up within 0.22 s and never shows half a word. Each word fades in over 320 ms (opacity only), except in code and links. Everything is off under reduced motion (`M f/sessions/ui/wordFade.tsx:14-184`, `M css:2492-2540`).
- **Approvals.**
  - Inline Allow/Deny under the pending tool row.
  - A top-right toast stack (360 px, dashed border, blur), gated per project by the "agentInput" category. Clicking the body focuses the session (`M f/sessions/ui/ApprovalToasts.tsx:26-141`).
  - The Pocket desktop cannot resolve a permission: its outbox sends only view / seen / close; it only receives `permission.request` / `permission.resolved` (`P d/agents/agents.rs:207-218,276-280`).
- **End of turn.** A "Changed N files" card offers Undo / Keep (tooltip "Undo all session changes") per session. It is backed by checkpoints (≤500 files) and hidden while the agent is busy (`M f/sessions/ui/SessionReview.tsx:82-259`, `M tauri/checkpoint.rs:18`).
  - Pocket has no pre-edit snapshot point. Its PreToolUse hook matches only `AskUserQuestion|ExitPlanMode` (`P pocketd/daemon/plugin.go:10-22`).
- **Add to chat.**
  - An editor selection becomes `@path (lines a-b)`.
  - A diff comment quotes the line as `> ±text` and tags deleted lines (`M f/files/model/editorSelection.ts:8-31`, `M f/source-control/model/diffComment.ts:8-32`).
  - Pocket sends `path line N: text` with no code. A deleted-only selection uses old-side numbers but is not marked as such, so it reads like a new-side line; mixed selections use new-side numbers only (`P d/pocket/main.rs:919-931`, `P d/pocket/diff.rs:127-139,621-625`).
- **Git safety.** MonoCode gates every action on repo state:
  - It blocks push when diverged.
  - It confirms push or PR from the default branch.
  - It warns before amending a pushed head (`M f/source-control/ui/GitChangesPanel.tsx:380-476,588`).
  - Pocket commits and pushes with no confirm (`P d/pocket/changes.rs:533-595`).
- **Upstream and PRs.** Publish / Sync / Create PR / View PR track the upstream. PR title and body come from an LLM ("## Summary" / "## Testing") and go through `gh pr create` (`M f/source-control/model/gitText.ts:44-73`, `M tauri/fs.rs:2596-2610,4045-4075`). Pocket's ahead/behind on a feature branch is measured against local `main`/`master`, not its upstream; only on `main`/`master` itself does it use `@{u}` (`P d/git/git.rs:76-84,107`).
- **Hunk staging.** It stages without touching the working tree, via `hash-object -w --stdin` + `update-index --cacheinfo` (`M tauri/fs.rs:2429-2447`). Fold bars expand 20 lines up or down. Pocket stages whole files only and expands the whole gap (`P d/git/git.rs:188-191`, `P d/pocket/diff.rs:154-170`).
- **Terminal.**
  - Mac editing keys: ⌥←/→ send ESC b/f; ⌘←/→ send ^A/^E; ⌘⌫ sends ^U.
  - Closing a terminal whose process is still running asks for confirmation (`M f/terminal/model/terminalKeys.ts:7-39`, `M f/terminal/model/terminalClose.ts:33-53`).
  - Pocket sends ESC ESC [D for ⌥←, has no ⌘ mappings and closes tabs without asking (`P d/keys/keys.rs:10-47`, `P d/pocket/main.rs:635-641`).
- **Priorities.** Two must-dos, both S: 13-6 (git safety confirms) and 13-12 (Mac terminal keys). Cheap next steps: 13-3 (quoted comments) and 13-2 (hunk staging). The transcript ideas (13-9, 13-10, 13-11) wait on 14-1, 14-2 and 14-3.

## Findings

### F1 Transcript: activity rows

- **Paging.** INITIAL_TURNS 20, FIRST_PAINT_TURNS 3, TURN_PAGE_SIZE 20. "Near bottom" is within 16 px (`M f/sessions/ui/AgentTranscript.tsx:143-162`).
- **Folding.** Thinking and tool calls fold into an "activity" group, but an edit awaiting approval stays outside the fold (`M f/sessions/model/transcriptActivity.ts:148-165`). "tasks" tool calls and "other" calls with weak titles are hidden (`:83-118`).
- **State mapping** (`:44-64`):
  - deny / failed / error / cancelled → rejected
  - pending approval / streaming / in_progress / pending / running → pending
  - completed / success → accepted
- **Row.** `flex items-center gap-1.5 py-1` (`AgentTranscript.tsx:3064-3160`).
  - Lead icon: pending uses CircleDashed 14 px `content/40`, spinning (`zen-tool-spin` 3.6 s linear) while live; every other state uses Minus `content/50` (`:3242-3262`; `M css:2515-2522`).
  - Trailing icon: only rejected gets one, a red-400 X. Code comment: "Running and success do not get a trailing icon." (`AgentTranscript.tsx:3242-3262`).
- **Summary grammar** (`:3467-3584`):
  - Verb: `font-sans text-sm text-content/50`, red-400 when failed.
  - Target: mono 13 px `content/85` on a `bg-content/6` chip with a FileTypeIcon; hover turns it `text-sky-300`.
  - Click: Edit and Write open the diff; other tools open the file.
  - Parsing: `^(Read|Find|Skill|List|Edit|Write)\s+(.+)$` (`transcriptActivity.ts:236-300`). Edit verbs are Delete / Move / Create / Write / Edit (`:204-211`).
- **Failed row.** Clicking toggles an error `pre` (mono 12/20 `text-red-400/80`, `pl-5`). A ChevronRight 14 px `red-400/60` rotates 90° (`AgentTranscript.tsx:3064-3160`).
- **Edit diffs.** The inline FilePreview card shows only while approval is pending; otherwise the edit is a one-line summary with a hover preview (`:3336-3420`).
  - Popover timing: opens 300 ms after pointer enter (touch ignored); closes 180 ms after leave; ↓ focuses it (`M f/sessions/ui/ToolDiffPreview.tsx:67,92-97`).
  - Popover size: 460 px wide, max 280 px tall. Header: `border-b px-2.5 py-1.5 text-[11px] text-content/50` with a 12 px close X (`:132-166`).
  - Header copy: "Proposed changes" / "Attempted changes · tool did not complete" / "Written content · previous contents unavailable" / "Change preview" (`:70-77`).
- **FilePreview card** (`M f/files/ui/FilePreview.tsx:88-215`):
  - Card: `rounded-[10px] border-content/10 bg-content/6`.
  - Header: `px-2.5 py-2`; path mono 12 medium `content/85`; `+N` emerald-400 and `-N` red-400, 11 px semibold.
  - Rows: added `bg-teal-800/20` with a 2 px teal-400 left bar; deleted `bg-rose-800/20` with a rose-400 bar.
  - Line number 28 px wide, 10 px `content/35`; +/- mark 12 px, 10 px bold; code mono 11/18.
  - At most 6 lines (`M src/integrations/harness/core/preview.ts:10`).
- **Thinking.** One line: the prose summary, falling back to "Thinking", 14 px `content/50`, with a Minus icon. Click expands the reasoning markdown (`pl-5`). While streaming it pulses opacity .35↔.9 over 1.8 s (`AgentTranscript.tsx:2926-3000`; `M css:2502-2512`).
- **Duration.** "Working…" becomes "Worked for 2m 39s"; per model it reads "<model> working for 12s". It ticks every 1 s and excludes paused time (`AgentTranscript.tsx:3276-3334`).
- **Entrance motion.**
  - Steps: 480 ms (min 160); queue 960 ms calm / 2000 ms (`:143-162`).
  - Prompt: rise 560, reveal 320, fade 480 ms (`:3787-3792`).
  - CSS: grid-rows 0fr→1fr over `--step-ms*0.4` with `cubic-bezier(0.22,1,0.36,1)`; rail grow plus a branch drawn via clip-path; masked rise of 10 px with `cubic-bezier(0.16,1,0.3,1)`; `--step-ms` defaults to 320 ms (`M css:2386-2481`).
- **Tasks.** Completed: emerald circle and line-through. In progress: sky-300. Pending: empty circle (`M f/sessions/ui/TaskListPreview.tsx:14-101`). Header reads "Complete" or "N of M" (`M f/sessions/model/taskList.ts:82-87`).
  - Pocket phone shows only completed / in_progress / pending, as "done/total" (`P app/components/TaskList.tsx:7-25`).
- **Spinner.** Braille frames at 80 ms (`M f/sessions/ui/TerminalSpinner.tsx:4,26`).
- **Pocket phone.**
  - ToolGroup "N actions · Ns" (`P app/components/ToolGroup.tsx:30,78`).
  - Expandable Thinking, Shimmer and "Auto-compacted" (`P app/components/TimelineView.tsx:104-120`).

### F2 Streaming text

- **Paced reveal** (`usePacedText`, `M f/sessions/ui/wordFade.tsx:61-114`):
  - Runs on rAF with dt capped at 0.05 s. Speed = max(90 chars/s, backlog / 0.22 s) (`:19,24`). Hold 150 ms (`:29`).
  - Text already present at mount shows at once.
  - `revealEnd` never cuts a word (`:36-48`).
- **Word fade.** `rehypeWordFade` wraps each word in `span[data-word-fade]` (`:151-184`). It skips a / code / pre / svg / math / kbd (`:141`). Words keep the fade for 320 ms after appearing (`:14,122-135`). CSS: `word-fade-in 320ms ease-out both`, opacity only (`M css:2492-2500`).
- **Reduced motion** turns off step, fade, pulse and spin animations (`M css:2528-2540`).
- **Pocket.** Codex deltas stream as `assistant_text` (`P pocketd/codex/session.go:157-159`). Claude text arrives as tailed JSONL, whole blocks, so pacing helps Codex only.

### F3 Approvals and questions

- **Inline controls** (`AgentTranscript.tsx:3601-3628`): `mt-1.5 flex gap-2`.
  - "Allow": `rounded-md bg-content px-2.5 py-0.5 text-[11px] text-background-base hover:bg-content/80`.
  - "Deny": `bg-content/10 text-content/70 hover:bg-content/20`.
- **Toast stack.**
  - Placement: portal, `fixed` top 12 right 12, `w-[min(360px,calc(100vw-24px))]`, gap 8 (`M f/sessions/ui/ApprovalToasts.tsx:26-47`).
  - Gate: shown only if the project's "agentInput" category is enabled (`:60-78`).
  - Card: `rounded-xl border border-dashed border-content/20 bg-content/10 shadow-xl backdrop-blur-xl`; body `px-3.5 py-3 gap-2` (`:98-141`).
  - Content: title 13 px semibold; amber-400 11 px "Question" / "Approval" tag with a 14 px CircleAlert; label clamped to 3 lines, 12 px `content/70`; harness name 11 px `content/40`.
  - Footer: `border-t px-3.5 py-2.5` with two flex-1 "Allow" / "Deny" buttons, 11 px medium.
  - Clicking the body focuses the session.
  - Entry: `approval-toast-in` 180 ms `cubic-bezier(0.22,1,0.36,1)` from translateY(-8px) scale(.98) (`M css:1930-1943`).
- **QuestionForm.**
  - Card: `rounded-lg border-content/10 bg-content/3 px-3 py-2.5` (`M f/sessions/ui/QuestionForm.tsx:104`).
  - Copy: "Optional question", "Select all that apply", "Type your answer", "Other" (`:175,263,269,359`).
  - Keys: ↑/↓, Enter/Space (`:227-238`).
  - Already covered by 05-4 / 10-10 / 14-11.
- **Pocket.**
  - Phone PermissionSheet: "Do you want to proceed?", Yes / options / No, and feedback "No, and tell Claude what to do differently" (`P app/components/PermissionSheet.tsx:30-80`).
  - Desktop agent ops are view / seen / close only; there is no resolve (`P d/agents/agents.rs:207-218`).

### F4 Composer

- **Box.** `data-composer-box`, `rounded-lg border border-content/10 bg-content/3 backdrop-blur-sm has-focus:border-content/20` (`M f/sessions/ui/Composer.tsx:1936-1953`).
  - File drag: `border-accent/60` plus a "Drop files to attach" overlay (`bg-accent/8`, 12 px `content/70`).
  - Light theme: base background with shadows `0 6px 24px /9%` and `0 2px 6px /6%` (`M css:1371-1376`).
- **Textarea.** `max-h-40` (160 px), 14/22, px 12 (`Composer.tsx:2103`). `.composer-field`: caret in content colour, placeholder at 40% (`M css:1427-1440`).
- **Placeholders by state** (`Composer.tsx:2087-2099`):
  - "Ask, build, / for commands, @ for references..."
  - "Select a branch or worktree to continue…"
  - "Add a note, or send to start…"
  - "Add a message, or send…"
  - "Add context, or send to continue…"
- **Send / Stop** (`:2540-2590`):
  - Send: `primary-action` 26 px `rounded-md`, ArrowUp 14 px stroke 2.25.
  - Stop: `bg-white text-black` with a 10 px filled square.
  - `.primary-action`, dark theme: white on black, hover white 90%; disabled white 30% on black 40%. Accent variants exist (`M css:1381-1425`).
- **Mode chips.** Buttons "Turn off Plan mode", "Turn off Draft mode", "Turn off Operator", "Turn off Orchestrator mode" (`Composer.tsx:2298-2348`).
- **Queue.**
  - Card docked on top of the box: `rounded-t-[10px] border border-b-0 border-content/10 bg-content/3 px-2 py-1` (`:357-470`).
  - Paused row: 28 px, "Queue paused because you interrupted" plus "Resume" (Play icon).
  - Rows: ≥28 px, 12 px, ListEnd icon.
  - Actions: "Steer" (CornerDownRight, 24 px tall), edit (Pencil) and remove (Trash2) as 24 px buttons; editing happens inline.
  - Default follow-up is `"steer"` (`M f/settings/model/settings.ts:532-547`).
  - Covered by 05-3 / 10-9.
- **Drafts.** An in-memory Map keyed by session id. Drafts survive a pane unmount but not a restart (`M f/sessions/model/draftCache.ts:1-34`). The Pocket phone draft is plain `useState`, and Attach and Mic have no handlers (`P app/components/Composer.tsx:1-92`). See 05-11 / 14-17.
- **Slash picker** = SkillPicker, ranking skills and commands (`Composer.tsx:664-706`). It offers "New skill", 2-line 11/16 descriptions and "Writes a starter SKILL.md you can edit." (`M f/skills/ui/SkillPicker.tsx:87,185,251`).
- **@ mentions.** "Indexing files…" / "No matching files or folders"; list `max-h-[min(240px,40vh)]` (`M f/sessions/ui/FileMentionPicker.tsx:67-82`). Mentions render as `text-mention`, #38bdf8 dark / #0284c7 light (`:122`; `M css:64-65,107-108`).
- **Model picker.** Menus 250 / 310 / 210 px; keybinding "App: Switch Model" (`M f/sessions/ui/ModelPicker.tsx:83-85,457`).
- **Access picker.**
  - Menu 288 px; trigger 26 px with `max-w-52` (208 px); full access shows in amber-400/90 (`M f/sessions/ui/AccessPicker.tsx:29,103,110`).
  - Footnote: "Access changes apply to the next turn. Stop and resend to apply them now." (`:171`).
  - Its label hides when the toolbar container is under 220 px (`M css:1358-1370`).
- **Attachments.** Max 20 files, 20 MiB (`M f/sessions/model/attachments.ts:5-6`). Chip names truncate at 140 px, 11 px (`M f/sessions/ui/AttachmentChip.tsx:53`).
- **Context meter.** Amber at ≥0.75, red at ≥0.9, plus "Compact now" (`M f/sessions/ui/ContextMeter.tsx:16-17,92`). Covered by 05-7.

### F5 Session status

- **Sidebar row status** (11 px, tabular numerals) (`M src/app/shell/Sidebar.tsx:2993-3029`):
  - Needs approval: amber-400 CircleAlert 12 px, "Need approval" ("Needs input" under orchestration).
  - Busy: accent braille spinner, "Working...".
  - Done: emerald-400 Check, "Done".
  - Draft: `content/55` CircleDashed, "Draft".
  - Otherwise: time in `content/45`.
- **Pocket** is at parity:
  - NeedsYou / Failed / Done / Working / Idle (`P d/pocket/status.rs:5-42`).
  - Pills, e.g. Working with a spinner on RUNNING_BG (`P d/ui/ui.rs:234-241`).
  - Tab lead dots (`P d/pocket/view.rs:806-810`).
  - Row treatment is covered by 12-3.

### F6 End-of-turn review

- **SessionReview card** (`M f/sessions/ui/SessionReview.tsx:82-259`):
  - Hidden while the agent is busy.
  - Container `rounded-xl border border-content/12 bg-content/3`; header "Changed N files" (`:129,138`).
  - Shows 3 files, then "Show N more files" (`:215`). A file can carry a "Mixed changes" tag (`:259`).
  - Buttons read "Undo" and "Keep". Undo is enabled only if every file is undoable and undo is not locked (another session running in the project). Undo-disabled tooltips at `:156-157`, Keep tooltip at `:167`.
  - Keep and Undo call `keepSessionChanges` / `undoSessionChanges`, then `notifyGitChanged` (`:107-124`).
- **Backing store.** Per-session checkpoints, MAX_SNAPSHOT_FILES 500, Tauri commands at `M tauri/checkpoint.rs:18,574-718`. Edits are tracked from tool calls (`M src/App.tsx:11263-11285`). Mechanics are covered by 10-14.
- **Pocket.**
  - `last_edit` comes only from edit/write timeline calls (`P d/agents/agents.rs:163-176`).
  - PreToolUse matches only "AskUserQuestion|ExitPlanMode"; PostToolUse matches all (`P pocketd/daemon/plugin.go:10-22`).
  - So there is no hook that fires before an edit, which a snapshot needs.

### F7 Add to chat

- **Bus.** Event `monocode:add-to-chat` with modes quote / plain; quote mode prefixes each line with `> ` and appends to the draft (`M f/sessions/model/quoteDraft.ts:1-60`).
- **Editor selection.**
  - Popover above the selection, gap 6, `p-1`; one 28 px `rounded-lg` 13 px button, MessageSquarePlus icon, "Add to chat" (`M f/files/ui/EditorSelectionMenu.tsx:39-68`).
  - Reference format: `@path (line N)` / `@path (lines a-b)`. `@` is used only for relative paths ≤120 chars with no space, `@` or `..`; otherwise the path is backticked (`M f/files/model/editorSelection.ts:8-31`).
- **Diff comment.**
  - Popover 320 px; placeholder "Leave a comment…"; hint "⌘↩ to add" (10 px `content/35`); button "Add to chat" (`M f/source-control/ui/DiffCommentComposer.tsx:38-93`).
  - Text inserted into the draft; N is the old line number for a deleted line, else the new one (`M f/source-control/model/diffComment.ts:8-32`):

```
Diff comment on `path:N` (deleted line):

> -text

body
```

- **Pocket.**
  - A comment sends at once as `{"op":"prompt","text": "{path} {lines}: {text}"}`, e.g. "src/a.rs line 54: …", with no quoted code (`P d/pocket/main.rs:919-931`).
  - Labels use new-side numbers; a selection of only deleted lines uses old-side numbers with no "deleted" marker, so "line 2" is ambiguous (`P d/pocket/diff.rs:127-139,621-625`).
  - Target: the picked session, else the open one, else the project's newest (`P d/pocket/main.rs:933-938`).
  - Explore only has "Ask about this file", which starts a new session with "About {rel}: " (`P d/pocket/explore.rs:292,318`).

### F8 Source control panel

- **Header.** 36 px tall, "Changes" 12 px medium (`M f/source-control/ui/GitChangesPanel.tsx:197-209`).
- **Commit box** (`:680-800`):
  - Textarea `rounded-md bg-content/10 text-[13px] leading-5 max-h-40`, placeholder `content/35`.
  - Placeholder copy: "Message (⌘↩ to commit)" / "Amend message (⌘↩ to amend)".
  - A generate button toggles between "Generate commit message" and "Cancel commit message generation".
  - Split button, 28 px: "Commit" / "Amend Commit" plus a "Commit options" menu with "Commit & Push", "Commit, Push & Create PR" and an amend toggle (the amend toggle loads the HEAD message, `:567-583`).
- **File list.**
  - Section headers 28 px, 10 px semibold uppercase, tracking 0.04em, `content/55` (`:1153-1170`).
  - Rows 28 px: name 13 px medium, directory 11 px `content/40`; status letter mono 11 px semibold, w 14 (`:1453-1514`).
  - Status colours: U sky-400, A emerald-400, D red-400, M amber-400 (`:1553-1565`).
  - Empty state: "No uncommitted changes" (`:840`).
- **Action gates** (`:380-405`):
  - canCommit = (staged or amend) ∧ message ∧ ¬busy.
  - canCommitPush = canCommit ∧ remote ∧ ¬diverged ∧ ¬(amend of a pushed head).
  - canCommitPushPr = canCommitPush ∧ no open PR ∧ not on the default branch.
  - canPublish = remote ∧ no upstream.
  - canSync = upstream ∧ (ahead ∨ behind).
  - canCreatePr = remote ∧ branch ∧ default branch known ∧ no open PR ∧ not on default ∧ ¬diverged ∧ clean tree ∧ aheadOfDefault > 0 ∧ behind = 0.
- **Confirms.**
  - `Push to default branch "X"?` / `Create a pull request from default branch "X"?` (`:468-476`).
  - Discard confirms (`:488,516,519`).
  - Amend of a pushed commit (`:588`).
- **Flows.**
  - Commit → push → optional PR (`:593-624`).
  - PR: LLM title/body → `gh pr create --title --body-file <tmp> --base --head` → parse `/pull/N` → open the URL (`:642-675`; `M tauri/fs.rs:4045-4075`).
  - Sync bar: `Publish Branch "X"`, `View PR #N: title`, "Sync Changes", plus a status label (`:976-1128`).
  - Push runs `push` if an upstream exists, else `push -u <remote> HEAD`. Sync runs `pull --no-edit --ff`, then `push` (`M tauri/fs.rs:2596-2610`).
  - A signing failure adds a pinentry-mac hint (`:2585-2590`).
- **LLM prompts** (`M f/source-control/model/gitText.ts:13-88`):
  - Commit: JSON {subject, body}; "subject must be imperative, <= 72 chars, and no trailing period"; staged summary ≤6 000 chars, patch ≤40 000.
  - PR: JSON {title, body}; body markdown with "## Summary" and "## Testing" ("Not run" where appropriate); caps 12 000 / 12 000 / 40 000.
  - Branch name: 2–6 words.
- **Graph.** Swimlanes 22 × 11 px, curve radius 5, node radius 4 with stroke 2 (`M f/source-control/model/gitGraph.ts:82-86`). Covered by 06-11.
- **Pocket.**
  - Button reads "Commit All" when nothing is staged, else "Commit"; menu "Commit & Push" / "Amend Last Commit" (`P d/pocket/changes.rs:215-216,266-267`).
  - `commit` stages everything if nothing is staged, runs `git commit -q [--amend] -F -` (`--no-edit` for an amend with no message) and optionally pushes. It has no default-branch confirm, no pushed-amend warning and no divergence check (`:533-574`).
  - Push is `git -c push.autoSetupRemote=true push` (`:627-629`).
  - Messages come from `claude -p --model haiku` and cannot be cancelled (`:597-624`).
  - Ahead/behind: against `@{u}` on `main`/`master`, else against local `main`/`master` (hardcoded, not the remote default) (`P d/git/git.rs:76-84,107`).
  - Staging is per path, `add -A` / `reset` (`:188-191`). There is no pull, stash, branch switch or hunk staging.

### F9 Diffs and editor git

- **Unified diff geometry.** Line 20 px, fold bar 32 px, hunk 22 px, overscan 1200 px (`M f/source-control/model/unifiedDiffWindow.ts:8-11`). Context 3 lines, fold step 20 (`M f/source-control/model/unifiedDiff.ts:6-7`).
- **Fold bar.** Two 20 px icon buttons, ChevronUp "Expand upward" and ChevronDown "Expand downward" (`content/40`, hover `bg-content/10`), then mono 11 px `content/45` "N unmodified lines", which reveals all on click (`M f/source-control/ui/UnifiedDiffView.tsx:860-886`).
- **Rows.** Added `bg-emerald-500/15`, deleted `bg-rose-500/15` (`:925`). Actions "Comment on line N" (`:956`) and "Stage hunk" (`:973`). Big diffs show "Diff is too large…" (`:234`).
- **Hunk staging.**
  - Builds the index text with only that chunk applied (`M f/files/editor/editorGit.ts:327-360`).
  - Writes it with `git hash-object -w --path rel --stdin` and `update-index --add --cacheinfo mode,hash,rel`. The mode comes from `ls-files --stage`, defaulting to 100644.
  - The working tree is never touched (`M tauri/fs.rs:2429-2447`).
- **CodeMirror git** (`@codemirror/merge` chunks, not MergeView):
  - DIFF_CONFIG: scanLimit 5000, timeout 100 (`editorGit.ts:25`). Line height round(13×1.6) = 21 (`:54`).
  - Hunk bar: "Comment on line" / "Revert change" / "Stage change" (`:812-814`).
  - Theme (`:1052-1130`):
    - Gutter 22 px with 3 px markers; added #34d399.
    - Inserted line at 18% with an inset 3 px bar; deleted #f87171 at 16%.
    - Overview ruler width `var(--editor-scrollbar-width, 18px)`; a modified hunk is a half-red/half-green gradient.
    - Buttons 18 px.
- **Pocket diff** (`P d/pocket/diff.rs`):
  - NUM 44, SIGN 18, ROW 22; colouring capped at 512 KiB; word diff only for ≤5-line blocks (`:13-18`).
  - "N unchanged lines" (12.5 px) expands the whole gap (`:154-170,383-389`).
  - Selected line: accent tint and a 3 px bar (`:188-208`).
  - Unified/Split control plus a "Viewed" checkbox (`:226-284`).
  - Comment composer: radius 16, "esc to dismiss", "Comment ⌘↵", target pill 32 px, menu 360 px (`:408-590`).
- **Pocket Explore.** Read-only, mono 13/22 (`P d/pocket/explore.rs:365`); gutter tints amber for modified, green for added (`:104-126`). No selection action and no hunk actions.

### F10 Branch and worktree dialogs

- **Switch with a dirty tree** (`M f/source-control/ui/SwitchBranchDialog.tsx:126-224`):
  - Title "Uncommitted changes".
  - Body: `Switching to “X” would overwrite your local changes. Stash them for later, or commit them on this branch first.` A create variant exists.
  - Buttons "Commit & switch" / "Stash & switch".
- **Delete worktree** (`M f/source-control/ui/DeleteWorktreeDialog.tsx:84-155`):
  - Title "Delete worktree?"; body "This permanently deletes the working copy and everything inside it."
  - A note that the branch and its commits are kept, and that unpushed commits are "not on a remote. They stay on the branch."
  - Checkbox "Also delete associated sessions".
- **Pocket** (`P d/pocket/overlay.rs:268-300`):
  - Delete worktree lists "Deletes the folder X", "Keeps the branch Y" and "N uncommitted files will be lost." on FAILED_BG.
  - Discard lists "Unstaged edits can't be restored" and "Deletes N untracked files".
  - It shows no unpushed-commit count.

### F11 Terminal

- **xterm settings.** cursorBlink, 13 px, scrollback 5000, smoothScrollDuration 0, macOptionIsMeta (`M f/terminal/ui/TerminalView.tsx:157-167`).
- **Mac keys** (`M f/terminal/model/terminalKeys.ts:7-39`):
  - ⌥← `\x1bb`, ⌥→ `\x1bf`, ⌘← `\x01`, ⌘→ `\x05`, ⌘⌫ `\x15`.
  - Skipped while Ctrl or Shift is held.
  - ⌘K clears only if "App: Search" is unbound.
- **Close confirm** (`M f/terminal/model/terminalClose.ts:33,45,53`):
  - `"${process}" is still running in ${label}. Close this terminal anyway?`
  - Multi-terminal: "These terminals are still running:\n…\n\nClose them anyway?"
- **Dock.** 6 px sash; double-click resets (`M f/terminal/ui/ProjectTerminalDock.tsx:172-203`). Buttons "New Terminal (⌘`)", "Move Terminal", "Hide Terminal (⌘J)" (`:221-239`).
- **Arcade.** Pac-Man, Snake and grid games in empty terminals, 33 ms frames (`M f/terminal/arcade/pacmanArcade.ts`, `snakeArcade.ts`, `gridGames.ts`; `M f/terminal/arcade/pacmanArcade.test.ts:9`). Already noted in 09 and 12.
- **Pocket.**
  - Alt adds an ESC prefix, so ⌥← sends `\x1b\x1b[D`; there is no ⌘-arrow or ⌘⌫ mapping (`P d/keys/keys.rs:10-47`).
  - `close_tab` does not confirm (`P d/pocket/main.rs:635-641`).
  - ⌘J is "next waiting" (`:1062-1071`).

### F12 Notifications

- **Categories.** pullRequests, issues, agentFinished, agentInput, reminders (`M f/notifications/model/notificationPreferences.ts:1-7`).
- **Mute presets.** 1 / 4 / 8 h, "Until resumed", "Choose date and time". Labels carry the end time, e.g. "(Tomorrow, 9:00)"; the status reads "Muted until …" (`M f/notifications/ui/notificationMuteActions.ts:1-50`).
- **Policy.** A banner shows unless the window is focused *and* the session is visible. Focus comes from the Tauri event because WKWebView keeps reporting `hasFocus()` true (`M f/notifications/model/notifications.ts:79-107`).
- **Text** (`:159-221`):
  - Title "MonoCode", subtitle = session title.
  - Body = the question prompt, or `Approve: <tool title>`, or the reply's first paragraph.
  - Whitespace is collapsed and the body capped at 240 chars (BODY_MAX `:166`).
- **Sound.** If the OS accepted the banner, the in-app cue is skipped (`:223-262`).
- **Dock badge** = pending approval count (`M f/notifications/model/dockBadge.ts:6-14`).
- **Pocket.** Title = agent title or provider name; body = status label only (`P d/pocket/main.rs:338-352`). A viewed agent is not alerted, but an inactive window views nothing, so background alerts already fire (`P d/pocket/status.rs:119-123`, `P d/pocket/main.rs:389-396`).
- Badge, mute and actions are covered by 09-1, 09-2 and 14-10.

### F13 Docs vs code

- **Screenshot drift.** The v0.1.0 screenshot shows an older "2 Files · Undo All · Keep All · Review" bar, inline "+100 −217" edit cards and "Worked for 2m 39s" folds (`M docs/screenshot.jpg`). The current code has the "Changed N files" card (`SessionReview.tsx:138`) and shows inline diffs only while pending (`AgentTranscript.tsx:3336-3420`).
- usemono.dev documents no session or git UX.

### Pocket delta summary

| Area | MonoCode | Pocket main@b9d14a1 |
|---|---|---|
| Transcript + tool rows | Structured, per-kind rows (F1) | Phone only (`P app/components/ToolGroup.tsx:30,78`); none on desktop (14-1) |
| Approvals | Inline + toast stack (F3) | Phone sheet; desktop cannot resolve (`P d/agents/agents.rs:207-218`) |
| End-of-turn review | Undo/Keep card (F6) | None |
| Diff comments | Quoted, deleted-line aware, goes into the draft (F7) | Sent at once, no code, ambiguous for deleted lines (`P d/pocket/main.rs:919-931`) |
| Git gates | 6 gates + 3 confirms (F8) | None (`P d/pocket/changes.rs:533-595`) |
| Upstream / PR | Publish / Sync / PR (F8) | Push only; feature-branch ahead/behind vs local main/master |
| Hunk staging | Index blob write (F9) | Whole file only |
| Terminal keys / close | Mac keys + confirm (F11) | ESC-prefix alt, no confirm |
| Notification text | The ask itself, ≤240 chars (F12) | Status label |

## Ideas to clone into Pocket

| ID | Idea | User value | Evidence | Pocket mapping (package/crate + port/adapt/new) | Effort | Prerequisites |
|---|---|---|---|---|---|---|
| 13-1 | End-of-turn card: "Changed N files", Undo / Keep, per-file rows, 3 shown + "Show N more files", hidden while busy | Revert one agent's turn without touching git or other agents' edits | `M f/sessions/ui/SessionReview.tsx:82-259`; `M tauri/checkpoint.rs:18,574-718`; `M src/App.tsx:11263-11285` | pocketd: new checkpoint store, snapshot on PreToolUse. desktop pocket: new card on the agent page. app: later | L | 10-14; pocketd PreToolUse matcher `Edit\|Write\|MultiEdit\|NotebookEdit` (`P pocketd/daemon/plugin.go:10-22`) |
| 13-2 | Hunk stage / revert: build the index text with one hunk applied, `hash-object -w --stdin` + `update-index --cacheinfo` | Commit part of an agent's change without a separate git client | `M tauri/fs.rs:2429-2447`; `M f/files/editor/editorGit.ts:327-360,812-814`; `M f/source-control/ui/UnifiedDiffView.tsx:973` | git crate: new `stage_hunk` (port). pocket `diff.rs`: hunk header buttons "Stage hunk" / "Revert change" (adapt) | M | None |
| 13-3 | Comment prompt quotes the line(s) (`> ±text`), uses `path:N` and tags "(deleted line)" with old-side numbers | The agent knows exactly which code the comment means | `M f/source-control/model/diffComment.ts:8-32`; `P d/pocket/main.rs:919-931`; `P d/pocket/diff.rs:127-139,621-625` | pocket `main.rs` `submit_comment` + `diff.rs` `label` (adapt) | S | None |
| 13-4 | Upstream sync bar: `Publish Branch "X"`, Sync ↓N ↑M (`pull --no-edit --ff` + push), Create PR, `View PR #N: title` | Keep worktree branches in sync with the remote without a terminal | `M f/source-control/ui/GitChangesPanel.tsx:380-405,626-640,976-1128`; `M tauri/fs.rs:2596-2610`; `P d/git/git.rs:107` | git crate: `@{u}` ahead/behind, pull (new). pocket `changes.rs` header (adapt) | M | 06-8 for PR status |
| 13-5 | "Commit, Push & Create PR": LLM JSON title/body ("## Summary", "## Testing"), `gh pr create --body-file` | One action from agent output to an open PR | `M f/source-control/ui/GitChangesPanel.tsx:593-675,791`; `M f/source-control/model/gitText.ts:44-73`; `M tauri/fs.rs:4045-4075` | pocket `changes.rs` menu item + `claude -p` (new) | M | 13-4, 13-6; `gh` on the login-shell PATH |
| 13-6 | Git safety: confirm push/PR from the default branch, warn before amending a pushed head, disable push when diverged | Prevents accidental pushes to main and force-push surprises | `M f/source-control/ui/GitChangesPanel.tsx:380-405,468-476,588`; `P d/pocket/changes.rs:533-595` | pocket `changes.rs` + `overlay.rs` confirm (adapt); git crate: upstream counts | S | Divergence check needs the upstream counts from 13-4 |
| 13-7 | Explore selection → floating "Add to chat" → `@path (lines a-b)` into the picked session | Point an agent at exact code without typing paths | `M f/files/ui/EditorSelectionMenu.tsx:39-68`; `M f/files/model/editorSelection.ts:8-31`; `M f/sessions/model/quoteDraft.ts:1-60`; `P d/pocket/explore.rs:292,318,365` | pocket `explore.rs` + target picker from `diff.rs:513-590` (adapt) | M | Text selection in the read-only editor |
| 13-8 | Explore change navigation: gutter markers, overview ruler (half red/green for modified), inline hunk bar Comment / Revert / Stage | Scan and act on changes in context | `M f/files/editor/editorGit.ts:25,54,812-814,1052-1130`; `P d/pocket/explore.rs:104-126` | pocket `explore.rs` (adapt) | M | 13-2 for Stage |
| 13-9 | Tool row grammar: verb + mono file chip, no trailing icon on success, expandable failed error, hover diff 300 ms open / 180 ms close, 460×280 | Scan long agent runs fast; errors one click away | `M f/sessions/ui/AgentTranscript.tsx:3064-3262,3467-3584`; `M f/sessions/model/transcriptActivity.ts:44-118,236-300`; `M f/sessions/ui/ToolDiffPreview.tsx:67-166` | app `ToolGroup.tsx` (adapt); desktop transcript (new) | M | 14-1, 14-3; ref 05-1, 05-10 |
| 13-10 | Paced reveal (≥90 chars/s, 0.22 s catch-up, whole words) + 320 ms word fade, off under reduced motion | Streaming reads calmly instead of jumping in bursts | `M f/sessions/ui/wordFade.tsx:14-184`; `M css:2492-2540` | app `Markdown.tsx` / `TimelineView.tsx` (port) | M | Streamed deltas (Codex only today, `P pocketd/codex/session.go:157-159`); alternative 05-19 |
| 13-11 | In-app approval toast stack: top-right, 360 px, dashed, "Question" / "Approval" tag, Allow / Deny, body click focuses | Answer asks from any screen without hunting for the agent | `M f/sessions/ui/ApprovalToasts.tsx:26-141`; `M css:1930-1943` | desktop pocket: new overlay layer | M | 14-2 (resolve op), 14-3 |
| 13-12 | Mac terminal editing keys: ⌥←/→ ESC b/f, ⌘←/→ ^A/^E, ⌘⌫ ^U; skipped with Ctrl/Shift | Word and line navigation work in shells as in iTerm/Terminal | `M f/terminal/model/terminalKeys.ts:7-39`; `P d/keys/keys.rs:10-47` | keys crate (adapt) | S | None |
| 13-13 | Close-tab confirm: `"X" is still running in Y. Close this terminal anyway?` plus a multi-terminal variant | No lost dev servers or agent runs from a stray ⌘W | `M f/terminal/model/terminalClose.ts:33-53`; `P d/pocket/main.rs:635-641`; `P d/pocket/sessions.rs:14-17` | pocket `main.rs` `close_tab` + `overlay.rs` confirm using existing `Session::busy()` foreground command (adapt) | S | None; pocketd already broadcasts the foreground command (`P pocketd/terminal/terminal.go:233-249`) |
| 13-14 | Task list: cancelled state, "Complete" / "N of M" header | Plan progress reads at a glance | `M f/sessions/ui/TaskListPreview.tsx:14-101`; `M f/sessions/model/taskList.ts:82-87`; `P app/components/TaskList.tsx:7-25` | app `TaskList.tsx` (adapt) | S | The timeline must carry a cancelled status |
| 13-15 | Delete-worktree dialog adds the unpushed-commit fact and "Also delete associated sessions" | Know what is lost; no orphan sessions | `M f/source-control/ui/DeleteWorktreeDialog.tsx:84-155`; `P d/pocket/overlay.rs:268-300` | pocket `overlay.rs` + git crate `rev-list @{u}..` (adapt) | S | Ref 06-14 |
| 13-16 | Fold bar steps of 20 lines up/down plus "N unmodified lines" (all) | Expand only the context needed | `M f/source-control/model/unifiedDiff.ts:6-7`; `M f/source-control/ui/UnifiedDiffView.tsx:860-886`; `P d/pocket/diff.rs:154-170,383-389` | pocket `diff.rs` `hunk()` (adapt) | S | None |
| 13-17 | Cancellable commit-message generation; JSON subject ≤72 chars, imperative, no period; input caps | No stuck spinner; consistent subjects | `M f/source-control/model/gitText.ts:13-42`; `M f/source-control/ui/GitChangesPanel.tsx:540-565`; `P d/pocket/changes.rs:597-624` | pocket `changes.rs` `write_message` (adapt) | S | None |
| 13-18 | Dirty-tree branch switch: "Commit & switch" / "Stash & switch" | Switch branches safely | `M f/source-control/ui/SwitchBranchDialog.tsx:126-224` | git crate + new dialog | M | Conflicts with Pocket's worktree-first model |
| 13-19 | Arcade games in empty terminals | Delight only | `M f/terminal/arcade/` | term crate (new) | M | None |
| 13-20 | Notification body = the ask (`Approve: <tool>`, the question prompt, or the reply's first paragraph ≤240 chars) | Decide from the banner whether to switch now | `M f/notifications/model/notifications.ts:159-221`; `P d/pocket/main.rs:338-352` | pocket `main.rs` `sync_alerts` (adapt) | S | Tool title / question in AgentSummary (14-3); ref 09-2, 14-10 |

## UI/UX spec to copy

Tailwind values below are converted to px. Pocket token names come from `P d/theme/theme.rs:30-53`: ACCENT 0x5b5bd6, WAITING 0xffb224, RUNNING 0x30a46c, FAILED 0xe5484d, DIFF_ADD_BG 0x30a46c1c, DIFF_DEL_BG 0xe5484d17.

**Layout**
- **Tool row:** icon 14 → gap 6 → verb (text 14, 50% ink) → chip (file icon + mono 13 path, 85% ink, bg ink 6%, radius 4) → trailing X only when failed. Padding 4 px vertical. Failed error block indents 20 px (`AgentTranscript.tsx:3064-3584`).
- **Composer stack**, top to bottom: queue card (radius 10 on top corners, no bottom border) → composer box (radius 8, ink 3% fill, 10% border, 20% on focus) → toolbar (model / access / mode chips left, context meter + send right) (`Composer.tsx:357-470,1936-1953`).
- **Review card** sits between the transcript and the composer: header "Changed N files" + Undo / Keep; up to 3 file rows; "Show N more files" (`SessionReview.tsx:129-215`).
- **Changes panel**, top to bottom (`GitChangesPanel.tsx:197-1565`):
  - Header 36.
  - Commit box: p 8; textarea; 28 px split button full width; sync bar 28 px.
  - Sections 28 px: "Staged Changes" / "Changes".
  - File rows 28 px.
  - Graph at the bottom.
- **Toasts:** top-right stack, 12 px inset, 8 px gap, max width 360 (`ApprovalToasts.tsx:26-47`).

**Measurements**
- **Hover diff:** 460 × ≤280. Header 11 px, padding 10/6. Close icon 12 (`ToolDiffPreview.tsx:132-166`).
- **FilePreview:**
  - Radius 10, header padding 10/8.
  - Line number column 28 (10 px text); mark column 12; code 11/18; left bar 2 px; ≤6 lines (`FilePreview.tsx:88-215`; `preview.ts:10`).
- **Composer:**
  - Textarea max 160 tall, 14/22.
  - Send 26 × 26, radius 6, arrow 14 stroke 2.25.
  - Stop glyph 10 (`Composer.tsx:2103,2540-2590`).
- **Queue rows:** ≥28, 12 px text, action buttons 24 (`Composer.tsx:357-470`).
- **Diff comment popover:** 320 (`DiffCommentComposer.tsx:38`). Selection menu: button 28, 13 px, gap 6 above the selection (`EditorSelectionMenu.tsx:39-68`).
- **Unified diff:** line 20, fold 32, hunk 22; fold buttons 20 with 12 icons (`unifiedDiffWindow.ts:8-11`; `UnifiedDiffView.tsx:860-886`).
- **Editor git:**
  - Gutter 22, marker 3; hunk buttons 18; line height 21.
  - Overview ruler 18 (`editorGit.ts:54,1052-1130`).
- **Terminal:** sash 6 (`ProjectTerminalDock.tsx:172-177`).

**Tokens**
- **Ink ramp** (alpha on text colour): 3% fills, 6% chips, 10% borders / secondary buttons, 12% review border, 20% focus border and hover, 35% line numbers, 40% placeholders and icons, 50% verbs, 70% labels, 85% targets (F1–F4).
- **Diff:**
  - Transcript preview: add teal-800 20% + teal-400 bar; delete rose-800 20% + rose-400 bar (`FilePreview.tsx:165-215`).
  - Full diff: emerald-500 15% / rose-500 15% (`UnifiedDiffView.tsx:925`).
  - Editor: #34d399 at 18%, #f87171 at 16% (`editorGit.ts:1052-1130`).
  - For Pocket, keep DIFF_ADD_BG / DIFF_DEL_BG and use RUNNING / FAILED for the 2–3 px bars.
- **Status:**
  - Approval: amber-400 (Pocket WAITING).
  - Busy: accent (Pocket shows Working in RUNNING green; keep Pocket's).
  - Done: emerald-400.
  - Draft: 55% ink (`Sidebar.tsx:2993-3029`).
- **Git letters:** U sky-400, A emerald-400, D red-400, M amber-400 (`GitChangesPanel.tsx:1553-1565`). Pocket already uses `ui::git_color` (`P d/pocket/changes.rs:381-460`).
- **Mentions:** #38bdf8 dark / #0284c7 light. Skills: #e8c547 / #a07c10 (`M css:64-65,107-108`).

**States**
- **Tool:** pending (dashed circle, spinning while live) / accepted (minus, no trailing icon) / rejected (red verb, red X, expandable error) (`transcriptActivity.ts:44-64`; `AgentTranscript.tsx:3242-3262`).
- **Edit awaiting approval:** stays outside the fold, shows the inline diff card and Allow / Deny (`transcriptActivity.ts:148-165`; `AgentTranscript.tsx:3336-3420,3601-3628`).
- **Commit button:** "Commit" / "Amend Commit". Disabled per the §F8 gates; the menu items disable independently (`GitChangesPanel.tsx:380-405,740-800`).
- **Review card:** hidden while busy. Undo disabled when undo is locked or any file is not undoable, with a tooltip explaining why (`SessionReview.tsx:82-157`).
- **Queue:** normal / paused ("Queue paused because you interrupted" + Resume) / editing (`Composer.tsx:357-470`).

**Motion**
- Tool spin 3.6 s linear. Thinking pulse 1.8 s ease-in-out, .35↔.9 (`M css:2502-2522`).
- Word fade 320 ms ease-out, opacity only; reveal ≥90 chars/s, 0.22 s catch-up, 150 ms hold (`wordFade.tsx:14-29`; `M css:2492-2500`).
- Step open: grid-rows over `--step-ms*0.4` (320 ms default) `cubic-bezier(0.22,1,0.36,1)`; rise 10 px `cubic-bezier(0.16,1,0.3,1)` (`M css:2386-2481`).
- Toast in: 180 ms, from −8 px and scale .98 (`M css:1930-1943`).
- Hover diff: open delay 300 ms, close delay 180 ms (`ToolDiffPreview.tsx:67,92-97`).
- Braille spinner 80 ms per frame (`TerminalSpinner.tsx:4,26`).
- Reduced motion turns all of these off (`M css:2528-2540`).

**Shortcuts**
- ⌘↩ commits in the commit box (`GitChangesPanel.tsx:680-700`) and adds a diff comment (`DiffCommentComposer.tsx:86`).
- ↓ from a tool row focuses the hover diff (`ToolDiffPreview.tsx:92-97`).
- QuestionForm: ↑/↓ and Enter/Space (`QuestionForm.tsx:227-238`).
- Terminal: ⌥←/→, ⌘←/→, ⌘⌫ (`terminalKeys.ts:7-39`).
- MonoCode ⌘J hides the terminal (`ProjectTerminalDock.tsx:239`). Pocket's ⌘J (next waiting) stays (`P d/pocket/main.rs:1062-1071`).

**Copy** (verbatim)
- Review: "Changed N files", "Show N more files", "Mixed changes".
- Git:
  - "No uncommitted changes"
  - "Message (⌘↩ to commit)"
  - "Commit & Push"
  - "Commit, Push & Create PR"
  - `Push to default branch "X"?`
  - `Create a pull request from default branch "X"?`
  - `Publish Branch "X"`
  - "Sync Changes"
  - `View PR #N: title`
  - "Generate commit message" / "Cancel commit message generation"
- Hover diff: "Proposed changes", "Attempted changes · tool did not complete", "Written content · previous contents unavailable".
- Diff: "N unmodified lines", "Expand upward", "Expand downward", "Leave a comment…", "⌘↩ to add", "Add to chat".
- Access: "Access changes apply to the next turn. Stop and resend to apply them now."
- Terminal:
  - `"X" is still running in Y. Close this terminal anyway?`
  - "These terminals are still running: … Close them anyway?"
- Worktree: "Also delete associated sessions"; unpushed commits are "not on a remote. They stay on the branch."

## Open questions / risks

- **Checkpoints on the PTY path (13-1).**
  - A PreToolUse hook for edit tools adds latency to every edit; Pocket's hook timeout is 5 s for every event except PermissionRequest, which gets 610 s (`P pocketd/daemon/plugin.go:26-31`).
  - Edits made by the user or another agent in the same worktree during a turn must be refused on Undo. That is MonoCode's "locked" / foreign-edit rule; see 10-14.
- **Pacing (13-10)** only helps streamed deltas. Claude JSONL arrives in whole blocks, so the gain is Codex-only unless a structured Claude driver lands (14-6).
- **`gh` availability.** It may be missing or unauthenticated on the login-shell PATH that pocketd and the desktop use. The PR buttons need a probe and a disabled state.
- **Amend and force-push policy.** MonoCode blocks commit & push after amending a pushed head (`GitChangesPanel.tsx:380-405,588`). Pocket has to decide whether to offer `--force-with-lease` at all.
- **Signing.** Git commit signing may prompt (pinentry-mac) when git runs without a tty. MonoCode only adds a hint (`M tauri/fs.rs:2585-2590`); Pocket's `commit` will fail the same way.
- **⌘J conflict.** MonoCode uses ⌘J to hide the terminal; Pocket uses it for next waiting. Keep Pocket's.
- **Comment targets.** When no session is live, `comment_target` falls back to the project's newest card (`P d/pocket/main.rs:933-938`). Should the comment go into a draft instead, as MonoCode does, rather than being sent?
- **OSC 10/11.** It is unverified that pocketd's ghostty-vt answers these with the right colours; `vt.go` has no colour handling and `replyToQuery` just forwards vt replies (`P pocketd/vt/vt.go`; `P pocketd/terminal/terminal.go:180-189`). This matters for TUIs that pick themes, and for the arcade idea.
- **Screenshot drift.** The screenshot and the code differ (§F13). Treat every visual here as code-derived, not as the shipped look.
- **Notification bodies (13-20)** need the tool title and question text in AgentSummary, which the desktop does not decode today (14-3).

## Verification

Date: 2026-09-30. Claims checked: 22. Corrected: 7.

- Pocket desktop "cannot resolve" citation pointed at event parsing; now cites the outbox (`P d/agents/agents.rs:207-218`).
- Pocket deleted-line labels: a deleted-only selection uses old-side numbers (not the new-side neighbour), unmarked, so it is ambiguous (`P d/pocket/diff.rs:127-133`, test at `:621-625`).
- MonoCode review buttons read "Undo" / "Keep"; "Undo all session changes" is only the tooltip. "Locked" means another session is running in the project (`M f/sessions/ui/SessionReview.tsx:154-172`).
- Pocket already alerts viewed agents when the window is inactive (`sync_view` empties the view set, `P d/pocket/main.rs:389-396`); removed that half of 13-20.
- 13-13: pocketd already tracks and broadcasts the foreground command, and the desktop exposes it as `busy()` (`P pocketd/terminal/terminal.go:233-249`, `P d/pocket/sessions.rs:14-17`); prerequisite dropped, effort M → S.
- Pocket ahead/behind: vs `@{u}` on main/master, vs local main/master elsewhere (`P d/git/git.rs:76-84`).
- Pocket `commit` uses `--no-edit` for a message-less amend; OSC note re-cited to `vt.go` (still unverified).

