# Worktree Gaps (open branch/PR, delete with teardown, PR status) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** Open an existing branch or GitHub PR as a worktree, delete worktrees with a teardown script and optionally their branch, and show PR status/CI with a Create PR action.

**Architecture:** pocketd creates worktrees, so opening a branch or PR is two new `checkout.new` fields (`branch`, `pr`) behind cap `open.v1`, handled in `pd/internal/worktree/open.go` and `pr.go`. The desktop deletes and displays, so teardown/branch deletion live in a new `pocket/src/removal.rs` feature module, and PR status in `git/src/github.rs` (parsing) plus `pocket/src/git_ui/pull_requests.rs` (scheduling, driven by `refresh_git`, one `gh` at a time). Design: `docs/designs/2026-10-05-worktree-gaps.md`.

**Toolset:**
- Go (from `packages/pocketd`): `go test ./internal/worktree/ -run <Name>`, `go vet ./... && go test ./...`
- Rust (from `packages/desktop`): `cargo test -p <crate> <name>`; before each PR is done: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace` (no new warnings)
- Protocol TS (from `packages/protocol`): see Task 1.1 for the exact command; a fresh worktree needs `pnpm install` first
- UI captures: `.ui-review/fixture/capture.sh <dir> <name>=<steps>…` before and after view changes

**Read first:**
- `CLAUDE.md` (repo root): desktop layout and performance rules every task must follow.
- `docs/adr/0003-desktop-code-layout.md`: where new code goes (model crates vs `pocket`, one feature module with its own state).
- `docs/designs/2026-10-05-worktree-gaps.md`: the settled decisions this plan implements.

**Executing:** line numbers cite `4074f80` and drift as earlier PRs land (PR 3 and PR 4 both edit `desktop.rs` and the `sidebar.rs` worktree row; PR 3 renames `ui::setting_up` to `ui::busy`). Anchor every edit on the quoted code, not the line number. Run PRs in order, one task at a time.

**Order:** PR 1 → PR 2 ship together (they share cap `open.v1`). PR 3, PR 4 are independent of PR 1–2 and of each other. PR 5 depends on PR 4.

---

## PR 1: Open an existing branch as a worktree

**Scope:** `checkout.new.branch` on the wire, behind the cap `open.v1`. pocketd checks out a local or origin branch in a new Worktree, or adopts the Worktree already on it. The desktop sheet gets a "Start from" chip (New branch / Existing branch), shown only when pocketd offers `open.v1`.
**Depends on:** nothing.
**Done when:** a branch that is only on origin opens in `<worktrees>/<branch with / as ->` and tracks origin; a branch already checked out opens its Worktree with no copy or setup; the desktop sends `{"new":{"branch":…}}` and reopens a failed open on the same branch; all three suites pass.

Facts the tasks rely on:
- Caps are an intersection (`Negotiate`), so pocketd must list `open.v1` in `ServerCaps` *and* the desktop must send it in its hello `CAPS`.
- Failures the person can act on are `coded("spawn_failed", msg)`. `gitFailure` in launch passes a coded error's message through as is, so no new error codes are needed.
- `Create::new` (creating.rs:83) shows every step for a `new` checkout, and `reach` drops the ones pocketd skips. An adopted branch therefore shows prepare/verify/fetch/worktree/agent, with no extra desktop code.
- `Creates::started` swaps the provisional path for pocketd's real cwd, so a provisional folder that guesses wrong (a taken `fix-login`, or an adopted Worktree) corrects itself.

### Task 1.1: Wire field, cap and goldens

**Files:**
- `packages/pocketd/internal/proto/launch.go:8` (cap), `:33-38` (NewWorktree), `:141` (strictObject), `:150` (name check)
- `packages/pocketd/internal/proto/version.go:21` (ServerCaps)
- `packages/pocketd/internal/proto/registry_caps_test.go` (append)
- `packages/pocketd/internal/proto/golden_test.go:180` (rejects), `:202` (accepts)
- new `packages/pocketd/internal/proto/testdata/golden/client/agent_create_branch.json`
- `packages/protocol/src/launch.ts:9-14`

**Context:**
- `decodeSpec` checks keys with `strictObject` before it unmarshals. Unmarshal alone would also accept `"Branch"`, so the key list is the real guard.
- TS `golden.test.mjs` reads every file in `golden/client` and decodes it with `onExcessProperty: "error"`. The new golden fails in both Go (`TestClientGolden`) and TS until both schemas know `branch` and an optional `name`.
- `packages/app/src/launch.ts:29` builds `{ new: { name } }` and still typechecks once `name` is optional. The phone UI is a non-goal, so no `CAP_OPEN` goes in `constants.ts`.

**Step 1: Failing tests**

`registry_caps_test.go`, append:
```go
func TestTheServerOffersTheOpenCap(t *testing.T) {
	_, caps, code := Negotiate(Range{Min: 3, Max: 3}, []string{CapOpen}, ServerCaps)
	if code != "" || !slices.Equal(caps, []string{CapOpen}) {
		t.Fatalf("caps %v, code %q", caps, code)
	}
}
```
`golden_test.go`, add to `TestDecodeClientRejects` after line 180:
```go
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"new":{}},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"new":{"branch":"b","base":"main"}},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"new":{"name":"n","Branch":"b"}},"provider":"claude","access":"ask","plan":false}}`,
```
Add to `TestDecodeClientAcceptsWhatTheSchemaAccepts` after line 202:
```go
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"new":{"branch":"b"}},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"new":{"name":"n","branch":"b"}},"provider":"claude","access":"ask","plan":false}}`,
```
`testdata/golden/client/agent_create_branch.json`:
```json
{"type":"agent.create","id":"c3","requestId":"r3","spec":{"project":"/Users/me/pocket","checkout":{"new":{"branch":"fix/login","copy":true,"setup":true}},"provider":"claude","access":"ask","plan":false}}
```

**Step 2: Run, expect failure**
```
cd packages/pocketd && go test ./internal/proto/
```
Expected: a build error `undefined: CapOpen`. With the cap stubbed, `TestClientGolden` rejects `agent_create_branch.json` and the accepts test fails on the two `branch` lines.
```
cd packages/protocol && pnpm test
```
Expected: `agent_create_branch.json` fails with `name … is missing`.

**Step 3: Implement**

`launch.go`, after `const CapLaunch`:
```go
// CapOpen: a NewWorktree may open an existing branch instead of naming a new one.
const CapOpen = "open.v1"
```
```go
type NewWorktree struct {
	Name string `json:"name"`
	Base string `json:"base,omitempty"`
	// Branch is an existing local branch, or one origin has.
	Branch string `json:"branch,omitempty"`
	Copy   *bool  `json:"copy,omitempty"`
	Setup  *bool  `json:"setup,omitempty"`
}

// valid: at most one of Base and Branch, and a Name unless opening a Branch.
func (n *NewWorktree) valid() bool {
	if n.Branch != "" {
		return n.Base == ""
	}
	return n.Name != ""
}
```
Line 141: `strictObject(n, "name", "base", "branch", "copy", "setup")`.
Line 150: `if (c.Worktree == "") == (c.New == nil) || c.New != nil && !c.New.valid() {`.

`version.go:21`: append `CapOpen` to `ServerCaps` (`…, CapLaunch, CapRestore, CapOpen}`).

`packages/protocol/src/launch.ts`:
```ts
export const NewWorktree = Schema.Struct({
  name: Schema.optional(Schema.String),
  base: Schema.optional(Schema.String),
  branch: Schema.optional(Schema.String),
  copy: Schema.optional(Schema.Boolean),
  setup: Schema.optional(Schema.Boolean),
});
```

**Step 4: Run, expect pass**
```
cd packages/pocketd && go test ./internal/proto/
cd packages/protocol && pnpm test
```
Expected: `ok pocketd/internal/proto`; TS 60 tests pass.

### Task 1.2: `worktree.Open`, a branch ready to check out

**Files:**
- new `packages/pocketd/internal/worktree/open.go`
- new `packages/pocketd/internal/worktree/open_test.go`
- reads, unchanged: `create.go` (`Dir`, `Validate`, `ancestor`, `coded`, `Code`, `ErrUnknownProject`, `Created`), `list.go:25` (`List`), and the test helpers `repo`, `pushed`, `registered` and `git` in the existing `*_test.go`

**Context:**
- **Prune first.** A Worktree whose folder was deleted still holds its branch until `git worktree prune`. Without the prune, `worktree add` fails with "already checked out" and List reports a dead folder to adopt.
- **Validate the branch.** `check-ref-format` accepts a name that starts with `-`. Such a name would read as a flag, so it is rejected by hand, and `--` comes before path and branch in `worktree add`.
- **Fetch one ref.** Fetch only `+refs/heads/B:refs/remotes/origin/B`, with a 15s timeout, `GIT_TERMINAL_PROMPT=0` and the login env, as `Plan.Fetch` does. A failed fetch (no origin, offline, branch not there) just means there is no remote copy.
- **The table from the design, made explicit:**
  - origin only: `git branch --track B origin/B`. This sets `created`, so rollback can delete it.
  - local behind origin: `update-ref refs/heads/B <remote> <local>`, which fast-forwards with an old-value guard, plus the note "Fast-forwarded B".
  - diverged: keep local, plus the note "B differs from origin/B".
  - neither: `No branch B`.

  The branch is made in Prepare rather than through `worktree add --track -b`, so that Add and rollback are the same for every case.
- **Adopt** applies when `List` shows a Worktree on B, the main checkout included. Nothing is added, and the note is "Already open in <folder>".
- **Folder.** The folder is the branch with `/` as `-`, then `-2`, `-3`… compared case-insensitively against what is in `Dir`. A given `name` goes through `Validate` against folders only. Unlike `Prepare`, it is not checked against branch names, since the branch exists on purpose.
- **Rollback** deletes the branch only if this Open made it and it still sits where it was made.

**Step 1: Failing test** — `open_test.go`:
```go
package worktree

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	"pocketd/internal/registry"
)

// commitOn adds a commit to ref in the bare repo, starting ref at main when
// the bare repo lacks it.
func commitOn(t *testing.T, bare, ref string) string {
	t.Helper()
	parent := "main"
	if exec.Command("git", "-C", bare, "rev-parse", "-q", "--verify", ref).Run() == nil {
		parent = ref
	}
	c := git(t, bare, "commit-tree", "-p", parent, "-m", ref, git(t, bare, "rev-parse", "main^{tree}"))
	git(t, bare, "update-ref", ref, c)
	return c
}

// origin is the bare repo pushed made for p.
func origin(p string) string { return filepath.Join(filepath.Dir(p), "origin.git") }

// opened opens branch of p in a fresh worktrees folder and adds it.
func opened(t *testing.T, p, branch string) (*Open, Created) {
	t.Helper()
	o, err := PrepareBranch(registered(p, registry.Repo{Worktrees: filepath.Join(t.TempDir(), "wt")}), p, "", branch, os.Environ())
	if err != nil {
		t.Fatal(err)
	}
	c, err := o.Add()
	if err != nil {
		t.Fatal(err)
	}
	return o, c
}

func TestAFolderIsTheBranchWithDashesAndANumberWhenTaken(t *testing.T) {
	if got := Folder("fix/login", nil); got != "fix-login" {
		t.Error(got)
	}
	if got := Folder("fix/login", []string{"fix-login", "Fix-Login-2"}); got != "fix-login-3" {
		t.Error(got)
	}
}

func TestALocalBranchOpensAsItIs(t *testing.T) {
	p := repo(t)
	git(t, p, "branch", "fix/login")
	o, c := opened(t, p, "fix/login")
	if filepath.Base(c.Path) != "fix-login" || c.Branch != "fix/login" || o.Note != "" {
		t.Fatalf("%+v, note %q", c, o.Note)
	}
	if b := git(t, c.Path, "branch", "--show-current"); b != "fix/login" {
		t.Fatalf("on %q", b)
	}
}

func TestABranchOnlyOnOriginTracksIt(t *testing.T) {
	p, _ := pushed(t)
	remote := commitOn(t, origin(p), "refs/heads/feat")
	_, c := opened(t, p, "feat")
	if head := git(t, c.Path, "rev-parse", "HEAD"); head != remote {
		t.Fatalf("HEAD %s, origin %s", head, remote)
	}
	if up := git(t, c.Path, "rev-parse", "--abbrev-ref", "@{upstream}"); up != "origin/feat" {
		t.Fatalf("upstream %q", up)
	}
}

func TestABranchBehindOriginIsFastForwarded(t *testing.T) {
	p, _ := pushed(t)
	git(t, p, "branch", "feat")
	remote := commitOn(t, origin(p), "refs/heads/feat")
	o, c := opened(t, p, "feat")
	if head := git(t, c.Path, "rev-parse", "HEAD"); head != remote || o.Note != "Fast-forwarded feat" {
		t.Fatalf("HEAD %s, origin %s, note %q", head, remote, o.Note)
	}
}

func TestABranchThatDivergedFromOriginKeepsItsCommits(t *testing.T) {
	p, _ := pushed(t)
	commitOn(t, origin(p), "refs/heads/feat")
	git(t, p, "branch", "feat")
	local := git(t, p, "commit-tree", "-p", "feat", "-m", "mine", git(t, p, "rev-parse", "main^{tree}"))
	git(t, p, "update-ref", "refs/heads/feat", local)
	o, c := opened(t, p, "feat")
	if head := git(t, c.Path, "rev-parse", "HEAD"); head != local || o.Note != "feat differs from origin/feat" {
		t.Fatalf("HEAD %s, local %s, note %q", head, local, o.Note)
	}
}

func TestABranchAlreadyOpenIsAdoptedMainIncluded(t *testing.T) {
	p := repo(t)
	tree := filepath.Join(filepath.Dir(p), "feat")
	git(t, p, "worktree", "add", "-q", "-b", "feat", tree)
	f := registered(p, registry.Repo{Worktrees: filepath.Join(t.TempDir(), "wt")})
	for branch, want := range map[string]string{"feat": tree, "main": p} {
		o, err := PrepareBranch(f, p, "", branch, os.Environ())
		if err != nil || o.Adopt != want || o.Path != want || o.Note != "Already open in "+filepath.Base(want) {
			t.Errorf("%s: %+v, %v", branch, o, err)
		}
	}
}

func TestADeletedWorktreeFolderNoLongerHoldsItsBranch(t *testing.T) {
	p := repo(t)
	tree := filepath.Join(filepath.Dir(p), "feat")
	git(t, p, "worktree", "add", "-q", "-b", "feat", tree)
	os.RemoveAll(tree)
	if o, c := opened(t, p, "feat"); o.Adopt != "" || filepath.Base(c.Path) != "feat" {
		t.Fatalf("%+v", o)
	}
}

func TestOpeningABranchNobodyHasFails(t *testing.T) {
	p := repo(t)
	f := registered(p, registry.Repo{Worktrees: filepath.Join(t.TempDir(), "wt")})
	for _, b := range []string{"nope", "-x", "a..b"} {
		if _, err := PrepareBranch(f, p, "", b, os.Environ()); err == nil || err.Error() != "No branch "+b || Code(err) != "spawn_failed" {
			t.Errorf("%s: %v", b, err)
		}
	}
}

func TestAFailedAddDeletesOnlyTheBranchItMade(t *testing.T) {
	p, _ := pushed(t)
	commitOn(t, origin(p), "refs/heads/feat")
	git(t, p, "branch", "mine")
	f := registered(p, registry.Repo{Worktrees: filepath.Join(t.TempDir(), "wt")})
	for _, b := range []string{"feat", "mine"} {
		o, err := PrepareBranch(f, p, "", b, os.Environ())
		if err != nil {
			t.Fatal(err)
		}
		os.MkdirAll(filepath.Dir(o.Path), 0o755)
		os.WriteFile(o.Path, nil, 0o644)
		if _, err := o.Add(); err == nil {
			t.Fatalf("%s: added over a file", b)
		}
	}
	if b := git(t, p, "branch", "--list", "feat", "mine"); b != "mine" {
		t.Fatalf("branches %q", b)
	}
}
```

**Step 2: Run, expect failure**
```
cd packages/pocketd && go test ./internal/worktree/ -run 'Branch|Folder|Open|Add'
```
Expected: build failure `undefined: PrepareBranch`, `undefined: Open`, `undefined: Folder`.

**Step 3: Implement** — `open.go`:
```go
package worktree

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"time"

	"pocketd/internal/registry"
)

// Open is an existing branch about to be checked out in a new Worktree, or
// the Worktree already on it.
type Open struct {
	Project, Path, Branch string
	// Adopt is a Worktree already on Branch; nothing is added.
	Adopt string
	Note  string
	// created: this Open made Branch at oid, so a failed Add deletes it.
	created bool
	oid     string
	env     []string
}

// Folder is branch with / as -, then -2, -3… until no folder has it.
func Folder(branch string, folders []string) string {
	base := strings.ReplaceAll(branch, "/", "-")
	name := base
	for n := 2; slices.ContainsFunc(folders, func(f string) bool { return strings.EqualFold(f, name) }); n++ {
		name = fmt.Sprintf("%s-%d", base, n)
	}
	return name
}

// PrepareBranch readies branch, local or on origin, for a new Worktree named
// name, or after branch when name is empty.
func PrepareBranch(f registry.File, project, name, branch string, env []string) (*Open, error) {
	o, err := prepare(f, project, name, branch, env)
	if err != nil || o.adopt() {
		return o, err
	}
	local := o.at("refs/heads/" + branch)
	remote := ""
	if _, err := o.git("fetch", "-q", "--no-tags", "origin", "+refs/heads/"+branch+":refs/remotes/origin/"+branch); err == nil {
		remote = o.at("refs/remotes/origin/" + branch)
	}
	switch {
	case local == "" && remote == "":
		return nil, failed("No branch %s", branch)
	case local == "":
		if _, err := o.git("branch", "--track", branch, "refs/remotes/origin/"+branch); err != nil {
			return nil, err
		}
		o.created, o.oid = true, remote
	case remote == "" || local == remote:
	case ancestor(project, local, remote):
		if _, err := o.git("update-ref", "refs/heads/"+branch, remote, local); err != nil {
			return nil, err
		}
		o.note("Fast-forwarded " + branch)
	default:
		o.note(branch + " differs from origin/" + branch)
	}
	return o, nil
}

func prepare(f registry.File, project, name, branch string, env []string) (*Open, error) {
	if !slices.Contains(f.Projects, project) {
		return nil, fmt.Errorf("%w: %s", ErrUnknownProject, project)
	}
	o := &Open{Project: project, Branch: branch, env: env}
	// A Worktree whose folder was deleted still holds its branch until pruned.
	if _, err := o.git("worktree", "prune"); err != nil {
		return nil, err
	}
	if strings.HasPrefix(branch, "-") || exec.Command("git", "check-ref-format", "refs/heads/"+branch).Run() != nil {
		return nil, failed("No branch %s", branch)
	}
	dir := Dir(f, project)
	var folders []string
	entries, _ := os.ReadDir(dir)
	for _, e := range entries {
		folders = append(folders, e.Name())
	}
	if name == "" {
		name = Folder(branch, folders)
	} else if err := Validate(name, folders); err != nil {
		return nil, err
	}
	o.Path = filepath.Join(dir, name)
	return o, nil
}

// adopt takes the Worktree already on o.Branch, the main one included.
func (o *Open) adopt() bool {
	ws, _ := List(o.Project)
	i := slices.IndexFunc(ws, func(w Worktree) bool { return w.Branch == o.Branch })
	if i < 0 {
		return false
	}
	o.Adopt, o.Path = ws[i].Path, ws[i].Path
	o.note("Already open in " + ws[i].Name)
	return true
}

// Add checks Branch out in a new Worktree at Path, the real path as List
// reports it. On failure a branch this Open made is deleted.
func (o *Open) Add() (Created, error) {
	if _, err := o.git("worktree", "add", "-q", "--", o.Path, o.Branch); err != nil {
		o.rollback()
		return Created{}, err
	}
	path := o.Path
	if real, err := filepath.EvalSymlinks(path); err == nil {
		path = real
	}
	return Created{Path: path, Branch: o.Branch}, nil
}

// rollback deletes Branch if this Open made it and nothing moved it since.
func (o *Open) rollback() {
	if o.created && o.at("refs/heads/"+o.Branch) == o.oid {
		o.git("branch", "-D", o.Branch)
	}
}

func (o *Open) note(s string) {
	if o.Note != "" {
		o.Note += "; "
	}
	o.Note += s
}

// at is ref's commit, or "" when there is no ref.
func (o *Open) at(ref string) string {
	sha, _ := o.git("rev-parse", "-q", "--verify", ref+"^{commit}")
	return sha
}

// git runs in o.Project with the login env, never prompts, and gives up
// after 15s, as Plan.Fetch does.
func (o *Open) git(args ...string) (string, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, "git", append([]string{"-C", o.Project}, args...)...)
	cmd.Env = append(slices.Clip(o.env), "GIT_TERMINAL_PROMPT=0")
	out, err := cmd.Output()
	if exit, ok := errors.AsType[*exec.ExitError](err); ok {
		return "", errors.New(strings.TrimSpace(string(exit.Stderr)))
	}
	return strings.TrimSpace(string(out)), err
}

