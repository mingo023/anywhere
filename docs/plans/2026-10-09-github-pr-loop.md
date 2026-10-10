# GitHub PR Loop Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** Ship the "Inline" variant of `.ui-review/prototypes/github.html` in the desktop app: from the Changes panel, push and open a PR with an agent-written title and body, then watch checks, reviews and review threads, reply/resolve, hand failures to the session's agent, and merge — all through `gh`.

**Architecture:** `git::github` (model crate, no GPUI) owns every `gh` argv and every parser, tested against real `gh` JSON. `pocket::git_ui::pull_request` owns the feature's state (`PullRequest`: composer, busy/error, subview, sent items, reply input) and its views; `pull_requests.rs` keeps polling. `impl Desktop` only gathers inputs, spawns `daemon::run_login` on the background executor, and applies the result.

**Toolset (from `packages/desktop`):**
- One crate: `cargo test -p git`, `cargo test -p store`, `cargo test -p pocket`
- All: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
- Screens: `.ui-review/fixture/capture.sh <dir> <name>=<steps>…`

**Read first:**
- `docs/adr/0003-desktop-code-layout.md` — where code goes.
- `CLAUDE.md` — layout, performance, checks.
- `packages/desktop/crates/pocket/src/git_ui/pull_requests.rs` — the existing poll loop this builds on.
- `.ui-review/prototypes/github.html` (Inline variant) — copy and layout.

---

## Decisions

- `gh pr view --json` gains `reviewDecision,latestReviews,reviewRequests,mergeable,mergeStateStatus,headRefOid,baseRefName`; review threads come from one GraphQL query per open PR, fetched with the view.
- PR create: `gh pr create --base B --title T --body-file -` (body on stdin) `[--draft]`. Title/body are written by the commit-message agent (`store::prefs::Git::pr_argv`), first line title, rest body; the composer lets the user edit both.
- Merge: `gh pr merge N --squash|--merge|--rebase`, never `--delete-branch`. Deleting the worktree afterwards goes through `removal.rs` (clean tree: delete directly and keep the branch; dirty: ask).
- Merge enabled iff open, not draft, mergeable, no failed or pending checks, review not ChangesRequested/Required, merge state not Blocked. `mergeable` UNKNOWN reads "Checking for conflicts…", never "No conflicts".
- Thread chip: resolved → "Resolved"; sent to agent → "Sent"; last comment by someone other than the first → "Replied"; else open.
- Send to agent: `chat_choices`/`chat_target` → `send_input` with the bytes, then `\r` after 80 ms. "Fixing" state lasts until `head_oid` changes.
- Conflicts: "Resolve with agent" sends "Merge origin/<base> into this branch and resolve the conflicts, then push."

## PR 1: Model — `gh` argv and parsers

**Scope:** `git::github` types/parsers, `git::pr_context`, `git::Repo.unpushed`, `store` `pr_argv`. Inert until PR 2.

- 1.1 Extend `Pr` (derive `Default`) with `runs: Vec<Check>`, `review: Review`, `reviews: Vec<Reviewed>`, `requested: Vec<String>`, `mergeable: Mergeable`, `blocked: bool`, `base: String`, `head: String`, `threads: Vec<Thread>`. Tests: `an_open_pr_lists_each_check_with_its_duration_and_log`, `a_check_that_completes_before_it_starts_lasts_no_time`, `an_open_pr_reads_its_review_and_mergeability`.
- 1.2 `threads_argv`, `threads_result`. Test: `review_threads_fall_back_to_their_original_line`.
- 1.3 `create_argv(base, title, draft)`, `merge_argv`, `ready_argv`, `reply_argv`, `resolve_argv`, `split_message`. Tests per argv.
- 1.4 `Pr::gate()` (merge gate) and `Thread::chip(sent)`. Tests: `a_pr_merges_only_once_every_gate_is_green`, `an_unknown_mergeability_never_reads_as_no_conflicts`.
- 1.5 `git::pr_context(cwd, base)`, `git::texts_between(cwd, base, path)`, `Repo.unpushed`. Temp-repo tests.
- 1.6 `store::prefs::Git::pr_argv`. Test: `a_pr_is_written_by_the_same_agent_as_commit_messages`.

## PR 2: Ship flow

**Scope:** `git_ui/pull_request.rs` state + composer; ship row in the commit box; header "Create" opens the composer; the old `--fill` create path goes away.

- 2.1 `PullRequest` state struct (`new` → `(Self, Vec<Subscription>)`), composer open/write/send.
- 2.2 Ship row (nothing to commit): "N commits to push" + "Push N commits", or "Up to date" + "Create Pull Request"; chevron menu with Push and create PR / Create pull request / Create PRs as drafts.
- 2.3 Commit menu gains "Commit, push and create PR".
- 2.4 On create: `prs.forget`, poll again, toast "Opened [draft ]PR #N into <base>".

## PR 3: PR card

- 3.1 Header (title, "#N · detail", open on GitHub), checks list with durations and Log, review row.
- 3.2 Merge gate rows, split merge button with methods menu, Ready for review, delete-after checkbox, merged Keep/Delete prompt.

## PR 4: Threads

- 4.1 Unresolved comments in the card; Comments subview (Unresolved/All, thread cards with last 3 hunk lines).
- 4.2 Reply / Resolve / Unresolve, optimistic, Undo toast on resolve.
- 4.3 Send failing checks / a thread to the agent; "Fixing" state.

## PR 5: Inline threads in the diff

- 5.1 `Doc::PrDiff`-style view of a file against the PR base (merge-base), thread rows inserted like `Row::Composer`.

## PR 6: Conflicts and cleanup

- 6.1 "Resolve with agent" on conflicts; delete the prototype.
