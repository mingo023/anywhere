# 06 — Zeron: git, diff, history, files, terminal, browser

Date: 2026-09-30.

Sources:
- Zeron: zeronsh/comet@ed3b1aae4a5189eef67143db7b8c5c3ee7a933c5 (MIT), cloned at `/Users/mingo/tmp/orchestrators/zeron`. https://github.com/zeronsh/comet
- Pocket: coding-pocket `main`@b9d14a1. This research worktree is at 86deb13, which has no `changes.rs`, so every Pocket line number refers to `main`.

Citation legend:
- `Z path:L` is a file and line in the Zeron clone. `P path:L` is a file and line in Pocket `main`@b9d14a1.
- `M` refers to another clone in this series. This report does not cite it.
- A URL is an external primary source.
- When code and docs disagree, the code wins, and the conflict is noted in place.
- Screenshots were read as images and are cited by their path.

## TL;DR

- **Diff engine.** Zeron shells out to `git` (never libgit2) for checksum snapshots. Each snapshot combines `name-status`, `numstat`, a `--unified=3` patch, and untracked files. A `notify` watch with a 500 ms debounce drives updates, with a 120 s repair tick. An unchanged sha256 publishes nothing. Patches are capped at 3 MiB; over that, the UI shows a "Partial snapshot" pill (Z crates/engine/src/diff_sync.rs:58-71, Z crates/engine/src/diff_sync.rs:1204).
- **Five diff scopes:** Working tree, Branch changes (vs `merge-base(base, HEAD)`), Latest turn, History, and Commit (Z crates/ui/src/changes.rs:871-916).
  - Latest turn diffs a temp-index `write-tree` snapshot, taken at turn start, against the current tree (Z crates/engine/src/diff_sync.rs:1731, Z crates/engine/src/diff_sync.rs:1778).
  - This is the best idea to clone. pocketd already sees `UserPromptSubmit`→working and `Stop`→done (P packages/pocketd/internal/daemon/claude_test.go:167-173).
- **Diff view.** All files render in one virtualized stream (Z crates/ui/src/changes.rs:66-111):
  - Sizes: sticky file headers 38 px with 16 px backdrop blur; hunk headers 28 px; lines 21 px; gutters 36 px per column.
  - Unified/split and wrap toggles are persisted. Per-file and fold-all collapse tween over 180 ms.
- **Write operations.** Zeron has no per-file or per-hunk stage, discard, revert, commit or push. The only write is a whole-working-tree discard, which refuses to run if the tree's checksum changed since the dialog opened (Z crates/engine/src/diff_sync.rs:1531-1547).
  - Pocket is already ahead here: per-path stage and discard, Commit / Commit & Push / Amend, an AI-written commit message, and click-to-unfold context (P packages/desktop/crates/git/src/git.rs:188-218, P packages/desktop/crates/pocket/src/changes.rs:186-267).
- **Review comments.** Comments are staged, then folded into the next prompt as one trailing block. The block has a header plus `- path:line (L|R): body` bullets; the chip reads "N comments", and the transcript parses the block back into a badge (Z crates/ui/src/comments.rs:121-160).
  - Pocket instead sends each comment immediately as a one-line prompt (P packages/desktop/crates/pocket/src/main.rs:919-931).
- **History.** A `--topo-order` lane graph with bezier edges, 6 hues at 0.72 saturation, and hover focus that dims everything else to 0.24 (Z crates/ui/src/history.rs:36-69, Z crates/ui/src/history.rs:638, Z crates/ui/src/history.rs:3324).
  - It also has a compact rail, folding of linear branch runs, 100-row pages, fuzzy search, and ahead/behind vs upstream.
  - Clicking a row opens that commit's diff.
- **PRs.** A read-only status badge comes from `gh pr list --head <branch>`, polled on a 120 s TTL (45 s when no PR exists) with 20 s→15 min failure backoff, and only while something is subscribed. There is no PR creation and no push (Z crates/engine/src/change_requests.rs:23-27, Z crates/engine/src/source_control.rs:198-260).
- **Files.** A virtualized tree (27 px rows, 14 px indent guides, arrow-key navigation) with git decorations. Text files up to 1 MiB are editable, saved with a hash-guarded atomic replace and conflict banners. Markdown and images get previews, and comments can be left on file lines (Z crates/ui/src/files/tree.rs:19-22, Z crates/engine/src/workspace_files.rs:37-41).
- **Browser.** A `WKWebView` (via wry) sits under a GPUI overlay plane. That plane required patching their own gpui fork (Z docs/research/diri-sidebar-browser-plan.md:24-27).
  - Dev servers are discovered automatically: `lsof`/`ps` map each process's working directory to a project, and a HEAD probe runs every 2 s.
  - Each server gets a stable proxy hostname, `<device>.<project>.localhost:7331`; remote devices connect over WebRTC (Z docs/preview-networking.md:3-21, Z crates/proto/src/preview.rs:4).
- **Worktrees.** Created at `~/.zeron/worktrees/<repo>/<adj-noun>` on a `zeron/<name>` branch. The host creates it while draining the queued run, then renames the branch after the chat title.
  - A setup Action runs with `ZERON_PROJECT_ROOT` and `ZERON_WORKTREE_PATH` set. Delete removes the branch only if Zeron created it (Z crates/engine/src/repos.rs:1092-1265, Z crates/proto/src/agent.rs:271-287).
  - Pocket already has worktrees, setup and env copying.

## Findings

### 1. Diff engine (Z crates/engine/src/diff_sync.rs)

- **Module and limits.**
  - Doc: Z crates/engine/src/diff_sync.rs:1-34.
  - Constants: `MAX_PATCH_BYTES` 3 MiB (Z crates/engine/src/diff_sync.rs:58), `WATCH_DEBOUNCE` 500 ms (:61), `REPAIR_INTERVAL` 120 s (:63), `MAX_WATCH_DIRS` 8,000 (:69).
  - Live watching is skipped above 8,000 dirs; the repair tick still runs.
- **Capture commands** (Z crates/engine/src/diff_sync.rs:1204):
  - `git diff --name-status -z --find-renames`
  - `--numstat`
  - `--no-ext-diff --no-color --find-renames --unified=3`
  - `--no-optional-locks status --porcelain=v1 -z --untracked-files=all`, from which untracked files are synthesized as additions.