// failed is a failure worded for the person opening, sent as spawn_failed.
func failed(format string, a ...any) error {
	return coded("spawn_failed", fmt.Sprintf(format, a...))
}
```

**Step 4: Run, expect pass**
```
cd packages/pocketd && go test ./internal/worktree/ -run 'Branch|Folder|Open|Add'
```
Expected: `ok pocketd/internal/worktree`, with 9 new tests passing.

### Task 1.3: launch opens the branch

**Files:**
- `packages/pocketd/internal/launch/launch.go:170-213` (`checkout`)
- `packages/pocketd/internal/launch/launch_test.go` (append after the `newWorktree` tests, ~line 208)

**Context:**
- Steps, in order: `verify` (trust), then `fetch` before `PrepareBranch`, since it fetches the branch, then `worktree` (plus a note when there is one), then `copy`, then `setup`. An adopted Worktree skips copy and setup: it already exists and has its files.
- The tail of the `new` path, copy then setup, moves into `filled` so the branch path can share it. The existing `TestANewWorktreeReportsEachStepAsItStarts` pins that this changes nothing.
- `Name` empty → `PrepareBranch` names the folder after the branch. Argv gets the empty name, so `claude -n` falls back to the prompt's slug, as it does when the phone sends no name.
- `opening` calls `l.Exited(…"setup"…)` only when `c.Setup` is set. Calling it unconditionally ends an adopted create early with an extra `agent` step.

**Step 1: Failing test** — append to `launch_test.go`:
```go
// opening runs a create for branch in a newWorktree project and gives its
// steps and the folder its terminal started in.
func opening(t *testing.T, branch string) (steps []string, cwd string) {
	defer func(w time.Duration) { createWait = w }(createWait)
	createWait = 500 * time.Millisecond
	t.Setenv("SHELL", "/bin/sh")
	l, s := newWorktree(t, "true")
	git(t, s.Project, "branch", "feat")
	s.Checkout.New = &proto.NewWorktree{Branch: branch}
	r := l.Create(Who{Owner: true, Key: "owner"}, "r1", s, func(step, note string) {
		steps = append(steps, strings.TrimSpace(step+" "+note))
	}, func(c Creating) {
		if cwd = c.Cwd; c.Setup {
			go l.Exited(c.Terminal, "setup", 0)
		}
	})
	if term := l.d.Terminals.Get(r.TerminalID); term != nil {
		t.Cleanup(term.Close)
	}
	return steps, cwd
}

func TestABranchOpensInItsOwnWorktreeWithCopyAndSetup(t *testing.T) {
	steps, cwd := opening(t, "feat")
	want := []string{"prepare", "verify", "fetch", "worktree", "copy", "setup", "agent"}
	if !slices.Equal(steps, want) || filepath.Base(cwd) != "feat" || filepath.Base(filepath.Dir(cwd)) != "wt" {
		t.Fatalf("steps %q in %s", steps, cwd)
	}
}

func TestABranchAlreadyCheckedOutIsUsedWithoutCopyOrSetup(t *testing.T) {
	steps, cwd := opening(t, "main")
	want := []string{"prepare", "verify", "fetch", "worktree", "worktree Already open in repo", "agent"}
	if !slices.Equal(steps, want) || filepath.Base(cwd) != "repo" {
		t.Fatalf("steps %q in %s", steps, cwd)
	}
}
```

**Step 2: Run, expect failure**
```
cd packages/pocketd && go test ./internal/launch/ -run 'ABranch'
```
Expected: both tests fail with steps `["prepare" "verify"]`. `Prepare` gets `Name ""` and fails with `invalid_name` before `fetch`.

**Step 3: Implement** — in `checkout`, right after `n := s.Checkout.New`:
```go
	if n.Branch != "" {
		progress(proto.StepFetch, "")
		o, err := worktree.PrepareBranch(f, s.Project, n.Name, n.Branch, env)
		return opened(f, s, o, err, progress)
	}
```
Replace everything after `c, err := plan.Add()` and its error check with `return filled(f, s, c.Path, progress)`, then add:
```go
// opened adds o's Worktree and fills it, or takes the one already on its
// branch as it is.
func opened(f registry.File, s proto.LaunchSpec, o *worktree.Open, err error, progress func(step, note string)) (string, string, *Failure) {
	if err != nil {
		return "", "", gitFailure(err)
	}
	progress(proto.StepWorktree, "")
	if o.Note != "" {
		progress(proto.StepWorktree, o.Note)
	}
	if o.Adopt != "" {
		return o.Adopt, "", nil
	}
	c, err := o.Add()
	if err != nil {
		return "", "", gitFailure(err)
	}
	return filled(f, s, c.Path, progress)
}

// filled copies into a new Worktree at path and picks its setup, unless the
// owner turned them off.
func filled(f registry.File, s proto.LaunchSpec, path string, progress func(step, note string)) (string, string, *Failure) {
	n := s.Checkout.New
	if on(n.Copy) {
		progress(proto.StepCopy, "")
		worktree.CopyInto(f, s.Project, path)
	}
	setup := ""
	if on(n.Setup) {
		setup = f.Repos[s.Project].Setup
	}
	return path, setup, nil
}
```

**Step 4: Run, expect pass**
```
cd packages/pocketd && go test ./internal/launch/
```
Expected: `ok`. The existing `TestANewWorktreeReportsEachStepAsItStarts` still passes, so `filled` changed nothing.

### Task 1.4: Desktop learns origin's branches and the cap

**Files:**
- `packages/desktop/crates/git/src/git.rs:152-155` (after `branches`), `mod tests` end (~line 934)
- `packages/desktop/crates/agents/src/agents.rs:250` (after `pairing`), `:322` (consts), `:399` (`CAPS`), `:605` (hello test), `:632` (new test before `hello_ok_host_and_host_changed_become_host_events`)

**Context:**
- The desktop sends `CAPS` in its hello. Without `open.v1` there, the intersection never has it, so the hello test's expected caps change too.
- `remote_branches` reads only refs already fetched (`refs/remotes/origin`). There is no network on the UI path; pocketd fetches the one branch on open.
- `HashSet` is already imported in git.rs.

**Step 1: Failing tests**

git.rs `mod tests`, append:
```rust
    #[test]
    fn remote_branches_leave_out_head_and_branches_already_local() {
        let dir = scratch_repo("remote-branches");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        sh(&repo, &["branch", "-M", "main"]);
        for b in ["main", "feat/x"] {
            sh(&repo, &["update-ref", &format!("refs/remotes/origin/{b}"), "HEAD"]);
        }
        sh(&repo, &["symbolic-ref", "refs/remotes/origin/HEAD", "refs/remotes/origin/main"]);
        assert_eq!(remote_branches(r), ["feat/x"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
```
agents.rs: in `sends_queued_messages_while_pocketd_is_quiet` (line 605), the expected hello becomes `"caps": ["pair.v1", "scopes.v1", "summary.v2", "host.v1", "open.v1"]`. Add:
```rust
    #[test]
    fn opening_branches_waits_for_pocketd_to_offer_it() {
        let mut a = Agents::default();
        assert!(!a.opens());
        a.apply(Event::Connected { scopes: vec![], caps: vec!["open.v1".into()] });
        assert!(a.opens());
    }
```

**Step 2: Run, expect failure**
```
cd packages/desktop && cargo test -p git remote_branches && cargo test -p agents
```
Expected: `cannot find function remote_branches` and `no method named opens`.

**Step 3: Implement**

git.rs, after `branches`:
```rust
/// Branches on origin that have no local twin, most recently committed first.
pub fn remote_branches(cwd: &str) -> Vec<String> {
    let local: HashSet<String> = branches(cwd).into_iter().collect();
    let remote = lines(git(cwd, &["for-each-ref", "--sort=-committerdate", "--format=%(refname:lstrip=3)", "refs/remotes/origin"]));
    remote.into_iter().filter(|b| b != "HEAD" && !local.contains(b)).collect()
}
```
agents.rs:
```rust
const OPEN_CAP: &str = "open.v1";                       // after PAIR_CAP
const CAPS: [&str; 5] = [PAIR_CAP, "scopes.v1", "summary.v2", "host.v1", OPEN_CAP];
```
After `pairing()`:
```rust
    /// pocketd can open an existing branch as a worktree.
    pub fn opens(&self) -> bool {
        self.caps.iter().any(|c| c == OPEN_CAP)
    }
```

**Step 4: Run, expect pass**
```
cd packages/desktop && cargo test -p git remote_branches && cargo test -p agents
```
Expected: 1 passed for git; 23 passed for agents.

### Task 1.5: The sheet's "Start from" chip

**Files:**
- `packages/desktop/crates/pocket/src/modals/new_session.rs`, in these places:
  - before `struct Draft` (line 58): the `Source` enum
  - `:59-77` Draft fields and `:79-99` Default
  - `:106-118` placeholder, ready and folder
  - `:128-135` spec
  - `:239` prompt placeholder
  - `:259-284` reset
  - `:286-325` pick_repo
  - `:327-330` new_name and pick_source
  - `:337-361` start_session
  - `:412-417` reopen
  - `:424-513` view
  - `:518` and `:592` tests
- `packages/desktop/crates/pocket/src/modals/new_session/picker.rs:1-10` (import, `Picker::Source`), `:142` (new pickers), `:159-170` (`branch_select`)
- `packages/desktop/crates/pocket/src/creating.rs:104` (`asked`)

**Context:**
- **Decisions are on `Draft`,** a plain value, so they can be tested: `ready`, `spec`, `folder`, `placeholder` and `Source::of`. `impl Desktop` only gathers input and notifies (CLAUDE.md).
- **The chip appears only when `agents.opens()`.** On an older pocketd, the sheet is unchanged.
- **In Branch mode the name field holds the branch.** The branch chip's slot becomes a list of local branches (the top 20 that `pick_repo` already loads), then origin's, filtered by what has been typed. A click fills the field. The design says "the base chip hides", but leaving the slot empty would leave no way to pick, so the slot is reused.
- **Provisional path** is `<worktrees>/<branch with / as ->`. `Creates::started` swaps in pocketd's real cwd, which may be `-2` or an adopted Worktree.
- **Reopen after a failure** must restore Branch mode. Otherwise Edit offers the folder name as a *new* branch. `Create::asked` exposes `checkout.new`, and `Source::of` reads it.
- `name_problem` (taken/invalid) applies only to New. An existing branch is supposed to exist.
- **No render tests.** Check the view with `.ui-review/fixture/capture.sh` before and after.

**Step 1: Failing tests** — `new_session.rs` tests. The import becomes `use super::{Draft, Source, auto_name, default_first, name_problem, slug};`. Add:
```rust
    #[test]
    fn an_existing_branch_needs_only_a_repository_and_a_branch() {
        let draft = Draft { worktree: true, source: Source::Branch, repo: Some("/src/app".into()), taken: ["main".to_string()].into(), ..Draft::default() };
        assert!(draft.ready("main", false));
        assert!(!draft.ready("", true));
        assert!(!Draft { repo: None, ..draft }.ready("main", true));
    }

    #[test]
    fn an_existing_branch_is_sent_without_a_name_or_base_and_lands_in_a_dashed_folder() {
        let draft = Draft { worktree: true, source: Source::Branch, branches: vec![("main".into(), None)], run_setup: true, ..Draft::default() };
        let want = json!({"project": "/p", "checkout": {"new": {"branch": "fix/login", "copy": false, "setup": true}}, "provider": "claude", "access": "ask", "plan": false});
        assert_eq!(draft.spec("/p", "/p", "fix/login", ""), want);
        assert_eq!(draft.folder("fix/login"), "fix-login");
    }

    #[test]
    fn a_failed_open_reopens_on_the_branch_it_asked_for() {
        assert_eq!(Source::of(&json!({"branch": "fix/login", "copy": true}), "fix-login".into()), (Source::Branch, "fix/login".to_string()));
        assert_eq!(Source::of(&json!({"name": "calm-otter", "base": "main"}), "calm-otter".into()), (Source::New, "calm-otter".to_string()));
    }
```

**Step 2: Run, expect failure**
```
cd packages/desktop && cargo test -p pocket new_session
```
Expected: `unresolved import super::Source`, `struct Draft has no field named source`.

**Step 3: Implement**, as unified diffs against the current files (validated: build, `cargo test -p pocket`, no new clippy warnings).

`new_session.rs` (test hunks omitted; they are Step 1):
```diff
@@ -55,6 +55,35 @@
     draft: Draft,
 }
 
+/// Where a new worktree's branch comes from.
+#[derive(Clone, Copy, PartialEq, Debug, Default)]
+pub(crate) enum Source {
+    /// A new branch named after the worktree, from the base.
+    #[default]
+    New,
+    /// An existing branch, local or on origin.
+    Branch,
+}
+
+impl Source {
+    const ALL: [Source; 2] = [Source::New, Source::Branch];
+
+    fn label(self) -> &'static str {
+        match self {
+            Source::New => "New branch",
+            Source::Branch => "Existing branch",
+        }
+    }
+
+    /// What a create's `checkout.new` asked for, and what its name field held.
+    fn of(new: &Value, folder: String) -> (Self, String) {
+        match new["branch"].as_str() {
+            Some(b) => (Source::Branch, b.to_string()),
+            None => (Source::New, folder),
+        }
+    }
+}
+
 /// The form's choices besides its text inputs.
 struct Draft {
     /// Branches and worktree folders a new worktree's name must not reuse.
@@ -63,6 +92,9 @@
     worktree: bool,
     repo: Option<String>,
     branches: Vec<(String, Option<i64>)>,
+    /// Branches on origin with no local twin.
+    remote_branches: Vec<String>,
+    source: Source,
     base: usize,
     /// The base to pick once the branches load, instead of the repo's.
     want_base: Option<String>,
@@ -84,6 +116,8 @@
             worktree: false,
             repo: None,
             branches: Vec::new(),
+            remote_branches: Vec::new(),
+            source: Source::New,
             base: 0,
             want_base: None,
             copy_env: false,
@@ -107,16 +141,32 @@
         auto_name(prompt, self.seed, &self.taken)
     }
 
+    /// What the name field shows while empty.
+    fn placeholder(&self, prompt: &str) -> String {
+        match self.source {
+            Source::New => self.auto_name(prompt),
+            Source::Branch => "Branch to open".into(),
+        }
+    }
+
     /// `in_tree`: a worktree is open to start the session in.
     fn ready(&self, name: &str, in_tree: bool) -> bool {
-        let place = if self.worktree {
-            self.repo.is_some() && !self.branches.is_empty() && name_problem(name, &self.taken).is_none()
-        } else {
-            in_tree
+        let place = match (self.worktree, self.source) {
+            (false, _) => in_tree,
+            (true, Source::New) => self.repo.is_some() && !self.branches.is_empty() && name_problem(name, &self.taken).is_none(),
+            (true, Source::Branch) => self.repo.is_some() && !name.is_empty(),
         };
         place && self.pending.is_none()
     }
 
+    /// The folder pocketd will likely make for `name`; `Creates::started` swaps in the one it made.
+    fn folder(&self, name: &str) -> String {
+        match self.source {
+            Source::New => name.to_string(),
+            Source::Branch => name.replace('/', "-"),
+        }
+    }
+
     /// A model or effort remembered for one agent means nothing to another.
     fn pick_provider(&mut self, provider: &'static str) {
         if provider != self.provider {
@@ -126,10 +176,10 @@
     }
 
     fn spec(&self, project: &str, tree: &str, name: &str, prompt: &str) -> Value {
-        let checkout = if self.worktree {
-            json!({"new": {"name": name, "base": self.base_branch(), "copy": self.copy_env, "setup": self.run_setup}})
-        } else {
-            json!({"worktree": tree})
+        let checkout = match (self.worktree, self.source) {
+            (false, _) => json!({"worktree": tree}),
+            (true, Source::New) => json!({"new": {"name": name, "base": self.base_branch(), "copy": self.copy_env, "setup": self.run_setup}}),
+            (true, Source::Branch) => json!({"new": {"branch": name, "copy": self.copy_env, "setup": self.run_setup}}),
         };
         let mut spec = json!({"project": project, "checkout": checkout, "provider": self.provider, "access": Access::Ask.wire(), "plan": false});
         for (key, value) in [("model", &self.model), ("effort", &self.effort)] {
@@ -235,7 +285,7 @@
             cx.subscribe_in(&prompt, window, |this, prompt, ev: &InputEvent, window, cx| match ev {
                 InputEvent::PressEnter { secondary: true, .. } => this.start_session(window, cx),
                 InputEvent::Change => {
-                    let name = this.new_form.draft.auto_name(&prompt.read(cx).value());
+                    let name = this.new_form.draft.placeholder(&prompt.read(cx).value());
                     this.new_form.name.update(cx, |b, cx| b.set_placeholder(name, window, cx));
                     cx.notify();
                 }
@@ -262,6 +312,7 @@
         let f = &mut self.new_form;
         f.draft.seed = crate::util::now_ms() as usize;
         f.draft.taken.clear();
+        f.draft.source = Source::New;
         let placeholder = f.draft.auto_name(&text);
         f.prompt.update(cx, |s, cx| {
             s.set_value(text, window, cx);
@@ -289,6 +340,7 @@
         let f = &mut self.new_form.draft;
         f.repo = Some(repo.clone());
         f.branches.clear();
+        f.remote_branches.clear();
         f.base = 0;
         f.copy_env = !cfg.copy.is_empty();
         f.run_setup = !cfg.setup.is_empty();
@@ -302,10 +354,10 @@
             }).collect();
             let entries = std::fs::read_dir(&folders).into_iter().flatten().flatten();
             let taken: HashSet<String> = all.into_iter().chain(entries.filter_map(|e| e.file_name().into_string().ok())).collect();
-            (current, branches, taken)
+            (current, branches, taken, git::remote_branches(&dir))
         });
         cx.spawn_in(window, async move |this, cx| {
-            let (current, mut branches, taken) = task.await;
+            let (current, mut branches, taken, remote) = task.await;
             this.update_in(cx, |d, window, cx| {
                 let f = &mut d.new_form;
                 if f.draft.repo.as_ref() != Some(&repo) {
@@ -314,8 +366,9 @@
                 default_first(&mut branches, f.draft.want_base.as_deref().unwrap_or(&cfg.base), &current);
                 f.draft.base = 0;
                 f.draft.branches = branches;
+                f.draft.remote_branches = remote;
                 f.draft.taken = taken;
-                let name = f.draft.auto_name(&f.prompt.read(cx).value());
+                let name = f.draft.placeholder(&f.prompt.read(cx).value());
                 f.name.update(cx, |s, cx| s.set_placeholder(name, window, cx));
                 cx.notify();
             })
@@ -324,11 +377,26 @@
         .detach();
     }
 
+    /// The new worktree's name, or the branch to open.
     fn new_name(&self, cx: &App) -> String {
         let f = &self.new_form;
-        typed_or(&f.name, || f.draft.auto_name(&f.prompt.read(cx).value()), cx)
+        match f.draft.source {
+            Source::New => typed_or(&f.name, || f.draft.auto_name(&f.prompt.read(cx).value()), cx),
+            _ => f.name.read(cx).value().trim().to_string(),
+        }
     }
 
+    fn pick_source(&mut self, source: Source, window: &mut Window, cx: &mut Context<Self>) {
+        let f = &mut self.new_form;
+        if f.draft.source != source {
+            f.name.update(cx, |s, cx| s.set_value("", window, cx));
+        }
+        (f.draft.source, f.draft.picker) = (source, None);
+        let placeholder = f.draft.placeholder(&f.prompt.read(cx).value());
+        f.name.update(cx, |s, cx| s.set_placeholder(placeholder, window, cx));
+        cx.notify();
+    }
+
     fn session_ready(&self, cx: &App) -> bool {
         self.new_form.draft.ready(&self.new_name(cx), self.cwd().is_some())
     }
@@ -343,7 +411,7 @@
         let tree = self.cwd().unwrap_or_default();
         let f = &self.new_form.draft;
         let Some(project) = f.repo.clone() else { return };
-        let (spec, worktree) = (f.spec(&project, &tree, &name, &prompt), f.worktree);
+        let (spec, worktree, folder) = (f.spec(&project, &tree, &name, &prompt), f.worktree, f.folder(&name));
         self.store.repos.entry(project.clone()).or_default().launch = f.pick();
         if worktree {
             self.store.collapsed.remove(&project);
@@ -354,7 +422,7 @@
             (self.new_form.draft.pending, self.new_form.draft.error) = (Some(request), None);
             return cx.notify();
         }
-        let path = format!("{}/{name}", self.worktrees_dir(&project));
+        let path = format!("{}/{folder}", self.worktrees_dir(&project));
         self.creates.list.push(Create::new(request, path.clone(), spec, Instant::now()));
         self.close_overlay(window, cx);
         self.select_tree(project, Some(path), cx);
@@ -412,8 +480,10 @@
     pub(crate) fn reopen_new_worktree(&mut self, c: &Create, window: &mut Window, cx: &mut Context<Self>) {
         self.new_worktree(&crate::actions::NewWorktree, window, cx);
         self.new_form.draft.want_base = Some(c.base().to_string());
+        let (source, name) = Source::of(c.asked(), c.name());
+        self.pick_source(source, window, cx);
         self.new_form.prompt.update(cx, |s, cx| s.set_value(c.prompt().to_string(), window, cx));
-        self.new_form.name.update(cx, |s, cx| s.set_value(c.name(), window, cx));
+        self.new_form.name.update(cx, |s, cx| s.set_value(name, window, cx));
     }
 
     pub fn new_worktree_in(&mut self, p: String, window: &mut Window, cx: &mut Context<Self>) {
@@ -436,16 +506,17 @@
             .child(div().flex().items_center().gap(px(6.)).text_size(px(13.)).text_color(TEXT_3).child(ui::repo_tile(&crate::util::initials(&name), 18., false, None)).child(name))
             .child(div().ml_auto().child(close));
         let agent = self.agent_select(cx);
+        let source = (f.worktree && self.agents.opens()).then(|| self.source_select(cx));
         let branch = f.worktree.then(|| self.branch_select(cx));
         let ready = self.session_ready(cx);
         let send = ui::primary(div().id("form-start").ml_auto().size(px(32.)).flex().flex_none().items_center().justify_center().rounded(px(16.)).cursor_pointer())
             .child(icon("arrow-up", 16., ON_TEXT))
             .when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.start_session(window, cx))))
             .when(!ready, |d| d.opacity(0.5).cursor_default());
-        let problem = if f.worktree { name_problem(&self.new_name(cx), &f.taken) } else { None };
+        let problem = if f.worktree && f.source == Source::New { name_problem(&self.new_name(cx), &f.taken) } else { None };
         let name_field = f.worktree.then(|| {
             ui::field_box()
-                .child(icon("worktree", 14., TEXT_3))
+                .child(icon(if f.source == Source::New { "worktree" } else { "branch" }, 14., TEXT_3))
                 .child(div().flex_1().min_w_0().font_family(MONO).child(Input::new(&self.new_form.name).appearance(false).p_0().text_size(px(13.))))
                 .children(problem.map(|p| div().flex_none().text_size(px(12.)).text_color(FAILED).child(p)))
         });
@@ -467,6 +538,7 @@
                     .pb(px(10.))
                     .text_size(px(13.))
                     .child(agent)
+                    .children(source)
                     .children(branch)
                     .child(send),
             );
@@ -485,16 +557,18 @@
                 .when(!detail.is_empty(), |d| d.child(div().font_family(MONO).text_size(px(12.)).text_color(TEXT_2).child(detail)))
         });
         let mono = |s: String| div().font_family(MONO).text_color(TEXT_2).child(s);
-        let summary: Vec<AnyElement> = if f.worktree {
-            vec![
+        let summary: Vec<AnyElement> = match (f.worktree, f.source) {
+            (true, Source::New) => vec![
                 div().child("New branch").into_any_element(),
                 mono(self.new_name(cx)).into_any_element(),
                 div().child("from").into_any_element(),
                 mono(f.base_branch()).into_any_element(),
-            ]
-        } else {
-            let place = self.repo().map(|r| r.branch.clone()).or_else(|| self.cwd().map(|c| tilde(&c))).unwrap_or_default();
-            vec![div().child("In").into_any_element(), mono(place).into_any_element()]
+            ],
+            (true, Source::Branch) => vec![div().child("Open branch").into_any_element(), mono(self.new_name(cx)).into_any_element()],
+            (false, _) => {
+                let place = self.repo().map(|r| r.branch.clone()).or_else(|| self.cwd().map(|c| tilde(&c))).unwrap_or_default();
+                vec![div().child("In").into_any_element(), mono(place).into_any_element()]
+            }
         };
         let footer = div()
             .flex()
```

`creating.rs`:
```diff
@@ -104,6 +104,11 @@
         git::Worktree { path: self.path.clone(), branch: self.name(), main: false }
     }
 
+    /// The spec's `checkout.new`: what the sheet asked for.
+    pub(crate) fn asked(&self) -> &Value {
+        &self.spec["checkout"]["new"]
+    }
+
     pub(crate) fn base(&self) -> &str {
         self.spec["checkout"]["new"]["base"].as_str().unwrap_or_default()
     }
```

`picker.rs`:
```diff
@@ -1,3 +1,4 @@
+use super::Source;
 use crate::desktop::Desktop;
 use gpui_kit::prelude::FluentBuilder as _;
 use gpui_kit::*;
@@ -6,6 +7,7 @@
 #[derive(Clone, Copy, PartialEq)]
 pub enum Picker {
     Agent,
+    Source,
     Branch,
 }
 
@@ -142,6 +144,60 @@
         picker_menu("branch-menu", 300., rows, cx)
     }
 
+    fn source_picker(&self, cx: &mut Context<Self>) -> Stateful<Div> {
+        let f = &self.new_form.draft;
+        let mut rows = vec![pick_head("Start from").into_any_element()];
+        for source in Source::ALL {
+            rows.push(
+                pick_row(source.label(), f.source == source, None::<Div>, div().child(source.label()), None)
+                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.pick_source(source, window, cx)))
+                    .into_any_element(),
+            );
+        }
+        picker_menu("source-menu", 220., rows, cx)
+    }
+
+    /// Local branches, then origin's, that hold what the name field holds. Picking one fills the field.
+    fn open_picker(&self, cx: &mut Context<Self>) -> Stateful<Div> {
+        let f = &self.new_form.draft;
+        let typed = self.new_form.name.read(cx).value().trim().to_lowercase();
+        let local = f.branches.iter().map(|(b, _)| (b, false));
+        let remote = f.remote_branches.iter().map(|b| (b, true));
+        let mut rows = Vec::new();
+        let mut head = None;
+        for (i, (b, on_origin)) in local.chain(remote).filter(|(b, _)| b.to_lowercase().contains(&typed)).take(50).enumerate() {
+            if head != Some(on_origin) {
+                rows.push(pick_head(if on_origin { "On origin" } else { "Local" }).into_any_element());
+                head = Some(on_origin);
+            }
+            let label = if on_origin { format!("origin/{b}") } else { b.clone() };
+            let name = b.clone();
+            rows.push(
+                pick_row(("open", i), b.to_lowercase() == typed, Some(icon("branch", 13., TEXT_3)), div().font_family(MONO).text_size(px(12.5)).child(label), None)
+                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
+                        this.new_form.name.update(cx, |s, cx| s.set_value(name.clone(), window, cx));
+                        this.new_form.draft.picker = None;
+                        cx.notify();
+                    }))
+                    .into_any_element(),
+            );
+        }
+        picker_menu("open-menu", 300., rows, cx)
+    }
+
+    pub(super) fn source_select(&self, cx: &mut Context<Self>) -> Div {
+        let f = &self.new_form.draft;
+        let source = chip("form-source", f.picker == Some(Picker::Source))
+            .child(div().font_weight(FontWeight::MEDIUM).child(f.source.label()))
+            .child(icon("chevron-down", 12., TEXT_4))
+            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
+                cx.stop_propagation();
+                this.toggle_picker(Picker::Source, cx);
+            }));
+        let source_menu = (f.picker == Some(Picker::Source)).then(|| ui::dropdown(36., ui::menu_in("source-menu-in", self.source_picker(cx))));
+        div().relative().child(source).children(source_menu)
+    }
+
     pub(super) fn agent_select(&self, cx: &mut Context<Self>) -> Div {
         let f = &self.new_form.draft;
         let agent = chip("form-agent", f.picker == Some(Picker::Agent))
@@ -156,17 +212,20 @@
         div().relative().child(agent).children(agent_menu)
     }
 
