# Research: git, GitHub, diff and file preview for the iOS app

Scope: what the desktop does today for git / GitHub / diff / file preview, how, and a proposed pocketd API so a phone gets the same features through pocketd (the phone never runs git). Paths are relative to the repo root. `D/` = `packages/desktop/crates/`, `P/` = `packages/pocketd/internal/`.

## 0. Key facts

1. **Everything shells out. There is no libgit2 and no GitHub REST client.** git goes through the `git` CLI (`D/git/src/git.rs`). GitHub goes through the `gh` CLI (`gh pr view --json …`, `gh api graphql`), with auth delegated to `gh` (`D/git/src/github.rs`). The only in-process computation is `imara-diff` (Histogram) for line and word diffs, plus tree-sitter for syntax colours.
2. **pocketd can already run git for a client, but only for an owner.** `P/ops/ops.go:369` serves `exec`, `fs.list|stat|read|write|mkdir|rename|remove|trash` through `P/ops/host.go` and `P/hostop/{run,fs}.go`, so the desktop's remote-host mode (`git::Runner::Remote`, `D/hosts/src/remote.rs:261`) can drive git on another machine. `P/peer/check.go:61-70` gates these as `Own` on the `ops:` (unix-socket) surface. The phone's `ws:` surface has none of them. **Do not give the phone raw `exec`.** Add typed `git.*` / `gh.*` / `fs.*` verbs on `ws:`, each with its own scope.
3. **The desktop's git model is portable.** `D/git` has no GPUI dependency, only `imara-diff` + `serde_json`. Its argv lists and parsers are the spec to port to Go. pocketd already reimplements part of it: `P/worktree/list.go` mirrors `git::worktrees`, `P/worktree/pr.go` runs `gh pr view`, and `P/daemon/gh.go` resolves the per-project `GH_TOKEN`.
4. **Desktop prefs that matter live in the `desktop.json` that pocketd already reads.** `P/registry/registry.go:29` reads `projects`/`repos`/`worktree`. The same file has `git` (`remote`, `untracked`, `commit_all`, `force_with_lease`, `draft`, …), `diff` (`context`, `ignore_whitespace`, `all_branches`, `page`) and `files` (`hide_ignored`, `hide_dotfiles`, …) (`D/store/src/store.rs:363-369`, `D/store/src/prefs/{git,diff,files}.rs`). pocketd can add those fields to `registry.File`, so phone pushes and commits behave exactly like the desktop's.
5. **The desktop has no pull, fetch, branch switch, stash, or PR list.** Write ops are: stage/unstage, discard, commit (plus amend), push (optionally with lease), create PR, merge PR, mark ready, reply to / resolve / unresolve a review thread, delete a worktree and its branch, and clone. "Every PR in the repo" is not a desktop feature: the desktop polls **the PR of each worktree's branch**.

## 1. Feature inventory (desktop)

### 1.1 Changes (status) — `git::read` (`D/git/src/git.rs:95`)

Shows: the branch, the base branch it grew from, ahead/behind against the base, unpushed count, a file list (path, +/−, staged/unstaged, status letter M/A/D), up to 20 commits since the base, and remotes. `Repo` is at `git.rs:31` and `FileStat` at `git.rs:14`.

git commands, run sequentially (each spawns a process; about 130 ms per worktree per CLAUDE.md):

- `rev-parse --abbrev-ref HEAD`, falling back to `symbolic-ref --short HEAD`.
- `diff --cached --name-only` (staged).
- `diff --name-only` (unstaged).
- `diff HEAD --name-status`. `R` is collapsed to `M` (`name_status`, `git.rs:160`).
- `diff HEAD --numstat --histogram`. A binary file prints `-\t-`, which parses to 0/0 (`numstat`, `git.rs:83`).
- With `untracked` on: `ls-files --others --exclude-standard`, then **each untracked file is read in full, up to 16 MB, to count its lines** (`git.rs:107-111`). This is a perf hazard with many untracked files.
- Base detection (`base_of`, `git.rs:139`):
  - Look at the first reflog entry, `log -g --format=%gs refs/heads/<b>`, for "branch: Created from X". Prefer `origin/X`, then `X`.
  - Otherwise use `symbolic-ref refs/remotes/origin/HEAD`, then `origin/main`, `origin/master`, `main`, `master`.
  - On main/master the base is `@{u}`.
- `rev-list --left-right --count base...HEAD`, `log --format=%h\t%s -n 20 base..HEAD`, `rev-list --count @{u}..HEAD` (unpushed; `None` before the first push), `remote`.

Quirks to fix in a port:

- Staged/unstaged membership uses `Vec::contains`, and status lookup uses `iter().find`. Both are O(n²) over 1000+ files.
- A staged rename (`git mv`) probably shows in `--numstat` as `old => new`, which matches neither list.

UI (`D/pocket/src/git_ui/changes.rs`):