- **Checksum.** Each snapshot carries a sha256; an unchanged checksum publishes nothing. The UI's fetch key includes the watch checksum, so any tree change triggers a recapture (Z crates/ui/src/changes.rs:2060).
- **Checkout identity** = `sha256(deviceId ‖ NUL ‖ canonical git dir)` (Z crates/engine/src/repos.rs:314). Diffs group by checkout, not by chat.
- **Latest turn.**
  - `note_turn_start` runs `snapshot_tree` in a background task when a turn is dispatched (Z crates/engine/src/diff_sync.rs:286-316, called from Z crates/engine/src/sessions.rs:485, Z crates/engine/src/sessions.rs:741).
  - `snapshot_tree` uses a temp `GIT_INDEX_FILE`, runs `add -A --ignore-errors`, then `write-tree` (Z crates/engine/src/diff_sync.rs:1731).
  - `capture_turn_diff` diffs that tree against a fresh snapshot (Z crates/engine/src/diff_sync.rs:1778).
  - Snapshots live only in memory ("since boot") (Z crates/engine/src/diff_sync.rs:318-320).
- **Commit scope:** first parent vs commit; a root commit diffs against the empty tree `4b825dc…` (Z crates/engine/src/diff_sync.rs:1625).
- **Branch scope:** `merge_base(root, base_ref)` vs the working tree (Z crates/engine/src/diff_sync.rs:1716).
- **Full-file sources.**
  - Fetched lazily via `GetCheckoutFileDiffText`, only for syntax highlighting, capped at 2 MiB each (Z crates/engine/src/diff_sync.rs:58-71, Z crates/rpc/src/lib.rs:140-195).
  - A stale checksum or any visible-line mismatch throws the whole highlight result away (Z docs/syntax-highlighting.md:5).
- **Context expansion.** None found in code. The `changes-expand-buttons` media shows the pane-expand (takeover) button keeping its position, not context expansion (Z docs/media/changes-expand-buttons/before-after.png).
  - Pocket has click-to-unfold (P packages/desktop/crates/pocket/src/diff.rs:314-320).

### 2. Scopes and base selection (Z crates/ui/src/changes.rs)

- **Labels:** "Working tree", "Branch changes", "Latest turn", "History", "Commit". Wire modes: `workingTree|branch|turn|history|commit`. `ALL` = the first three (Z crates/ui/src/changes.rs:871-916).
- **Headers:**
  - "{n} Changed files vs {base}"
  - "{n} Changed files this turn"
  - "{n} Changed files in this commit"
  - Working tree: "N Uncommitted change(s)"
  - Source: Z crates/ui/src/changes.rs:918-934.
- **Default base:** the first branch (the repo default) unless it is the current branch; otherwise `main`, then `master` (Z crates/ui/src/changes.rs:936).
- **Clean states:** "No uncommitted changes", "No changes vs {base}", "No branch changes", "No changes this turn", "No commits found", "Empty commit" (Z crates/ui/src/changes.rs:954).
- **Scoped notices:**
  - "No turn recorded yet — send a message first" (Z crates/ui/src/changes.rs:4989)
  - "This chat's device is running an older Zeron — update it to view branch and turn diffs"
- **Ref selector** `{branch} → {base ⌄}` (Z crates/ui/src/changes.rs:4071):
  - Type: mono 11.5 px; the branch is dim; `ARROW_RIGHT` icon 12 px, faint.
  - Base trigger: px6, gap4, radius 6, wash 0→0.12.
  - `flex_shrink` is weighted by length², so the shorter side survives truncation (confirmed in Z docs/media/changes-expand-buttons/sending-and-refs.png).
- **Ref menu:** w240, list max-height 240 with scroll, filter input, ↑↓/⏎ keys. Empty copy: "No branches" / "No matching branches".

### 3. Diff view UI (Z crates/ui/src/changes.rs)

- **Rows.** `DiffRow` = FileHeader | Notice | HunkHeader | Line | SplitLine | CommentCard | CommentDraft | BodyPad | FoldingBody (Z crates/ui/src/changes.rs:1261).
  - Rendered as a gpui `list()`; a collapsed file's body rows are removed from the list.
- **Sticky header.** Blur 16 px. Tint alpha is 0.40 in dark, 0.85 in light. The next header pushes it up by `min(next_y − 38, 0)` (Z crates/ui/src/changes.rs:66-72, Z crates/ui/src/changes.rs:1467-1495).
- **Fold tween.** A single analytic-height stand-in row. Window 400 ms, capped at 2,400 px (Z crates/ui/src/changes.rs:1564-1570).
- **Split pairing.** Within a change block, deletions pair with additions index-for-index; leftovers stay one-sided (Z crates/ui/src/changes.rs:681-800).
- **File notices:**
  - "New file", "Deleted file", "Renamed from {from}", "Binary file — contents not shown"
  - "Diff truncated — showing first {max} of {total} lines"
  - Source: Z crates/ui/src/changes.rs:594.
- **Highlighting.** Parses old and new hunk excerpts first, then upgrades to full sources. Up to 200k source lines. States: Pending, Ready, Excerpt, Plain. Deleted lines use the old document; context and added lines use the new one (Z docs/syntax-highlighting.md:5).
- **Header controls** (Z crates/ui/src/changes.rs:3800):
  - Scope trigger: h24, px8, gap6, wash 0.05→0.14, 12 px label.
  - Menu: w180, anchored 10 px below.
  - Trailing buttons: Discard (WorkingTree scope only), Split, Wrap, Fold all.
  - The History scope adds search, fetch, view-mode and refresh buttons.
- **Watch stream.** Retries every 2 s. Banners: "Diff stream interrupted — retrying" and "Diff watch unavailable: {err}" (Z crates/ui/src/changes.rs:1862).

### 4. Stage / discard / revert

- Zeron has no per-file or per-hunk stage, unstage or revert. `rg` found no commit, push or PR-creation UI; `gh pr create` appears only in a demo transcript (Z crates/client/src/demo/transcripts.rs:198), and `git push` only in tests and a harness fixture.
- **Whole-tree discard** (Z crates/ui/src/changes.rs:1951, Z crates/engine/src/diff_sync.rs:1531):
  - `DiscardWorkingTreeRequest {chat_id, checkout_id, expected_checksum, target_device_id, file_count}`.
  - Disabled when the snapshot is truncated or empty.
  - The engine refuses when:
    - there is no first commit;
    - the snapshot is partial;
    - the checksum mismatches: "working tree changed since the confirmation was opened" (Z crates/engine/src/diff_sync.rs:1547);
    - an untracked dir, nested repo or submodule is involved.
  - It runs `git restore --source=HEAD --staged --worktree` plus `git clean -fd` on exact paths, removes empty parents, then verifies the tree is clean.
- **Dialog** (Z crates/ui/src/shell.rs:9630-9690):
  - Title "Discard working tree changes?"
  - Body "Discard all uncommitted changes in this working tree? This can’t be undone."
  - Buttons "Cancel" and "Discard changes" (danger).
  - Failure: "Couldn’t discard changes" with a "Close" button.