+    /// The base to branch from, or for an existing branch, the list to pick it from.
     pub(super) fn branch_select(&self, cx: &mut Context<Self>) -> Div {
         let f = &self.new_form.draft;
+        let label = if f.source == Source::New { f.base_branch() } else { "Branches".into() };
         let branch = chip("form-branch", f.picker == Some(Picker::Branch))
             .child(icon("branch", 14., TEXT_3))
-            .child(div().font_family(MONO).text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child(f.base_branch()))
+            .child(div().font_family(MONO).text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child(label))
             .child(icon("chevron-down", 12., TEXT_4))
             .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                 cx.stop_propagation();
                 this.toggle_picker(Picker::Branch, cx);
             }));
-        let branch_menu = (f.picker == Some(Picker::Branch)).then(|| ui::dropdown(36., ui::menu_in("branch-menu-in", self.branch_picker(cx))));
+        let menu = |cx: &mut Context<Self>| if f.source == Source::New { self.branch_picker(cx) } else { self.open_picker(cx) };
+        let branch_menu = (f.picker == Some(Picker::Branch)).then(|| ui::dropdown(36., ui::menu_in("branch-menu-in", menu(cx))));
         div().relative().child(branch).children(branch_menu)
     }
 }
```

**Step 4: Run, expect pass**
```
cd packages/desktop && cargo test -p pocket new_session
```
Expected: 17 passed. Then capture the sheet in both modes with `.ui-review/fixture/capture.sh` and compare. The chip row reads Agent · Start from · Branches · send.

### Task 1.6: Full checks

**Step 1–2:** none new.
**Step 3:** none.
**Step 4:**
```
cd packages/desktop && cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace
cd packages/pocketd && go vet ./... && go test ./...
cd packages/protocol && pnpm test
```
Expected: everything passes, and clippy shows no warnings in `git.rs:150-170`, `agents.rs`, `new_session*.rs` or `creating.rs`. The existing warnings at `git.rs:104` and `:766-768` are not ours. A fresh worktree needs `pnpm install` and `third_party/ghostty` (launch tests link it) first.

## PR 2: Open a GitHub PR as a worktree

**Scope:** `checkout.new.pr` ("123", "#123" or a PR URL) on the wire, under the same cap `open.v1`. pocketd asks `gh pr view`. It fetches the PR's head itself: from the head branch while that branch lives and the PR is from this repo, otherwise from `refs/pull/N/head`. It checks the commit is the one gh reported, and checks it out on the head branch, or on `pr/N` for a fork. The desktop "Start from" chip gains "Pull request".
**Depends on:** PR 1 (`Open`, `prepare`, `adopt`, `Add`, `rollback`, `opened`, `filled`, the cap and the Source chip).
**Done when:** a same-repo PR opens on its head branch tracking `origin/<head>`; a fork PR opens on `pr/N` pulling from `refs/pull/N/head`; no gh, no login, an unknown PR and a moved PR each fail with one readable line; all three suites pass.

Facts the tasks rely on:
- **gh only resolves.** The `gh` CLI is used just to turn the ref into its number, URL, head, head commit, fork flag and state. It never checks anything out, so pocketd's own rules hold (prompt-free fetch, timeouts, rollback), and tests need no gh: `View` is a thin shell over `parseView` and `ghFailure`, which are pure.
- **Where gh is found.** The `worktree` package must not import `terminal`, which pulls in cgo vt/ghostty. So launch finds gh on the login PATH with `terminal.LookPath` and passes the path in.
- **What gh prints.** The stderr strings in the tests were captured from real gh 2.x runs. `--` before the ref is accepted by `gh pr view`.

### Task 2.1: Wire field and golden

**Files:**
- `packages/pocketd/internal/proto/launch.go` (NewWorktree, `valid`, strictObject; as left by Task 1.1)
- `packages/pocketd/internal/proto/golden_test.go` (rejects and accepts lists, next to the Task 1.1 lines)
- new `packages/pocketd/internal/proto/testdata/golden/client/agent_create_pr.json`
- `packages/protocol/src/launch.ts` (NewWorktree)

**Context:** at most one of `base`, `branch` and `pr`. A `name` is optional when opening.

**Step 1: Failing test**

Rejects:
```go
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"new":{"pr":"7","base":"main"}},"provider":"claude","access":"ask","plan":false}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"new":{"pr":"7","branch":"b"}},"provider":"claude","access":"ask","plan":false}}`,
```
Accepts:
```go
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"new":{"pr":"#7"}},"provider":"claude","access":"ask","plan":false}}`,
```
`agent_create_pr.json`:
```json
{"type":"agent.create","id":"c4","requestId":"r4","spec":{"project":"/Users/me/pocket","checkout":{"new":{"pr":"#123","copy":true,"setup":false}},"provider":"codex","access":"ask","plan":false}}
```

**Step 2: Run, expect failure**
```
cd packages/pocketd && go test ./internal/proto/
cd packages/protocol && pnpm test
```
Expected: Go `TestClientGolden` and the accepts test fail on `"pr"` (strictObject rejects the key). TS fails on `agent_create_pr.json` (excess property `pr`).

**Step 3: Implement**

`launch.go`, NewWorktree after `Branch`:
```go
	// PR is "123", "#123" or a PR URL, passed to gh as is.
	PR    string `json:"pr,omitempty"`
```
strictObject: `strictObject(n, "name", "base", "branch", "pr", "copy", "setup")`.
```go
// valid: at most one of Base, Branch and PR, and a Name unless opening a
// Branch or PR.
func (n *NewWorktree) valid() bool {
	set := 0
	for _, s := range []string{n.Base, n.Branch, n.PR} {
		if s != "" {
			set++
		}
	}
	return set <= 1 && (n.Name != "" || n.Branch != "" || n.PR != "")
}
```
`launch.ts`, after `branch`: `pr: Schema.optional(Schema.String),`.

**Step 4: Run, expect pass**
```
cd packages/pocketd && go test ./internal/proto/
cd packages/protocol && pnpm test
```
Expected: `ok`; TS 61 pass.

### Task 2.2: Read what gh says

**Files:**
- new `packages/pocketd/internal/worktree/pr.go`
- new `packages/pocketd/internal/worktree/pr_test.go`

**Context:**
- `View(gh, project, ref, env)` runs `gh pr view --json number,url,headRefName,headRefOid,isCrossRepository,state -- <ref>` in the project, with the login env, `GIT_TERMINAL_PROMPT=0` and a 15s timeout.
- A non-zero exit becomes a readable line through `ghFailure`:
  - not logged in → "Run gh auth login to open pull requests"
  - "Could not resolve" or "no pull requests found" → "No pull request <ref>"
  - anything else → the first stderr line
- `parseView` refuses JSON with no number or head commit. A gh too old to know a field, or a non-PR answer, must not get as far as a fetch.
- Every message is `coded("spawn_failed", …)` through `failed` (open.go), so `gitFailure` passes it through unchanged.

**Step 1: Failing test** — `pr_test.go`:
```go
package worktree

import "testing"

func TestParseViewReadsWhatGhPrints(t *testing.T) {
	out := `{"headRefName":"andyfeller/flag-level-disableauth","headRefOid":"cc36d32a212a2b8b6611fb73549fe6d04fb6ec38","isCrossRepository":false,"number":9000,"state":"MERGED","url":"https://github.com/cli/cli/pull/9000"}`
	pr, err := parseView([]byte(out))
	want := PR{Number: 9000, URL: "https://github.com/cli/cli/pull/9000", Head: "andyfeller/flag-level-disableauth", OID: "cc36d32a212a2b8b6611fb73549fe6d04fb6ec38", State: "MERGED"}
	if err != nil || pr != want {
		t.Fatalf("%+v, %v", pr, err)
	}
	if _, err := parseView([]byte(`{}`)); Code(err) != "spawn_failed" {
		t.Fatal(err)
	}
}

func TestGhFailuresSayWhatToDo(t *testing.T) {
	for stderr, want := range map[string]string{
		"To get started with GitHub CLI, please run:  gh auth login\nAlternatively, populate the GH_TOKEN environment variable with a GitHub API authentication token.\n": "Run gh auth login to open pull requests",
		"GraphQL: Could not resolve to a PullRequest with the number of 99999. (repository.pullRequest)\n":                                                                "No pull request #99999",
		`no pull requests found for branch "#99999"` + "\n":                                                                                                               "No pull request #99999",
		"HTTP 502: Bad Gateway\nTry again\n": "HTTP 502: Bad Gateway",
		"":                                   "gh couldn't open pull request #99999",
	} {
		if err := ghFailure("#99999", stderr); err.Error() != want || Code(err) != "spawn_failed" {
			t.Errorf("%q: %v", stderr, err)
		}
	}
}
```

**Step 2: Run, expect failure**
```
cd packages/pocketd && go test ./internal/worktree/ -run 'ParseView|GhFailures'
```
Expected: `undefined: parseView`, `undefined: PR`, `undefined: ghFailure`.

**Step 3: Implement** — `pr.go`:
```go
package worktree

import (
	"context"
	"encoding/json"
	"errors"
	"os/exec"
	"slices"
	"strings"
	"time"
)

// ErrNoGh is the failure when the login PATH has no gh.
var ErrNoGh = coded("spawn_failed", "Install GitHub CLI (gh) to open pull requests")

type PR struct {
	Number         int
	URL, Head, OID string
	Fork           bool
	State          string
}

// View asks gh, at path gh, about the pull request ref ("123", "#123" or a
// URL) of project.
func View(gh, project, ref string, env []string) (PR, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, gh, "pr", "view", "--json", "number,url,headRefName,headRefOid,isCrossRepository,state", "--", ref)
	cmd.Dir = project
	cmd.Env = append(slices.Clip(env), "GIT_TERMINAL_PROMPT=0")
	out, err := cmd.Output()
	if ctx.Err() != nil {
		return PR{}, failed("GitHub didn't answer in 15s")
	}
	if exit, ok := errors.AsType[*exec.ExitError](err); ok {
		return PR{}, ghFailure(ref, string(exit.Stderr))
	}
	if err != nil {
		return PR{}, err
	}
	return parseView(out)
}

func parseView(out []byte) (PR, error) {
	var v struct {
		Number int    `json:"number"`
		URL    string `json:"url"`
		Head   string `json:"headRefName"`
		OID    string `json:"headRefOid"`
		Fork   bool   `json:"isCrossRepository"`
		State  string `json:"state"`
	}
	if err := json.Unmarshal(out, &v); err != nil || v.Number == 0 || v.OID == "" {
		return PR{}, failed("gh answered with something other than a pull request")
	}
	return PR(v), nil
}

func ghFailure(ref, stderr string) error {
	switch {
	case strings.Contains(stderr, "gh auth login"):
		return failed("Run gh auth login to open pull requests")
	case strings.Contains(stderr, "Could not resolve"), strings.Contains(stderr, "no pull requests found"):
		return failed("No pull request %s", ref)
	}
	line, _, _ := strings.Cut(strings.TrimSpace(stderr), "\n")
	if line == "" {
		return failed("gh couldn't open pull request %s", ref)
	}
	return failed("%s", line)
}
```

**Step 4: Run, expect pass**
```
cd packages/pocketd && go vet ./internal/worktree/ && go test ./internal/worktree/ -run 'ParseView|GhFailures'
```
Expected: `ok`.

### Task 2.3: Fetch, check and land the PR's head

**Files:**
- `packages/pocketd/internal/worktree/pr.go` (append; imports gain `fmt`, `net/url`, `pocketd/internal/registry`)
- `packages/pocketd/internal/worktree/pr_test.go` (imports become `os`, `os/exec`, `path/filepath`, `testing`, `pocketd/internal/registry`, plus helpers and tests)

**Context:**
- **Branch.**
  - Same-repo PR: the head branch, with folder `Folder(head)`.
  - Fork PR: `pr/<n>`, with folder `pr-<n>`. A fork's head name, such as `main`, would clash with ours.
  - `prepare` and `adopt` from PR 1 do the folder, the prune and adopting a branch already open.
- **Remote.**
  - It is the one whose URL ends in the PR URL's `owner/repo`, compared case-insensitively, `.git` and a trailing `/` trimmed, for both https and scp forms. `origin` is tried first.
  - None → "No remote points at owner/repo". This avoids fetching a PR number from the wrong repo.
- **Fetch.**
  - Same repo: `+refs/heads/<head>:refs/remotes/<remote>/<head>` (live).
  - Fork, or a head branch that is gone: `+refs/pull/<n>/head:refs/pocket/pr/<n>`, a private ref (not FETCH_HEAD, which other fetches share), deleted right after it is read.
- **Race check.** The fetched commit must equal gh's `headRefOid`. Otherwise: "PR #n changed while opening, try again".
- **Local branch already there.**
  - At or past the PR's commit: kept.
  - Behind: fast-forwarded with an old-value guard, plus the note "Fast-forwarded B".
  - Diverged: "Branch B has commits not in PR #n". Unlike a plain branch (PR 1), a PR checkout must show the PR.
- **Upstream** is the live head branch, otherwise `branch.B.remote=<remote>` and `branch.B.merge=refs/pull/n/head`, as `gh pr checkout` does. Because the upstream's name differs from B, a plain `git push` refuses rather than pushing a fork's PR somewhere wrong.
- **Adopting.** A Worktree already on the branch is adopted as in PR 1 and left untouched. If its branch is not at the PR's commit, the note says "B isn't at PR #n's head", so the session never silently opens on an older commit.
- **State.** A PR that is not `OPEN` still opens, with the note "PR #n is merged|closed".
- **Rollback** (PR 1) deletes a branch this open made, and only that one, if `Add` fails.

**Step 1: Failing test** — `pr_test.go`: replace `import "testing"` and add the helpers and tests:
```go
import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	"pocketd/internal/registry"
)

// pulled is a repo whose origin is a bare acme/pocket.git, where PR 7 points
// at a new commit on feat.
func pulled(t *testing.T) (p, bare string, pr PR) {
	t.Helper()
	p = repo(t)
	bare = filepath.Join(filepath.Dir(p), "acme", "pocket.git")
	git(t, p, "clone", "-q", "--bare", p, bare)
	git(t, p, "remote", "add", "origin", bare)
	oid := commitOn(t, bare, "refs/heads/feat")
	git(t, bare, "update-ref", "refs/pull/7/head", oid)
	return p, bare, PR{Number: 7, URL: "https://github.com/acme/pocket/pull/7", Head: "feat", OID: oid, State: "OPEN"}
}

func preparedPR(t *testing.T, p string, pr PR) (*Open, error) {
	t.Helper()
	return PreparePR(registered(p, registry.Repo{Worktrees: filepath.Join(t.TempDir(), "wt")}), p, "", pr, os.Environ())
}

func openedPR(t *testing.T, p string, pr PR) (*Open, Created) {
	t.Helper()
	o, err := preparedPR(t, p, pr)
	if err != nil {
		t.Fatal(err)
	}
	c, err := o.Add()
	if err != nil {
		t.Fatal(err)
	}
	return o, c
}

