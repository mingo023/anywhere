# Design: closing worktree gaps vs Superset

Date: 2026-10-05. Base: `4074f80`. `pd` = `packages/pocketd`, `pk` = `packages/desktop/crates/pocket/src`, `d` = `packages/desktop/crates`.
Source: arena of three candidate designs (`/tmp/arena-worktree-gaps`), synthesis note at the end.

## 1. Gaps

1. No way to open an existing branch or a GitHub PR as a worktree. New worktrees always get a new branch (`pd/internal/worktree/create.go` `Plan.Add`).
2. Delete only runs `git worktree remove --force` (`pk/desktop/project.rs` `delete_worktree`). No teardown script, no way to delete the branch, no warning about commits only that branch holds.
3. No PR status or CI on a worktree, and no Create PR.

Non-goals: phone UI for the new modes; merge/review actions; a repo-committed script file; a WS remove verb; teardown for worktrees removed outside the app; a `pocketd worktree open` CLI verb.

## 2. Open a branch or a PR (pocketd creates, desktop asks)

### Wire

`NewWorktree` (`pd/internal/proto/launch.go`, `packages/protocol/src/launch.ts`) gains two optional fields:

```go
Branch string `json:"branch,omitempty"` // existing local branch, or one origin has
PR     string `json:"pr,omitempty"`     // "123", "#123" or a PR URL; passed to gh as is
```

`decodeSpec` rules: at most one of `base`, `branch`, `pr`; `name` is required only when neither `branch` nor `pr` is set. When `name` is empty, pocketd names the folder (`Folder`, below). Cap `open.v1` joins `proto.ServerCaps`; the desktop offers the new modes only when the server advertises it. No new error codes: failures are `spawn_failed` with a readable message, folder clashes stay `worktree_exists`. Goldens `agent_create_branch.json` and `agent_create_pr.json` round-trip in Go and TS.

### pocketd (`pd/internal/worktree/open.go`, `pr.go`)

```go
// Folder is branch with / as -, then -2, -3… until no folder in dir has it.
func Folder(branch string, folders []string) string

type Open struct {
    Project, Path, Branch string
    Adopt   string // a worktree already on Branch; nothing is added
    Note    string
    created bool   // Branch made by this Open; Rollback deletes it
    oid     string
}
func PrepareBranch(f registry.File, project, name, branch string, env []string) (*Open, error)
func PreparePR(f registry.File, project, name string, pr PR, env []string) (*Open, error)
func (o *Open) Add() (Created, error)
func (o *Open) Rollback()

type PR struct{ Number int; URL, Head, OID string; Fork bool; State string }
func View(project, ref string, env []string) (PR, error) // gh pr view <ref> --json …, 15s
func parseView([]byte) (PR, error)
func ghFailure(stderr string) error
```

Every git call runs with the login env plus `GIT_TERMINAL_PROMPT=0`; fetches have a 15s timeout, as `Plan.Fetch` does. Each open starts with `git worktree prune`, so a worktree whose folder was deleted no longer holds its branch.

Branch `b` (remote is `origin`):

| State | Action |
|---|---|
| `b` checked out in a worktree, main included | Adopt it: no add, copy or setup. Note "Already open in <folder>". |
| Local `b` only, or fetch fails / no origin | `git worktree add -q -- <path> b` |
| `origin/b` only, after `fetch origin +refs/heads/b:refs/remotes/origin/b` | `git worktree add -q --track -b b -- <path> origin/b` |
| Both, `b` an ancestor of `origin/b` | `git branch -f b origin/b`, then add. Note "Fast-forwarded b". |
| Both, ahead or diverged | Add local `b` unchanged. Note "b differs from origin/b". |
| Neither | Fail "No branch b". |

PR `ref`: `gh pr view <ref> --json number,url,headRefName,headRefOid,isCrossRepository,state` in the project.

1. Remote: the one whose fetch URL holds `OWNER/REPO` from the PR URL (ssh or https). None → "No remote points at OWNER/REPO". This also catches a URL from another repo.
2. Local branch `L`: `headRefName` for a same-repo PR, `pr/<n>` for a fork.
3. Fetch: same repo `+refs/heads/<head>:refs/remotes/<remote>/<head>`; if the head branch is gone, or for a fork, `+refs/pull/<n>/head:refs/pocket/pr/<n>` (a private ref, not the shared `FETCH_HEAD`; deleted afterwards).
4. The fetched commit must equal `headRefOid`, else "PR #n changed while opening, try again".
5. `L` checked out → adopt. Missing → `git branch --no-track L <oid>`, `created`. Behind → fast-forward. At or ahead → keep. Diverged → fail "Branch L has commits not in PR #n".
6. Tracking: same repo with a live head → `branch --set-upstream-to=<remote>/<head> L`. Otherwise `branch.L.remote=<remote>`, `branch.L.merge=refs/pull/<n>/head`, like `gh pr checkout`. Git then refuses a plain `git push` (upstream name differs, `push.default=simple`), so a fork's PR can't be pushed to the wrong place.
7. `git worktree add -q -- <path> L`. Any failure after 5 calls `Rollback`: `branch -D L` only if this open created it and it still points at `oid`.