- **Pocket today:**
  - `set_staged` runs `add -A` / `reset -q` per path; `discard` runs `clean -fq` on untracked and `checkout -q` on tracked paths (P packages/desktop/crates/git/src/git.rs:188-206).
  - `ask_discard` opens a confirm overlay (P packages/desktop/crates/pocket/src/changes.rs:516-531), rendered in P packages/desktop/crates/pocket/src/overlay.rs:282-289.
  - The confirm carries no staleness guard.

### 5. Review comments to the agent

- **Model.** `ReviewComment{id, path, line, body, source}`, where `source` is Diff{side L/R, old_path} or File. The citation uses `old_path` for the Old side (Z crates/ui/src/comments.rs:1-120).
- **Prompt text** (Z crates/ui/src/comments.rs:121-156):
  - Diff comments get the header "Comments on the diff (each cites the file and line it belongs to; L = line number in the original file, R = in the changed file):".
  - If any comment is on a file (not a diff), the header is "Review comments (each cites the workspace file and line it belongs to):".
  - With an empty prompt, the body becomes "Address the review comments below."
  - Multi-line bodies are indented 2 spaces.
- **Storage.** Kept per composer key in `AppState.review_comments` (Z crates/ui/src/state.rs:759-1001).
- **Send path** (Z crates/ui/src/composer.rs):
  - At send: `take_review_comments` then `with_comments` (:7708).
  - Send is blocked while a flush is pending (:7491).
  - Comments are restored on failure (:8361).
  - Chip: CHAT_ROUND_LINE icon + "N comments" (:5918-5948).
- **Transcript.** `extract_badge` matches only a whole trailing block and renders it as a chip. Labels: "1 comment" / "N comments" (Z crates/ui/src/comments.rs:161, Z crates/ui/src/comments.rs:249).
- **Adding.** Hovering a line shows a 16 px `+` in the gutter. In split mode only the right (new) column offers it. One draft exists at a time; a second `+` moves it (Z crates/ui/src/changes.rs:3192, Z crates/ui/src/changes.rs:4742-4759).
- **File-line comments.** Also available in the Files editor. Placeholders: "Request a change…" / "Add a comment…". Card 320 px wide, draft 92 px tall (Z crates/ui/src/files/preview.rs:44-47, Z crates/ui/src/files/preview.rs:934-936).
- **Pocket today** (P packages/desktop/crates/pocket/src/main.rs:919-938):
  - Each submit calls `daemon.send({"op":"prompt","text":"{path} {lines}: {text}"})` right away.
  - The target is the picked session, else the open one, else the newest.
  - The L/R side is stored (`old_side`) but never sent (P packages/desktop/crates/pocket/src/main.rs:86-93).
  - pocketd `Prompt` writes the text, then `\r` after 150 ms (P packages/pocketd/internal/terminal/terminal.go:276-287).

### 6. History graph (Z crates/ui/src/history.rs)

- **Data** (Z crates/engine/src/repos.rs:560-660):
  - `git log --topo-order --skip N --max-count=limit+1 --format=%H%x00%P%x00%s%x00%an%x00%ae%x00%aI%x00 HEAD --branches --remotes --tags`
  - Branch tips come from `--no-walk=sorted`; counts from `rev-list --count`.
- **Comparison target**, in order: upstream/origin HEAD, then upstream/main, origin/main, upstream/master, origin/master, then the tracking ref (Z crates/engine/src/repos.rs:674).
- **FetchAll** runs `git fetch --all --quiet` (Z crates/engine/src/repos.rs:300).
- **Layout.**
  - Active lanes target the parent sha. The first parent keeps the lane and its color; extra parents get new lanes (Z crates/ui/src/history.rs:638).
  - `collapse_branch_runs` hides linear commits of collapsed branches but keeps refs, junctions and tips (Z crates/ui/src/history.rs:760).
- **Paint.**
  - A canvas draws cubic beziers with control points at middle×0.55 / ×1.45.
  - Palette: accent, busy, success, warning, danger, text_muted × saturation 0.72.
  - Focus is painted in two passes (Z crates/ui/src/history.rs:3324).
- **Constants** (Z crates/ui/src/history.rs:36-69):
  - Rows and pages: page 100, row 36.
  - Lanes and nodes: lane 12, node radius 3, HEAD ring +2, stroke 1.5, focused stroke 2.25, hit radius 5.5.
  - Focus dimming: unfocused graph 0.24, unfocused row 0.6.
  - Width and compact mode: graph max width 0.34 of the pane. Compact mode enters/exits at subject width 160/184 and lane spacing 4/5 (hysteresis).
  - Refs and search: ref area 0.45, ref badge max 112, search width 196 with 70 ms debounce and 1,500 ms idle dismiss, comparison pill hidden below 260 px.
- **Rows** (Z crates/ui/src/history.rs:3923):
  - h36, 11 px meta, 12 px subject ("(no subject)" when empty), hover ink 0.025.
  - Click emits `OpenCommit`, which opens a Commit-scope diff.
  - "Load more" / "Loading…" / "Retry" button: h28, px11, radius 7.
  - Rows entering or leaving animate height over COLLAPSE, with opacity 0.35→1.
- **Refs and columns.** Branch = accent, Remote = busy, Tag = warning. Date format "%b %-d, %Y". Author / Date / SHA columns can be toggled, reordered and resized, and are persisted. View modes: AllCommits / BranchTips (Z crates/ui/src/history.rs:200).
- **Pocket today:** only `log --format=%h\t%s -n 20 base..HEAD` (P packages/desktop/crates/git/src/git.rs:85).

### 7. Change requests (PRs)

- **Engine** (Z crates/engine/src/source_control.rs:198-260):
  - `gh pr list --head <selector> --state all --limit 20 --json number,title,url,state,baseRefName,headRefName,updatedAt,isCrossRepository,headRepositoryOwner`
  - Runs with `GH_PROMPT_DISABLED=1` and a 20 s timeout.
  - Default branch via `gh repo view … --json defaultBranchRef`.
- **Polling** (Z crates/engine/src/change_requests.rs:23-27):
  - TTL 120 s when a PR exists, 45 s when none does.
  - Failure backoff 20 s→15 min; context poll every 15 s.
  - Stops when there are no subscribers.
