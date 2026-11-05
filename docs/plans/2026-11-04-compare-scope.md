# Compare Scope Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** Give the Changes panel a Compare scope (issue #173): everything a worktree changed against its base, a branch, a tag, a commit or another worktree, read-only, with Direct and Include uncommitted toggles.

**Architecture:** The range logic (which refs, which `git diff`, parsing) is a new `git::compare` module tested against scratch repos. Each worktree's scope is saved in `store` (`Store.scopes`, keyed by worktree path). `pocket/src/git_ui/compare.rs` owns `CompareState` (the comparison on screen, the ref menu) with its decisions in plain functions; the Changes panel draws a scope bar and, in Compare, commit and file rows instead of the commit box. A file opens as a new `Doc::CompareFile` tab diffed through a new `At::Compare(Range)`.

**Toolset (from `packages/desktop`):**
- One crate's tests: `cargo test -p git compare`, `cargo test -p store`, `cargo test -p pocket changes`
- Everything: `cargo build --workspace`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`
- Screens: `.ui-review/fixture/capture.sh <dir> changes=changes` (from the repo root)

**Read first:**
- `CLAUDE.md` and `docs/adr/0003-desktop-code-layout.md`: model crates hold logic, `pocket` holds views, state per feature, tests beside the logic.
- `packages/desktop/crates/git/src/git.rs`: `read`, `base_of`, `ahead_behind`, `blobs_between`, `commit_files`, and the scratch-repo tests.
- `packages/desktop/crates/pocket/src/git_ui/changes.rs` (+ `changes/header.rs`, `changes/rows.rs`): rows, tree, sections.
- `packages/desktop/crates/pocket/src/git_ui/diff.rs`: `At`, `DiffState::apply`, `read_diff`.

## Decisions (answered for the issue's open questions)

- **Narrowed after trying it:** Compare picks only branches (the base, local and remote) and lists only the files changed since the worktree left that branch (`<branch>...HEAD`). Worktrees, typed tags and SHAs, the commit lists, ahead/behind counts and the Direct and Include uncommitted toggles were dropped; the tasks below still describe them.
- **Include uncommitted** is off by default: Compare starts as the PR shows the branch; Uncommitted already shows the rest.
- **Worktree against worktree** compares the other worktree's branch (its HEAD), not its working tree: stable while that agent works. Worktrees on no branch (detached) aren't offered.
- **Tags and SHAs** are typed into the menu's search; it offers "Compare with `<typed>`". A ref that names no commit falls back to the base with a notice.
- **Direct / Include uncommitted** sit at the bottom of the ref menu as check rows, so the header stays one row plus a slim scope bar.
- **Review comments** pin to a Compare diff only when it is PR-shaped (not Direct, not Include uncommitted); **Add to chat** works in every Compare diff, since its new side is this worktree's file.
- **Phone** and the **multi-file continuous diff** (idea 06-4) are later issues.
- `CONTEXT.md` gains **Compare**.

---

## PR 1: Compare scope in the Changes panel

**Scope:** All of issue #173 on the desktop. Not the phone.
**Depends on:** nothing
**Done when:** `cargo test --workspace` is green, clippy adds no warnings, and the Changes panel switches between Uncommitted and Compare against base / worktree / branch / typed ref, opening compare diffs in tabs.

### Task 1.1: `git::compare` — the range's files, commits and blobs

**Files:**
- Create: `packages/desktop/crates/git/src/compare.rs`
- Modify: `packages/desktop/crates/git/src/git.rs` (add `old_path` to `FileStat`, `pub mod compare;`, extract `totals`, `untracked_files`, `changed_files`)
- Modify (add `old_path: None` to `FileStat` literals): `crates/pocket/src/explorer.rs:335`, `crates/pocket/src/sidebar/column.rs:80`, `crates/pocket/src/modals/confirm.rs:397`, `crates/pocket/src/git_ui/changes.rs:393`, `crates/pocket/src/git_ui/pull_request.rs:775`

**Context:** `commit_files` already parses `--name-status -z -M`; extract its loop as `changed_files(out) -> Vec<CommitFile>` and call it from there. `read` already counts untracked files; extract that as `untracked_files(run, cwd) -> Vec<FileStat>`. Child modules see the crate root's private helpers (`git`, `git_bytes`, `lines`), so they stay private. `git diff -z --numstat -M` prints `added\tremoved\tpath\0`, or for a rename `added\tremoved\t\0old\0new\0`.

**Step 1: Write the failing tests** (bottom of `compare.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn sh(repo: &Path, args: &[&str]) {
        let ok = Command::new("git").arg("-C").arg(repo).args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).output().unwrap().status.success();
        assert!(ok, "git {args:?}");
    }

    /// main with `a`, then `feature` off it changing `a` and adding `b`, while main gains `c`; on `feature`.
    fn forked(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pocket-compare-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        sh(&dir, &["init", "-q", "-b", "main"]);
        std::fs::write(dir.join("a"), "a\n").unwrap();
        sh(&dir, &["add", "."]);
        sh(&dir, &["commit", "-qm", "init"]);
        sh(&dir, &["checkout", "-qb", "feature"]);
        std::fs::write(dir.join("a"), "changed\n").unwrap();
        std::fs::write(dir.join("b"), "b\n").unwrap();
        sh(&dir, &["add", "."]);
        sh(&dir, &["commit", "-qm", "feature work"]);
        sh(&dir, &["checkout", "-q", "main"]);
        std::fs::write(dir.join("c"), "c\n").unwrap();
        sh(&dir, &["add", "."]);
        sh(&dir, &["commit", "-qm", "main work"]);
        sh(&dir, &["checkout", "-q", "feature"]);
        dir
    }

    fn range(against: &str, direct: bool, working: bool) -> Range {
        Range { against: against.into(), direct, working }
    }

    fn files(c: &Comparison) -> Vec<(&str, char, usize, usize)> {
        c.files.iter().map(|f| (f.path.as_str(), f.status, f.added, f.removed)).collect()
    }

    fn subjects(commits: &[Commit]) -> Vec<&str> {
        commits.iter().map(|c| c.subject.as_str()).collect()
    }

    #[test]
    fn a_compare_shows_what_the_branch_changed_since_it_left_the_ref() {
        let dir = forked("since");
        let d = dir.to_str().unwrap();
        let c = read(&Runner::Local, d, &range("main", false, false), true).unwrap();
        assert_eq!(files(&c), [("a", 'M', 1, 1), ("b", 'A', 1, 0)]);
        assert_eq!((subjects(&c.ahead), c.behind.len(), c.counts), (vec!["feature work"], 0, (1, 1)));
        assert_eq!(c.ahead[0].sha.len(), 40, "full shas, for the commit view");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_direct_compare_diffs_the_two_tips_and_lists_what_only_the_ref_has() {
        let dir = forked("direct");
        let d = dir.to_str().unwrap();
        let c = read(&Runner::Local, d, &range("main", true, false), true).unwrap();
        assert_eq!(files(&c), [("a", 'M', 1, 1), ("b", 'A', 1, 0), ("c", 'D', 0, 1)]);
        assert_eq!(subjects(&c.behind), ["main work"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn including_uncommitted_takes_the_working_tree_untracked_files_too() {
        let dir = forked("working");
        let d = dir.to_str().unwrap();
        std::fs::write(dir.join("b"), "b\nmore\n").unwrap();
        std::fs::write(dir.join("new"), "n\n").unwrap();
        let paths = |working, untracked| read(&Runner::Local, d, &range("main", false, working), untracked).unwrap().files.into_iter().map(|f| (f.path, f.added)).collect::<Vec<_>>();
        assert_eq!(paths(false, true), [("a".to_string(), 1), ("b".into(), 1)]);
        assert_eq!(paths(true, true), [("a".to_string(), 1), ("b".into(), 2), ("new".into(), 1)]);
        assert_eq!(paths(true, false), [("a".to_string(), 1), ("b".into(), 2)]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_renamed_file_shows_its_old_path_and_diffs_from_it() {
        let dir = forked("rename");
        let d = dir.to_str().unwrap();
        std::fs::write(dir.join("a"), "one\ntwo\nthree\nfour\n").unwrap();
        sh(&dir, &["commit", "-qam", "grow a"]);
        let base = range("main", false, false);
        sh(&dir, &["mv", "a", "z"]);
        sh(&dir, &["commit", "-qm", "rename"]);
        let c = read(&Runner::Local, d, &base, true).unwrap();
        let z = c.files.iter().find(|f| f.path == "z").unwrap();
        assert_eq!((z.status, z.old_path.as_deref()), ('R', Some("a")));
        assert_eq!(blobs(&Runner::Local, d, &base, "z"), (b"a\n".to_vec(), b"one\ntwo\nthree\nfour\n".to_vec()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn blobs_read_the_ref_s_side_and_head_or_the_working_tree() {
        let dir = forked("blobs");
        let d = dir.to_str().unwrap();
        std::fs::write(dir.join("a"), "uncommitted\n").unwrap();
        assert_eq!(blobs(&Runner::Local, d, &range("main", false, false), "a"), (b"a\n".to_vec(), b"changed\n".to_vec()));
        assert_eq!(blobs(&Runner::Local, d, &range("main", false, true), "a"), (b"a\n".to_vec(), b"uncommitted\n".to_vec()));
        assert_eq!(blobs(&Runner::Local, d, &range("main", true, false), "c"), (b"c\n".to_vec(), Vec::new()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_ref_that_names_no_commit_reads_as_none() {
        let dir = forked("gone");
        assert_eq!(read(&Runner::Local, dir.to_str().unwrap(), &range("deleted-branch", false, false), true), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn numstat_reads_renames_by_their_new_path() {
        let counts = numstat_z("1\t2\ta\0-\t-\timg.png\0003\t0\t\0old\0new\0");
        assert_eq!((counts["a"], counts["img.png"], counts["new"]), ((1, 2), (0, 0), (3, 0)));
    }
}
```

**Step 2: Run** `cargo test -p git compare` → FAIL (module missing).

**Step 3: Implement**

In `git.rs`: add `/// The path it had before a rename. pub old_path: Option<String>,` to `FileStat` (`None` in both `read` literals); add `pub mod compare;`; replace `Repo::totals`'s body with `totals(&self.files)` and add

```rust
pub fn totals(files: &[FileStat]) -> (usize, usize) {
    files.iter().fold((0, 0), |(a, r), f| (a + f.added, r + f.removed))
}

/// Files git doesn't track, each counted as added in full.
fn untracked_files(run: &Runner, cwd: &str) -> Vec<FileStat> { /* the loop moved out of `read` */ }

/// The files `git diff -z -M --name-status` (or `diff-tree`) lists; `old_path` is set for renames and copies.
fn changed_files(out: &str) -> Vec<CommitFile> { /* the loop moved out of `commit_files` */ }
```

`compare.rs`:

```rust
//! What a worktree changed against another ref: the Compare scope of the Changes panel.

use crate::{Blobs, Commit, FileStat, MAX_READ, Runner, ahead_behind, changed_files, git, git_bytes, lines, totals, untracked_files};
use std::collections::HashMap;
use std::path::Path;

/// The most commits a Compare lists on each side.
const MAX_COMMITS: usize = 200;

/// What a Compare diffs: HEAD from where it left `against` (`<against>...HEAD`, as a PR shows it), or with `direct` from `against`'s tip (`<against>..HEAD`); with `working` the working tree in place of HEAD.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Range {
    pub against: String,
    pub direct: bool,
    pub working: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Comparison {
    pub files: Vec<FileStat>,
    /// Commits HEAD has that `against` doesn't, newest first.
    pub ahead: Vec<Commit>,
    /// Commits `against` has that HEAD doesn't, newest first; read only for a direct Compare.
    pub behind: Vec<Commit>,
    /// How many commits each side has that the other doesn't: (ahead, behind).
    pub counts: (usize, usize),
}

impl Comparison {
    pub fn totals(&self) -> (usize, usize) {
        totals(&self.files)
    }
}

/// The commit `range` diffs from: `against` itself when direct, else where HEAD left it, or `against` when they share no history.
fn old_side(run: &Runner, cwd: &str, range: &Range) -> String {
    let fork = if range.direct { None } else { git(run, cwd, &["merge-base", &range.against, "HEAD"]) };
    fork.map_or_else(|| range.against.clone(), |f| f.trim().to_string())
}

fn diff<'a>(old: &'a str, working: bool, format: &'a str) -> Vec<&'a str> {
    let mut args = vec!["diff", "-z", "-M", format, old];
    if !working {
        args.push("HEAD");
    }
    args
}

/// What `range` shows in `cwd`, untracked files only with `untracked` and `range.working`; `None` when `range.against` names no commit.
pub fn read(run: &Runner, cwd: &str, range: &Range, untracked: bool) -> Option<Comparison> {
    git(run, cwd, &["rev-parse", "--verify", "--quiet", &format!("{}^{{commit}}", range.against)])?;
    let old = old_side(run, cwd, range);
    let counts = numstat_z(&git(run, cwd, &diff(&old, range.working, "--numstat")).unwrap_or_default());
    let mut files: Vec<FileStat> = changed_files(&git(run, cwd, &diff(&old, range.working, "--name-status")).unwrap_or_default())
        .into_iter()
        .map(|f| {
            let (added, removed) = counts.get(&f.path).copied().unwrap_or_default();
            FileStat { path: f.path, old_path: f.old_path, added, removed, staged: false, unstaged: false, status: f.status }
        })
        .collect();
    if range.working && untracked {
        files.extend(untracked_files(run, cwd));
    }
    let behind = if range.direct { commits(run, cwd, &format!("HEAD..{}", range.against)) } else { Vec::new() };
    let ahead = commits(run, cwd, &format!("{}..HEAD", range.against));
    Some(Comparison { files, ahead, behind, counts: ahead_behind(run, cwd, &range.against) })
}

/// `path` on each side of `range`, the old side read from its name before a rename.
pub fn blobs(run: &Runner, cwd: &str, range: &Range, path: &str) -> Blobs {
    let old = old_side(run, cwd, range);
    let show = |p: &str| git_bytes(run, cwd, &["show", &format!("{old}:{p}")]);
    let renamed_from = || changed_files(&git(run, cwd, &diff(&old, range.working, "--name-status")).unwrap_or_default()).into_iter().find(|f| f.path == path)?.old_path;
    let before = show(path).or_else(|| show(&renamed_from()?)).unwrap_or_default();
    let after = if range.working {
        run.read(&Path::new(cwd).join(path).to_string_lossy(), MAX_READ).unwrap_or_default()
    } else {
        git_bytes(run, cwd, &["show", &format!("HEAD:{path}")]).unwrap_or_default()
    };
    (before, after)
}

fn commits(run: &Runner, cwd: &str, range: &str) -> Vec<Commit> {
    let n = MAX_COMMITS.to_string();
    lines(git(run, cwd, &["log", "--format=%H%x00%s", "-n", &n, range, "--"]))
        .into_iter()
        .filter_map(|l| l.split_once('\0').map(|(sha, subject)| Commit { sha: sha.into(), subject: subject.into() }))
        .collect()
}

/// Lines added and removed by path, from `git diff -z --numstat`; a binary file counts none.
fn numstat_z(out: &str) -> HashMap<String, (usize, usize)> {
    let mut tokens = out.split('\0');
    let mut counts = HashMap::new();
    while let Some(t) = tokens.next() {
        let mut parts = t.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next()) else { continue };
        // A rename leaves the path empty and gives the old then the new one as the next two tokens.
        let path = if path.is_empty() { tokens.nth(1).unwrap_or_default() } else { path };
        counts.insert(path.to_string(), (added.parse().unwrap_or(0), removed.parse().unwrap_or(0)));
    }
    counts
}
```

**Step 4: Run** `cargo test -p git` → PASS (old tests too).

### Task 1.2: `store::scope` — each worktree's scope

**Files:**
- Create: `packages/desktop/crates/store/src/scope.rs`
- Modify: `packages/desktop/crates/store/src/store.rs` (`pub mod scope;`, the `scopes` field, a test)

**Step 1: Test** (in `store.rs` tests)

```rust
#[test]
fn a_worktrees_compare_scope_is_remembered() {
    let mut store = Store::default();
    let scope = scope::Scope { compare: true, against: scope::Against::Worktree("/w/other".into()), direct: true, working: false };
    store.scopes.insert("/w/fix".into(), scope.clone());
    let back: Store = serde_json::from_slice(&store.encode_for_test()).unwrap();
    assert_eq!(back.scopes["/w/fix"], scope);
    assert!(serde_json::from_str::<Store>("{\"scopes\": 3}").unwrap().scopes.is_empty(), "an unreadable scope list reads as none");
}
```

(Use `serde_json::to_vec(&store)` if there's no helper.)

**Step 3: Implement** `scope.rs`:

```rust
use serde::{Deserialize, Serialize};

/// What a worktree's Changes panel shows: its uncommitted changes, or with `compare` everything it changed against `against`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Scope {
    pub compare: bool,
    pub against: Against,
    /// Diffs the two tips instead of from where the worktree left the ref.
    pub direct: bool,
    /// Takes the working tree, uncommitted files included, as the new side instead of HEAD.
    pub working: bool,
}

/// The ref a Compare is against.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Against {
    /// The branch the worktree was created from, as `git::read` finds it.
    #[default]
    Base,
    /// Another worktree of the project, by its path, at its branch.
    Worktree(String),
    /// A branch, remote branch, tag or commit.
    Ref(String),
}
```

`Store` field: `/// Each worktree's Changes scope, keyed by the worktree's path. #[serde(deserialize_with = "lenient")] pub scopes: BTreeMap<String, scope::Scope>,`

**Step 4:** `cargo test -p store` → PASS.

### Task 1.3: `Doc::CompareFile` and `At::Compare` — the diff tab

**Files:**
- Modify: `crates/workspace/src/workspace.rs` (`Doc::CompareFile { against: String, direct: bool, working: bool, path: String }` + doc comment)
- Modify: `crates/pocket/src/git_ui/diff.rs` (`At::Compare(git::compare::Range)`, `read_diff`, crumbs `vs <ref>`, meta `against <ref>`, `DiffView::worktree_file`, `draft_quote`, `pin` filter, tests)
- Modify: `crates/pocket/src/git_ui/diff/row.rs:106` (`if v.worktree_file().is_none()`)
- Modify: `crates/pocket/src/add_to_chat.rs:230` (match `Doc::CompareFile { .. }` too; quotable via `worktree_file()`)
- Modify: `crates/pocket/src/desktop.rs:388-420` (`show_doc`, `loaded` arms)
- Modify: `crates/pocket/src/desktop/project.rs:188` (refresh re-reads every pane whose `worktree_file()` is set, with its own `At`)
- Modify the `Doc` matches: `panels.rs:343`, `panels/tab_actions.rs:62`, `panels/top_tabs.rs:112`, `panels/strip.rs:139,166` (label `"<name> (vs <ref>)"`), `modals/more.rs:14`

**Context:** `worktree_file()` is the file shown when its new side is this worktree's (Working or Compare): such a diff refreshes with git and quotes into a chat. Threads pin to `At::Base` and to a Compare that is neither Direct nor Include uncommitted (`At::pins_threads`).

**Tests** (in `diff.rs`):

```rust
fn compare(direct: bool, working: bool) -> At {
    At::Compare(git::compare::Range { against: "origin/main".into(), direct, working })
}

#[test]
fn threads_pin_to_a_compare_only_when_it_diffs_as_the_pr_does() {
    let thread = Thread { id: "t".into(), path: "a.rs".into(), line: Some(3), ..Thread::default() };
    for (at, pinned) in [(compare(false, false), true), (compare(true, false), false), (compare(false, true), false)] {
        let mut state = DiffState::default();
        state.select(MAIN, "a.rs".into(), at.clone());
        state.apply(MAIN, DiffLoad { at, ..load("a.rs", DIFF) });
        state.pin(std::slice::from_ref(&thread));
        assert_eq!(!state.view(MAIN).unwrap().pins.is_empty(), pinned);
    }
}

#[test]
fn a_compare_diff_quotes_its_pick_into_a_chat() {
    let mut state = DiffState::default();
    state.select(MAIN, "a.rs".into(), compare(true, true));
    state.apply(MAIN, DiffLoad { at: compare(true, true), ..load("a.rs", DIFF) });
    state.panes.get_mut(&MAIN).unwrap().pick = Pick { range: Some((3, 3)), dragging: false, composing: true };
    assert_eq!(state.draft_quote().map(|q| q.path), Some("a.rs".to_string()));
}
```

Run `cargo test -p pocket diff` → PASS after the change.

### Task 1.4: `git_ui/compare.rs` — state and decisions

**Files:**
- Create: `packages/desktop/crates/pocket/src/git_ui/compare.rs`
- Modify: `crates/pocket/src/git_ui.rs` (`pub(crate) mod compare;`), `crates/pocket/src/desktop.rs` (compose `CompareState` in `Desktop::new`; `menu_open` / `close_menus` include `compare.menu`), `crates/pocket/src/desktop/project.rs` (`refresh_git` ends with `d.load_compare(cx)`)

**Context:** `CompareState` holds the search `Entity`, so its decisions live on the plain `View` inside it and in free functions:
- `resolve(against, base, worktrees) -> Option<String>`: the ref an `Against` names; `None` with no base, or for a worktree no longer listed or on no branch.
- `View::land(cwd, range, read, scope) -> bool`: shows a read; when the ref named no commit and the scope isn't on the base, points the scope back at the base, sets the notice, and returns `true` to read again.
- `choices(base, worktrees, branches, remote, search) -> Vec<Choice>`: the menu's refs (base, the project's other worktrees by sidebar name with branch, then branches via `modals::new_session::listed_branches`, minus worktree branches; a typed row when the search names no listed ref).
- `switch_label(scope, range, counts)`: `Uncommitted`, `vs <ref>`, or `vs <ref> · N ahead · M behind`.

Desktop glue (thin): `scope(cwd)`, `set_scope(scope, cx)`, `load_compare(cx)` (resolve, fall back for a vanished worktree, spawn `git::compare::read` off the UI thread, `land` only if the cwd and range still hold), `open_compare_menu(window, cx)` (loads branches in the background), `pick_compare(choice, cx)`, `open_compare_file(path, pin, cx)`.

**Tests** (bottom of `compare.rs`): `resolve` for each `Against`; `land` shows a read, falls back once for a missing ref, and keeps the base when the base itself is missing; `choices` order, filtering, dedupe and typed row; `switch_label` texts.

### Task 1.5: The Changes panel in Compare

**Files:**
- Create: `crates/pocket/src/git_ui/changes/scope.rs` (scope bar, ref menu)
- Modify: `crates/pocket/src/git_ui/changes.rs` (`Section::{Ahead, Behind, Files}`, `Row::Commit`, `push_files` shared by `change_rows` and new `compare_rows`, `changes_list` branch per scope, tests)
- Modify: `crates/pocket/src/git_ui/changes/rows.rs` (no actions for Compare sections, section labels, `commit_row`, compare file rows: `+a −r`, `old → new`, opens `Doc::CompareFile`)

**Tests** (in `changes.rs`): `compare_rows_list_commits_then_only_in_the_ref_then_the_file_tree` and `a_folded_commit_section_keeps_its_header`.

### Task 1.6: Docs

- `CONTEXT.md`: **Compare** entry.
- `docs/orchestrators/product/01-feature-matrix.md` §1.7 "Branch-changes scope + ref picker": Pocket column ✓ (#173).

### Final checks

From `packages/desktop`: `cargo build --workspace`, `cargo clippy --workspace --all-targets` (no new warnings), `cargo test --workspace`. Capture the Changes panel before and after.