func TestASameRepoPRTracksItsHeadBranch(t *testing.T) {
	p, _, pr := pulled(t)
	o, c := openedPR(t, p, pr)
	if filepath.Base(c.Path) != "feat" || git(t, c.Path, "rev-parse", "HEAD") != pr.OID || o.Note != "" {
		t.Fatalf("%+v, note %q", c, o.Note)
	}
	if up := git(t, c.Path, "rev-parse", "--abbrev-ref", "@{upstream}"); up != "origin/feat" {
		t.Fatalf("upstream %q", up)
	}
}

func TestAForkPRLandsOnPrNTrackingThePullRef(t *testing.T) {
	p, bare, pr := pulled(t)
	git(t, bare, "update-ref", "-d", "refs/heads/feat")
	pr.Fork = true
	_, c := openedPR(t, p, pr)
	if filepath.Base(c.Path) != "pr-7" || git(t, c.Path, "branch", "--show-current") != "pr/7" || git(t, c.Path, "rev-parse", "HEAD") != pr.OID {
		t.Fatalf("%+v", c)
	}
	if m := git(t, p, "config", "branch.pr/7.merge"); m != "refs/pull/7/head" {
		t.Fatalf("merge %q", m)
	}
	if exec.Command("git", "-C", p, "rev-parse", "-q", "--verify", "refs/pocket/pr/7").Run() == nil {
		t.Fatal("the fetched pull ref was left behind")
	}
}

func TestAPRWhoseHeadBranchIsGoneOpensFromThePullRef(t *testing.T) {
	p, bare, pr := pulled(t)
	git(t, bare, "update-ref", "-d", "refs/heads/feat")
	_, c := openedPR(t, p, pr)
	if git(t, c.Path, "rev-parse", "HEAD") != pr.OID || git(t, p, "config", "branch.feat.merge") != "refs/pull/7/head" {
		t.Fatalf("%+v", c)
	}
}

func TestAPRThatMovedWhileOpeningIsRefused(t *testing.T) {
	p, bare, pr := pulled(t)
	git(t, bare, "update-ref", "refs/pull/7/head", commitOn(t, bare, "refs/heads/feat"))
	if _, err := preparedPR(t, p, pr); err == nil || err.Error() != "PR #7 changed while opening, try again" {
		t.Fatal(err)
	}
}

func TestALocalBranchWithCommitsOffThePRIsRefused(t *testing.T) {
	p, _, pr := pulled(t)
	git(t, p, "commit", "-q", "--allow-empty", "-m", "mine")
	git(t, p, "branch", "feat")
	if _, err := preparedPR(t, p, pr); err == nil || err.Error() != "Branch feat has commits not in PR #7" {
		t.Fatal(err)
	}
}

func TestAMergedPROpensWithANote(t *testing.T) {
	p, _, pr := pulled(t)
	git(t, p, "branch", "feat")
	pr.State = "MERGED"
	if o, _ := openedPR(t, p, pr); o.Note != "Fast-forwarded feat; PR #7 is merged" {
		t.Fatalf("note %q", o.Note)
	}
}

func TestAPRAlreadyOpenOffItsHeadSaysSo(t *testing.T) {
	p, _, pr := pulled(t)
	tree := filepath.Join(filepath.Dir(p), "feat")
	git(t, p, "worktree", "add", "-q", "-b", "feat", tree)
	o, err := preparedPR(t, p, pr)
	if err != nil || o.Adopt != tree || o.Note != "Already open in feat; feat isn't at PR #7's head" {
		t.Fatalf("%+v, %v", o, err)
	}
}

func TestAFailedPROpenDeletesOnlyTheBranchItMade(t *testing.T) {
	p, _, pr := pulled(t)
	git(t, p, "branch", "feat")
	fork := pr
	fork.Fork = true
	for _, pr := range []PR{pr, fork} {
		o, err := preparedPR(t, p, pr)
		if err != nil {
			t.Fatal(err)
		}
		os.MkdirAll(filepath.Dir(o.Path), 0o755)
		os.WriteFile(o.Path, nil, 0o644)
		if _, err := o.Add(); err == nil {
			t.Fatalf("%s: added over a file", o.Branch)
		}
	}
	if b := git(t, p, "branch", "--list", "feat", "pr/7"); b != "feat" {
		t.Fatalf("branches %q", b)
	}
}

func TestThePRsRemoteIsTheOneWithItsRepo(t *testing.T) {
	p := repo(t)
	git(t, p, "remote", "add", "up", "git@github.com:Acme/Pocket.git/")
	o := &Open{Project: p, env: os.Environ()}
	if r, err := o.remoteFor("https://github.com/acme/pocket/pull/7"); r != "up" || err != nil {
		t.Fatalf("%q, %v", r, err)
	}
	if _, err := o.remoteFor("https://github.com/other/thing/pull/7"); err == nil || err.Error() != "No remote points at other/thing" {
		t.Fatal(err)
	}
}
```

**Step 2: Run, expect failure**
```
cd packages/pocketd && go test ./internal/worktree/ -run 'PR|Remote'
```
Expected: `undefined: PreparePR`, `o.remoteFor undefined`.

**Step 3: Implement** — append to `pr.go`:
```go
// PreparePR readies pr's head for a new Worktree named name, or after its
// branch when name is empty: the head branch for a PR from this repo, pr/<n>
// for one from a fork.
func PreparePR(f registry.File, project, name string, pr PR, env []string) (*Open, error) {
	branch := pr.Head
	if pr.Fork {
		branch = fmt.Sprintf("pr/%d", pr.Number)
	}
	o, err := prepare(f, project, name, branch, env)
	if err != nil {
		return nil, err
	}
	remote, err := o.remoteFor(pr.URL)
	if err != nil {
		return nil, err
	}
	live, oid := false, ""
	if !pr.Fork {
		tracking := "refs/remotes/" + remote + "/" + pr.Head
		if _, err := o.git("fetch", "-q", "--no-tags", remote, "+refs/heads/"+pr.Head+":"+tracking); err == nil {
			live, oid = true, o.at(tracking)
		}
	}
	if !live {
		// A private ref, as FETCH_HEAD is shared with whatever else fetches.
		ref := fmt.Sprintf("refs/pocket/pr/%d", pr.Number)
		if _, err := o.git("fetch", "-q", "--no-tags", remote, fmt.Sprintf("+refs/pull/%d/head:%s", pr.Number, ref)); err != nil {
			return nil, err
		}
		oid = o.at(ref)
		o.git("update-ref", "-d", ref)
	}
	if oid != pr.OID {
		return nil, failed("PR #%d changed while opening, try again", pr.Number)
	}
	if !o.adopt() {
		if err := o.branchAt(oid, pr.Number); err != nil {
			return nil, err
		}
		if err := o.track(remote, pr, live); err != nil {
			o.rollback()
			return nil, err
		}
	} else if o.at("refs/heads/"+o.Branch) != oid {
		o.note(fmt.Sprintf("%s isn't at PR #%d's head", o.Branch, pr.Number))
	}
	if pr.State != "OPEN" {
		o.note(fmt.Sprintf("PR #%d is %s", pr.Number, strings.ToLower(pr.State)))
	}
	return o, nil
}

// remoteFor is the remote whose URL ends in the OWNER/REPO of prURL, origin
// first.
func (o *Open) remoteFor(prURL string) (string, error) {
	u, err := url.Parse(prURL)
	if err != nil {
		return "", err
	}
	parts := strings.Split(strings.Trim(u.Path, "/"), "/")
	if len(parts) < 2 {
		return "", failed("No remote points at %s", prURL)
	}
	repo := parts[0] + "/" + parts[1]
	out, err := o.git("remote")
	if err != nil {
		return "", err
	}
	remotes := strings.Fields(out)
	if i := slices.Index(remotes, "origin"); i > 0 {
		remotes[0], remotes[i] = remotes[i], remotes[0]
	}
	for _, r := range remotes {
		at, _ := o.git("remote", "get-url", r)
		at = strings.TrimSuffix(strings.TrimSuffix(strings.ToLower(at), "/"), ".git")
		if want := strings.ToLower(repo); strings.HasSuffix(at, "/"+want) || strings.HasSuffix(at, ":"+want) {
			return r, nil
		}
	}
	return "", failed("No remote points at %s", repo)
}

// branchAt makes Branch, or moves it forward, to oid. A Branch already at or
// past oid is kept; one with commits off the PR fails.
func (o *Open) branchAt(oid string, n int) error {
	local := o.at("refs/heads/" + o.Branch)
	switch {
	case local == "":
		if _, err := o.git("branch", "--no-track", o.Branch, oid); err != nil {
			return err
		}
		o.created, o.oid = true, oid
	case ancestor(o.Project, oid, local):
	case ancestor(o.Project, local, oid):
		if _, err := o.git("update-ref", "refs/heads/"+o.Branch, oid, local); err != nil {
			return err
		}
		o.note("Fast-forwarded " + o.Branch)
	default:
		return failed("Branch %s has commits not in PR #%d", o.Branch, n)
	}
	return nil
}

// track makes Branch pull from the PR's head branch while it lives, else from
// the PR's ref, as gh pr checkout does. A plain git push then refuses, since
// the upstream's name differs, so a fork's PR isn't pushed to the wrong place.
func (o *Open) track(remote string, pr PR, live bool) error {
	if live {
		_, err := o.git("branch", "--set-upstream-to="+remote+"/"+pr.Head, o.Branch)
		return err
	}
	if _, err := o.git("config", "branch."+o.Branch+".remote", remote); err != nil {
		return err
	}
	_, err := o.git("config", "branch."+o.Branch+".merge", fmt.Sprintf("refs/pull/%d/head", pr.Number))
	return err
}
```

**Step 4: Run, expect pass**
```
cd packages/pocketd && go vet ./internal/worktree/ && go test ./internal/worktree/
```
Expected: `ok`. All 9 PR tests and the PR 1 tests pass.

### Task 2.4: launch opens the PR

**Files:**
- `packages/pocketd/internal/launch/launch.go` (`checkout`, after the Task 1.3 branch block; new `openPR`)
- `packages/pocketd/internal/launch/launch_test.go` (append)

**Context:**
- **gh lookup.** gh is looked up with `terminal.LookPath("gh", env)` on the login env. GUI-launched pocketd has a bare PATH, while Homebrew's gh lives in `/opt/homebrew/bin`. No gh → `worktree.ErrNoGh`.
- **The test needs no gh.** `onPath` sets `PATH=<tmp>:/usr/bin:/bin`, and `LoginEnv` on a bare `Daemon{}` is `os.Environ()`. So the lookup fails deterministically, and gh is never run.
- **Reuse.** The rest reuses `opened` and `filled` (Task 1.3): adoption, copy, setup and notes all behave as for a branch.

**Step 1: Failing test** — append:
```go
func TestAPRWithoutGhAsksToInstallIt(t *testing.T) {
	l, s := newWorktree(t, "")
	s.Checkout.New = &proto.NewWorktree{PR: "7"}
	r := l.Create(Who{Owner: true, Key: "owner"}, "r1", s, func(string, string) {}, func(Creating) {})
	if r.Err == nil || r.Err.Code != "spawn_failed" || r.Err.Message != "Install GitHub CLI (gh) to open pull requests" {
		t.Fatalf("got %+v", r.Err)
	}
}
```

**Step 2: Run, expect failure**
```
cd packages/pocketd && go test ./internal/launch/ -run APRWithoutGh
```
Expected: `got &{Code:invalid_name Message:Use letters, digits, - _ or . …}`. The empty Name falls through to `Prepare`.

**Step 3: Implement** — in `checkout`, after the `n.Branch` block:
```go
	if n.PR != "" {
		progress(proto.StepFetch, "")
		o, err := openPR(f, s, env)
		return opened(f, s, o, err, progress)
	}
```
```go
func openPR(f registry.File, s proto.LaunchSpec, env []string) (*worktree.Open, error) {
	gh, err := terminal.LookPath("gh", env)
	if err != nil {
		return nil, worktree.ErrNoGh
	}
	pr, err := worktree.View(gh, s.Project, s.Checkout.New.PR, env)
	if err != nil {
		return nil, err
	}
	return worktree.PreparePR(f, s.Project, s.Checkout.New.Name, pr, env)
}
```

**Step 4: Run, expect pass**
```
cd packages/pocketd && go vet ./... && go test ./internal/launch/
```
Expected: `ok`.

### Task 2.5: The sheet opens a pull request

**Files:**
- `packages/desktop/crates/pocket/src/modals/new_session.rs`, as left by Task 1.5:
  - `Source` gains `Pr`
  - `placeholder`, `ready`, `folder` and `spec`
  - `parse_pr` before `slug`
  - the view's branch chip, problem and summary
  - the tests

**Context:**
- **Pure decisions.** `parse_pr` and the `Draft` methods are pure, so they carry the tests. The sheet accepts only what gh will take: "123", "#123", or an http(s) URL whose 4th path part is `pull`. It sends the text as typed, and gh resolves it.
- **No branch chip** in PR mode: there is nothing to pick.
- **Provisional folder** is `pr-<n>`. A same-repo PR really lands in `<head with / as ->`, and `Creates::started` swaps it in once pocketd answers.
- **Problem line.** A non-empty field that is not a PR shows "Use 123, #123 or a PR URL".
- **No pull-request icon** exists in `crates/theme/assets/icons`, so PR mode reuses `branch`.

**Step 1: Failing test** — the import gains `parse_pr`. Add:
```rust
    #[test]
    fn a_pr_is_a_number_a_hash_number_or_a_pull_url() {
        for ok in ["123", " #123 ", "https://github.com/acme/app/pull/123", "https://github.com/acme/app/pull/123/files"] {
            assert_eq!(parse_pr(ok), Some(123), "{ok}");
        }
        for bad in ["", "#", "+5", "0", "abc", "https://github.com/acme/app/issues/123"] {
            assert_eq!(parse_pr(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_pr_is_sent_as_typed_and_reopens_as_a_pr() {
        let draft = Draft { worktree: true, source: Source::Pr, repo: Some("/p".into()), copy_env: true, ..Draft::default() };
        assert!(draft.ready("#7", false) && !draft.ready("seven", false));
        let want = json!({"project": "/p", "checkout": {"new": {"pr": "#7", "copy": true, "setup": false}}, "provider": "claude", "access": "ask", "plan": false});
        assert_eq!(draft.spec("/p", "/p", "#7", ""), want);
        assert_eq!(draft.folder("#7"), "pr-7");
        assert_eq!(Source::of(&want["checkout"]["new"], "pr-7".into()), (Source::Pr, "#7".to_string()));
    }
```

**Step 2: Run, expect failure**
```
cd packages/desktop && cargo test -p pocket new_session
```
Expected: `unresolved import super::parse_pr`, `no variant named Pr`.

**Step 3: Implement** (diff against Task 1.5's result; test hunks omitted):
```diff
@@ -63,23 +63,27 @@
     New,
     /// An existing branch, local or on origin.
     Branch,
+    /// A GitHub pull request's head.
+    Pr,
 }
 
 impl Source {
-    const ALL: [Source; 2] = [Source::New, Source::Branch];
+    const ALL: [Source; 3] = [Source::New, Source::Branch, Source::Pr];
 
     fn label(self) -> &'static str {
         match self {
             Source::New => "New branch",
             Source::Branch => "Existing branch",
+            Source::Pr => "Pull request",
         }
     }
 
     /// What a create's `checkout.new` asked for, and what its name field held.
     fn of(new: &Value, folder: String) -> (Self, String) {
-        match new["branch"].as_str() {
-            Some(b) => (Source::Branch, b.to_string()),
-            None => (Source::New, folder),
+        match (new["branch"].as_str(), new["pr"].as_str()) {
+            (Some(b), _) => (Source::Branch, b.to_string()),
+            (_, Some(pr)) => (Source::Pr, pr.to_string()),
+            _ => (Source::New, folder),
         }
     }
 }
@@ -146,6 +150,7 @@
         match self.source {
             Source::New => self.auto_name(prompt),
             Source::Branch => "Branch to open".into(),
+            Source::Pr => "123, #123 or a PR URL".into(),
         }
     }
 
@@ -155,6 +160,7 @@
             (false, _) => in_tree,
             (true, Source::New) => self.repo.is_some() && !self.branches.is_empty() && name_problem(name, &self.taken).is_none(),
             (true, Source::Branch) => self.repo.is_some() && !name.is_empty(),
+            (true, Source::Pr) => self.repo.is_some() && parse_pr(name).is_some(),
         };
         place && self.pending.is_none()
     }
@@ -164,6 +170,7 @@
         match self.source {
             Source::New => name.to_string(),
             Source::Branch => name.replace('/', "-"),
+            Source::Pr => format!("pr-{}", parse_pr(name).unwrap_or_default()),
         }
     }
 
@@ -180,6 +187,7 @@
             (false, _) => json!({"worktree": tree}),
             (true, Source::New) => json!({"new": {"name": name, "base": self.base_branch(), "copy": self.copy_env, "setup": self.run_setup}}),
             (true, Source::Branch) => json!({"new": {"branch": name, "copy": self.copy_env, "setup": self.run_setup}}),
+            (true, Source::Pr) => json!({"new": {"pr": name, "copy": self.copy_env, "setup": self.run_setup}}),
         };
         let mut spec = json!({"project": project, "checkout": checkout, "provider": self.provider, "access": Access::Ask.wire(), "plan": false});
         for (key, value) in [("model", &self.model), ("effort", &self.effort)] {
@@ -222,6 +230,19 @@
     }
 }
 
+/// The number in "123", "#123" or a pull request URL.
+fn parse_pr(s: &str) -> Option<u32> {
+    let s = s.trim();
+    let n = match s.strip_prefix("https://").or_else(|| s.strip_prefix("http://")) {
+        Some(url) => match url.split('/').collect::<Vec<_>>()[..] {
+            [_, _, _, "pull", n, ..] => n,
+            _ => return None,
+        },
+        None => s.strip_prefix('#').unwrap_or(s),
+    };
+    n.bytes().all(|b| b.is_ascii_digit()).then(|| n.parse().ok()).flatten().filter(|&n| n > 0)
+}
+
 fn slug(prompt: &str) -> String {
     prompt.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).take(4).map(str::to_lowercase).collect::<Vec<_>>().join("-")
 }
@@ -507,13 +528,20 @@
             .child(div().ml_auto().child(close));
         let agent = self.agent_select(cx);
         let source = (f.worktree && self.agents.opens()).then(|| self.source_select(cx));
-        let branch = f.worktree.then(|| self.branch_select(cx));
+        let branch = (f.worktree && f.source != Source::Pr).then(|| self.branch_select(cx));
         let ready = self.session_ready(cx);
         let send = ui::primary(div().id("form-start").ml_auto().size(px(32.)).flex().flex_none().items_center().justify_center().rounded(px(16.)).cursor_pointer())
             .child(icon("arrow-up", 16., ON_TEXT))
             .when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.start_session(window, cx))))
             .when(!ready, |d| d.opacity(0.5).cursor_default());