- Sections are "Staged" then "Changes", shown flat or as a tree that merges single-folder chains (`tree`, `changes.rs:95`). Folds are per section.
- The list is virtualized with `uniform_list` (`changes.rs:199`).

Refresh:

- `refresh_git` (`D/pocket/src/desktop/project.rs:171`) runs every 2 s (`general.git_every()`, 10 s with battery saver). It reads every worktree of the selected project sequentially in one background task.
- A newer run discards an older one through the `git_run` counter.
- `load_shown_repo` (`project.rs:233`) reads only the shown tree, so a click is fast.

Writes:

- **Stage / unstage**: `git add -A -- <paths>` and `git reset -q -- <paths>` (`set_staged`, `git.rs:274`). The UI updates optimistically (`flip_staged`) and bumps `git_run` (`changes.rs:228`).
- **Discard**: untracked paths go to `git clean -fq -- …`, tracked paths to `git checkout -q -- …` (`git.rs:280`). This discards unstaged edits only, and the desktop confirms first (`Confirm::Discard`).
- **Commit**: `git commit -q [--amend] (-F - | --no-edit)`, with the message on stdin (`commit_argv`, `git.rs:292`). It runs through the **login shell** (`daemon::run_login`) so hooks, gpg and similar find the user's PATH (`changes.rs:286`).
  - `CommitKind` is Commit / Push / Amend / Ship. Ship means commit, push, then open the PR composer.
  - When nothing is staged and `git.commit_all` is set, every changed path is staged first. Otherwise the desktop shows "Stage the changes to commit first" (`commit_step`, `changes.rs:34`).
- **Push**: `git -c push.autoSetupRemote=true [-c remote.pushDefault=<r>] push [--force-with-lease --force-if-includes]` (`push_argv`, `git.rs:303`). It runs through the login shell for credential helpers and ssh-agent.
- **AI commit message**: `commit_context` (`git.rs:322`) is the last 10 subjects plus `diff --cached` or `diff HEAD`, capped at `MAX_CONTEXT` = 60 000 chars. It goes on stdin to a one-shot agent, `prefs::Git::message_argv` (`D/store/src/prefs/git.rs:74`).

### 1.2 Diff — `D/git/src/git.rs:346-545`, `D/pocket/src/git_ui/diff.rs`

- **Compare targets.** `At` (`diff.rs:41`) is one of:
  - `Working`: HEAD against the working tree, via `git show HEAD:./<rel>` plus a file read capped at `MAX_READ` = 16 MB (`texts`, `git.rs:377`).
  - `Commit(sha)`: first parent against the commit, via `commit_files` / `commit_texts` (`git.rs:659-697`). That uses `diff-tree -r -z -M --name-status`, so **renames are detected**, and `old_path` is followed.
  - `Base(base)`: merge-base against HEAD, the PR view (`texts_between`, `git.rs:346`).
- **Diff engine.** The desktop does **not** use git's patch output. It fetches both full texts and runs `diff_texts` (`git.rs:386`): imara-diff `Algorithm::Histogram` plus `postprocess_lines`.
  - Context lines come from `Options.context` (default 3).
  - `ignore_whitespace` compares lines with whitespace collapsed but shows them as written.
  - Folded gaps become `Kind::Hunk` rows `@@ -a +b @@`. A gap opens fully when its first new-side line number is in the `open: HashSet<usize>` (click to expand).
- **Binary.** If either side contains `\0`, `diff_texts` returns an empty diff (`git.rs:387`).
- **Output shape.** `Line { kind: Hunk|Context|Add|Del, old: Option<usize>, new: Option<usize>, text }` (`git.rs:465-479`). `split()` pairs del/add runs for side-by-side view (`git.rs:525`). `parse()` reads unified text (`git.rs:489`).
- **Word-level diff.** `git::words` (`git.rs:447`) tokenizes into alnum/underscore runs plus single punctuation chars, runs Histogram over the tokens, and returns byte ranges for each side.
  - It is applied only where a run of D deletions is followed by exactly D additions and D ≤ `MAX_WORD_LINES` = 5 (`diff.rs:35,97-127`). Same gate as Zed.
- **Syntax highlighting.**
  - tree-sitter through gpui-kit's `SyntaxHighlighter` (`D/pocket/src/syntax.rs`).
  - It parses the **whole old and the whole new file** and splits the spans per line (`line_spans`, `syntax.rs:58`).
  - Deleted lines take colours from the old side, others from the new side (`highlights`, `diff.rs:97`).
  - Skipped when either side is over `MAX_COLORED` = 512 KB (`diff.rs:34,83`).
  - Languages: `language_for` (`syntax.rs:35`) covers rs, ts/mts/cts, tsx, js/jsx/mjs/cjs, py, go, swift, kt/kts, java, json, md, toml, yml/yaml, sh/zsh/bash, css/scss, html. Anything else is "text".
  - Custom: Swift queries and TSX JSX-component captures (`syntax.rs:9-31`).