- **Badge** (Z crates/ui/src/change_requests.rs):
  - Tones: Open = success, Merged = code_text, Closed = danger.
  - Sidebar size: h16, px4, radius 4, 10 px text. Composer size: h20, px7, radius 6, 11 px text.
  - Background tone at 0.08, 0.16 on hover. PULL_REQUEST icon, mono number.
  - Click opens the URL. Tooltip: "PR #{n} · {state}" plus the title, max width 320.
  - Placed in the sidebar (Z crates/ui/src/shell.rs:7415-7502), the picker (Z crates/ui/src/pickers.rs:3130) and the Files sections (Z crates/ui/src/files/sections.rs:837).
- **Pocket today:** no `gh` usage anywhere (rg over packages/desktop, packages/pocketd, packages/protocol).

### 8. Files: tree, editor, preview

- **Tree.** Virtualized `list`. `TREE_ROW_HEIGHT` 27, `TREE_INDENT` 14, fade band 24 (Z crates/ui/src/files/tree.rs:19-22).
  - Indent guides are 1 px `theme.border` at `8 + 7 + level×14` (Z crates/ui/src/files/tree.rs:25-48).
  - Keys: ↑↓ move; ← collapses or goes to the parent; → expands or goes to the first child; ⏎/Space activates (Z crates/ui/src/files/tree.rs:439-480).
- **Git decorations.** Untracked/Added = success, Modified = warning, Renamed = accent, Deleted/Conflict = danger (Z crates/ui/src/files/git_status.rs:16-39).
  - Metadata-only subscription with leases; the last lease dropping cancels the RPC (Z crates/ui/src/files/git_status.rs:1-2).
- **Engine limits** (Z crates/engine/src/workspace_files.rs:31-44):
  - Directory page 500, max 50k entries, search results 200.
  - Editable ≤1 MiB, preview ≤8 MiB.
  - Watch debounce 100 ms, max burst 1 s, repair 120 s.
- **Save.**
  - Writes are optimistic: the request carries `expected_content_hash` and the file is swapped in with an atomic temp-file rename (Z crates/engine/src/workspace_files.rs:1748-1863).
  - Conflict reasons: Deleted, Replaced, Changed, NotRegularFile.
- **Editor document phases:** Loading, Ready, Saving, SaveFailed, ExternallyModified, Conflict, DeletedOnDisk, ReadOnly, Error (Z crates/ui/src/files/document.rs:20-32).
- **Editor copy** (Z crates/ui/src/files/preview.rs:2135-2334):
  - Banners: "Save conflict" / "The file changed on disk. Your editor buffer was preserved."; "Deleted on disk"; "Changed on disk" / "Review it before saving."
  - Actions: "Keep Editing", "Reload from Disk", "Discard & Reload".
- **Read-only reasons:** "Binary files cannot be previewed.", "Symlink targets are read-only.", "Files with mixed line endings are read-only.", "Read-only: outside this chat's folder." and others (Z crates/ui/src/files/preview.rs:3312-3331).
- **Sizes.** Editor text 13 px, preview text 11.5 px, line height 20. Retains 16 docs / 32 MiB (Z crates/ui/src/files/preview.rs:33-52).
- **Images** (Z docs/image-preview.md:3-22):
  - Formats: PNG, JPEG, GIF, WebP, SVG, BMP, TIFF.
  - Initial fit never upscales; zoom 1%–3200%; pinch or Ctrl+wheel zooms.
  - Transfer: ≤8 MiB in 384 KiB chunks. Decode: ≤4096 px per side and ≤64 MiB. SVG is sanitized.
- **Markdown preview:** ≤2 MiB, content width ≤900 (Z crates/ui/src/files/markdown_preview.rs:22-26).
- **Explorer footer:** Subagents and Chats sections, 29 px rows, 10 rows then "Show N more" (Z crates/ui/src/files/sections.rs:1-60).
- **Pocket today** (P packages/desktop/crates/pocket/src/explore.rs):
  - The explorer is a recursive, non-virtualized `div` list with no keyboard navigation (:200-270).
  - A "Touched by agents · N" chip (:212-221) is better agent context than anything in Zeron.
  - Read-only Editor; previews ≤512 KB (:21-23).

### 9. Terminal

- **Agents are not in these PTYs.** Zeron drives agents through harness adapters (ClaudeCode, Codex, ACP agents) (Z crates/proto/src/agent.rs:7-15, Z crates/proto/src/agent.rs:295). Its terminals are side login shells in the chat's working directory (Z crates/engine/src/terminals.rs:1-13).
- **Engine** (Z crates/engine/src/terminals.rs:34-38):
  - Up to 32 PTYs; input frames ≤64 KiB.
  - 1 MiB replay, resumable via `afterSeq`; output batched every 12 ms (Z crates/doc/src/constants.rs:23).
  - Exited sessions expire after 30 min. Detaching does not close a shell.
- **UI** (Z crates/ui/src/terminal/panel.rs:1-56, Z crates/ui/src/terminal/view.rs:25-36, Z crates/ui/src/terminal/emulator.rs:44):
  - Tabs are per chat.
  - Tab bar: 118 px tabs, 40 px bar; drag-reorder slides over 150 ms; middle-click closes.
  - Cmd+J (Ctrl+J elsewhere) toggles the panel.
  - Font 13/18, padding 12. Keystrokes coalesce for 12 ms; resize debounces 80 ms. Scrollback 10k lines.
  - Height 160 px to 55vh (Z crates/ui/src/settings.rs:53-54).
- **Relevance.** Pocket's model is terminal-first, so little to copy. Nothing found for "send terminal selection to chat" (rg returned no hits).

### 10. Browser and dev-server preview

- **macOS host.** wry `WKWebView` as an AppKit child. A `nonPersistentDataStore` is shared per profile, and preview hostnames are routed via a per-domain CONNECT proxy config (Z crates/ui/src/browser/macos.rs:1-60).
  - GPUI draws menus and tooltips on a transparent Metal layer above the page, which the Zeron team added to their gpui fork ("Zui") (Z docs/research/diri-sidebar-browser-plan.md:24-33).
- **Linux: code vs docs.**
  - The code renders WebKitGTK offscreen in a helper process and composites the frames in GPUI (Z crates/ui/src/browser/linux/mod.rs:1-2). The empty-state copy says "Embedded browsing is available on macOS and Linux." (Z crates/ui/src/browser/view.rs:299).
  - The plan says Linux is external-only (Z docs/research/diri-sidebar-browser-plan.md:41). **Code wins.**
  - The Linux screenshots show the older external fallback (Z docs/screenshots/browser/README.md).