A merged or closed PR still opens, with note "PR #n is merged".

gh failures (`ghFailure`): not on PATH → "Install GitHub CLI (gh) to open pull requests"; stderr mentions `gh auth login` → "Run gh auth login to open pull requests"; "Could not resolve"/"no pull requests found" → "No pull request <ref>"; timeout → "GitHub didn't answer in 15s"; else the first stderr line.

`launch.go` `checkout()` gets one branch per mode before the existing `New` path, reusing `StepFetch`, `StepWorktree`, `StepCopy`, `StepSetup`. Adopting skips copy and setup and launches in the adopted path.

### Desktop (`pk/modals/new_session.rs`, `picker.rs`)

- `Draft` gains `source: Source { New, Branch, Pr }` and `remote_branches`. A "Start from" chip beside the base chip picks it; shown only with cap `open.v1`.
- Existing branch: the name field becomes the branch; the branch picker lists local branches, then `origin/*` without a local twin (`git::remote_branches`, loaded in the task that loads `branches`). Picking `origin/x` sends `x`. The base chip hides.
- Pull request: the field takes `123`, `#123` or a PR URL; `parse_pr` checks the form. Base chip hides.
- `Draft::spec` sends `{"branch": b}` or `{"pr": ref}` with `copy`/`setup`, no `name`. The provisional path is `<worktrees dir>/<b with / as ->` (or `pr-<n>`); `Creates::started` already swaps in pocketd's real cwd.

## 3. Delete with teardown and branch (desktop)

### Config

`store::RepoConfig` gains `teardown: String` (serde default). Project settings (`pk/modals/add_project.rs`) gets "When a worktree is deleted" under the setup field. pocketd ignores it.

### Model

- `git::lost_commits(tree: &str, branch: Option<&str>) -> usize`: commits nothing else holds.
  - Branch: `git rev-list --count refs/heads/<b> --not --exclude=<b> --branches --remotes --tags`. (`--exclude=refs/heads/<b>` matches nothing and always counts 0.)
  - Detached: `git rev-list --count HEAD --not --branches --remotes --tags` in the tree.
- `git::delete_branch(repo, branch)`: `git branch -D -- <b>`.
- `daemon::run_script(script, cwd, timeout) -> Result<String, String>`: login shell `-l -c script` in its own process group, killed on timeout ("Timed out after 2m"). Err carries the output's last lines.
- `git worktree remove --force` already succeeds when the folder is gone, so no prune path.

### Feature module `pk/removal.rs`

```rust
pub(crate) struct Removals { running: HashSet<String> }  // trees being deleted
pub(crate) struct Removal { project, tree, branch: Option<String>, delete_branch: bool, teardown: bool }
fn needs_confirm(dirty: usize, terminals: usize, lost: usize) -> bool
```

Flow, `impl Desktop` thin:

1. Row menu keeps "Delete worktree" and adds "Delete worktree and branch…" when the tree has a branch that isn't the project's base, `main` or `master`.
2. Ask: a background task counts dirty files and, when deleting the branch, lost commits (detached trees always count). Confirm only when `needs_confirm` (dirty, terminals, or lost commits), as today. `Confirm::DeleteWorktree` gains `lost` and `delete_branch`.
3. Run: mark `running` (the row shows "Deleting…"); background: teardown (if set and the folder exists) → `remove_worktree` → `delete_branch` if asked. Teardown runs first because scripts like `docker compose down` need the folder.
4. Teardown fails: stop, unmark, show `Confirm::TeardownFailed { removal, tail }` with "Delete anyway", which reruns without teardown.
5. Branch delete fails after removal: the worktree stays deleted; `error` says "Deleted the worktree; kept branch b: <reason>".
6. Success: today's cleanup (close terminals, drop workspace and layout, `refresh_git`).

## 4. PR status and Create PR (desktop)

### Model `d/git/src/git/github.rs`

```rust
pub struct Pr { pub number: u32, pub title: String, pub url: String, pub state: PrState, pub draft: bool, pub checks: Checks }
pub enum PrState { Open, Merged, Closed }
#[derive(Default)] pub struct Checks { pub passed: u16, pub failed: u16, pub pending: u16 }
pub enum GhProblem { Missing, LoggedOut }
pub const VIEW: &[&str] = &["gh", "pr", "view", "--json", "number,title,url,state,isDraft,statusCheckRollup"];
pub fn view_result(out: Result<String, String>) -> Result<Option<Pr>, GhProblem>  // Err(other) → Ok(None)
pub fn create_argv(base: &str) -> Vec<String>  // gh pr create --fill --base <base without origin/>
```