- **Performance.**
  - Diffs are read in `cx.background_executor()` (`diff.rs:626`).
  - A result applies only if the pane still shows that `(path, at, open, options)` (`DiffState::apply`, `diff.rs:400`).
  - Re-colouring is skipped when the lines didn't change (`read_diff`, `diff.rs:60`: `colors = (lines != shown).then(...)`).
  - Rows are drawn with GPUI `list` (variable height) (`diff.rs:581`).
  - The Working diffs of open panes also refresh inside `refresh_git`.
- **Interaction.**
  - Line pick, drag and quote into chat (`Pick`, `diff.rs:214`).
  - PR review threads are pinned under their line (`pins`, `diff.rs:147`). A thread with `left` set sits on the old side.

### 1.3 Commit graph — `D/git/src/git.rs:546-645`, `D/git/src/graph.rs`, `D/pocket/src/git_ui/graph.rs`

- Data:
  - `log --format=%H%x00%P%x00%aN%x00%s --topo-order --skip=S -nN <revs> --` gives `GraphCommit { sha, parents, author, subject }` (`git.rs:554`).
  - `tips()` (`git.rs:619`) runs `for-each-ref --format=%(refname)%00%(objectname)%00%(upstream) refs/heads refs/remotes` and `symbolic-ref -q HEAD`. The result is `Tips { head, branch, upstream, base, others }`; `others` holds every ref only when `diff.all_branches` is set.
- Layout: `graph::Lanes::push` (`D/git/src/graph.rs:37`) is a 60-line port of VS Code's `scmHistory.ts`.
  - It is stateful across pages, so page N+1 continues page N's lanes.
  - `GraphRow { input, output: Vec<Lane{sha,color}>, column, color }`, where `LaneColor` is Head/Upstream/Base/Lane(n).
- Paging: `PAGE` = 50 (`D/pocket/src/git_ui/graph.rs:18`; also `prefs.diff.page`). A short page ends the history. A refresh re-reads as many commits as are shown.
- Expanding a commit lists its files (`commit_files`). Clicking a file opens the `At::Commit` diff.

### 1.4 Branches / worktrees

- `branches` uses `for-each-ref --sort=-committerdate refs/heads` (`git.rs:217`). `remote_branches` lists origin refs that have no local twin (`git.rs:222`). `dated_branches` adds the epoch (`git.rs:233`).
- These feed new-worktree and session pickers. There is no checkout or switch UI.
- `worktrees` uses `worktree list --porcelain`, puts the main one first and sorts the rest by folder birth time (`git.rs:207`). pocketd's copy is `P/worktree/list.go:23`.
- Destructive ops:
  - `worktree remove --force` (10-minute timeout).
  - `branch -D`.
  - `lost_commits` runs `rev-list --count <tip> --not --branches --remotes --tags`, to warn before deleting (`git.rs:249`).
  - `D/pocket/src/removal.rs:178-195`.
- Worktree creation is already in pocketd (`P/worktree/create.go`, LaunchSpec `checkout.new`).

### 1.5 GitHub — `D/git/src/github.rs`, `D/pocket/src/git_ui/{pull_requests,pull_request,gh_accounts}.rs`

- **Transport.**
  - Only the `gh` CLI, run under the login shell (`daemon::run_login_env`).
  - `GH_TOKEN` is set to the project's chosen account's token, from `gh auth token --hostname github.com --user <login>` (`github.rs:322`, `gh_accounts.rs:104`). After an HTTP 401 the token is forgotten and the call retried once.
  - Status: `gh --version` and `gh auth status`, giving `GhStatus::{Missing, SignedOut, Ready{login, accounts}}` (`github.rs:283-320`).
  - Account auto-pick: `gh repo view --json viewerPermission` for each account, keeping the highest (`github.rs:332-351`).
  - pocketd already has this logic: `GhEnv` / `ghToken` in `P/daemon/gh.go`.
- **PR for a worktree.** `VIEW` (`github.rs:104`) is `gh pr view --json number,title,url,state,isDraft,statusCheckRollup,reviewDecision,latestReviews,reviewRequests,mergeable,mergeStateStatus,headRefOid,baseRefName`, run in the worktree dir, so the PR is the current branch's.
  - Parsed into `Pr` (`github.rs:6`): `checks{passed,failed,pending}`, `runs: Vec<Check{name,outcome,url}>`, `review`, `reviews: Vec<(login, Review)>`, `requested`, `mergeable`, `blocked`, `base`, `head` (oid), `threads`.
  - `runs()` (`github.rs:417`) handles both `StatusContext` (`state`) and `CheckRun` (`status` + `conclusion`). It keeps only the latest run per `(workflowName, name)` by `startedAt`.
  - Outcome mapping: FAILURE, TIMED_OUT, CANCELLED, ACTION_REQUIRED and STARTUP_FAILURE are Failed; not COMPLETED is Pending; SKIPPED and NEUTRAL are Skipped.
  - Errors: "command not found" means `Missing`, and "gh auth login" means `LoggedOut` (`view_result`, `github.rs:353`).