- **URL policy.** http(s) only. Bare loopback defaults to `http`, other hosts to `https`. Credentials are rejected; there is no page-to-engine bridge (Z crates/ui/src/browser/model.rs:62-137).
- **Tab label:** title, else host, else "Browser".
- **Shortcuts:** mod-L focus address, mod-T new tab, mod-W close, mod-[ back, mod-] forward. Reload is configurable (default mod-shift-R, Z crates/ui/src/settings.rs:1102). Every browser chord is skipped if an existing app chord already uses it (Z crates/ui/src/browser/mod.rs:23-60).
- **Discovery** (Z docs/preview-networking.md:13-39, Z crates/preview/src/discovery.rs:32-35, :87, Z crates/preview/src/service.rs:199):
  - `lsof`/`ps` on macOS, `/proc` on Linux.
  - Only loopback listeners whose working directory is inside a known project; the deepest project wins.
  - A HEAD probe runs every 2 s with an 800 ms timeout.
  - Framework labels: Vite, Next.js, Astro, Miniflare; "Node HTTP server" for node/bun/deno; otherwise "HTTP server" (Z crates/preview/src/discovery.rs:30-54).
- **Proxy and hostnames.**
  - Stable hostnames `http://<device>.<project>.localhost:7331`; extra services get `-api`-style suffixes (Z docs/preview-networking.md:3-6, Z crates/proto/src/preview.rs:4).
  - The proxy keeps Host/Origin, rewrites localhost redirects, and passes WebSocket upgrades through (Vite HMR works) (Z docs/preview-networking.md:64-68).
  - Remote devices connect over a WebRTC DataChannel. The mux frames 8 KiB chunks with a 64 KiB window and ≤64 streams (Z crates/preview/src/mux.rs:20-31).
- **Empty state** (Z crates/ui/src/browser/view.rs:100-300; Z docs/screenshots/browser/macos-empty-dark.png, macos-error-light.png, macos-preview-dark.png, macos-menu-dark.png):
  - A service list titled "Running locally" or "Running on your device". Rows are h56, px14, radius 10, with an "Open" button (h28).
  - While searching: "Looking for dev servers…". With nothing found: "Start a dev server in this project. It will appear here automatically, ready to open."
  - Always offers "Or enter a website address".

### 11. Worktree per session

- **Checkout modes.** `CheckoutKind {Local, NewWorktree}`. "Current worktree" is Local when the picked ref already has a worktree (Z crates/proto/src/view.rs:540-600).
  - Labels: "Current checkout", "Current worktree", "New worktree".
  - Ref rows carry right-aligned muted `current`/`worktree` tags (Z crates/ui/src/pickers.rs:3375-3436).
- **Creation.** `WorktreeSpec{repo_path, base, space_id}` rides the queued Run, and the host creates the worktree while draining the queue, so a lost relay frame can't wedge "Sending…" (Z crates/proto/src/agent.rs:271-287).
- **Path and branch.** `~/.zeron/worktrees/<repo>/<adjective-noun>`, overridable with `ZERON_WORKTREES_DIR`, on a fresh `zeron/<name>` branch. Up to 50 name attempts (Z crates/engine/src/repos.rs:1-11, Z crates/engine/src/repos.rs:1092-1150).
- **Rename after title** (Z crates/engine/src/repos.rs:1171-1217, Z crates/engine/src/repos.rs:2013-2028, called from Z crates/engine/src/titles.rs:120-127):
  - Slug of the chat title, ≤48 chars, "update" as fallback.
  - On collision, a 6-hex suffix derived from the path hash.
  - Only runs while the worktree is still on the original `zeron/<folder>` branch.
- **Delete.** `worktree remove --force`, falling back to `rm -rf`, then `worktree prune`. `branch -D` only if the branch starts with `zeron/` (Z crates/engine/src/repos.rs:1219-1265).
- **Setup Action.** Marked by the `runOnWorktreeCreate` flag and importable from `zeron.json` (Z crates/proto/src/entities.rs:1038, Z crates/engine/src/project_actions.rs:27).
  - Runs in a visible terminal with `ZERON_PROJECT_ROOT` and `ZERON_WORKTREE_PATH` set (Z crates/engine/src/project_actions.rs:307-341).
  - Its outcome returns in the `CreateWorktreeOutcome{setup_action, setup_error}` reply (Z crates/engine/src/rpc.rs:3035-3103).
- **Pocket today:**
  - Path `~/.worktrees/<repo>/<name>` (per-repo override); name taken from the prompt or typed (P packages/desktop/crates/pocket/src/forms.rs:278-281, P packages/desktop/crates/pocket/src/forms.rs:376-379).
  - Copies `.env*` or a listed set of files, and runs a setup op (P packages/desktop/crates/pocket/src/forms.rs:284-291, P packages/desktop/crates/pocket/src/forms.rs:418-450).
  - `remove_worktree` keeps the branch (P packages/desktop/crates/git/src/git.rs:173-177).

### 12. Right pane shell

- **Surfaces.** `RightSurface {Picker, File, Browser, Diff, Terminal, Subagent, SideChat}`. Panels are per chat, held in memory only, and closed by default (Z crates/ui/src/shell.rs:684).
- **Picker cards** (Z crates/ui/src/shell.rs:10631-10700):
  - Cards: h44, px14, radius 10, bordered, ink 0.02 background → 0.05 with a strong border on hover. Icon 15 px, title 13 px medium.
  - Column: max width 280, gap 8.
  - Options: Browser and Terminal, plus Diffs and History when git is detected.
- **Tab strip** (Z crates/ui/src/shell.rs:10802-10849):
  - Chips: 112 px wide on a 116 px slot, h24, radius 6.
  - On hover the icon becomes ✕; it shows a spinner while loading or running.
  - 36 px edge fade; drag to reorder.
- **Takeover:** the pane can take over the chat column (Z crates/ui/src/shell.rs:671).

### 13. Pocket vs Zeron

| Capability | Zeron | Pocket (main@b9d14a1) |
|---|---|---|
| Diff source | git patch snapshots, fs watch, checksum no-op (Z crates/engine/src/diff_sync.rs:1204) | `git::read` on every project cwd plus imara-diff of HEAD vs file, polled every 2 s (P packages/desktop/crates/git/src/git.rs:60-104, P docs/plans/2026-09-28-zed-explore-diff.md:20) |
| Scopes | Working tree, Branch, Latest turn, Commit, History | HEAD vs working tree only; `base` used only for ahead/behind (P packages/desktop/crates/git/src/git.rs:75-89) |
| Files per view | All files, sticky headers | One file (`diff_file`) (P packages/desktop/crates/pocket/src/diff.rs:226-284) |
| Word diff / unfold | No / No | Yes / Yes (P packages/desktop/crates/git/src/git.rs:243-300) |
| Stage / discard / commit / push | Whole-tree discard only | Per path + Commit / Amend / Push + AI message (P packages/desktop/crates/pocket/src/changes.rs:14, :186-267, :597-615) |
| Comments | Batched into the next prompt, L/R tags, header | Sent immediately, one line each, with Resolve (P packages/desktop/crates/pocket/src/diff.rs:459-504) |
| History graph | Yes | No |
| PR badge | Yes (gh) | No |
| Files tree | Virtualized, keyboard, editable | Non-virtualized, read-only, "Touched by agents" |
| Browser / preview | Embedded, discovery, proxy, P2P | None |
| Worktree | Host-created on queued run, title rename, zeron-only branch delete | Created at session start, prompt-named, setup + env copy |