`gh pr view` without a number, in the worktree, resolves the PR from the branch's upstream, so pushed branches and fork PR checkouts both work. Checks: CheckRun `conclusion` FAILURE/TIMED_OUT/CANCELLED/ACTION_REQUIRED/STARTUP_FAILURE or StatusContext FAILURE/ERROR → failed; CheckRun not COMPLETED or StatusContext PENDING/EXPECTED → pending; else passed. `serde_json` joins git's deps.

### Feature module `pk/git_ui/pull_requests.rs`

```rust
pub(crate) struct PullRequests {
    by_tree: HashMap<String, (Instant, Option<Pr>)>,
    running: Option<String>,
    pub problem: Option<GhProblem>,
}
fn due(&self, selected: Option<&str>, trees: &[String], now: Instant) -> Option<String>
fn apply(&mut self, tree: String, r: Result<Option<Pr>, GhProblem>, now: Instant) -> bool  // changed
fn stale(&mut self, tree: &str)
```

No timer: each `refresh_git` result calls `due` for the selected project's non-main, non-detached trees. One gh at a time; the selected tree is due after 30s, others after 5min; nothing is due once `problem` is set (until restart, or Create PR clears it). `apply` returns whether anything changed; `cx.notify()` only then.

### UI

- Sidebar worktree row: `#123` after the name, toned failed red, pending amber, passed green, merged purple, draft/closed muted. Tooltip: title and counts.
- Changes header: the same chip; click opens the URL.
- Changes more menu: "Open pull request" when there is one; "Create pull request" when gh works, there is none and the branch isn't the base; a disabled hint when gh is missing or logged out.

### Create PR

Background task: `run_login(git::PUSH)` (sets upstream via `push.autoSetupRemote`), then `run_login(create_argv(base))`; open the URL from the last stdout line; mark the tree stale. Errors go to `changes.error`.

## 5. Phasing

Each step builds, passes clippy and tests on its own.

1. Open existing branch: wire + cap + goldens, `open.go`, sheet source chip, `remote_branches`.
2. Open PR: `pr.go`, `pr` field, sheet PR mode.
3. Delete: `teardown` config + field, `lost_commits`, `delete_branch`, `run_script`, `removal.rs`, row menu.
4. PR status: `github.rs`, `pull_requests.rs`, row and header chips.
5. Create PR: menu action.

## 6. Tests

Beside the code, real temp repos, no mocks, no real gh.

- Go `open_test.go`: local branch, remote-only branch tracks origin, behind is fast-forwarded, diverged kept, adopts a worktree on the branch, adopts main, a deleted worktree folder no longer blocks; `Folder` replaces slashes and skips taken folders.
- Go `pr_test.go` (bare remote with `refs/pull/7/head`, PR given as data): same-repo PR tracks its head, fork PR lands on `pr/7` tracking the pull ref, deleted head falls back to the pull ref, OID mismatch refuses, diverged local refuses, rollback deletes only a branch it created; `parseView` and `ghFailure` from captured output.
- Go `launch_test.go`: decode rejects two of base/branch/pr; name optional only for branch/pr.
- Rust git: lost commits for merged, pushed, local-only, detached; `delete_branch` after removal; `view_result`, check rollup, gh missing vs logged out.
- Rust daemon: `run_script` returns a failing script's output; kills one that runs too long.
- Rust pocket: `parse_pr`, spec for branch/PR mode, `needs_confirm`, `due` order and backoff, `apply` reports no change for the same PR.
- TS: golden round-trip.

## Synthesis note (arena)

- Candidates: C1 (opus), C2 (fable), C3 (sonnet). No dropouts. Cross-judge (opus): C1 27, C3 27, C2 21; base C1. My scoring agreed: C1 had the correct git and gh behaviour and the smallest surface.
- Base: C1 (fields on `NewWorktree`, adopt/fast-forward/refuse rules, private PR ref + OID check, rollback, fork tracking via `refs/pull/N/head`, correct lost-commit query, `github.rs` in the git crate).
- Grafts: C3 — pocketd alone names the folder (no Rust twin of `Folder`), provisional path swapped by `Creates::started`; a separate "Delete worktree and branch…" item keeping clean deletes instant; `removal.rs` state; `due()` driven by `refresh_git` instead of a timer; `pr/<n>` for fork branches. C2 — `pr` as a string ref, so URLs go to gh as is and a wrong-repo URL fails at the remote match.
- Rejected: C1's always-confirm dialog (changes today's instant delete); C1 `teardown` in Go `registry.Repo` (unused); C1 `missing`/prune path (verified `worktree remove --force` handles a missing folder); C3's `--exclude=refs/heads/<b>` (verified: always 0); C3 remote/ambiguity params and new error codes (origin and URL matching suffice); C3 push guard (verified git refuses the push); C2/C3 gh-optional PR open and `gh --version` probe (more paths for little gain); C2 `pocketd worktree open` CLI and ADR (unrequested).
- Verified on temp repos: lost-commit query (2 before merge, 0 after; C3 form 0), fork push refused with `branch.L.merge=refs/pull/N/head` even with `push.autoSetupRemote`, `worktree remove --force` on a missing folder exits 0.