- **Review threads.** GraphQL `THREADS` (`github.rs:172`): `reviewThreads(first:100){ id isResolved isOutdated path line originalLine diffSide comments(first:50){ author{login} body createdAt diffHunk } }`.
  - Gives `Thread{id,path,line,left,outdated,resolved,comments: Vec<Comment{author,body,at,hunk}>}`.
  - Fetched only for OPEN PRs, together with the view. If it fails, the old threads stay (`pull_requests.rs:250-268`).
- **Merge gate.** `Pr::gate()` (`github.rs:235`) checks, in order: Done, Draft, Conflicts, ChecksFailed, ChecksRunning, ChangesRequested, ReviewRequired, Checking (mergeable UNKNOWN), Blocked, else Ready.
- **Thread state.** `Thread::state(sent)` (`github.rs:270`): Resolved, Sent, Replied (the last author differs from the first), or Open.
- **Polling** (`pull_requests.rs:34`, `:238`):
  - One `gh` call at a time.
  - The tree on screen is polled every `general.prs_every()` (default 30 s, settable, 300 s with battery saver). Other trees every 5 minutes.
  - A hung `gh` loses its turn after 60 s. A missing or logged-out `gh` is retried every 5 minutes.
  - Only non-main, non-detached worktrees are polled.
- **Actions.**
  - Create: `gh pr create --base <b> --title T --body-file - [--draft]`. It pushes first; the PR number is parsed from the printed URL (`github.rs:118-130`). Title and body come from an agent via `pr_context` (log plus `diff base...HEAD`, 60 000-char cap) (`git.rs:335`).
  - Merge: `gh pr merge N --squash|--merge|--rebase`, never `--delete-branch` (`github.rs:159`).
  - Ready: `gh pr ready N`.
  - Reply: GraphQL `addPullRequestReviewThreadReply`.
  - Resolve / unresolve: `resolveReviewThread` / `unresolveReviewThread`, applied optimistically with an Undo toast.
  - "Fix with agent" / "Resolve conflicts with agent" write a prompt (`fix_prompt`, `conflict_prompt`, `pull_request.rs:70-84`) into the session's terminal.
  - After a merge, the worktree can be deleted.
- **Opening a PR as a worktree** is already in pocketd: `P/worktree/pr.go` (`View`, `PreparePR`). It runs `gh pr view --json number,url,headRefName,headRefOid,isCrossRepository,state` and fetches `refs/pull/N/head`.

### 1.6 File tree and preview — `D/pocket/src/explorer.rs`, `D/pocket/src/explorer/preview.rs`

- **Tree.**
  - Lazy, one directory at a time: `list_dir` (`D/pocket/src/util.rs:56`) uses the `ignore` crate at depth 1, honouring gitignore when `files.hide_ignored` is set. It also applies `hide_dotfiles`, `also_hide` and `folders_first` from `prefs::Files`.
  - Open folders are re-listed inside `refresh_git`.
  - Rows go through `uniform_list`.
  - Git letters: `status_of` for files; `folder_status` gives a folder the letter its changes share, else M (`explorer.rs:36-47`).
  - "Go to file" uses `git ls-files --cached --others --exclude-standard` (`git.rs:239`).
  - Remote: pocketd `fs.list` returns `Entry{name,dir,link,size,mtime,ignored}`. `ignored` comes from `git check-ignore --stdin -z` (`P/hostop/fs.go:16-81`).
- **Preview classes.** `Preview` (`preview.rs:34`) is `Text(String)`, `Image(bytes?)`, `Binary(len)`, `TooLarge(len)` or `Unreadable`.
  - Text cap: `MAX_BYTES` = 512 KB. Image cap: `MAX_IMAGE_BYTES` = 20 MB (`preview.rs:30-31`). The decision is made from stat before reading (`classify`).
  - Binary means a NUL byte in the first 8 KB, or bytes that aren't UTF-8 (`decode`, `preview.rs:44`).
  - Images: png, jpg/jpeg, gif, webp, bmp, ico, svg (`image_format`, `preview.rs:61`).
- **Code view.**
  - gpui-kit `Editor` (virtualized, tree-sitter, line numbers, soft wrap and tab width from prefs).
  - Git gutter marks Added / Modified / Deleted from `git::file_diff` (`D/pocket/src/explorer/preview/code.rs:20`).
  - Editable, with autosave (`explorer/edit.rs`).