-        let problem = if f.worktree && f.source == Source::New { name_problem(&self.new_name(cx), &f.taken) } else { None };
+        let problem = match (f.worktree, f.source) {
+            (true, Source::New) => name_problem(&self.new_name(cx), &f.taken),
+            (true, Source::Pr) => {
+                let name = self.new_name(cx);
+                (!name.is_empty() && parse_pr(&name).is_none()).then_some("Use 123, #123 or a PR URL")
+            }
+            _ => None,
+        };
         let name_field = f.worktree.then(|| {
             ui::field_box()
                 .child(icon(if f.source == Source::New { "worktree" } else { "branch" }, 14., TEXT_3))
@@ -565,6 +593,7 @@
                 mono(f.base_branch()).into_any_element(),
             ],
             (true, Source::Branch) => vec![div().child("Open branch").into_any_element(), mono(self.new_name(cx)).into_any_element()],
+            (true, Source::Pr) => vec![div().child("Open pull request").into_any_element(), mono(self.new_name(cx)).into_any_element()],
             (false, _) => {
                 let place = self.repo().map(|r| r.branch.clone()).or_else(|| self.cwd().map(|c| tilde(&c))).unwrap_or_default();
                 vec![div().child("In").into_any_element(), mono(place).into_any_element()]
```

**Step 4: Run, expect pass**
```
cd packages/desktop && cargo test -p pocket new_session
```
Expected: 19 passed. Capture the sheet in PR mode and compare: the chip row reads Agent · Pull request · send, and the summary reads "Open pull request #7".

### Task 2.6: Full checks

**Step 4:**
```
cd packages/desktop && cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace
cd packages/pocketd && go vet ./... && go test ./...
cd packages/protocol && pnpm test
```
Expected: everything passes, with no new clippy warnings. Optionally, by hand with a real `gh auth login`, open these and check the branch, the upstream (`git rev-parse --abbrev-ref @{u}`) and the note:
- a same-repo PR
- a fork PR
- a merged PR

---

## PR 3: Delete a worktree with teardown and its branch

**Scope.** Design `docs/designs/2026-10-05-worktree-gaps.md` §3 and phasing step 3, desktop only. A per-project teardown script ("When a worktree is deleted") runs in the worktree before it is removed; a second row-menu item "Delete worktree and branch…" also deletes the branch; the confirm warns about commits no other branch holds; a failed teardown offers "Delete anyway"; the row shows "Deleting…" while it runs. pocketd is untouched.

**Depends on.** Nothing. Base `4074f80`.

**Done when.**
- `store::RepoConfig.teardown` saves from Project settings and old `desktop.json` files load with it empty.
- `git::lost_commits`, `git::delete_branch`, `daemon::run_script` exist with real-repo / real-shell tests.
- `pocket/src/removal.rs` owns `Removals`, `Removal`, `needs_confirm`, `branch_deletable`, `run`, and the thin `impl Desktop` for asking and deleting; `ask_delete_worktree` / `delete_worktree` are gone from `desktop/project.rs`.
- Clean worktree with no terminals deletes without a confirm (as today); dirty, terminals open, or lost commits confirm.
- From `packages/desktop`: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace` pass with no new warnings.

Paths below are relative to the repo root `/Users/mingo/.worktrees/coding-pocket/worktree-management`; run cargo from `packages/desktop`. `pk` = `packages/desktop/crates/pocket/src`. Line numbers are at `4074f80`; re-find by the quoted text if they moved.

Conventions for every task (from `CLAUDE.md` and `docs/adr/0003-desktop-code-layout.md`): tests sit in `#[cfg(test)] mod tests` at the bottom of the file, named as sentences; no mocks, no render tests; git writes are tested on temp repos; comments only for the non-obvious why; match the terse one-line-builder style around you.

---

### Task 3.0: Capture the screens before any change

At `4074f80`, before Task 3.1, from the repo root (needs `bun`; builds release with `--features capture`):

```sh
env -u POCKETD_SOCK .ui-review/fixture/capture.sh /tmp/pr3-before add-repo=add-repo repositories-worktrees=
```

`add-repo` is a capture step (opens the Add project modal); a name with no steps after `=` captures the scenario as loaded (`repositories-worktrees` uses the worktrees fixture). Expect two PNGs in `/tmp/pr3-before`. No code changes, no commit.

---

### Task 3.1: Teardown setting per project

**Files**
- Modify `packages/desktop/crates/store/src/store.rs` — `RepoConfig` (lines 6-17); add a test at the end of `mod tests` (before the final `}` at line 313).
- Modify `packages/desktop/crates/pocket/src/modals/add_project.rs` — `RepoForm` (25-30), `RepoForm::new` (103-118), `reset_repo_form` (160), `save_repo` (210-218), `repo_view` (378-381 and 436).

**Context**
- `RepoConfig` is `#[serde(default)]`, so a missing key loads as `""`; mimic `setup`.
- `RepoForm.setup` is the field to copy: an `Entity<InputState>` built in `new`, filled in `reset_repo_form`, read and trimmed in `save_repo`, rendered with `field("When a worktree is created", …)`. `teardown` is the same with label "When a worktree is deleted" and placeholder `docker compose down`.
- `save_repo` builds `RepoConfig { … }` listing every field, so it fails to compile until `teardown` is added there. No other `RepoConfig` literal lists every field (others use `..Default::default()`).
- pocketd never reads `teardown`; only the desktop runs it (Task 3.5).

**Step 1: failing test** — append inside `mod tests` in `store.rs`:

```rust
    #[test]
    fn a_teardown_round_trips_and_an_old_desktop_json_loads_without_one() {
        let old: RepoConfig = serde_json::from_str(r#"{"name":"w","setup":"make"}"#).unwrap();
        assert_eq!(old.teardown, "");
        let cfg = RepoConfig { teardown: "docker compose down".into(), ..Default::default() };
        let back: RepoConfig = serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(back, cfg);
    }
```

**Step 2: run, expect failure**

```sh
cd packages/desktop && cargo test -p store teardown
```

Expected: compile error `E0560 struct RepoConfig has no field named teardown` (and `E0609 no field teardown`).

**Step 3: implement**

`store.rs`, `RepoConfig` — add after `pub setup: String,`:

```rust
    pub teardown: String,
```

`add_project.rs`:

1. `RepoForm` struct — after `setup: Entity<InputState>,` add:
   ```rust
       teardown: Entity<InputState>,
   ```
2. `RepoForm::new` — after the `let setup = …placeholder("pnpm install"));` line add:
   ```rust
           let teardown = cx.new(|cx| InputState::new(window, cx).placeholder("docker compose down"));
   ```
   and change the return to:
   ```rust
           (Self { url, name, setup, teardown, draft: RepoDraft::default() }, subs)
   ```
3. `reset_repo_form` — after `f.setup.update(cx, |s, cx| s.set_value(cfg.setup, window, cx));` add:
   ```rust
           f.teardown.update(cx, |s, cx| s.set_value(cfg.teardown, window, cx));
   ```
4. `save_repo` — in the `RepoConfig { … }` literal, after `setup: f.setup.read(cx).value().trim().to_string(),` add:
   ```rust
               teardown: f.teardown.read(cx).value().trim().to_string(),
   ```
5. `repo_view` — after the `let setup = field("When a worktree is created", …);` statement add:
   ```rust
           let teardown = field(
               "When a worktree is deleted",
               ui::field_box().child(icon("terminal", 13., TEXT_3)).child(div().flex_1().font_family(MONO).child(Input::new(&f.teardown).appearance(false).p_0().text_size(px(13.)))),
           );
   ```
   and in `body.extend([…])` change
   `div().flex().flex_col().gap(px(10.)).child(setup).child(copies).into_any_element(),`
   to
   `div().flex().flex_col().gap(px(10.)).child(setup).child(teardown).child(copies).into_any_element(),`

**Step 4: run, expect pass**

```sh
cd packages/desktop && cargo test -p store && cargo build -p pocket
```

Expected: all store tests pass (including `a_teardown_round_trips_and_an_old_desktop_json_loads_without_one`); pocket builds with no new warnings.

---

### Task 3.2: `git::lost_commits` and `git::delete_branch`

**Files**
- Modify `packages/desktop/crates/git/src/git.rs` — add two functions right after `remove_worktree` (lines 171-175); add tests at the end of `mod tests` (before the final `}` at line 935).

**Context**
- `git(cwd, args) -> Option<String>` (line 43) returns stdout on success; use it for the read. Writes return `Result<(), String>` with trimmed stderr, exactly like `remove_worktree` / `clone`.
- Query (verified on temp repos): branch `b` → `git rev-list --count refs/heads/<b> --not --exclude=<b> --branches --remotes --tags`. `--exclude` matches the names `--branches` lists, which have no `refs/heads/` prefix; `--exclude=refs/heads/<b>` matches nothing and always counts 0. Detached → `git rev-list --count HEAD --not --branches --remotes --tags` run in the tree.
- With a branch any checkout of the repo works as `cwd` (refs are shared), so callers pass the project path; detached needs the tree itself.
- `git branch -D` refuses while a worktree has the branch checked out (`error: cannot delete branch 'b' used by worktree at …`), so it must run after `remove_worktree`.
- Test helpers already in `mod tests`: `scratch_repo(tag) -> PathBuf` (dir holding `repo/` with one commit), `add_worktree(repo, path, branch, base)`, `committer(repo)` (sets user and disables gpgsign in the shared config), `sh(repo, args)` (asserts a git command succeeds). The initial branch name depends on the machine, so tests run `sh(&repo, &["branch", "-M", "main"])` first, as `tips_follow_the_branch_its_upstream_and_main` does.

**Step 1: failing tests** — append inside `mod tests`:

```rust
    fn commit_file(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), "x\n").unwrap();
        sh(dir, &["add", "."]);
        sh(dir, &["commit", "-qm", name]);
    }

    #[test]
    fn lost_commits_are_those_no_other_branch_remote_or_tag_holds() {
        let dir = scratch_repo("lost");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        committer(&repo);
        sh(&repo, &["branch", "-M", "main"]);
        let tree = dir.join("fix");
        add_worktree(r, tree.to_str().unwrap(), "fix/a", "HEAD").unwrap();
        assert_eq!(lost_commits(r, Some("fix/a")), 0);
        commit_file(&tree, "b");
        commit_file(&tree, "c");
        assert_eq!(lost_commits(r, Some("fix/a")), 2);
        sh(&repo, &["update-ref", "refs/remotes/origin/fix/a", "fix/a~1"]);
        assert_eq!(lost_commits(r, Some("fix/a")), 1);
        sh(&repo, &["update-ref", "refs/remotes/origin/fix/a", "fix/a"]);
        assert_eq!(lost_commits(r, Some("fix/a")), 0);
        sh(&repo, &["update-ref", "-d", "refs/remotes/origin/fix/a"]);
        sh(&repo, &["merge", "-q", "--ff-only", "fix/a"]);
        assert_eq!(lost_commits(r, Some("fix/a")), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_detached_tree_loses_the_commits_only_its_head_holds() {
        let dir = scratch_repo("lost-detached");
        let repo = dir.join("repo");
        committer(&repo);
        let tree = dir.join("look");
        sh(&repo, &["worktree", "add", "-q", "--detach", tree.to_str().unwrap()]);
        let t = tree.to_str().unwrap();
        assert_eq!(lost_commits(t, None), 0);
        commit_file(&tree, "b");
        assert_eq!(lost_commits(t, None), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_branch_deletes_once_its_worktree_is_removed() {
        let dir = scratch_repo("delete-branch");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        let tree = dir.join("fix");
        add_worktree(r, tree.to_str().unwrap(), "fix", "HEAD").unwrap();
        assert!(delete_branch(r, "fix").unwrap_err().contains("fix"));
        remove_worktree(r, tree.to_str().unwrap()).unwrap();
        delete_branch(r, "fix").unwrap();
        assert!(!branches(r).contains(&"fix".to_string()));
        assert!(delete_branch(r, "fix").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
```

**Step 2: run, expect failure**

```sh
cd packages/desktop && cargo test -p git
```

Expected: compile errors `E0425 cannot find function lost_commits` and `cannot find function delete_branch`.

**Step 3: implement** — in `git.rs`, directly after `remove_worktree`:

```rust
/// Commits that deleting `branch` would lose, or with no branch the detached HEAD of `cwd`: those no other branch, remote or tag holds.
pub fn lost_commits(cwd: &str, branch: Option<&str>) -> usize {
    let (tip, exclude) = match branch {
        // `--exclude` matches the names `--branches` lists, which lack `refs/heads/`.
        Some(b) => (format!("refs/heads/{b}"), Some(format!("--exclude={b}"))),
        None => ("HEAD".to_string(), None),
    };
    let mut args = vec!["rev-list", "--count", tip.as_str(), "--not"];
    args.extend(exclude.as_deref());
    args.extend(["--branches", "--remotes", "--tags"]);
    git(cwd, &args).and_then(|n| n.trim().parse().ok()).unwrap_or(0)
}

/// Deletes `branch`, merged or not; git refuses while a worktree has it checked out.
pub fn delete_branch(repo: &str, branch: &str) -> Result<(), String> {
    let out = Command::new("git").arg("-C").arg(repo).args(["branch", "-D", "--", branch]).output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}
```

**Step 4: run, expect pass**

```sh
cd packages/desktop && cargo test -p git
```

Expected: all git tests pass, including the three new ones.

---

### Task 3.3: `daemon::run_script` with a timeout that kills what the script started

**Files**
- Modify `packages/desktop/crates/daemon/src/daemon.rs` — imports (lines 6 and 9); add `run_script` / `run_script_in` right after `run_in` (ends line 152, before `fn spawn_op`); tests after `runs_argv_in_real_shells_with_stdin` (lines 290-300).

**Context**
- `run_login(argv, cwd, input)` → `run_in(shell, env, …)` (lines 120-152) is the pattern: the public fn passes `login_shell()` and `terminal_env()`, the private `_in` fn takes them so tests can use real `/bin/sh` etc. with a bare env (`runs_argv_in_real_shells_with_stdin`).
- Unlike `run_in`, this runs a script string (`<shell> -l -c <script>`), the same way pocketd runs the setup script in the user's shell (`packages/pocketd/internal/launch/wrap.go`), so fish users write fish.
- `process_group(0)` (`std::os::unix::process::CommandExt`) puts the shell in its own group; on timeout `libc::kill(-pid, SIGKILL)` kills it and everything it started. `libc` is already a dependency.
- stdout and stderr share one `std::io::pipe()` so the tail interleaves as a terminal would. A thread reads it to EOF so a chatty script never blocks on a full pipe. EOF arrives only when the script and everything it started have exited, so `recv_timeout` on that thread's result is the timeout.
- `Err` is the last 20 lines of output, else the exit status (`exit status: 4`); on timeout `Timed out after {timeout:?}` (`120s` for the 2-minute limit used in Task 3.4).
- Prototyped and passing with `/bin/zsh`, `/bin/bash`, `/bin/sh` and fish, clippy clean.

**Step 1: failing tests** — add in `mod tests` after `runs_argv_in_real_shells_with_stdin`:

```rust
    fn bare_env() -> Vec<(String, String)> {
        let home = std::env::temp_dir().join("pocket-desktop-no-home");
        vec![("HOME".to_string(), home.to_string_lossy().into_owned()), ("PATH".to_string(), "/usr/bin:/bin".to_string())]
    }

    #[test]
    fn a_script_returns_its_output_and_on_failure_its_last_twenty_lines() {
        for shell in ["/bin/zsh", "/bin/bash", "/bin/sh", "/opt/homebrew/bin/fish"].into_iter().filter(|s| Path::new(s).exists()) {
            let wait = Duration::from_secs(5);
            assert_eq!(run_script_in(shell, bare_env(), "echo out; echo err >&2", "/", wait), Ok("out\nerr".to_string()), "{shell}");
            let tail: Vec<String> = (12..=30).map(|n| n.to_string()).chain(["oops".to_string()]).collect();
            assert_eq!(run_script_in(shell, bare_env(), "seq 30; echo oops >&2; exit 3", "/", wait), Err(tail.join("\n")), "{shell}");
            assert_eq!(run_script_in(shell, bare_env(), "exit 4", "/", wait), Err("exit status: 4".to_string()), "{shell}");
        }
    }

    #[test]
    fn a_script_that_runs_too_long_is_killed_with_what_it_started() {
        let marker = std::env::temp_dir().join(format!("pocket-script-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let started = std::time::Instant::now();
        let script = format!("(sleep 1; touch '{}') & sleep 5", marker.display());
        assert_eq!(run_script_in("/bin/sh", bare_env(), &script, "/", Duration::from_millis(300)), Err("Timed out after 300ms".to_string()));
        assert!(started.elapsed() < Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(1200));
        assert!(!marker.exists(), "the background job outlived the timeout");
    }
```

**Step 2: run, expect failure**

```sh
cd packages/desktop && cargo test -p daemon script
```

Expected: compile error `E0425 cannot find function run_script_in`.

**Step 3: implement**

Imports — change line 6 to `use std::io::{BufRead, BufReader, Read, Write};` and add after line 9 (`use std::process::{Command, Stdio};`):

```rust
use std::os::unix::process::CommandExt;
```

After `run_in` (before `fn spawn_op`):

```rust
/// Runs `script` in `cwd` under the login shell, in its own process group so a timeout kills whatever it started too.
/// Its output, or on failure its last 20 lines.
pub fn run_script(script: &str, cwd: &str, timeout: Duration) -> Result<String, String> {
    run_script_in(login_shell(), terminal_env(), script, cwd, timeout)
}

fn run_script_in(shell: &str, env: Vec<(String, String)>, script: &str, cwd: &str, timeout: Duration) -> Result<String, String> {
    let (mut out, writer) = std::io::pipe().map_err(|e| e.to_string())?;
    let mut child = Command::new(shell)
        .args(["-l", "-c", script])
        .current_dir(cwd)
        .env_clear()
        .envs(env)
        .stdin(Stdio::null())
        .stdout(writer.try_clone().map_err(|e| e.to_string())?)
        .stderr(writer)
        .process_group(0)
        .spawn()
        .map_err(|e| e.to_string())?;
    let (tx, rx) = std::sync::mpsc::channel();
    // Read on its own thread: a script that prints more than the pipe holds blocks until someone reads it.
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out.read_to_end(&mut buf);
        let _ = tx.send(buf);
    });
    // The pipe closes once the script and everything it started have exited.
    let Ok(buf) = rx.recv_timeout(timeout) else {
        unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
        let _ = child.wait();
        return Err(format!("Timed out after {timeout:?}"));
    };
    let status = child.wait().map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&buf).trim().to_string();
    if status.success() {
        return Ok(text);
    }
    let lines: Vec<&str> = text.lines().collect();
    let tail = lines[lines.len().saturating_sub(20)..].join("\n");
    Err(if tail.is_empty() { status.to_string() } else { tail })
}
```

**Step 4: run, expect pass**

```sh
cd packages/desktop && cargo test -p daemon && cargo clippy -p daemon --all-targets
```

Expected: all daemon tests pass (the timeout test takes ~1.5s); `run_script` is `pub`, so no unused warning; clippy reports nothing new.

---

### Task 3.4: `removal.rs` — what a deletion does, as plain data

**Files**
- Create `packages/desktop/crates/pocket/src/removal.rs`.
- Modify `packages/desktop/crates/pocket/src/main.rs` — add `mod removal;` between `mod panels;` (line 12) and `mod sidebar;` (line 13).

**Context**
- ADR 0003: one module per feature with its own state; logic as free functions or methods on plain values so it is testable without GPUI. `Removal` is that value; `run` is the whole background job (teardown → `git::remove_worktree` → `git::delete_branch`), with no `Window`/`Context`. Task 3.5 adds the `Removals` state and the thin `impl Desktop`.
- Teardown goes first because scripts like `docker compose down` need the folder; it is skipped when the folder is already gone (`git worktree remove --force` succeeds on a missing folder, verified) or when the script is empty (the setting is trimmed on save).
- A branch-delete failure after removal is `Failure::Branch`; the worktree stays deleted.
- `Removal::lost` counts only what this deletion loses: a detached tree's HEAD commits (always), a branch's commits only when the branch goes too. With a branch it queries the project path, so a missing tree folder doesn't hide lost commits.
- `git::Worktree.branch` is the literal `"detached"` for a detached tree (`git.rs` `parse_worktrees`); Task 3.5 maps that to `branch: None`.
- Until Task 3.5 wires it up, `cargo build` warns that `Removal`, `run`, etc. are never used. Expected; they're gone after 3.5.
- `a_failed_teardown_deletes_nothing` runs your real login shell (`daemon::run_script`), like the existing real-shell tests; the script is portable across sh/zsh/bash/fish.

**Step 1: failing tests** — create `removal.rs` with only the tests, and add `mod removal;` to `main.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::{Failure, Removal, needs_confirm, run};
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn sh(dir: &Path, args: &[&str]) {
        let out = Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    /// A repository with a worktree `feat` on branch `feat`, and the removal of both.
    fn scratch(tag: &str) -> (PathBuf, Removal) {
        let dir = std::env::temp_dir().join(format!("pocket-removal-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let (repo, tree) = (dir.join("repo"), dir.join("feat"));
        std::fs::create_dir_all(&repo).unwrap();
        sh(&repo, &["init", "-q"]);
        sh(&repo, &["commit", "-q", "--allow-empty", "-m", "init"]);
        sh(&repo, &["worktree", "add", "-q", "-b", "feat", tree.to_str().unwrap()]);
        let removal = Removal { project: repo.to_string_lossy().into_owned(), tree: tree.to_string_lossy().into_owned(), branch: Some("feat".into()), delete_branch: true, teardown: true };
        (dir, removal)
    }

    #[test]
    fn deleting_asks_only_when_it_would_lose_work_or_close_terminals() {
        assert!(!needs_confirm(0, 0, 0));
        assert!(needs_confirm(2, 0, 0));
        assert!(needs_confirm(0, 1, 0));
        assert!(needs_confirm(0, 0, 3));
    }

    #[test]
    fn lost_commits_count_only_when_the_branch_goes_or_the_tree_is_detached() {
        let (dir, r) = scratch("lost");
        sh(Path::new(&r.tree), &["commit", "-q", "--allow-empty", "-m", "b"]);
        assert_eq!(r.lost(), 1);
        assert_eq!(Removal { delete_branch: false, ..r.clone() }.lost(), 0);
        sh(Path::new(&r.tree), &["checkout", "-q", "--detach"]);
        sh(Path::new(&r.tree), &["commit", "-q", "--allow-empty", "-m", "c"]);
        assert_eq!(Removal { branch: None, delete_branch: false, ..r.clone() }.lost(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn deletes_the_worktree_then_its_branch() {
        let (dir, r) = scratch("both");
        assert_eq!(run(&r, ""), Ok(()));
        assert!(!Path::new(&r.tree).exists());
        assert!(!git::branches(&r.project).contains(&"feat".to_string()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_branch_that_cannot_be_deleted_leaves_the_worktree_deleted() {
        let (dir, r) = scratch("kept");
        let r = Removal { branch: Some("gone".into()), ..r };
        assert!(matches!(run(&r, ""), Err(Failure::Branch(b, _)) if b == "gone"));
        assert!(!Path::new(&r.tree).exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_teardown_deletes_nothing() {
        let (dir, r) = scratch("teardown");
        assert!(matches!(run(&r, "echo nope; exit 3"), Err(Failure::Teardown(tail)) if tail.ends_with("nope")));
        assert!(Path::new(&r.tree).exists());
        assert!(git::branches(&r.project).contains(&"feat".to_string()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn teardown_is_skipped_once_the_folder_is_gone() {
        let (dir, r) = scratch("gone");
        std::fs::remove_dir_all(&r.tree).unwrap();
        assert_eq!(run(&Removal { delete_branch: false, ..r.clone() }, "exit 1"), Ok(()));
        assert_eq!(git::worktrees(&r.project).len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
```

**Step 2: run, expect failure**

```sh
cd packages/desktop && cargo test -p pocket removal::
```

Expected: compile errors `E0432 unresolved imports super::Failure, super::Removal, super::needs_confirm, super::run`.

**Step 3: implement** — put this above the tests in `removal.rs`:

```rust
use std::path::Path;
use std::time::Duration;

const TEARDOWN_LIMIT: Duration = Duration::from_secs(120);

/// A worktree to delete, and what goes with it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Removal {
    pub(crate) project: String,
    pub(crate) tree: String,
    /// `None` when the tree is detached.
    pub(crate) branch: Option<String>,
    pub(crate) delete_branch: bool,
    pub(crate) teardown: bool,
}

impl Removal {
    /// Commits only this deletion loses: a detached tree's, or its branch's when that goes too.
    fn lost(&self) -> usize {
        match (&self.branch, self.delete_branch) {
            (None, _) => git::lost_commits(&self.tree, None),
            (Some(b), true) => git::lost_commits(&self.project, Some(b)),
            (Some(_), false) => 0,
        }
    }
}

#[derive(Debug, PartialEq)]
enum Failure {
    /// Nothing was deleted; holds the end of the script's output.
    Teardown(String),
    Remove(String),
    /// The worktree is gone but this branch stayed, for this reason.
    Branch(String, String),
}

fn needs_confirm(dirty: usize, terminals: usize, lost: usize) -> bool {
    dirty + terminals + lost > 0
}

/// Teardown goes first: scripts like `docker compose down` need the folder.
fn run(r: &Removal, teardown: &str) -> Result<(), Failure> {
    if r.teardown && !teardown.is_empty() && Path::new(&r.tree).is_dir() {
        daemon::run_script(teardown, &r.tree, TEARDOWN_LIMIT).map_err(Failure::Teardown)?;
    }
    git::remove_worktree(&r.project, &r.tree).map_err(Failure::Remove)?;
    match r.branch.as_deref().filter(|_| r.delete_branch) {
        Some(b) => git::delete_branch(&r.project, b).map_err(|e| Failure::Branch(b.to_string(), e)),
        None => Ok(()),
    }
}
```

**Step 4: run, expect pass**

```sh
cd packages/desktop && cargo test -p pocket removal::
```

Expected: 6 tests pass. Six `dead_code` warnings (`Removal`, `Removal::lost`, `Failure`, `needs_confirm`, `run`, `TEARDOWN_LIMIT`) in the non-test build are expected until Task 3.5.

---

### Task 3.5: Wire the delete flow — confirm, background run, "Deleting…", "Delete anyway"

**Files**
- Modify `packages/desktop/crates/pocket/src/removal.rs` — add `Removals` and the `impl Desktop`.
- Modify `packages/desktop/crates/pocket/src/desktop/project.rs` — delete `ask_delete_worktree` and `delete_worktree` (lines 203-251, from `/// Asks first when deleting would close terminals…` through the end of `delete_worktree`).
- Modify `packages/desktop/crates/pocket/src/desktop/chrome.rs` — `Confirm` (lines 54-65), imports (lines 1-9).
- Modify `packages/desktop/crates/pocket/src/modals/confirm.rs` — `ConfirmText` (20-93), `confirm_view` (96-126), `confirmed` (136-154), tests (157-243).
- Modify `packages/desktop/crates/pocket/src/desktop.rs` — import (after `use crate::panels::Panels;`, line 29), field after `creates` (line 55), `Desktop::new` (after `creates: Creates::default(),`, line 137).
- Modify `packages/desktop/crates/ui/src/ui.rs` — `setting_up` (lines 426-437).
- Modify `packages/desktop/crates/pocket/src/sidebar.rs` — `row_mark` (line 23), worktree row mark (lines 235-239).
- Modify `packages/desktop/crates/pocket/src/sidebar/row_menu.rs` — `RowMenu::Tree` call (line 122).
- Modify `docs/adr/0003-desktop-code-layout.md` — add a row to the `pocket` module map.

**Context**
- Today's flow (`desktop/project.rs` 203-251): `ask_delete_worktree` counts dirty files in a background task and confirms when dirty or terminals open, else deletes; `delete_worktree` runs `git::remove_worktree` in the background and on success closes the tree's terminals, drops its workspace and saved layout, `save_soon`, clears `worktree`/`session` if shown, then `refresh_git`. Keep that cleanup verbatim as `forget_tree`.
- `impl Desktop` stays thin (ADR 0003): gather inputs, call `run`/`needs_confirm`/`Removal::lost`, set state, `cx.notify()`.
- `Removals` is the feature's state, composed into `Desktop` like `Creates` (`creates: Creates::default()`); it has no subscriptions.
- `Confirm` derives `Clone`, so `Removal` must too (it does). `Confirm::DeleteWorktree` now carries the `Removal` plus `dirty` and `lost`; `Confirm::TeardownFailed { removal, tail }` is new. `confirmed()` calls `close_overlay` after every arm, which is fine: the teardown failure arrives later, from the background task.
- `ConfirmText` gains `lost: usize`; every other constructor sets `lost: 0`. The warning joins both losses: `"2 uncommitted files and 3 commits on no other branch will be lost."`; the existing one-loss sentences are unchanged.
- A detached tree no longer reads "Keeps the branch detached" in the confirm; it names no branch.
- `ui::setting_up` becomes `ui::busy(id, label)` so the row can say "Deleting…"; its only caller is `sidebar.rs` line 23.
- `git::read(tree)` returning `None` (folder gone or unreadable) still always confirms, as today: `git worktree remove --force` would delete whatever is there.
- gpui lays out `\n` in text as line breaks, so the teardown tail renders as-is in a mono block.

**Step 1: failing tests** — in `modals/confirm.rs` `mod tests`:

1. Change the import line to:
   ```rust
       use super::{Busy, ConfirmText, FileStat};
       use crate::removal::Removal;
   ```
2. Change the `text` helper to set `lost: 0`:
   ```rust
       fn text(title: &str, action: &'static str, facts: &[&str], dirty: usize) -> ConfirmText {
           ConfirmText { title: title.into(), action, facts: facts.iter().map(|f| f.to_string()).collect(), dirty, lost: 0, danger: true }
       }
   ```
3. Replace `deleting_a_worktree_keeps_its_branch_and_warns_about_uncommitted_files` with:
   ```rust
       fn removal(branch: Option<&str>, delete_branch: bool) -> Removal {
           Removal { project: "/src/app".into(), tree: "/src/feat".into(), branch: branch.map(String::from), delete_branch, teardown: true }
       }

       #[test]
       fn deleting_a_worktree_keeps_its_branch_and_warns_about_uncommitted_files() {
           let got = ConfirmText::delete_worktree(&removal(Some("feat-x"), false), 2, 0, 1);
           assert_eq!(got, text("Delete feat?", "Delete", &["Closes 1 terminal", "Deletes the folder /src/feat", "Keeps the branch feat-x"], 2));
           let warnings = [0, 1, 2].map(|dirty| ConfirmText::delete_worktree(&removal(Some("feat-x"), false), dirty, 0, 0).warning());
           assert_eq!(warnings, [None, Some("1 uncommitted file will be lost.".into()), Some("2 uncommitted files will be lost.".into())]);
       }

       #[test]
       fn deleting_the_branch_too_says_so_and_warns_about_commits_no_other_branch_has() {
           let got = ConfirmText::delete_worktree(&removal(Some("feat-x"), true), 0, 3, 0);
           assert_eq!(got.facts, ["Deletes the folder /src/feat", "Deletes the branch feat-x"]);
           assert_eq!(got.warning(), Some("3 commits on no other branch will be lost.".into()));
           let both = ConfirmText::delete_worktree(&removal(Some("feat-x"), true), 1, 1, 0).warning();
           assert_eq!(both, Some("1 uncommitted file and 1 commit on no other branch will be lost.".into()));
       }

       #[test]
       fn a_detached_worktree_names_no_branch() {
           assert_eq!(ConfirmText::delete_worktree(&removal(None, false), 0, 2, 0).facts, ["Deletes the folder /src/feat"]);
       }

       #[test]
       fn a_failed_teardown_offers_to_delete_anyway() {
           let got = ConfirmText::teardown_failed("/src/feat");
           assert_eq!(got, text("Couldn't tear down feat", "Delete anyway", &["Its teardown script failed", "Deleting anyway skips it"], 0));
       }
   ```
4. In `paste_confirm_counts_lines`, the literal becomes `ConfirmText { title: "Paste 1 line into zsh?".into(), action: "Paste", facts: vec![], dirty: 0, lost: 0, danger: false }`.

**Step 2: run, expect failure**

```sh
cd packages/desktop && cargo test -p pocket confirm::
```

Expected: compile errors — `E0560 struct ConfirmText has no field named lost`, `E0061` wrong argument count/types for `ConfirmText::delete_worktree`, `E0599 no function teardown_failed`.

**Step 3: implement**

`ui/src/ui.rs` — replace `setting_up` (426-437) with:

```rust
pub fn busy(id: impl Into<ElementId>, label: &'static str) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(5.))
        .text_size(px(12.))
        .font_weight(FontWeight::NORMAL)
        .text_color(TEXT_4)
        .child(spinner(id, 11., TEXT_4))
        .child(label)
}
```

`removal.rs` — add to the top imports:

```rust
use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay};
use gpui_kit::*;
use std::collections::HashSet;
```

and after `const TEARDOWN_LIMIT…`:

```rust
/// Worktrees being deleted: their rows say so, and a second delete of one is ignored.
#[derive(Default)]
pub(crate) struct Removals {
    running: HashSet<String>,
}

impl Removals {
    pub(crate) fn running(&self, tree: &str) -> bool {
        self.running.contains(tree)
    }
}
```

and after `fn run` (before `#[cfg(test)]`):

```rust
impl Desktop {
    /// Asks first when deleting would close terminals or lose uncommitted changes or commits.
    pub(crate) fn ask_delete_worktree(&mut self, project: String, tree: String, delete_branch: bool, cx: &mut Context<Self>) {
        if self.removals.running(&tree) {
            return;
        }
        let branch = self.worktrees.get(&project).into_iter().flatten().find(|w| w.path == tree).map(|w| w.branch.clone()).filter(|b| !b.is_empty() && b != "detached");
        let removal = Removal { project, tree, branch, delete_branch, teardown: true };
        let job = removal.clone();
        let task = cx.background_executor().spawn(async move { (git::read(&job.tree).map(|r| r.files.len()), job.lost()) });
        cx.spawn(async move |this, cx| {
            let (dirty, lost) = task.await;
            this.update(cx, |d, cx| {
                // An unreadable tree may still hold work: ask.
                if dirty.is_none_or(|n| needs_confirm(n, d.tree_terminals(&removal.tree).len(), lost)) {
                    d.confirm = Some(Confirm::DeleteWorktree { removal, dirty: dirty.unwrap_or(0), lost });
                    d.overlay = Some(Overlay::Confirm);
                } else {
                    d.delete_worktree(removal, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn delete_worktree(&mut self, removal: Removal, cx: &mut Context<Self>) {
        if !self.removals.running.insert(removal.tree.clone()) {
            return;
        }
        let teardown = self.store.repos.get(&removal.project).map(|r| r.teardown.clone()).unwrap_or_default();
        let job = removal.clone();
        let task = cx.background_executor().spawn(async move { run(&job, &teardown) });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |d, cx| {
                d.removals.running.remove(&removal.tree);
                match res {
                    Err(Failure::Teardown(tail)) => {
                        d.confirm = Some(Confirm::TeardownFailed { removal, tail });
                        d.overlay = Some(Overlay::Confirm);
                    }
                    Err(Failure::Remove(e)) => d.error = Some(e),
                    Err(Failure::Branch(branch, e)) => {
                        d.error = Some(format!("Deleted the worktree; kept branch {branch}: {e}"));
                        d.forget_tree(&removal.tree, cx);
                    }
                    Ok(()) => d.forget_tree(&removal.tree, cx),
                }
                d.refresh_git(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn forget_tree(&mut self, tree: &str, cx: &mut Context<Self>) {
        for id in self.tree_terminals(tree) {
            self.close_pane(&id, cx);
        }
        self.workspaces.remove(tree);
        self.store.layouts.remove(tree);
        self.save_soon(cx);
        if self.worktree.as_deref() == Some(tree) {
            (self.worktree, self.session) = (None, None);
        }
    }
}
```

`desktop/project.rs` — delete lines 203-251 (`ask_delete_worktree` and `delete_worktree` with their doc comments). `Confirm` and `Overlay` stay imported (used by `ask_remove_project`).

`desktop/chrome.rs` — add `use crate::removal::Removal;` after `use crate::desktop::Desktop;`, and in `Confirm` replace the `DeleteWorktree` variant with:

```rust
    DeleteWorktree { removal: Removal, dirty: usize, lost: usize },
    TeardownFailed { removal: Removal, tail: String },
```

`desktop.rs`:
- add `use crate::removal::Removals;` after `use crate::panels::Panels;`
- in `pub struct Desktop`, after `pub(crate) creates: Creates,` add `pub(crate) removals: Removals,`
- in `Desktop::new`, after `creates: Creates::default(),` add `removals: Removals::default(),`

`modals/confirm.rs`:
- imports: add `use crate::removal::Removal;` after `use crate::desktop::chrome::Confirm;`
- `ConfirmText` struct: after `dirty: usize,` add `lost: usize,`
- every constructor that has `dirty: 0, danger` gets `dirty: 0, lost: 0, danger` (8 places: `remove_project`, `discard`, `close_terminals`, `paste`, `close_session`, `close_file`, `quit`, `open_external`). On macOS: `sed -i '' 's/dirty: 0, danger/dirty: 0, lost: 0, danger/g' packages/desktop/crates/pocket/src/modals/confirm.rs` (also fixes the `paste` test literal from Step 1.4 if not done).
- replace `fn delete_worktree(…)` with:
  ```rust
      fn delete_worktree(r: &Removal, dirty: usize, lost: usize, terminals: usize) -> Self {
          let branch = r.branch.as_ref().map(|b| if r.delete_branch { format!("Deletes the branch {b}") } else { format!("Keeps the branch {b}") });
          let facts = closes(terminals).into_iter().chain([format!("Deletes the folder {}", tilde(&r.tree))]).chain(branch).collect();
          Self { title: format!("Delete {}?", basename(&r.tree)), action: "Delete", facts, dirty, lost, danger: true }
      }

      fn teardown_failed(tree: &str) -> Self {
          let facts = vec!["Its teardown script failed".into(), "Deleting anyway skips it".into()];
          Self { title: format!("Couldn't tear down {}", basename(tree)), action: "Delete anyway", facts, dirty: 0, lost: 0, danger: true }
      }
  ```
- replace `fn warning(&self)` with:
  ```rust
      fn warning(&self) -> Option<String> {
          let count = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
          let files = (self.dirty > 0).then(|| count(self.dirty, "uncommitted file", "uncommitted files"));
          let commits = (self.lost > 0).then(|| count(self.lost, "commit", "commits") + " on no other branch");
          let lost: Vec<String> = files.into_iter().chain(commits).collect();
          (!lost.is_empty()).then(|| format!("{} will be lost.", lost.join(" and ")))
      }
  ```
- `confirm_view`: replace the `DeleteWorktree` arm with these two arms:
  ```rust
              Some(Confirm::DeleteWorktree { removal, dirty, lost }) => ConfirmText::delete_worktree(removal, *dirty, *lost, self.tree_terminals(&removal.tree).len()),
              Some(Confirm::TeardownFailed { removal, .. }) => ConfirmText::teardown_failed(&removal.tree),
  ```
  and after the `if let Some(warning) = text.warning() { … }` block add:
  ```rust
          if let Some(Confirm::TeardownFailed { tail, .. }) = &self.confirm {
              body.push(div().w_full().px(px(10.)).py(px(8.)).rounded(px(8.)).bg(FILL_2).text_left().font_family(MONO).text_size(px(11.5)).text_color(TEXT_2).child(tail.clone()).into_any_element());
          }
  ```
- `confirmed`: replace the `DeleteWorktree` arm with:
  ```rust
              Some(Confirm::DeleteWorktree { removal, .. }) => self.delete_worktree(removal, cx),
              Some(Confirm::TeardownFailed { removal, .. }) => self.delete_worktree(Removal { teardown: false, ..removal }, cx),
  ```

`sidebar.rs`:
- `row_mark` line 23: `Some(ui::busy(id(format!("aside-setup:{key}")), "Setting up…").into_any_element())`
- worktree row mark (lines 235-239) becomes:
  ```rust
                  let mark = if self.removals.running(tree) {
                      Some(ui::busy(id(format!("aside-deleting:{tree}")), "Deleting…").into_any_element())
                  } else if self.creates.failed(tree) {
                      ui::indicator(id(format!("aside-failed:{tree}")), Some(ui::State::Failed))
                  } else {
                      row_mark(tree, self::setting_up(&setups, tree), &self.tree_cards(p, tree))
                  };
  ```

`sidebar/row_menu.rs` line 122: `this.ask_delete_worktree(project.clone(), tree.clone(), false, cx);`

`docs/adr/0003-desktop-code-layout.md` — in the `pocket` module table, after the `sidebar` row add:

```
| `removal` | deleting a worktree: its teardown script, its branch, the confirm and the row's "Deleting…" |
```

**Step 4: run, expect pass**

```sh
cd packages/desktop && cargo test -p pocket confirm:: && cargo test -p pocket removal:: && cargo clippy -p pocket --all-targets
```

Expected: confirm and removal tests pass; the Task 3.4 dead-code warnings are gone; clippy shows no new warnings.

---

### Task 3.6: "Delete worktree and branch…" in the row menu

**Files**
- Modify `packages/desktop/crates/pocket/src/removal.rs` — add `branch_deletable` and a test.
- Modify `packages/desktop/crates/pocket/src/sidebar/row_menu.rs` — `RowMenu::Tree` arm (lines 118-125), imports (lines 1-7).

**Context**
- Offer the item only when the tree is on a branch that isn't the project's saved base (`store.repos[project].base`, may be empty), `main` or `master`. Detached trees have `git::Worktree.branch == "detached"`.
- Keep the existing `danger_row("aside-menu-delete", "trash", "Delete worktree…")` first and unchanged, so a clean delete stays instant; the new item goes after it.
- `row_menu_items` borrows `&self`; compute a `bool` before building listeners so nothing borrowed from `self` moves into closures.

**Step 1: failing test** — in `removal.rs` `mod tests`, change the `use super::{…}` line to also import `branch_deletable`:

```rust
    use super::{Failure, Removal, branch_deletable, needs_confirm, run};
```

and add:

```rust
    #[test]
    fn only_a_branch_the_project_does_not_build_on_can_go_with_its_worktree() {
        assert!(branch_deletable("feat", "dev"));
        for kept in ["dev", "main", "master", "detached", ""] {
            assert!(!branch_deletable(kept, "dev"), "{kept}");
        }
    }
```

**Step 2: run, expect failure**

```sh
cd packages/desktop && cargo test -p pocket removal::
```

Expected: `E0432 unresolved import super::branch_deletable`.

**Step 3: implement**

`removal.rs`, after `fn needs_confirm`:

```rust
/// Whether "Delete worktree and branch" may take `branch`: not a detached tree, nor a branch the project builds on.
pub(crate) fn branch_deletable(branch: &str, base: &str) -> bool {
    !["", "detached", "main", "master", base].contains(&branch)
}
```

`row_menu.rs` — add `use crate::removal::branch_deletable;` after `use crate::desktop::chrome::{…};`, and replace the `RowMenu::Tree { project, tree } => vec![ … ],` arm with:

```rust
            RowMenu::Tree { project, tree } => {
                let base = self.store.repos.get(&project).map_or("", |r| r.base.as_str());
                let with_branch = self.worktrees.get(&project).into_iter().flatten().any(|w| w.path == tree && branch_deletable(&w.branch, base));
                let (p, t) = (project.clone(), tree.clone());
                let mut rows = vec![
                    ui::danger_row("aside-menu-delete", "trash", "Delete worktree…")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.row_menu = None;
                            this.ask_delete_worktree(p.clone(), t.clone(), false, cx);
                        }))
                        .into_any_element(),
                ];
                if with_branch {
                    rows.push(
                        ui::danger_row("aside-menu-delete-branch", "trash", "Delete worktree and branch…")
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.row_menu = None;
                                this.ask_delete_worktree(project.clone(), tree.clone(), true, cx);
                            }))
                            .into_any_element(),
                    );
                }
                rows
            }
```

**Step 4: run, expect pass**

```sh
cd packages/desktop && cargo test -p pocket removal:: && cargo clippy -p pocket --all-targets
```

Expected: 7 removal tests pass; no new clippy warnings.

---

### PR 3 finish

```sh
cd packages/desktop && cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace
```

All pass with no new warnings.

UI capture (view changes: settings field, row menu, "Deleting…", confirm): the "before" set was taken in Task 3.0. After the PR, from the repo root:

```sh
env -u POCKETD_SOCK .ui-review/fixture/capture.sh /tmp/pr3-after add-repo=add-repo repositories-worktrees=
```

Compare with `/tmp/pr3-before`: `add-repo` gains the "When a worktree is deleted" field under "When a worktree is created"; `repositories-worktrees` rows are unchanged at rest. There is no capture step for an open row menu, a running delete or the confirm; check those by hand in a release build: right-click a worktree on a feature branch (both items), on `main`/detached (one item); delete a dirty tree (confirm with warning); set teardown `sleep 3; false` (row shows "Deleting…", then "Couldn't tear down …" with the output and "Delete anyway"); delete a branch that's already gone (toast "Deleted the worktree; kept branch …").

---

## PR 4: Show PR status and CI on worktrees

**Scope:** Every non-main, non-detached worktree shows its PR as a `#123` chip, toned by state and CI, in the sidebar row and in the Changes header. A tooltip names the state and counts the checks. The Changes `⋯` menu gains "Open pull request". When `gh` is missing or logged out, it shows a one-line hint instead. Data comes from `gh pr view --json …`, run through `daemon::run_login`. Only one `gh` runs at a time: the tree on screen is refreshed every 30s, the others every 5 minutes. Polling stops for the session once gh is missing or logged out. A gh that hangs gives up its turn after a minute.

**Depends on:** nothing.

**Done when:**
- `cargo test -p git github` and `cargo test -p pocket pull_requests` pass.
- Build, clippy (no new warnings) and test pass for the whole workspace.
- Captures show the chips in the sidebar and the Changes header.
- Nothing else changes on screen when no PR data is present.

All commands run from `packages/desktop`.

### Task 4.1: Parse `gh pr view` in the git crate

**Files:**
- Create `packages/desktop/crates/git/src/github.rs`.
- Modify `packages/desktop/crates/git/src/git.rs`, line 7 (`pub mod graph;`).
- Modify `packages/desktop/crates/git/Cargo.toml`, line 11 (`[dependencies]` block).

**Context:**
- The crate root is `src/git.rs`, so `pub mod github;` resolves to `src/github.rs`, a sibling of `src/graph.rs`. It is **not** `src/git/github.rs`.
- The git crate has no GPUI and no serde yet. `serde_json` is already a workspace dependency (`packages/desktop/Cargo.toml`).
- `daemon::run_login(argv, cwd, input)` returns:
  - `Ok(trimmed stdout)` on success.
  - `Err(trimmed stderr, or stdout if stderr is empty)` on failure.
- It runs through the login shell (`$SHELL -l -c`), so a missing `gh` surfaces as the shell's own message:
  - zsh: `zsh:1: command not found: gh`
  - bash: `bash: line 1: gh: command not found`
  - fish: `fish: Unknown command: gh`
- A logged-out `gh` prints `To get started with GitHub CLI, please run:  gh auth login …`.
- A branch with no PR prints `no pull requests found for branch "…"`. That is `Ok(None)`, not a problem.
- Real JSON from `gh pr view --json number,title,url,state,isDraft,statusCheckRollup`:
  - `state` is `OPEN`, `MERGED` or `CLOSED`.
  - `statusCheckRollup` mixes CheckRun entries (`__typename: "CheckRun"`, `status`, `conclusion`) with StatusContext entries (`__typename: "StatusContext"`, `state`).
- Mimic `git.rs`: plain functions, tests in `#[cfg(test)] mod tests` with `use super::*;`, named as sentences.

**Step 1: Write the failing test.**

In `crates/git/Cargo.toml`, make `[dependencies]` read:

```toml
[dependencies]
imara-diff.workspace = true
serde_json.workspace = true
```

In `crates/git/src/git.rs`, change line 7 from `pub mod graph;` to:

```rust
pub mod github;
pub mod graph;
```

Create `crates/git/src/github.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const OPEN: &str = r#"{"isDraft":false,"number":42,"state":"OPEN","title":"Fix the flaky snapshot","url":"https://github.com/acme/app/pull/42","statusCheckRollup":[
        {"__typename":"CheckRun","name":"build","status":"COMPLETED","conclusion":"SUCCESS"},
        {"__typename":"CheckRun","name":"label","status":"COMPLETED","conclusion":"SKIPPED"},
        {"__typename":"CheckRun","name":"test","status":"COMPLETED","conclusion":"FAILURE"},
        {"__typename":"CheckRun","name":"e2e","status":"COMPLETED","conclusion":"TIMED_OUT"},
        {"__typename":"CheckRun","name":"lint","status":"IN_PROGRESS","conclusion":""},
        {"__typename":"StatusContext","context":"ci/circleci","state":"PENDING"},
        {"__typename":"StatusContext","context":"deploy","state":"ERROR"},
        {"__typename":"StatusContext","context":"vercel","state":"SUCCESS"}
    ]}"#;

    #[test]
    fn an_open_pr_counts_its_checks_by_outcome() {
        let pr = Pr {
            number: 42,
            title: "Fix the flaky snapshot".into(),
            url: "https://github.com/acme/app/pull/42".into(),
            state: PrState::Open,
            draft: false,
            checks: Checks { passed: 3, failed: 3, pending: 2 },
        };
        assert_eq!(view_result(Ok(OPEN.into())), Ok(Some(pr)));
    }

    #[test]
    fn merged_closed_and_draft_prs_keep_their_state() {
        let pr = |json: &str| view_result(Ok(json.into())).unwrap().unwrap();
        let merged = pr(r#"{"isDraft":false,"number":7,"state":"MERGED","title":"t","url":"u","statusCheckRollup":[]}"#);
        let closed = pr(r#"{"isDraft":false,"number":8,"state":"CLOSED","title":"t","url":"u","statusCheckRollup":[]}"#);
        let draft = pr(r#"{"isDraft":true,"number":9,"state":"OPEN","title":"t","url":"u","statusCheckRollup":[]}"#);
        assert_eq!((merged.state, closed.state, draft.state, draft.draft), (PrState::Merged, PrState::Closed, PrState::Open, true));
        assert_eq!(merged.checks, Checks::default());
    }

    #[test]
    fn a_missing_gh_differs_from_a_logged_out_one() {
        for missing in ["zsh:1: command not found: gh", "bash: line 1: gh: command not found", "fish: Unknown command: gh"] {
            assert_eq!(view_result(Err(missing.into())), Err(GhProblem::Missing), "{missing}");
        }
        let logged_out = "To get started with GitHub CLI, please run:  gh auth login\nAlternatively, populate the GH_TOKEN environment variable with a GitHub API authentication token.";
        assert_eq!(view_result(Err(logged_out.into())), Err(GhProblem::LoggedOut));
    }

    #[test]
    fn a_branch_without_a_pr_or_a_failing_gh_has_none() {
        assert_eq!(view_result(Err("no pull requests found for branch \"fix/restore-handoff\"".into())), Ok(None));
        assert_eq!(view_result(Err("fatal: not a git repository (or any of the parent directories): .git".into())), Ok(None));
        assert_eq!(view_result(Ok("not json".into())), Ok(None));
    }
}
```

**Step 2: Run it.** `cd packages/desktop && cargo test -p git github`

Expected: fails to compile, with errors like `cannot find struct, variant or union type 'Pr' in this scope` and `cannot find function 'view_result' in this scope`.

**Step 3: Implement.** Put this above the `#[cfg(test)]` block in `crates/git/src/github.rs`:

```rust
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct Pr {
    pub number: u32,
    pub title: String,
    pub url: String,
    pub state: PrState,
    pub draft: bool,
    pub checks: Checks,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PrState {
    Open,
    Merged,
    Closed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Checks {
    pub passed: u16,
    pub failed: u16,
    pub pending: u16,
}

/// Why `gh` can't be asked at all, as opposed to a branch having no PR.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GhProblem {
    Missing,
    LoggedOut,
}

/// The PR for the branch checked out in the working directory.
pub const VIEW: &[&str] = &["gh", "pr", "view", "--json", "number,title,url,state,isDraft,statusCheckRollup"];

/// Reads what [`VIEW`] returned through `daemon::run_login`. No PR, or any other failure, is `Ok(None)`.
pub fn view_result(out: Result<String, String>) -> Result<Option<Pr>, GhProblem> {
    match out {
        Ok(json) => Ok(parse(&json)),
        // The login shell reports a missing command: zsh and bash say "command not found", fish "Unknown command".
        Err(e) if e.contains("command not found") || e.contains("Unknown command") => Err(GhProblem::Missing),
        Err(e) if e.contains("gh auth login") => Err(GhProblem::LoggedOut),
        Err(_) => Ok(None),
    }
}

fn parse(json: &str) -> Option<Pr> {
    let v: Value = serde_json::from_str(json).ok()?;
    let state = match v["state"].as_str()? {
        "MERGED" => PrState::Merged,
        "CLOSED" => PrState::Closed,
        _ => PrState::Open,
    };
    Some(Pr {
        number: u32::try_from(v["number"].as_u64()?).ok()?,
        title: v["title"].as_str()?.to_string(),
        url: v["url"].as_str()?.to_string(),
        state,
        draft: v["isDraft"].as_bool().unwrap_or(false),
        checks: checks(&v["statusCheckRollup"]),
    })
}

fn checks(rollup: &Value) -> Checks {
    let mut c = Checks::default();
    for check in rollup.as_array().into_iter().flatten() {
        let field = |k: &str| check[k].as_str().unwrap_or("");
        // Commit statuses carry `state`; check runs carry `status` and `conclusion`.
        let (failed, pending) = if field("__typename") == "StatusContext" {
            (matches!(field("state"), "FAILURE" | "ERROR"), matches!(field("state"), "PENDING" | "EXPECTED"))
        } else {
            let failed = matches!(field("conclusion"), "FAILURE" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED" | "STARTUP_FAILURE");
            (failed, field("status") != "COMPLETED")
        };
        if failed {
            c.failed += 1;
        } else if pending {
            c.pending += 1;
        } else {
            c.passed += 1;
        }
    }
    c
}
```

**Step 4: Run it.** `cd packages/desktop && cargo test -p git github`

Expected: `test result: ok. 4 passed` (the 4 `github::tests::*` tests).

### Task 4.2: Poll `gh` for each worktree, one at a time

**Files:**
- Create `packages/desktop/crates/pocket/src/git_ui/pull_requests.rs`.
- Modify `packages/desktop/crates/pocket/src/git_ui.rs`, lines 1–4.
- Modify `packages/desktop/crates/pocket/src/desktop.rs`:
  - imports, line 24 (`use crate::git_ui::graph::GraphState;`)
  - struct field, line 69 (`pub(crate) changes: ChangesState,`)
  - init in `new()`, line 150 (`changes,`)
- Modify `packages/desktop/crates/pocket/src/desktop/project.rs`, `refresh_git` callback, line 135.

**Context:**
- Per CLAUDE.md, a feature gets its own module and state struct.
- Decisions go in plain methods (`due`, `apply`), testable without GPUI. The `impl Desktop` part only gathers inputs, spawns and notifies.
- Mimic `refresh_git` in `desktop/project.rs` (lines 93–150): background spawn, then `cx.spawn` + `this.update`, and `cx.notify()` only when something changed.
- `refresh_git` already runs every 2s (from `main.rs`). Hooking the poll into its callback after `d.worktrees` is assigned (line 135) gives fresh worktree lists and a natural 2s tick. At most one `gh` is in flight, guarded by `running`.
- `self.worktrees: HashMap<String /*project*/, Vec<git::Worktree { path, branch, main }>>`.
  - Detached trees have `branch == "detached"`. They and main trees have no PR to ask about.
- `self.cwd()` is the worktree on screen (`desktop/project.rs:57`).
- `Option::is_none_or` is available (rustc 1.98, already used in the codebase).
- No test runs real `gh`. Only `due` and `apply` are tested.

**Step 1: Write the failing test.**

In `crates/pocket/src/git_ui.rs`, make the file read:

```rust
pub(crate) mod commit;
pub(crate) mod changes;
pub(crate) mod diff;
pub(crate) mod graph;
pub(crate) mod pull_requests;
```

Create `crates/pocket/src/git_ui/pull_requests.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use git::github::{Checks, PrState};

    fn pr(n: u32) -> Pr {
        Pr { number: n, title: format!("PR {n}"), url: format!("https://github.com/acme/app/pull/{n}"), state: PrState::Open, draft: false, checks: Checks::default() }
    }

    fn trees() -> Vec<String> {
        vec!["/a".into(), "/b".into(), "/c".into()]
    }

    #[test]
    fn the_tree_on_screen_is_asked_about_first_then_the_rest_in_order() {
        let (mut prs, now) = (PullRequests::default(), Instant::now());
        assert_eq!(prs.due(Some("/b"), &trees(), now).as_deref(), Some("/b"));
        prs.apply("/b".into(), Ok(None), now);
        assert_eq!(prs.due(Some("/b"), &trees(), now).as_deref(), Some("/a"));
        prs.apply("/a".into(), Ok(Some(pr(1))), now);
        assert_eq!(prs.due(Some("/b"), &trees(), now).as_deref(), Some("/c"));
        prs.apply("/c".into(), Ok(None), now);
        assert_eq!(prs.due(Some("/b"), &trees(), now), None);
        assert_eq!(prs.due(Some("/main"), &trees(), now), None);
    }

    #[test]
    fn gh_runs_one_at_a_time_a_hung_one_gives_up_its_turn_and_it_stops_once_missing_or_logged_out() {
        let (mut prs, now) = (PullRequests::default(), Instant::now());
        prs.running = Some(("/a".into(), now));
        assert_eq!(prs.due(None, &trees(), now), None);
        assert_eq!(prs.due(None, &trees(), now + Duration::from_secs(60)).as_deref(), Some("/a"));
        prs.apply("/a".into(), Err(GhProblem::LoggedOut), now);
        assert!(prs.running.is_none());
        assert_eq!(prs.due(None, &trees(), now + Duration::from_secs(3600)), None);
    }

    #[test]
    fn the_tree_on_screen_is_asked_again_after_30s_and_the_others_after_5_minutes() {
        let (mut prs, t0) = (PullRequests::default(), Instant::now());
        for t in trees() {
            prs.apply(t, Ok(None), t0);
        }
        let at = |s| t0 + Duration::from_secs(s);
        assert_eq!(prs.due(Some("/b"), &trees(), at(29)), None);
        assert_eq!(prs.due(Some("/b"), &trees(), at(30)).as_deref(), Some("/b"));
        prs.apply("/b".into(), Ok(None), at(290));
        assert_eq!(prs.due(Some("/b"), &trees(), at(299)), None);
        assert_eq!(prs.due(Some("/b"), &trees(), at(300)).as_deref(), Some("/a"));
    }

    #[test]
    fn applying_the_same_pr_again_changes_nothing() {
        let (mut prs, now) = (PullRequests::default(), Instant::now());
        assert!(prs.apply("/a".into(), Ok(None), now));
        assert!(!prs.apply("/a".into(), Ok(None), now));
        assert!(prs.apply("/a".into(), Ok(Some(pr(1))), now));
        assert!(!prs.apply("/a".into(), Ok(Some(pr(1))), now));
        let failing = Pr { checks: Checks { failed: 1, ..Checks::default() }, ..pr(1) };
        assert!(prs.apply("/a".into(), Ok(Some(failing)), now));
        assert!(prs.apply("/a".into(), Err(GhProblem::Missing), now));
        assert!(!prs.apply("/a".into(), Err(GhProblem::Missing), now));
    }
}
```

**Step 2: Run it.** `cd packages/desktop && cargo test -p pocket pull_requests`

Expected: fails to compile, with `cannot find type 'Pr' in this scope`, `failed to resolve: use of undeclared type 'PullRequests'`, and similar for `Instant`, `Duration` and `GhProblem`.

**Step 3: Implement.** Put this above the `#[cfg(test)]` block in `pull_requests.rs`:

```rust
use crate::desktop::Desktop;
use git::github::{self, GhProblem, Pr};
use gpui_kit::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};

const SELECTED_EVERY: Duration = Duration::from_secs(30);
const OTHERS_EVERY: Duration = Duration::from_secs(5 * 60);
const GH_TURN: Duration = Duration::from_secs(60);

/// Each worktree's PR as `gh` last reported it, and when it was asked.
#[derive(Default)]
pub(crate) struct PullRequests {
    by_tree: HashMap<String, (Instant, Option<Pr>)>,
    /// The tree `gh` is asked about, and since when.
    running: Option<(String, Instant)>,
    problem: Option<GhProblem>,
}

impl PullRequests {
    /// The tree to ask `gh` about next: the one on screen every 30s, the others every 5 minutes, one at a time.
    fn due(&self, selected: Option<&str>, trees: &[String], now: Instant) -> Option<String> {
        // A gh that hangs gives up its turn after a minute.
        if self.running.as_ref().is_some_and(|(_, at)| now.duration_since(*at) < GH_TURN) || self.problem.is_some() {
            return None;
        }
        let stale = |t: &str, every: Duration| self.by_tree.get(t).is_none_or(|(at, _)| now.duration_since(*at) >= every);
        selected
            .filter(|s| trees.iter().any(|t| t == s) && stale(s, SELECTED_EVERY))
            .map(str::to_string)
            .or_else(|| trees.iter().find(|t| stale(t, OTHERS_EVERY)).cloned())
    }

    /// Records what `gh` said about `tree`. True when it changes what's shown.
    pub(crate) fn apply(&mut self, tree: String, result: Result<Option<Pr>, GhProblem>, now: Instant) -> bool {
        if self.running.as_ref().is_some_and(|(t, _)| *t == tree) {
            self.running = None;
        }
        match result {
            Err(p) => {
                let changed = self.problem != Some(p);
                self.problem = Some(p);
                changed
            }
            Ok(pr) => {
                let changed = self.by_tree.get(&tree).is_none_or(|(_, old)| *old != pr);
                self.by_tree.insert(tree, (now, pr));
                changed
            }
        }
    }
}

impl Desktop {
    /// Asks `gh` about the next due worktree, off the UI thread.
    pub(crate) fn poll_prs(&mut self, cx: &mut Context<Self>) {
        let mut trees: Vec<String> = self.project.as_ref().and_then(|p| self.worktrees.get(p)).into_iter().flatten().filter(|w| !w.main && w.branch != "detached").map(|w| w.path.clone()).collect();
        trees.sort();
        let Some(tree) = self.prs.due(self.cwd().as_deref(), &trees, Instant::now()) else { return };
        self.prs.running = Some((tree.clone(), Instant::now()));
        let dir = tree.clone();
        let task = cx.background_executor().spawn(async move { github::view_result(daemon::run_login(github::VIEW, &dir, "")) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |d, cx| {
                if d.prs.apply(tree, result, Instant::now()) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }
}
```

Wire it in. In `crates/pocket/src/desktop.rs`:

1. After line 24 (`use crate::git_ui::graph::GraphState;`), add:
   ```rust
   use crate::git_ui::pull_requests::PullRequests;
   ```
2. After line 69 (`pub(crate) changes: ChangesState,`), add:
   ```rust
       pub(crate) prs: PullRequests,
   ```
3. In `new()`, after line 150 (`changes,`), add:
   ```rust
               prs: PullRequests::default(),
   ```

In `crates/pocket/src/desktop/project.rs`, `refresh_git`'s callback, after line 135 (`(d.repos, d.explorer.tree, d.initials, d.worktrees) = (repos, tree, initials, worktrees);`), add:

```rust
                d.poll_prs(cx);
```

**Step 4: Run it.** `cd packages/desktop && cargo test -p pocket pull_requests`

Expected: `test result: ok. 4 passed`. `cargo build -p pocket` builds with no new warnings.

### Task 4.3: PR chip on sidebar worktree rows

**Files:**
- Modify `packages/desktop/crates/theme/src/theme.rs`, after line 110 (`pub const SUCCESS_BG …`).
- Modify `packages/desktop/crates/pocket/src/git_ui/pull_requests.rs`, created in 4.2.
- Modify `packages/desktop/crates/pocket/src/sidebar.rs`:
  - imports, lines 9–19
  - worktree row, lines 242–243
- Modify `packages/desktop/crates/pocket/src/capture.rs`:
  - imports, lines 1–13
  - `STEPS`, line 17 and line 91

**Context:**
- The theme has no purple, so merged PRs need a new `MERGED` token (GitHub's merged purple).
- `ui` has no generic tooltip helper. Mimic `ContextCard` in `crates/pocket/src/terminal_view/context.rs` (lines 35–53, 91–92): a small `Render` struct in `ui::pop(div())`, attached with `.tooltip(move |_, cx| cx.new(|_| …).into()).tooltip_show_delay(Duration::from_millis(350))`. Its tests compare `Token`s with `assert_eq!` (`Token` is `Copy + Debug + PartialEq`).
- `ui::worktree_row` (ui.rs:943) is a flex row: icon, then a `flex_1` truncated name, with gap 7px. A `flex_none` child placed before `trail` sits right-aligned, between the name and the trail.
- `id(String) -> ElementId` comes from `crate::desktop::chrome`, which is already imported in sidebar.rs.
- Capture mode runs real `gh` under the fixture HOME, which is probably logged out, so the screenshot step seeds PRs through `apply`.
  - Seeded entries are fresh, so `due` skips them.
  - A `LoggedOut` result only sets `problem`; it doesn't clear seeded entries.

**Step 1: Write the failing test.** Append these tests inside `mod tests` in `pull_requests.rs`:

```rust
    #[test]
    fn a_chip_is_toned_by_state_then_by_its_worst_check() {
        let with = |state, draft, failed, pending, passed| Pr { state, draft, checks: Checks { passed, failed, pending }, ..pr(1) };
        let tones = [
            with(PrState::Merged, false, 1, 0, 0),
            with(PrState::Closed, false, 0, 0, 1),
            with(PrState::Open, true, 1, 0, 0),
            with(PrState::Open, false, 1, 1, 1),
            with(PrState::Open, false, 0, 1, 1),
            with(PrState::Open, false, 0, 0, 1),
            with(PrState::Open, false, 0, 0, 0),
        ]
        .map(|p| tone(&p));
        assert_eq!(tones, [MERGED, TEXT_4, TEXT_4, FAILED_TEXT, WAITING_TEXT, SUCCESS_TEXT, TEXT_3]);
    }

    #[test]
    fn a_chips_tooltip_names_the_state_and_counts_the_checks() {
        let open = Pr { checks: Checks { passed: 3, failed: 3, pending: 2 }, ..pr(1) };
        let merged = Pr { state: PrState::Merged, ..pr(2) };
        let draft = Pr { draft: true, checks: Checks { pending: 1, ..Checks::default() }, ..pr(3) };
        assert_eq!([detail(&open), detail(&merged), detail(&draft)], ["3 failed · 2 pending · 3 passed", "Merged · No checks", "Draft · 1 pending"]);
    }
```

The theme tokens reach the tests through `use super::*;` once Step 3 imports `theme::*` in the module.

**Step 2: Run it.** `cd packages/desktop && cargo test -p pocket pull_requests`

Expected: fails to compile, with `cannot find function 'tone' in this scope`, `cannot find function 'detail' in this scope` and `cannot find value 'MERGED' in this scope` (and the other tokens).

**Step 3: Implement.**

In `crates/theme/src/theme.rs`, after line 110 (`pub const SUCCESS_BG: Token = Token::fixed(0x30a46c21);`), add:

```rust
pub const MERGED: Token = Token::new(0x8250dfff, 0xa371f7ff);
```

In `pull_requests.rs`, replace the import block with:

```rust
use crate::desktop::Desktop;
use git::github::{self, GhProblem, Pr, PrState};
use gpui_kit::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use theme::*;
```

Add `get` to `impl PullRequests`:

```rust
    pub(crate) fn get(&self, tree: &str) -> Option<&Pr> {
        self.by_tree.get(tree)?.1.as_ref()
    }
```

Add these below `impl PullRequests`, above `impl Desktop`:

```rust
fn tone(pr: &Pr) -> Token {
    let c = pr.checks;
    match pr.state {
        PrState::Merged => MERGED,
        PrState::Closed => TEXT_4,
        PrState::Open if pr.draft => TEXT_4,
        PrState::Open if c.failed > 0 => FAILED_TEXT,
        PrState::Open if c.pending > 0 => WAITING_TEXT,
        PrState::Open if c.passed > 0 => SUCCESS_TEXT,
        PrState::Open => TEXT_3,
    }
}

fn detail(pr: &Pr) -> String {
    let state = match pr.state {
        PrState::Merged => Some("Merged"),
        PrState::Closed => Some("Closed"),
        PrState::Open => pr.draft.then_some("Draft"),
    };
    let c = pr.checks;
    let counts: Vec<String> = [(c.failed, "failed"), (c.pending, "pending"), (c.passed, "passed")].into_iter().filter(|(n, _)| *n > 0).map(|(n, w)| format!("{n} {w}")).collect();
    let checks = if counts.is_empty() { "No checks".to_string() } else { counts.join(" · ") };
    match state {
        Some(s) => format!("{s} · {checks}"),
        None => checks,
    }
}

struct PrCard {
    title: String,
    line: String,
}

impl Render for PrCard {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        ui::pop(div())
            .p(px(10.))
            .max_w(px(320.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .text_size(px(12.))
            .text_color(TEXT_2)
            .child(div().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(self.title.clone()))
            .child(self.line.clone())
    }
}

/// `#123`, coloured by the PR's state and checks, with its title and checks on hover.
pub(crate) fn chip(id: impl Into<ElementId>, pr: &Pr) -> Stateful<Div> {
    let (title, line) = (format!("#{} {}", pr.number, pr.title), detail(pr));
    div()
        .id(id)
        .flex_none()
        .font_family(MONO)
        .text_size(px(11.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(tone(pr))
        .child(format!("#{}", pr.number))
        .tooltip(move |_, cx| cx.new(|_| PrCard { title: title.clone(), line: line.clone() }).into())
        .tooltip_show_delay(Duration::from_millis(350))
}
```

In `crates/pocket/src/sidebar.rs`, add an import after line 12 (`use crate::status::{self, Card};`):

```rust
use crate::git_ui::pull_requests;
```

Then change lines 242–243 from:

```rust
                    ui::worktree_row(id(format!("aside-tree:{tree}")), basename(tree), current.as_ref() == Some(tree))
                        .child(trail)
```

to:

```rust
                    ui::worktree_row(id(format!("aside-tree:{tree}")), basename(tree), current.as_ref() == Some(tree))
                        .children(self.prs.get(tree).map(|pr| pull_requests::chip(id(format!("aside-pr:{tree}")), pr)))
                        .child(trail)
```

In `crates/pocket/src/capture.rs`:

1. After line 8 (`use git::Kind;`), add:
   ```rust
   use git::github::{Checks, Pr, PrState};
   ```
2. Change line 17 to `const STEPS: [(&str, Step); 29] = [`.
3. After the `("error", …)` entry (line 91), add:

```rust
    ("prs", |d, _, _| {
        let Some(trees) = d.project.as_ref().and_then(|p| d.worktrees.get(p)) else { return };
        let trees: Vec<String> = trees.iter().filter(|w| !w.main).map(|w| w.path.clone()).collect();
        let now = Instant::now();
        let checks = [Checks { passed: 4, ..Checks::default() }, Checks { passed: 2, failed: 1, ..Checks::default() }, Checks { passed: 1, pending: 2, ..Checks::default() }, Checks { passed: 5, ..Checks::default() }];
        for (i, tree) in trees.into_iter().enumerate() {
            let number = 120 + i as u32;
            let state = if i == 3 { PrState::Merged } else { PrState::Open };
            let pr = Pr { number, title: format!("Capture PR {number}"), url: format!("https://github.com/acme/app/pull/{number}"), state, draft: false, checks: checks[i % 4] };
            d.prs.apply(tree, Ok(Some(pr)), now);
        }
    }),
```

**Step 4: Run it.** `cd packages/desktop && cargo test -p pocket pull_requests`

Expected: `test result: ok. 6 passed`. `cargo build -p pocket` builds with no new warnings.

### Task 4.4: PR chip in the Changes header, and "Open pull request" or a gh hint in its menu

**Files:**
- Modify `packages/desktop/crates/pocket/src/git_ui/pull_requests.rs`.
- Modify `packages/desktop/crates/pocket/src/git_ui/changes/header.rs`:
  - imports, lines 1–6
  - `changes_header`, lines 26–47 (insert before line 46 `.child(view)`)
  - `changes_menu_view`, line 64 (the Push row)

**Context:**
- `header.rs` is a child module of `git_ui::changes` and builds the header plus the `⋯` menu (`changes_menu_view`).
- Menu rows use `ui::menu_row(id, icon, label, None)` (ui.rs:887) with `cx.listener`. Close the menu (`this.changes.menu = false`) before acting, as `push` does (changes.rs:293).
- Greyed info lines use the local `info` closure (header.rs:51). There is no disabled-row style, so the hint uses `info`.
- `cx.open_url(&url)` is the existing way to open a link (used in `sidebar/host.rs` and `browser/view.rs`).
- The icon `external` exists in the icon set.
- `self.cwd()` is the main tree when none is picked. Main trees are never polled, so no chip or item shows there. That is intended.

**Step 1: Write the failing test.** Append inside `mod tests` in `pull_requests.rs`:

```rust
    #[test]
    fn the_changes_menu_opens_the_pr_or_says_what_gh_needs() {
        let (mut prs, now) = (PullRequests::default(), Instant::now());
        assert_eq!(prs.item("/a"), None);
        prs.apply("/a".into(), Ok(Some(pr(7))), now);
        assert_eq!(prs.item("/a"), Some(PrItem::Open("https://github.com/acme/app/pull/7")));
        prs.apply("/b".into(), Err(GhProblem::Missing), now);
        assert_eq!(prs.item("/b"), Some(PrItem::Hint("Install gh to see PRs")));
        assert_eq!(prs.item("/a"), Some(PrItem::Open("https://github.com/acme/app/pull/7")));
        let mut logged_out = PullRequests::default();
        logged_out.apply("/a".into(), Err(GhProblem::LoggedOut), now);
        assert_eq!(logged_out.item("/a"), Some(PrItem::Hint("Run gh auth login to see PRs")));
    }
```

**Step 2: Run it.** `cd packages/desktop && cargo test -p pocket pull_requests`

Expected: fails to compile, with `no method named 'item' found for struct 'PullRequests'` and `failed to resolve: use of undeclared type 'PrItem'`.

**Step 3: Implement.**

In `pull_requests.rs`, add above `impl PullRequests`:

```rust
/// What the Changes menu offers for the tree on screen.
#[derive(Debug, PartialEq)]
pub(crate) enum PrItem<'a> {
    Open(&'a str),
    Hint(&'static str),
}
```

and add to `impl PullRequests`:

```rust
    pub(crate) fn item(&self, tree: &str) -> Option<PrItem<'_>> {
        if let Some(pr) = self.get(tree) {
            return Some(PrItem::Open(&pr.url));
        }
        match self.problem? {
            GhProblem::Missing => Some(PrItem::Hint("Install gh to see PRs")),
            GhProblem::LoggedOut => Some(PrItem::Hint("Run gh auth login to see PRs")),
        }
    }
```

In `crates/pocket/src/git_ui/changes/header.rs`:

1. After line 1 (`use crate::desktop::Desktop;`), add:
   ```rust
   use crate::git_ui::pull_requests::{self, PrItem};
   ```

2. In `changes_header`, after the `more` binding (line 25) and before `div()` (line 26), add:

   ```rust
           let pr = self.cwd().and_then(|tree| {
               let pr = self.prs.get(&tree)?;
               let url = pr.url.clone();
               Some(pull_requests::chip("changes-pr", pr).cursor_pointer().on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| cx.open_url(&url)))
           });
   ```

   Then change line 46 `.child(view)` to:

   ```rust
               .children(pr)
               .child(view)
   ```

3. In `changes_menu_view`, after the `counts` binding (line 53), add:

   ```rust
           let pr = self.cwd().and_then(|tree| self.prs.item(&tree)).map(|item| match item {
               PrItem::Open(url) => {
                   let url = url.to_string();
                   ui::menu_row("changes-open-pr", "external", "Open pull request", None)
                       .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                           this.changes.menu = false;
                           cx.open_url(&url);
                           cx.notify();
                       }))
                       .into_any_element()
               }
               PrItem::Hint(text) => info(text.to_string()).into_any_element(),
           });
   ```

   Then, after line 64 (the `changes-push` row), add:

   ```rust
               .children(pr)
   ```

**Step 4: Run it.** `cd packages/desktop && cargo test -p pocket pull_requests`

Expected: `test result: ok. 7 passed`. `cargo build -p pocket` builds with no new warnings.

**Known, accepted:** when a hung gh turn expires and the same tree is asked again, the first run's late result clears `running` while the second is in flight, so two gh processes can briefly overlap.

### PR 4 verification

From `packages/desktop`:

```sh
cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace
```

Expected: everything passes with no new clippy warnings.

UI capture (per CLAUDE.md), from the repo root:

```sh
# before (on main, or with the PR stashed as a WIP commit)
.ui-review/fixture/capture.sh /tmp/prs-before repositories-worktrees=session,changes
# after
.ui-review/fixture/capture.sh /tmp/prs-after repositories-worktrees=prs,session,changes
```

Compare the two images:
- The sidebar's four `app-android` worktree rows show `#120` (green), `#121` (red), `#122` (amber) and `#123` (purple, merged), right-aligned before the row trail.
- The Changes header shows the chip after the branch name when a seeded worktree is on screen. Add the `worktree` step before `changes` to land on one if needed.
- Nothing else moves.

To check the menu, add a manual step or open `⋯` by hand. It shows "Open pull request" for a seeded tree. With the fixture's logged-out gh and a non-seeded tree, it shows "Run gh auth login to see PRs".

## PR 5: Create pull request

**Scope:** When `gh` found no PR for the worktree on screen, and its branch isn't the base, the Changes `⋯` menu offers "Create pull request".
- Clicking it pushes the branch (setting upstream), runs `gh pr create --fill --base <base>`, and opens the new PR's URL.
- It then marks the tree stale, so the next 2s poll fetches the chip.
- Progress shows on the commit button via `changes.busy`. Failures show in the existing `changes.error` box.

**Depends on:** PR 4.

**Done when:**
- `cargo test -p git create_argv` and `cargo test -p pocket pull_requests` pass.
- Build, clippy and test pass for the whole workspace.
- A capture of the Changes menu on a branch with no PR shows the Create row.

### Task 5.1: `gh pr create` argv

**Files:**
- Modify `packages/desktop/crates/git/src/github.rs`, created in 4.1: below `VIEW`, and the tests module.

**Context:**
- `daemon::run_login` takes `&[&str]`, so return `Vec<&str>` borrowing `base`, not `Vec<String>`.
- `Repo.base` is the local `main`/`master` off the main branch, or the upstream (`origin/main`) on it. `gh --base` wants a branch name on the remote, so strip `origin/`.
- `--fill` takes the title and body from the commits, with no prompt. The login shell has no TTY.

**Step 1: Write the failing test.** Append inside `mod tests` in `github.rs`:

```rust
    #[test]
    fn create_argv_targets_the_base_without_its_remote() {
        assert_eq!(create_argv("origin/main"), ["gh", "pr", "create", "--fill", "--base", "main"]);
        assert_eq!(create_argv("develop"), ["gh", "pr", "create", "--fill", "--base", "develop"]);
    }
```

**Step 2: Run it.** `cd packages/desktop && cargo test -p git create_argv`

Expected: fails to compile with `cannot find function 'create_argv' in this scope`.

**Step 3: Implement.** In `github.rs`, below `VIEW`:

```rust
/// Opens a PR for the checked-out branch into `base`, titled and described from its commits.
pub fn create_argv(base: &str) -> Vec<&str> {
    vec!["gh", "pr", "create", "--fill", "--base", base.strip_prefix("origin/").unwrap_or(base)]
}
```

**Step 4: Run it.** `cd packages/desktop && cargo test -p git create_argv`

Expected: `test result: ok. 1 passed`.

### Task 5.2: "Create pull request" in the Changes menu

**Files:**
- Modify `packages/desktop/crates/pocket/src/git_ui/pull_requests.rs`: `PrItem`, `item`, a new `stale` method, and tests.
- Modify `packages/desktop/crates/pocket/src/git_ui/changes.rs`:
  - a new `fn create_pr` after `fn push` (lines 292–310)
  - imports (lines 1–13)
- Modify `packages/desktop/crates/pocket/src/git_ui/changes/header.rs`, the `pr` item binding added in 4.4, inside `changes_menu_view`.

**Context:**
- Mimic `fn push` in `changes.rs` (lines 292–310):
  - close the menu
  - guard on `changes.busy`
  - set `busy`, clear `error`
  - do the work in a background spawn, then a `cx.spawn` + `this.update` that clears `busy`, sets `error`, calls `refresh_git` and notifies.
- `create_pr` lives in `changes.rs`, not `pull_requests.rs`, because it is the same kind of action as `push`. `header.rs` is a child module, so it can call the private method, as it does `this.push(cx)`.
- Show "Create" only after `gh` has answered for this tree (`by_tree` has an entry) with no PR and no problem. Otherwise the row would flash before the first poll.
- On main, `base` is `origin/main` while `branch` is `main`, so compare after stripping `origin/`.
- `gh pr create` prints the new PR's URL as the last stdout line. `run_login` already trims.
- After success, `stale(tree)` drops the entry, so `due` picks the tree on the next 2s refresh and the chip appears.
- `problem` is never cleared. Create only shows when `problem` is `None`, so there's nothing to clear.

**Step 1: Write the failing test.**

In `pull_requests.rs` tests, replace `the_changes_menu_opens_the_pr_or_says_what_gh_needs` (from 4.4) with the new signature, and add two tests:

```rust
    #[test]
    fn the_changes_menu_opens_the_pr_or_says_what_gh_needs() {
        let (mut prs, now) = (PullRequests::default(), Instant::now());
        assert_eq!(prs.item("/a", "fix", Some("main")), None);
        prs.apply("/a".into(), Ok(Some(pr(7))), now);
        assert_eq!(prs.item("/a", "fix", Some("main")), Some(PrItem::Open("https://github.com/acme/app/pull/7")));
        prs.apply("/b".into(), Err(GhProblem::Missing), now);
        assert_eq!(prs.item("/b", "fix", Some("main")), Some(PrItem::Hint("Install gh to see PRs")));
        assert_eq!(prs.item("/a", "fix", Some("main")), Some(PrItem::Open("https://github.com/acme/app/pull/7")));
        let mut logged_out = PullRequests::default();
        logged_out.apply("/a".into(), Err(GhProblem::LoggedOut), now);
        assert_eq!(logged_out.item("/a", "fix", Some("main")), Some(PrItem::Hint("Run gh auth login to see PRs")));
    }

    #[test]
    fn the_changes_menu_offers_create_once_gh_found_no_pr_for_a_branch_off_its_base() {
        let (mut prs, now) = (PullRequests::default(), Instant::now());
        assert_eq!(prs.item("/a", "fix", Some("main")), None);
        prs.apply("/a".into(), Ok(None), now);
        assert_eq!(prs.item("/a", "fix", Some("main")), Some(PrItem::Create));
        assert_eq!(prs.item("/a", "fix", None), None);
        assert_eq!(prs.item("/a", "main", Some("origin/main")), None);
        prs.apply("/a".into(), Ok(Some(pr(3))), now);
        assert_eq!(prs.item("/a", "fix", Some("main")), Some(PrItem::Open("https://github.com/acme/app/pull/3")));
    }

    #[test]
    fn a_stale_tree_is_due_at_once() {
        let (mut prs, now) = (PullRequests::default(), Instant::now());
        let trees = vec!["/a".to_string()];
        prs.apply("/a".into(), Ok(None), now);
        assert_eq!(prs.due(Some("/a"), &trees, now), None);
        prs.stale("/a");
        assert_eq!(prs.due(Some("/a"), &trees, now).as_deref(), Some("/a"));
    }
```

**Step 2: Run it.** `cd packages/desktop && cargo test -p pocket pull_requests`

Expected: fails to compile, with:
- `this method takes 1 argument but 3 arguments were supplied` (on `item`)
- `no variant named 'Create' found for enum 'PrItem'`
- `no method named 'stale' found for struct 'PullRequests'`

**Step 3: Implement.**

In `pull_requests.rs`, replace `PrItem` with:

```rust
/// What the Changes menu offers for the tree on screen.
#[derive(Debug, PartialEq)]
pub(crate) enum PrItem<'a> {
    Open(&'a str),
    Create,
    Hint(&'static str),
}
```

Replace `item` and add `stale` in `impl PullRequests`:

```rust
    /// Create is offered only once `gh` has answered for `tree` with no PR, and `branch` isn't `base`.
    pub(crate) fn item(&self, tree: &str, branch: &str, base: Option<&str>) -> Option<PrItem<'_>> {
        if let Some(pr) = self.get(tree) {
            return Some(PrItem::Open(&pr.url));
        }
        match self.problem {
            Some(GhProblem::Missing) => Some(PrItem::Hint("Install gh to see PRs")),
            Some(GhProblem::LoggedOut) => Some(PrItem::Hint("Run gh auth login to see PRs")),
            None => (self.by_tree.contains_key(tree) && base.is_some_and(|b| b.strip_prefix("origin/").unwrap_or(b) != branch)).then_some(PrItem::Create),
        }
    }

    /// Forgets `tree`'s PR so the next poll asks `gh` again.
    pub(crate) fn stale(&mut self, tree: &str) {
        self.by_tree.remove(tree);
    }
```

In `crates/pocket/src/git_ui/changes.rs`:

1. Add an import after line 6 (`use git::FileStat;`):
   ```rust
   use git::github;
   ```
2. Add after `fn push` (after line 310):

```rust
    fn create_pr(&mut self, cx: &mut Context<Self>) {
        self.changes.menu = false;
        let (Some(tree), Some(base)) = (self.cwd(), self.repo().and_then(|r| r.base.clone())) else { return cx.notify() };
        if self.changes.busy.is_some() {
            return cx.notify();
        }
        self.changes.busy = Some("Creating PR…");
        self.changes.error = None;
        let dir = tree.clone();
        let task = cx.background_executor().spawn(async move {
            daemon::run_login(git::PUSH, &dir, "")?;
            daemon::run_login(&github::create_argv(&base), &dir, "")
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |d, cx| {
                d.changes.busy = None;
                match res {
                    Ok(out) => {
                        if let Some(url) = out.lines().last() {
                            cx.open_url(url);
                        }
                        d.prs.stale(&tree);
                    }
                    Err(e) => d.changes.error = Some(e),
                }
                d.refresh_git(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }
```

In `header.rs` `changes_menu_view`, replace the `pr` binding from 4.4 with:

```rust
        let pr = self.cwd().and_then(|tree| self.prs.item(&tree, &repo.branch, repo.base.as_deref())).map(|item| match item {
            PrItem::Open(url) => {
                let url = url.to_string();
                ui::menu_row("changes-open-pr", "external", "Open pull request", None)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.changes.menu = false;
                        cx.open_url(&url);
                        cx.notify();
                    }))
                    .into_any_element()
            }
            PrItem::Create => ui::menu_row("changes-create-pr", "plus", "Create pull request", None)
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.create_pr(cx)))
                .into_any_element(),
            PrItem::Hint(text) => info(text.to_string()).into_any_element(),
        });
```

**Step 4: Run it.** `cd packages/desktop && cargo test -p pocket pull_requests`

Expected: `test result: ok. 9 passed`.

### PR 5 verification

From `packages/desktop`:

```sh
cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace
```

UI capture, from the repo root. Capture before and after with the Changes menu open on a worktree whose poll returned no PR. With the fixture's logged-out gh, Create won't show by itself. Either:
- capture against a logged-in HOME, or
- add a temporary capture step `("no-pr", |d, _, _| { if let Some(t) = d.cwd() { d.prs.apply(t, Ok(None), Instant::now()); } })` and drop it before committing.

```sh
.ui-review/fixture/capture.sh /tmp/create-pr-after repositories-worktrees=worktree,no-pr,changes
```

Expect the `⋯` menu to show "Create pull request" (plus icon) under Push, and nothing else to change. Manually, on a real repo with gh logged in:
1. Click Create.
2. The commit button reads "Creating PR…".
3. The browser opens the PR.
4. Within ~2s the `#N` chip appears in the sidebar and header.
