# Team Mode 4: Running a Team Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** `team.start`, `team.stop` and `team.merge` in pocketd: workers launch in their own worktrees under the lead, every normal turn end hands back to the lead, asks the plan allows are answered without the user, the team merges into the lead's checkout, and after a restart every team picks up where it was left.

**Architecture:** Git work is plain functions in `internal/worktree/merge.go`, tested against temp repos. Every team decision is a plain function over `proto.Team` in `internal/team/run.go` (`Started`, `Starting`, `Free`, `Handed`, `Verdict`, `Merged`, …). The glue is **plan 3's `team.Teams`, extended, not a second engine**: `internal/team/running.go` adds methods to it (`Start`, `Stop`, `Merge`, `TurnEnded`, `Told`, `Allow`) and event kinds (`turn`, `told`) to its `Run` queue. Every change still goes through plan 3's `Teams.change` (one mutex; save, then `team.update`), so plan 3's planning and this plan's running never write a team at the same time. Two new hooks feed it: `agent.Registry.OnTurnEnd` (a turn ended, normally or not) and `broker.Broker.Allow` (answer an ask before anyone sees it). An e2e test with fake claude runs a two-task plan end to end. After a restart, `Teams.Recover` (`recovering.go`) gathers what came back and applies `Recovered` (`recover.go`), once `daemon.Daemon.WaitRestored` says the restored sessions have settled.

**Toolset (from `packages/pocketd` unless noted):**
- One package's tests: `go test ./internal/worktree -run TestMerge -count=1` (swap the package and `-run`)
- e2e: `go test ./e2e -run TestATeam -count=1 -v` (once per machine first: `scripts/build-ghostty.sh` from the repo root, for the cgo terminal library)
- Rewrite proto goldens: `go test ./internal/proto -update` (this plan writes client goldens by hand; none need `-update`)
- Everything: `go vet ./... && go test ./...`
- TS protocol (from the repo root): `pnpm --filter @pocket/protocol test`
- Swift wire (iOS decodes every pocketd golden): `cd packages/ios/Packages/AnywhereKit && PATH="$HOME/.swiftly/bin:$PATH" swift test --filter GoldenTests`

**Read first:**
- `docs/plans/2026-11-05-team-mode.md`: the decisions and contracts. This plan implements its plan 4 row.
- `docs/spike-orchestrator-team.md`, findings 1–4: why a task with several `after` gets a merged base, why the lead's verdict gates Merge, why notes say "The user told", and that workers share `.git`.
- `CLAUDE.md`: decisions in plain functions, glue thin, tests beside the logic, named as sentences.
- `packages/pocketd/internal/worktree/create.go`: `Prepare`/`Plan.Fetch`/`Plan.Add`/`Remove`, and `coded` errors.
- `packages/pocketd/internal/launch/launch.go:103-215`: `Create`, and how a chat agent gets its first prompt.
- `packages/pocketd/internal/agent/agent.go:293-390`: status, `TurnEnded`, `Clear`, `Stop`.
- `packages/pocketd/internal/broker/broker.go`: `Ask`. Claude's `can_use_tool` reaches it through `daemon.decide` (`internal/daemon/daemon.go:251`); Codex's `item/commandExecution/requestApproval` through `codex.Session.ask` (`internal/codex/session.go:494`).
- `packages/pocketd/cmd/pocketd/serve.go:204-212`: the automation `Starter`, the precedent for pocketd launching as the owner.
- `packages/pocketd/internal/daemon/restore.go`: `hint`, `expect`, `finish` (PR 4 waits for every restored agent to settle).

## What plans 1–3 give this plan

This plan runs after plans 1, 2 and 3 merge, and uses exactly what they add:

| From | What | Used by |
|---|---|---|
| plan 1 | `proto.LaunchSpec.Parent` (launch records it on the agent) | `WorkerSpec` |
| plan 2 | `func (d *daemon.Daemon) Mail(agentID, text string) error`; e2e `stop`, `unacked` (`e2e/mail_test.go`) | `Teams.Mail`, Task 4.1 |
| plan 3 | `proto.Team`, `proto.TeamTask`, `proto.NewTeamUpdate`, `proto.CapTeam`, client/server team goldens, Swift `Sources/Wire/Team.swift`, TS `packages/protocol/src/team.ts` (`TeamClient`) | Tasks 2.2, 3.2 |
| plan 3 | `team.Teams` (`internal/team/teams.go`): fields `Store`, `Agents`, `Hub`, `Poll`; `change(lead, f)` (the only writer); `post(event)`, `Run`, `handle`, `turnEnd`, `closed`, `Ended`; `send` (prompts a lead, records `since`); `errOff`; the `event` struct (`kind`, `agent`, `from`, `to`) | `running.go` |
| plan 3 | `team.ValidateKept`, `team.Order`, `team.Startable`, `team.Allowed`, `team.Branch`, `team.Reply`; `reads`, `turnItems`, `Teams.awaits`; `agent.Agent.SetAwaiting`; test helpers `tk`, `plan`, `with`, `writes`, `in`, `ids`, `twoTasks`, `fixture` in `internal/team` | `run.go`, tests |
| plan 3 | `serve.go`: `teams` (nil when `state/team.db` can't open), `teams.Status` in `OnStatus`, `l.Teams = teams`, `go teams.Run(ctx)`; `wsserver.Server.Teams`; the `agent.prompt` case with `m.Team` | Tasks 2.5, 3.4 |
| plan 3 | e2e: `Message.Team`/`Teams`, `teamOf(lead, state)`, `chatClaude` | Task 2.1 |

Names this plan adds to `internal/team` (none exist yet): `Started`, `Base`, `Branches`, `Free`, `WorkerSpec`, `Assignment`, `Starting`, `Launched`, `WorkerStatus`, `Failed`, `FailedHandback`, `Find`, `Allows`, `LastText`, `Handed`, `Handback`, `Ready`, `Verdict`, `Stopped`, `Note`, `ConflictNote`, `Merging`, `MergeOrder`, `Merged`, `Conflicted`, `MergeFailed`, `taskAt`, `editTask`, `inTurn`, `TurnID`, `Recovered`, `Facts`, `Worker`, `Recovery`, `WorkerRestarted`, `LeadRestarted`, `MergeRestarted`; on `Teams`: `Start`, `Stop`, `Merge`, `TurnEnded`, `Told`, `Allow`, `work`, `review`, `pump`, `taken`, `launch`, `spec`, `handback`, `commit`, `fail`, `tell`, `lastText`, `leadCwd`, `merge`, `cleanUp`, `Recover`, `pickUp`, `facts`, `back`, `lastTurn`; test helpers `testLead`, `planOfThree`, `startedPlan`, `t1Working`, `reviewedPair`. In `e2e`: `taskIn`, `replyTo`, `gitOut`, `gitIdentity`, `teamPlan`, `reviewedTeam`, `askingPlan`, `leadsTeam`. Elsewhere: `worktree.MergeInProgress`, `worktree.AbortMerge`, `daemon.Daemon.WaitRestored`.

## Decisions taken here (beyond the index)

- **A worker's branch takes the first free name.** `team.Branch(lead, task)` is the preferred name. When a task starts, pocketd picks the first of `name`, `name-2`, `name-3`, … that is neither a branch (local or remote) nor a folder in the project's worktrees folder (`worktree.Taken`, compared as `worktree.Validate` does), whose `<name>-base` is free too, and that no other task of the team holds. It is stored in `TeamTask.Branch` (and the worktree, named the same, in `TeamTask.Worktree`), and everything after reads the stored name: a dependent's base, the `-base` branch, the stat, the merge. A lead's second team after a merge kept the first one's branches, so its `t1` becomes `team-<lead6>-t1-2`.
- **`team.start` refuses to drop a task a kept task lists in `after`**: `Task t3 starts from t1's work; keep t1 or remove t3 too`. Plan 6's sheet already prevents it. The kept tasks must pass `ValidateKept`: every rule of the lead's plan, but 1 to 4 tasks, so you can start just one.
- **A task with no `after` starts from the lead's HEAD commit**, not its branch name: `Plan.Fetch` would otherwise start it from `origin/<branch>` when origin is ahead, which isn't what the lead planned against.
- **The assignment goes out after the worker is on record.** `WorkerSpec` leaves `Prompt` empty; `Teams` records `Worker` and only then sends `Assignment(task)` with `Driver().Prompt`, exactly as `launch.create` would. Otherwise a fast turn could end before `Teams` knows whose it is. The worker is named after its task's title.
- **The `-base` branch is made without a checkout**: `git merge-tree --write-tree` + `git commit-tree` (git ≥ 2.38).
- **`Stat` diffs from the merge base** (`git diff --stat <base>...HEAD`), so a base that moved on (the lead committed, a dependency handed back again) adds nothing.
- **Worker commits skip hooks** (`--no-verify`): pocketd commits for an agent told not to, and a failing hook would lose the handback.
- **A turn ends "ok"** unless it failed or the user stopped it (`Agent.Stop` then `Clear`, for Claude and Codex). A failed or closed worker fails its task; a stopped one is ignored.
- **A failed task's reason goes in its `Summary`**: `TeamTask` has no error field.
- **A turn hands back once.** `TeamTask.Turn` holds the prompt id of the turn last handed back; a handback of that turn again (heard through `Run` and through restart recovery) does nothing.
- **A merge records its checkout** (`Team.Checkout`), so restart recovery can abort it without the lead.
- **"Ready to merge" doesn't count after "not" or "n't"**: the spike's lead said "Not ready to merge".
- **The verdict is read at every ok turn end of the lead while in review**, so asking the lead again updates it.
- **Merge refuses tracked changes only** (`git status --untracked-files=no`); untracked files don't block a merge unless git itself refuses.
- **A full merge closes the workers' sessions** before removing their worktrees; branches stay.
- **A restart is reconciled, not replayed.** Once restored sessions settle, a worker that is back takes its status, or hands back if its last turn ended ok; one that is gone fails its task (`the worker stopped when pocketd restarted`) and the lead is mailed the failed handback. A merge in progress in the team's recorded checkout (`Team.Checkout`) is aborted, whether or not the lead came back (`conflict`, `Error: pocketd restarted during the merge`); a `merging` team with none goes back to `review` with that `Error`. A planning team whose lead is gone fails; a lead back and idle has its turn read again. `Run` handles nothing once shutdown starts, so closing sessions doesn't fail tasks.
- **`team.start` tasks:** `id` and `agent` are required; `model` and `allow` are optional (absent `allow` is an empty allowlist).

---

## PR 1: Git helpers and the two hooks

**Scope:** `worktree.Head`, `Commit`, `Stat`, `MergeBranches`, `Dirty`, `Merge`, `Conflict`, `ErrDirty`; `agent.Registry.OnTurnEnd`; `broker.Broker.Allow`. All inert: nothing calls them until PR 2.
**Depends on:** plans 1–3 merged
**Done when:** `go test ./internal/worktree ./internal/agent ./internal/broker -count=1` and `go test ./...` are green.

### Task 1.1: Commit a worktree, read its head and its diff stat

**Files:**
- Create: `packages/pocketd/internal/worktree/merge.go`
- Test: `packages/pocketd/internal/worktree/merge_test.go`

**Context:** `internal/worktree/list_test.go` already has `TestMain` (pins `GIT_CONFIG_GLOBAL=/dev/null` and a git identity for every git the code runs), `git(t, dir, args...)` (runs git, fails the test on error, returns trimmed output) and `repo(t)` (a fresh repo on `main` with one empty commit, at its real path). Use them; don't add another `TestMain`. The package's own commands use `exec.Command("git", "-C", …)` with the process environment; `errors.AsType` is already used in `create.go`.

**Step 1: Write the failing tests** (`merge_test.go`)

```go
package worktree

import (
	"os"
	"path/filepath"
	"testing"
)

// commitFile writes body to name in dir and commits it.
func commitFile(t *testing.T, dir, name, body string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(dir, name), []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
	git(t, dir, "add", name)
	git(t, dir, "commit", "-q", "-m", name)
}

func TestHeadIsTheCommitTheCheckoutIsOn(t *testing.T) {
	p := repo(t)
	if got, err := Head(p); err != nil || got != git(t, p, "rev-parse", "HEAD") {
		t.Fatalf("got %q, %v", got, err)
	}
}

func TestCommitTakesEveryChangeWithoutRunningHooks(t *testing.T) {
	p := repo(t)
	commitFile(t, p, "kept.txt", "a\n")
	os.WriteFile(filepath.Join(p, "kept.txt"), []byte("b\n"), 0o644)
	os.WriteFile(filepath.Join(p, "new.txt"), []byte("n\n"), 0o644)
	os.WriteFile(filepath.Join(p, ".git", "hooks", "pre-commit"), []byte("#!/bin/sh\nexit 1\n"), 0o755)
	committed, err := Commit(p, "team t1: Add new")
	if err != nil || !committed {
		t.Fatalf("committed %t, %v", committed, err)
	}
	if got := git(t, p, "show", "--name-only", "--format=%s", "HEAD"); got != "team t1: Add new\n\nkept.txt\nnew.txt" {
		t.Fatalf("got %q", got)
	}
}

func TestCommitSkipsACleanCheckout(t *testing.T) {
	p := repo(t)
	before := git(t, p, "rev-parse", "HEAD")
	committed, err := Commit(p, "nothing")
	if err != nil || committed || git(t, p, "rev-parse", "HEAD") != before {
		t.Fatalf("committed %t, %v", committed, err)
	}
}

func TestStatIsWhatTheBranchChangedSinceItLeftItsBase(t *testing.T) {
	p := repo(t)
	git(t, p, "checkout", "-q", "-b", "work")
	commitFile(t, p, "a.txt", "one\ntwo\n")
	git(t, p, "checkout", "-q", "main")
	commitFile(t, p, "later.txt", "x\n")
	git(t, p, "checkout", "-q", "work")
	got, err := Stat(p, "main")
	if want := " a.txt | 2 ++\n 1 file changed, 2 insertions(+)"; err != nil || got != want {
		t.Fatalf("got %q, %v", got, err)
	}
}
```

What each proves: `Head` is the checkout's commit; `Commit` takes modified and untracked files and ignores a failing hook; a clean checkout makes no commit; `Stat` leaves out what the base gained after the branch left it.

**Step 2: Run the tests to verify they fail**

Run: `go test ./internal/worktree -run 'TestHead|TestCommit|TestStat' -count=1`
Expected: FAIL, build error `undefined: Head` (and `Commit`, `Stat`)

**Step 3: Write the implementation** (`merge.go`)

```go
package worktree

import (
	"errors"
	"os/exec"
	"strings"
)

// gitIn runs git in dir and returns its output; its error is git's own message.
func gitIn(dir string, args ...string) (string, error) {
	out, err := exec.Command("git", append([]string{"-C", dir}, args...)...).Output()
	if exit, ok := errors.AsType[*exec.ExitError](err); ok {
		return string(out), errors.New(strings.TrimSpace(string(exit.Stderr)))
	}
	return string(out), err
}

// Head is the commit dir's checkout is on.
func Head(dir string) (string, error) {
	out, err := gitIn(dir, "rev-parse", "HEAD")
	return strings.TrimSpace(out), err
}

// Commit commits everything in dir's checkout, untracked files too, and says
// whether there was anything to commit. Hooks don't run: pocketd commits for
// an agent that was told not to, and a failing hook would lose its work.
func Commit(dir, message string) (bool, error) {
	status, err := gitIn(dir, "status", "--porcelain")
	if err != nil || strings.TrimSpace(status) == "" {
		return false, err
	}
	if _, err := gitIn(dir, "add", "-A"); err != nil {
		return false, err
	}
	if _, err := gitIn(dir, "commit", "-q", "--no-verify", "-m", message); err != nil {
		return false, err
	}
	return true, nil
}

// Stat is `git diff --stat` of what dir's branch changed since it left base,
// counted from their merge base so base moving on since adds nothing.
func Stat(dir, base string) (string, error) {
	out, err := gitIn(dir, "diff", "--stat", base+"...HEAD")
	return strings.TrimRight(out, "\n"), err
}
```

**Step 4: Run the tests to verify they pass**

Run: `go test ./internal/worktree -run 'TestHead|TestCommit|TestStat' -count=1`
Expected: PASS

### Task 1.2: Make a base branch from several task branches

**Files:**
- Modify: `packages/pocketd/internal/worktree/merge.go`
- Test: `packages/pocketd/internal/worktree/merge_test.go`

**Context:** A task with several `after` starts from `team-<lead6>-<task>-base`: its dependencies' branches merged in order. No checkout may move, so each merge is computed with `git merge-tree --write-tree --name-only --no-messages <a> <b>` and recorded with `git commit-tree <tree> -p <a> -p <b>`. `merge-tree` prints the tree id on its first line, then one conflicted path per line, and exits 1 on a conflict. It also exits 1 with nothing on stdout for a bad ref, so a conflict is "exit 1 and more than one line".

**Step 1: Write the failing tests** (append to `merge_test.go`; add `"errors"` and `"reflect"` to its imports)

```go
// branchWith makes branch off main in p holding name=body, and leaves p on main.
func branchWith(t *testing.T, p, branch, name, body string) {
	t.Helper()
	git(t, p, "checkout", "-q", "-b", branch, "main")
	commitFile(t, p, name, body)
	git(t, p, "checkout", "-q", "main")
}

func TestMergeBranchesMakesABranchHoldingEachOfThemWithoutACheckout(t *testing.T) {
	p := repo(t)
	branchWith(t, p, "t1", "a.txt", "a\n")
	branchWith(t, p, "t2", "b.txt", "b\n")
	if err := MergeBranches(p, "t3-base", []string{"t1", "t2"}); err != nil {
		t.Fatal(err)
	}
	if got := git(t, p, "ls-tree", "--name-only", "t3-base"); got != "a.txt\nb.txt" {
		t.Fatalf("tree %q", got)
	}
	if got := git(t, p, "log", "-1", "--format=%s", "t3-base"); got != "Merge t2" {
		t.Fatalf("subject %q", got)
	}
	if branch, status := git(t, p, "branch", "--show-current"), git(t, p, "status", "--porcelain"); branch != "main" || status != "" {
		t.Fatalf("checkout moved: %q %q", branch, status)
	}
}

func TestMergeBranchesNamesTheFilesThatConflictAndMakesNoBranch(t *testing.T) {
	p := repo(t)
	branchWith(t, p, "t1", "same.txt", "one\n")
	branchWith(t, p, "t2", "same.txt", "two\n")
	err := MergeBranches(p, "t3-base", []string{"t1", "t2"})
	if c, ok := errors.AsType[*Conflict](err); !ok || !reflect.DeepEqual(*c, Conflict{Branch: "t2", Files: []string{"same.txt"}}) {
		t.Fatalf("got %v", err)
	}
	if got := git(t, p, "branch", "--list", "t3-base"); got != "" {
		t.Fatalf("made %q", got)
	}
}
```

**Step 2: Run the tests to verify they fail**

Run: `go test ./internal/worktree -run TestMergeBranches -count=1`
Expected: FAIL, build error `undefined: MergeBranches` and `undefined: Conflict`

**Step 3: Write the implementation** (append to `merge.go`)

```go
// Conflict is a merge of Branch that git stopped on, and the files it conflicted in.
type Conflict struct {
	Branch string
	Files  []string
}

func (c *Conflict) Error() string {
	return "Merging " + c.Branch + " conflicted in: " + strings.Join(c.Files, ", ")
}

// MergeBranches makes branch name in project from branches merged in order,
// without touching a checkout. A conflict is a *Conflict, and makes no branch.
func MergeBranches(project, name string, branches []string) error {
	out, err := gitIn(project, "rev-parse", "--verify", branches[0]+"^{commit}")
	if err != nil {
		return err
	}
	head := strings.TrimSpace(out)
	for _, b := range branches[1:] {
		out, err := gitIn(project, "merge-tree", "--write-tree", "--name-only", "--no-messages", head, b)
		lines := strings.Split(strings.TrimSpace(out), "\n")
		if err != nil && len(lines) > 1 {
			return &Conflict{Branch: b, Files: lines[1:]}
		}
		if err != nil {
			return err
		}
		if out, err = gitIn(project, "commit-tree", lines[0], "-p", head, "-p", b, "-m", "Merge "+b); err != nil {
			return err
		}
		head = strings.TrimSpace(out)
	}
	_, err = gitIn(project, "branch", name, head)
	return err
}
```

**Step 4: Run the tests to verify they pass**

Run: `go test ./internal/worktree -run TestMergeBranches -count=1`
Expected: PASS

### Task 1.3: Merge task branches into the lead's checkout

**Files:**
- Modify: `packages/pocketd/internal/worktree/merge.go`
- Test: `packages/pocketd/internal/worktree/merge_test.go`

**Context:** Merge runs `git merge --no-ff` per branch in the order given, into whatever branch the checkout is on. It refuses a checkout with uncommitted changes to tracked files. On a conflict it runs `git merge --abort`, stops, and reports the branch and files; the branches merged before it stay merged. `coded(code, message)` in `create.go` makes an error with a wire code (`Code(err)` reads it).

**Step 1: Write the failing tests** (append to `merge_test.go`)

```go
func TestMergeMergesEachBranchWithItsOwnMergeCommitInOrder(t *testing.T) {
	p := repo(t)
	branchWith(t, p, "t1", "a.txt", "a\n")
	git(t, p, "checkout", "-q", "-b", "t2", "t1")
	commitFile(t, p, "b.txt", "b\n")
	git(t, p, "checkout", "-q", "main")
	merged, err := Merge(p, []string{"t1", "t2"})
	if err != nil || !reflect.DeepEqual(merged, []string{"t1", "t2"}) {
		t.Fatalf("merged %v, %v", merged, err)
	}
	if got := git(t, p, "log", "--merges", "--format=%s"); got != "Merge t2\nMerge t1" {
		t.Fatalf("merges %q", got)
	}
}

func TestMergeRefusesUncommittedChangesButNotUntrackedFiles(t *testing.T) {
	p := repo(t)
	commitFile(t, p, "a.txt", "a\n")
	branchWith(t, p, "t1", "b.txt", "b\n")
	os.WriteFile(filepath.Join(p, "a.txt"), []byte("edited\n"), 0o644)
	if merged, err := Merge(p, []string{"t1"}); err != ErrDirty || merged != nil {
		t.Fatalf("merged %v, %v", merged, err)
	}
	os.WriteFile(filepath.Join(p, "a.txt"), []byte("a\n"), 0o644)
	os.WriteFile(filepath.Join(p, "scratch.txt"), []byte("x\n"), 0o644)
	if merged, err := Merge(p, []string{"t1"}); err != nil || !reflect.DeepEqual(merged, []string{"t1"}) {
		t.Fatalf("merged %v, %v", merged, err)
	}
}

func TestAConflictAbortsThatMergeAndKeepsTheOnesBefore(t *testing.T) {
	p := repo(t)
	branchWith(t, p, "t1", "a.txt", "a\n")
	branchWith(t, p, "t2", "same.txt", "two\n")
	commitFile(t, p, "same.txt", "main\n")
	merged, err := Merge(p, []string{"t1", "t2"})
	c, ok := errors.AsType[*Conflict](err)
	if !ok || !reflect.DeepEqual(*c, Conflict{Branch: "t2", Files: []string{"same.txt"}}) || !reflect.DeepEqual(merged, []string{"t1"}) {
		t.Fatalf("merged %v, %v", merged, err)
	}
	if got := git(t, p, "status", "--porcelain"); got != "" {
		t.Fatalf("merge left in progress: %q", got)
	}
	if got := git(t, p, "log", "--merges", "--format=%s"); got != "Merge t1" {
		t.Fatalf("merges %q", got)
	}
}
```

**Step 2: Run the tests to verify they fail**

Run: `go test ./internal/worktree -run 'TestMerge|TestAConflict' -count=1`
Expected: FAIL, build error `undefined: Merge` and `undefined: ErrDirty`

**Step 3: Write the implementation** (append to `merge.go`)

```go
// ErrDirty refuses a merge into a checkout with uncommitted changes.
var ErrDirty = coded("dirty_checkout", "This checkout has uncommitted changes; commit or stash them first")

// Dirty is whether dir's checkout has uncommitted changes to tracked files.
func Dirty(dir string) (bool, error) {
	out, err := gitIn(dir, "status", "--porcelain", "--untracked-files=no")
	return strings.TrimSpace(out) != "", err
}

// Merge merges branches into dir's checkout in order, each with its own merge
// commit, and returns those it merged. A conflict aborts that merge and stops
// there: the error is a *Conflict.
func Merge(dir string, branches []string) ([]string, error) {
	dirty, err := Dirty(dir)
	if err != nil {
		return nil, err
	}
	if dirty {
		return nil, ErrDirty
	}
	var merged []string
	for _, b := range branches {
		if _, err := gitIn(dir, "merge", "-q", "--no-ff", "--no-edit", "-m", "Merge "+b, b); err != nil {
			files, _ := gitIn(dir, "diff", "--name-only", "--diff-filter=U")
			gitIn(dir, "merge", "--abort")
			if files = strings.TrimSpace(files); files != "" {
				return merged, &Conflict{Branch: b, Files: strings.Split(files, "\n")}
			}
			return merged, err
		}
		merged = append(merged, b)
	}
	return merged, nil
}
```

**Step 4: Run the tests to verify they pass**

Run: `go test ./internal/worktree -count=1`
Expected: PASS (the whole package, including `TestRemoveKeepsTheBranchAndRefusesTheMainWorktree`, which already pins that `Remove` keeps the branch; PR 3 reuses `Remove`)

### Task 1.4: `agent.Registry.OnTurnEnd`

**Files:**
- Modify: `packages/pocketd/internal/agent/agent.go` (`Registry` struct ~line 110; `turnEnded` ~line 364; `Clear` ~line 379)
- Test: `packages/pocketd/internal/agent/agent_test.go`

**Context:** Status alone can't tell how a turn ended: a seen agent ends at `idle` whether it finished or was stopped. Every turn end passes through `turnEnded` (normal or failed; also a held turn's late end) or `Clear` (the user stopped it; both Claude's and Codex's `Interrupt` call `Agent.Stop()` then `daemon.cleared`, which calls `Clear`). A Codex turn the user stopped can also end through `turnEnded(false)` when its thread goes idle first, so "ok" also needs `!a.stopped` (`Stop()` sets it, `Working()` clears it at the next turn). Both run inside `a.update`, under the agent's lock, like `OnStatus`.