- **Markdown.**
  - Rendered by default (`files.markdown_source` toggles the raw source) with gpui-kit `TextView` (markdown-rs, GFM).
  - Front matter (YAML/TOML) is rendered as a table (`explorer/frontmatter.rs`).
  - Fenced code is highlighted by language, with shell/zsh/console mapped to bash (`preview/markdown.rs`).
  - **Mermaid** fences are rendered by the `merman` crate to SVG, rasterized by resvg, with a 64 MB cache budget and an 8192 px max side (`explorer/mermaid.rs:12-13`). See `docs/research-mermaid-rendering.md`.
- `language()` (`preview.rs:100`) gives display names.

### 1.7 What pocketd already sends the phone

`proto.FileDiff{lines: []DiffLine{number,kind,text}, additions, deletions, contentOnly}` (`P/proto/proto.go:28-39`) carries the diffs of agent edit/write **tool calls only**.

- The diff comes from a greedy 8-line-window diff (`P/timeline/diff.go`), not git's.
- It is capped at `DiffPreviewLines` = 24 lines of `DiffLineChars` = 160 runes.
- `ParseUnified` reads Codex patches.

This is a chat preview, not a git diff. Keep it for chat, and add a separate git diff type.

## 2. Proposed pocketd API (ws surface)

Conventions follow existing ws messages: a request `{type, id, …}`, a reply with the same `id`, and errors via `proto.NewCodedError`.

- Advertise caps `git.v1`, `gh.v1` and `fs.v1` in `ServerCaps` (`P/proto/version.go:33`).
- Every request names a **tree**: an absolute worktree or Local path. pocketd resolves it against the registry (`worktree.Project(f, cwd)`) and refuses any path outside a registered project. That check is the security boundary that replaces owner-only `exec`.
- Add each verb to `peer.Needs`:
  - Reads (`git.status`, `git.diff`, `git.log`, `git.branches`, `git.commit`, `gh.pr`, `fs.list`, `fs.read`, `fs.search`) need `Observe`.
  - `git.stage`, `git.unstage`, `git.commit.create` and `gh.thread.reply|resolve` need `Drive`.
  - `git.discard`, `git.push`, `gh.pr.create`, `gh.pr.merge` and `gh.pr.ready` need `Own`, or a new `Ship` scope. Discard and merge are irreversible.
- `ClientMessage` (`P/proto/messages.go:11`) is one flat struct. Add `Tree`, `Paths []string`, `Rev`, `Base`, `Cursor`, `Max`, `Context`, `Expand []int`, `Amend`, `Push`, `Lease`, `Draft`, `Method`, `Number`, `Thread`, `Body`. Remember `authedReadLimit` = 1 MB for client to server (`P/wsserver/wsserver.go:48`).

### 2.1 Status

```
→ {type:"git.status", id, tree, cursor?, limit?: 500}
← {type:"git.status", id, tree, rev: "<stamp>", branch, detached, base?, ahead, behind, unpushed?, upstream?,
   remotes:[...], totals:{files, added, removed, staged},
   files:[{path, oldPath?, status:"M|A|D|R|U|?", staged:bool, unstaged:bool, added, removed, binary?:bool}],
   commits:[{sha, subject}]  // ≤20 since base
   nextCursor?}
```

- **Implement with 2–3 git calls, not 9:**
  - `git status --porcelain=v2 -z --branch --untracked-files=<all|no>` gives the branch, upstream, ahead/behind against the upstream, staged/unstaged XY flags, renames (`2 R…` with the original path), and conflicts (`u`).
  - `git diff HEAD --numstat -z -M` gives the counts. With `-z`, renames come as separate old and new fields.
  - Base and its ahead/behind reuse the `base_of` logic, cached per branch: it changes only when the reflog does.
  - `-z` keeps paths with spaces, tabs or unicode intact.
- **Untracked line counts:** don't read every file as the desktop does. Run `git diff --no-index --numstat /dev/null <f>` only for the first page, or just report `added: null` and let the phone show "new".
- **Paging:** files sorted by path. The first page is 500 files (about 60 bytes each, about 30 KB), with `totals` always present. For 5k files the phone pages on scroll.
- **Change push:** pocketd computes `rev` (a hash of the porcelain output). Add `git.watch {tree}` / `git.unwatch` so pocketd polls only watched trees (about every 2 s, or on FSEvents) and pushes `{type:"git.changed", tree, rev}`. The phone then re-fetches only if `rev` differs. Nothing polls when no phone is looking.

### 2.2 Diff

```
→ {type:"git.diff", id, tree, path, at:{kind:"working"|"staged"|"commit"|"base", rev?, base?},
   context?:3, ignoreWhitespace?:false, expand?:[newLine...], maxLines?: 4000}
← {type:"git.diff", id, path, oldPath?, at, status, binary?:bool, tooLarge?:{oldBytes,newBytes},
   image?:{old?:blobRef, new?:blobRef},
   language:"rust"|"text"|…,      // from language_for
   added, removed,
   lines:[{k:"h"|"c"|"a"|"d", o?:int, n?:int, t:"text", w?:[[start,end],…]}],  // w = word-diff UTF-8 byte ranges (or UTF-16 for Swift; pick one and document it)
   truncated?:{atLine, total}}
```