## Ideas to clone into Pocket

| ID | Idea | User value | Evidence | Pocket mapping | Effort | Prerequisites |
|---|---|---|---|---|---|---|
| 06-1 | "Latest turn" diff: snapshot the tree with a temp index + `write-tree` on UserPromptSubmit; diff vs now | Review exactly what the agent changed this turn, even in a dirty tree | Z crates/engine/src/diff_sync.rs:286-316, :1731, :1778 | pocketd `internal/daemon` hook handler (new, Go); protocol event carrying the tree sha (new); desktop `git` crate `diff_trees` (new); `diff.rs` scope switch (adapt) | M | Codex turn start (pocketd notes a codex Enter, P packages/pocketd/internal/daemon/claude.go:26-29); take the claude snapshot synchronously inside the hook |
| 06-2 | Batch review comments: stage N, send one prompt with a header and `path:line (L\|R): body` bullets; "N comments" chip | One agent turn per review instead of N; the agent gets side-aware line numbers | Z crates/ui/src/comments.rs:121-156; Z crates/ui/src/composer.rs:7708 | desktop `pocket/src/main.rs` `submit_comment` + `diff.rs` composer (adapt); pocketd `Prompt` unchanged | S | Verify a multi-line burst through `Prompt` stays one message in claude and codex (P packages/pocketd/internal/terminal/terminal.go:276-287) |
| 06-3 | Branch-changes scope with a `{branch} → {base ⌄}` ref picker (length²-weighted truncation) | See the whole PR-to-be for a worktree session | Z crates/ui/src/changes.rs:936, :4071; Z crates/engine/src/diff_sync.rs:1716 | desktop `git` crate `merge_base` + diff vs base (new); `changes.rs` header (adapt) | M | 06-4 helps |
| 06-4 | Multi-file continuous diff: one virtualized list, 38 px sticky blurred headers, per-file fold and fold-all | Scan a whole change set without clicking file by file | Z crates/ui/src/changes.rs:66-111, :1261, :1467-1495 | desktop `pocket/src/diff.rs` rows across files (adapt); `git` crate batch texts (adapt) | L | Keep the `ListState::splice` scroll-preserve rule (P docs/plans/2026-09-28-zed-explore-diff.md:64-70) |
| 06-5 | Watch-driven refresh: fs events, 500 ms debounce, sha256 no-op, 120 s repair; skip watching above 8k dirs | Less CPU than a 2 s poll across every cwd; instant updates | Z crates/engine/src/diff_sync.rs:58-71 | desktop `pocket/src/main.rs` `refresh_git` (adapt), or pocketd watcher (new) | M | Pick a watcher crate |
| 06-6 | Snapshot caps plus notices: "Partial snapshot" pill; "Diff truncated — showing first N of M lines" | No hangs on huge generated diffs; honest UI | Z crates/engine/src/diff_sync.rs:58; Z crates/ui/src/changes.rs:594, :4321 | desktop `git` crate + `diff.rs` (adapt) | S | — |
| 06-7 | Staleness-guarded discard: confirm carries a status hash; refuse "working tree changed since the confirmation was opened" | Never discard edits an agent made while the dialog was open | Z crates/engine/src/diff_sync.rs:1531-1547 | desktop `git::discard` + `changes.rs` `ask_discard` (adapt) | S | — |
| 06-8 | PR status badge per worktree via `gh pr list --head`; TTL 120 s/45 s, backoff 20 s→15 min, poll only while visible | See Open/Merged/Closed without leaving Pocket; know when to delete a worktree | Z crates/engine/src/source_control.rs:198-260; Z crates/engine/src/change_requests.rs:23-27; Z crates/ui/src/change_requests.rs | desktop `git` crate `gh` module (new); projects sidebar worktree row (adapt) | M | `gh` installed and authenticated; silent when missing |
| 06-9 | Dev-server discovery list: `lsof` listeners whose working directory is under a Project/Worktree, 2 s HEAD probe, "Open" in the default browser | One click to the right localhost port for each worktree; phone can see what's running | Z docs/preview-networking.md:13-26; Z crates/preview/src/discovery.rs:32-35 | pocketd discovery (new, Go) + protocol + desktop list (new) | M | — |
| 06-10 | Embedded browser surface (WKWebView under GPUI) | Preview beside the terminal | Z crates/ui/src/browser/macos.rs; Z docs/research/diri-sidebar-browser-plan.md:24-33 | desktop new crate (new) | XL | gpui-kit 0.6.6 support for native child views and overlays: unknown |
| 06-11 | History lane graph; click a commit to open its diff | Understand agent commits across worktrees | Z crates/ui/src/history.rs:36-69, :638, :760, :3324; Z crates/engine/src/repos.rs:560-660 | desktop `git` crate log parser (new); `pocket/src/history.rs` (new, port; MIT) | L | Canvas path API in gpui-kit |
| 06-12 | Virtualized keyboard file tree: 27 px rows, 14 px indent guides, ←→ expand/collapse, git colors | Big repos stay smooth; keyboard users | Z crates/ui/src/files/tree.rs:19-48, :439-480; Z crates/ui/src/files/git_status.rs:30-39 | desktop `pocket/src/explore.rs` `tree` (adapt to `list`) | M | — |
| 06-13 | Comments on file lines in Explore, folded into the same batch under the "Review comments…" header | Ask for changes to code the agent didn't touch | Z crates/ui/src/comments.rs:124-156; Z crates/ui/src/files/preview.rs:44-47 | desktop `explore.rs` + comment model (adapt) | S | 06-2 |
| 06-14 | Worktree delete: `worktree prune` and `branch -D` only when Pocket created the branch | No branch litter; user branches stay safe | Z crates/engine/src/repos.rs:1219-1265 | desktop `git::remove_worktree` (adapt) + a record of created branches | S | Record branch provenance in the store |
| 06-15 | Setup env: `POCKET_PROJECT_ROOT` / `POCKET_WORKTREE_PATH`; setup runs in a visible terminal; optional committed setup file | Setup scripts can copy caches or ports relative to the main checkout | Z crates/engine/src/project_actions.rs:307-341, :27 | desktop `forms.rs` setup op + pocketd spawn env (adapt) | S | — |
| 06-16 | Ref picker with `current`/`worktree` tags and "Current worktree" reuse (no third mode) | Start a session in an existing worktree instead of minting another | Z crates/proto/src/view.rs:540-600; Z crates/ui/src/pickers.rs:3375-3537 | desktop `forms.rs` new-session form (adapt); `git::worktrees` exists | M | — |
| 06-17 | Wrap toggle; persist split and wrap | Long lines readable; choice survives restart | Z crates/ui/src/changes.rs:3768-3783 ("Split view", "Wrap long lines"); Z crates/ui/src/settings.rs:858-860 (`diff_split`, `diff_wrap`) | desktop `diff.rs` + store (adapt) | S | — |
| 06-18 | Phone "This turn" diff list | Review agent output from the couch | 06-1 evidence; P packages/protocol/src/timeline.ts:9-29 (FileDiff exists) | packages/app (new screen) + protocol (adapt) | M | 06-1 |
| 06-19 | Editable files with hash-guarded atomic save and conflict banners | Quick fixes without an IDE | Z crates/engine/src/workspace_files.rs:1748-1863; Z crates/ui/src/files/preview.rs:2135-2334 | desktop `explore.rs` Editor editable (adapt) | L | Scope question: Pocket is a watch-and-drive tool |