**Step 1: Write the failing test** (append to `agent_test.go`; add `"strconv"` to its imports)

```go
func TestOnTurnEndSaysWhetherEachTurnEndedNormally(t *testing.T) {
	reg := NewRegistry(hub.New())
	var got []string
	reg.OnTurnEnd = func(id string, ok bool) { got = append(got, id+" "+strconv.FormatBool(ok)) }
	a := reg.Add("a1", "/w", "claude", fakeDriver{})
	a.Working()
	a.TurnEnded(false)
	a.Working()
	a.TurnEnded(true)
	a.Working()
	a.Stop()
	a.Clear()
	a.Working()
	a.Stop()
	a.TurnEnded(false)
	a.TurnEnded(false)
	want := []string{"a1 true", "a1 false", "a1 false", "a1 false"}
	if strings.Join(got, "|") != strings.Join(want, "|") {
		t.Fatalf("%q", got)
	}
}
```

It proves: a finished turn is ok; a failed one isn't; a stop is not ok whether it ends through `Clear` or the provider's own end; an end with no turn running reports nothing.

**Step 2: Run the test to verify it fails**

Run: `go test ./internal/agent -run TestOnTurnEnd -count=1`
Expected: FAIL, build error `reg.OnTurnEnd undefined (type *Registry has no field or method OnTurnEnd)`

**Step 3: Write the implementation**

In `Registry`, after `OnStatus`:

```go
	// OnTurnEnd runs under the agent's lock when a turn ends: ok unless it
	// failed or the user stopped it. Like OnStatus, it must stay short.
	OnTurnEnd func(id string, ok bool)
```

Add after `NewRegistry`:

```go
func (r *Registry) turnEnd(id string, ok bool) {
	if r.OnTurnEnd != nil {
		r.OnTurnEnd(id, ok)
	}
}
```

In `turnEnded`, the last block becomes:

```go
	if a.phase != "idle" {
		a.enter("idle")
		a.unseenEnd, a.failed = !a.seen, failed
		a.endCompactFromIdle()
		a.reg.turnEnd(a.id, !failed && !a.stopped)
	}
```

In `Clear`, the inner block becomes:

```go
		if a.phase != "idle" {
			a.enter("idle")
			a.unseenEnd = false
			a.endCompactFromIdle()
			a.reg.turnEnd(a.id, false)
		}
```

**Step 4: Run the tests to verify they pass**

Run: `go test ./internal/agent -count=1`
Expected: PASS

### Task 1.5: `broker.Broker.Allow`

**Files:**
- Modify: `packages/pocketd/internal/broker/broker.go` (`Broker` struct ~line 34; `Ask` ~line 56)
- Test: `packages/pocketd/internal/broker/broker_test.go`

**Context:** Both providers' tool asks end in `Broker.Ask`: Claude's `can_use_tool` through `daemon.decide` (tool `Bash`, `Detail.Kind == "shell"`, `Detail.Command`), Codex's command approval through `codex.Session.ask` (tool `shell`, same detail). A hook at the top of `Ask` answers an allowlisted command before it is published, so no client and no push sees it. The test file's `ask(b, ctx, agentID, key)` asks for `Bash` `ls`; `types(ch)` lists what the hub published.

**Step 1: Write the failing tests** (append to `broker_test.go`)

```go
func TestAnAskTheAllowHookTakesIsAllowedWithoutBeingShown(t *testing.T) {
	h := hub.New()
	msgs, _ := h.Subscribe()
	b := New(h)
	b.Allow = func(req proto.PermissionRequest) bool { return req.Detail.Command == "ls" }
	if got := <-ask(b, context.Background(), "a1", "k"); got != "allow" {
		t.Fatalf("got %q", got)
	}
	if got := types(msgs); len(got) != 0 {
		t.Fatalf("published %v", got)
	}
}

func TestAnAskTheAllowHookLeavesIsShownAsBefore(t *testing.T) {
	b := New(hub.New())
	b.Allow = func(proto.PermissionRequest) bool { return false }
	got := ask(b, context.Background(), "a1", "k")
	req := waitOpen(t, b, 1)[0]
	b.Resolve(req.RequestID, Answer{Decision: "deny"})
	if d := <-got; d != "deny" {
		t.Fatalf("got %q", d)
	}
}
```

**Step 2: Run the tests to verify they fail**

Run: `go test ./internal/broker -run TestAnAskTheAllowHook -count=1`
Expected: FAIL, build error `b.Allow undefined (type *Broker has no field or method Allow)`

**Step 3: Write the implementation**

In `Broker`, after `OnAsk`:

```go
	// Allow runs first on each ask; true answers it allow without showing it
	// to anyone. Set it before the first Ask.
	Allow func(proto.PermissionRequest) bool
```

At the top of `Ask`:

```go
	if b.Allow != nil && b.Allow(req) {
		return Answer{Decision: "allow"}
	}
```

**Step 4: Run the tests to verify they pass**

Run: `go test ./internal/broker -count=1 && go test ./... `
Expected: PASS

---

## PR 2: Start a team, hand back, answer allowed asks

**Scope:** `team.start` on the wire (Go, TS and Swift); `team.start` refuses to drop a task a kept one starts from; workers launch from the right base as the owner under the lead, each on the first free branch name; worker status shows on its task; a normal turn end commits, hands back to the lead and starts what is now startable; the last handback moves the team to review; the lead's verdict sets `Ready`; asks the plan allows are answered with a timeline notice. Not `team.stop`, notes or `team.merge` (PR 3).
**Depends on:** PR 1
**Done when:** `go test ./e2e -run TestATeamsWorkersHandBack -count=1` passes, and `go test ./...`, `pnpm --filter @pocket/protocol test` and the Swift `GoldenTests` are green.

### Task 2.1: The e2e test, and fake claude doing an assignment

**Files:**
- Modify: `packages/pocketd/e2e/fakeclaude/chat.go` (doc comment; `turn` switch; new `assignment`)
- Create: `packages/pocketd/e2e/team_run_test.go`