- **Engine.** Port `diff_texts` + `words` to Go, a Histogram diff over interned lines. There is no imara-diff in Go: `github.com/sergi/go-diff` is Myers-only, and pocketd has no diff dependency today (`packages/pocketd/go.mod`).
  - Option A: port imara's Histogram (about 300 LOC).
  - Option B (simpler, matches git exactly): run `git diff --no-color --histogram -U<context> [--ignore-all-space] [-M] <at> -- <path>` and parse the patch. For untracked files use `git diff --no-index /dev/null <path>`. Gap expansion becomes a separate `git.blob {tree, rev, path, from, to}` line-range read that the phone splices in.
  - Then compute word ranges in Go only for equal del/add runs of 5 lines or fewer (the desktop's gate), over the alnum/punct tokenizer in `git.rs:427-445`. Return them as `w` on those lines only.
- **Caps** (in Go, before sending):
  - Binary: NUL in the first 8 KB, or `-\t-` from numstat. Send `binary:true` and no lines.
  - Each side over 16 MB (`MAX_READ`): `tooLarge`.
  - Line text capped at 1024 chars, Zed's `MAX_LINE_LEN`. Lines with `t` truncated get `cut:true`.
  - `maxLines` (default 4000 rows) gives `truncated`, and the phone pages with `offset`.
- **Images.** If the extension is an image format, return blob refs instead of lines. The phone fetches them with `fs.read`, or `git.blob` for the HEAD side.
- **Rename-aware:** for `at.kind:"commit"`, port `commit_files` (`diff-tree -r -z -M --name-status`).

### 2.3 Log and graph

```
→ {type:"git.log", id, tree, all?:false, skip:0, limit:50}
← {type:"git.log", id, tips:{head, branch?:{name,sha,remote}, upstream?, base?, others?:[sha]},
   commits:[{sha, parents:[sha], author, subject, at?:unix}], more:bool}
→ {type:"git.commit", id, tree, sha}
← {type:"git.commit", id, sha, parent?, files:[{path, oldPath?, status}]}
```

- This is a direct port of `log_graph` (`git.rs:554`), `tips` (`git.rs:619`) and `commit_files` (`git.rs:659`).
- **Lay out the lanes on the phone**: port `graph::Lanes::push` (60 lines, `D/git/src/graph.rs:37`) to Swift. Server-side lanes would add `input`/`output` arrays to every row, roughly 5–10× the bytes. Lanes are also stateful across pages, which the phone already holds.
- 50 commits per page is about 8 KB.

### 2.4 Branches

```
→ {type:"git.branches", id, tree, remote?:true}
← {type:"git.branches", id, current, local:[{name, at:unix, upstream?}], remote:[{name, at}]}
```

- One call: `for-each-ref --sort=-committerdate --format=%(refname)%00%(committerdate:unix)%00%(upstream:short) refs/heads refs/remotes/origin`.
- Cap the list at 200 entries, with a `query` filter.

### 2.5 Writes

```
→ {type:"git.stage",   id, tree, paths:[…], staged:true|false}     // add -A -- / reset -q --
→ {type:"git.discard", id, tree, paths:[…]}                         // clean -fq / checkout -q, as git.rs:280
→ {type:"git.commit.create", id, tree, message, amend?:false, all?:<git.commit_all>, then?:"push"|"ship"}
→ {type:"git.push",    id, tree, lease?:<git.force_with_lease>}     // push_argv semantics, remote from git.remote
→ {type:"git.message", id, tree}  → {type:"git.message", id, text}  // AI commit message, commit_context + git.message_argv
← {type:"git.done", id, ok:true} | coded error {code:"git_failed", message:<stderr first line>}
```

- Optional extras, not on the desktop: `git.pull {tree}` (`pull --ff-only`) and `git.fetch`. Add them only if wanted; the desktop has neither.
- Run commit and push under the **login env** (`s.loginEnv()`, as `hostop.Run` does with `env`) so hooks, gpg and credential helpers work. Always pass the message on stdin. Set `GIT_TERMINAL_PROMPT=0` (as `P/worktree/pr.go` does) so a push never hangs on a prompt.
- After any write, push `git.changed` for that tree.
- Serialize writes per repo, one mutex per git common dir, so they don't race on `index.lock`.

### 2.6 GitHub

```
→ {type:"gh.status", id}                         ← {type:"gh.status", id, state:"missing"|"signedOut"|"ready", login?, accounts?}
→ {type:"gh.pr", id, tree}                        ← {type:"gh.pr", id, tree, pr: null | {number,title,url,state,draft,base,head,
      checks:{passed,failed,pending}, runs:[{name,outcome,url}], review, reviews:[{login,verdict}], requested:[…],
      mergeable, blocked, gate, threads:[{id,path,line,left,outdated,resolved,comments:[{author,body,at,hunk}]}]}}
→ {type:"gh.prs", id, project}                    ← {type:"gh.prs", id, items:[{tree, number, state, gate, checks, title}]}  // cached roll-up for the sidebar
→ {type:"gh.pr.create", id, tree, title, body, draft?}  → {…, number, url}
→ {type:"gh.pr.merge",  id, tree, number, method:"squash"|"merge"|"rebase"}
→ {type:"gh.pr.ready",  id, tree, number}
→ {type:"gh.thread.reply",   id, tree, thread, body}
→ {type:"gh.thread.resolve", id, tree, thread, resolved:bool}
→ {type:"gh.pr.write", id, tree} → {title, body}   // AI title and description via pr_context
```

- **Implementation.** Port `VIEW`, `runs()`, `parse()`, `THREADS`, `gate()` and `Thread::state` from `github.rs` into a new `P/github` package.
- **Auth.** Reuse `GhEnv` / `ghToken` from `P/daemon/gh.go` for the per-project account, with a retry on 401.
- **Server-side poller** (moved from `pull_requests.rs`): one `gh` at a time, with a 60 s hang limit and a 5-minute backoff when `gh` is missing or logged out.
  - Poll watched trees every 30 s and others every 5 minutes.
  - Push `gh.pr.changed {tree, number, gate, checks}` when the result changes.
  - This lets pocketd fire a **push notification or Live Activity update** when checks finish, a review lands or the PR merges. Event triggers: the gate changes, or `checks.pending` reaches 0.
- **Payload.** Review comments can be long; send `body` in full but cap `hunk` at its last 3–4 lines, as the desktop shows (`hunk_tail`).
- If the desktop and pocketd both poll, they double the `gh` load. Long-term, the desktop should read pocketd's PR cache.

### 2.7 Files

```
→ {type:"fs.list", id, tree, dir:"<rel>"}   ← {type:"fs.list", id, dir, entries:[{name, dir, link, size, mtime, ignored, git?:"M|A|D|?"}]}
→ {type:"fs.read", id, tree, path, max?:524288, offset?:0}
← {type:"fs.read", id, path, kind:"text"|"image"|"binary"|"tooLarge"|"unreadable", size, mtime,
   text?:string, truncated?:bool, language?:string, mime?:"image/png", data?:base64, gutter?:[[line,"a|m|d"]]}
→ {type:"fs.search", id, tree, query, limit:50}  ← fuzzy over `git ls-files --cached --others --exclude-standard` (cached per tree, refreshed on git.changed)
```

- Reuse `hostop.List` (with `ignored`) and `hostop.Read` (`P/hostop/fs.go`). Merge in the git letters from the cached status, so the tree needs no extra call.
- Classify like `preview.rs`: stat first; text cap 512 KB; image cap 20 MB (consider 10 MB on cellular); NUL-in-8 KB or non-UTF-8 means binary.
- **Shrink images on the Mac.** Above about 2 MB or 2048 px, transcode to HEIC/JPEG at the phone's screen size with `sips` or ImageIO via cgo, and send `fullSize`. SVG goes as text; render it on the phone with WKWebView or SVGKit.
- Markdown goes as text with `language:"markdown"`. Mermaid rendering is the phone's job: WKWebView with mermaid.js, or a pre-rendered SVG from the Mac. See the chat/markdown research.
- Writing files from the phone (`fs.write`) is out of scope unless editing is wanted. If it is, it needs `Own`.

## 3. Syntax highlighting: phone or Mac?

**Recommendation: highlight on the phone, in the visible range only. The Mac sends plain text plus a `language` id.**

Reasons:

- **Payload.** Spans for each line roughly triple a diff payload: about 6–20 spans per code line × `[start,end,token]`. For a 4000-line diff that is hundreds of KB per open file, re-sent on every `git.changed`. Plain text compresses well; enable `permessage-deflate` on the coder/websocket server for git/fs replies.
- **Server code.** pocketd is Go with cgo already on (`scripts/release-mac.sh:39`), so `go-tree-sitter` would build. But it adds about 20 grammars (several MB) to a daemon that also ships to Linux and Windows hosts, plus query and theme mapping in a second language. Desktop colours come from `theme::highlight_theme` in Rust; Go would have to duplicate them.
- **Phone cost is fine.** SwiftTreeSitter with the same grammars and `highlights.scm` (MIT) parses a 512 KB file in tens of ms on an A-series chip, and runs off the main thread. Highlighting only on-screen rows (Neon-style range invalidation) keeps scroll at 120 Hz. Colour matches the desktop because the grammars and query captures are the same; port the capture-to-token map from `theme`.
- **Order of display.** Show plain text first, then apply colours when the parse finishes, as Zed and the desktop do. Keep the desktop's caps: skip highlighting above 512 KB a side, and above 1024 chars a line.
- **Diffs need both sides parsed** for correct context, because deleted lines are coloured from the old text. The phone gets only hunks, not full files. Two options:
  - (a) Highlight each hunk line in isolation with a line-level lexer. That is cheap but wrong inside multi-line strings and comments.
  - (b) Use `git.diff {full:true}` for files at or under 512 KB, sending both full texts (compressed) so the phone parses them like the desktop does.
  - Recommend (b) with a size gate, falling back to (a) or no colour above the gate.
- **Fallback.** If the iOS tree-sitter work slips, a server `tokens:true` flag can come later. The API leaves room for it as an optional `s: [[start,end,cls]]` field per line.

## 4. Payload and performance limits for 1000+ changed files

| Limit | Value | Source of number |
|---|---|---|
| `git.status` page | 500 files, `totals` always | new; about 30 KB per page |
| Status refresh | only watched trees; push a `rev` stamp, the phone pulls on change | new; desktop polls every 2 s |
| Untracked line counts | don't read files (the desktop reads up to 16 MB each) | `git.rs:107-111` |
| Diff rows per reply | 4000, then `truncated` and paging | new |
| Line length | 1024 chars, `cut:true` | Zed `MAX_LINE_LEN` |
| Word diff | equal del/add runs of 5 lines or fewer | `diff.rs:35` |
| Highlight cap | 512 KB per side | `diff.rs:34`, `preview.rs:30` |
| Text read cap | 512 KB preview; 16 MB hard max | `preview.rs:30`, `runner.rs:112`, `hostop.ReadMax` |
| Image cap | 20 MB; downscale above 2 MB | `preview.rs:31` |
| Log page | 50 commits | `graph.rs:18` |
| PR threads | 100 threads × 50 comments, hunk tail of 4 lines | `github.rs:172` |
| AI context | 60 000 chars | `git.rs:319` |
| `exec` output cap (if reused) | 64 MB per stream | `P/hostop/run.go:15` |

On the phone, use `List` / `LazyVStack`, or better a UICollectionView with fixed-height rows for diff and changes, which matches `uniform_list`. Never build all 1000 rows. Fold the tree view client-side from the flat `files` list (port `change_rows`/`tree` from `changes.rs:95-140`).

## 5. Reuse and port map

| Need | Reuse as-is | Port from Rust |
|---|---|---|
| Run git/gh with the user's env | `hostop.Run` (`P/hostop/run.go`), `s.loginEnv()`, `terminal.LookPath` | — |
| Per-project gh token | `Daemon.GhEnv`, `ghToken` (`P/daemon/gh.go`) | 401 retry (`gh_accounts.rs:104`) |
| Worktree list / project lookup | `worktree.List`, `worktree.Project`, `registry` | — |
| Dir listing with gitignore | `hostop.List` (`P/hostop/fs.go:37`) | git letters (`explorer.rs:36-47`) |
| File read with cap | `hostop.Read` | `classify`/`decode` (`preview.rs:44-74`) |
| Status | — | `git::read`, `base_of`, `created_from` (`git.rs:95-158`) → rewrite on porcelain v2 |
| Diff + word diff | `timeline.hunkHeader` / `ParseUnified` as a start | `diff_texts`, `words`, `tokens`, `split` (`git.rs:386-545`) |
| Graph | — | `log_graph`, `tips`, `commit_files`, `commit_texts` (Go); `Lanes::push` (Swift) |
| Writes | — | `set_staged`, `discard`, `commit_argv`, `push_argv` (`git.rs:274-312`), `commit_step` (`changes.rs:34`) |
| GitHub | `worktree.View`/`ghFailure` style | `github.rs` whole: `VIEW`, `parse`, `runs`, `THREADS`, mutations, `gate`, `status`, poll cadence (`pull_requests.rs:34`) |
| Prefs | `registry.File` | add `git`/`diff`/`files` structs mirroring `D/store/src/prefs/{git,diff,files}.rs` |

The desktop's tests are named as behaviours, for example `a_check_stays_fixing_while_any_run_of_it_still_fails`, and use real `gh` JSON fixtures (`github.rs:478-560`). Copy those fixtures into Go golden tests so the two implementations agree.

## 6. Open questions

1. Should the phone get destructive git ops (discard, push, merge)? If so, under `Own` or a new scope?
2. Should the PR poller live in pocketd and the desktop consume it, rather than both polling `gh`? This is needed for PR push notifications and the Live Activity anyway.
3. Should we add pull/fetch and branch switch for the phone? The desktop has neither, so this would be new behaviour on both sides.
4. Diff engine: port Histogram to Go (exact parity with the desktop's imara output), or use `git diff --histogram` patches (simpler, and git's own output)?