## UI/UX spec to copy

### Layout and measurements (diff pane)

- **Header strip:** h38, gap10, px16, bottom hairline 0.06. Label 12 px `text_muted`; `+N/−N` 11 px mono (Z crates/ui/src/changes.rs:4271).
- **"Partial snapshot" pill:** 10 px text, px6 py2, radius 4, warning 0.08 background / 0.75 text (Z crates/ui/src/changes.rs:4321).
- **File header** (Z crates/ui/src/changes.rs:3481):
  - Height 38 (= titlebar), row gap 8, px12.
  - Contents in order: chevron 14 px (icon 13, `text_muted` 0.7); file icon 14; mono path 12 px dim, truncated; "BIN" 10 px; `+N/−N` 11 px mono in diff_add/diff_del; "Open in file browser" button 24 px.
  - Background ink 0.025, hover 0.05; 0.04 hairline between files.
- **Hunk header:** h28, px16, `diff_hunk_bg`, mono 11 px faint. **Notice:** h24, 11 px faint (Z crates/ui/src/changes.rs:73-76).
- **Line row** (Z crates/ui/src/changes.rs:4430):
  - Height 21. Accent bar 3 px, at 0.55 of the add/del color. Add/del row wash 0.055.
  - Gutter: 36 px per column; numbers 11 px mono, right-aligned, pr8. Changed-side number at 0.9, others faint 0.8.
  - Marker column 28 (split 18): "+", "−", "·" (faint 0.5).
  - Code: 12 px scaled by `code_font_size`; tab = 4; padding left 12 (split 6), right 24. Text at 0.92. Horizontal scroll per file, locked to its axis.
  - Split divider 1 px.
- **Header toggles:** 24 px, radius 6, hover wash 0→0.14, active latched at 0.14 (Z crates/ui/src/changes.rs:3715; Z crates/ui/src/surface_chrome.rs:7-11).
- **Tooltips:** px8 py6, radius 6, 11 px, delay 350 ms.
- **Scope trigger:** h24, px8, gap6, wash 0.05→0.14, label 12, chevron 12. Menu w180, rows gap 2, 10 px below.
- **Commit-pinned chip:** 7-char mono sha at 10.5 px on ink 0.05, then subject at 12 px (Z crates/ui/src/changes.rs:3800).

### Comments

- **Adder:** 16 px, radius 4, `theme.solid` background, PLUS 11 px, aria "Add comment", centered in the gutter (Z crates/ui/src/comment_ui.rs:20-35; Z crates/ui/src/changes.rs:4742-4759).
- **Card** (Z crates/ui/src/comment_ui.rs; Z crates/ui/src/comments.rs:257-272):
  - Ink 0.05 background, 3 px accent bar at 0.35, px16.
  - Header: CHAT_ROUND_LINE 12 plus mono 11 location (faint). Edit (PEN) and remove (CLOSE_CIRCLE) buttons, 16 px, shown on hover; tooltips "Edit comment" / "Remove comment".
  - Body 12/18, dim.
  - Metrics: pad-v 20, header 22, line 18, gap 6; wraps at 64 cols; max 8 lines, measured analytically.
- **Draft** (Z crates/ui/src/comment_ui.rs:240-290; Z crates/ui/src/changes.rs:2851):
  - h116, ink 0.08, accent bar 0.7, py10. Input h46, placeholder "Request a change…".
  - Action row: h28, right-aligned, gap6. "Cancel" (ghost) and "Comment" (or "Save" when editing) (primary): h22, px10, radius 6, 11 px medium.
  - Escape cancels.
- **Composer chip:** CHAT_ROUND_LINE + "N comments" (Z crates/ui/src/composer.rs:5918-5948).

### History

- Row h36, lane 12, node r3, HEAD ring +2, stroke 1.5, focused 2.25, unfocused 0.24 / row 0.6.
- Ref badge ≤112 px, gap 5; search input 196 px (Z crates/ui/src/history.rs:36-69).

### PR badge

- Sidebar h16, px4, radius 4, 10 px. Composer h20, px7, radius 6, 11 px. Background at 0.08, 0.16 on hover.
- Tooltip "PR #{n} · {state}", max width 320.

### Tokens

- **Dark:**
  - diff_add `oklch(0.765 0.177 163.223)` (emerald-400)
  - diff_del `oklch(0.704 0.191 22.216)` (red-400)
  - hunk `hsla(0.6, 0.35, 0.6, 0.05)`
  - Source: Z crates/ui/src/theme.rs:1201-1203.
- **Light:**
  - add `oklch(0.596 0.145 163.225)`
  - del `oklch(0.577 0.245 27.325)`
  - hunk `hsla(0.6, 0.35, 0.35, 0.07)`
  - Source: Z crates/ui/src/theme.rs:1301-1303.
- **Spacing:** 8 / 12 / 16 (Z crates/ui/src/theme.rs:840-842). **Controls:** 24 px, radius 6, icon 14, gap 4 (Z crates/ui/src/surface_chrome.rs:7-11).

### Motion (Z crates/ui/src/motion.rs)