**Context:** Fake claude (`e2e/fakeclaude/chat.go`) runs each user line as a turn and echoes it: `echo: <text>`. A lead's team prompt is wrapped by plan 3 with the rules, which contain an example plan block; the echo ends with the test's own block, and `team.Parse` takes the last one, so the echo *is* the plan. A worker's first prompt is the assignment, so fake claude learns to act on it: each line `write <path> <text>` writes `<text>\n` to the path in its folder, each line `run <cmd>` asks `can_use_tool` as `run` does, then it echoes the assignment. Only a prompt that starts with `<anywhere_team_assignment` does this, so a lead echoing a handback writes nothing. "Ready to merge" sits in both tasks' instructions, so whichever lead turn is read in review carries it. pocketd's own `git commit`/`git merge` need an identity: `gitIdentity` adds one to pocketd's environment. Helpers you reuse: `Start`, `launchReady`, `h.Owner`, `create`, `inRepo`, `reply` (`launch_test.go`), `WaitFor`, `streamOf` (`phone_test.go`; plan 3 gave `Message` its `Team` field), `teamOf(lead, state)` (plan 3's `e2e/team_test.go`). `WaitFor` fails after 10s. `team.update` goes to every client, so `h.Owner()` needs no cap.

**Step 1: Teach fake claude assignments**

In `chat.go`'s doc comment, after the `agent <d>` line, add:

```go
//	<anywhere_team_assignment …>  do each "write <path> <text>" and "run <cmd>" line, then echo it
```

In `turn`'s `switch`, before `case strings.HasPrefix(text, "run "):`, add:

```go
	case strings.HasPrefix(text, "<anywhere_team_assignment"):
		failed = c.assignment(model, text)
```

Add `"path/filepath"` to `chat.go`'s imports and this method after `run`:

```go
// assignment does a team worker's instructions line by line: "write <path>
// <text>" writes text to path in its folder, "run <cmd>" runs cmd as run does.
// Then it echoes the assignment, its summary.
func (c *chatter) assignment(model, text string) (failed bool) {
	for _, line := range strings.Split(text, "\n") {
		switch {
		case strings.HasPrefix(line, "write "):
			path, body, _ := strings.Cut(strings.TrimPrefix(line, "write "), " ")
			if err := os.WriteFile(filepath.Join(c.s.cwd, path), []byte(body+"\n"), 0o644); err != nil {
				note("write %s: %v", path, err)
				return true
			}
		case strings.HasPrefix(line, "run "):
			c.next++
			if c.run(model, fmt.Sprintf("toolu_%d", c.next), strings.TrimPrefix(line, "run ")) {
				return true
			}
		}
	}
	c.reply(model, text)
	return false
}
```

**Step 2: Write the failing test** (`e2e/team_run_test.go`)

```go
//go:build unix

package e2e

import (
	"encoding/json"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

// gitIdentity lets pocketd commit and merge with no git config.
func gitIdentity(h *Harness) {
	h.Env = append(h.Env, "GIT_AUTHOR_NAME=e2e", "GIT_AUTHOR_EMAIL=e2e@x", "GIT_COMMITTER_NAME=e2e", "GIT_COMMITTER_EMAIL=e2e@x")
}

func gitOut(t *testing.T, dir string, args ...string) string {
	t.Helper()
	out, err := exec.Command("git", append([]string{"-C", dir}, args...)...).CombinedOutput()
	if err != nil {
		t.Fatalf("git %v: %v\n%s", args, err, out)
	}
	return strings.TrimSpace(string(out))
}

func taskIn(lead, id, state string) func(Message) bool {
	return func(m Message) bool {
		if m.Type != "team.update" || m.Team.Lead != lead {
			return false
		}
		for _, task := range m.Team.Tasks {
			if task.ID == id {
				return task.State == state
			}
		}
		return false
	}
}

func replyTo(id string) func(Message) bool {
	return func(m Message) bool { return m.ID == id && (m.Type == "ack" || m.Type == "error") }
}

// teamPlan is the lead's team prompt. Fake claude echoes it, so its block is
// the lead's plan: t1 writes a.txt and runs `make test`, which its allowlist
// lets through; t2 starts from t1's work and writes b.txt.
func teamPlan(t *testing.T) string {
	t.Helper()
	plan, err := json.Marshal(map[string]any{"tasks": []map[string]any{
		{"id": "t1", "title": "Write a", "agent": "claude", "files": []string{"a.txt"}, "after": []string{}, "allow": []string{"make test"},
			"instructions": "write a.txt one\nrun make test\nReady to merge"},
		{"id": "t2", "title": "Write b", "agent": "claude", "files": []string{"b.txt"}, "after": []string{"t1"}, "allow": []string{},
			"instructions": "write b.txt two\nReady to merge"},
	}})
	if err != nil {
		t.Fatal(err)
	}
	return "<anywhere_team_plan>\n" + string(plan) + "\n</anywhere_team_plan>"
}

// reviewedTeam has a chat lead plan teamPlan, starts it, and returns the team
// once both workers handed back and the lead said it is ready to merge.
func reviewedTeam(t *testing.T) (h *Harness, o *Phone, lead string, team proto.Team) {
	t.Helper()
	h = Start(t, launchReady(""), gitIdentity)
	o = h.Owner()
	create(o, "lead", inRepo(h, map[string]any{"chat": true}))
	created := reply(o, "agent.created", "error")
	if created.Type != "agent.created" {
		t.Fatalf("got %s", created.Raw)
	}
	lead = created.AgentID
	h.WaitScreen(created.TerminalID, "fake claude ready")
	o.Send(map[string]any{"type": "agent.prompt", "id": "plan", "agentId": lead, "text": teamPlan(t), "team": true})
	o.WaitFor("the proposed plan", teamOf(lead, "proposed"))
	o.Send(map[string]any{"type": "team.start", "id": "start", "lead": lead, "tasks": []map[string]any{
		{"id": "t1", "agent": "claude", "allow": []string{"make test"}},
		{"id": "t2", "agent": "claude", "allow": []string{}},
	}})
	if m := o.WaitFor("team.start's reply", replyTo("start")); m.Type != "ack" {
		t.Fatalf("team.start: %s", m.Raw)
	}
	asked, noticed := false, false
	o.WaitFor("t1's handback", func(m Message) bool {
		asked = asked || m.Type == "permission.request"
		noticed = noticed || m.Type == "agent.stream" && m.Item.Kind == "notice" && m.Item.Text == "Allowed by the team plan: make test"
		return taskIn(lead, "t1", "done")(m)
	})
	if asked || !noticed {
		t.Fatalf("make test: asked the user %t, noted in the timeline %t", asked, noticed)
	}
	o.WaitFor("the review", teamOf(lead, "review"))
	team = o.WaitFor("the lead's verdict", func(m Message) bool { return teamOf(lead, "review")(m) && m.Team.Ready }).Team
	return h, o, lead, team
}

func TestATeamsWorkersHandBackInOrderAndTheLeadReviews(t *testing.T) {
	h, _, lead, team := reviewedTeam(t)
	b1, b2 := "team-"+lead[:6]+"-t1", "team-"+lead[:6]+"-t2"
	t1, t2 := team.Tasks[0], team.Tasks[1]
	trees := filepath.Join(filepath.Dir(h.Repo), "wt")
	if t1.Branch != b1 || t2.Branch != b2 || t1.Worktree != filepath.Join(trees, b1) || t2.Worktree != filepath.Join(trees, b2) || t1.Worker == "" || t2.Worker == "" {
		t.Fatalf("t1 %+v\nt2 %+v", t1, t2)
	}
	if !strings.Contains(t1.Stat, "a.txt | 1 +") || !strings.Contains(t2.Stat, "b.txt | 1 +") || strings.Contains(t2.Stat, "a.txt") {
		t.Fatalf("stats:\n%s\n%s", t1.Stat, t2.Stat)
	}
	if !strings.HasPrefix(t1.Summary, `echo: <anywhere_team_assignment task="t1">`) {
		t.Fatalf("summary %q", t1.Summary)
	}
	if got := gitOut(t, h.Repo, "log", "-1", "--format=%s", b1); got != "team t1: Write a" {
		t.Fatalf("t1's commit %q", got)
	}
	if got := gitOut(t, h.Repo, "show", b2+":a.txt"); got != "one" {
		t.Fatalf("t2 didn't start from t1's work: %q", got)
	}
}
```

What it proves: the plan starts; t1 runs first and its `make test` is answered by the allowlist with a notice and no ask; t1 commits and hands back; t2 starts from t1's branch, so its stat shows only its own file; the team reaches review and the lead's verdict sets `Ready`.

**Step 3: Run the test to verify it fails**

Run: `go test ./e2e -run TestATeamsWorkersHandBack -count=1`
Expected: FAIL with `team.start: {"type":"error","id":"start","message":"Malformed message"}`

### Task 2.2: `team.start` on the wire

**Files:**
- Create: `packages/pocketd/internal/proto/teampick.go`
- Modify: `packages/pocketd/internal/proto/team.go` (plan 3's: `TeamTask` gains `Turn`)
- Modify: `packages/pocketd/internal/proto/messages.go` (`ClientMessage` fields; a `DecodeClient` case)
- Create: `packages/pocketd/internal/proto/testdata/golden/client/team_start.json`
- Modify: `packages/pocketd/internal/proto/golden_test.go` (`TestDecodeClientRejects` list)
- Modify: `packages/pocketd/internal/peer/check.go` (`Needs`), `packages/pocketd/internal/peer/check_test.go` (matrix)
- Modify: `packages/protocol/src/team.ts` (`TeamTask` gains `turn`; `TaskPick`; `TeamClient` gains `team.start`)
- Modify: `packages/ios/Packages/AnywhereKit/Sources/Wire/Team.swift` (`TeamTask` gains `turn`; `ClientMessage.TaskPick`, `ClientMessage.TeamStart`)
- Modify: `packages/ios/Packages/AnywhereKit/Sources/Wire/ClientMessage.swift` (the enum case, `init(from:)`, `encode(to:)`, `type`)

**Context:** Every client verb needs: a `DecodeClient` case, a `peer.Needs` entry (a verb missing there is refused for everyone), a matrix row in `check_test.go`, and a client golden that Go (`TestClientGolden`), TS (`packages/protocol/test/golden.test.mjs`, with excess properties as errors) and Swift (`Tests/WireTests/GoldenTests.swift`, which decodes every client golden into a `ClientMessage` and encodes it back to the same JSON) all decode. Plan 3 put the team schemas in `packages/protocol/src/team.ts` (`TeamClient`, spread into `ClientMessage`) and `Sources/Wire/Team.swift`; client payload structs live in `extension ClientMessage` and hold neither `type` nor `id`. `strictObject(raw, keys...)` (`proto/launch.go:157`) refuses unknown keys and nulls. `team.start` is Spawn, like `agent.create`: the owner and a phone with spawn may send it; legacy and PTY may not. `TeamTask` also gains `turn` (Task 2.4: the prompt id of the turn last handed back), optional and left off when empty, so no golden changes; TS and Swift mirror it so a team that carries it still decodes and round-trips.

**Step 1: Write the failing tests**

`testdata/golden/client/team_start.json`:

```json
{"type":"team.start","id":"tm1","lead":"5f0c2a9e-1b2c-4d3e-8f90-123456789abc","tasks":[{"id":"t1","agent":"claude","model":"sonnet","allow":["npm test"]},{"id":"t2","agent":"codex"}]}
```

In `TestDecodeClientRejects`, add before `` `null`, ``:

```go
		`{"type":"team.start","id":"1","lead":"a1"}`,
		`{"type":"team.start","id":"1","tasks":[]}`,
		`{"type":"team.start","id":"1","lead":"a1","tasks":null}`,
		`{"type":"team.start","id":"1","lead":"a1","tasks":[{"id":"t1"}]}`,
		`{"type":"team.start","id":"1","lead":"a1","tasks":[{"id":"t1","agent":"claude","files":["x"]}]}`,
		`{"type":"team.start","id":"1","lead":"a1","tasks":[{"id":"t1","agent":"claude","allow":"npm test"}]}`,
		`{"type":"team.start","id":"1","lead":"a1","tasks":[{"id":"t1","agent":"claude","model":null}]}`,
```

In `check_test.go`'s matrix, add:

```go
		"ws:team.start":         {"", "", "scope_denied", "scope_denied"},
```

**Step 2: Run the tests to verify they fail**

Run: `go test ./internal/proto ./internal/peer -count=1`
Expected: FAIL: `team_start.json: Malformed message` and `ws:team.start by principal 0: "scope_denied", want ""`

Run (repo root): `pnpm --filter @pocket/protocol test`
Expected: FAIL on `client/team_start.json`

Run: `cd packages/ios/Packages/AnywhereKit && PATH="$HOME/.swiftly/bin:$PATH" swift test --filter GoldenTests`
Expected: FAIL: `team_start.json` decodes with `unknown message type team.start`

**Step 3: Write the implementation**

`internal/proto/teampick.go`:

```go
package proto

import "encoding/json"

// TaskPick is a task kept at team.start, with the agent, model and allowlist
// the user left it. A task the plan has but the picks lack is removed.
type TaskPick struct {
	ID    string   `json:"id"`
	Agent string   `json:"agent"`
	Model string   `json:"model,omitempty"`
	Allow []string `json:"allow,omitempty"`
}

// decodePicks takes team.start's tasks: each an id and an agent, and maybe a
// model and an allowlist.
func decodePicks(raw json.RawMessage, dst *[]TaskPick) bool {
	var items []json.RawMessage
	if json.Unmarshal(raw, &items) != nil || items == nil {
		return false
	}
	picks := make([]TaskPick, len(items))
	for i, item := range items {
		f, ok := strictObject(item, "id", "agent", "model", "allow")
		p := &picks[i]
		if !ok || !fields(f).get("id", &p.ID) || !fields(f).get("agent", &p.Agent) ||
			!fields(f).optionalString("model", &p.Model) || !fields(f).optionalList("allow", &p.Allow) {
			return false
		}
	}
	*dst = picks
	return true
}
```

In `messages.go`, `ClientMessage` gains, after `Ended`:

```go
	// team.start's.
	Lead  string
	Tasks []TaskPick
```

and `DecodeClient` gains, before `default:`:

```go
	case m.Type == "team.start":
		ok = get("lead", &m.Lead) && decodePicks(f["tasks"], &m.Tasks)
```

In `internal/proto/team.go`, `TeamTask` gains after `Stat`:

```go
	Turn         string   `json:"turn,omitempty"`    // the prompt id of the worker's turn last handed back
```

In `team.ts`'s `TeamTask`, after `stat: Schema.optional(Schema.String),` add `turn: Schema.optional(Schema.String),`. In `Team.swift`'s `TeamTask`, after `public var stat: String?` add `public var turn: String?`.

In `peer/check.go`'s `Needs`, after `"ws:agent.create"`:

```go
	"ws:team.start":           Spawn,
```

In `packages/protocol/src/team.ts`, before `export const TeamClient`:

```ts
// A task kept at team.start, as the user left its agent, model and allowlist.
export const TaskPick = Schema.Struct({
  id: Schema.String,
  agent: ProviderId,
  model: Schema.optional(Schema.String),
  allow: Schema.optional(Schema.Array(Schema.String)),
});
export type TaskPick = typeof TaskPick.Type;
```

and replace `TeamClient` with:

```ts
export const TeamClient = [
  Schema.Struct({ type: Schema.Literal("team.list"), id: Schema.String }),
  Schema.Struct({ type: Schema.Literal("team.start"), id: Schema.String, lead: Schema.String, tasks: Schema.Array(TaskPick) }),
] as const;
```

In `Sources/Wire/Team.swift`, append:

```swift
extension ClientMessage {
    /// A task kept at team.start (desktop only).
    public struct TaskPick: Codable, Hashable, Sendable {
        public var id: String
        public var agent: ProviderId
        public var model: String?
        public var allow: [String]?
    }

    /// team.start: the lead and the tasks kept.
    public struct TeamStart: Codable, Hashable, Sendable {
        public var lead: String
        public var tasks: [TaskPick]
    }
}
```

In `ClientMessage.swift`: add `case teamStart(ClientMessage.TeamStart)` after `case teamList`; in `init(from:)` after the `"team.list"` line: `case "team.start": self = .teamStart(try ClientMessage.TeamStart(from: decoder))`; in `encode(to:)` after `case .teamList: break`: `case .teamStart(let m): try m.encode(to: encoder)`; in `type` after `case .teamList: "team.list"`: `case .teamStart: "team.start"`.

**Step 4: Run the tests to verify they pass**

Run: `gofmt -l ./internal && go test ./internal/proto ./internal/peer -count=1`
Expected: no files listed by gofmt; PASS

Run (repo root): `pnpm --filter @pocket/protocol test`
Expected: PASS

Run: `cd packages/ios/Packages/AnywhereKit && PATH="$HOME/.swiftly/bin:$PATH" swift test --filter GoldenTests`
Expected: PASS

### Task 2.3: Decisions for starting workers

**Files:**
- Create: `packages/pocketd/internal/team/run.go`
- Test: `packages/pocketd/internal/team/run_test.go`

**Context:** Every decision is a function from a `proto.Team` (and plain inputs) to a new `proto.Team`; no git, no agents. They copy `Tasks` before changing one (`editTask`), so the team passed in is never changed. Task states: `planned → waiting → running ⇄ needsYou → done | failed | stopped → merged`. `team.Branch(lead, id)` is `"team-" + lead[:6] + "-" + id` (plan 3), the name a task's branch and worktree try first; `Free` picks the first free one from a list of taken names, read the way `worktree.Validate` reads them (case aside; a branch `a/b` holds `a`; it returns `worktree.ErrExists` for a taken name). Once picked, a task's name lives in its `Branch`, and `Base`, `Branches` and `WorkerSpec` read it from there, never from `Branch(lead, id)`. `team.ValidateKept` (plan 3) refuses no task or more than 4, a missing `after`, an unknown agent, a cycle, and shared files without an order; unlike the lead's `Validate`, one task is enough. `Allowed(allow, command)` (plan 3) is the single-command prefix rule.

**Step 1: Write the failing tests** (`run_test.go`)

```go
package team

import (
	"reflect"
	"testing"

	"pocketd/internal/proto"
)

const testLead = "abcdef123456"

// planOfThree is a plan waiting for Start: t1 and t2 side by side, t3 after both.
func planOfThree() proto.Team {
	return proto.Team{Lead: testLead, State: "proposed", UpdatedAt: 1, Tasks: []proto.TeamTask{
		{ID: "t1", Title: "Add truncate", Agent: "claude", Files: []string{"src/truncate.js"}, After: []string{}, Allow: []string{"npm test"}, Instructions: "Write truncate.", State: "planned"},
		{ID: "t2", Title: "Add titleCase", Agent: "claude", Files: []string{"src/title.js"}, After: []string{}, Allow: []string{}, Instructions: "Write titleCase.", State: "planned"},
		{ID: "t3", Title: "Export both", Agent: "codex", Files: []string{"src/index.js", "README.md"}, After: []string{"t1", "t2"}, Allow: []string{}, Instructions: "Export them.", State: "planned"},
	}}
}

// startedPlan is planOfThree started as planned.
func startedPlan(t *testing.T) proto.Team {
	t.Helper()
	tm, err := Started(planOfThree(), []proto.TaskPick{{ID: "t1", Agent: "claude", Allow: []string{"npm test"}}, {ID: "t2", Agent: "claude"}, {ID: "t3", Agent: "codex"}})
	if err != nil {
		t.Fatal(err)
	}
	return tm
}

// t1Working is startedPlan with t1's worker w1 at work.
func t1Working(t *testing.T) proto.Team {
	t.Helper()
	return Launched(Starting(startedPlan(t), "t1", nil), "t1", "w1", "/wt/team-abcdef-t1")
}

func TestStartKeepsThePickedTasksWithTheirEditsInPlanOrder(t *testing.T) {
	got, err := Started(planOfThree(), []proto.TaskPick{
		{ID: "t2", Agent: "codex", Model: "gpt-6.1-sol", Allow: []string{"npm test", "npx eslint"}},
		{ID: "t1", Agent: "claude", Model: "sonnet"},
	})
	want := planOfThree()
	want.State, want.Tasks = "running", want.Tasks[:2]
	want.Tasks[0].Model, want.Tasks[0].Allow, want.Tasks[0].State = "sonnet", []string{}, "waiting"
	want.Tasks[1].Agent, want.Tasks[1].Model, want.Tasks[1].Allow, want.Tasks[1].State = "codex", "gpt-6.1-sol", []string{"npm test", "npx eslint"}, "waiting"
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v, %v\nwant %+v", got, err, want)
	}
}

func TestStartRefusesWhatTheUserCannotPick(t *testing.T) {
	notProposed := planOfThree()
	notProposed.State = "running"
	all := []proto.TaskPick{{ID: "t1", Agent: "claude"}, {ID: "t2", Agent: "claude"}, {ID: "t3", Agent: "codex"}}
	for name, c := range map[string]struct {
		team  proto.Team
		picks []proto.TaskPick
	}{
		"a team that isn't proposed":    {notProposed, all},
		"a task the plan doesn't have":  {planOfThree(), append(all[:2:2], proto.TaskPick{ID: "t9", Agent: "claude"})},
		"a task picked twice":           {planOfThree(), append(all[:3:3], proto.TaskPick{ID: "t1", Agent: "claude"})},
		"no task left":                  {planOfThree(), nil},
		"an agent pocketd can't run":    {planOfThree(), []proto.TaskPick{{ID: "t1", Agent: "gemini"}, all[1], all[2]}},
	} {
		if got, err := Started(c.team, c.picks); err == nil {
			t.Errorf("%s: started %+v", name, got)
		}
	}
}

func TestStartCanKeepASingleTask(t *testing.T) {
	got, err := Started(planOfThree(), []proto.TaskPick{{ID: "t2", Agent: "claude"}})
	if err != nil || got.State != "running" || len(got.Tasks) != 1 || got.Tasks[0].ID != "t2" || got.Tasks[0].State != "waiting" {
		t.Fatalf("got %+v, %v", got, err)
	}
}

func TestStartRefusesToDropATaskAKeptOneStartsFrom(t *testing.T) {
	_, err := Started(planOfThree(), []proto.TaskPick{{ID: "t2", Agent: "claude"}, {ID: "t3", Agent: "codex"}})
	if err == nil || err.Error() != "Task t3 starts from t1's work; keep t1 or remove t3 too" {
		t.Fatalf("got %v", err)
	}
}

func TestABranchTakesTheFirstNameThatIsFreeAndWhoseBaseIsFree(t *testing.T) {
	for _, c := range []struct {
		taken []string
		want  string
	}{
		{nil, "team-abcdef-t1"},
		{[]string{"main", "team-abcdef-t2"}, "team-abcdef-t1"},
		{[]string{"team-abcdef-t1"}, "team-abcdef-t1-2"},
		{[]string{"Team-ABCDEF-T1"}, "team-abcdef-t1-2"},
		{[]string{"team-abcdef-t1/old"}, "team-abcdef-t1-2"},
		{[]string{"team-abcdef-t1-base"}, "team-abcdef-t1-2"},
		{[]string{"team-abcdef-t1", "team-abcdef-t1-2-base", "team-abcdef-t1-3"}, "team-abcdef-t1-4"},
	} {
		if got := Free("team-abcdef-t1", c.taken); got != c.want {
			t.Errorf("%q: got %q, want %q", c.taken, got, c.want)
		}
	}
}

func TestATaskStartsFromTheLeadsHeadItsOneDependencyOrTheMergeOfSeveral(t *testing.T) {
	tm := planOfThree()
	tm.Tasks[0].Branch, tm.Tasks[1].Branch, tm.Tasks[2].Branch = "team-abcdef-t1-2", "team-abcdef-t2", "team-abcdef-t3"
	one := proto.TeamTask{ID: "t4", After: []string{"t1"}}
	got := []string{Base(tm, tm.Tasks[0], "c0ffee"), Base(tm, one, "c0ffee"), Base(tm, tm.Tasks[2], "c0ffee")}
	if want := []string{"c0ffee", "team-abcdef-t1-2", "team-abcdef-t3-base"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("got %q", got)
	}
	if got, want := Branches(tm, tm.Tasks[2].After), []string{"team-abcdef-t1-2", "team-abcdef-t2"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("got %q", got)
	}
}

func TestAWorkerLaunchesAsAnEditingChatInItsOwnWorktreeUnderTheLead(t *testing.T) {
	task := planOfThree().Tasks[2]
	task.Model, task.Branch = "gpt-6.1-sol", "team-abcdef-t3-2"
	got := WorkerSpec("/repo", testLead, task, "team-abcdef-t3-2-base")
	want := proto.LaunchSpec{Project: "/repo", Checkout: proto.Checkout{New: &proto.NewWorktree{Name: "team-abcdef-t3-2", Base: "team-abcdef-t3-2-base"}},
		Provider: "codex", Model: "gpt-6.1-sol", Access: "edits", Chat: true, Parent: testLead}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
}

func TestTheAssignmentGivesTheWorkerItsTaskAndTheFilesItMayChange(t *testing.T) {
	want := `<anywhere_team_assignment task="t3">
You're one worker in a team that a lead agent planned. Your task: Export both.
Export them.
Change only these files: src/index.js, README.md. Don't commit, and don't create branches or worktrees.
When you're done, reply with a short summary of what you changed and how you checked it.
</anywhere_team_assignment>`
	if got := Assignment(planOfThree().Tasks[2]); got != want {
		t.Fatalf("got\n%s", got)
	}
}

func TestAStartingTaskRunsOnItsBranchAndThenNamesItsWorker(t *testing.T) {
	before := startedPlan(t)
	got := Launched(Starting(before, "t1", nil), "t1", "w1", "/wt/team-abcdef-t1")
	want := startedPlan(t)
	want.Tasks[0].State, want.Tasks[0].Branch, want.Tasks[0].Worker, want.Tasks[0].Worktree = "running", "team-abcdef-t1", "w1", "/wt/team-abcdef-t1"
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
	if before.Tasks[0].State != "waiting" {
		t.Fatal("Starting changed the team it was given")
	}
}

func TestAStartingTaskSkipsNamesTheProjectOrItsTeamHasTaken(t *testing.T) {
	tm := startedPlan(t)
	tm.Tasks[1].Branch = "team-abcdef-t1-2"
	got := Starting(tm, "t1", []string{"team-abcdef-t1"})
	if b := got.Tasks[0].Branch; b != "team-abcdef-t1-3" {
		t.Fatalf("got %q", b)
	}
}

func TestAWorkersStatusShowsOnItsTaskWhileItRuns(t *testing.T) {
	asking, changed := WorkerStatus(t1Working(t), "w1", "needsYou")
	if !changed || asking.Tasks[0].State != "needsYou" {
		t.Fatalf("asking: %t %+v", changed, asking.Tasks[0])
	}
	back, changed := WorkerStatus(asking, "w1", "working")
	if !changed || back.Tasks[0].State != "running" {
		t.Fatalf("back: %t %+v", changed, back.Tasks[0])
	}
	failed, _ := Failed(back, "t1", "x")
	for name, c := range map[string]struct {
		team           proto.Team
		worker, status string
	}{
		"the same status":  {back, "w1", "working"},
		"another worker":   {back, "w9", "needsYou"},
		"a turn's end":     {back, "w1", "done"},
		"a task that ended": {failed, "w1", "working"},
	} {
		if _, changed := WorkerStatus(c.team, c.worker, c.status); changed {
			t.Errorf("%s changed the team", name)
		}
	}
}

func TestAFailedTurnFailsOnlyATaskThatIsRunning(t *testing.T) {
	got, changed := Failed(t1Working(t), "t1", "The worker's turn failed")
	want := t1Working(t)
	want.Tasks[0].State, want.Tasks[0].Summary = "failed", "The worker's turn failed"
	if !changed || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %t %+v", changed, got)
	}
	if _, changed := Failed(got, "t1", "again"); changed {
		t.Fatal("a failed task failed again")
	}
	if _, changed := Failed(startedPlan(t), "t2", "x"); changed {
		t.Fatal("a waiting task failed")
	}
}

func TestAFailedHandbackCarriesWhy(t *testing.T) {
	want := "<anywhere_team_handback task=\"t3\" status=\"failed\">\nMerging team-abcdef-t2 conflicted in: src/index.js\n</anywhere_team_handback>"
	if got := FailedHandback("t3", "Merging team-abcdef-t2 conflicted in: src/index.js"); got != want {
		t.Fatalf("got\n%s", got)
	}
}

func TestOnlyALiveTeamsWorkersAreFound(t *testing.T) {
	live := t1Working(t)
	if tm, task, ok := Find([]proto.Team{live}, "w1"); !ok || tm.Lead != testLead || task.ID != "t1" {
		t.Fatalf("got %v %+v", ok, task)
	}
	if _, _, ok := Find([]proto.Team{live}, ""); ok {
		t.Fatal("found a task with no worker")
	}
	merged := t1Working(t)
	merged.State = "merged"
	if _, _, ok := Find([]proto.Team{merged}, "w1"); ok {
		t.Fatal("found a merged team's worker")
	}
}

func TestARunningTeamAllowsItsWorkersListedCommandsOnly(t *testing.T) {
	running := []proto.Team{t1Working(t)}
	review := t1Working(t)
	review.State = "review"
	for _, c := range []struct {
		teams                 []proto.Team
		worker, tool, command string
		want                  bool
	}{
		{running, "w1", "Bash", "npm test", true},
		{running, "w1", "shell", "npm test -- --run", true},
		{running, "w1", "Bash", "npm test && rm -rf src", false},
		{running, "w1", "Bash", "npm run build", false},
		{running, "w1", "Write", "npm test", false},
		{running, "w9", "Bash", "npm test", false},
		{[]proto.Team{review}, "w1", "Bash", "npm test", false},
	} {
		if got := Allows(c.teams, c.worker, c.tool, c.command); got != c.want {
			t.Errorf("%s %s %q: %t", c.worker, c.tool, c.command, got)
		}
	}
}
```

**Step 2: Run the tests to verify they fail**

Run: `go test ./internal/team -count=1`
Expected: FAIL, build error `undefined: Started` (and the others)

**Step 3: Write the implementation** (`run.go`)

```go
package team

import (
	"errors"
	"fmt"
	"slices"
	"strings"

	"pocketd/internal/proto"
	"pocketd/internal/worktree"
)

// Started applies the user's edits at Start to a proposed team: the picked
// tasks stay, in plan order, with their agent, model and allowlist; the rest
// go. A kept task's after must all be kept, and the kept tasks must still make
// a valid plan, though one task is enough. Each waits to start.
func Started(t proto.Team, picks []proto.TaskPick) (proto.Team, error) {
	if t.State != "proposed" {
		return t, errors.New("This team's plan isn't waiting to start")
	}
	kept := map[string]proto.TaskPick{}
	for _, p := range picks {
		if _, twice := kept[p.ID]; twice {
			return t, fmt.Errorf("Task %s is listed twice", p.ID)
		}
		if taskAt(t, p.ID) < 0 {
			return t, fmt.Errorf("The plan has no task %s", p.ID)
		}
		kept[p.ID] = p
	}
	for _, x := range t.Tasks {
		if _, ok := kept[x.ID]; !ok {
			continue
		}
		for _, a := range x.After {
			if _, ok := kept[a]; !ok {
				return t, fmt.Errorf("Task %s starts from %s's work; keep %s or remove %s too", x.ID, a, a, x.ID)
			}
		}
	}
	tasks := []proto.TeamTask{}
	for _, x := range t.Tasks {
		p, ok := kept[x.ID]
		if !ok {
			continue
		}
		x.Agent, x.Model, x.Allow, x.State = p.Agent, p.Model, p.Allow, "waiting"
		if x.Allow == nil {
			x.Allow = []string{}
		}
		tasks = append(tasks, x)
	}
	if err := ValidateKept(tasks); err != nil {
		return t, err
	}
	t.Tasks, t.State, t.Error = tasks, "running", ""
	return t, nil
}

// Base is what task of t starts from: head (the lead's commit) without
// dependencies, its one dependency's branch, or for several its own branch's
// -base branch, which pocketd merges them into.
func Base(t proto.Team, task proto.TeamTask, head string) string {
	switch len(task.After) {
	case 0:
		return head
	case 1:
		return t.Tasks[taskAt(t, task.After[0])].Branch
	}
	return task.Branch + "-base"
}

// Branches are the branches t's tasks ids took.
func Branches(t proto.Team, ids []string) []string {
	out := make([]string, len(ids))
	for i, id := range ids {
		out[i] = t.Tasks[taskAt(t, id)].Branch
	}
	return out
}

// Free is the first of name, name-2, name-3, … that taken doesn't hold, nor
// its -base branch, read as worktree.Validate reads taken.
func Free(name string, taken []string) string {
	for n := 1; ; n++ {
		b := name
		if n > 1 {
			b = fmt.Sprintf("%s-%d", name, n)
		}
		if worktree.Validate(b, taken) != worktree.ErrExists && worktree.Validate(b+"-base", taken) != worktree.ErrExists {
			return b
		}
	}
}

// WorkerSpec launches task's worker: a chat under the lead, in a new worktree
// named for its branch, from base, accepting edits. It has no prompt: the
// assignment goes once the worker is on record.
func WorkerSpec(project, lead string, task proto.TeamTask, base string) proto.LaunchSpec {
	return proto.LaunchSpec{
		Project:  project,
		Checkout: proto.Checkout{New: &proto.NewWorktree{Name: task.Branch, Base: base}},
		Provider: task.Agent,
		Model:    task.Model,
		Access:   "edits",
		Chat:     true,
		Parent:   lead,
	}
}

// Assignment is a worker's first prompt.
func Assignment(task proto.TeamTask) string {
	return fmt.Sprintf(`<anywhere_team_assignment task=%q>
You're one worker in a team that a lead agent planned. Your task: %s.
%s
Change only these files: %s. Don't commit, and don't create branches or worktrees.
When you're done, reply with a short summary of what you changed and how you checked it.
</anywhere_team_assignment>`, task.ID, strings.TrimSuffix(task.Title, "."), task.Instructions, strings.Join(task.Files, ", "))
}

// Starting marks task id running, before its worker exists, on the first
// free name for its branch: not in taken (the project's branches and worktree
// folders, worktree.Taken) and no other task of t's.
func Starting(t proto.Team, id string, taken []string) proto.Team {
	names := slices.Clone(taken)
	for _, x := range t.Tasks {
		if x.Branch != "" {
			names = append(names, x.Branch)
		}
	}
	branch := Free(Branch(t.Lead, id), names)
	t, _ = editTask(t, id, func(x *proto.TeamTask) { x.State, x.Branch = "running", branch })
	return t
}

// Launched records task id's worker and its folder.
func Launched(t proto.Team, id, worker, folder string) proto.Team {
	t, _ = editTask(t, id, func(x *proto.TeamTask) { x.Worker, x.Worktree = worker, folder })
	return t
}

// WorkerStatus shows a running worker's status on its task: working is
// running, needsYou is needsYou. Anything else changes nothing.
func WorkerStatus(t proto.Team, worker, status string) (proto.Team, bool) {
	state := map[string]string{"working": "running", "needsYou": "needsYou"}[status]
	i := slices.IndexFunc(t.Tasks, func(x proto.TeamTask) bool { return worker != "" && x.Worker == worker })
	if state == "" || i < 0 || !inTurn(t.Tasks[i].State) || t.Tasks[i].State == state {
		return t, false
	}
	return editTask(t, t.Tasks[i].ID, func(x *proto.TeamTask) { x.State = state })
}

// Failed fails task id while it runs, with why as its summary.
func Failed(t proto.Team, id, why string) (proto.Team, bool) {
	if i := taskAt(t, id); i < 0 || !inTurn(t.Tasks[i].State) {
		return t, false
	}
	return editTask(t, id, func(x *proto.TeamTask) { x.State, x.Summary = "failed", why })
}

// FailedHandback tells the lead task id failed, and why.
func FailedHandback(id, why string) string {
	return fmt.Sprintf("<anywhere_team_handback task=%q status=\"failed\">\n%s\n</anywhere_team_handback>", id, why)
}

// Find is the live team (running or in review) that worker works for, and its task.
func Find(teams []proto.Team, worker string) (proto.Team, proto.TeamTask, bool) {
	for _, t := range teams {
		if t.State != "running" && t.State != "review" {
			continue
		}
		for _, task := range t.Tasks {
			if worker != "" && task.Worker == worker {
				return t, task, true
			}
		}
	}
	return proto.Team{}, proto.TeamTask{}, false
}

// Allows is whether a running team's plan lets worker run command, asked as a
// shell tool (Claude's Bash, Codex's shell), without asking the user.
func Allows(teams []proto.Team, worker, tool, command string) bool {
	if tool != "Bash" && tool != "shell" {
		return false
	}
	t, task, ok := Find(teams, worker)
	return ok && t.State == "running" && Allowed(task.Allow, command)
}

func taskAt(t proto.Team, id string) int {
	return slices.IndexFunc(t.Tasks, func(x proto.TeamTask) bool { return x.ID == id })
}

// editTask is t with f applied to a copy of task id; false when t has none.
func editTask(t proto.Team, id string, f func(*proto.TeamTask)) (proto.Team, bool) {
	i := taskAt(t, id)
	if i < 0 {
		return t, false
	}
	t.Tasks = slices.Clone(t.Tasks)
	f(&t.Tasks[i])
	return t, true
}

// inTurn is whether a task in state has a worker at work.
func inTurn(state string) bool { return state == "running" || state == "needsYou" }
```

**Step 4: Run the tests to verify they pass**

Run: `gofmt -l ./internal/team && go test ./internal/team -count=1`
Expected: nothing listed; PASS

### Task 2.4: Decisions for handbacks and the verdict

**Files:**
- Modify: `packages/pocketd/internal/team/run.go`
- Test: `packages/pocketd/internal/team/run_test.go`

**Context:** A handback's summary is the worker's last assistant text in its timeline (`proto.Item` kinds `user`, `assistant`, `notice`, …). The transcript can lag the turn's end, so `LastText` also says whether that text answers the last prompt; `Teams` reads again while it doesn't. A handback can come again for a task already done (the user prompted the worker directly): it updates the summary and stat. But the same turn hands back once: a turn is named by its prompt's item id (`TurnID`; the timeline is rebuilt from the transcript, so the id survives a restart), `Handed` records it in `TeamTask.Turn`, and a handback of the turn already recorded changes nothing. A turn end can be heard twice, through `Run` and through restart recovery (PR 4). A turn whose prompt has no id can't be told apart and always hands back. The team moves to `review` when every task is done. The lead's verdict counts only in review.

**Step 1: Write the failing tests** (append to `run_test.go`)

```go
func TestTheLastTextIsTheReplyToTheLastPrompt(t *testing.T) {
	items := []proto.Item{{Kind: "user", Text: "go"}, {Kind: "assistant", Text: "Looking."}, {Kind: "assistant", Text: "Added truncate; npm test passes."}, {Kind: "notice", Text: "Allowed by the team plan: npm test"}}
	if text, ok := LastText(items); !ok || text != "Added truncate; npm test passes." {
		t.Fatalf("got %q %t", text, ok)
	}
	if text, ok := LastText(append(items, proto.Item{Kind: "user", Text: "more"})); ok || text != "" {
		t.Fatalf("an unanswered prompt read %q", text)
	}
	if _, ok := LastText(nil); ok {
		t.Fatal("an empty timeline has a reply")
	}
}

func TestATurnIsNamedByItsPrompt(t *testing.T) {
	items := []proto.Item{{Kind: "user", ID: "u1"}, {Kind: "assistant", ID: "a1"}, {Kind: "user", ID: "s1", ParentToolUseID: "toolu_1"}, {Kind: "assistant", ID: "a2"}}
	if got := TurnID(items); got != "u1" {
		t.Fatalf("got %q", got)
	}
	if got := TurnID(nil); got != "" {
		t.Fatalf("no prompt named %q", got)
	}
}

func TestATurnHandsBackOnce(t *testing.T) {
	first, _ := Handed(t1Working(t), "t1", "u1", "Added truncate.", " src/truncate.js | 9 +")
	for name, c := range map[string]struct {
		turn    string
		changed bool
	}{
		"the same turn again":           {"u1", false},
		"the worker's next turn":        {"u2", true},
		"a turn whose prompt has no id": {"", true},
	} {
		if _, changed := Handed(first, "t1", c.turn, "Again.", " src/truncate.js | 10 +"); changed != c.changed {
			t.Errorf("%s: changed %t", name, changed)
		}
	}
}

func TestAHandbackFinishesItsTaskAndTheLastOneStartsTheReview(t *testing.T) {
	pair, err := Started(planOfThree(), []proto.TaskPick{{ID: "t1", Agent: "claude"}, {ID: "t2", Agent: "claude"}})
	if err != nil {
		t.Fatal(err)
	}
	pair = Launched(Starting(pair, "t1", nil), "t1", "w1", "/wt1")
	pair = Launched(Starting(pair, "t2", nil), "t2", "w2", "/wt2")
	one, changed := Handed(pair, "t1", "u1", "Added truncate.", " src/truncate.js | 9 +")
	want := pair
	want.Tasks = append([]proto.TeamTask(nil), pair.Tasks...)
	want.Tasks[0].State, want.Tasks[0].Summary, want.Tasks[0].Stat, want.Tasks[0].Turn = "done", "Added truncate.", " src/truncate.js | 9 +", "u1"
	if !changed || !reflect.DeepEqual(one, want) {
		t.Fatalf("got %+v", one)
	}
	both, _ := Handed(one, "t2", "v1", "Added titleCase.", " src/title.js | 7 +")
	if both.State != "review" {
		t.Fatalf("state %q", both.State)
	}
	again, changed := Handed(both, "t1", "u2", "Fixed the edge case.", " src/truncate.js | 10 +")
	if !changed || again.State != "review" || again.Tasks[0].Summary != "Fixed the edge case." {
		t.Fatalf("a second handback: %+v", again)
	}
	if _, changed := Handed(both, "t9", "x1", "", ""); changed {
		t.Fatal("an unknown task handed back")
	}
	merged := both
	merged.Tasks = append([]proto.TeamTask(nil), both.Tasks...)
	merged.Tasks[0].State = "merged"
	if _, changed := Handed(merged, "t1", "u3", "late", ""); changed {
		t.Fatal("a merged task handed back")
	}
}

func TestTheHandbackTellsTheLeadWhatTheWorkerDidOnItsBranch(t *testing.T) {
	task := t1Working(t).Tasks[0]
	want := `<anywhere_team_handback task="t1" title="Add truncate" branch="team-abcdef-t1">
Summary from the worker:
Added truncate.

git diff --stat:
 src/truncate.js | 9 +
 1 file changed, 9 insertions(+)
</anywhere_team_handback>`
	if got := Handback(task, "Added truncate.", " src/truncate.js | 9 +\n 1 file changed, 9 insertions(+)"); got != want {
		t.Fatalf("got\n%s", got)
	}
	want = `<anywhere_team_handback task="t1" title="Add truncate" branch="team-abcdef-t1">
Summary from the worker:
Nothing needed changing.

git diff --stat:
(no changes)
</anywhere_team_handback>`
	if got := Handback(task, "Nothing needed changing.", ""); got != want {
		t.Fatalf("got\n%s", got)
	}
}

func TestOnlyAnUnnegatedReadyToMergeIsReady(t *testing.T) {
	for text, want := range map[string]bool{
		"Ready to merge.":                                  true,
		"All three look right. READY TO MERGE":             true,
		"Not ready to merge: t3 doesn't export truncate.": false,
		"This isn't ready to merge yet.":                   false,
		"t3 needs another pass.":                           false,
	} {
		if got := Ready(text); got != want {
			t.Errorf("%q: %t", text, got)
		}
	}
}

func TestTheLeadsVerdictCountsOnlyInReview(t *testing.T) {
	review := t1Working(t)
	review.State = "review"
	ready, changed := Verdict(review, "Ready to merge.")
	want := review
	want.Ready = true
	if !changed || !reflect.DeepEqual(ready, want) {
		t.Fatalf("got %t %+v", changed, ready)
	}
	if _, changed := Verdict(ready, "Ready to merge, still."); changed {
		t.Fatal("the same verdict changed the team")
	}
	if again, changed := Verdict(ready, "Not ready to merge."); !changed || again.Ready {
		t.Fatalf("a new verdict: %t %+v", changed, again)
	}
	if _, changed := Verdict(t1Working(t), "Ready to merge."); changed {
		t.Fatal("a running team took a verdict")
	}
}
```

**Step 2: Run the tests to verify they fail**

Run: `go test ./internal/team -count=1`
Expected: FAIL, build error `undefined: LastText` (and `TurnID`, `Handed`, `Handback`, `Ready`, `Verdict`)

**Step 3: Write the implementation** (append to `run.go`; add `"cmp"` to its imports)

```go
// LastText is the last assistant text in items, and whether it answers the
// last prompt; while the transcript lags a turn's end it doesn't.
func LastText(items []proto.Item) (string, bool) {
	for i := len(items) - 1; i >= 0; i-- {
		switch items[i].Kind {
		case "assistant":
			return items[i].Text, true
		case "user":
			return "", false
		}
	}
	return "", false
}

// TurnID names the last turn in items by its prompt's id; "" when items hold no prompt.
func TurnID(items []proto.Item) string {
	for i := len(items) - 1; i >= 0; i-- {
		if items[i].Kind == "user" && items[i].ParentToolUseID == "" {
			return items[i].ID
		}
	}
	return ""
}

// Handed records a worker's handback of turn: its task is done, with what it
// said and what it changed. The turn handed back last changes nothing again.
// Once every task is done, a running team is in review.
func Handed(t proto.Team, id, turn, summary, stat string) (proto.Team, bool) {
	if i := taskAt(t, id); i < 0 || t.Tasks[i].State == "merged" || turn != "" && t.Tasks[i].Turn == turn {
		return t, false
	}
	t, _ = editTask(t, id, func(x *proto.TeamTask) { x.State, x.Summary, x.Stat, x.Turn = "done", summary, stat, turn })
	if t.State == "running" && !slices.ContainsFunc(t.Tasks, func(x proto.TeamTask) bool { return x.State != "done" }) {
		t.State = "review"
	}
	return t, true
}

// Handback is the mail that hands task's work to the lead.
func Handback(task proto.TeamTask, summary, stat string) string {
	return fmt.Sprintf("<anywhere_team_handback task=%q title=%q branch=%q>\nSummary from the worker:\n%s\n\ngit diff --stat:\n%s\n</anywhere_team_handback>",
		task.ID, task.Title, task.Branch, summary, cmp.Or(stat, "(no changes)"))
}

// Ready is whether text says "ready to merge", in any case, other than right
// after "not" or "n't".
func Ready(text string) bool {
	s := strings.ToLower(text)
	for {
		i := strings.Index(s, "ready to merge")
		if i < 0 {
			return false
		}
		before := strings.TrimSpace(s[:i])
		if !strings.HasSuffix(before, "not") && !strings.HasSuffix(before, "n't") && !strings.HasSuffix(before, "n’t") {
			return true
		}
		s = s[i+len("ready to merge"):]
	}
}

// Verdict reads the lead's last text at a turn end while the team is in review.
func Verdict(t proto.Team, text string) (proto.Team, bool) {
	ready := Ready(text)
	if t.State != "review" || t.Ready == ready {
		return t, false
	}
	t.Ready = ready
	return t, true
}
```

**Step 4: Run the tests to verify they pass**

Run: `gofmt -l ./internal/team && go test ./internal/team -count=1`
Expected: nothing listed; PASS

### Task 2.5: `Teams` runs teams: `team.start`, handbacks, allowed asks, and wiring

**Files:**
- Modify: `packages/pocketd/internal/team/teams.go` (plan 3's: the `Teams` struct gains three fields; `event` gains `ok`; `handle` is replaced)
- Create: `packages/pocketd/internal/team/running.go`
- Create: `packages/pocketd/internal/wsserver/team_run.go`
- Modify: `packages/pocketd/internal/wsserver/wsserver.go` (a `dispatch` case)
- Modify: `packages/pocketd/cmd/pocketd/serve.go` (right before plan 3's `go teams.Run(ctx)`)

**Context:** This is glue on plan 3's `Teams`, the one team engine: it gathers inputs (the lead's folder and HEAD, the names already taken, a worker's last text, git results), calls the functions of Tasks 2.3–2.4, and writes only through plan 3's `change`, which saves and publishes. Hooks run under an agent's lock, so `TurnEnded` only queues, like plan 3's `Status`; plan 3's `Run` handles the queue in order on its own goroutine, and `handle` now routes each event to plan 3's planning (`closed`, `Ended`), the lead's verdict (`review`) or the worker's task (`work`). Launches run on their own goroutines (a launch waits for the agent to attach) and write back through `change` too. `launch.Create` dedups by `Who.Key` + request id for 10 minutes, so each worker launch gets a fresh request id. `AgentSummary.Project` is a display name; the project's path is `worktree.Project(reg.Load(), cwd)`, its worktrees folder `worktree.Dir(f, project)`, and the names a new worktree can't take `worktree.Taken(project, dir)`. `ClaimNaming`/`SetNamed` give the worker its task's title (`launch.create` names nothing when the spec has no prompt). Hub publishing never blocks. Plan 3's `send` prompts a lead and records where its timeline stood; the method that mails is named `tell` so the two don't clash. `serve.go`'s `teams` is nil when `state/team.db` can't open; every exported method here is nil-safe.

**Step 1: The failing test is Task 2.1's.** Nothing new to write.

**Step 2: Write the implementation**

In `internal/team/teams.go` (plan 3), add `"pocketd/internal/registry"` to the imports; in `Teams`, after `Poll`:

```go
	// Mail queues text for an agent's chat (daemon.Mail): handbacks and notes to a lead.
	Mail func(agentID, text string) error
	// Launch starts a worker as the owner and returns its agent id and folder.
	Launch func(lead, task string, s proto.LaunchSpec) (agentID, cwd string, err error)
	// Projects finds the project a lead's folder is in.
	Projects *registry.Registry
```

replace `event` with:

```go
// event is something an agent did, queued by a hook that runs under the agent's lock.
type event struct {
	kind     string // "status" or "turn"
	agent    string
	from, to string // status: the change
	ok       bool   // turn: it ended normally
}
```

and replace `handle` with:

```go
func (t *Teams) handle(e event) {
	switch {
	case e.kind == "status" && e.to == "closed":
		t.closed(e.agent)
	case e.kind == "status" && turnEnd(e.from, e.to):
		t.Ended(e.agent)
	case e.kind == "turn" && e.ok && t.review(e.agent):
		return
	}
	t.work(e)
}
```

Create `internal/team/running.go`:

```go
package team

import (
	"errors"
	"fmt"
	"log"
	"time"

	"pocketd/internal/proto"
	"pocketd/internal/timeline"
	"pocketd/internal/worktree"
)

// The transcript a reply is read from can lag the turn's end.
var (
	textTries = 10
	textWait  = 300 * time.Millisecond
)

var errNoTeam = errors.New("This session has no team")

// TurnEnded is agent.Registry's OnTurnEnd. It runs under the agent's lock, so it only queues.
func (t *Teams) TurnEnded(id string, ok bool) {
	if t != nil {
		t.post(event{kind: "turn", agent: id, ok: ok})
	}
}

// work is what e means for the task whose worker did it, if any.
func (t *Teams) work(e event) {
	teams, err := t.Store.List()
	if err != nil {
		log.Printf("teams: %v", err)
		return
	}
	team, task, ok := Find(teams, e.agent)
	switch {
	case !ok:
	case e.kind == "turn" && e.ok:
		t.handback(team, task)
	case e.kind == "turn":
		t.fail(team.Lead, task.ID, "The worker's turn failed")
	case e.to == "closed":
		t.fail(team.Lead, task.ID, "The worker's session closed")
	default:
		t.change(team.Lead, func(tm proto.Team) (proto.Team, bool) { return WorkerStatus(tm, e.agent, e.to) })
	}
}

// review reads the verdict in a turn end of a lead whose team is in review;
// false when id leads no team in review.
func (t *Teams) review(id string) bool {
	team, ok, err := t.Store.Get(id)
	if err != nil || !ok || team.State != "review" {
		return false
	}
	_, text := t.lastText(id)
	t.change(id, func(tm proto.Team) (proto.Team, bool) { return Verdict(tm, text) })
	return true
}

// Start applies the user's picks to lead's proposed plan and starts what can start.
func (t *Teams) Start(lead string, picks []proto.TaskPick) error {
	if t == nil {
		return errOff
	}
	if _, err := t.leadCwd(lead); err != nil {
		return err
	}
	err := errNoTeam
	t.change(lead, func(tm proto.Team) (proto.Team, bool) {
		tm, err = Started(tm, picks)
		return tm, err == nil
	})
	if err != nil {
		return err
	}
	t.pump(lead)
	return nil
}

// pump starts every task lead's team can start now, each on the first free
// name for its branch.
func (t *Teams) pump(lead string) {
	taken := t.taken(lead)
	var starting []proto.TeamTask
	team, _ := t.change(lead, func(tm proto.Team) (proto.Team, bool) {
		if tm.State != "running" {
			return tm, false
		}
		for _, task := range Startable(tm) {
			tm = Starting(tm, task.ID, taken)
			starting = append(starting, tm.Tasks[taskAt(tm, task.ID)])
		}
		return tm, len(starting) > 0
	})
	for _, task := range starting {
		go t.launch(team, task)
	}
}

// taken is every name a worker's branch can't take in lead's project: its
// branches, its remotes' and the folders in its worktrees folder. None when
// the project can't be read; the launch then fails saying why.
func (t *Teams) taken(lead string) []string {
	cwd, err := t.leadCwd(lead)
	if err != nil {
		return nil
	}
	f := t.Projects.Load()
	project, ok := worktree.Project(f, cwd)
	if !ok {
		return nil
	}
	taken, err := worktree.Taken(project, worktree.Dir(f, project))
	if err != nil {
		log.Printf("teams: %s: %v", lead, err)
	}
	return taken
}

// launch starts task's worker, records it, names it, and only then sends its
// assignment, so the turn that answers it finds the worker on record. team is
// the team as task started, for the branches it starts from.
func (t *Teams) launch(team proto.Team, task proto.TeamTask) {
	lead := team.Lead
	spec, err := t.spec(team, task)
	if err != nil {
		t.fail(lead, task.ID, err.Error())
		return
	}
	id, cwd, err := t.Launch(lead, task.ID, spec)
	if err != nil {
		t.fail(lead, task.ID, err.Error())
		return
	}
	t.change(lead, func(tm proto.Team) (proto.Team, bool) { return Launched(tm, task.ID, id, cwd), true })
	a, err := t.Agents.Get(id)
	if err == nil {
		if claim, ok := a.ClaimNaming(); ok {
			a.SetNamed(claim, task.Title)
		}
		err = a.Driver().Prompt(Assignment(task))
	}
	if err != nil {
		t.fail(lead, task.ID, err.Error())
	}
}

// spec is how task's worker launches, once its -base branch is made when it needs one.
func (t *Teams) spec(team proto.Team, task proto.TeamTask) (proto.LaunchSpec, error) {
	cwd, err := t.leadCwd(team.Lead)
	if err != nil {
		return proto.LaunchSpec{}, err
	}
	project, ok := worktree.Project(t.Projects.Load(), cwd)
	if !ok {
		return proto.LaunchSpec{}, errors.New("The lead's folder isn't in a project on this Mac")
	}
	head, err := worktree.Head(cwd)
	if err != nil {
		return proto.LaunchSpec{}, err
	}
	base := Base(team, task, head)
	if len(task.After) > 1 {
		if err := worktree.MergeBranches(project, base, Branches(team, task.After)); err != nil {
			return proto.LaunchSpec{}, err
		}
	}
	return WorkerSpec(project, team.Lead, task, base), nil
}

// handback commits what task's worker changed and hands it to the lead, once
// per turn. It runs only on Run's goroutine, so two handbacks never commit at once.
func (t *Teams) handback(team proto.Team, task proto.TeamTask) {
	turn, summary := t.lastText(task.Worker)
	if turn != "" && turn == task.Turn {
		return
	}
	stat, err := t.commit(team, task)
	if err != nil {
		t.fail(team.Lead, task.ID, err.Error())
		return
	}
	if _, ok := t.change(team.Lead, func(tm proto.Team) (proto.Team, bool) { return Handed(tm, task.ID, turn, summary, stat) }); !ok {
		return
	}
	t.tell(team.Lead, Handback(task, summary, stat))
	t.pump(team.Lead)
}

// commit commits everything in task's worktree and says what its branch changed.
func (t *Teams) commit(team proto.Team, task proto.TeamTask) (string, error) {
	cwd, err := t.leadCwd(team.Lead)
	if err != nil {
		return "", err
	}
	head, err := worktree.Head(cwd)
	if err != nil {
		return "", err
	}
	if _, err := worktree.Commit(task.Worktree, fmt.Sprintf("team %s: %s", task.ID, task.Title)); err != nil {
		return "", err
	}
	return worktree.Stat(task.Worktree, Base(team, task, head))
}

// fail fails a running task, tells the lead, and starts what its slot frees.
func (t *Teams) fail(lead, id, why string) {
	if _, ok := t.change(lead, func(tm proto.Team) (proto.Team, bool) { return Failed(tm, id, why) }); !ok {
		return
	}
	t.tell(lead, FailedHandback(id, why))
	t.pump(lead)
}

// tell mails text to agentID's chat.
func (t *Teams) tell(agentID, text string) {
	if err := t.Mail(agentID, text); err != nil {
		log.Printf("teams: mail to %s: %v", agentID, err)
	}
}

// lastText is id's last turn, by its prompt's id, and its reply, read again while the transcript catches up.
func (t *Teams) lastText(id string) (turn, text string) {
	for try := 1; ; try++ {
		a, err := t.Agents.Get(id)
		if err != nil {
			return "", ""
		}
		items, _ := a.Timeline.Page(0, 50)
		if text, ok := LastText(items); ok || try == textTries {
			return TurnID(items), text
		}
		time.Sleep(textWait)
	}
}

func (t *Teams) leadCwd(lead string) (string, error) {
	a, err := t.Agents.Get(lead)
	if err != nil {
		return "", errors.New("The lead's session is closed")
	}
	return a.Summary().Cwd, nil
}

// Allow is broker.Broker's Allow: it lets a worker run a shell command its
// running team's plan allows, and notes that in the worker's timeline.
func (t *Teams) Allow(req proto.PermissionRequest) bool {
	if t == nil {
		return false
	}
	teams, err := t.Store.List()
	if err != nil || !Allows(teams, req.AgentID, req.ToolName, req.Detail.Command) {
		return false
	}
	if a, err := t.Agents.Get(req.AgentID); err == nil {
		a.Record(timeline.Event{Kind: "notice", Text: "Allowed by the team plan: " + req.Detail.Command})
	}
	return true
}
```

`internal/wsserver/team_run.go`:

```go
package wsserver

import "pocketd/internal/proto"

// teamRun serves the verbs that run a team, through plan 3's Teams (nil answers "Teams are off").
func (c *conn) teamRun(m proto.ClientMessage) error {
	var err error
	switch m.Type {
	case "team.start":
		err = c.s.Teams.Start(m.Lead, m.Tasks)
	}
	if err != nil {
		return err
	}
	c.send(proto.NewAck(m.ID))
	return nil
}
```

In `wsserver.go`'s `dispatch`, first `switch`, after plan 3's `case "team.list":` case:

```go
	case "team.start":
		return c.teamRun(m)
```

In `cmd/pocketd/serve.go`, add `"errors"` and `"strconv"` to the imports (if not there). Right before plan 3's `go teams.Run(ctx)` line (after `l.Teams = teams`), add:

```go
	if teams != nil {
		teams.Mail, teams.Projects = d.Mail, reg
		teams.Launch = func(lead, task string, s proto.LaunchSpec) (string, string, error) {
			// A fresh request id each time: a lead's next plan reuses task ids.
			r := l.Create(launch.Who{Owner: true, Key: "team:" + lead}, task+"-"+strconv.FormatInt(time.Now().UnixNano(), 36), s, func(string, string) {}, func(launch.Creating) {})
			if r.Err != nil {
				return "", "", errors.New(r.Err.Message)
			}
			return r.AgentID, r.Cwd, nil
		}
		d.Agents.OnTurnEnd = teams.TurnEnded
		d.Broker.Allow = teams.Allow
	}
```

Plan 3's `teams.Status` line in `OnStatus` already queues every status change, so `OnStatus` needs nothing more. This block sits before `d.Restore`/`d.Adopt`, so every hook is set before the first agent exists, as `OnStatus` requires.

**Step 3: Run the tests to verify they pass**

Run: `gofmt -l ./internal ./cmd && go vet ./... && go test ./internal/team -count=1 && go test ./e2e -run 'TestATeamsWorkersHandBack|Team' -count=1 -v`
Expected: nothing listed by gofmt; vet clean; `ok  	pocketd/internal/team` (plan 3's `Teams` tests still pass with the new `handle`); `--- PASS: TestATeamsWorkersHandBackInOrderAndTheLeadReviews` and plan 3's four team e2e tests.

If it times out at "the lead's verdict", check that plan 2 delivers mail to a fake-claude lead (its own e2e covers that); if at "t1's handback" with `asked true`, the allowlist didn't fire: check `d.Broker.Allow` is set before `d.Restore`.

**Step 4: Run everything**

Run: `go test ./... && (cd ../.. && pnpm --filter @pocket/protocol test)`
Expected: PASS

---

## PR 3: Stop a task, note the user's prompts, merge

**Scope:** `team.stop` and `team.merge` on the wire; stopping a task interrupts its worker; prompting a worker from the chat mails the lead a note; merge refuses a dirty checkout, merges in dependency order, aborts and reports a conflict (and mails the lead), and after a full merge closes the workers and removes their worktrees, keeping the branches.
**Depends on:** PR 2
**Done when:** `go test ./e2e -run TestATeam -count=1` passes both team tests, and `go test ./...`, `pnpm --filter @pocket/protocol test` and the Swift `GoldenTests` are green.

### Task 3.1: The e2e test for notes and the merge

**Files:**
- Modify: `packages/pocketd/e2e/team_run_test.go`

**Context:** `reviewedTeam` (Task 2.1) leaves a team in review with `Ready`. Prompting worker t1 from the chat mails the lead `<anywhere_team_note task="t1">The user told worker t1: "check again"</anywhere_team_note>` (fake claude shows it as the lead's user item) and makes t1 hand back again with `echo: check again`. Waiting for that handback before merging keeps the merge from racing it. The merge commits are `Merge <branch>`; after it, only the main worktree is left and both branches stay. Add `"os"` to the file's imports.

**Step 1: Write the failing test** (append)

```go
func TestATeamNotesWhatTheUserToldAWorkerAndMergesIntoTheLeadsCheckout(t *testing.T) {
	h, o, lead, team := reviewedTeam(t)
	o.Prompt(team.Tasks[0].Worker, "check again")
	o.WaitAll("the note and the second handback",
		streamOf(lead, "user", `<anywhere_team_note task="t1">The user told worker t1: "check again"</anywhere_team_note>`),
		func(m Message) bool { return taskIn(lead, "t1", "done")(m) && m.Team.Tasks[0].Summary == "echo: check again" },
	)
	o.Send(map[string]any{"type": "team.merge", "id": "merge", "lead": lead})
	if m := o.WaitFor("team.merge's reply", replyTo("merge")); m.Type != "ack" {
		t.Fatalf("team.merge: %s", m.Raw)
	}
	merged := o.WaitFor("the merge", teamOf(lead, "merged")).Team
	for _, task := range merged.Tasks {
		if task.State != "merged" {
			t.Fatalf("%s is %s", task.ID, task.State)
		}
	}
	for name, want := range map[string]string{"a.txt": "one\n", "b.txt": "two\n"} {
		if got, err := os.ReadFile(filepath.Join(h.Repo, name)); err != nil || string(got) != want {
			t.Fatalf("%s: %q, %v", name, got, err)
		}
	}
	b1, b2 := "team-"+lead[:6]+"-t1", "team-"+lead[:6]+"-t2"
	if got := gitOut(t, h.Repo, "log", "--merges", "--format=%s"); got != "Merge "+b2+"\nMerge "+b1 {
		t.Fatalf("merges %q", got)
	}
	if got := gitOut(t, h.Repo, "worktree", "list", "--porcelain"); strings.Count(got, "worktree ") != 1 {
		t.Fatalf("worktrees left:\n%s", got)
	}
	if got := gitOut(t, h.Repo, "branch", "--list", "team-*", "--format=%(refname:short)"); got != b1+"\n"+b2 {
		t.Fatalf("branches %q", got)
	}
}
```

**Step 2: Run the test to verify it fails**

Run: `go test ./e2e -run TestATeamNotes -count=1`
Expected: FAIL with `waiting for the note and the second handback: context deadline exceeded`

### Task 3.2: `team.stop` and `team.merge` on the wire

**Files:**
- Modify: `packages/pocketd/internal/proto/messages.go`
- Modify: `packages/pocketd/internal/proto/team.go` (`Team` gains `Checkout`)
- Create: `packages/pocketd/internal/proto/testdata/golden/client/team_stop.json`, `…/client/team_merge.json`
- Modify: `packages/pocketd/internal/proto/golden_test.go`
- Modify: `packages/pocketd/internal/peer/check.go`, `packages/pocketd/internal/peer/check_test.go`
- Modify: `packages/protocol/src/team.ts` (`TeamClient`)
- Modify: `packages/ios/Packages/AnywhereKit/Sources/Wire/Team.swift`, `packages/ios/Packages/AnywhereKit/Sources/Wire/ClientMessage.swift`

**Context:** As Task 2.2, Swift included: the iOS golden tests decode the two new client goldens too. `Team` gains `checkout`, the lead's checkout a merge runs in (Task 3.4 records it), so restart recovery can find a merge in progress there even when the lead didn't come back (PR 4); optional and left off when empty, as `turn` is. `team.stop` is Drive (like `agent.interrupt`); `team.merge` is Spawn (it writes the lead's checkout and removes worktrees).

**Step 1: Write the failing tests**

`team_stop.json`:

```json
{"type":"team.stop","id":"tm2","lead":"5f0c2a9e-1b2c-4d3e-8f90-123456789abc","task":"t1"}
```

`team_merge.json`:

```json
{"type":"team.merge","id":"tm3","lead":"5f0c2a9e-1b2c-4d3e-8f90-123456789abc"}
```

`TestDecodeClientRejects` gains:

```go
		`{"type":"team.stop","id":"1","lead":"a1"}`,
		`{"type":"team.stop","id":"1","task":"t1"}`,
		`{"type":"team.merge","id":"1"}`,
```

The peer matrix gains:

```go
		"ws:team.stop":          {"", "", "", "scope_denied"},
		"ws:team.merge":         {"", "", "scope_denied", "scope_denied"},
```

**Step 2: Run the tests to verify they fail**

Run: `go test ./internal/proto ./internal/peer -count=1`
Expected: FAIL: `team_merge.json: Malformed message`, `team_stop.json: Malformed message`, and `ws:team.stop by principal 0: "scope_denied", want ""`

Run: `cd packages/ios/Packages/AnywhereKit && PATH="$HOME/.swiftly/bin:$PATH" swift test --filter GoldenTests`
Expected: FAIL: `unknown message type team.stop` and `team.merge`

**Step 3: Write the implementation**

In `internal/proto/team.go`, `Team` gains after `Repaired`:

```go
	Checkout  string     `json:"checkout,omitempty"` // the lead's checkout a merge runs in
```

In `team.ts`'s `Team`, after `repaired: Schema.optional(Schema.Boolean),` add `checkout: Schema.optional(Schema.String),`. In `Team.swift`'s `Team`, after `public var repaired: Bool?` add `public var checkout: String?`.

`ClientMessage`: the `// team.start's.` block becomes:

```go
	// team.start, team.stop and team.merge's.
	Lead  string
	Task  string
	Tasks []TaskPick
```

`DecodeClient`, after the `team.start` case:

```go
	case m.Type == "team.stop":
		ok = get("lead", &m.Lead) && get("task", &m.Task)
	case m.Type == "team.merge":
		ok = get("lead", &m.Lead)
```

`peer.Needs`, after `"ws:team.start"`:

```go
	"ws:team.stop":            Drive,
	"ws:team.merge":           Spawn,
```

`team.ts`, in `TeamClient` after the `team.start` member:

```ts
  Schema.Struct({ type: Schema.Literal("team.stop"), id: Schema.String, lead: Schema.String, task: Schema.String }),
  Schema.Struct({ type: Schema.Literal("team.merge"), id: Schema.String, lead: Schema.String }),
```

`Team.swift`, in the `extension ClientMessage` from Task 2.2, after `TeamStart`:

```swift
    /// team.stop: one task of the lead's team.
    public struct TeamStop: Codable, Hashable, Sendable {
        public var lead: String
        public var task: String
    }

    /// team.merge: the lead whose team merges.
    public struct TeamRef: Codable, Hashable, Sendable {
        public var lead: String
    }
```

`ClientMessage.swift`: after `case teamStart(…)` add `case teamStop(ClientMessage.TeamStop)` and `case teamMerge(ClientMessage.TeamRef)`; in `init(from:)` after the `"team.start"` line: `case "team.stop": self = .teamStop(try ClientMessage.TeamStop(from: decoder))` and `case "team.merge": self = .teamMerge(try ClientMessage.TeamRef(from: decoder))`; in `encode(to:)` after `.teamStart`: `case .teamStop(let m): try m.encode(to: encoder)` and `case .teamMerge(let m): try m.encode(to: encoder)`; in `type` after `.teamStart`: `case .teamStop: "team.stop"` and `case .teamMerge: "team.merge"`.

**Step 4: Run the tests to verify they pass**

Run: `go test ./internal/proto ./internal/peer -count=1 && (cd ../.. && pnpm --filter @pocket/protocol test)`
Expected: PASS

Run: `cd packages/ios/Packages/AnywhereKit && PATH="$HOME/.swiftly/bin:$PATH" swift test --filter GoldenTests`
Expected: PASS

### Task 3.3: Decisions for stopping, notes and merging

**Files:**
- Modify: `packages/pocketd/internal/team/run.go`
- Test: `packages/pocketd/internal/team/run_test.go`

**Context:** `ids(tasks)` (plan 3's `team_test.go`) lists task ids; don't declare another. Stop applies only to a task whose worker is at work. Merge runs from `review` or, after fixing a conflict, from `conflict`; it takes the `done` tasks in dependency order (`team.Order`, plan 3), so tasks merged before a conflict aren't merged twice. A full merge marks every task merged and the team `merged`. A conflict keeps what merged, records the files and leaves the team in `conflict`. Any other git refusal returns to `review` with `Error`.

**Step 1: Write the failing tests** (append to `run_test.go`)

```go
// reviewedPair is a reviewed team listed t2 first, though t2 is after t1.
func reviewedPair() proto.Team {
	return proto.Team{Lead: testLead, State: "review", Tasks: []proto.TeamTask{
		{ID: "t2", Title: "Use truncate", Agent: "claude", Files: []string{"src/index.js"}, After: []string{"t1"}, Allow: []string{}, State: "done", Branch: "team-abcdef-t2"},
		{ID: "t1", Title: "Add truncate", Agent: "claude", Files: []string{"src/truncate.js"}, After: []string{}, Allow: []string{}, State: "done", Branch: "team-abcdef-t1"},
	}}
}

func TestStopStopsOnlyATaskWhoseWorkerIsAtWork(t *testing.T) {
	got, err := Stopped(t1Working(t), "t1")
	want := t1Working(t)
	want.Tasks[0].State = "stopped"
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v, %v", got, err)
	}
	if _, err := Stopped(got, "t1"); err == nil {
		t.Fatal("stopped a stopped task")
	}
	if _, err := Stopped(startedPlan(t), "t2"); err == nil {
		t.Fatal("stopped a waiting task")
	}
}

func TestTheNotesSayWhoSaidWhat(t *testing.T) {
	if got, want := Note("t1", "use tabs"), `<anywhere_team_note task="t1">The user told worker t1: "use tabs"</anywhere_team_note>`; got != want {
		t.Fatalf("got %s", got)
	}
	want := `<anywhere_team_note>Merging team-abcdef-t2 conflicted in: src/a.go, src/b.go. The merge was aborted; nothing was merged after it.</anywhere_team_note>`
	if got := ConflictNote("team-abcdef-t2", []string{"src/a.go", "src/b.go"}); got != want {
		t.Fatalf("got %s", got)
	}
}

func TestMergingWaitsForTheReviewAndClearsTheLastConflict(t *testing.T) {
	got, err := Merging(Conflicted(reviewedPair(), []string{"src/index.js"}))
	want := reviewedPair()
	want.State = "merging"
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v, %v", got, err)
	}
	if _, err := Merging(reviewedPair()); err != nil {
		t.Fatal(err)
	}
	if _, err := Merging(t1Working(t)); err == nil {
		t.Fatal("a running team merged")
	}
}

func TestTheMergeTakesTheDoneTasksInDependencyOrder(t *testing.T) {
	if got := ids(MergeOrder(reviewedPair())); !reflect.DeepEqual(got, []string{"t1", "t2"}) {
		t.Fatalf("got %q", got)
	}
	if got := ids(MergeOrder(Merged(reviewedPair(), []string{"t1"}))); !reflect.DeepEqual(got, []string{"t2"}) {
		t.Fatalf("after t1 merged: %q", got)
	}
}

func TestAFullMergeMergesTheTeam(t *testing.T) {
	merging, _ := Merging(reviewedPair())
	got := Merged(merging, []string{"t1", "t2"})
	want := reviewedPair()
	want.State, want.Tasks[0].State, want.Tasks[1].State = "merged", "merged", "merged"
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
}

func TestAConflictKeepsWhatMergedAndNamesTheFiles(t *testing.T) {
	merging, _ := Merging(reviewedPair())
	got := Conflicted(Merged(merging, []string{"t1"}), []string{"src/index.js"})
	want := reviewedPair()
	want.State, want.Conflict, want.Tasks[1].State = "conflict", []string{"src/index.js"}, "merged"
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
}

func TestAMergeGitRefusesGoesBackToReviewSayingWhy(t *testing.T) {
	merging, _ := Merging(reviewedPair())
	got := MergeFailed(merging, "This checkout has uncommitted changes; commit or stash them first")
	want := reviewedPair()
	want.Error = "This checkout has uncommitted changes; commit or stash them first"
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
}
```

**Step 2: Run the tests to verify they fail**

Run: `go test ./internal/team -count=1`
Expected: FAIL, build error `undefined: Stopped` (and `Note`, `ConflictNote`, `Merging`, `MergeOrder`, `Merged`, `Conflicted`, `MergeFailed`)

**Step 3: Write the implementation** (append to `run.go`)

```go
// Stopped stops task id while its worker is at work.
func Stopped(t proto.Team, id string) (proto.Team, error) {
	if i := taskAt(t, id); i < 0 || !inTurn(t.Tasks[i].State) {
		return t, fmt.Errorf("Task %s isn't running", id)
	}
	t, _ = editTask(t, id, func(x *proto.TeamTask) { x.State = "stopped" })
	return t, nil
}

// Note tells the lead what the user told worker id directly.
func Note(id, text string) string {
	return fmt.Sprintf("<anywhere_team_note task=%q>The user told worker %s: %q</anywhere_team_note>", id, id, text)
}

// ConflictNote tells the lead the merge stopped on branch.
func ConflictNote(branch string, files []string) string {
	return fmt.Sprintf("<anywhere_team_note>Merging %s conflicted in: %s. The merge was aborted; nothing was merged after it.</anywhere_team_note>", branch, strings.Join(files, ", "))
}

// Merging starts a merge of a team in review, or again after a conflict.
func Merging(t proto.Team) (proto.Team, error) {
	if t.State != "review" && t.State != "conflict" {
		return t, errors.New("A team merges once every task has handed back")
	}
	t.State, t.Conflict, t.Error = "merging", nil, ""
	return t, nil
}

// MergeOrder is the tasks a merge takes: those done, in dependency order.
func MergeOrder(t proto.Team) []proto.TeamTask {
	return slices.DeleteFunc(slices.Clone(Order(t.Tasks)), func(x proto.TeamTask) bool { return x.State != "done" })
}

// Merged marks the tasks ids merged; the team is merged once every task is.
func Merged(t proto.Team, ids []string) proto.Team {
	for _, id := range ids {
		t, _ = editTask(t, id, func(x *proto.TeamTask) { x.State = "merged" })
	}
	if !slices.ContainsFunc(t.Tasks, func(x proto.TeamTask) bool { return x.State != "merged" }) {
		t.State = "merged"
	}
	return t
}

// Conflicted ends a merge that stopped on a conflict in files.
func Conflicted(t proto.Team, files []string) proto.Team {
	t.State, t.Conflict = "conflict", files
	return t
}

// MergeFailed ends a merge git refused for another reason, back in review.
func MergeFailed(t proto.Team, why string) proto.Team {
	t.State, t.Error = "review", why
	return t
}
```

**Step 4: Run the tests to verify they pass**

Run: `gofmt -l ./internal/team && go test ./internal/team -count=1`
Expected: nothing listed; PASS

### Task 3.4: `Teams` stops, notes and merges

**Files:**
- Modify: `packages/pocketd/internal/team/teams.go` (`event` gains `text`)
- Modify: `packages/pocketd/internal/team/running.go`
- Modify: `packages/pocketd/internal/wsserver/team_run.go`, `packages/pocketd/internal/wsserver/wsserver.go`

**Context:** Still plan 3's `Teams`, still writing only through `change`. `team.stop` marks the task stopped first, then interrupts: the interrupt ends the turn through `Clear` (not ok), which `Failed` then ignores because the task is no longer running. A prompt from the chat reaches a worker through `agent.prompt` in `wsserver.dispatch`; once sent, it is queued as a `told` event so the note follows `Run`'s order. A team prompt (`m.Team`) is the lead's own and is never noted. Merge checks the lead's checkout synchronously so the client gets the refusal as the verb's error, records it on the team (`Checkout`, for restart recovery), then merges in the background. A full merge closes the workers (`Driver().Close()`) and removes their worktrees with `worktree.Remove` (branch kept) *before* the team shows `merged`; the workers' `closed` statuses arrive while the team is `merging`, which `Find` skips.

**Step 1: The failing test is Task 3.1's.**

**Step 2: Write the implementation**

In `teams.go`, `event` becomes:

```go
// event is something an agent did, queued by a hook that runs under the agent's lock.
type event struct {
	kind     string // "status", "turn" or "told"
	agent    string
	from, to string // status: the change
	ok       bool   // turn: it ended normally
	text     string // told: what the user sent
}
```

In `running.go`, add `"cmp"` to the imports. In `work`, after `case !ok:`:

```go
	case e.kind == "told":
		t.tell(team.Lead, Note(task.ID, e.text))
```

Add:

```go
// Told is a prompt the user sent agentID from the chat; one sent to a worker is noted to its lead.
func (t *Teams) Told(agentID, text string) {
	if t != nil {
		t.post(event{kind: "told", agent: agentID, text: text})
	}
}

// Stop stops task id of lead's team and its worker's turn.
func (t *Teams) Stop(lead, id string) error {
	if t == nil {
		return errOff
	}
	err := errNoTeam
	team, _ := t.change(lead, func(tm proto.Team) (proto.Team, bool) {
		tm, err = Stopped(tm, id)
		return tm, err == nil
	})
	if err != nil {
		return err
	}
	if a, err := t.Agents.Get(team.Tasks[taskAt(team, id)].Worker); err == nil {
		a.Driver().Interrupt()
	}
	return nil
}

// Merge merges lead's team into the lead's checkout in the background, once
// the checkout has no uncommitted changes.
func (t *Teams) Merge(lead string) error {
	if t == nil {
		return errOff
	}
	cwd, err := t.leadCwd(lead)
	if err != nil {
		return err
	}
	if dirty, err := worktree.Dirty(cwd); err != nil || dirty {
		return cmp.Or(err, worktree.ErrDirty)
	}
	err = errNoTeam
	team, _ := t.change(lead, func(tm proto.Team) (proto.Team, bool) {
		tm, err = Merging(tm)
		tm.Checkout = cwd
		return tm, err == nil
	})
	if err != nil {
		return err
	}
	go t.merge(lead, cwd, team)
	return nil
}

// merge merges team's done tasks into cwd in order. A full merge cleans up
// before the team shows merged; a conflict is mailed to the lead.
func (t *Teams) merge(lead, cwd string, team proto.Team) {
	tasks := MergeOrder(team)
	branches := make([]string, len(tasks))
	for i, task := range tasks {
		branches[i] = task.Branch
	}
	done, err := worktree.Merge(cwd, branches)
	merged := make([]string, len(done))
	for i := range done {
		merged[i] = tasks[i].ID
	}
	conflict, _ := errors.AsType[*worktree.Conflict](err)
	if err == nil {
		t.cleanUp(cwd, team.Tasks)
	}
	t.change(lead, func(tm proto.Team) (proto.Team, bool) {
		tm = Merged(tm, merged)
		switch {
		case conflict != nil:
			tm = Conflicted(tm, conflict.Files)
		case err != nil:
			tm = MergeFailed(tm, err.Error())
		}
		return tm, true
	})
	if conflict != nil {
		t.tell(lead, ConflictNote(conflict.Branch, conflict.Files))
	}
}

// cleanUp closes the workers of tasks and removes their worktrees; their branches stay.
func (t *Teams) cleanUp(cwd string, tasks []proto.TeamTask) {
	f := t.Projects.Load()
	project, ok := worktree.Project(f, cwd)
	if !ok {
		return
	}
	for _, task := range tasks {
		if a, err := t.Agents.Get(task.Worker); err == nil {
			a.Driver().Close()
		}
		if task.Worktree == "" {
			continue
		}
		if err := worktree.Remove(f, project, task.Worktree); err != nil {
			log.Printf("teams: removing %s: %v", task.Worktree, err)
		}
	}
}
```

In `wsserver/team_run.go`, the `switch` becomes:

```go
	switch m.Type {
	case "team.start":
		err = c.s.Teams.Start(m.Lead, m.Tasks)
	case "team.stop":
		err = c.s.Teams.Stop(m.Lead, m.Task)
	case "team.merge":
		err = c.s.Teams.Merge(m.Lead)
	}
```

In `wsserver.go`: the dispatch case becomes `case "team.start", "team.stop", "team.merge":`, and plan 3's `agent.prompt` case becomes (the team branch as plan 3 wrote it; only a plain prompt is noted, with the text the user typed):

```go
	case "agent.prompt":
		if m.Team {
			err = c.s.Teams.Prompt(a, m.Text)
		} else if err = a.Driver().Prompt(m.Text); err == nil {
			c.s.Teams.Told(a.ID(), m.Text)
		}
```

**Step 3: Run the tests to verify they pass**

Run: `gofmt -l ./internal && go vet ./... && go test ./e2e -run TestATeam -count=1 -v`
Expected: nothing listed; vet clean; both `TestATeamsWorkersHandBackInOrderAndTheLeadReviews` and `TestATeamNotesWhatTheUserToldAWorkerAndMergesIntoTheLeadsCheckout` PASS

**Step 4: Run everything**

Run: `go test ./... && (cd ../.. && pnpm --filter @pocket/protocol test)`, then `cd packages/ios/Packages/AnywhereKit && PATH="$HOME/.swiftly/bin:$PATH" swift test --filter GoldenTests`
Expected: PASS

---

## PR 4: Teams pick up where a restart left them

**Scope:** Once pocketd is back and the restored sessions have come back or failed (`daemon.Daemon.WaitRestored`), `Teams.Recover` reconciles every team that was planning, running, in review or merging. A worker that is back shows its status, or hands back if its last turn ended normally and wasn't handed back already. A worker that is gone fails its task with `the worker stopped when pocketd restarted` and the lead is mailed the failed handback. A team stuck merging aborts the merge in progress in its recorded checkout (`Team.Checkout`, so even with the lead gone) and shows `conflict`, or goes back to `review`, either way with an `Error`. A planning team whose lead is gone fails; one whose lead is back and idle has its last turn read again. A shutdown's closing sessions no longer fail tasks.
**Depends on:** PR 3
**Done when:** `go test ./e2e -run 'TestARestart|TestATeam' -count=1` passes, and `go test ./...` is green.

### Task 4.1: The e2e test for a restart

**Files:**
- Create: `packages/pocketd/e2e/team_restart_test.go`

**Context:** The harness doesn't resume agents unless a test sets `h.resume` (a resumed agent would run the real claude from the login shell's PATH), so after `h.Restart()` neither the lead nor the worker comes back: the worker is gone, which is what this test needs. t1's instructions run `make build`, which its allowlist doesn't hold, so its worker waits on the user (`needsYou`) when pocketd restarts. Recovery may run before or after the new phone connects, so the test asks `team.list` and takes the team from either the list or a later `team.update`. Mail to an agent that isn't there waits in the mailbox; plan 2's `stop` and `unacked` (`e2e/mail_test.go`) read it with pocketd stopped. Reuse `teamOf`, `taskIn`, `replyTo`, `gitIdentity`, `create`, `inRepo`, `reply`.

**Step 1: Write the failing test**

```go
//go:build unix

package e2e

import (
	"encoding/json"
	"reflect"
	"testing"

	"pocketd/internal/proto"
)

// askingPlan is a lead's plan whose t1 runs `make build`, which its allowlist
// doesn't hold, so t1's worker waits on the user; t2 starts from t1's work.
func askingPlan(t *testing.T) string {
	t.Helper()
	plan, err := json.Marshal(map[string]any{"tasks": []map[string]any{
		{"id": "t1", "title": "Build a", "agent": "claude", "files": []string{"a.txt"}, "after": []string{}, "allow": []string{"make test"},
			"instructions": "write a.txt one\nrun make build"},
		{"id": "t2", "title": "Write b", "agent": "claude", "files": []string{"b.txt"}, "after": []string{"t1"}, "allow": []string{},
			"instructions": "write b.txt two"},
	}})
	if err != nil {
		t.Fatal(err)
	}
	return "<anywhere_team_plan>\n" + string(plan) + "\n</anywhere_team_plan>"
}

// leadsTeam is lead's team in a team.update or a team.list reply.
func leadsTeam(lead string, m Message) (proto.Team, bool) {
	if m.Type == "team.update" && m.Team.Lead == lead {
		return m.Team, true
	}
	if m.Type == "team.list" {
		for _, tm := range m.Teams {
			if tm.Lead == lead {
				return tm, true
			}
		}
	}
	return proto.Team{}, false
}

func TestARestartFailsATaskWhoseWorkerDidNotComeBackAndTellsTheLead(t *testing.T) {
	h := Start(t, launchReady(""), gitIdentity)
	o := h.Owner()
	create(o, "lead", inRepo(h, map[string]any{"chat": true}))
	created := reply(o, "agent.created", "error")
	if created.Type != "agent.created" {
		t.Fatalf("got %s", created.Raw)
	}
	lead := created.AgentID
	h.WaitScreen(created.TerminalID, "fake claude ready")
	o.Send(map[string]any{"type": "agent.prompt", "id": "plan", "agentId": lead, "text": askingPlan(t), "team": true})
	o.WaitFor("the proposed plan", teamOf(lead, "proposed"))
	o.Send(map[string]any{"type": "team.start", "id": "start", "lead": lead, "tasks": []map[string]any{
		{"id": "t1", "agent": "claude", "allow": []string{"make test"}},
		{"id": "t2", "agent": "claude", "allow": []string{}},
	}})
	if m := o.WaitFor("team.start's reply", replyTo("start")); m.Type != "ack" {
		t.Fatalf("team.start: %s", m.Raw)
	}
	o.WaitFor("t1 asking to run make build", taskIn(lead, "t1", "needsYou"))

	h.Restart()
	again := h.Owner()
	again.Send(map[string]any{"type": "team.list", "id": "tl"})
	m := again.WaitFor("t1 failed by the restart", func(m Message) bool {
		tm, ok := leadsTeam(lead, m)
		return ok && len(tm.Tasks) > 0 && tm.Tasks[0].State == "failed"
	})
	team, _ := leadsTeam(lead, m)
	if team.State != "running" || team.Tasks[0].Summary != "the worker stopped when pocketd restarted" || team.Tasks[1].State != "waiting" {
		t.Fatalf("team %+v", team)
	}
	stop(t, h)
	want := []string{"<anywhere_team_handback task=\"t1\" status=\"failed\">\nthe worker stopped when pocketd restarted\n</anywhere_team_handback>"}
	if got := unacked(t, h, lead); !reflect.DeepEqual(got, want) {
		t.Fatalf("the lead's mail %q", got)
	}
}
```

What it proves: a task whose worker didn't survive the restart fails with the restart as its reason, its dependent keeps waiting, the team stays running, and the lead is mailed exactly one failed handback. Shutting down didn't fail it first.

**Step 2: Run the test to verify it fails**

Run: `go test ./e2e -run TestARestart -count=1`
Expected: FAIL. Either `waiting for t1 failed by the restart: context deadline exceeded` or, when the shutdown's close was handled first, `team {… Summary:The worker's session closed …}`.

### Task 4.2: Find and abort a merge in progress

**Files:**
- Modify: `packages/pocketd/internal/worktree/merge.go`
- Test: `packages/pocketd/internal/worktree/merge_test.go`

**Context:** A restart during `Merge` can leave the lead's checkout mid-merge (`MERGE_HEAD` exists). `git rev-parse -q --verify MERGE_HEAD` exits 1 with no output when there is none. `gitIn` turns git's exit into a plain error, so `MergeInProgress` runs git itself to tell "no merge" from a failure. The test makes the conflicting merge with `exec.Command`, since the `git` helper fails the test on any error: add `"os/exec"` to `merge_test.go`'s imports.

**Step 1: Write the failing test** (append to `merge_test.go`)

```go
func TestAMergeInProgressIsFoundAndAborted(t *testing.T) {
	p := repo(t)
	branchWith(t, p, "t1", "same.txt", "one\n")
	commitFile(t, p, "same.txt", "main\n")
	if busy, err := MergeInProgress(p); err != nil || busy {
		t.Fatalf("before the merge: %t, %v", busy, err)
	}
	if out, err := exec.Command("git", "-C", p, "merge", "--no-edit", "t1").CombinedOutput(); err == nil {
		t.Fatalf("the merge didn't conflict:\n%s", out)
	}
	if busy, err := MergeInProgress(p); err != nil || !busy {
		t.Fatalf("during the merge: %t, %v", busy, err)
	}
	if err := AbortMerge(p); err != nil {
		t.Fatal(err)
	}
	if busy, _ := MergeInProgress(p); busy || git(t, p, "status", "--porcelain") != "" {
		t.Fatal("the merge is still in progress")
	}
}
```

**Step 2: Run the test to verify it fails**

Run: `go test ./internal/worktree -run TestAMergeInProgress -count=1`
Expected: FAIL, build error `undefined: MergeInProgress` and `undefined: AbortMerge`

**Step 3: Write the implementation** (append to `merge.go`)

```go
// MergeInProgress is whether dir's checkout is in the middle of a merge.
func MergeInProgress(dir string) (bool, error) {
	err := exec.Command("git", "-C", dir, "rev-parse", "-q", "--verify", "MERGE_HEAD").Run()
	if _, ok := errors.AsType[*exec.ExitError](err); ok {
		return false, nil
	}
	return err == nil, err
}

// AbortMerge aborts the merge in progress in dir's checkout.
func AbortMerge(dir string) error {
	_, err := gitIn(dir, "merge", "--abort")
	return err
}
```

**Step 4: Run the tests to verify they pass**

Run: `go test ./internal/worktree -count=1`
Expected: PASS

### Task 4.3: Decisions for a restart

**Files:**
- Create: `packages/pocketd/internal/team/recover.go`
- Test: `packages/pocketd/internal/team/recover_test.go`

**Context:** `Recovered` decides, from what a restart left (`Facts`, gathered by Task 4.5's glue), what a team becomes and what is left to do (`Recovery`), so the glue only gathers and acts. A worker counts as back unless it's missing, closed, or its restore failed; the glue only reads the last turn of a worker that is back and not mid-turn. In `running` or `review`, each task in a turn (`inTurn`): a worker at work shows its status (`WorkerStatus`); one whose last turn ended normally hands back (the glue queues it as a turn end, and `handback` records it), unless that turn, named by its prompt's id (`Worker.Turn`), is the one the task last handed back (`TeamTask.Turn`, Task 2.4); one whose last turn failed fails as `work` does; any other (gone, or back without a finished turn, or a task still launching with no worker) fails with `WorkerRestarted`. Every task failed here is listed so the glue mails the lead. `merging`: a merge in progress in `Team.Checkout` (Task 3.4 records it; the glue looks there, so the lead needn't be back) is aborted and the team shows `conflict` (no files: the merge didn't finish, so they aren't known) with `MergeRestarted`; otherwise it goes back to `review` with it. `planning`: a lead that is gone fails the team; one back and idle or done has its turn read again (`Teams.Ended`); a working one ends its turn normally. `t1Working`, `reviewedPair` and `testLead` are in `run_test.go`.

**Step 1: Write the failing tests** (`recover_test.go`)

```go
package team

import (
	"reflect"
	"testing"

	"pocketd/internal/proto"
)

func TestARestartLeavesEachTeamAsWhatItsAgentsAndCheckoutSay(t *testing.T) {
	asking, _ := WorkerStatus(t1Working(t), "w1", "needsYou")
	turnFailed, _ := Failed(t1Working(t), "t1", "The worker's turn failed")
	gone, _ := Failed(t1Working(t), "t1", WorkerRestarted)
	launching := Starting(startedPlan(t), "t1", nil)
	launchingGone, _ := Failed(launching, "t1", WorkerRestarted)
	planning := proto.Team{Lead: testLead, State: "planning"}
	handed := t1Working(t)
	handed.Tasks[0].Turn = "u1"
	merging, _ := Merging(reviewedPair())
	aborted := Conflicted(merging, nil)
	aborted.Error = MergeRestarted
	for name, c := range map[string]struct {
		team    proto.Team
		facts   Facts
		want    proto.Team
		r       Recovery
		changed bool
	}{
		"a worker at work shows its status":               {t1Working(t), Facts{Workers: map[string]Worker{"t1": {Status: "needsYou"}}}, asking, Recovery{}, true},
		"a worker still on its turn changes nothing":      {t1Working(t), Facts{Workers: map[string]Worker{"t1": {Status: "working"}}}, t1Working(t), Recovery{}, false},
		"a worker that finished hands back":               {t1Working(t), Facts{Workers: map[string]Worker{"t1": {Status: "done", Turn: "u1", Ended: true, OK: true}}}, t1Working(t), Recovery{HandBack: []string{"t1"}}, false},
		"a turn already handed back isn't again":          {handed, Facts{Workers: map[string]Worker{"t1": {Status: "done", Turn: "u1", Ended: true, OK: true}}}, handed, Recovery{}, false},
		"the worker's next turn hands back":               {handed, Facts{Workers: map[string]Worker{"t1": {Status: "done", Turn: "u2", Ended: true, OK: true}}}, handed, Recovery{HandBack: []string{"t1"}}, false},
		"a worker whose turn failed fails":                {t1Working(t), Facts{Workers: map[string]Worker{"t1": {Status: "done", Ended: true}}}, turnFailed, Recovery{Failed: []string{"t1"}}, true},
		"a worker that is gone fails":                     {t1Working(t), Facts{Workers: map[string]Worker{"t1": {}}}, gone, Recovery{Failed: []string{"t1"}}, true},
		"a worker back without its turn's end fails":      {t1Working(t), Facts{Workers: map[string]Worker{"t1": {Status: "idle"}}}, gone, Recovery{Failed: []string{"t1"}}, true},
		"a task still launching fails":                    {launching, Facts{}, launchingGone, Recovery{Failed: []string{"t1"}}, true},
		"a planning lead that is gone fails the team":     {planning, Facts{}, proto.Team{Lead: testLead, State: "failed", Error: LeadRestarted}, Recovery{}, true},
		"a planning lead back and idle is read again":     {planning, Facts{Lead: "idle"}, planning, Recovery{Reread: true}, false},
		"a planning lead still working is left to finish": {planning, Facts{Lead: "working"}, planning, Recovery{}, false},
		"a merge in progress is aborted as a conflict":    {merging, Facts{Merging: true}, aborted, Recovery{Abort: true}, true},
		"a merge not in progress goes back to review":     {merging, Facts{}, MergeFailed(merging, MergeRestarted), Recovery{}, true},
		"a proposed plan is left as it is":                {planOfThree(), Facts{}, planOfThree(), Recovery{}, false},
	} {
		got, r, changed := Recovered(c.team, c.facts)
		if !reflect.DeepEqual(got, c.want) || !reflect.DeepEqual(r, c.r) || changed != c.changed {
			t.Errorf("%s: got %t %+v\n%+v", name, changed, r, got)
		}
	}
}
```

What it proves: each rule above, including that a worker whose turn ended normally is handed back without the team changing here (the handback records it), that the same turn arriving again (already recorded on its task) isn't handed back twice while the worker's next turn is, and that only a merge actually in progress is aborted.

**Step 2: Run the test to verify it fails**

Run: `go test ./internal/team -run TestARestart -count=1`
Expected: FAIL, build error `undefined: WorkerRestarted` (and `Recovered`, `Facts`, `Worker`, `Recovery`, `LeadRestarted`, `MergeRestarted`)

**Step 3: Write the implementation** (`recover.go`)

```go
package team

import "pocketd/internal/proto"

// Why a restart failed a task or a team, or stopped a merge.
const (
	WorkerRestarted = "the worker stopped when pocketd restarted"
	LeadRestarted   = "the lead stopped when pocketd restarted"
	MergeRestarted  = "pocketd restarted during the merge"
)

// Worker is what a restart left of a task's worker.
type Worker struct {
	// Status is its status now; "" when it didn't come back.
	Status string
	// Turn names its last turn by its prompt's id (TurnID).
	Turn string
	// Ended is whether its last turn has ended; OK whether that turn ended normally.
	Ended, OK bool
}

// Facts is what a restart left of a team's lead, its workers in a turn (by task id) and the lead's checkout.
type Facts struct {
	// Lead is the lead's status; "" when it didn't come back.
	Lead    string
	Workers map[string]Worker
	// Merging is whether a merge is in progress in the team's checkout.
	Merging bool
}

// Recovery is what is left to do once a recovered team is saved.
type Recovery struct {
	// HandBack lists tasks whose worker finished; Failed those failed here, for the lead to hear.
	HandBack, Failed []string
	// Abort aborts the merge in the team's checkout; Reread reads the planning lead's last turn again.
	Abort, Reread bool
}

// Recovered is t once a restart left f, what is left to do, and whether t changed.
func Recovered(t proto.Team, f Facts) (proto.Team, Recovery, bool) {
	var r Recovery
	switch t.State {
	case "planning":
		switch f.Lead {
		case "":
			t.State, t.Error = "failed", LeadRestarted
			return t, r, true
		case "idle", "done":
			r.Reread = true
		}
		return t, r, false
	case "merging":
		if !f.Merging {
			return MergeFailed(t, MergeRestarted), r, true
		}
		t = Conflicted(t, nil)
		t.Error, r.Abort = MergeRestarted, true
		return t, r, true
	case "running", "review":
	default:
		return t, r, false
	}
	changed := false
	for _, task := range t.Tasks {
		if !inTurn(task.State) {
			continue
		}
		w, edited := f.Workers[task.ID], false
		switch {
		case w.Status == "working" || w.Status == "needsYou":
			t, edited = WorkerStatus(t, task.Worker, w.Status)
		case w.Ended && w.OK:
			if w.Turn == "" || w.Turn != task.Turn {
				r.HandBack = append(r.HandBack, task.ID)
			}
		case w.Ended:
			t, edited = Failed(t, task.ID, "The worker's turn failed")
			r.Failed = append(r.Failed, task.ID)
		default:
			t, edited = Failed(t, task.ID, WorkerRestarted)
			r.Failed = append(r.Failed, task.ID)
		}
		changed = changed || edited
	}
	return t, r, changed
}
```

**Step 4: Run the tests to verify they pass**

Run: `gofmt -l ./internal/team && go test ./internal/team -count=1`
Expected: nothing listed; PASS

### Task 4.4: `daemon.Daemon.WaitRestored`

**Files:**
- Modify: `packages/pocketd/internal/daemon/restore.go` (`hint`; `finish`; new `settle`, `WaitRestored`, `restored`)
- Test: `packages/pocketd/internal/daemon/restore_test.go`

**Context:** Each saved agent waits in `d.restoring` as a `hint` until `finish` settles it: resumed (the hint is deleted, the provider's driver takes over) or failed (`over`, then `SetAttached(false)` and `SetRestore(failed)` after `mu` is released). Recovery must not read a worker before then, or a worker still resuming would count as gone, and a failed one as back. `over` is set before the agent shows the outcome, so the hint gains `settled`, set when `finish` returns. A hint a switch adds later (`switch.go`) waits too; that's harmless. The test reuses `resuming`, `savedAgent`, `restoredIn` and `newDaemon`; add `"context"` to `restore_test.go`'s imports.

**Step 1: Write the failing test** (append to `restore_test.go`)

```go
func TestWaitRestoredReturnsOnceTheRestoreHasFailed(t *testing.T) {
	old := RestoreWait
	RestoreWait = 200 * time.Millisecond
	t.Cleanup(func() { RestoreWait = old })
	d := newDaemon(t)
	resuming(d, "/bin/sh", "-c", "sleep 30")
	d.Restore(savedAgent(t.TempDir(), "claude"), "/bin/sh")
	_, a := restoredIn(t, d)
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	if err := d.WaitRestored(ctx); err != nil {
		t.Fatal(err)
	}
	if s := a.Summary(); s.Restore != proto.RestoreFailed || s.Attached {
		t.Fatalf("summary = %+v", s)
	}
}
```

What it proves: `WaitRestored` doesn't return while the agent is still resuming, and once it does the agent shows its failed restore.

**Step 2: Run the test to verify it fails**

Run: `go test ./internal/daemon -run TestWaitRestored -count=1`
Expected: FAIL, build error `d.WaitRestored undefined`

**Step 3: Write the implementation**

In `restore.go`, add `"context"` to the imports. In `hint`, after `over    bool`:

```go
	settled bool // finish has returned: the agent shows how its restore went
```

In `finish`, right after `h.over = true`:

```go
	defer d.settle(h)
```

Add after `finish`:

```go
func (d *Daemon) settle(h *hint) {
	d.mu.Lock()
	h.settled = true
	d.mu.Unlock()
}

// WaitRestored returns once every saved agent has come back or failed to, or with ctx's error.
func (d *Daemon) WaitRestored(ctx context.Context) error {
	for !d.restored() {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(100 * time.Millisecond):
		}
	}
	return nil
}

func (d *Daemon) restored() bool {
	d.mu.Lock()
	defer d.mu.Unlock()
	for _, h := range d.restoring {
		if !h.settled {
			return false
		}
	}
	return true
}
```

**Step 4: Run the tests to verify they pass**

Run: `go test ./internal/daemon -count=1`
Expected: PASS

### Task 4.5: `Teams.Recover`, and wiring it after the restore

**Files:**
- Create: `packages/pocketd/internal/team/recovering.go`
- Modify: `packages/pocketd/internal/team/teams.go` (plan 3's `Run`)
- Modify: `packages/pocketd/cmd/pocketd/serve.go` (after `go d.Watch(context.Background())`)

**Context:** Glue only: gather `Facts`, apply `Recovered` through `change`, then act on the `Recovery`. A worker is back unless `Agents.Get` fails, its status is `closed` or its restore failed (`proto.RestoreFailed`). Its last turn is read like plan 3's `read` (`Reply` and `TurnID` over the newest `turnItems` items, up to `reads` times, `Poll` apart) and only for a worker back and not mid-turn. A merge is looked for, and aborted, in `Team.Checkout`, which `Merge` records (Task 3.4), so a lead that didn't come back doesn't matter. A team with no `Checkout` (none should be `merging`) is taken as having no merge in progress and goes back to review: the safe side, since pocketd then writes nothing to a folder it can't name, and git refuses the next merge while one is in progress, which shows as that merge's `Error`. `SetAwaiting` lives in memory, so `awaits` runs for every team to show a proposed team's lead `needsYou` again. `pump` starts what failed tasks no longer hold back. `Recover` runs on its own goroutine after `WaitRestored`, so it doesn't hand back itself: it queues each handback as an ok turn end (`post`), which `Run` handles through `work` and `handback` like any other. So every handback runs on `Run`'s goroutine (two never commit in the same worktree at once), and a turn end heard both after the restart and from recovery hands back once: `handback` and `Handed` skip a turn already recorded (Task 2.4). Shutting down closes every session after `ctx` is cancelled (`serve.go`'s `NotifyContext`), so `Run` stops before handling any event queued after that: otherwise a shutdown would fail every running task as `The worker's session closed`.

**Step 1: The failing test is Task 4.1's.**

**Step 2: Write the implementation**

`internal/team/recovering.go`:

```go
package team

import (
	"cmp"
	"log"
	"time"

	"pocketd/internal/proto"
	"pocketd/internal/worktree"
)

// Recover picks up every team where pocketd's restart left it (Recovered). Call it once the restored sessions
// have come back or failed (daemon.WaitRestored), so a worker still resuming isn't taken for gone.
func (t *Teams) Recover() {
	if t == nil {
		return
	}
	teams, err := t.Store.List()
	if err != nil {
		log.Printf("teams: %v", err)
		return
	}
	for _, team := range teams {
		t.pickUp(team)
	}
}

// pickUp applies Recovered to team, then does what is left.
func (t *Teams) pickUp(team proto.Team) {
	lead, f := team.Lead, t.facts(team)
	var r Recovery
	next, _ := t.change(lead, func(tm proto.Team) (proto.Team, bool) {
		var changed bool
		tm, r, changed = Recovered(tm, f)
		return tm, changed
	})
	if r.Abort {
		if err := worktree.AbortMerge(next.Checkout); err != nil {
			log.Printf("teams: %s: aborting the merge: %v", lead, err)
		}
	}
	for _, id := range r.Failed {
		t.tell(lead, FailedHandback(id, next.Tasks[taskAt(next, id)].Summary))
	}
	for _, id := range r.HandBack {
		t.post(event{kind: "turn", agent: next.Tasks[taskAt(next, id)].Worker, ok: true})
	}
	if r.Reread {
		t.Ended(lead)
	}
	t.awaits(lead, next.State)
	t.pump(lead)
}

// facts is what the restart left of team's lead, its workers in a turn, and a merging lead's checkout.
func (t *Teams) facts(team proto.Team) Facts {
	f := Facts{Lead: t.back(team.Lead), Workers: map[string]Worker{}}
	for _, task := range team.Tasks {
		if !inTurn(task.State) {
			continue
		}
		w := Worker{Status: t.back(task.Worker)}
		if w.Status != "" && w.Status != "working" && w.Status != "needsYou" {
			w.Turn, w.Ended, w.OK = t.lastTurn(task.Worker)
		}
		f.Workers[task.ID] = w
	}
	if team.State == "merging" && team.Checkout != "" {
		var err error
		if f.Merging, err = worktree.MergeInProgress(team.Checkout); err != nil {
			log.Printf("teams: %s: %v", team.Lead, err)
		}
	}
	return f
}

// back is id's status, or "" when it didn't come back from the restart.
func (t *Teams) back(id string) string {
	if id == "" {
		return ""
	}
	a, err := t.Agents.Get(id)
	if err != nil {
		return ""
	}
	s := a.Summary()
	if s.Status == "closed" || s.Restore == proto.RestoreFailed {
		return ""
	}
	return s.Status
}

// lastTurn is id's last turn by its prompt's id, whether it has ended (looking again while the transcript catches
// up), and whether it ended normally.
func (t *Teams) lastTurn(id string) (turn string, ended, ok bool) {
	a, err := t.Agents.Get(id)
	if err != nil {
		return "", false, false
	}
	for i := range reads {
		if i > 0 {
			time.Sleep(cmp.Or(t.Poll, time.Second))
		}
		items, _ := a.Timeline.Page(0, turnItems)
		if _, done, failed := Reply(items); done {
			return TurnID(items), true, !failed
		}
	}
	return "", false, false
}
```

In `teams.go`, `Run`'s inner loop becomes:

```go
		for _, e := range queue {
			// Shutting down closes every session; those closes aren't workers failing.
			if ctx.Err() != nil {
				return
			}
			t.handle(e)
		}
```

In `serve.go`, right after `go d.Watch(context.Background())`:

```go
	go func() {
		if d.WaitRestored(ctx) == nil {
			teams.Recover()
		}
	}()
```

**Step 3: Run the tests to verify they pass**

Run: `gofmt -l ./internal ./cmd && go vet ./... && go test ./e2e -run 'TestARestart|TestATeam' -count=1 -v`
Expected: nothing listed; vet clean; `TestARestartFailsATaskWhoseWorkerDidNotComeBackAndTellsTheLead` and both `TestATeam…` tests PASS

**Step 4: Run everything**

Run: `go test -race -count=1 ./...`
Expected: PASS

---

## Open questions

- **A failed task blocks its dependents** and the team never reaches review. Is a retry verb (or "start it again") wanted, or does the user prompt the worker and let the next ok turn hand back (which works today)?
- **Codex failed turns read as ok**: Codex's thread goes `idle` after a failed turn and the daemon reports it as a normal end. Only `systemError` fails.
- **A compaction the user starts from idle ends like a turn** and makes a worker hand back again (harmless: the commit is skipped when clean).
- **Commit signing.** A user with `commit.gpgsign` and a passphrase prompt would block the worker commit and the merge.
- **git ≥ 2.38** is needed for `merge-tree --write-tree`; pocketd has no version check.
- **Closing the lead** leaves its workers running; nothing cascades (plan 1's open question). This plan doesn't close workers until a full merge.
- **Plan 2's e2e mail tests** still queue mail by opening `mail.db` from the test; this plan's e2e covers the real senders (handbacks, notes) without moving them.

Resolved by the UX decisions: Start may keep a single task (`ValidateKept`, Task 2.3); restarts are reconciled (PR 4), handing each turn back once (`TeamTask.Turn`) and aborting a merge in the recorded checkout (`Team.Checkout`) even with the lead gone.

## Contract changes needed

All folded into the index (`docs/plans/2026-11-05-team-mode.md`): `agent.Registry.OnTurnEnd`, `broker.Broker.Allow`, `proto.TaskPick` and the `ClientMessage` fields, one engine (`Teams`, extended here; no `Runner`), first-free branch names, `team.start` refusing to drop a needed task, "Not ready to merge" isn't ready, the three-dot stat, root tasks from the lead's HEAD commit, the assignment after `launch.Create`, a failed task's reason in `Summary`, Swift mirrors for the new client goldens, `team.start` keeping 1 to 4 tasks (`ValidateKept`), restart recovery (`Recovered`, `Teams.Recover`, `daemon.Daemon.WaitRestored`, `worktree.MergeInProgress`/`AbortMerge`), `TeamTask.turn` and `Team.checkout` on the wire.