| Name | Duration | Easing / notes | Line |
|---|---|---|---|
| COLLAPSE | 180 ms | ease-out (0, 0, 0.58, 1) | :393 |
| CHEVRON | 200 ms | crossfade opacity 0.25→1 | :401 |
| HOVER_FADE | 150 ms | tailwind (0.4, 0, 0.2, 1) | :411 |
| MENU_IN / MENU_OUT | 140 / 100 ms | | |
| DIALOG_IN | 180 ms | | |
| RESIZE | 200 ms | | |
| TAB_SLIDE | 150 ms | | |
| SCROLL_GLIDE | 500 ms | | |
| FADE_IN | 500 ms | expo (0.16, 1, 0.3, 1) | |

- Fold tween: 400 ms window, capped at 2,400 px (Z crates/ui/src/changes.rs:1564-1570).
- History rows entering or leaving: COLLAPSE height, opacity 0.35→1.

### Shortcuts

- Cmd+J toggles the terminal (Z crates/ui/src/terminal/panel.rs:49-56).
- Browser: mod-L, mod-T, mod-W, mod-[, mod-]; reload is configurable (Z crates/ui/src/browser/mod.rs:44-58).
- Ref menu: ↑↓/⏎. Tree: ↑↓←→ ⏎ Space. Comment draft: Esc.
- Pocket's existing ⌘↵ submit (P packages/desktop/crates/pocket/src/diff.rs:423) should stay.

### States and copy to reuse

- **Diff:**
  - "Preparing diff…" with a gradient spinner (Z crates/ui/src/changes.rs:5039)
  - Clean-state strings (§2)
  - "Diff stream interrupted — retrying"
- **Discard dialog:** §4 strings.
- **Browser:**
  - Empty state: "Preview your work" / "Preview your local app or keep a website beside your conversation." / "Enter an address ⌘L"
  - Error: "Couldn’t load this page" / "Check the address and make sure your server is running, then try again." / "Try again"
  - Address placeholder: "Website or localhost:3000"
  - Sources: Z crates/ui/src/browser/view.rs:288-301; Z crates/ui/src/browser/macos.rs:205; Z crates/ui/src/browser/mod.rs:125; Z docs/screenshots/browser/macos-empty-dark.png; Z docs/screenshots/browser/macos-error-light.png.
- **Preview list:** "Running locally", "Looking for dev servers…", "Start a dev server in this project. It will appear here automatically, ready to open.", "Or enter a website address" (Z crates/ui/src/browser/view.rs:104-269).

## Open questions / risks

- **Turn snapshot race.** Zeron snapshots asynchronously after dispatch, so the agent's first writes can land before `write-tree` runs (Z crates/engine/src/diff_sync.rs:286-316).
  - Pocket could snapshot inside the blocking `UserPromptSubmit` hook, which has a 5 s timeout budget (P docs/designs/2026-09-28-agent-sessions.md:75).
  - `add -A` on big repos may exceed that.
- **Codex turn boundaries.** Codex sends no claude hooks (P packages/pocketd/internal/daemon/claude_test.go:143). pocketd instead notes a codex Enter as the turn start and maps it to an app-server thread (P packages/pocketd/internal/daemon/claude.go:26-29, P packages/pocketd/internal/daemon/codex.go:14-15). A codex snapshot could hang off that Enter, but it is not blocking, so it races the agent's first writes.
- **Snapshot persistence.** Zeron's snapshots are memory-only and lost on restart. Pocket should persist the tree sha per session. Loose `write-tree` objects are unreferenced and can be pruned by `git gc` after 2 weeks.
- **Multi-line comments through a PTY.** `Prompt` relies on the TUI treating a fast burst as a paste (P packages/pocketd/internal/terminal/terminal.go:276-277). Unverified for codex and for long blocks; bracketed paste may be needed.
- **Batch vs immediate.** Pocket's immediate send plus "Resolve" is a deliberate choice. Should batching replace it or be a mode? The product owner should decide.
- **Embedded browser.** Needs GPUI content composited over a native `WKWebView`. Zeron patched its gpui fork for this (Z docs/research/diri-sidebar-browser-plan.md:24-33). gpui-kit 0.6.6 support (P packages/desktop/Cargo.toml:25) is unknown, hence "wont" for now.
- **Licensing and porting.** Zeron is MIT, so porting is allowed with a notice. Its code targets a pinned Zed-rev gpui ("Zui"), not gpui-kit, so APIs will differ (canvas paths, `list`, blur). Zeron also adapted Apache-2.0 code from egoist/zed; keep that attribution if any of it is ported.
- **`gh` dependency.** Missing, unauthenticated or rate-limited `gh` must fail silently. Enterprise hosts need a `host/owner/repo` selector (Z crates/engine/src/source_control.rs:240-260).
- **Code vs docs disagreements:**
  - Linux browser is embedded in code, external-only in the plan (§10).
  - The screenshots come from `d5c08649`, which predates this sha (Z docs/screenshots/browser/README.md).
- **Missing in Zeron.** Hunk-level stage and revert are absent, so there is nothing to port for them. Pocket's per-path model stays the base.

## Verification

Date: 2026-09-30. Claims checked: 34. Corrected: 6. Pocket claims checked against `git show b9d14a1:…`.

Confirmed include: diff capture commands and limits (3 MiB, 500 ms, 120 s, 8,000 dirs); temp-index `write-tree` turn snapshot, async, memory-only; five scopes and labels; sticky header 38/16 px blur, row sizes; fold 180 ms / 400 ms window / 2,400 px; comment block headers and `(L|R)` bullets; whole-tree discard guard and dialog copy; history constants; `gh pr list` args, TTLs and backoff; file tree and workspace limits; Cmd+J, browser chords; worktree path, `zeron/` branch, prune and `branch -D`, setup env vars; preview hostnames, port 7331, 2 s / 800 ms probe, mux sizes; diff tokens; motion table. Pocket: `set_staged`/`discard`, commit kinds, immediate comment prompt, unsent `old_side`, `Prompt` 150 ms Enter, hook statuses, 2 s `refresh_git`, non-virtualized read-only explorer, 512 KB cap, worktree dir and `.env*` copy, branch kept on remove, no `gh`, gpui-kit 0.6.6.

Corrections:
- §4: `transcripts.rs:198` is `gh pr create`, not "push"; reworded.
- §4: Pocket's discard confirm is rendered in `overlay.rs:282-289`; `changes.rs:516-531` is `ask_discard`/`discard`.
- §10: framework labels also include "Node HTTP server" (node/bun/deno); added citation.
- §10: all browser chords are conflict-checked, not only reload; added the reload default's citation.
- 06-1 and open questions: codex turn start is already noted by pocketd on Enter; the "unverified" question is now answered.
- 06-17 and preview list: fixed line citations (`changes.rs:3768-3783`, `view.rs:104-269`).
