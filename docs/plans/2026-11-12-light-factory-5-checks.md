# Light Factory 5: Checks and Auto-fix Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** A Ticket that reaches Checks gets its worker branches merged, a draft PR, its local commands run, and its GitHub checks watched. A failure goes back to the lead up to the cap. After the cap the Ticket is failed, and you send it back with a hint or close it.

**Architecture:** `factory.Next` gains one pure function for the Checks station, `checksNext`, which reads `Facts` and returns the Ticket and the actions. The engine keeps one check record per commit in `state/factory/<ticket>/checks/`, and that record is where `Facts.Local` comes from. Slow work (the push, `gh pr create`, each check command) runs on its own goroutine and comes back to the engine as a queued event. The desktop draws the record in the **Checks** tab and the Failed decision in the footer bar.

**Toolset:** Go (pocketd), Rust and GPUI (desktop). From `packages/pocketd`: `go test ./internal/factory/ -run <Name> -count=1`, `go test ./e2e -run <Name> -count=1 -v`, then `go vet ./... && go test ./...`. From `packages/desktop`: `cargo test -p agents <name>`, `cargo test -p pocket <name>`, then `cargo build --workspace`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`. Capture from the repository root: `.ui-review/fixture/capture.sh <dir> <name>=<steps>`.

**Read first:**
- `docs/plans/2026-11-12-light-factory.md`: the index. Its Decisions "Integration", "The draft PR", "Checks", "Auto-fix", and its Contracts.
- `docs/designs/2026-11-12-light-factory.md`: C1 to C9, E1, E4, E5, E6, E14, T8, and §11 for the failed callout.
- `docs/designs/2026-11-12-light-factory-stories.md`: US-23, US-24, US-25, US-26, US-36.
- `packages/pocketd/internal/github/service.go`: `Create` (`:369`), `pushArgv` (`:395`), `Listen` (`:88`), `Watch` (`:104`), `Leave` (`:114`), `PR` (`:269`), `after` (`:356`).
- `packages/pocketd/internal/github/service_test.go` and `testdata/fakegh/gh`: the fake `gh`.
- `packages/pocketd/internal/team/running.go:287`: `Teams.Merge`.
- `packages/pocketd/internal/worktree/merge.go`: `Head`, `Commit`, `Dirty`, `MergeInProgress`.
- `packages/pocketd/internal/hostop/run.go`: `Run`.
- `packages/pocketd/e2e/team_run_test.go`: `teamPlan`, `reviewedTeam`. `e2e/fakeclaude/chat.go:259`: `assignment`.
- `.ui-review/prototypes/light-factory/app.js`: `checksTab` (`:340`) and the failed branch of `barHTML` (`:397`).
- `packages/desktop/crates/pocket/src/git_ui/pull_requests.rs` and `git_ui/pull_request/card.rs:171` (`pr_checks`).

---

## What plans 1 to 4 give this plan

Plans 1 to 4 are not in the repository when this plan is written. The left column is what this plan uses. Where the merged code has another name for the same thing, use the merged name. The behaviour in the right column is what this plan needs.

| Name | Source | What this plan needs from it |
|---|---|---|
| `proto.StationChecks`, `proto.StationPR`, `proto.GateAsk`, `proto.MaxFix`, `Ticket.Station`, `.Gate`, `.PR`, `.Fix`, `.Hint` | index, Wire (plan 1) | As the index declares them |
| `proto.FactoryConfig`, `factory.Configs.Get(project)` | index (plan 1) | `Fix` and `Checks` of the Ticket's project |
| `proto.FactoryTicket`, `proto.FactoryEvent` | index (plan 3) | `FactoryTicket` has the fields `Checks` and `NoChecks`. If plan 3 left them out, add them as the index shows them |
| `factory.Facts`, `Session`, `Action`, `Next` | index, `station.go` (plan 3) | `Next` switches on `f.Ticket.Station` |
| `e.step(id string, ev event)` | plan 3, Task 1.5 | Gathers `Facts`, calls `Next`, saves the Ticket with `e.update`, notes what `Notes` returns (a `moved` event when the station changes, a `failed` event when the status becomes `failed`), then runs each `Action` with the Ticket that `Next` returned |
| `e.facts(t, ev) Facts`, `e.act(t, a) bool`, `same(a, b)` | plan 3, Task 1.5 | `facts` fills `Facts`. `act` is the switch over `a.Kind`. `same` compares the fields `Next` may change |
| `event{ticket, agent, turn, ok}`, `e.post(ev)`, `e.handle(ev)` | plan 3, Task 1.5 | The queue. `e.post(event{ticket: id})` steps one Ticket. `event{agent: id}` steps the Ticket whose lead is that agent. `TurnEnded` posts `event{agent, turn: true, ok}`. `TeamChanged` posts `event{agent: t.Lead}` |
| `e.now()`, `e.update(id, change) bool`, `e.ticket(id)`, `e.lead(id)`, `e.tell(id)` | plan 3, Task 1.5 | Small helpers of the engine |
| `Next` fails a Ticket whose lead is gone | plan 3, Task 1.1 | `Next` checks `f.Lead.Present` before it looks at the station, so `checksNext` never sees a Ticket without a lead |
| `e.note(ticket, ev)` | index, `activity.go` (plan 3) | Appends the event and publishes `factory.update` |
| `Engine.Detail`, `Engine.Recover` | plan 3 | `Recover` posts `event{ticket: id}` for every live Ticket that has a station |
| `ErrOff` | plan 3, Task 1.5 | The error a nil `*Engine` returns: "The Factory is off" |
| `chat`, `leadID`, `kinds` in the tests of `internal/factory` | plan 3 | The fake lead (`chat.sent()` lists its prompts), its agent id, and `kinds(acts) []string` |
| `Engine.Advance`, the `factory.advance` handler in `internal/wsserver` | index (plan 4) | The pattern that `factory.hint` copies |
| A Ticket at `Station: checks` | index (plan 4) | It arrives when its Team is in `review` with `Ready: true`, or after `factory.advance` |
| `agents::factory::{Config, Detail}`, `Agents::detail`, `Agents::factory_config`, `factory_id()` | index, Desktop crates (plans 1 and 3) | `factory_id()` is the id helper of the Factory verbs, beside `board_id()` (`agents/src/agents.rs:835`) |
| `pocket::factory::FactoryState`, `factory/page.rs`, `factory/tabs.rs`, `factory/bar.rs`, `factory/station.rs` | index (plans 2 and 3) | The page draws the open `Tab`. The bar draws a `Decision`. `station::note` gives the card its note |

## Decisions

- **The Team's state decides the merge, not the way the Ticket arrived.** A Team in `review` or `conflict` is merged. A Ticket with no Team, or with a Team in any other state, skips the merge. That covers `factory.advance` and the rework from PR review without a flag.
- **A dirty lead checkout is committed first.** `Teams.Merge` refuses a checkout with tracked changes (`worktree.ErrDirty`). The lead may have edited files during its review turn. The engine commits them with `worktree.Commit` and the message `Lead's changes before the merge`, then calls `Teams.Merge`. A checkout with a merge still open is never committed by this step. It fails the Ticket.
- **A fault is a failure that is not a failed check.** A merge conflict, a merge that failed, a push that failed, a PR that could not be opened. A fault fails the Ticket at Checks with the reason in `Why`, uses no attempt, and sends no automatic fix note. A hint is the way on. A new commit in the checkout also starts Checks again.
- **Team mode still mails its conflict note to the lead** (`team.ConflictNote`), so the lead may resolve the conflict before you send a hint. The engine commits nothing until a fix turn that it started ends.
- **One check record per commit.** `state/factory/<ticket>/checks/0001.json`, `0002.json`, and so on. The newest file is the record that counts. A record whose `Head` is not the commit in the checkout is stale, unless it holds a fix note that is out or a hint that waits. A stale record reads as "nothing ran", so a commit made by hand starts Checks again and uses no attempt.
- **Order inside Checks:** merge, push and open the draft PR, local commands, GitHub checks. The push comes before the local commands so that GitHub's checks run at the same time.
- **A check command runs as `sh -c "exec 2>&1\n<command>"`** in the lead's checkout, with the login environment, through `hostop.Run`. The limit is 30 minutes for each command. `hostop.Run` keeps the first 64 MB, and `Tail` then keeps the last 64 KB from a line start. Windows is out of scope (PRD N6).
- **"No GitHub checks reported" is final 90 seconds after the push.** GitHub creates the check runs of a push within seconds. The PR cache polls a watched worktree every 30 seconds (`github.WatchedEvery`), so 90 seconds is three polls. A shorter wait could pass a Ticket before a slow workflow appears. A longer wait makes every project without CI wait for nothing. The engine saves the result as `Record.Quiet`, so a restart does not wait again.
- **Waiting for GitHub keeps the Ticket `working`,** so it holds its slot (US-23). Pending GitHub checks have no time limit in v1.
- **`Ticket.Fix` counts automatic fix notes only.** A hint does not add to it. A hint note carries `attempt="<Fix+1>"` and `by="you"`.
- **`Ticket.Hint` is true from the hint until the Ticket passes Checks.** The text of the hint waits in the check record until its note is sent.
- **Any normal turn end of the lead while a fix note is out counts as the fix turn.** The engine cannot tell which prompt a turn answered.
- **The same head is not counted twice (E6).** When a fix turn ends and the commit in the checkout did not change, the next record is marked `Same`. The checks run again once, because a flaky check may pass. A failure on a `Same` record sends no automatic note.
- **A failed Ticket whose GitHub checks later pass on the same commit passes.** A re-run on github.com is not an attempt (US-25).
- **`Why` is the card's note.** `Checks · 2/3`, `Fixing · attempt 1`, `Fixing · your hint`, and for a failure `<what failed> · <why no fix follows>`, for example `cargo test · Auto-fix 2 of 2 used`.
- **A hint note for a fault carries only your text.** The index's fix note has no line for a fault, and this plan adds none.
- **The Ticket's key in the PR body** is the first three letters or digits of the project folder's name in upper case, then `-`, then `Ticket.Number`. That is the rule of the desktop's `ticket_key` (`pocket/src/board/logic.rs:91`).
- **Passing sets `Station: pr` and `Status: working`.** Nothing reads that station until plan 6, so the Ticket rests there and holds its slot.
- **The long output is cut on the desktop to its last 40 lines once,** when the `Detail` arrives, not in `render`.

## Checks: states and transitions

`checksNext` reads the rows from the top. The first row that matches decides. "Local" is `Facts.Local`, built from the newest check record.

| # | When | The Ticket reads | Action | What moves it on |
|---|---|---|---|---|
| 1 | `Local.Fix` is `mailed` and `Lead.Turn` is `needsYou` | `needsYou`, `Gate: ask` | none | You answer the Ask |
| 2 | `Local.Fix` is `mailed` | `working`, `Why` unchanged | none | The lead's turn ends: the record becomes `ended` or `stopped` |
| 3 | `Local.Fix` is `ended` | `working`, `Committing the fix` | `commitPush` | A new record for the new commit |
| 4 | `Local.Hint` | `Hint: true`, `working`, `Fixing · your hint` | `sendFix` with `Text: "you"` | Row 2 |
| 5 | `Local.Fix` is `stopped` | `failed`, `The lead's fix turn failed or was stopped` | none | A hint, or the lead's next normal turn end |
| 6 | `Local.Error` is set (a fault) | `failed`, `Why` is the error | none | A hint, or a new commit |
| 7 | `Team.State` is `review` or `conflict` | `working`, `Merging the worker branches` | `integrate` | `TeamChanged` |
| 8 | `Team.State` is `merging` | `working`, `Merging the worker branches` | none | `TeamChanged` |
| 9 | `Ticket.PR` is 0 and `Facts.PR` is open | `PR` is that number, then row 10 | | |
| 10 | `Ticket.PR` is 0, or `Local.Pushed` is false | `working`, `Opening the draft PR` | `openPR` | The push comes back |
| 11 | `Local.State` is empty | `working`, `Checks · 0/<n>` | `runChecks` | Each command comes back |
| 12 | `Local.State` is `running` | `working`, `Checks · <done>/<n>` | none | Each command comes back |
| 13 | `Local.State` is `failed` | the fix rows, with the command as "what" | | |
| 14 | `Facts.PR` is nil | `working`, `Waiting for GitHub` | none | `PRChanged` |
| 15 | `PR.State` is `CLOSED` | `failed`, `The pull request was closed` | none | Close ticket |
| 16 | `PR.State` is `OPEN` and `PR.Head` is not `Local.Head` | `working`, `Waiting for GitHub` | none | `PRChanged` |
| 17 | `PR.Mergeable` is `CONFLICTING` | the fix rows, with `Conflicts with <base>` as "what" (E5) | | |
| 18 | `PR.Checks.Failed` is above 0 | the fix rows, with the first failed check's name as "what" | | |
| 19 | `PR.Checks.Pending` is above 0 | `working`, `GitHub checks · <passed>/<all>` | none | `PRChanged` |
| 20 | The PR has no check and `Local.Quiet` is false | `working`, `Waiting for GitHub checks` | none | The 90 second timer |
| 21 | otherwise | `Station: pr`, `working`, `Why` empty, `Hint: false` | none | Plan 6 |

The fix rows, for a failed check named "what":

| # | When | The Ticket reads | Action |
|---|---|---|---|
| F1 | `Local.Same` | `failed`, `<what> · The lead changed nothing` | none |
| F2 | `Config.Fix` is 0 | `failed`, `<what> · Auto-fix is off` | none |
| F3 | `Ticket.Fix` is at or above `Config.Fix` | `failed`, `<what> · Auto-fix <Fix> of <cap> used` | none |
| F4 | otherwise | `Fix` plus 1, `working`, `Fixing · attempt <Fix>` | `sendFix` |

A Ticket whose lead is gone never reaches these rows. `Next` fails it first (plan 3).

What each PR ships of the table: PR 1 rows 6 to 10, PR 2 rows 11 to 21 with every fix row as a plain failure, PR 3 rows 1 to 5 and F1 to F4.

How the cases of the PRD map: C1 and E4 are rows 6 to 8. C2 is rows 9 and 10. C3 is rows 11 to 13. C4 is rows 14 to 21. C5, C6, C8 and E6 are the fix rows with rows 1 to 3. C7 and E14 are rows 4 and 5. E1 is row 11: a record that says `running` while nothing runs in this process reads as empty.

---

## PR 1: Integrate and open the draft PR

**Scope:** `github.Service.Push`. The check record. Rows 6 to 10 of the table. The engine merges the Team, pushes, and opens the draft PR.

**Depends on:** plans 1 to 4.

**Done when:** a Ticket at Checks has its worker branches merged and a draft PR whose number is in `Ticket.PR`. A conflict fails it with the files in `Why` and `Fix` unchanged. After the PR is open the Ticket rests at Checks with `Status: working` and `Why: Draft PR open`. PR 2 moves it on.

### Task 1.1: `github.Service.Push`, and a fake `gh` that knows the head

**Files:**
- Modify: `packages/pocketd/internal/github/service.go:367-376`
- Modify: `packages/pocketd/internal/github/testdata/fakegh/gh`
- Test: `packages/pocketd/internal/github/service_test.go`

**Context:** `Create` pushes with `s.env.run(ctx, tree, s.pushArgv(ctx, tree), "", pushTimeout)`. The index asks for that push as `Push(ctx, tree string) error`. Later tasks also need two things from the fake `gh`: a way to say "this branch has no PR until `gh pr create` ran", and a `headRefOid` that follows the checkout. Both are new files and markers, so no existing test changes.

**Step 1: Write the failing test**

Add to `service_test.go`, after `TestAFailedPushOpensNoPR`:

```go
func TestPushPushesTheBranchAndOpensNoPR(t *testing.T) {
	f := newFake(t)
	_, tree, reg := repo(t, true)
	s := New(f.env(true), reg)
	if err := s.Push(ctx(t), tree); err != nil {
		t.Fatal(err)
	}
	origin := filepath.Join(filepath.Dir(tree), "origin.git")
	if got, want := git(t, origin, "rev-parse", "fix"), git(t, tree, "rev-parse", "HEAD"); got != want {
		t.Fatalf("the remote has %s, the checkout has %s", got, want)
	}
	if f.called("pr create") {
		t.Fatalf("Push opened a PR: %v", f.calls())
	}
}

func TestAPushThatGitRefusesIsAGitFailure(t *testing.T) {
	f := newFake(t)
	_, tree, reg := repo(t, false)
	s := New(f.env(true), reg)
	err := s.Push(ctx(t), tree)
	if e, ok := errors.AsType[*Error](err); !ok || e.Code != proto.CodeGitFailed {
		t.Fatalf("got %v", err)
	}
}

func TestTheFakeGhHasNoPRUntilOneIsCreatedAndReportsTheCheckoutsHead(t *testing.T) {
	f := newFake(t)
	_, tree, reg := repo(t, true)
	f.put("nopr", "")
	f.put("created", "https://github.com/acme/app/pull/57\n")
	f.put("threads.json", `{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[]}}}}}`)
	f.put("view.json", `{"isDraft":true,"number":57,"state":"OPEN","title":"Fix it","url":"https://github.com/acme/app/pull/57","headRefOid":"@HEAD@","baseRefName":"main","statusCheckRollup":[]}`)
	s := New(f.env(true), reg)
	if pr, problem := s.PR(ctx(t), tree); pr != nil || problem != "" {
		t.Fatalf("before the PR exists: %+v %q", pr, problem)
	}
	if _, _, err := s.Create(ctx(t), tree, "Fix it", "body", "", true); err != nil {
		t.Fatal(err)
	}
	s.after(tree, func(*PR) {})
	pr, _ := s.PR(ctx(t), tree)
	if pr == nil || pr.Head != git(t, tree, "rev-parse", "HEAD") {
		t.Fatalf("got %+v", pr)
	}
}
```

**Step 2: Run it**

Run: `cd packages/pocketd && go test ./internal/github -run 'TestPush|TestAPushThat|TestTheFakeGh' -count=1`
Expected: FAIL to build with `s.Push undefined (type *Service has no field or method Push)`.

**Step 3: Implement**

In `service.go`, replace the first lines of `Create`.

Before (`service.go:369-376`):

```go
func (s *Service) Create(ctx context.Context, tree, title, body, base string, draft bool) (int, string, error) {
	if _, err := s.env.run(ctx, tree, s.pushArgv(ctx, tree), "", pushTimeout); err != nil {
		msg, detail := failure(err)
		if f, ok := errors.AsType[*Failure](err); ok {
			detail = strings.TrimSpace(f.Stderr)
		}
		return 0, "", &Error{Code: proto.CodeGitFailed, Message: msg, Detail: detail}
	}
```

After:

```go
func (s *Service) Create(ctx context.Context, tree, title, body, base string, draft bool) (int, string, error) {
	if err := s.push(ctx, tree); err != nil {
		return 0, "", err
	}
```

Add above `pushArgv`:

```go
// Push pushes tree's branch as Create does, then asks for its PR again.
func (s *Service) Push(ctx context.Context, tree string) error {
	if err := s.push(ctx, tree); err != nil {
		return err
	}
	s.after(tree, func(*PR) {})
	return nil
}

func (s *Service) push(ctx context.Context, tree string) error {
	_, err := s.env.run(ctx, tree, s.pushArgv(ctx, tree), "", pushTimeout)
	if err == nil {
		return nil
	}
	msg, detail := failure(err)
	if f, ok := errors.AsType[*Failure](err); ok {
		detail = strings.TrimSpace(f.Stderr)
	}
	return &Error{Code: proto.CodeGitFailed, Message: msg, Detail: detail}
}
```

In `testdata/fakegh/gh`, add two lines to the header comment, after the `view.json` line:

```sh
#   nopr             gh pr view says "no pull requests found"; gh pr create removes it
#   @HEAD@           in view.json, replaced by the commit of the folder gh runs in
```

Replace the `"pr view")` arm.

Before:

```sh
"pr view")
	if [ -f "$d/view.err" ]; then
		cat "$d/view.err" >&2
		exit 1
	fi
	cat "$d/view.json"
	;;
```

After:

```sh
"pr view")
	if [ -f "$d/nopr" ]; then
		echo "no pull requests found for branch" >&2
		exit 1
	fi
	if [ -f "$d/view.err" ]; then
		cat "$d/view.err" >&2
		exit 1
	fi
	sed "s/@HEAD@/$(git rev-parse HEAD 2>/dev/null)/" "$d/view.json"
	;;
```

Replace the `"pr create")` arm.

Before:

```sh
"pr create")
	act
	cat > "$d/body"
	cat "$d/created"
	;;
```

After:

```sh
"pr create")
	act
	rm -f "$d/nopr"
	cat > "$d/body"
	cat "$d/created"
	;;
```

**Step 4: Run it**

Run: `cd packages/pocketd && go test ./internal/github -count=1`
Expected: `ok  	pocketd/internal/github`. Every older test of the package still passes.

### Task 1.2: The check record and `LocalChecks`

**Files:**
- Create: `packages/pocketd/internal/factory/checks.go`
- Modify: `packages/pocketd/internal/factory/station.go` (the `Facts` struct, plan 3 Task 1.1)
- Test: `packages/pocketd/internal/factory/checks_test.go`

**Context:** The index declares `Facts.Local LocalChecks` with the note "none | running | passed | failed, for the head they ran on", and `Facts.PR *proto.PR`. Plan 3's `Facts` has neither field yet. Checks needs more than one word for `Local`, so this task makes `LocalChecks` a struct. See "Contract changes needed".

**Step 1: Write the failing test**

Create `checks_test.go`:

```go
//go:build unix

package factory

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"pocketd/internal/proto"
)

func TestATicketWithNoCheckRecordHasNone(t *testing.T) {
	if r, ok := ReadRecord(t.TempDir(), "t1"); ok {
		t.Fatalf("got %+v", r)
	}
}

func TestTheNewestCheckRecordIsTheOneThatIsRead(t *testing.T) {
	home := t.TempDir()
	first := Record{Head: "aaa", State: LocalFailed, Total: 1, Checks: []proto.FactoryCheck{{Command: "make test", Exit: 2, Output: "boom\n", Attempt: 0, At: 5}}}
	if err := WriteRecord(home, "t1", first, true); err != nil {
		t.Fatal(err)
	}
	first.Fix = FixMailed
	if err := WriteRecord(home, "t1", first, false); err != nil {
		t.Fatal(err)
	}
	if err := WriteRecord(home, "t1", Record{Head: "bbb", Same: false}, true); err != nil {
		t.Fatal(err)
	}
	got, ok := ReadRecord(home, "t1")
	if !ok || got.Head != "bbb" {
		t.Fatalf("got %+v", got)
	}
	names, _ := filepath.Glob(filepath.Join(home, "state", "factory", "t1", "checks", "*.json"))
	if len(names) != 2 {
		t.Fatalf("a save that is not fresh must replace the newest file, got %v", names)
	}
	raw, _ := os.ReadFile(names[0])
	if want := `{"head":"aaa","state":"failed","total":1,"checks":[{"command":"make test","exit":2,"output":"boom\n","attempt":0,"at":5}],"fix":"mailed"}`; string(raw) != want {
		t.Fatalf("got  %s\nwant %s", raw, want)
	}
}

func TestACheckRecordThatCantBeReadCountsAsNone(t *testing.T) {
	home := t.TempDir()
	dir := filepath.Join(home, "state", "factory", "t1", "checks")
	os.MkdirAll(dir, 0o700)
	os.WriteFile(filepath.Join(dir, "0001.json"), []byte("{"), 0o600)
	if r, ok := ReadRecord(home, "t1"); ok || !reflect.DeepEqual(r, Record{}) {
		t.Fatalf("got %+v", r)
	}
}
```

**Step 2: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -run 'CheckRecord' -count=1`
Expected: FAIL to build with `undefined: ReadRecord`. `proto.FactoryCheck` is also undefined until the next step.

**Step 3: Implement**

Add to `packages/pocketd/internal/proto/factory.go`, if plan 3 did not add it:

```go
// FactoryCheck is one local command of a Ticket's newest check record.
type FactoryCheck struct {
	Command string `json:"command"`
	Exit    int    `json:"exit"`
	Output  string `json:"output"`  // the last 64 KB
	Attempt int    `json:"attempt"` // Ticket.Fix when it ran
	At      int64  `json:"at"`
}
```

In `station.go`, add two fields to `Facts`, after `Team`:

```go
	// PR is the PR of the Ticket's worktree, nil until one is known.
	PR *proto.PR
	// Local is what Checks knows about the commit in the lead's checkout.
	Local LocalChecks
```

Create `checks.go`:

```go
package factory

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"slices"

	"pocketd/internal/atomicfile"
	"pocketd/internal/proto"
)

const (
	LocalRunning = "running"
	LocalPassed  = "passed"
	LocalFailed  = "failed"

	FixMailed  = "mailed"
	FixEnded   = "ended"
	FixStopped = "stopped"
)

// LocalChecks is what Checks knows about the commit in the lead's checkout.
type LocalChecks struct {
	Head   string
	Pushed bool
	// State is "" until the commands ran on Head, then running, passed or failed.
	State       string
	Done, Total int
	// Failed is the command that failed.
	Failed string
	// Fix follows a fix note: mailed, then ended or stopped with the lead's turn.
	Fix string
	// Hint: a hint waits to be sent.
	Hint bool
	// Same: the fix turn before this commit changed nothing.
	Same bool
	// Error is a fault: a failure that is not a failed check.
	Error string
	// Quiet: GitHub reported no check for Head within quietAfter.
	Quiet bool
}

// Record is what Checks keeps about one commit of a Ticket's branch.
type Record struct {
	Head   string               `json:"head"`
	Pushed int64                `json:"pushed,omitempty"` // Unix ms
	State  string               `json:"state,omitempty"`
	Total  int                  `json:"total,omitempty"`
	Checks []proto.FactoryCheck `json:"checks,omitempty"`
	Fix    string               `json:"fix,omitempty"`
	Hint   string               `json:"hint,omitempty"` // your text, until its note is sent
	Same   bool                 `json:"same,omitempty"`
	Error  string               `json:"error,omitempty"`
	Quiet  bool                 `json:"quiet,omitempty"`
}

func recordFiles(home, ticket string) []string {
	names, _ := filepath.Glob(filepath.Join(home, "state", "factory", ticket, "checks", "*.json"))
	slices.Sort(names)
	return names
}

// ReadRecord is ticket's newest check record.
func ReadRecord(home, ticket string) (Record, bool) {
	names := recordFiles(home, ticket)
	if len(names) == 0 {
		return Record{}, false
	}
	raw, err := os.ReadFile(names[len(names)-1])
	var r Record
	if err != nil || json.Unmarshal(raw, &r) != nil {
		return Record{}, false
	}
	return r, true
}

// WriteRecord saves r over ticket's newest record, or as the next one when fresh or there is none.
func WriteRecord(home, ticket string, r Record, fresh bool) error {
	names := recordFiles(home, ticket)
	dir := filepath.Join(home, "state", "factory", ticket, "checks")
	path := filepath.Join(dir, fmt.Sprintf("%04d.json", len(names)+1))
	if !fresh && len(names) > 0 {
		path = names[len(names)-1]
	}
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return err
	}
	raw, err := json.Marshal(r)
	if err != nil {
		return err
	}
	return atomicfile.Write(path, raw, 0o600)
}
```

**Step 4: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -count=1`
Expected: `ok  	pocketd/internal/factory`.

### Task 1.3: `checksNext`, rows 6 to 10

**Files:**
- Modify: `packages/pocketd/internal/factory/checks.go`
- Modify: `packages/pocketd/internal/factory/station.go` (`Next`, plan 3)
- Test: `packages/pocketd/internal/factory/checks_next_test.go`

**Context:** `Next(f Facts) (proto.Ticket, []Action)` is plan 3's (index, `station.go`). It switches on `f.Ticket.Station`. This task adds the arm for `proto.StationChecks`. Team states are plain strings in pocketd (`internal/proto/team.go`): `review`, `merging`, `merged`, `conflict`.

**Step 1: Write the failing test**

Create `checks_next_test.go`:

```go
package factory

import (
	"strings"
	"testing"

	"pocketd/internal/proto"
)

// checking is a Ticket at Checks whose lead is idle, changed by edit.
func checking(edit func(*Facts)) Facts {
	f := Facts{
		Ticket: proto.Ticket{ID: "t1", Title: "Fix it", State: proto.TicketLive, Status: proto.TicketWorking, Station: proto.StationChecks, AgentID: "lead"},
		Config: proto.FactoryConfig{Enabled: true, Fix: 2, Checks: []string{"make test"}},
		Lead:   Session{Present: true, Turn: "idle"},
	}
	edit(&f)
	return f
}

// did is the kinds of acts, joined. kinds is plan 3's helper in station_test.go.
func did(acts []Action) string { return strings.Join(kinds(acts), ",") }

func TestATeamInReviewIsMergedBeforeAnythingElse(t *testing.T) {
	got, acts := Next(checking(func(f *Facts) { f.Team = &proto.Team{Lead: "lead", State: "review"} }))
	if got.Status != proto.TicketWorking || got.Why != "Merging the worker branches" || did(acts) != "integrate" {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestATeamThatIsMergingIsWaitedFor(t *testing.T) {
	got, acts := Next(checking(func(f *Facts) { f.Team = &proto.Team{Lead: "lead", State: "merging"} }))
	if got.Status != proto.TicketWorking || len(acts) != 0 {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestAConflictFailsTheTicketWithItsFilesAndUsesNoAttempt(t *testing.T) {
	got, acts := Next(checking(func(f *Facts) {
		f.Team = &proto.Team{Lead: "lead", State: "conflict", Conflict: []string{"a.rs", "b.rs"}}
		f.Local = LocalChecks{Head: "aaa", Error: "Conflict in a.rs, b.rs"}
	}))
	if got.Status != proto.TicketFailed || got.Station != proto.StationChecks || got.Why != "Conflict in a.rs, b.rs" || got.Fix != 0 || len(acts) != 0 {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestATicketWithNoTeamSkipsTheMergeAndOpensItsPR(t *testing.T) {
	got, acts := Next(checking(func(f *Facts) {}))
	if got.Why != "Opening the draft PR" || did(acts) != "openPR" {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestAMergedTeamIsNotMergedAgain(t *testing.T) {
	_, acts := Next(checking(func(f *Facts) { f.Team = &proto.Team{Lead: "lead", State: "merged"} }))
	if did(acts) != "openPR" {
		t.Fatalf("got %v", acts)
	}
}

func TestAnOpenPRIsTakenAsTheTicketsAndOnlyPushedTo(t *testing.T) {
	got, acts := Next(checking(func(f *Facts) { f.PR = &proto.PR{Number: 57, State: "OPEN"} }))
	if got.PR != 57 || did(acts) != "openPR" {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestATicketWhosePRIsPushedRestsUntilTheChecksShip(t *testing.T) {
	got, acts := Next(checking(func(f *Facts) {
		f.Ticket.PR = 57
		f.Local = LocalChecks{Head: "aaa", Pushed: true}
	}))
	if got.Status != proto.TicketWorking || got.Why != "Draft PR open" || len(acts) != 0 {
		t.Fatalf("got %+v %v", got, acts)
	}
}
```

**Step 2: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -run 'TestATeam|TestAConflict|TestATicketWith|TestAMerged|TestAnOpenPR|TestATicketWhosePR' -count=1`
Expected: FAIL. `TestATeamInReviewIsMergedBeforeAnythingElse` reports the Ticket unchanged and no action, because `Next` has no arm for Checks.

**Step 3: Implement**

In `station.go`, `Next` first returns a Ticket it does not handle, then fails a Ticket whose lead is gone, then calls the function of the station (plan 3 Task 1.1, plan 4 Task 4.3).

Before (after plan 4):

```go
	if t.Station != proto.StationSpec && t.Station != proto.StationPlan && t.Station != proto.StationImplement {
		return t, nil
	}
```

After:

```go
	if t.Station != proto.StationSpec && t.Station != proto.StationPlan && t.Station != proto.StationImplement && t.Station != proto.StationChecks {
		return t, nil
	}
```

Before (after plan 4):

```go
	switch t.Station {
	case proto.StationSpec:
		return spec(f)
	case proto.StationImplement:
		return implement(f), nil
	}
	return plan(f)
```

After:

```go
	switch t.Station {
	case proto.StationSpec:
		return spec(f)
	case proto.StationImplement:
		// A Ticket that enters Checks starts Checks in the same step, as Spec does for Plan.
		if f.Ticket = implement(f); f.Ticket.Station != proto.StationChecks {
			return f.Ticket, nil
		}
		return checksNext(f)
	case proto.StationChecks:
		return checksNext(f)
	}
	return plan(f)
```

Plan 4's `TestWorkThatIsReadyMovesTheTicketToChecks` (`implement_test.go` or `station_test.go`) asserts `len(acts) != 0` is false and then that a second `Next` leaves the Ticket alone. Both stop holding, because Checks now acts. Change its first condition to `did(acts) != "integrate"` and delete its last four lines, from `again, _ := Next(` to the closing brace of that `if`.

Add to `checks.go`:

```go
const (
	ActIntegrate  = "integrate"
	ActOpenPR     = "openPR"
	ActRunChecks  = "runChecks"
	ActSendFix    = "sendFix"
	ActCommitPush = "commitPush"
)

// checksNext is Next for a Ticket at Checks. The table in docs/plans/2026-11-12-light-factory-5-checks.md
// lists the rows in the order the stages below read them.
func checksNext(f Facts) (proto.Ticket, []Action) {
	for _, stage := range []func(*Facts) ([]Action, bool){faulted, integration, pullRequest} {
		if acts, done := stage(&f); done {
			return f.Ticket, acts
		}
	}
	acts, _ := work(&f, "Draft PR open")
	return f.Ticket, acts
}

func work(f *Facts, why string, acts ...Action) ([]Action, bool) {
	f.Ticket.Status, f.Ticket.Gate, f.Ticket.Why = proto.TicketWorking, "", why
	return acts, true
}

func fail(f *Facts, why string) ([]Action, bool) {
	f.Ticket.Status, f.Ticket.Gate, f.Ticket.Why = proto.TicketFailed, "", why
	return nil, true
}

// faulted is row 6.
func faulted(f *Facts) ([]Action, bool) {
	if f.Local.Error != "" {
		return fail(f, f.Local.Error)
	}
	return nil, false
}

// integration is rows 7 and 8.
func integration(f *Facts) ([]Action, bool) {
	if f.Team == nil {
		return nil, false
	}
	switch f.Team.State {
	case "review", "conflict":
		return work(f, "Merging the worker branches", Action{Kind: ActIntegrate})
	case "merging":
		return work(f, "Merging the worker branches")
	}
	return nil, false
}

// pullRequest is rows 9 and 10.
func pullRequest(f *Facts) ([]Action, bool) {
	if f.Ticket.PR == 0 && f.PR != nil && f.PR.State == "OPEN" {
		f.Ticket.PR = f.PR.Number
	}
	if f.Ticket.PR == 0 || !f.Local.Pushed {
		return work(f, "Opening the draft PR", Action{Kind: ActOpenPR})
	}
	return nil, false
}
```

**Step 4: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -count=1`
Expected: `ok  	pocketd/internal/factory`.

### Task 1.4: The PR body and the Ticket's key

**Files:**
- Modify: `packages/pocketd/internal/factory/checks.go`
- Test: `packages/pocketd/internal/factory/checks_test.go`

**Context:** The index says: "The body is the spec's brief and acceptance criteria, then `Ticket <key>`." The spec is the markdown that plan 3 saves to `state/factory/<ticket>/spec.md`, with the headings `## Brief`, `## Acceptance criteria` and `## Risks`. With `spec: false` there is no file.

**Step 1: Write the failing test**

Add to `checks_test.go`:

```go
func TestThePRBodyIsTheBriefAndTheCriteriaThenTheKey(t *testing.T) {
	spec := "## Brief\nAdd the flag.\n\n## Acceptance criteria\n- [ ] The flag exists\n- [ ] It is off by default\n\n## Risks\n- None\n"
	want := "## Brief\nAdd the flag.\n\n## Acceptance criteria\n- [ ] The flag exists\n- [ ] It is off by default\n\nTicket APP-12"
	if got := PRBody(spec, "APP-12"); got != want {
		t.Fatalf("got  %q\nwant %q", got, want)
	}
}

func TestATicketWithoutASpecHasOnlyItsKeyInThePRBody(t *testing.T) {
	if got := PRBody("", "APP-12"); got != "Ticket APP-12" {
		t.Fatalf("got %q", got)
	}
}

func TestATicketsKeyIsThreeLettersOfItsProjectAndItsNumber(t *testing.T) {
	for folder, want := range map[string]string{"/code/anywhere": "ANY-7", "/code/my-app": "MYA-7", "/code/x": "X-7"} {
		if got := TicketKey(proto.Ticket{Folder: folder, Number: 7}); got != want {
			t.Fatalf("%s: got %s, want %s", folder, got, want)
		}
	}
}
```

**Step 2: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -run 'PRBody|TicketsKey' -count=1`
Expected: FAIL to build with `undefined: PRBody`.

**Step 3: Implement**

Add to `checks.go` (new imports: `strings`, `unicode`):

```go
// PRBody is the draft PR's body: the spec's Brief and Acceptance criteria, then the Ticket's key.
func PRBody(spec, key string) string {
	var parts []string
	for _, heading := range []string{"## Brief", "## Acceptance criteria"} {
		if s := section(spec, heading); s != "" {
			parts = append(parts, s)
		}
	}
	return strings.Join(append(parts, "Ticket "+key), "\n\n")
}

// section is heading's lines in spec, up to the next "## " heading.
func section(spec, heading string) string {
	var out []string
	for _, line := range strings.Split(spec, "\n") {
		switch {
		case strings.TrimSpace(line) == heading:
			out = []string{heading}
		case out != nil && strings.HasPrefix(line, "## "):
			return strings.TrimSpace(strings.Join(out, "\n"))
		case out != nil:
			out = append(out, line)
		}
	}
	return strings.TrimSpace(strings.Join(out, "\n"))
}

// TicketKey is the key the Board shows for t: three letters or digits of its project's name, then its number.
func TicketKey(t proto.Ticket) string {
	var prefix []rune
	for _, r := range filepath.Base(t.Folder) {
		if len(prefix) < 3 && (unicode.IsLetter(r) || unicode.IsDigit(r)) {
			prefix = append(prefix, unicode.ToUpper(r))
		}
	}
	return fmt.Sprintf("%s-%d", string(prefix), t.Number)
}
```

**Step 4: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -count=1`
Expected: `ok  	pocketd/internal/factory`.

### Task 1.5: The engine merges, pushes and opens the draft PR

**Files:**
- Modify: `packages/pocketd/internal/factory/engine.go` (plan 3: the `Engine` struct, `Run`, `event`, `handle`, `facts`, `act`)
- Modify: `packages/pocketd/internal/factory/checks.go`
- Modify: `packages/pocketd/cmd/pocketd/serve.go` (where plan 3 builds the `factory.Engine`)
- Test: `packages/pocketd/internal/factory/checks_engine_test.go`

**Context:** Plan 3 adds the engine's queue. This task adds one more kind of event, a function to run on the engine's goroutine, so that work done on another goroutine can come back. `Engine.GitHub *github.Service` is declared by the index for this plan. `Teams.Merge(lead)` starts the merge and returns. The result arrives through `Engine.TeamChanged`.

**Step 1: Write the failing test**

Create `checks_engine_test.go`. The rig below is used by every engine test of this plan.

```go
//go:build unix

package factory

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/board"
	"pocketd/internal/github"
	"pocketd/internal/hub"
	"pocketd/internal/proto"
	"pocketd/internal/registry"
	"pocketd/internal/team"
)

// rig is an engine with one live Ticket at Checks, in a worktree with a remote, and the fake gh of internal/github.
// Its lead is plan 3's chat with plan 3's leadID (engine_test.go).
type rig struct {
	t                         *testing.T
	e                         *Engine
	teams                     *team.Store
	home, project, tree, fake string
	ticket                    string
	mail                      chan string
}

func gitIn(t *testing.T, dir string, args ...string) string {
	t.Helper()
	out, err := exec.Command("git", append([]string{"-C", dir}, args...)...).CombinedOutput()
	if err != nil {
		t.Fatalf("git %v: %v\n%s", args, err, out)
	}
	return strings.TrimSpace(string(out))
}

func newRig(t *testing.T, cfg proto.FactoryConfig) *rig {
	t.Helper()
	for k, v := range map[string]string{"GIT_CONFIG_GLOBAL": "/dev/null", "GIT_AUTHOR_NAME": "T", "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "T", "GIT_COMMITTER_EMAIL": "t@t"} {
		t.Setenv(k, v)
	}
	root, _ := filepath.EvalSymlinks(t.TempDir())
	r := &rig{t: t, home: filepath.Join(root, "home"), project: filepath.Join(root, "app"), tree: filepath.Join(root, "app-fix"), fake: filepath.Join(root, "gh"), mail: make(chan string, 16)}
	for _, dir := range []string{r.home, r.project, r.fake} {
		os.MkdirAll(dir, 0o755)
	}
	gitIn(t, r.project, "init", "-q", "-b", "main")
	gitIn(t, r.project, "commit", "-q", "--allow-empty", "-m", "first")
	gitIn(t, root, "init", "-q", "--bare", "origin.git")
	gitIn(t, r.project, "remote", "add", "origin", filepath.Join(root, "origin.git"))
	gitIn(t, r.project, "worktree", "add", "-q", "-b", "fix", r.tree)
	gitIn(t, r.tree, "commit", "-q", "--allow-empty", "-m", "Fix it")
	reg := filepath.Join(root, "desktop.json")
	os.WriteFile(reg, []byte(fmt.Sprintf(`{"projects":[%q]}`, r.project)), 0o600)

	r.put("version", "gh version 2.81.0 (2025-10-01)\n")
	r.put("auth", "github.com\n  ✓ Logged in to github.com account t (keyring)\n  - Active account: true\n")
	r.put("nopr", "")
	r.put("created", "https://github.com/acme/app/pull/57\n")
	r.put("threads.json", `{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[]}}}}}`)
	r.view(`[{"__typename":"CheckRun","name":"ci","status":"COMPLETED","conclusion":"SUCCESS"}]`)
	bin, _ := filepath.Abs(filepath.Join("..", "github", "testdata", "fakegh"))
	env := func(string) []string {
		return []string{"PATH=" + bin + ":/usr/bin:/bin", "FAKEGH=" + r.fake, "HOME=" + r.fake, "GIT_CONFIG_GLOBAL=/dev/null", "GH_TOKEN=tok"}
	}

	store := board.Open(r.home)
	saved, err := store.Save(proto.Ticket{Title: "Fix it", Prompt: "Fix it.", Provider: "claude", Folder: r.project}, false, time.Now())
	if err != nil {
		t.Fatal(err)
	}
	r.ticket = saved.ID
	store.Update(saved.ID, func(x *proto.Ticket) {
		x.State, x.Status, x.Station, x.AgentID, x.Worktree = proto.TicketLive, proto.TicketWorking, proto.StationChecks, leadID, r.tree
	})
	configs := OpenConfigs(r.home)
	full := proto.DefaultFactoryConfig()
	full.Enabled, full.Fix, full.Checks = true, cfg.Fix, cfg.Checks
	if err := configs.Save(r.project, full); err != nil {
		t.Fatal(err)
	}
	h := hub.New()
	agents := agent.NewRegistry(h)
	agents.Add(leadID, r.tree, "claude", &chat{})
	r.teams, err = team.OpenStore(r.home)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { r.teams.Close() })
	r.e = &Engine{
		Home: r.home, Board: store, Configs: configs, Agents: agents, Now: time.Now, Poll: time.Millisecond,
		Teams:  &team.Teams{Store: r.teams, Agents: agents, Hub: h, Poll: time.Millisecond},
		Mail:   func(_, text string) error { r.mail <- text; return nil },
		GitHub: github.New(env, registry.New(reg)),
		Env:    env,
	}
	ctx, cancel := context.WithCancel(context.Background())
	t.Cleanup(cancel)
	go r.e.Run(ctx)
	return r
}

func (r *rig) put(name, content string) {
	r.t.Helper()
	if err := os.WriteFile(filepath.Join(r.fake, name), []byte(content), 0o644); err != nil {
		r.t.Fatal(err)
	}
}

// view sets what gh pr view reports for the open PR: its checks, on the commit of the checkout.
func (r *rig) view(rollup string) {
	r.put("view.json", `{"isDraft":true,"number":57,"state":"OPEN","title":"Fix it","url":"https://github.com/acme/app/pull/57","headRefOid":"@HEAD@","baseRefName":"main","mergeable":"MERGEABLE","statusCheckRollup":`+rollup+`}`)
}

func (r *rig) called(prefix string) bool {
	raw, _ := os.ReadFile(filepath.Join(r.fake, "calls"))
	for _, c := range strings.Split(strings.TrimSpace(string(raw)), "\n") {
		if _, args, _ := strings.Cut(c, " "); strings.HasPrefix(args, prefix) {
			return true
		}
	}
	return false
}

func (r *rig) get() proto.Ticket {
	tickets, _ := r.e.Board.Snapshot()
	for _, x := range tickets {
		if x.ID == r.ticket {
			return x
		}
	}
	r.t.Fatal("the ticket is gone")
	return proto.Ticket{}
}

func (r *rig) until(what string, ok func(proto.Ticket) bool) proto.Ticket {
	r.t.Helper()
	for end := time.Now().Add(15 * time.Second); time.Now().Before(end); time.Sleep(10 * time.Millisecond) {
		if x := r.get(); ok(x) {
			return x
		}
	}
	r.t.Fatalf("timed out waiting for %s: %+v", what, r.get())
	return proto.Ticket{}
}

func TestATicketAtChecksGetsADraftPRWithItsSpecAndItsKey(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Enabled: true, Fix: 2})
	dir := filepath.Join(r.home, "state", "factory", r.ticket)
	os.MkdirAll(dir, 0o700)
	os.WriteFile(filepath.Join(dir, "spec.md"), []byte("## Brief\nAdd the flag.\n\n## Acceptance criteria\n- [ ] The flag exists\n\n## Risks\n- None\n"), 0o600)
	r.e.Recover()
	got := r.until("the draft PR", func(x proto.Ticket) bool { return x.PR == 57 })
	if !r.called("pr create --title Fix it --body-file - --draft") {
		t.Fatal("gh pr create was not called for a draft with the ticket's title")
	}
	body, _ := os.ReadFile(filepath.Join(r.fake, "body"))
	if want := fmt.Sprintf("## Brief\nAdd the flag.\n\n## Acceptance criteria\n- [ ] The flag exists\n\nTicket APP-%d", got.Number); string(body) != want {
		t.Fatalf("body\n%s\nwant\n%s", body, want)
	}
	if remote, local := gitIn(t, filepath.Join(filepath.Dir(r.tree), "origin.git"), "rev-parse", "fix"), gitIn(t, r.tree, "rev-parse", "HEAD"); remote != local {
		t.Fatalf("the remote has %s, the checkout has %s", remote, local)
	}
	events, _ := ReadActivity(r.home, r.ticket)
	if !slices.ContainsFunc(events, func(ev proto.FactoryEvent) bool { return ev.Kind == "check" && ev.Actor == "anywhere" && ev.Text == "Opened draft PR #57" }) {
		t.Fatalf("activity %+v", events)
	}
}

func TestAMergeConflictFailsTheTicketWithTheFilesAndUsesNoAttempt(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Fix: 2})
	tm := proto.Team{Lead: leadID, State: "conflict", Conflict: []string{"a.rs", "b.rs"}, Tasks: make([]proto.TeamTask, 2)}
	if err := r.teams.Put(tm); err != nil {
		t.Fatal(err)
	}
	r.e.TeamChanged(tm)
	got := r.until("the failure", func(x proto.Ticket) bool { return x.Status == proto.TicketFailed })
	if got.Station != proto.StationChecks || got.Why != "Conflict in a.rs, b.rs" || got.Fix != 0 || got.PR != 0 {
		t.Fatalf("got %+v", got)
	}
	if r.called("pr create") {
		t.Fatal("a PR was opened for a branch that did not merge")
	}
}

func TestAPushThatFailsFailsTheTicketWithoutAnAttempt(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Enabled: true, Fix: 2})
	gitIn(t, r.project, "remote", "set-url", "origin", filepath.Join(r.home, "nowhere.git"))
	r.e.Recover()
	got := r.until("the failure", func(x proto.Ticket) bool { return x.Status == proto.TicketFailed })
	if got.Station != proto.StationChecks || got.Fix != 0 || !strings.HasPrefix(got.Why, "Couldn't open the pull request") {
		t.Fatalf("got %+v", got)
	}
}
```

**Step 2: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -run 'TestATicketAtChecksGets|TestAPushThatFails' -count=1`
Expected: FAIL to build with `unknown field Env in struct literal of type Engine`.

**Step 3: Implement**

In `engine.go`, add to the `Engine` struct after `GitHub`:

```go
	// Env is the login environment a check command runs with: the function github.New gets.
	Env func(dir string) []string

	// Only Run's goroutine reads and writes the fields below.
	ctx    context.Context
	flight map[string]bool      // Tickets with a push or a check command on another goroutine
	prs    map[string]*proto.PR // the newest PR of each worktree
```

At the top of `Run`, before its loop:

```go
	e.ctx, e.flight, e.prs = ctx, map[string]bool{}, map[string]*proto.PR{}
```

Add the field `do func() // a function to run on Run's goroutine` to plan 3's `event` struct, and make it the first lines of `handle`:

```go
	if ev.do != nil {
		ev.do()
		return
	}
```

`TeamChanged` queues the Team's result before the step.

Before (plan 3, `engine.go`):

```go
func (e *Engine) TeamChanged(t proto.Team) {
	if e != nil {
		e.post(event{agent: t.Lead})
	}
}
```

After:

```go
func (e *Engine) TeamChanged(t proto.Team) {
	if e != nil {
		e.later(func() { e.integrated(t) })
		e.post(event{agent: t.Lead})
	}
}
```

`step` saves and compares only the fields that plan 3's `Next` changed. Checks also changes `PR`, `Fix` and `Hint`. In `step`, inside the function passed to `e.update`, after the line that sets `x.Station, x.Gate, x.Status, x.Why`, add:

```go
			x.PR, x.Fix, x.Hint = next.PR, next.Fix, next.Hint
```

In `same`, add to the expression: `&& a.PR == b.PR && a.Fix == b.Fix && a.Hint == b.Hint`. If plan 4 or a later edit already did either, leave it.

In `e.facts`, before its last `return f`, add:

```go
	f.Local = e.local(t)
	f.PR = e.prs[t.Worktree]
```

In `e.act`, add the arms:

```go
	case ActIntegrate:
		e.integrate(t)
	case ActOpenPR:
		e.openPR(t)
```

In `serve.go`, where plan 3 builds the `factory.Engine`, set the two fields. `ghEnv` is the function that `github.New` gets at `serve.go:261`. Move it into a variable above both uses:

```go
	ghEnv := func(dir string) []string { return d.GhEnv(d.LoginEnv(), dir) }
	// ...
	fac.GitHub, fac.Env = gh, ghEnv // gh is the *github.Service that ws.GitHub also gets
```

Add to `checks.go` (new imports: `context`, `log`, `time`, `pocketd/internal/worktree`):

```go
// later runs do on Run's goroutine, after what is queued.
func (e *Engine) later(do func()) { e.post(event{do: do}) }

func (e *Engine) say(ticket, actor, kind, text string) {
	e.note(ticket, proto.FactoryEvent{At: e.now().UnixMilli(), Station: proto.StationChecks, Actor: actor, Kind: kind, Text: text})
}

// atChecks is the live Tickets at Checks.
func (e *Engine) atChecks() []proto.Ticket {
	all, _ := e.Board.Snapshot()
	var out []proto.Ticket
	for _, t := range all {
		if t.State == proto.TicketLive && t.Station == proto.StationChecks {
			out = append(out, t)
		}
	}
	return out
}

// current is the record of the commit in t's checkout: the newest one when it is for that commit or holds a fix
// note or a hint, else a new one.
func (e *Engine) current(t proto.Ticket) (rec Record, fresh bool) {
	head, _ := worktree.Head(t.Worktree)
	rec, ok := ReadRecord(e.Home, t.ID)
	if !ok || (rec.Head != head && rec.Fix == "" && rec.Hint == "") {
		return Record{Head: head}, true
	}
	return rec, false
}

func (e *Engine) save(ticket string, rec Record, fresh bool) {
	if err := WriteRecord(e.Home, ticket, rec, fresh); err != nil {
		log.Printf("factory: the check record of %s: %v", ticket, err)
	}
}

func (e *Engine) local(t proto.Ticket) LocalChecks {
	rec, fresh := e.current(t)
	if fresh {
		return LocalChecks{Head: rec.Head}
	}
	l := LocalChecks{Head: rec.Head, Pushed: rec.Pushed != 0, State: rec.State, Done: len(rec.Checks), Total: rec.Total, Fix: rec.Fix, Hint: rec.Hint != "", Same: rec.Same, Error: rec.Error, Quiet: rec.Quiet}
	if l.State == LocalRunning && !e.flight[t.ID] {
		// pocketd stopped while a command ran: run them again (E1).
		l.State, l.Done = "", 0
	}
	if n := len(rec.Checks); l.State == LocalFailed && n > 0 {
		l.Failed = rec.Checks[n-1].Command
	}
	return l
}

// fault records a failure that is not a failed check.
func (e *Engine) fault(t proto.Ticket, why string) {
	rec, fresh := e.current(t)
	if rec.Error == why {
		return
	}
	rec.Error = why
	e.save(t.ID, rec, fresh)
}

// integrate merges the Team's branches into the lead's checkout. The result comes back through TeamChanged.
func (e *Engine) integrate(t proto.Ticket) {
	again := func(why string) {
		e.fault(t, why)
		e.post(event{ticket: t.ID})
	}
	if open, _ := worktree.MergeInProgress(t.Worktree); open {
		again("A merge is still open in the checkout")
		return
	}
	if dirty, _ := worktree.Dirty(t.Worktree); dirty {
		if _, err := worktree.Commit(t.Worktree, "Lead's changes before the merge"); err != nil {
			again("Couldn't commit the lead's changes: " + err.Error())
			return
		}
	}
	e.say(t.ID, "anywhere", "check", "Merging the worker branches")
	if err := e.Teams.Merge(t.AgentID); err != nil {
		again("Couldn't merge the worker branches: " + err.Error())
	}
}

// integrated records how the merge of tm ended for its Ticket at Checks.
func (e *Engine) integrated(tm proto.Team) {
	for _, t := range e.atChecks() {
		switch {
		case t.AgentID != tm.Lead:
		case tm.State == "conflict" && len(tm.Conflict) > 0:
			e.fault(t, "Conflict in "+strings.Join(tm.Conflict, ", "))
		case tm.State == "review" && tm.Error != "":
			e.fault(t, "Couldn't merge the worker branches: "+tm.Error)
		}
	}
}

// openPR pushes t's branch and opens its draft PR, or only pushes when the PR exists.
func (e *Engine) openPR(t proto.Ticket) {
	if e.flight[t.ID] {
		return
	}
	if e.GitHub == nil {
		e.fault(t, "GitHub is not set up")
		e.post(event{ticket: t.ID})
		return
	}
	e.flight[t.ID] = true
	spec, _ := os.ReadFile(filepath.Join(e.Home, "state", "factory", t.ID, "spec.md"))
	body := PRBody(string(spec), TicketKey(t))
	go func() {
		ctx, cancel := context.WithTimeout(e.ctx, 5*time.Minute)
		defer cancel()
		head, _ := worktree.Head(t.Worktree)
		number, err := t.PR, error(nil)
		if number == 0 {
			number, _, err = e.GitHub.Create(ctx, t.Worktree, t.Title, body, t.Base, true)
			if err != nil {
				// The branch may have a PR that Anywhere did not open.
				if pr, _ := e.GitHub.PR(ctx, t.Worktree); pr != nil && pr.State == "OPEN" {
					number, err = pr.Number, e.GitHub.Push(ctx, t.Worktree)
				}
			}
		} else {
			err = e.GitHub.Push(ctx, t.Worktree)
		}
		e.later(func() { e.pushed(t, head, number, err) })
	}()
}

// pushed takes the result of openPR for the commit head.
func (e *Engine) pushed(t proto.Ticket, head string, number int, err error) {
	delete(e.flight, t.ID)
	rec, fresh := e.current(t)
	switch {
	case err != nil:
		rec.Error = "Couldn't open the pull request: " + err.Error()
		if t.PR != 0 {
			rec.Error = "Couldn't push the branch: " + err.Error()
		}
	case rec.Head != head:
		// The checkout moved while the push ran. The next step pushes again.
	case t.PR == 0:
		rec.Pushed = e.now().UnixMilli()
		if e.prs[t.Worktree] == nil {
			e.prs[t.Worktree] = &proto.PR{Number: number, State: "OPEN", Draft: true}
		}
		e.say(t.ID, "anywhere", "check", fmt.Sprintf("Opened draft PR #%d", number))
	default:
		rec.Pushed = e.now().UnixMilli()
		e.say(t.ID, "anywhere", "check", "Pushed "+head[:min(7, len(head))])
	}
	e.save(t.ID, rec, fresh)
	e.post(event{ticket: t.ID})
}
```

**Step 4: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -count=1`
Expected: `ok  	pocketd/internal/factory`.

### PR 1 checks

**Files:** none.

**Step 1: Run the checks**

Run: `cd packages/pocketd && go vet ./... && go test ./...`
Expected: every package `ok`. No new failure.

**Step 2: Commit**

```bash
git add packages/pocketd
git commit -m "Checks merges the worker branches and opens the draft PR"
```

The red-team gate is not needed: this PR changes neither `peer.Needs` nor the broker. The TypeScript and Swift golden suites read nothing this PR adds.

---

## PR 2: Local commands and GitHub checks

**Scope:** `Tail` and the command runner. Rows 11 to 21 of the table, with every fix row as a plain failure. `Engine.PRChanged`, the watch on the PR cache, the 90 second wait, and `Checks` and `NoChecks` in `Engine.Detail`.

**Depends on:** PR 1.

**Done when:** a Ticket at Checks runs its commands, waits for GitHub, and moves to `Station: pr` with `Status: working`. A failed command or a failed GitHub check fails the Ticket with the name of what failed in `Why`. `factory.ticket` returns the commands of the newest record. Nothing is mailed to the lead until PR 3. A Ticket at `Station: pr` rests until plan 6.

### Task 2.1: `Tail` keeps the end of an output

**Files:**
- Modify: `packages/pocketd/internal/factory/checks.go`
- Test: `packages/pocketd/internal/factory/checks_test.go`

**Context:** `hostop.Run` keeps the first 64 MB of each stream (`internal/hostop/run.go:16`). The index wants the last 64 KB. `Tail` cuts at a line start so the first line shown is whole, and never inside a UTF-8 sequence.

**Step 1: Write the failing test**

Add to `checks_test.go` (new imports: `strings`, `unicode/utf8`):

```go
func TestAShortOutputIsKeptWhole(t *testing.T) {
	if got := Tail([]byte("one\ntwo\n"), 64); got != "one\ntwo\n" {
		t.Fatalf("got %q", got)
	}
}

func TestALongOutputKeepsItsLastLinesFromALineStart(t *testing.T) {
	out := strings.Repeat("0123456789\n", 10)
	got := Tail([]byte(out), 25)
	if got != "0123456789\n0123456789\n" {
		t.Fatalf("got %q", got)
	}
}

func TestOneLineLongerThanTheLimitIsCutToValidText(t *testing.T) {
	got := Tail([]byte(strings.Repeat("é", 100)), 15)
	if !utf8.ValidString(got) || len(got) > 15 || len(got) < 13 {
		t.Fatalf("got %q (%d bytes)", got, len(got))
	}
}
```

**Step 2: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -run 'Output|OneLine' -count=1`
Expected: FAIL to build with `undefined: Tail`.

**Step 3: Implement**

Add to `checks.go` (new imports: `bytes`, `unicode/utf8`):

```go
// OutputLimit is how much of a check command's output is kept.
const OutputLimit = 64 << 10

// Tail is the last limit bytes of out, from a line start when one is inside them.
func Tail(out []byte, limit int) string {
	if len(out) <= limit {
		return string(out)
	}
	out = out[len(out)-limit:]
	if i := bytes.IndexByte(out, '\n'); i >= 0 && i < len(out)-1 {
		return string(out[i+1:])
	}
	for len(out) > 0 && !utf8.RuneStart(out[0]) {
		out = out[1:]
	}
	return string(out)
}
```

**Step 4: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -run 'Output|OneLine' -count=1`
Expected: `ok  	pocketd/internal/factory`.

### Task 2.2: `RunCheck` runs one command in the checkout

**Files:**
- Modify: `packages/pocketd/internal/factory/checks.go`
- Test: `packages/pocketd/internal/factory/checks_test.go`

**Context:** `hostop.Run(ctx, env, cwd, argv, stdin, timeout)` (`internal/hostop/run.go:20`) finds `argv[0]` on the `PATH` of `env`, and returns code -1 when the timeout or `ctx` ends the command. `exec 2>&1` puts both streams in one output in the order they were written. The tests run real commands (US-24).

**Step 1: Write the failing test**

Add to `checks_test.go` (new imports: `context`, `time`):

```go
var plain = []string{"PATH=/usr/bin:/bin", "GREETING=hello"}

func TestACommandThatExitsZeroPasses(t *testing.T) {
	if exit, out := RunCheck(context.Background(), plain, t.TempDir(), "true", time.Minute); exit != 0 || out != "" {
		t.Fatalf("got %d %q", exit, out)
	}
}

func TestACommandThatExitsOneFails(t *testing.T) {
	if exit, _ := RunCheck(context.Background(), plain, t.TempDir(), "false", time.Minute); exit != 1 {
		t.Fatalf("got %d", exit)
	}
}

func TestACommandsExitCodeAndOutputAreKept(t *testing.T) {
	exit, out := RunCheck(context.Background(), plain, t.TempDir(), "sh -c 'echo x; exit 3'", time.Minute)
	if exit != 3 || out != "x\n" {
		t.Fatalf("got %d %q", exit, out)
	}
}

func TestStderrIsKeptWithStdoutInTheOrderWritten(t *testing.T) {
	_, out := RunCheck(context.Background(), plain, t.TempDir(), "echo one; echo two >&2; echo three", time.Minute)
	if out != "one\ntwo\nthree\n" {
		t.Fatalf("got %q", out)
	}
}

func TestACommandRunsInTheCheckoutWithTheEnvironmentItIsGiven(t *testing.T) {
	dir, _ := filepath.EvalSymlinks(t.TempDir())
	_, out := RunCheck(context.Background(), plain, dir, "pwd; echo $GREETING", time.Minute)
	if out != dir+"\nhello\n" {
		t.Fatalf("got %q", out)
	}
}

func TestACommandThatRunsTooLongIsStoppedAndFails(t *testing.T) {
	exit, out := RunCheck(context.Background(), plain, t.TempDir(), "echo started; exec sleep 30", 200*time.Millisecond)
	if exit != -1 || !strings.HasPrefix(out, "started\n") || !strings.HasSuffix(out, "Stopped after 200ms.\n") {
		t.Fatalf("got %d %q", exit, out)
	}
}

func TestOnlyTheEndOfALongOutputIsKept(t *testing.T) {
	_, out := RunCheck(context.Background(), plain, t.TempDir(), "i=0; while [ $i -lt 9000 ]; do echo 0123456789; i=$((i+1)); done; echo last", time.Minute)
	if len(out) > OutputLimit || !strings.HasSuffix(out, "0123456789\nlast\n") || !strings.HasPrefix(out, "0123456789\n") {
		t.Fatalf("kept %d bytes", len(out))
	}
}
```

**Step 2: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -run 'Command|Stderr|LongOutput' -count=1`
Expected: FAIL to build with `undefined: RunCheck`.

**Step 3: Implement**

Add to `checks.go` (new import: `pocketd/internal/hostop`):

```go
// CheckLimit is how long one check command may run.
const CheckLimit = 30 * time.Minute

// RunCheck runs command with sh in dir and returns its exit code and the end of its output. The code is -1 when
// the command could not start, ran longer than limit, or ctx ended it.
func RunCheck(ctx context.Context, env []string, dir, command string, limit time.Duration) (int, string) {
	code, out, _, err := hostop.Run(ctx, env, dir, []string{"sh", "-c", "exec 2>&1\n" + command}, nil, limit)
	switch {
	case err != nil:
		return -1, "Couldn't run the command: " + err.Error() + "\n"
	case code == -1 && ctx.Err() == nil:
		return -1, Tail(out, OutputLimit) + fmt.Sprintf("Stopped after %s.\n", limit)
	}
	return code, Tail(out, OutputLimit)
}
```

**Step 4: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -run 'Command|Stderr|LongOutput' -count=1`
Expected: `ok  	pocketd/internal/factory`. The slowest test takes about a second.

### Task 2.3: `checksNext`, rows 11 to 21

**Files:**
- Modify: `packages/pocketd/internal/factory/checks.go`
- Test: `packages/pocketd/internal/factory/checks_next_test.go`

**Context:** C4 says when the station passes: every local command exited 0, and every GitHub check passed or was skipped. `proto.PR` (`internal/proto/gh.go`) has `State` (`OPEN`, `MERGED`, `CLOSED`), `Head` (the commit GitHub has), `Mergeable` (`CONFLICTING` when it conflicts with its base), `Checks{Passed, Failed, Pending}` and `Runs []CheckRun{Name, Outcome, URL}`. A skipped check is in `Runs` and in none of the three counts. In this PR a failure fails the Ticket. PR 3 replaces `broken` with the fix rows.

**Step 1: Write the failing test**

In `checks_next_test.go`, delete `TestATicketWhosePRIsPushedRestsUntilTheChecksShip` (Task 1.3). Its Ticket now runs its commands. Add:

```go
// pushed is a Ticket at Checks whose draft PR is open and pushed, changed by edit.
func pushed(edit func(*Facts)) Facts {
	return checking(func(f *Facts) {
		f.Ticket.PR = 57
		f.Local = LocalChecks{Head: "aaa", Pushed: true}
		edit(f)
	})
}

// green is the PR of commit aaa with one passed check.
func green() *proto.PR {
	return &proto.PR{Number: 57, State: "OPEN", Draft: true, Base: "main", Head: "aaa", Mergeable: "MERGEABLE",
		Checks: proto.Checks{Passed: 1}, Runs: []proto.CheckRun{{Name: "ci", Outcome: "passed"}}}
}

func TestACommitWhoseCommandsHaveNotRunRunsThem(t *testing.T) {
	got, acts := Next(pushed(func(f *Facts) {}))
	if got.Status != proto.TicketWorking || got.Why != "Checks · 0/1" || did(acts) != "runChecks" {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestRunningCommandsShowHowManyAreDone(t *testing.T) {
	got, acts := Next(pushed(func(f *Facts) { f.Local.State, f.Local.Done, f.Local.Total = LocalRunning, 2, 3 }))
	if got.Why != "Checks · 2/3" || len(acts) != 0 {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestAFailedCommandFailsTheTicketAtChecks(t *testing.T) {
	got, acts := Next(pushed(func(f *Facts) {
		f.Config.Fix = 0
		f.Local.State, f.Local.Failed = LocalFailed, "cargo test"
	}))
	if got.Status != proto.TicketFailed || got.Station != proto.StationChecks || !strings.HasPrefix(got.Why, "cargo test") || len(acts) != 0 {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestPassedCommandsWaitForGitHubToKnowTheCommit(t *testing.T) {
	for name, pr := range map[string]*proto.PR{"no PR yet": nil, "an older commit": {Number: 57, State: "OPEN", Head: "old"}} {
		got, acts := Next(pushed(func(f *Facts) { f.Local.State, f.PR = LocalPassed, pr }))
		if got.Status != proto.TicketWorking || got.Station != proto.StationChecks || got.Why != "Waiting for GitHub" || len(acts) != 0 {
			t.Fatalf("%s: got %+v %v", name, got, acts)
		}
	}
}

func TestPendingGitHubChecksAreWaitedFor(t *testing.T) {
	got, _ := Next(pushed(func(f *Facts) {
		f.Local.State, f.PR = LocalPassed, green()
		f.PR.Checks = proto.Checks{Passed: 1, Pending: 2}
	}))
	if got.Status != proto.TicketWorking || got.Station != proto.StationChecks || got.Why != "GitHub checks · 1/3" {
		t.Fatalf("got %+v", got)
	}
}

func TestAFailedGitHubCheckFailsTheTicketWithItsName(t *testing.T) {
	got, _ := Next(pushed(func(f *Facts) {
		f.Config.Fix = 0
		f.Local.State, f.PR = LocalPassed, green()
		f.PR.Checks = proto.Checks{Passed: 1, Failed: 1}
		f.PR.Runs = append(f.PR.Runs, proto.CheckRun{Name: "e2e", Outcome: "failed", URL: "https://x/1"})
	}))
	if got.Status != proto.TicketFailed || !strings.HasPrefix(got.Why, "e2e") {
		t.Fatalf("got %+v", got)
	}
}

func TestAPRThatConflictsWithItsBaseFailsTheChecks(t *testing.T) {
	got, _ := Next(pushed(func(f *Facts) {
		f.Config.Fix = 0
		f.Local.State, f.PR = LocalPassed, green()
		f.PR.Mergeable = "CONFLICTING"
	}))
	if got.Status != proto.TicketFailed || !strings.HasPrefix(got.Why, "Conflicts with main") {
		t.Fatalf("got %+v", got)
	}
}

func TestAClosedPRFailsTheTicket(t *testing.T) {
	got, _ := Next(pushed(func(f *Facts) {
		f.Local.State, f.PR = LocalPassed, green()
		f.PR.State = "CLOSED"
	}))
	if got.Status != proto.TicketFailed || got.Why != "The pull request was closed" {
		t.Fatalf("got %+v", got)
	}
}

func TestPassedCommandsAndPassedGitHubChecksMoveTheTicketToPRReview(t *testing.T) {
	got, acts := Next(pushed(func(f *Facts) {
		f.Ticket.Hint, f.Ticket.Fix = true, 1
		f.Local.State, f.PR = LocalPassed, green()
	}))
	if got.Station != proto.StationPR || got.Status != proto.TicketWorking || got.Why != "" || got.Hint || got.Fix != 1 || len(acts) != 0 {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestSkippedGitHubChecksCountAsPassed(t *testing.T) {
	got, _ := Next(pushed(func(f *Facts) {
		f.Local.State, f.PR = LocalPassed, green()
		f.PR.Checks, f.PR.Runs = proto.Checks{}, []proto.CheckRun{{Name: "label", Outcome: "skipped"}}
	}))
	if got.Station != proto.StationPR {
		t.Fatalf("got %+v", got)
	}
}

func TestWithNoGitHubCheckTheTicketWaitsUntilGitHubHadTimeToReportOne(t *testing.T) {
	none := func(f *Facts) {
		f.Local.State, f.PR = LocalPassed, green()
		f.PR.Checks, f.PR.Runs = proto.Checks{}, nil
	}
	got, _ := Next(pushed(none))
	if got.Station != proto.StationChecks || got.Status != proto.TicketWorking || got.Why != "Waiting for GitHub checks" {
		t.Fatalf("got %+v", got)
	}
	got, _ = Next(pushed(func(f *Facts) { none(f); f.Local.Quiet = true }))
	if got.Station != proto.StationPR {
		t.Fatalf("after the wait: got %+v", got)
	}
}

func TestATicketWithNoCommandsAndNoGitHubChecksStillPasses(t *testing.T) {
	got, _ := Next(pushed(func(f *Facts) {
		f.Config.Checks = nil
		f.Local.State, f.Local.Quiet, f.PR = LocalPassed, true, green()
		f.PR.Checks, f.PR.Runs = proto.Checks{}, nil
	}))
	if got.Station != proto.StationPR || got.Status != proto.TicketWorking {
		t.Fatalf("got %+v", got)
	}
}

func TestAFailedTicketWhoseGitHubChecksPassLaterPasses(t *testing.T) {
	got, _ := Next(pushed(func(f *Facts) {
		f.Ticket.Status, f.Ticket.Why, f.Ticket.Fix = proto.TicketFailed, "e2e · Auto-fix 2 of 2 used", 2
		f.Local.State, f.PR = LocalPassed, green()
	}))
	if got.Station != proto.StationPR || got.Status != proto.TicketWorking || got.Fix != 2 {
		t.Fatalf("got %+v", got)
	}
}
```

**Step 2: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -run 'TestACommit|TestRunning|TestAFailed|TestPassed|TestPending|TestAPRThat|TestAClosed|TestSkipped|TestWithNo|TestATicketWithNoCommands' -count=1`
Expected: FAIL. `TestACommitWhoseCommandsHaveNotRunRunsThem` reports `Why: "Draft PR open"` and no action.

**Step 3: Implement**

In `checks.go`, replace `checksNext`.

Before:

```go
func checksNext(f Facts) (proto.Ticket, []Action) {
	for _, stage := range []func(*Facts) ([]Action, bool){faulted, integration, pullRequest} {
		if acts, done := stage(&f); done {
			return f.Ticket, acts
		}
	}
	acts, _ := work(&f, "Draft PR open")
	return f.Ticket, acts
}
```

After:

```go
func checksNext(f Facts) (proto.Ticket, []Action) {
	for _, stage := range []func(*Facts) ([]Action, bool){faulted, integration, pullRequest, commands} {
		if acts, done := stage(&f); done {
			return f.Ticket, acts
		}
	}
	acts, _ := remote(&f)
	return f.Ticket, acts
}
```

Add below `pullRequest`:

```go
// commands is rows 11 to 13.
func commands(f *Facts) ([]Action, bool) {
	switch f.Local.State {
	case "":
		return work(f, fmt.Sprintf("Checks · 0/%d", len(f.Config.Checks)), Action{Kind: ActRunChecks})
	case LocalRunning:
		return work(f, fmt.Sprintf("Checks · %d/%d", f.Local.Done, f.Local.Total))
	case LocalFailed:
		return broken(f, f.Local.Failed)
	}
	return nil, false
}

// remote is rows 14 to 21.
func remote(f *Facts) ([]Action, bool) {
	pr := f.PR
	switch {
	case pr == nil:
		return work(f, "Waiting for GitHub")
	case pr.State == "CLOSED":
		return fail(f, "The pull request was closed")
	case pr.State == "OPEN" && pr.Head != f.Local.Head:
		return work(f, "Waiting for GitHub")
	case pr.Mergeable == "CONFLICTING":
		return broken(f, "Conflicts with "+pr.Base)
	case pr.Checks.Failed > 0:
		return broken(f, failedRun(pr))
	case pr.Checks.Pending > 0:
		return work(f, fmt.Sprintf("GitHub checks · %d/%d", pr.Checks.Passed, pr.Checks.Passed+pr.Checks.Failed+pr.Checks.Pending))
	case len(pr.Runs) == 0 && !f.Local.Quiet:
		return work(f, "Waiting for GitHub checks")
	}
	f.Ticket.Station, f.Ticket.Hint = proto.StationPR, false
	return work(f, "")
}

// failedRun is the name of pr's first failed check.
func failedRun(pr *proto.PR) string {
	for _, run := range pr.Runs {
		if run.Outcome == "failed" {
			return run.Name
		}
	}
	return "GitHub checks"
}

// broken is a failed check named what. PR 3 of the Checks plan sends it to the lead first.
func broken(f *Facts, what string) ([]Action, bool) {
	return fail(f, what)
}
```

**Step 4: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -count=1`
Expected: `ok  	pocketd/internal/factory`. `e.act` has no arm for `runChecks` until Task 2.4, so a Ticket of the engine tests rests at `Checks · 0/0`.

### Task 2.4: The engine runs the commands

**Files:**
- Modify: `packages/pocketd/internal/factory/checks.go`
- Modify: `packages/pocketd/internal/factory/engine.go` (`e.act`)
- Test: `packages/pocketd/internal/factory/checks_engine_test.go`

**Context:** The commands run on their own goroutine. Each result comes back with `e.later` (Task 1.5), so only `Run`'s goroutine writes the record. `e.flight` (Task 1.5) says that a goroutine works for the Ticket. `e.Env` may ask `gh` for a token, so it is called on the goroutine too. A result that arrives after pocketd began to stop is dropped: the record stays `running`, and `e.local` reads that as "nothing ran" after the restart (E1).

**Step 1: Write the failing test**

Add to `checks_engine_test.go`:

```go
// record is the Ticket's newest check record once ok holds.
func (r *rig) record(what string, ok func(Record) bool) Record {
	r.t.Helper()
	for end := time.Now().Add(15 * time.Second); time.Now().Before(end); time.Sleep(10 * time.Millisecond) {
		if rec, has := ReadRecord(r.home, r.ticket); has && ok(rec) {
			return rec
		}
	}
	rec, _ := ReadRecord(r.home, r.ticket)
	r.t.Fatalf("timed out waiting for %s: %+v", what, rec)
	return Record{}
}

func TestTheCommandsRunInOrderInTheCheckoutAndTheirOutputIsSaved(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Checks: []string{"echo one > first.txt", "cat first.txt; pwd"}})
	r.e.Recover()
	rec := r.record("both commands", func(rec Record) bool { return rec.State == LocalPassed })
	if rec.Total != 2 || len(rec.Checks) != 2 || rec.Checks[1].Exit != 0 || rec.Checks[1].Output != "one\n"+r.tree+"\n" || rec.Checks[1].Attempt != 0 || rec.Checks[1].At == 0 {
		t.Fatalf("got %+v", rec)
	}
	if rec.Head != gitIn(t, r.tree, "rev-parse", "HEAD") {
		t.Fatalf("the record is for %s", rec.Head)
	}
}

func TestTheCommandsStopAtTheFirstFailureAndTheTicketFails(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Checks: []string{"true", "sh -c 'echo boom; exit 3'", "touch never.txt"}})
	r.e.Recover()
	got := r.until("the failure", func(x proto.Ticket) bool { return x.Status == proto.TicketFailed })
	rec, _ := ReadRecord(r.home, r.ticket)
	if got.Station != proto.StationChecks || !strings.HasPrefix(got.Why, "sh -c 'echo boom; exit 3'") || rec.State != LocalFailed || len(rec.Checks) != 2 || rec.Checks[1].Exit != 3 || rec.Checks[1].Output != "boom\n" {
		t.Fatalf("got %+v\n%+v", got, rec)
	}
	if _, err := os.Stat(filepath.Join(r.tree, "never.txt")); err == nil {
		t.Fatal("the command after the failure ran")
	}
	events, _ := ReadActivity(r.home, r.ticket)
	var texts []string
	for _, ev := range events {
		if ev.Kind == "check" {
			texts = append(texts, ev.Text)
		}
	}
	if got := strings.Join(texts, " | "); !strings.HasSuffix(got, "Passed: true | Failed (exit 3): sh -c 'echo boom; exit 3'") {
		t.Fatalf("activity: %s", got)
	}
}

func TestACommandThatWasRunningWhenPocketdStoppedRunsAgain(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Checks: []string{"echo again >> ran.txt"}})
	head := gitIn(t, r.tree, "rev-parse", "HEAD")
	if err := WriteRecord(r.home, r.ticket, Record{Head: head, Pushed: time.Now().UnixMilli(), State: LocalRunning, Total: 1}, true); err != nil {
		t.Fatal(err)
	}
	r.e.Board.Update(r.ticket, func(x *proto.Ticket) { x.PR, x.Fix = 57, 1 })
	r.e.Recover()
	r.record("the command", func(rec Record) bool { return rec.State == LocalPassed })
	if got := r.get(); got.Fix != 1 {
		t.Fatalf("a restart changed the attempts: %+v", got)
	}
	if raw, _ := os.ReadFile(filepath.Join(r.tree, "ran.txt")); string(raw) != "again\n" {
		t.Fatalf("ran.txt: %q", raw)
	}
}
```

**Step 2: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -run 'TestTheCommands|TestACommandThatWasRunning' -count=1`
Expected: FAIL after 15 seconds each with `timed out waiting for both commands`, `the failure` and `the command`.

**Step 3: Implement**

In `e.act` (`engine.go`), add the arm:

```go
	case ActRunChecks:
		e.runChecks(t)
```

Add to `checks.go`:

```go
// runChecks runs the project's commands for the commit in t's checkout, one after another, and stops at the first
// failure.
func (e *Engine) runChecks(t proto.Ticket) {
	if e.flight[t.ID] {
		return
	}
	rec, fresh := e.current(t)
	cmds := e.Configs.Get(t.Folder).Checks
	rec.State, rec.Total, rec.Checks = LocalRunning, len(cmds), nil
	if len(cmds) == 0 {
		rec.State = LocalPassed
		e.save(t.ID, rec, fresh)
		e.post(event{ticket: t.ID})
		return
	}
	e.save(t.ID, rec, fresh)
	e.flight[t.ID] = true
	head := rec.Head
	go func() {
		env := e.Env(t.Worktree)
		for _, command := range cmds {
			exit, output := RunCheck(e.ctx, env, t.Worktree, command, CheckLimit)
			if e.ctx.Err() != nil {
				return
			}
			check := proto.FactoryCheck{Command: command, Exit: exit, Output: output, Attempt: t.Fix, At: e.now().UnixMilli()}
			e.later(func() { e.checked(t.ID, head, check) })
			if exit != 0 {
				break
			}
		}
		e.later(func() {
			delete(e.flight, t.ID)
			e.post(event{ticket: t.ID})
		})
	}()
}

// checked takes one command's result for the commit head.
func (e *Engine) checked(ticket, head string, c proto.FactoryCheck) {
	rec, ok := ReadRecord(e.Home, ticket)
	if !ok || rec.Head != head || rec.State != LocalRunning {
		return
	}
	rec.Checks = append(rec.Checks, c)
	text := "Passed: " + c.Command
	switch {
	case c.Exit != 0:
		rec.State, text = LocalFailed, fmt.Sprintf("Failed (exit %d): %s", c.Exit, c.Command)
	case len(rec.Checks) == rec.Total:
		rec.State = LocalPassed
	}
	e.save(ticket, rec, false)
	e.say(ticket, "anywhere", "check", text)
	e.post(event{ticket: ticket})
}
```

**Step 4: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -count=1`
Expected: `ok  	pocketd/internal/factory`.

### Task 2.5: `Engine.PRChanged`, the watch, and the wait for GitHub's checks

**Files:**
- Modify: `packages/pocketd/internal/github/service.go:385` (`Create`)
- Modify: `packages/pocketd/internal/factory/checks.go`
- Modify: `packages/pocketd/internal/factory/engine.go` (the struct, `Run`, `drain`)
- Test: `packages/pocketd/internal/github/service_test.go`, `packages/pocketd/internal/factory/checks_engine_test.go`

**Context:** `github.Service.Listen(key, changed)` starts the poller and calls `changed(tree)` on its own goroutine when the PR of a watched tree changes. `Watch(key, trees)` sets the trees. `Leave(key)` stops. A watched tree is asked every 30 seconds (`github.WatchedEvery`). `Service.PR(ctx, tree)` returns the cached PR while it is fresh. `Create` caches the new PR without its head commit and without its checks, and that entry stays fresh for 30 seconds. This task makes `Create` expire the entry, as `after` does for every other verb, so the next read asks `gh`. The engine keeps the newest PR of each worktree in `e.prs`, because `Next` must not call `gh`.

**Step 1: Write the failing test**

Add to `internal/github/service_test.go`:

```go
func TestAfterCreateThePRIsAskedOfGhAgainSoItsChecksShow(t *testing.T) {
	f := newFake(t)
	f.put("created", "https://github.com/acme/app/pull/57\n")
	f.put("threads.json", `{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[]}}}}}`)
	f.put("view.json", `{"isDraft":true,"number":57,"state":"OPEN","title":"Fix it","url":"https://github.com/acme/app/pull/57","headRefOid":"@HEAD@","baseRefName":"main","statusCheckRollup":[{"__typename":"CheckRun","name":"ci","status":"IN_PROGRESS","conclusion":""}]}`)
	_, tree, reg := repo(t, true)
	s := New(f.env(true), reg)
	if _, _, err := s.Create(ctx(t), tree, "Fix it", "", "", true); err != nil {
		t.Fatal(err)
	}
	pr, _ := s.PR(ctx(t), tree)
	if pr == nil || pr.Checks.Pending != 1 || pr.Head == "" {
		t.Fatalf("got %+v", pr)
	}
}
```

In `checks_engine_test.go`, add this line to `newRig` after `go r.e.Run(ctx)`:

```go
	t.Cleanup(func() { r.e.GitHub.Leave("factory") })
```

Add:

```go
const passedCI = `[{"__typename":"CheckRun","name":"ci","status":"COMPLETED","conclusion":"SUCCESS"}]`

// asked makes the PR cache ask gh again now. In production the poller asks every 30 seconds.
func (r *rig) asked() {
	r.t.Helper()
	if err := r.e.GitHub.Push(context.Background(), r.tree); err != nil {
		r.t.Fatal(err)
	}
}

func TestATicketWhoseCommandsAndGitHubChecksPassMovesToPRReview(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Checks: []string{"true"}})
	r.e.Recover()
	got := r.until("PR review", func(x proto.Ticket) bool { return x.Station == proto.StationPR })
	if got.Status != proto.TicketWorking || got.Why != "" || got.PR != 57 || got.Fix != 0 {
		t.Fatalf("got %+v", got)
	}
}

func TestAFailedGitHubCheckFailsTheTicket(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Checks: []string{"true"}})
	r.view(`[{"__typename":"CheckRun","name":"e2e","status":"COMPLETED","conclusion":"FAILURE"}]`)
	r.e.Recover()
	got := r.until("the failure", func(x proto.Ticket) bool { return x.Status == proto.TicketFailed })
	if got.Station != proto.StationChecks || !strings.HasPrefix(got.Why, "e2e") {
		t.Fatalf("got %+v", got)
	}
}

func TestPendingGitHubChecksHoldTheTicketUntilTheyPass(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{})
	r.view(`[{"__typename":"CheckRun","name":"ci","status":"IN_PROGRESS","conclusion":""}]`)
	r.e.Recover()
	r.until("the wait", func(x proto.Ticket) bool { return x.Why == "GitHub checks · 0/1" && x.Status == proto.TicketWorking })
	r.view(passedCI)
	r.asked()
	r.until("PR review", func(x proto.Ticket) bool { return x.Station == proto.StationPR })
}

func TestWithNoCommandsAndNoGitHubChecksTheTicketPassesAfterTheWait(t *testing.T) {
	quietAfter = 300 * time.Millisecond
	t.Cleanup(func() { quietAfter = 90 * time.Second })
	r := newRig(t, proto.FactoryConfig{})
	r.view(`[]`)
	r.e.Recover()
	r.until("the wait", func(x proto.Ticket) bool { return x.Why == "Waiting for GitHub checks" })
	r.until("PR review", func(x proto.Ticket) bool { return x.Station == proto.StationPR })
	if rec, _ := ReadRecord(r.home, r.ticket); !rec.Quiet || rec.State != LocalPassed || rec.Total != 0 {
		t.Fatalf("got %+v", rec)
	}
}

func TestAfterARestartTheWaitForGitHubCountsFromThePush(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{})
	r.view(`[]`)
	os.Remove(filepath.Join(r.fake, "nopr"))
	head := gitIn(t, r.tree, "rev-parse", "HEAD")
	pushed := time.Now().Add(-2 * quietAfter).UnixMilli()
	if err := WriteRecord(r.home, r.ticket, Record{Head: head, Pushed: pushed, State: LocalPassed}, true); err != nil {
		t.Fatal(err)
	}
	r.e.Board.Update(r.ticket, func(x *proto.Ticket) { x.PR = 57 })
	r.e.Recover()
	r.until("PR review", func(x proto.Ticket) bool { return x.Station == proto.StationPR })
}
```

**Step 2: Run it**

Run: `cd packages/pocketd && go test ./internal/github -run TestAfterCreate -count=1 ; go test ./internal/factory/ -run 'TestATicketWhoseCommands|TestAFailedGitHub|TestPendingGitHubChecksHold|TestWithNoCommands|TestAfterARestartTheWait' -count=1`
Expected: the first FAILs with `got &{Number:57 … Head:}` and no pending check. The second FAILs to build with `undefined: quietAfter`.

**Step 3: Implement**

In `internal/github/service.go`, in `Create`.

Before:

```go
	s.mu.Lock()
	s.cache.put(tree, &PR{Number: number, Title: title, URL: url, State: "OPEN", Draft: draft, Base: baseBranch(base), Mergeable: "UNKNOWN"}, s.now())
	s.mu.Unlock()
```

After:

```go
	s.mu.Lock()
	s.cache.put(tree, &PR{Number: number, Title: title, URL: url, State: "OPEN", Draft: draft, Base: baseBranch(base), Mergeable: "UNKNOWN"}, s.now())
	// Shown at once, and asked of gh at the next read for its head commit and its checks.
	s.cache.expire(tree, s.now())
	s.mu.Unlock()
```

In `engine.go`, add to the fields that only `Run`'s goroutine uses:

```go
	watched []string          // the worktrees the PR cache polls for the engine
	armed   map[string]string // per Ticket, the commit whose wait for GitHub's checks is armed
```

Extend the line at the top of `Run` (Task 1.5):

```go
	e.ctx, e.flight, e.prs, e.armed = ctx, map[string]bool{}, map[string]*proto.PR{}, map[string]string{}
```

In `drain`, call `e.watch()` after each batch.

Before (plan 3):

```go
		for _, ev := range queue {
			// Shutting down closes every session. Those closes aren't leads failing.
			if ctx.Err() != nil {
				return
			}
			e.handle(ev)
		}
```

After:

```go
		for _, ev := range queue {
			// Shutting down closes every session. Those closes aren't leads failing.
			if ctx.Err() != nil {
				return
			}
			e.handle(ev)
		}
		e.watch()
```

In `checks.go`, at the end of `pushed` (Task 1.5).

Before:

```go
	e.save(t.ID, rec, fresh)
	e.post(event{ticket: t.ID})
}
```

After:

```go
	e.save(t.ID, rec, fresh)
	e.post(event{ticket: t.ID})
	if err == nil {
		e.PRChanged(t.Worktree)
	}
}
```

Add to `checks.go` (new import: `slices` is already there):

```go
// quietAfter is how long after a push GitHub has to report a check before the commit counts as having none.
// GitHub creates the checks of a push within seconds, and the PR cache asks every 30 seconds: three polls.
var quietAfter = 3 * github.WatchedEvery

// PRChanged is github.Service.Listen's callback: the PR of tree changed. It reads the PR off Run's goroutine and
// queues it.
func (e *Engine) PRChanged(tree string) {
	if e == nil || e.GitHub == nil {
		return
	}
	go func() {
		ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
		defer cancel()
		pr, _ := e.GitHub.PR(ctx, tree)
		if pr == nil {
			return
		}
		e.later(func() {
			e.prs[tree] = pr
			all, _ := e.Board.Snapshot()
			for _, t := range all {
				if t.State == proto.TicketLive && t.Station != "" && t.Worktree == tree {
					e.post(event{ticket: t.ID})
				}
			}
		})
	}()
}

// watch keeps the PR cache polling the worktrees of the Tickets at Checks and PR review, and arms the wait for
// GitHub's checks of each pushed commit.
func (e *Engine) watch() {
	if e.GitHub == nil {
		return
	}
	all, _ := e.Board.Snapshot()
	var trees []string
	for _, t := range all {
		if t.State != proto.TicketLive || t.Worktree == "" || (t.Station != proto.StationChecks && t.Station != proto.StationPR) {
			continue
		}
		trees = append(trees, t.Worktree)
		if rec, ok := ReadRecord(e.Home, t.ID); ok && t.Station == proto.StationChecks {
			e.settle(t.ID, rec)
		}
	}
	if slices.Equal(trees, e.watched) {
		return
	}
	added := slices.DeleteFunc(slices.Clone(trees), func(tree string) bool { return slices.Contains(e.watched, tree) })
	e.watched = trees
	if len(trees) == 0 {
		e.GitHub.Leave("factory")
		return
	}
	e.GitHub.Listen("factory", e.PRChanged)
	e.GitHub.Watch("factory", trees)
	for _, tree := range added {
		e.PRChanged(tree)
	}
}

// settle arms the wait after which rec's commit, with no GitHub check, counts as having none.
func (e *Engine) settle(ticket string, rec Record) {
	if rec.Pushed == 0 || rec.Quiet || e.armed[ticket] == rec.Head {
		return
	}
	e.armed[ticket] = rec.Head
	head := rec.Head
	wait := max(0, quietAfter-e.now().Sub(time.UnixMilli(rec.Pushed)))
	time.AfterFunc(wait, func() { e.later(func() { e.quiet(ticket, head) }) })
}

// quiet marks the wait for GitHub's checks of the commit head as over.
func (e *Engine) quiet(ticket, head string) {
	rec, ok := ReadRecord(e.Home, ticket)
	if !ok || rec.Head != head || rec.Quiet {
		return
	}
	rec.Quiet = true
	e.save(ticket, rec, false)
	e.post(event{ticket: ticket})
}
```

`checks.go` now imports `pocketd/internal/github` for `github.WatchedEvery`.

**Step 4: Run it**

Run: `cd packages/pocketd && go test ./internal/github ./internal/factory/ -count=1`
Expected: `ok` for both packages.

### Task 2.6: `factory.ticket` returns the commands of the newest record

**Files:**
- Modify: `packages/pocketd/internal/factory/engine.go` (`Engine.Detail`, plan 3)
- Create: `packages/pocketd/internal/proto/testdata/factory/server/factory_ticket_checks.json`
- Test: `packages/pocketd/internal/factory/checks_engine_test.go`, `packages/pocketd/internal/proto/factory_test.go` (plan 1 creates it)

**Context:** `Engine.Detail(ticket)` (plan 3) fills `Spec` and `Activity` of `proto.FactoryTicket`. The index gives this plan `Checks` ("the last run, one per command") and `NoChecks` ("the station passed with nothing to run"). `NoChecks` is true when the newest record passed with no command and GitHub reported no check. Factory goldens live in `internal/proto/testdata/factory/` (index, Decisions "Goldens"), which the TypeScript and Swift suites do not read.

**Step 1: Write the failing test**

Create `internal/proto/testdata/factory/server/factory_ticket_checks.json`:

```json
{"type":"factory.ticket","id":"factory-9","ticket":{"ticketId":"t1","activity":[{"at":1762900000000,"station":"checks","actor":"anywhere","kind":"check","text":"Failed (exit 101): cargo test"}],"checks":[{"command":"cargo fmt --check","exit":0,"output":"","attempt":1,"at":1762900000000},{"command":"cargo test","exit":101,"output":"test result: FAILED. 41 passed; 1 failed\n","attempt":1,"at":1762900030000}]}}
```

Add to `internal/proto/factory_test.go`:

```go
func TestTheChecksOfAFactoryTicketAreWhatTheServerWrites(t *testing.T) {
	raw, err := os.ReadFile(filepath.Join("testdata", "factory", "server", "factory_ticket_checks.json"))
	if err != nil {
		t.Fatal(err)
	}
	var frame struct {
		Type   string        `json:"type"`
		ID     string        `json:"id"`
		Ticket FactoryTicket `json:"ticket"`
	}
	dec := json.NewDecoder(bytes.NewReader(raw))
	dec.DisallowUnknownFields()
	if err := dec.Decode(&frame); err != nil {
		t.Fatal(err)
	}
	if len(frame.Ticket.Checks) != 2 || frame.Ticket.Checks[1].Exit != 101 || frame.Ticket.Checks[1].Attempt != 1 || frame.Ticket.NoChecks {
		t.Fatalf("got %+v", frame.Ticket)
	}
	again, _ := json.Marshal(frame)
	if string(again) != strings.TrimSpace(string(raw)) {
		t.Fatalf("got  %s\nwant %s", again, raw)
	}
}
```

Add the imports the file lacks (`bytes`, `encoding/json`, `os`, `path/filepath`, `strings`).

Add to `checks_engine_test.go`:

```go
func TestTheTicketsDetailHoldsTheCommandsOfTheNewestRecord(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Checks: []string{"true", "sh -c 'echo boom; exit 3'"}})
	r.e.Recover()
	r.until("the failure", func(x proto.Ticket) bool { return x.Status == proto.TicketFailed })
	d, err := r.e.Detail(r.ticket)
	if err != nil || len(d.Checks) != 2 || d.Checks[0].Command != "true" || d.Checks[1].Exit != 3 || d.Checks[1].Output != "boom\n" || d.NoChecks {
		t.Fatalf("got %+v %v", d, err)
	}
}

func TestATicketThatPassedWithNothingToRunSaysNoChecksRan(t *testing.T) {
	quietAfter = 100 * time.Millisecond
	t.Cleanup(func() { quietAfter = 90 * time.Second })
	r := newRig(t, proto.FactoryConfig{})
	r.view(`[]`)
	r.e.Recover()
	r.until("PR review", func(x proto.Ticket) bool { return x.Station == proto.StationPR })
	if d, _ := r.e.Detail(r.ticket); !d.NoChecks || len(d.Checks) != 0 {
		t.Fatalf("got %+v", d)
	}
}

func TestATicketWhoseGitHubChecksPassedDoesNotSayNoChecksRan(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{})
	r.e.Recover()
	r.until("PR review", func(x proto.Ticket) bool { return x.Station == proto.StationPR })
	if d, _ := r.e.Detail(r.ticket); d.NoChecks {
		t.Fatalf("got %+v", d)
	}
}
```

**Step 2: Run it**

Run: `cd packages/pocketd && go test ./internal/proto -run TestTheChecksOfAFactoryTicket -count=1 ; go test ./internal/factory/ -run 'Detail|NoChecksRan' -count=1`
Expected: the proto test passes if plan 3 declared `Checks` and `NoChecks`, and fails with `json: unknown field "checks"` if it did not. The factory tests FAIL with `got {TicketID:… Checks:[] …}`.

**Step 3: Implement**

If `proto.FactoryTicket` lacks them, add the two fields as the index declares them:

```go
	Checks   []FactoryCheck `json:"checks,omitempty"`
	NoChecks bool           `json:"noChecks,omitempty"`
```

In `Engine.Detail` (`engine.go`), before its last `return out, nil`:

```go
	if rec, ok := ReadRecord(e.Home, ticket); ok {
		out.Checks = rec.Checks
		out.NoChecks = rec.State == LocalPassed && rec.Total == 0 && rec.Quiet
	}
```

**Step 4: Run it**

Run: `cd packages/pocketd && go test ./internal/proto ./internal/factory/ -count=1`
Expected: `ok` for both packages.

### PR 2 checks

**Files:** none.

**Step 1: Run the checks**

Run: `cd packages/pocketd && go vet ./... && go test ./...`
Expected: no vet output, every package `ok`.

**Step 2: Commit**

```bash
git add packages/pocketd
git commit -m "Checks runs the local commands and waits for GitHub's checks"
```

The red-team gate is not needed: this PR changes neither `peer.Needs` nor the broker. No golden under `testdata/golden/` changed, so the TypeScript and Swift suites gain no failure.

---

## PR 3: Auto-fix and `factory.hint`

**Scope:** `fix.go` with the fix note. Rows 1 to 5 and the fix rows of the table. The engine mails the note, takes the end of the lead's turn, commits, and starts Checks again. `Engine.Hint`, the verb `factory.hint`, its red-team probe. Two end-to-end tests.

**Depends on:** PR 2.

**Done when:** a failed check goes back to the lead up to `FactoryConfig.Fix` times, and each note adds 1 to `Ticket.Fix`. After the cap the Ticket is `failed` at Checks with `Auto-fix 2 of 2 used` in `Why`. `factory.hint` grants exactly one more attempt. A restart changes neither `Fix` nor `Hint`. The desktop has no button for the hint until PR 4.

### Task 3.1: `FixNote`

**Files:**
- Create: `packages/pocketd/internal/factory/fix.go`
- Test: `packages/pocketd/internal/factory/fix_test.go`

**Context:** The text is fixed by the index (section "Text the agents see", "The fix note"). A hint is the same block with `by="you"` and your text first. A base conflict adds one line. The Factory rules already tell the lead not to push. The note repeats the rule that applies.

**Step 1: Write the failing test**

Create `fix_test.go`:

```go
package factory

import (
	"fmt"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

func TestTheFixNoteNamesTheFailedCommandWithItsExitCodeAndOutput(t *testing.T) {
	checks := []proto.FactoryCheck{{Command: "cargo fmt --check", Exit: 0, Output: "fine\n"}, {Command: "cargo test", Exit: 101, Output: "test a ... ok\ntest b ... FAILED\n"}}
	want := `<anywhere_factory_fix attempt="1" of="2">
The checks failed. Fix the cause in this checkout. Don't commit and don't push: Anywhere does both when you finish.
$ cargo test   (exit 101)
test a ... ok
test b ... FAILED
</anywhere_factory_fix>`
	if got := FixNote(1, 2, "", checks, nil); got != want {
		t.Fatalf("got\n%s\nwant\n%s", got, want)
	}
}

func TestTheFixNoteKeepsTheLast200LinesOfAnOutput(t *testing.T) {
	var out strings.Builder
	for i := range 500 {
		fmt.Fprintf(&out, "line %d\n", i)
	}
	got := FixNote(1, 2, "", []proto.FactoryCheck{{Command: "make", Exit: 2, Output: out.String()}}, nil)
	if strings.Contains(got, "line 299\n") || !strings.Contains(got, "(exit 2)\nline 300\n") || !strings.Contains(got, "line 499\n</anywhere_factory_fix>") {
		t.Fatalf("got\n%s", got)
	}
}

func TestTheFixNoteListsEachFailedGitHubCheckWithItsLink(t *testing.T) {
	pr := &proto.PR{Base: "main", Runs: []proto.CheckRun{{Name: "lint", Outcome: "passed", URL: "https://x/1"}, {Name: "e2e", Outcome: "failed", URL: "https://x/2"}, {Name: "build", Outcome: "failed", URL: "https://x/3"}}}
	got := FixNote(2, 2, "", nil, pr)
	if !strings.Contains(got, "\nGitHub check failed: e2e https://x/2\nGitHub check failed: build https://x/3\n</anywhere_factory_fix>") || strings.Contains(got, "lint") {
		t.Fatalf("got\n%s", got)
	}
}

func TestTheFixNoteForABaseConflictSaysWhatToMerge(t *testing.T) {
	got := FixNote(1, 2, "", nil, &proto.PR{Base: "main", Mergeable: "CONFLICTING"})
	if !strings.Contains(got, "\nThe pull request conflicts with main. Merge origin/main into this branch and resolve the conflicts.\n") {
		t.Fatalf("got\n%s", got)
	}
}

func TestAHintNoteIsMarkedAsYoursAndStartsWithYourText(t *testing.T) {
	got := FixNote(3, 2, "  Bump the snapshot instead.\n", []proto.FactoryCheck{{Command: "make", Exit: 2, Output: "no newline at the end"}}, nil)
	want := `<anywhere_factory_fix attempt="3" of="2" by="you">
Bump the snapshot instead.
The checks failed. Fix the cause in this checkout. Don't commit and don't push: Anywhere does both when you finish.
$ make   (exit 2)
no newline at the end
</anywhere_factory_fix>`
	if got != want {
		t.Fatalf("got\n%s\nwant\n%s", got, want)
	}
}
```

**Step 2: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -run 'Note' -count=1`
Expected: FAIL to build with `undefined: FixNote`.

**Step 3: Implement**

Create `fix.go`:

```go
package factory

import (
	"fmt"
	"strings"

	"pocketd/internal/proto"
)

// noteLines is how many lines of a failed command's output the fix note carries.
const noteLines = 200

// FixNote is the note mailed to the lead after failed checks. hint is the user's text, "" for an automatic note.
// checks are the commands of the newest record. pr is the PR when its GitHub checks or its base failed, else nil.
func FixNote(attempt, limit int, hint string, checks []proto.FactoryCheck, pr *proto.PR) string {
	var b strings.Builder
	fmt.Fprintf(&b, "<anywhere_factory_fix attempt=\"%d\" of=\"%d\"", attempt, limit)
	if hint = strings.TrimSpace(hint); hint != "" {
		b.WriteString(" by=\"you\">\n" + hint + "\n")
	} else {
		b.WriteString(">\n")
	}
	b.WriteString("The checks failed. Fix the cause in this checkout. Don't commit and don't push: Anywhere does both when you finish.\n")
	for _, c := range checks {
		if c.Exit != 0 {
			fmt.Fprintf(&b, "$ %s   (exit %d)\n%s", c.Command, c.Exit, lastLines(c.Output, noteLines))
		}
	}
	if pr != nil {
		if pr.Mergeable == "CONFLICTING" {
			fmt.Fprintf(&b, "The pull request conflicts with %s. Merge origin/%s into this branch and resolve the conflicts.\n", pr.Base, pr.Base)
		}
		for _, run := range pr.Runs {
			if run.Outcome == "failed" {
				fmt.Fprintf(&b, "GitHub check failed: %s %s\n", run.Name, run.URL)
			}
		}
	}
	b.WriteString("</anywhere_factory_fix>")
	return b.String()
}

// lastLines is the last n lines of text, ending with a newline when text is not empty.
func lastLines(text string, n int) string {
	text = strings.TrimRight(text, "\n")
	if text == "" {
		return ""
	}
	lines := strings.Split(text, "\n")
	if len(lines) > n {
		lines = lines[len(lines)-n:]
	}
	return strings.Join(lines, "\n") + "\n"
}
```

**Step 4: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -run 'Note' -count=1`
Expected: `ok  	pocketd/internal/factory`.

### Task 3.2: `checksNext`, rows 1 to 5 and the fix rows

**Files:**
- Modify: `packages/pocketd/internal/factory/checks.go`
- Test: `packages/pocketd/internal/factory/checks_next_test.go`

**Context:** `LocalChecks.Fix` (Task 1.2) follows a fix note: `mailed` once it is sent, then `ended` or `stopped` with the lead's turn. `LocalChecks.Hint` says that a hint waits to be sent. `LocalChecks.Same` says that the fix turn before this commit changed nothing. Rows 1 to 5 come before the fault of row 6, because a hint is the way out of a fault. `Ticket.Fix` counts the automatic notes. A hint note does not add to it, so a hint grants exactly one attempt (C7, US-26).

**Step 1: Write the failing test**

Add to `checks_next_test.go`:

```go
// failing is a Ticket whose command failed on commit aaa, with Fix attempts used of a cap of 2.
func failing(fix int, edit func(*Facts)) Facts {
	return pushed(func(f *Facts) {
		f.Ticket.Fix = fix
		f.Local.State, f.Local.Failed = LocalFailed, "cargo test"
		edit(f)
	})
}

func TestAFailedCommandGoesBackToTheLeadAndUsesAnAttempt(t *testing.T) {
	got, acts := Next(failing(0, func(*Facts) {}))
	if got.Status != proto.TicketWorking || got.Fix != 1 || got.Why != "Fixing · attempt 1" || did(acts) != "sendFix" || acts[0].Text != "" {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestTheSecondFailureUsesTheSecondAttempt(t *testing.T) {
	got, acts := Next(failing(1, func(*Facts) {}))
	if got.Fix != 2 || got.Why != "Fixing · attempt 2" || did(acts) != "sendFix" {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestAFailureAfterTheLastAttemptFailsTheTicketAtChecksAndSaysSo(t *testing.T) {
	got, acts := Next(failing(2, func(*Facts) {}))
	if got.Status != proto.TicketFailed || got.Station != proto.StationChecks || got.Fix != 2 || got.Why != "cargo test · Auto-fix 2 of 2 used" || len(acts) != 0 {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestWithAutoFixOffTheFirstFailureFailsTheTicket(t *testing.T) {
	got, acts := Next(failing(0, func(f *Facts) { f.Config.Fix = 0 }))
	if got.Status != proto.TicketFailed || got.Fix != 0 || got.Why != "cargo test · Auto-fix is off" || len(acts) != 0 {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestAFailureOnACommitTheLeadDidNotChangeSendsNoNote(t *testing.T) {
	got, acts := Next(failing(1, func(f *Facts) { f.Local.Same = true }))
	if got.Status != proto.TicketFailed || got.Fix != 1 || got.Why != "cargo test · The lead changed nothing" || len(acts) != 0 {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestAFailedGitHubCheckGoesBackToTheLeadToo(t *testing.T) {
	got, acts := Next(pushed(func(f *Facts) {
		f.Local.State, f.PR = LocalPassed, green()
		f.PR.Checks.Failed = 1
		f.PR.Runs = append(f.PR.Runs, proto.CheckRun{Name: "e2e", Outcome: "failed"})
	}))
	if got.Fix != 1 || got.Why != "Fixing · attempt 1" || did(acts) != "sendFix" {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestAPRThatConflictsWithItsBaseUsesAnAttempt(t *testing.T) {
	got, acts := Next(pushed(func(f *Facts) {
		f.Local.State, f.PR = LocalPassed, green()
		f.PR.Mergeable = "CONFLICTING"
	}))
	if got.Fix != 1 || did(acts) != "sendFix" {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestWhileTheLeadFixesTheTicketWorksAndUsesNoMoreAttempts(t *testing.T) {
	got, acts := Next(failing(1, func(f *Facts) {
		f.Ticket.Why = "Fixing · attempt 1"
		f.Local.Fix = FixMailed
		f.Lead.Turn = "working"
	}))
	if got.Status != proto.TicketWorking || got.Fix != 1 || got.Why != "Fixing · attempt 1" || len(acts) != 0 {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestALeadThatAsksDuringItsFixWaitsOnYou(t *testing.T) {
	got, _ := Next(failing(1, func(f *Facts) {
		f.Local.Fix = FixMailed
		f.Lead.Turn = "needsYou"
	}))
	if got.Status != proto.TicketNeedsYou || got.Gate != proto.GateAsk || got.Station != proto.StationChecks {
		t.Fatalf("got %+v", got)
	}
}

func TestAFixTurnThatEndedIsCommittedAndPushed(t *testing.T) {
	got, acts := Next(failing(1, func(f *Facts) { f.Local.Fix = FixEnded }))
	if got.Status != proto.TicketWorking || got.Fix != 1 || got.Why != "Committing the fix" || did(acts) != "commitPush" {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestAFixTurnThatFailedOrWasStoppedFailsTheTicket(t *testing.T) {
	got, acts := Next(failing(1, func(f *Facts) { f.Local.Fix = FixStopped }))
	if got.Status != proto.TicketFailed || got.Fix != 1 || got.Why != "The lead's fix turn failed or was stopped" || len(acts) != 0 {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestAHintSendsYourNoteAndGrantsOneAttemptBeyondTheCap(t *testing.T) {
	got, acts := Next(failing(2, func(f *Facts) {
		f.Ticket.Status = proto.TicketFailed
		f.Local.Hint = true
	}))
	if got.Status != proto.TicketWorking || !got.Hint || got.Fix != 2 || got.Why != "Fixing · your hint" || did(acts) != "sendFix" || acts[0].Text != "you" {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestAHintIsTheWayOutOfAFault(t *testing.T) {
	got, acts := Next(pushed(func(f *Facts) {
		f.Ticket.Status = proto.TicketFailed
		f.Local.Error, f.Local.Hint = "Conflict in a.rs", true
	}))
	if got.Status != proto.TicketWorking || did(acts) != "sendFix" {
		t.Fatalf("got %+v %v", got, acts)
	}
}

func TestAFailureAfterAHintFailsTheTicketWithTheCapUsed(t *testing.T) {
	got, acts := Next(failing(2, func(f *Facts) { f.Ticket.Hint = true }))
	if got.Status != proto.TicketFailed || !got.Hint || got.Fix != 2 || got.Why != "cargo test · Auto-fix 2 of 2 used" || len(acts) != 0 {
		t.Fatalf("got %+v %v", got, acts)
	}
}
```

The three tests of Task 2.3 that set `f.Config.Fix = 0` keep passing: their `Why` starts with the name of what failed.

**Step 2: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -run 'TestAFailed|TestTheSecond|TestAFailure|TestWithAutoFix|TestAPRThatConflicts|TestWhileTheLead|TestALeadThatAsks|TestAFixTurn|TestAHint' -count=1`
Expected: FAIL. `TestAFailedCommandGoesBackToTheLeadAndUsesAnAttempt` reports `Status: failed` and `Fix: 0`.

**Step 3: Implement**

In `checks.go`, put the new stage first in `checksNext`.

Before:

```go
	for _, stage := range []func(*Facts) ([]Action, bool){faulted, integration, pullRequest, commands} {
```

After:

```go
	for _, stage := range []func(*Facts) ([]Action, bool){fixing, faulted, integration, pullRequest, commands} {
```

Replace `broken`.

Before:

```go
// broken is a failed check named what. PR 3 of the Checks plan sends it to the lead first.
func broken(f *Facts, what string) ([]Action, bool) {
	return fail(f, what)
}
```

After:

```go
// broken is a failed check named what: the fix rows.
func broken(f *Facts, what string) ([]Action, bool) {
	limit := f.Config.Fix
	switch {
	case f.Local.Same:
		return fail(f, what+" · The lead changed nothing")
	case limit == 0:
		return fail(f, what+" · Auto-fix is off")
	case f.Ticket.Fix >= limit:
		// A cap lowered after attempts were used reads as all of it used.
		return fail(f, fmt.Sprintf("%s · Auto-fix %d of %d used", what, min(f.Ticket.Fix, limit), limit))
	}
	f.Ticket.Fix++
	return work(f, fmt.Sprintf("Fixing · attempt %d", f.Ticket.Fix), Action{Kind: ActSendFix})
}

// fixing is rows 1 to 5.
func fixing(f *Facts) ([]Action, bool) {
	switch {
	case f.Local.Fix == FixMailed && f.Lead.Turn == "needsYou":
		f.Ticket.Status, f.Ticket.Gate = proto.TicketNeedsYou, proto.GateAsk
		return nil, true
	case f.Local.Fix == FixMailed:
		return work(f, f.Ticket.Why)
	case f.Local.Fix == FixEnded:
		return work(f, "Committing the fix", Action{Kind: ActCommitPush})
	case f.Local.Hint:
		f.Ticket.Hint = true
		return work(f, "Fixing · your hint", Action{Kind: ActSendFix, Text: "you"})
	case f.Local.Fix == FixStopped:
		return fail(f, "The lead's fix turn failed or was stopped")
	}
	return nil, false
}
```

**Step 4: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -count=1`
Expected: `ok  	pocketd/internal/factory`. The engine tests of PR 2 that fail a check run with `Fix: 0`, so their Tickets still fail at once, now with `· Auto-fix is off` after the command. The engine has no arm for `sendFix` until Task 3.3.

### Task 3.3: The engine mails the note, takes the end of the fix turn, and commits

**Files:**
- Modify: `packages/pocketd/internal/factory/checks.go`
- Modify: `packages/pocketd/internal/factory/engine.go` (`TurnEnded`, `e.act`)
- Test: `packages/pocketd/internal/factory/checks_engine_test.go`

**Context:** `Engine.Mail` is `daemon.Mail` (`internal/daemon/mail.go`). It queues the note and delivers it to the lead's chat as a plain user turn. `Registry.OnTurnEnd` calls `Engine.TurnEnded(id, ok)`, where `ok` is false when the turn failed or you stopped it (`internal/agent/agent.go:136`). A stopped turn puts the note back in the mailbox, so the lead may run it again later. That is why the end of a stopped turn's next normal turn also counts. `worktree.Commit(dir, message)` stages everything and commits with `--no-verify`, and returns false when there was nothing to commit (`internal/worktree/merge.go:27`). The commit runs on `Run`'s goroutine as the merge's commit does in Task 1.5. After `commitPush` the new record has `Pushed: 0`, so row 10 pushes it with `github.Service.Push` before the commands run again.

**Step 1: Write the failing test**

Add to `checks_engine_test.go`:

```go
// note is the next note mailed to the lead.
func (r *rig) note() string {
	r.t.Helper()
	select {
	case text := <-r.mail:
		return text
	case <-time.After(15 * time.Second):
		r.t.Fatalf("no note was mailed: %+v", r.get())
		return ""
	}
}

// quiet fails when a note is mailed within d.
func (r *rig) quiet(d time.Duration) {
	r.t.Helper()
	select {
	case text := <-r.mail:
		r.t.Fatalf("mailed %q", text)
	case <-time.After(d):
	}
}

// fixes is the lead's fix turn: it writes file in the checkout, then ends normally.
func (r *rig) fixes(file, body string) {
	r.t.Helper()
	if file != "" {
		if err := os.WriteFile(filepath.Join(r.tree, file), []byte(body), 0o644); err != nil {
			r.t.Fatal(err)
		}
	}
	r.e.TurnEnded(leadID, true)
}

const needsFixed = `test -f fixed.txt || { echo "fixed.txt is missing"; exit 1; }`

func TestAFailedCommandIsMailedToTheLeadAndItsFixPassesOnTheFirstAttempt(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Fix: 2, Checks: []string{needsFixed}})
	r.e.Recover()
	note := r.note()
	if want := "<anywhere_factory_fix attempt=\"1\" of=\"2\">\nThe checks failed. Fix the cause in this checkout. Don't commit and don't push: Anywhere does both when you finish.\n$ " + needsFixed + "   (exit 1)\nfixed.txt is missing\n</anywhere_factory_fix>"; note != want {
		t.Fatalf("note\n%s\nwant\n%s", note, want)
	}
	if got := r.get(); got.Fix != 1 || got.Status != proto.TicketWorking || got.Why != "Fixing · attempt 1" {
		t.Fatalf("while fixing: %+v", got)
	}
	r.fixes("fixed.txt", "ok\n")
	got := r.until("PR review", func(x proto.Ticket) bool { return x.Station == proto.StationPR })
	if got.Fix != 1 || got.Status != proto.TicketWorking {
		t.Fatalf("got %+v", got)
	}
	if msg := gitIn(t, r.tree, "log", "-1", "--format=%s"); msg != "Fix the failed checks" {
		t.Fatalf("the last commit is %q", msg)
	}
	if remote, local := gitIn(t, filepath.Join(filepath.Dir(r.tree), "origin.git"), "rev-parse", "fix"), gitIn(t, r.tree, "rev-parse", "HEAD"); remote != local {
		t.Fatalf("the fix was not pushed: %s, %s", remote, local)
	}
	if d, _ := r.e.Detail(r.ticket); len(d.Checks) != 1 || d.Checks[0].Exit != 0 || d.Checks[0].Attempt != 1 {
		t.Fatalf("the newest record: %+v", d.Checks)
	}
}

func TestATicketFailsAtChecksAfterTheLastAttemptAndSaysSo(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Fix: 2, Checks: []string{needsFixed}})
	r.e.Recover()
	if note := r.note(); !strings.HasPrefix(note, `<anywhere_factory_fix attempt="1" of="2">`) {
		t.Fatalf("first note %q", note)
	}
	r.fixes("a.txt", "one\n")
	if note := r.note(); !strings.HasPrefix(note, `<anywhere_factory_fix attempt="2" of="2">`) {
		t.Fatalf("second note %q", note)
	}
	r.fixes("a.txt", "two\n")
	got := r.until("the failure", func(x proto.Ticket) bool { return x.Status == proto.TicketFailed })
	if got.Station != proto.StationChecks || got.Fix != 2 || got.Why != needsFixed+" · Auto-fix 2 of 2 used" {
		t.Fatalf("got %+v", got)
	}
	r.quiet(300 * time.Millisecond)
}

func TestAFixTurnThatChangesNothingIsCheckedAgainButNotSentAgain(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Fix: 2, Checks: []string{needsFixed}})
	r.e.Recover()
	r.note()
	r.fixes("", "")
	got := r.until("the failure", func(x proto.Ticket) bool { return x.Status == proto.TicketFailed })
	if got.Fix != 1 || got.Why != needsFixed+" · The lead changed nothing" {
		t.Fatalf("got %+v", got)
	}
	if rec, _ := ReadRecord(r.home, r.ticket); !rec.Same || rec.State != LocalFailed {
		t.Fatalf("the commands did not run again on the same commit: %+v", rec)
	}
	r.quiet(300 * time.Millisecond)
}

func TestAStoppedFixTurnFailsTheTicketAndTheLeadsNextNormalEndCarriesOn(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Fix: 2, Checks: []string{needsFixed}})
	r.e.Recover()
	r.note()
	r.e.TurnEnded(leadID, false)
	got := r.until("the failure", func(x proto.Ticket) bool { return x.Status == proto.TicketFailed })
	if got.Fix != 1 || got.Why != "The lead's fix turn failed or was stopped" {
		t.Fatalf("got %+v", got)
	}
	r.fixes("fixed.txt", "ok\n")
	r.until("PR review", func(x proto.Ticket) bool { return x.Station == proto.StationPR })
}

func TestAFailedGitHubCheckIsMailedWithItsLink(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Fix: 1})
	r.view(`[{"__typename":"CheckRun","name":"e2e","status":"COMPLETED","conclusion":"FAILURE","detailsUrl":"https://github.com/acme/app/actions/runs/9"}]`)
	r.e.Recover()
	if note := r.note(); !strings.Contains(note, "\nGitHub check failed: e2e https://github.com/acme/app/actions/runs/9\n") {
		t.Fatalf("note %q", note)
	}
	r.view(passedCI)
	r.fixes("b.txt", "x\n")
	r.until("PR review", func(x proto.Ticket) bool { return x.Station == proto.StationPR })
}
```

**Step 2: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -run 'TestAFailedCommandIsMailed|TestATicketFailsAtChecksAfter|TestAFixTurn|TestAStoppedFixTurn|TestAFailedGitHubCheckIsMailed' -count=1`
Expected: FAIL with `no note was mailed` in each test, because `e.act` has no arm for `sendFix`.

**Step 3: Implement**

In `engine.go`, `TurnEnded` queues how the fix turn ended before the step.

Before (plan 3):

```go
func (e *Engine) TurnEnded(id string, ok bool) {
	if e != nil {
		e.post(event{agent: id, turn: true, ok: ok})
	}
}
```

After:

```go
func (e *Engine) TurnEnded(id string, ok bool) {
	if e != nil {
		e.later(func() { e.fixEnded(id, ok) })
		e.post(event{agent: id, turn: true, ok: ok})
	}
}
```

If plan 4 changed the body of `TurnEnded`, keep its lines and add the `e.later` line first.

In `e.act`, add the arms:

```go
	case ActSendFix:
		e.sendFix(t, a.Text)
	case ActCommitPush:
		e.commitPush(t)
```

Add to `checks.go`:

```go
// sendFix mails the lead the fix note for t's newest record. by is "you" for the note of a hint.
func (e *Engine) sendFix(t proto.Ticket, by string) {
	rec, fresh := e.current(t)
	limit := e.Configs.Get(t.Folder).Fix
	attempt, hint := t.Fix, ""
	if by == "you" {
		attempt, hint = t.Fix+1, rec.Hint
	}
	checks, pr := rec.Checks, e.prs[t.Worktree]
	if rec.State != LocalFailed {
		checks = nil
	}
	if rec.State != LocalPassed {
		pr = nil
	}
	if err := e.Mail(t.AgentID, FixNote(attempt, limit, hint, checks, pr)); err != nil {
		rec.Hint, rec.Error = "", "Couldn't send the fix note: "+err.Error()
		e.save(t.ID, rec, fresh)
		e.post(event{ticket: t.ID})
		return
	}
	rec.Fix, rec.Hint = FixMailed, ""
	e.save(t.ID, rec, fresh)
	if by == "you" {
		e.say(t.ID, "you", "hint", "Sent your hint to the lead")
		return
	}
	e.say(t.ID, "anywhere", "fix", fmt.Sprintf("Sent the failure to the lead: attempt %d of %d", attempt, limit))
}

// fixEnded records how lead's turn ended while its fix note was out. The next normal end after a stopped fix turn
// counts too, because the mailbox shows a stopped turn's note again.
func (e *Engine) fixEnded(lead string, ok bool) {
	for _, t := range e.atChecks() {
		rec, has := ReadRecord(e.Home, t.ID)
		if t.AgentID != lead || !has || (rec.Fix != FixMailed && rec.Fix != FixStopped) || (rec.Fix == FixStopped && !ok) {
			continue
		}
		rec.Fix = FixStopped
		if ok {
			rec.Fix = FixEnded
			e.say(t.ID, "lead", "fix", "Finished the fix")
		}
		e.save(t.ID, rec, false)
	}
}

// commitPush commits what the fix turn changed and starts the record of the commit. The next step pushes it and
// runs the commands again, also on the same commit, once.
func (e *Engine) commitPush(t proto.Ticket) {
	old, _ := ReadRecord(e.Home, t.ID)
	if _, err := worktree.Commit(t.Worktree, "Fix the failed checks"); err != nil {
		old.Fix, old.Error = "", "Couldn't commit the fix: "+err.Error()
		e.save(t.ID, old, false)
		e.post(event{ticket: t.ID})
		return
	}
	head, _ := worktree.Head(t.Worktree)
	same := head == old.Head
	e.save(t.ID, Record{Head: head, Same: same}, true)
	text := "Committed the fix " + head[:min(7, len(head))]
	if same {
		text = "The lead changed nothing: checking the same commit once more"
	}
	e.say(t.ID, "anywhere", "fix", text)
	e.post(event{ticket: t.ID})
}
```

**Step 4: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -count=1`
Expected: `ok  	pocketd/internal/factory`.

### Task 3.4: `Engine.Hint`, and a restart grants nothing

**Files:**
- Modify: `packages/pocketd/internal/factory/fix.go`
- Test: `packages/pocketd/internal/factory/checks_engine_test.go`

**Context:** The index declares `Engine.Hint(ticket, text string) error` for this plan. It is called on a connection's goroutine, so it checks what it can and queues the rest for `Run`'s goroutine, which alone writes the check record. A hint needs a live Ticket that failed at Checks and a text that is not blank (C7, E14). `Fix` and `Hint` live in `board.json`, and the record lives on disk, so a restart reads them back and grants no attempt (E1, US-25, US-36). The errors map to the wire's codes in Task 3.5: `ErrOff` to `factory_off`, `board.ErrUnknown` to `unknown_ticket`, `ErrNoHint` to `ticket_busy`.

**Step 1: Write the failing test**

Add to `checks_engine_test.go`:

```go
// spent is a rig whose Ticket failed at Checks after both attempts.
func spent(t *testing.T) *rig {
	t.Helper()
	r := newRig(t, proto.FactoryConfig{Fix: 2, Checks: []string{needsFixed}})
	r.e.Recover()
	r.note()
	r.fixes("a.txt", "one\n")
	r.note()
	r.fixes("a.txt", "two\n")
	r.until("the failure", func(x proto.Ticket) bool { return x.Status == proto.TicketFailed })
	return r
}

func TestAHintSendsYourTextToTheLeadAndGrantsExactlyOneMoreAttempt(t *testing.T) {
	r := spent(t)
	if err := r.e.Hint(r.ticket, "  Create fixed.txt.  "); err != nil {
		t.Fatal(err)
	}
	note := r.note()
	if !strings.HasPrefix(note, "<anywhere_factory_fix attempt=\"3\" of=\"2\" by=\"you\">\nCreate fixed.txt.\nThe checks failed.") || !strings.Contains(note, "fixed.txt is missing") {
		t.Fatalf("note %q", note)
	}
	if got := r.get(); !got.Hint || got.Fix != 2 || got.Status != proto.TicketWorking || got.Why != "Fixing · your hint" {
		t.Fatalf("while fixing: %+v", got)
	}
	r.fixes("a.txt", "three\n")
	got := r.until("the failure", func(x proto.Ticket) bool { return x.Status == proto.TicketFailed })
	if got.Fix != 2 || !got.Hint || got.Why != needsFixed+" · Auto-fix 2 of 2 used" {
		t.Fatalf("got %+v", got)
	}
	r.quiet(300 * time.Millisecond)

	if err := r.e.Hint(r.ticket, "Really, create fixed.txt."); err != nil {
		t.Fatal(err)
	}
	r.note()
	r.fixes("fixed.txt", "ok\n")
	got = r.until("PR review", func(x proto.Ticket) bool { return x.Station == proto.StationPR })
	if got.Hint || got.Fix != 2 {
		t.Fatalf("got %+v", got)
	}
	events, _ := ReadActivity(r.home, r.ticket)
	hints := slices.DeleteFunc(slices.Clone(events), func(ev proto.FactoryEvent) bool { return ev.Kind != "hint" })
	if len(hints) != 2 || hints[0].Actor != "you" || hints[0].Station != proto.StationChecks {
		t.Fatalf("hints in the activity: %+v", hints)
	}
}

func TestAHintIsRefusedUnlessTheTicketFailedAtChecksAndTheTextSaysSomething(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Fix: 2, Checks: []string{"sleep 5"}})
	r.e.Recover()
	r.until("the commands", func(x proto.Ticket) bool { return strings.HasPrefix(x.Why, "Checks · ") })
	if err := r.e.Hint(r.ticket, "try harder"); !errors.Is(err, ErrNoHint) {
		t.Fatalf("a working ticket: %v", err)
	}
	if err := r.e.Hint("nope", "try harder"); !errors.Is(err, board.ErrUnknown) {
		t.Fatalf("an unknown ticket: %v", err)
	}
	r.e.Board.Update(r.ticket, func(x *proto.Ticket) { x.Status = proto.TicketFailed })
	if err := r.e.Hint(r.ticket, " \n "); !errors.Is(err, ErrNoHint) {
		t.Fatalf("a blank hint: %v", err)
	}
	var off *Engine
	if err := off.Hint(r.ticket, "try harder"); !errors.Is(err, ErrOff) {
		t.Fatalf("a nil engine: %v", err)
	}
}

func TestARestartGrantsNoAttemptToATicketThatUsedThemAll(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Fix: 2, Checks: []string{needsFixed}})
	head := gitIn(t, r.tree, "rev-parse", "HEAD")
	failed := Record{Head: head, Pushed: time.Now().UnixMilli(), State: LocalFailed, Total: 1, Checks: []proto.FactoryCheck{{Command: needsFixed, Exit: 1, Output: "fixed.txt is missing\n", Attempt: 2}}}
	if err := WriteRecord(r.home, r.ticket, failed, true); err != nil {
		t.Fatal(err)
	}
	r.e.Board.Update(r.ticket, func(x *proto.Ticket) {
		x.PR, x.Fix, x.Status, x.Why = 57, 2, proto.TicketFailed, needsFixed+" · Auto-fix 2 of 2 used"
	})
	r.e.Recover()
	r.quiet(500 * time.Millisecond)
	if got := r.get(); got.Status != proto.TicketFailed || got.Fix != 2 || got.Station != proto.StationChecks {
		t.Fatalf("got %+v", got)
	}
}

func TestAFixNoteThatWasOutWhenPocketdStoppedIsNotSentAgain(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Fix: 2, Checks: []string{needsFixed}})
	os.Remove(filepath.Join(r.fake, "nopr"))
	head := gitIn(t, r.tree, "rev-parse", "HEAD")
	out := Record{Head: head, Pushed: time.Now().UnixMilli(), State: LocalFailed, Total: 1, Fix: FixMailed, Checks: []proto.FactoryCheck{{Command: needsFixed, Exit: 1, Attempt: 0}}}
	if err := WriteRecord(r.home, r.ticket, out, true); err != nil {
		t.Fatal(err)
	}
	r.e.Board.Update(r.ticket, func(x *proto.Ticket) { x.PR, x.Fix, x.Why = 57, 1, "Fixing · attempt 1" })
	r.e.Recover()
	r.quiet(500 * time.Millisecond)
	if got := r.get(); got.Status != proto.TicketWorking || got.Fix != 1 || got.Why != "Fixing · attempt 1" {
		t.Fatalf("got %+v", got)
	}
	r.fixes("fixed.txt", "ok\n")
	r.until("PR review", func(x proto.Ticket) bool { return x.Station == proto.StationPR })
}
```

Add `"errors"` to the imports of `checks_engine_test.go`.

**Step 2: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -run 'TestAHint|TestARestartGrants|TestAFixNoteThatWasOut' -count=1`
Expected: FAIL to build with `r.e.Hint undefined` and `undefined: ErrNoHint`.

**Step 3: Implement**

Add to `fix.go` (new imports: `errors`, `pocketd/internal/board`):

```go
// ErrNoHint refuses a hint for a Ticket that did not fail at Checks, or a blank one.
var ErrNoHint = errors.New("A hint needs a ticket that failed at Checks, and some text")

// Hint sends text to the lead of a Ticket that failed at Checks, as one more attempt beyond the cap (C7).
func (e *Engine) Hint(ticket, text string) error {
	if e == nil {
		return ErrOff
	}
	text = strings.TrimSpace(text)
	t, ok := e.ticket(ticket)
	if !ok {
		return board.ErrUnknown
	}
	if !hintable(t) || text == "" {
		return ErrNoHint
	}
	e.later(func() {
		// The Ticket may have moved on since the check above.
		if t, ok := e.ticket(ticket); ok && hintable(t) {
			rec, fresh := e.current(t)
			rec.Hint = text
			e.save(ticket, rec, fresh)
			e.post(event{ticket: ticket})
		}
	})
	return nil
}

func hintable(t proto.Ticket) bool {
	return t.State == proto.TicketLive && t.Station == proto.StationChecks && t.Status == proto.TicketFailed
}
```

**Step 4: Run it**

Run: `cd packages/pocketd && go test ./internal/factory/ -count=1`
Expected: `ok  	pocketd/internal/factory`.

### Task 3.5: The verb `factory.hint`, and its red-team probe

**Files:**
- Modify: `packages/pocketd/internal/proto/messages.go` (`DecodeClient`, after plan 4's `factory.advance` case)
- Create: `packages/pocketd/internal/proto/testdata/factory/client/factory_hint.json`
- Create: `packages/pocketd/internal/wsserver/factory_hint.go`
- Modify: `packages/pocketd/internal/wsserver/wsserver.go` (the dispatch, after plan 4's `factory.advance` case)
- Modify: `packages/pocketd/e2e/redteam/main.go` (`wsOutcomes`), `packages/pocketd/e2e/redteam_test.go` (the expected map)
- Test: `packages/pocketd/internal/proto/factory_test.go`, `packages/pocketd/e2e/factory_checks_test.go`

**Context:** The index gives `factory.hint` the fields `id`, `ticketId`, `text` and the scope Drive. Plan 1 added its `peer.Needs` row and its row in the scope matrix, so `dispatch` already refuses it to a terminal peer before the switch. `ClientMessage` has `TicketID` and `Text`. `agent.btw` already refuses a blank text at decode (`internal/proto/messages.go:196`), and `factory.hint` does the same. The handler copies plan 4's `factoryAdvance` (`internal/wsserver/factory_advance.go`). Plan 1 asks the plan that adds the decoder of a gate verb to add its red-team probe (plan 1, "Checks every plan runs"). The probe's message must decode, or its outcome is `Malformed message` and proves nothing. Factory client goldens are written by hand (plan 1, Decisions).

**Step 1: Write the failing test**

Create `internal/proto/testdata/factory/client/factory_hint.json` (one line, then a newline):

```json
{"type":"factory.hint","id":"factory-7","ticketId":"t1","text":"Update the snapshot instead of the test."}
```

Add to `internal/proto/factory_test.go`:

```go
func TestTheFactoryHintGoldenDecodesToItsFields(t *testing.T) {
	m, err := DecodeClient(factoryGolden(t, "client", "factory_hint.json"))
	if err != nil || m.Type != "factory.hint" || m.ID != "factory-7" || m.TicketID != "t1" || m.Text != "Update the snapshot instead of the test." {
		t.Fatalf("%+v %v", m, err)
	}
}

func TestAHintWithoutATicketOrWithABlankTextIsMalformed(t *testing.T) {
	for _, raw := range []string{
		`{"type":"factory.hint","id":"1","text":"x"}`,
		`{"type":"factory.hint","id":"1","ticketId":"","text":"x"}`,
		`{"type":"factory.hint","id":"1","ticketId":"t1"}`,
		`{"type":"factory.hint","id":"1","ticketId":"t1","text":" \n "}`,
		`{"type":"factory.hint","id":"1","ticketId":"t1","text":7}`,
	} {
		if _, err := DecodeClient([]byte(raw)); err != ErrMalformed {
			t.Errorf("%s: got %v, want ErrMalformed", raw, err)
		}
	}
}
```

Create `e2e/factory_checks_test.go`:

```go
//go:build unix

package e2e

import (
	"strings"
	"testing"

	"pocketd/internal/proto"
)

func TestAHintForAnUnknownTicketIsRefusedByItsCode(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner(proto.CapBoard, proto.CapTeam, proto.CapFactory)
	o.Send(map[string]any{"type": "factory.hint", "id": "hint", "ticketId": "nope", "text": "try again"})
	if m := o.WaitFor("the reply", replyTo("hint")); m.Type != "error" || !strings.Contains(m.Raw, `"unknown_ticket"`) {
		t.Fatalf("got %s", m.Raw)
	}
}
```

In `e2e/redteam_test.go`, add one row at the end of the expected map, after the rows that plans 1 and 4 added:

```go
		"ws.factory.hint":        "scope_denied",
```

**Step 2: Run it**

Run: `cd packages/pocketd && go test ./internal/proto -run 'Hint' -count=1 ; go test ./e2e -run TestAHintForAnUnknownTicket -count=1`
Expected: both FAIL. The proto tests report `factory_hint.json: … malformed` from the golden test (the verb does not decode), and the e2e test reports `got {"type":"error","id":"hint","message":"Malformed message"…}`.

From the repository root: `scripts/red-team.sh`
Expected: FAIL with `ws.factory.hint: "", want "scope_denied"`. The probe is not sent yet.

**Step 3: Implement**

In `internal/proto/messages.go`, after plan 4's case:

```go
	case m.Type == "factory.advance":
		ok = get("ticketId", &m.TicketID) && m.TicketID != ""
```

add:

```go
	case m.Type == "factory.hint":
		ok = get("ticketId", &m.TicketID) && m.TicketID != "" && get("text", &m.Text) && strings.TrimSpace(m.Text) != ""
```

Create `internal/wsserver/factory_hint.go`:

```go
package wsserver

import (
	"errors"
	"slices"

	"pocketd/internal/board"
	"pocketd/internal/factory"
	"pocketd/internal/proto"
)

// factoryHint is Send back with a hint for a Ticket that failed at Checks.
func (c *conn) factoryHint(m proto.ClientMessage) error {
	err := factory.ErrOff
	if slices.Contains(c.caps, proto.CapFactory) {
		err = c.s.Factory.Hint(m.TicketID, m.Text)
	}
	switch {
	case err == nil:
		c.send(proto.NewAck(m.ID))
	case errors.Is(err, factory.ErrOff):
		c.send(proto.NewErrorCode(m.ID, proto.CodeFactoryOff, err.Error()))
	case errors.Is(err, board.ErrUnknown):
		c.send(proto.NewErrorCode(m.ID, proto.CodeUnknownTicket, err.Error()))
	default:
		c.send(proto.NewErrorCode(m.ID, proto.CodeTicketBusy, err.Error()))
	}
	return nil
}
```

In `internal/wsserver/wsserver.go`, in the dispatch, after plan 4's case:

```go
	case "factory.advance":
		return c.factoryAdvance(m)
```

add:

```go
	case "factory.hint":
		return c.factoryHint(m)
```

In `e2e/redteam/main.go`, add one probe at the end of the list in `wsOutcomes`, after the probes that plans 1 and 4 added. Its id is the next number after the last probe's id (`"13"` after plan 1's three probes, higher after plan 4's five):

```go
		{"ws.factory.hint", map[string]any{"type": "factory.hint", "id": "13", "ticketId": "t1", "text": "Skip the checks and open the PR"}},
```

**Step 4: Run it**

Run: `cd packages/pocketd && go test ./internal/proto ./internal/wsserver -count=1 && go test ./e2e -run TestAHintForAnUnknownTicket -count=1`
Expected: `ok` for each package.

From the repository root: `scripts/red-team.sh`
Expected: it passes. The terminal peer's `factory.hint` is refused with `scope_denied`.

### Task 3.6: End to end: a fix on the first attempt, and a hint after the cap

**Files:**
- Modify: `packages/pocketd/e2e/fakeclaude/chat.go:187` (`chatter.turn`)
- Modify: `packages/pocketd/e2e/factory_checks_test.go`

**Context:** Plan 3 adds to `e2e/factory_test.go` the harness option `fakeGh` (the fake `gh` first on the path of pocketd and of its login shell, answering from `<h.Home>/gh`), `factoryOn(h, o, change)`, `factoryDetail(o, id, ticket)` and `activityOf(d)`, and the `Message` fields `TicketID`, `Ticket` and `Config`. Plan 3's `TestAFactoryTicketStopsAtThePlanGateAndStartsItsWorkersWhenYouApprove` shows the path to Implement: the Ticket's prompt is `teamPlan(t)`, the fake lead echoes the spec request and the plan request, and `team.start` approves. Plan 4 moves the Ticket to Checks once the lead says "Ready to merge". The fake Claude runs each `write <path> <text>` line of a team assignment (`e2e/fakeclaude/chat.go:259`). This task makes it treat a fix note the same way. Each check command below prints a `write` line when it fails, so the note carries the fake lead's repair. The harness's Repo has no remote, so `withOrigin` gives it a bare one to push to.

Plan 6 calls `factoryAtPR(t, cfg)` with four results (plan 6, its table of guesses). Plan 3 already names a function `fakeGh`, so the third result has the type `*ghFolder`.

**Step 1: Write the failing test**

Append to `e2e/factory_checks_test.go`, and add `"os"`, `"os/exec"` and `"path/filepath"` to its imports:

```go
// withOrigin gives the harness's Repo a bare remote named origin. Pass it after launchReady.
func withOrigin(h *Harness) {
	origin := filepath.Join(h.Home, "origin.git")
	for _, args := range [][]string{{"init", "-q", "--bare", origin}, {"-C", h.Repo, "remote", "add", "origin", origin}} {
		if out, err := exec.Command("git", args...).CombinedOutput(); err != nil {
			h.t.Fatalf("git %v: %s", args, out)
		}
	}
}

// ghFolder is the folder the fake gh of fakeGh answers from.
type ghFolder struct {
	t   *testing.T
	dir string
}

func (g *ghFolder) put(name, content string) {
	g.t.Helper()
	if err := os.WriteFile(filepath.Join(g.dir, name), []byte(content), 0o644); err != nil {
		g.t.Fatal(err)
	}
}

func (g *ghFolder) called(prefix string) bool {
	raw, _ := os.ReadFile(filepath.Join(g.dir, "calls"))
	for _, line := range strings.Split(strings.TrimSpace(string(raw)), "\n") {
		if _, args, _ := strings.Cut(line, " "); strings.HasPrefix(args, prefix) {
			return true
		}
	}
	return false
}

// factoryAtChecks turns the Factory on with cfg, starts a Ticket whose lead plans teamPlan, approves the plan, and
// returns once the Ticket is at Checks. The fake gh has no PR until pocketd opens #57, whose one check passes on the
// commit of the checkout.
func factoryAtChecks(t *testing.T, cfg proto.FactoryConfig) (*Harness, *Phone, *ghFolder, proto.Ticket) {
	t.Helper()
	h := Start(t, launchReady(""), gitIdentity, fakeGh, withOrigin)
	o := h.Owner(proto.CapBoard, proto.CapTeam, proto.CapFactory, proto.CapGitHub)
	gh := &ghFolder{t: t, dir: filepath.Join(h.Home, "gh")}
	gh.put("nopr", "")
	gh.put("created", "https://github.com/acme/app/pull/57\n")
	gh.put("view.json", `{"isDraft":true,"number":57,"state":"OPEN","title":"Review","url":"https://github.com/acme/app/pull/57","mergeable":"MERGEABLE","headRefOid":"@HEAD@","baseRefName":"main","statusCheckRollup":[{"__typename":"CheckRun","name":"ci","status":"COMPLETED","conclusion":"SUCCESS"}]}`)
	factoryOn(h, o, func(c *proto.FactoryConfig) { *c = cfg; c.Enabled = true })
	saved := o.Board("save", "s1", map[string]any{"ticket": ticketDraft(h, teamPlan(t)), "start": true})
	if saved.Type != "ack" {
		t.Fatalf("board.save: %s", saved.Raw)
	}
	gate := o.WaitTicket("the plan gate", func(x proto.Ticket) bool { return x.ID == saved.TicketID && x.Gate == proto.GatePlan })
	o.Send(map[string]any{"type": "team.start", "id": "start", "lead": gate.AgentID, "tasks": []map[string]any{
		{"id": "t1", "agent": "claude", "allow": []string{"make test"}},
		{"id": "t2", "agent": "claude", "allow": []string{}},
	}})
	if m := o.WaitFor("team.start's reply", replyTo("start")); m.Type != "ack" {
		t.Fatalf("team.start: %s", m.Raw)
	}
	ticket := o.WaitTicket("Checks", func(x proto.Ticket) bool { return x.ID == saved.TicketID && x.Station == proto.StationChecks })
	return h, o, gh, ticket
}

// factoryAtPR is factoryAtChecks, returned once the Ticket passed Checks.
func factoryAtPR(t *testing.T, cfg proto.FactoryConfig) (*Harness, *Phone, *ghFolder, proto.Ticket) {
	t.Helper()
	h, o, gh, ticket := factoryAtChecks(t, cfg)
	ticket = o.WaitTicket("PR review", func(x proto.Ticket) bool { return x.ID == ticket.ID && x.Station == proto.StationPR })
	return h, o, gh, ticket
}

func TestAFailingCheckIsRepairedByTheLeadOnTheFirstAttempt(t *testing.T) {
	cfg := proto.DefaultFactoryConfig()
	cfg.Fix, cfg.Checks = 2, []string{`test -f fixed.txt || { echo "write fixed.txt ok"; exit 1; }`}
	_, o, gh, done := factoryAtPR(t, cfg)
	if done.Fix != 1 || done.PR != 57 || done.Status != proto.TicketWorking || done.Hint {
		t.Fatalf("got %+v", done)
	}
	if raw, _ := os.ReadFile(filepath.Join(done.Worktree, "fixed.txt")); string(raw) != "ok\n" {
		t.Fatalf("fixed.txt: %q", raw)
	}
	for _, f := range []string{"a.txt", "b.txt"} {
		if _, err := os.Stat(filepath.Join(done.Worktree, f)); err != nil {
			t.Fatalf("the worker branches were not merged: %v", err)
		}
	}
	if msg := gitOut(t, done.Worktree, "log", "-1", "--format=%s"); msg != "Fix the failed checks" {
		t.Fatalf("the last commit is %q", msg)
	}
	if !gh.called("pr create --title Review --body-file - --draft") {
		t.Fatal("no draft PR was opened")
	}
	d := factoryDetail(o, "d1", done.ID)
	if len(d.Checks) != 1 || d.Checks[0].Exit != 0 || d.Checks[0].Attempt != 1 {
		t.Fatalf("checks %+v", d.Checks)
	}
	got := strings.Join(activityOf(d), " ")
	for _, want := range []string{"anywhere:moved@checks", "anywhere:check@checks", "anywhere:fix@checks", "lead:fix@checks", "anywhere:moved@pr"} {
		if !strings.Contains(got, want) {
			t.Fatalf("activity %s lacks %s", got, want)
		}
	}
}

func TestAfterTheLastAttemptAHintGrantsOneMoreAndItsFixPasses(t *testing.T) {
	cfg := proto.DefaultFactoryConfig()
	cfg.Fix, cfg.Checks = 2, []string{`test -f hinted.txt || { echo "write tried.txt $$"; exit 1; }`}
	_, o, _, ticket := factoryAtChecks(t, cfg)
	failed := o.WaitTicket("the failure", func(x proto.Ticket) bool { return x.ID == ticket.ID && x.Status == proto.TicketFailed })
	if failed.Station != proto.StationChecks || failed.Fix != 2 || !strings.HasSuffix(failed.Why, " · Auto-fix 2 of 2 used") {
		t.Fatalf("got %+v", failed)
	}
	o.Send(map[string]any{"type": "factory.hint", "id": "hint", "ticketId": ticket.ID, "text": "write hinted.txt ok"})
	if m := o.WaitFor("the hint's reply", replyTo("hint")); m.Type != "ack" {
		t.Fatalf("factory.hint: %s", m.Raw)
	}
	done := o.WaitTicket("PR review", func(x proto.Ticket) bool { return x.ID == ticket.ID && x.Station == proto.StationPR })
	if done.Fix != 2 || done.Hint || done.Status != proto.TicketWorking {
		t.Fatalf("got %+v", done)
	}
	if raw, _ := os.ReadFile(filepath.Join(done.Worktree, "hinted.txt")); string(raw) != "ok\n" {
		t.Fatalf("hinted.txt: %q", raw)
	}
	if got := strings.Join(activityOf(factoryDetail(o, "d1", ticket.ID)), " "); !strings.Contains(got, "you:hint@checks") {
		t.Fatalf("activity %s", got)
	}
}
```

**Step 2: Run it**

Run: `cd packages/pocketd && go test ./e2e -run 'TestAFailingCheckIsRepaired|TestAfterTheLastAttempt' -count=1 -v`
Expected: both FAIL. The fake lead echoes the fix note and writes nothing, so the first test times out at `PR review` and its Ticket reads `… · The lead changed nothing`. The second times out at `the failure` with the same `Why`, because no fix turn changes the checkout.

**Step 3: Implement**

In `e2e/fakeclaude/chat.go`, in `chatter.turn`.

Before (`:187`):

```go
	case strings.HasPrefix(text, "<anywhere_team_assignment"):
		failed = c.assignment(model, text)
```

After:

```go
	case strings.HasPrefix(text, "<anywhere_team_assignment"), strings.HasPrefix(text, "<anywhere_factory_fix"):
		failed = c.assignment(model, text)
```

**Step 4: Run it**

Run: `cd packages/pocketd && go test ./e2e -run 'TestAFailingCheckIsRepaired|TestAfterTheLastAttempt|TestAHintForAnUnknownTicket|Factory' -count=1 -v`
Expected: every test PASSES. If `Checks` times out, the Ticket stopped at Implement: read pocketd's log in the output for `factory:` lines and check that plan 4's move to Checks fired. If `PR review` times out with `Why: Waiting for GitHub`, the fake `gh` did not see the pushed commit: check that `view.json` holds `@HEAD@` and that `nopr` was removed by `gh pr create`.

### PR 3 checks

**Step 1: Run the checks**

Run: `cd packages/pocketd && go vet ./... && go test ./...`
Expected: no vet output, every package `ok`.

From the repository root: `scripts/red-team.sh`
Expected: it passes. This PR adds a probe to the gate, so the gate is required.

**Step 2: Commit**

```bash
git add packages/pocketd
git commit -m "Checks sends failures back to the lead, and factory.hint grants one more attempt"
```

Only `testdata/factory/client/factory_hint.json` is a new golden. Nothing under `testdata/golden/` changed, so the TypeScript and Swift suites gain no failure.

---

## PR 4: The desktop: the Checks tab, the Failed bar and the card note

**Scope:** `agents`: `factory::Check`, `Detail.checks`, `Detail.no_checks`, `Outbox::factory_hint`. `pocket`: `factory/checks.rs` with the **Checks** tab (T8), `Decision::Failed` with **Send back with a hint** and **Close ticket** (C7, US-26), the card note of a Ticket at Checks, and the capture step `factory-ticket-checks`.

**Depends on:** PR 3 for the behaviour. The code builds on plans 1 to 4 and on `testdata/factory/` of PR 2 and PR 3.

**Done when:** from `packages/desktop`, `cargo build --workspace`, `cargo clippy --workspace --all-targets` with no new warning and `cargo test --workspace` pass, and the capture of Task 4.5 matches the prototype's Checks tab and failed bar.

### Task 4.1: `agents`: the checks of a detail, and `factory_hint`

**Files:**
- Modify: `packages/desktop/crates/agents/src/factory.rs` (`Detail`, plan 3 Task 3.1)
- Modify: `packages/desktop/crates/agents/src/agents.rs` (`impl Outbox`, after plan 4's `factory_advance`; `mod tests`)

**Context:** Plan 3 adds `agents::factory::Detail { ticket_id, spec, activity }` with serde `default` and camelCase, so a key it does not name is ignored. It also adds `factory_id()` and the test macro `factory_golden!`, which reads `packages/pocketd/internal/proto/testdata/factory/<name>`. PR 2 wrote `server/factory_ticket_checks.json` and PR 3 wrote `client/factory_hint.json`. Factory verbs go out on the local Outbox (`self.outbox.local`), with ids `factory-<n>`. pocketd keeps the attempt as an `int` that is at most `MaxFix + 1` plus the hints, so `u32` holds it.

**Step 1: Write the failing test**

In `agents/src/agents.rs`, in `mod tests`, after plan 4's test of `factory_advance`:

```rust
    #[test]
    fn a_factory_ticket_frame_carries_its_checks() {
        let raw = include_str!("../../../../pocketd/internal/proto/testdata/factory/server/factory_ticket_checks.json");
        let Some(Event::FactoryTicket(d)) = factory_event(&serde_json::from_str::<Frame>(raw).unwrap()) else { panic!("not a detail") };
        assert_eq!(d.checks.len(), 2);
        assert_eq!(d.checks[1], factory::Check { command: "cargo test".into(), exit: 101, output: "test result: FAILED. 41 passed; 1 failed\n".into(), attempt: 1, at: 1762900030000 });
        assert!(!d.no_checks);
    }

    #[test]
    fn factory_hint_sends_the_golden_frame_but_its_id() {
        assert_eq!(sent_but_id(|o| o.factory_hint("t1", "Update the snapshot instead of the test.")), without_id(factory_golden!("client/factory_hint.json")));
    }
```

In `agents/src/factory.rs`, in `mod tests` (add the module if plan 3 left none, with `use super::*;`):

```rust
    #[test]
    fn a_detail_without_checks_reads_as_none_ran() {
        let d: Detail = serde_json::from_str(r#"{"ticketId":"t1","activity":[],"noChecks":true}"#).unwrap();
        assert!(d.checks.is_empty() && d.no_checks);
    }
```

**Step 2: Run it**

Run: `cd packages/desktop && cargo test -p agents factory 2>&1 | tail -20`
Expected: the crate does not compile, with `no field `checks` on type `Detail`` and `no method named `factory_hint` found for reference `&Outbox``.

**Step 3: Implement**

In `agents/src/factory.rs`, in `Detail`.

Before (plan 3):

```rust
    /// Oldest first.
    pub activity: Vec<Activity>,
}
```

After:

```rust
    /// Oldest first.
    pub activity: Vec<Activity>,
    /// The local commands of the newest commit, in the order they ran. A failure ends the list.
    pub checks: Vec<Check>,
    /// Checks passed with no local command and no GitHub check.
    pub no_checks: bool,
}

/// One local command's run in the Checks station.
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Check {
    pub command: String,
    /// -1 when it ran too long or could not start.
    pub exit: i32,
    /// The last 64 KB.
    pub output: String,
    /// The auto-fix attempts used when it ran.
    pub attempt: u32,
    /// Unix ms.
    pub at: i64,
}
```

If plan 6 has already added `findings` to `Detail`, keep it and add the two fields after it.

In `agents/src/agents.rs`, in `impl Outbox`, after `factory_advance`:

```rust
    /// Sends a Ticket that failed at Checks back to its lead with `text`: one more attempt.
    pub fn factory_hint(&self, ticket: &str, text: &str) {
        self.send(json!({"type": "factory.hint", "id": factory_id(), "ticketId": ticket, "text": text}));
    }
```

`agents.rs` re-exports `factory`'s types the way plan 1 set up (`pub use factory::…` or `pub mod factory`). If it lists them one by one, add `Check` to the list.

**Step 4: Run it**

Run: `cd packages/desktop && cargo test -p agents factory`
Expected: `test result: ok`.

### Task 4.2: `factory/checks.rs`: rows, the attempt line, the output's tail, and the card note

**Files:**
- Create: `packages/desktop/crates/pocket/src/factory/checks.rs`
- Modify: `packages/desktop/crates/pocket/src/factory.rs` (module)
- Modify: `packages/desktop/crates/pocket/src/factory/station.rs` (`note`, plan 2)

**Context:** The Checks tab (T8, `checksTab` in `app.js:340`) lists each configured command with its mark, then a callout with the failure and the attempt line, then the output. pocketd's `Ticket.why` already says where Checks is: `Checks · 2/3`, `Fixing · attempt 1`, `Fixing · your hint`, or for a failure `<what failed> · <why no fix follows>` (PR 1 to PR 3 of this plan). The card note and the callout's title read it. Plan 2's `station::note` returns `Failed · <why>` for any failed Ticket and nothing for a working one. This task gives a Ticket at Checks its own note: the progress while it works, and only the name of what failed once it failed (`Failed · cargo test`).

Output can be 64 KB. Laying out 64 KB of mono text on every frame would cost what exists, not what is visible (CLAUDE.md, "Desktop performance"). So the tab shows the last 40 lines of the failed command, and `tail` cuts them once, when the detail arrives (Task 4.3). The fix note gave the lead the last 200 lines. These are plain functions over data, so they are tested here and drawn in Task 4.3.

**Step 1: Write the failing test**

Create `factory/checks.rs`:

```rust
use agents::factory::Check;
use agents::tickets::{Status, Ticket};

/// A configured command's mark in the Checks tab.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Mark {
    Passed,
    Failed,
    /// It failed and the lead is fixing it.
    Fixing,
    /// It has not run on this commit yet.
    Waiting,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Row {
    pub(crate) command: String,
    pub(crate) mark: Mark,
    pub(crate) exit: i32,
}

/// The end of a failed command's output, cut once when the detail arrives.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Output {
    pub(crate) command: String,
    pub(crate) exit: i32,
    pub(crate) text: String,
    /// The lines in `text`, and in the whole output.
    pub(crate) shown: usize,
    pub(crate) total: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use agents::tickets::{State, Station};

    fn check(command: &str, exit: i32) -> Check {
        Check { command: command.into(), exit, output: format!("{command} said something\n"), attempt: 1, at: 1 }
    }

    fn at_checks(status: Status, why: &str, fix: u8) -> Ticket {
        Ticket { state: State::Live, station: Station::Checks, status, why: why.into(), fix, ..Ticket::sample() }
    }

    fn commands() -> Vec<String> {
        vec!["cargo fmt --check".into(), "cargo clippy".into(), "cargo test".into()]
    }

    #[test]
    fn each_configured_command_shows_how_it_ran_on_the_newest_commit() {
        let rows = rows(&commands(), &[check("cargo fmt --check", 0), check("cargo clippy", 101)], false);
        assert_eq!(rows.iter().map(|r| r.mark).collect::<Vec<_>>(), [Mark::Passed, Mark::Failed, Mark::Waiting]);
        assert_eq!(rows[1].exit, 101);
        assert_eq!(passed(&rows), 1);
    }

    #[test]
    fn while_the_lead_fixes_the_failed_command_reads_fixing() {
        let rows = rows(&commands(), &[check("cargo fmt --check", 0), check("cargo clippy", 101)], true);
        assert_eq!(rows[1].mark, Mark::Fixing);
    }

    #[test]
    fn a_command_added_since_the_last_run_waits() {
        let rows = rows(&commands(), &[check("make lint", 0)], false);
        assert!(rows.iter().all(|r| r.mark == Mark::Waiting));
    }

    #[test]
    fn without_the_projects_settings_the_rows_are_the_commands_that_ran() {
        let rows = rows(&[], &[check("cargo test", 0)], false);
        assert_eq!(rows, [Row { command: "cargo test".into(), mark: Mark::Passed, exit: 0 }]);
    }

    #[test]
    fn the_ticket_says_whether_the_lead_is_fixing() {
        assert!(fixing(&at_checks(Status::Working, "Fixing · attempt 1", 1)));
        assert!(fixing(&at_checks(Status::Working, "Fixing · your hint", 2)));
        assert!(!fixing(&at_checks(Status::Working, "Checks · 1/3", 0)));
        assert!(!fixing(&at_checks(Status::Failed, "cargo test · Auto-fix 2 of 2 used", 2)));
    }

    #[test]
    fn the_attempt_line_counts_the_attempts_used_of_the_cap() {
        assert_eq!(attempt_line(&at_checks(Status::Working, "Fixing · attempt 1", 1), 2), "Auto-fix 1 of 2 used");
        assert_eq!(attempt_line(&at_checks(Status::Failed, "cargo test · Auto-fix 2 of 2 used", 2), 2), "Auto-fix 2 of 2 used");
        assert_eq!(attempt_line(&at_checks(Status::Failed, "cargo test · Auto-fix is off", 0), 0), "Auto-fix is off");
        assert_eq!(attempt_line(&at_checks(Status::Failed, "x", 2), 1), "Auto-fix 1 of 1 used");
        let hinted = Ticket { hint: true, ..at_checks(Status::Working, "Fixing · your hint", 2) };
        assert_eq!(attempt_line(&hinted, 2), "One attempt granted by you");
    }

    #[test]
    fn the_callout_is_titled_with_what_failed() {
        assert_eq!(what("cargo test · Auto-fix 2 of 2 used"), "cargo test");
        assert_eq!(what("Conflict in a.rs, b.rs"), "Conflict in a.rs, b.rs");
        assert_eq!(what(""), "Checks failed");
    }

    #[test]
    fn a_short_output_is_shown_whole() {
        let out = tail(&check("cargo test", 101), 40);
        assert_eq!((out.text.as_str(), out.shown, out.total), ("cargo test said something", 1, 1));
        assert_eq!(hidden(&out), None);
    }

    #[test]
    fn a_long_output_shows_its_last_lines_and_says_how_many_are_left_out() {
        let long = Check { command: "cargo test".into(), exit: 101, output: (1..=500).map(|i| format!("line {i}\n")).collect(), ..Check::default() };
        let out = tail(&long, 40);
        assert_eq!((out.shown, out.total), (40, 500));
        assert!(out.text.starts_with("line 461\n") && out.text.ends_with("line 500"));
        assert_eq!(hidden(&out).as_deref(), Some("Showing the last 40 of 500 lines"));
    }

    #[test]
    fn the_output_shown_is_the_failed_commands() {
        assert_eq!(failed_output(&[check("cargo fmt --check", 0), check("cargo test", 101)]).map(|o| o.command), Some("cargo test".to_string()));
        assert_eq!(failed_output(&[check("cargo fmt --check", 0)]), None);
    }

    #[test]
    fn a_ticket_at_checks_notes_its_progress_and_what_failed() {
        assert_eq!(checks_note(&at_checks(Status::Working, "Checks · 2/3", 0)), Some(("Checks · 2/3".into(), Tone::Dim)));
        assert_eq!(checks_note(&at_checks(Status::Working, "Fixing · attempt 1", 1)), Some(("Fixing · attempt 1".into(), Tone::Dim)));
        assert_eq!(checks_note(&at_checks(Status::Failed, "cargo test · Auto-fix 2 of 2 used", 2)), Some(("Failed · cargo test".into(), Tone::Failed)));
        assert_eq!(checks_note(&at_checks(Status::Working, "", 0)), None);
    }
}
```

Add to `factory.rs`, beside the other `mod` lines:

```rust
pub(crate) mod checks;
```

**Step 2: Run it**

Run: `cd packages/desktop && cargo test -p pocket factory::checks 2>&1 | tail -20`
Expected: the crate does not compile, with `cannot find function `rows` in this scope`, `cannot find function `attempt_line``, `cannot find function `tail``, `cannot find type `Tone``.

**Step 3: Implement**

In `factory/checks.rs`, replace the `use` lines with:

```rust
use crate::board::logic::Tone;
use agents::factory::Check;
use agents::tickets::{Status, Ticket};
```

Add between `Output` and the tests:

```rust
/// One row per configured command, marked by the check at the same place in the newest record. A record of other
/// commands, from before the settings changed, marks nothing. Without the settings, the commands that ran.
pub(crate) fn rows(commands: &[String], checks: &[Check], fixing: bool) -> Vec<Row> {
    let ran: Vec<String> = checks.iter().map(|c| c.command.clone()).collect();
    let commands = if commands.is_empty() { &ran[..] } else { commands };
    commands
        .iter()
        .enumerate()
        .map(|(i, command)| {
            let check = checks.get(i).filter(|c| &c.command == command);
            let mark = match check {
                None => Mark::Waiting,
                Some(c) if c.exit == 0 => Mark::Passed,
                Some(_) if fixing => Mark::Fixing,
                Some(_) => Mark::Failed,
            };
            Row { command: command.clone(), mark, exit: check.map_or(0, |c| c.exit) }
        })
        .collect()
}

pub(crate) fn passed(rows: &[Row]) -> usize {
    rows.iter().filter(|r| r.mark == Mark::Passed).count()
}

/// Whether the lead works on a fix note: pocketd says so in `why`.
pub(crate) fn fixing(t: &Ticket) -> bool {
    t.status == Status::Working && t.why.starts_with("Fixing · ")
}

/// "Auto-fix 1 of 2 used", or what the hint granted.
pub(crate) fn attempt_line(t: &Ticket, cap: u8) -> String {
    match () {
        _ if cap == 0 => "Auto-fix is off".into(),
        _ if t.hint && fixing(t) => "One attempt granted by you".into(),
        _ => format!("Auto-fix {} of {cap} used", t.fix.min(cap)),
    }
}

/// What failed, from a failed Ticket's `why`: the part before " · ".
pub(crate) fn what(why: &str) -> &str {
    match why.split(" · ").next() {
        Some(w) if !w.is_empty() => w,
        _ => "Checks failed",
    }
}

/// The last `lines` lines of `check`'s output.
pub(crate) fn tail(check: &Check, lines: usize) -> Output {
    let all: Vec<&str> = check.output.trim_end_matches('\n').lines().collect();
    let shown = &all[all.len().saturating_sub(lines)..];
    Output { command: check.command.clone(), exit: check.exit, text: shown.join("\n"), shown: shown.len(), total: all.len() }
}

/// The line above a cut output.
pub(crate) fn hidden(out: &Output) -> Option<String> {
    (out.shown < out.total).then(|| format!("Showing the last {} of {} lines", out.shown, out.total))
}

/// The tail of the command that failed, if one did.
pub(crate) fn failed_output(checks: &[Check]) -> Option<Output> {
    checks.iter().find(|c| c.exit != 0).map(|c| tail(c, OUTPUT_LINES))
}

/// How many lines of a failed command's output the tab shows.
pub(crate) const OUTPUT_LINES: usize = 40;

/// A card's note at Checks: the progress while it works, what failed once it failed.
pub(crate) fn checks_note(t: &Ticket) -> Option<(String, Tone)> {
    match t.status {
        Status::Failed => Some((format!("Failed · {}", what(&t.why)), Tone::Failed)),
        _ if t.why.is_empty() => None,
        _ => Some((t.why.clone(), Tone::Dim)),
    }
}
```

In `factory/station.rs`, in `note`.

Before (plan 2):

```rust
        Gate::None if failed(t) && t.why.is_empty() => return Some(("Failed".to_string(), Tone::Failed)),
        Gate::None if failed(t) => return Some((format!("Failed · {}", t.why), Tone::Failed)),
```

After:

```rust
        Gate::None if t.station == Station::Checks => return super::checks::checks_note(t),
        Gate::None if failed(t) && t.why.is_empty() => return Some(("Failed".to_string(), Tone::Failed)),
        Gate::None if failed(t) => return Some((format!("Failed · {}", t.why), Tone::Failed)),
```

Add `Station` to the `agents::tickets` import of `station.rs` if it is not there. `Tone` and `Ticket::sample()` already exist (`board/logic.rs:711`, plan 1). If `Ticket.fix` is not `u8`, change the `fix` parameter of `at_checks` and the `cap` of `attempt_line` to its type.

**Step 4: Run it**

Run: `cd packages/desktop && cargo test -p pocket factory::`
Expected: `test result: ok`. Plan 2's `a_failed_ticket_reads_failed_with_the_reason` still passes: its `why` has no ` · `, so the note is the whole `why`. `cargo build` warns that `rows`, `passed`, `attempt_line`, `hidden` and `failed_output` are unused until Task 4.3.

### Task 4.3: The Checks tab

**Files:**
- Modify: `packages/desktop/crates/pocket/src/factory/checks.rs` (the view)
- Modify: `packages/desktop/crates/pocket/src/factory.rs` (`FactoryState.outputs`)
- Modify: `packages/desktop/crates/pocket/src/factory/tabs.rs` (`OFFERED`, plan 3)
- Modify: `packages/desktop/crates/pocket/src/factory/page.rs` (`factory_page`, plan 3 Task 3.4)
- Modify: `packages/desktop/crates/pocket/src/factory/spec.rs` (`heading` becomes `pub(super)`)
- Modify: `packages/desktop/crates/pocket/src/desktop/alerts.rs` (beside plan 3's `Event::FactoryUpdate` lines)

**Context:** The prototype draws the tab as `<h2>Checks<small>N/M passed</small></h2>`, then one rounded list of rows (`.check`: 10px gap, 9/12 padding, the mark, the command in mono `TEXT_2`, and on the right the time, `Fixing` or `Failed`), then the callout, then the output in `<pre>` (11.5/17 mono, `TEXT_2`, no wrap). PRD §11: the callout has a 20% red hairline and an 8% red tint, red only on the title and the attempt line, and the output stays neutral. While the lead fixes, the callout is calm (`FILL_1`, title `TEXT`, attempt line `TEXT_3`). The GitHub checks come from the desktop's own PR cache, `self.prs.get(&t.worktree) -> Option<&Pr>` (`git_ui/pull_requests.rs:79`), with `Pr.runs: Vec<git::github::Check { name, outcome, url }>`. `theme::spinner(id, size, color)` and `ui::ring(color, width)` exist. Plan 3's spec tab has the `heading(title, note, right)` helper of the tab's `<h2>`. `Tab::Checks` exists since plan 3, but `OFFERED` leaves it out until a plan gives it a body.

The output's tail is cut when `Event::FactoryTicket` arrives, not in `render`. `FactoryState` holds `Entity` values and can't be built in a test, so the cut is the plain `failed_output` of Task 4.2, and `FactoryState` only keeps its result.

**Step 1: Write the failing test**

Add to the tests of `factory/tabs.rs`:

```rust
    #[test]
    fn a_ticket_at_checks_opens_on_the_checks_tab() {
        assert_eq!(shown(None, Column::Checks), Tab::Checks);
        assert_eq!(shown(Some(Tab::Activity), Column::Checks), Tab::Activity);
    }
```

**Step 2: Run it**

Run: `cd packages/desktop && cargo test -p pocket factory::tabs`
Expected: FAIL with `assertion `left == right` failed` and `left: Spec`, `right: Checks`.

**Step 3: Implement**

In `factory/tabs.rs`, add `Tab::Checks` to `OFFERED` before `Tab::Activity`, and add 1 to the array's length. With only plan 3's tabs:

Before:

```rust
pub(crate) const OFFERED: [Tab; 2] = [Tab::Spec, Tab::Activity];
```

After:

```rust
pub(crate) const OFFERED: [Tab; 3] = [Tab::Spec, Tab::Checks, Tab::Activity];
```

If plan 4 added `Tab::Workers`, the result is `[Tab; 4] = [Tab::Spec, Tab::Workers, Tab::Checks, Tab::Activity]`.

In `factory/spec.rs`, change `fn heading(` to `pub(super) fn heading(`.

In `factory.rs`, add the field to `FactoryState`, and `outputs: Default::default()` to the struct literal in `FactoryState::new`:

```rust
    /// The tail of each ticket's failed command, cut when its detail arrived.
    pub(crate) outputs: HashMap<String, checks::Output>,
```

Add `use std::collections::HashMap;` to `factory.rs` if it is not there.

In `desktop/alerts.rs`, after plan 3's lines:

```rust
        if let Event::FactoryUpdate(ticket) = &ev {
            self.factory_updated(ticket);
        }
```

add:

```rust
        if let Event::FactoryTicket(d) = &ev {
            match crate::factory::checks::failed_output(&d.checks) {
                Some(out) => _ = self.factory.outputs.insert(d.ticket_id.clone(), out),
                None => _ = self.factory.outputs.remove(&d.ticket_id),
            }
        }
```

In `factory/page.rs`, in `factory_page`, give the tab its body.

Before (plan 3):

```rust
        let body = match tab {
            Tab::Activity => self.factory_activity_tab(&t, cx).into_any_element(),
```

After:

```rust
        let body = match tab {
            Tab::Activity => self.factory_activity_tab(&t, cx).into_any_element(),
            Tab::Checks => div().id("factory-checks").flex_1().min_h_0().overflow_y_scroll().pb(px(72.)).child(self.factory_checks_tab(&t)).into_any_element(),
```

In `factory/checks.rs`, replace the `use` lines with these, and add the view after `checks_note`:

```rust
use super::spec::heading;
use crate::board::logic::Tone;
use crate::desktop::Desktop;
use agents::factory::Check;
use agents::tickets::{Status, Ticket};
use git::github::{Check as Run, Outcome};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
```

```rust
fn list(rows: impl Iterator<Item = Div>) -> Div {
    let rows = rows.enumerate().map(|(i, row)| row.when(i > 0, |r| r.border_t(px(0.5)).border_color(SEPARATOR)));
    div().flex().flex_col().rounded(px(9.)).bg(SURFACE).shadow(vec![ui::ring(SEPARATOR, 0.5)]).overflow_hidden().children(rows)
}

fn line(mark: AnyElement, name: Div, end: Div) -> Div {
    div().px(px(12.)).py(px(9.)).flex().items_center().gap(px(10.)).text_size(px(13.)).child(div().w(px(14.)).flex_none().flex().justify_center().child(mark)).child(name).child(end)
}

fn quiet(text: &'static str) -> Div {
    div().text_size(px(13.5)).line_height(px(21.)).text_color(TEXT_3).child(text)
}

impl Desktop {
    /// The Checks tab of `t`'s page: each local command, the failure with its attempts, then the PR's checks.
    pub(super) fn factory_checks_tab(&self, t: &Ticket) -> Div {
        let detail = self.agents.detail(&t.id);
        let config = self.agents.factory_config(&t.folder);
        let commands = config.map(|c| c.checks.as_slice()).unwrap_or_default();
        let cap = config.map_or(t.fix, |c| c.fix);
        let checks = detail.map(|d| d.checks.as_slice()).unwrap_or_default();
        let fixing = fixing(t);
        let rows = rows(commands, checks, fixing);
        let running = t.status == Status::Working && t.why.starts_with("Checks · ");
        let mut doc = div().flex().flex_col().child(heading("Checks", Some(format!("{}/{} passed", passed(&rows), rows.len())), None).mt(px(18.)));
        doc = match (rows.is_empty(), detail.is_some_and(|d| d.no_checks)) {
            (true, true) => doc.child(quiet("No checks ran: this project has no local commands and GitHub reported none")),
            (true, false) => doc.child(quiet("This project runs no local commands")),
            _ => {
                let first_waiting = rows.iter().position(|r| r.mark == Mark::Waiting);
                doc.child(list(rows.iter().enumerate().map(|(i, row)| check_row(i, row, running && first_waiting == Some(i)))))
            }
        };
        if t.status == Status::Failed || fixing {
            doc = doc.child(self.checks_callout(t, cap, fixing));
        }
        if let Some(pr) = self.prs.get(&t.worktree).filter(|pr| !pr.runs.is_empty()) {
            let done = pr.runs.iter().filter(|r| matches!(r.outcome, Outcome::Passed | Outcome::Skipped)).count();
            doc = doc.child(heading("GitHub checks", Some(format!("{done}/{} passed", pr.runs.len())), None)).child(list(pr.runs.iter().enumerate().map(|(i, run)| run_row(i, run))));
        }
        doc
    }

    fn checks_callout(&self, t: &Ticket, cap: u8, fixing: bool) -> Div {
        let (title, title_color, line_color, bg) = if fixing { ("Fixing".to_string(), TEXT, TEXT_3, FILL_1) } else { (what(&t.why).to_string(), FAILED_TEXT, FAILED_TEXT, FAILED_BG) };
        let head = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .text_size(px(13.))
            .child(div().min_w_0().truncate().font_weight(FontWeight::SEMIBOLD).text_color(title_color).child(title))
            .child(div().ml_auto().flex_none().text_size(px(12.)).text_color(line_color).child(attempt_line(t, cap)));
        let output = self.factory.outputs.get(&t.id).map(|out| {
            div()
                .mt(px(8.))
                .flex()
                .flex_col()
                .gap(px(4.))
                .children(hidden(out).map(|note| div().text_size(px(11.5)).text_color(TEXT_3).child(note)))
                .child(div().font_family(MONO).text_size(px(11.5)).line_height(px(17.)).text_color(TEXT_2).whitespace_nowrap().overflow_hidden().child(out.text.clone()))
        });
        div()
            .mt(px(10.))
            .px(px(14.))
            .py(px(12.))
            .rounded(px(9.))
            .bg(bg)
            .when(!fixing, |d| d.border_1().border_color(FAILED.opacity(0.2)))
            .flex()
            .flex_col()
            .child(head)
            .children(output)
    }
}

fn check_row(i: usize, row: &Row, running: bool) -> Div {
    let (mark, end): (AnyElement, &str) = match row.mark {
        Mark::Passed => (icon("check", 13., SUCCESS).into_any_element(), ""),
        Mark::Failed => (icon("x", 13., FAILED).into_any_element(), "Failed"),
        Mark::Fixing => (spinner(("check-fix", i), 11., WAITING).into_any_element(), "Fixing"),
        Mark::Waiting if running => (spinner(("check-run", i), 11., TEXT_3).into_any_element(), "Running"),
        Mark::Waiting => (ui::dot(6., TEXT_4).into_any_element(), ""),
    };
    let exit = (row.mark == Mark::Failed && row.exit != 0).then(|| format!("exit {}", row.exit));
    let name = div().flex_1().min_w_0().truncate().font_family(MONO).text_size(px(12.)).text_color(TEXT_2).child(row.command.clone());
    let end = div().flex_none().flex().gap(px(8.)).text_size(px(12.)).text_color(if row.mark == Mark::Failed { FAILED_TEXT } else { TEXT_3 }).children(exit).child(end);
    line(mark, name, end)
}

fn run_row(i: usize, run: &Run) -> Div {
    let (mark, end): (AnyElement, &str) = match run.outcome {
        Outcome::Passed => (icon("check", 13., SUCCESS).into_any_element(), ""),
        Outcome::Skipped => (ui::dot(6., TEXT_4).into_any_element(), "skipped"),
        Outcome::Pending => (spinner(("gh-run", i), 11., WAITING).into_any_element(), "in progress"),
        Outcome::Failed => (icon("x", 13., FAILED).into_any_element(), "Failed"),
    };
    let name = div().flex_1().min_w_0().truncate().text_color(TEXT).child(run.name.clone());
    line(mark, name, div().flex_none().text_size(px(12.)).text_color(if run.outcome == Outcome::Failed { FAILED_TEXT } else { TEXT_3 }).child(end))
}
```

**Step 4: Run it**

Run: `cd packages/desktop && cargo test -p pocket factory:: && cargo build --workspace`
Expected: `test result: ok` and a clean build. If `Token` has no `opacity`, draw the hairline with `FAILED_BG` at full width (`border_color(FAILED_BG)`) and say so in the PR. If `ml_auto` is not a method of this GPUI version, put a `div().flex_1()` before the attempt line, as plan 3's `heading` notes.

### Task 4.4: The footer bar of a failed Ticket: a hint, or close it

**Files:**
- Modify: `packages/desktop/crates/pocket/src/factory/bar.rs` (`Decision`, `decision`, `page_decision`, `factory_key`, `factory_bar`; new `hint_text`, `failed_bar`, `factory_send_hint`, `factory_close`)
- Modify: `packages/desktop/crates/pocket/src/factory.rs` (`FactoryState.hint`, `FactoryState::new`)

**Context:** The index declares `Decision::Failed { used, cap }`. Plan 4 left `decision` as a `match t.gate` with an arm for an idle Ticket at Implement, `page_decision` with an `other => other` arm, `factory_key` with a `_ => false` arm, and `factory_bar` as a `match` that puts each decision's content in one shell (plan 4, Task 6.2). The prototype's failed bar (`barHTML`, `app.js:397`): a red dot, **Checks failed**, a text field `Hint for the lead`, **Close ticket** (ghost) and **Send back with a hint** (primary, `⌘↵`), which stays disabled while the field is empty. There is no Retry button (US-26). `decision` gets no project settings, so it reports the Ticket's own count as the cap, and `page_decision` puts in the project's `fix`, as it puts in the kept tasks of a plan. **Close ticket** is the Board's `board.close` (`board.rs:305`).

**Step 1: Write the failing test**

Add to the tests of `factory/bar.rs`:

```rust
    #[test]
    fn a_ticket_that_failed_at_checks_asks_for_a_hint() {
        let t = Ticket { state: State::Live, station: Station::Checks, status: Status::Failed, fix: 2, agent_id: "a1".into(), ..Ticket::sample() };
        assert_eq!(decision(&t, None, None, None, None), Some(Decision::Failed { used: 2, cap: 2 }));
        assert_eq!(decision(&Ticket { status: Status::Working, ..t.clone() }, None, None, None, None), None);
        assert_eq!(decision(&Ticket { station: Station::Spec, ..t }, None, None, None, None), None);
    }

    #[test]
    fn a_hint_needs_words() {
        assert_eq!(hint_text("  Update the snapshot \n"), Some("Update the snapshot"));
        assert_eq!(hint_text(" \n"), None);
    }
```

**Step 2: Run it**

Run: `cd packages/desktop && cargo test -p pocket factory::bar 2>&1 | tail`
Expected: the crate does not compile, with `no variant named `Failed` found for enum `Decision`` and `cannot find function `hint_text` in this scope`.

**Step 3: Implement**

In `factory/bar.rs`, add the variant to `Decision`, after `Idle`:

```rust
    /// The Ticket failed at Checks. `used` of `cap` auto-fix attempts are spent.
    Failed { used: u8, cap: u8 },
```

In `decision`, after plan 4's arm `_ if t.station == Station::Implement && t.status == Status::Idle => Some(Decision::Idle),` add:

```rust
        // The project's cap is not known here; page_decision fills it in.
        _ if t.station == Station::Checks && t.status == Status::Failed => Some(Decision::Failed { used: t.fix, cap: t.fix }),
```

In `page_decision`, before `other => other,` add:

```rust
            Decision::Failed { used, .. } => Decision::Failed { used, cap: self.agents.factory_config(&t.folder).map_or(used, |c| c.fix) },
```

In `factory_key`, before `_ => false,` add:

```rust
            Some(Decision::Failed { .. }) => self.factory_send_hint(window, cx),
```

In `factory_bar`, in the `match`, after `Decision::Idle => self.idle_bar(t, cx),` add:

```rust
            Decision::Failed { cap, .. } => self.failed_bar(t, cap, cx),
```

Add after `change_note`:

```rust
pub(crate) fn hint_text(typed: &str) -> Option<&str> {
    Some(typed.trim()).filter(|t| !t.is_empty())
}
```

Add to the `impl Desktop` block of `bar.rs`:

```rust
    /// Failed at Checks: what failed and the attempts used, a hint for the lead, Close ticket and Send back with a hint.
    fn failed_bar(&self, t: &Ticket, cap: u8, cx: &mut Context<Self>) -> Div {
        let what = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(7.))
            .child(div().size(px(6.)).rounded(px(3.)).bg(FAILED))
            .child(div().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child("Checks failed"))
            .child(div().text_color(FAILED_TEXT).child(super::checks::attempt_line(t, cap)));
        let field = div().flex_1().min_w(px(120.)).h(px(28.)).px(px(9.)).flex().items_center().rounded(px(7.)).bg(FILL_2).child(Input::new(&self.factory.hint).appearance(false).p_0().text_size(px(12.5)));
        let can = hint_text(&self.factory.hint.read(cx).value()).is_some();
        let ticket = t.id.clone();
        let buttons = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .child(bar_button("factory-close", Variant::Ghost, None, "Close ticket").h(px(28.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.factory_close(&ticket, cx))))
            .child(
                bar_button("factory-hint", Variant::Primary, None, "Send back with a hint")
                    .h(px(28.))
                    .child(start_kbd())
                    .when(!can, |b| b.opacity(0.5))
                    .when(can, |b| b.on_click(cx.listener(|this, _: &ClickEvent, window, cx| _ = this.factory_send_hint(window, cx)))),
            );
        div().flex_1().flex().items_center().gap(px(10.)).child(what).child(field).child(buttons)
    }

    /// Sends the typed hint to the page's lead; pocketd grants one more attempt (C7).
    pub(crate) fn factory_send_hint(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let typed = self.factory.hint.read(cx).value().to_string();
        let Some(text) = hint_text(&typed) else { return false };
        let Some(ticket) = self.factory.page.as_ref().map(|p| p.ticket.clone()) else { return false };
        self.outbox.local.factory_hint(&ticket, text);
        self.factory.hint.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
        true
    }

    fn factory_close(&mut self, ticket: &str, cx: &mut Context<Self>) {
        self.outbox.local.board_close(ticket);
        cx.notify();
    }
```

In `factory.rs`, add the field to `FactoryState`:

```rust
    /// Where a hint for the lead of a Ticket that failed at Checks is typed.
    pub(crate) hint: Entity<InputState>,
```

In `FactoryState::new`, build it next to plan 3's `changes`, redraw the bar as you type so the button turns on and off, and return that subscription with the others:

```rust
        let hint = cx.new(|cx| InputState::new(window, cx).placeholder("Hint for the lead"));
        let typed = cx.subscribe(&hint, |_: &mut Desktop, _, _: &InputEvent, cx| cx.notify());
```

Add `hint` to the struct literal, and `typed` to the `Vec` of subscriptions it returns. Add `InputEvent` to the `gpui_kit::component::input` import.

**Step 4: Run it**

Run: `cd packages/desktop && cargo test -p pocket factory:: && cargo build --workspace && cargo clippy --workspace --all-targets`
Expected: `test result: ok`, a clean build, and no new clippy warning. If plan 6 has already added `Decision::Pr`, the new arms go beside its arms and nothing else changes.

### Task 4.5: Capture `factory-ticket-checks`

**Files:**
- Modify: `packages/desktop/crates/pocket/src/capture.rs` (`STEPS` at :31, a helper next to plan 4's `show_factory_implement`)

**Context:** Plan 2's fixture Ticket `t20`, "Paste files into the composer", is live at Checks with `status: "failed"`, `fix: 2`, `why: "cargo test failed in composer.rs"` and `worktree: "/tmp/wt/paste-files"`. The fixture sends no `factory.config` and answers no `factory.ticket`, so the step applies the settings and the detail in-process with `d.on_agents`, as plan 3's `show_factory_plan` does. The GitHub checks come from the desktop's PR cache, filled the way the `pr-loop` step does (`capture.rs:875`). The page opens on Checks, because Checks' tab is Checks (Task 4.3). A second step types a hint, so the primary button shows on. PR 4 commits only at its end, so the "before" pictures come from a second checkout of `HEAD`.

**Step 1: Take the "before" pictures**

From the repository root:

```bash
git worktree add /tmp/lf5-base HEAD
/tmp/lf5-base/.ui-review/fixture/capture.sh /tmp/lf5-before board-factory=board,board-factory factory-ticket-plan=board,factory-ticket-plan
git worktree remove /tmp/lf5-base
```

Expected: two PNG files in `/tmp/lf5-before`.

**Step 2: Add the steps**

In `capture.rs`, add two entries to `STEPS`, next to `"factory-ticket-idle"`, and add 2 to the array's length:

```rust
    ("factory-ticket-checks", |d, window, cx| show_factory_checks(d, window, cx)),
    ("factory-hint-typed", |d, window, cx| d.factory.hint.update(cx, |s, cx| s.set_value("The test pastes two files, the fixture holds one", window, cx))),
```

Add after `show_factory_implement`:

```rust
/// `t20` failed at Checks: fmt passed, `cargo test` failed after 2 of 2 auto-fixes, and the PR's GitHub checks.
fn show_factory_checks(d: &mut Desktop, window: &mut Window, cx: &mut Context<Desktop>) {
    let Some(project) = d.agents.board.tickets.iter().find(|t| t.id == "t20").map(|t| t.folder.clone()) else { return };
    let commands = ["cargo fmt --check", "cargo test", "cargo clippy --all-targets"];
    let config = agents::factory::Config { enabled: true, spec: true, reviewer: true, fix: 2, checks: commands.iter().map(|c| c.to_string()).collect(), ..Default::default() };
    d.on_agents(agents::Event::FactoryConfig { project, config, problem: String::new() }, window, cx);
    let tree = "/tmp/wt/paste-files".to_string();
    let run = |name: &str, outcome: Outcome| Check { name: name.into(), outcome, url: "https://github.com/acme/app/actions/runs/2".into() };
    let pr = Pr {
        number: 204,
        title: "Paste files into the composer".into(),
        url: "https://github.com/acme/app/pull/204".into(),
        draft: true,
        checks: Checks { passed: 2, failed: 0, pending: 1 },
        runs: vec![run("build", Outcome::Passed), run("lint", Outcome::Passed), run("test (ubuntu)", Outcome::Pending)],
        ..Pr::default()
    };
    let now = Instant::now();
    d.prs.start(&tree, now);
    d.prs.apply(tree, now, Ok(Some(pr)), now);
    d.open_factory_ticket("t20", window, cx);
    let mut output: String = (1..=44).map(|i| format!("   Compiling crate-{i:02} v0.1.0\n")).collect();
    output.push_str("running 12 tests\ntest composer::paste_keeps_order ... ok\ntest composer::paste_two_files ... FAILED\n\nfailures:\n---- composer::paste_two_files stdout ----\nthread 'composer::paste_two_files' panicked at crates/composer/src/composer.rs:212:9:\nassertion `left == right` failed: left: 1, right: 2\n");
    let at = crate::util::now_ms();
    let check = |command: &str, exit: i32, output: &str| agents::factory::Check { command: command.into(), exit, output: output.into(), attempt: 2, at };
    let detail = agents::factory::Detail {
        ticket_id: "t20".into(),
        checks: vec![check(commands[0], 0, ""), check(commands[1], 101, &output)],
        ..Default::default()
    };
    d.on_agents(agents::Event::FactoryTicket(detail), window, cx);
}
```

The output has 52 lines, so the tab cuts it to its last 40 and says so.

**Step 3: Take the "after" pictures and compare**

Run:

```bash
.ui-review/fixture/capture.sh /tmp/lf5-after board-factory=board,board-factory factory-ticket-plan=board,factory-ticket-plan factory-ticket-checks=board,factory-ticket-checks factory-hint-typed=board,factory-ticket-checks,factory-hint-typed
```

Expected: four PNG files in `/tmp/lf5-after`.

Open `/tmp/lf5-before/impl-board-factory.png` beside `/tmp/lf5-after/impl-board-factory.png`. They match, apart from times and the notes of the Checks cards: `t20` still reads `Failed · cargo test failed in composer.rs`. `impl-factory-ticket-plan.png` matches apart from times.

Open `/tmp/lf5-after/impl-factory-ticket-checks.png` beside `.ui-review/prototypes/light-factory/index.html` with `ANY-39` open (Checks, failed). Check:

- The status reads `Checks  failed`, with `failed` in red, and **Checks** is the chosen tab.
- The heading reads `Checks  1/3 passed`.
- One rounded list of three rows: `cargo fmt --check` with a green check, `cargo test` with a red cross, `exit 101` and `Failed` in red, and `cargo clippy --all-targets` with a grey dot and no end text. Commands are mono in `TEXT_2`.
- The callout has the red hairline and the light red tint. Its title `cargo test failed in composer.rs` and the line `Auto-fix 2 of 2 used` are red. The line `Showing the last 40 of 52 lines` is grey, and the output is neutral mono, 11.5 on 17, with no wrap.
- `GitHub checks  2/3 passed`, then `build` and `lint` with green checks and `test (ubuntu)` with a spinner and `in progress`.
- The footer bar: a red dot, **Checks failed**, `Auto-fix 2 of 2 used` in red, the field `Hint for the lead`, **Close ticket** (ghost), and **Send back with a hint** with `⌘↵`, dimmed. No Retry button.

Open `/tmp/lf5-after/impl-factory-hint-typed.png`. The field holds the hint, and **Send back with a hint** is no longer dimmed. Fix any difference in size, colour or text before you go on.

### PR 4 checks

**Files:** none.

**Step 1: Run the checks**

Run: `cd packages/desktop && cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: a clean build, no new clippy warning, and `test result: ok` for every crate. The capture comparison of Task 4.5 is done.

**Step 2: Commit**

```bash
git add packages/desktop
git commit -m "The Checks tab, the failed bar and the card note at Checks"
```

The red-team gate is not needed: this PR changes no pocketd code.

---

## Contract changes needed

1. **pocketd packages, `station.go`.** Replace the line

   ```go
   	Local    LocalChecks    // none | running | passed | failed, for the head they ran on
   ```

   with

   ```go
   	Local    LocalChecks    // plan 5: the newest check record, empty when its Head is not the checkout's commit
   ```

   and add after the `station.go` block:

   ```go
   // checks.go (plan 5)
   type LocalChecks struct {
   	Head        string
   	Pushed      bool
   	State       string // "" | running | passed | failed, for Head
   	Done, Total int
   	Failed      string // the command that failed
   	Fix         string // "" | mailed | ended | stopped: the fix note, then the lead's turn after it
   	Hint        bool   // a hint waits to be sent
   	Same        bool   // the fix turn before Head changed nothing (E6)
   	Error       string // a fault: a failure that is not a failed check
   	Quiet       bool   // GitHub reported no check for Head within 90 seconds
   }
   func ReadRecord(home, ticket string) (Record, bool)              // the newest state/factory/<ticket>/checks/NNNN.json
   func WriteRecord(home, ticket string, r Record, fresh bool) error // fresh starts the next file
   func RunCheck(ctx context.Context, env []string, dir, command string, limit time.Duration) (exit int, output string) // sh -c through hostop.Run, the last 64 KB
   func FixNote(attempt, limit int, hint string, checks []proto.FactoryCheck, pr *proto.PR) string
   func PRBody(spec, key string) string
   func TicketKey(t proto.Ticket) string
   var ErrNoHint error                                               // sent as ticket_busy
   ```

2. **pocketd packages, `type Engine struct`.** After the `GitHub` line, add:

   ```go
   	Env      func(dir string) []string                        // plan 5: the login environment a check command runs with; serve.go passes the closure github.New gets
   ```

3. **Hooks, `github.Service.Push`.** Replace the bullet with: "`github.Service.Push(ctx, tree string) error` (plan 5): the push that `Create` already does, exported. `Create` now expires the tree's cache entry after it opens the PR, as `after` does for every other verb, so the next `PR` read asks `gh` for the head and the checks."

4. **Decisions, "Auto-fix".** After the first sentence, add: "A fault (a merge conflict, a failed merge, push or `gh pr create`) fails the Ticket at Checks with the reason in `Why`, uses no attempt and sends no fix note. A hint does not add to `Fix`. A fix turn that leaves the commit unchanged runs the checks once more and sends no note on a second failure (E6). `Why` holds the card's note: `Checks · <done>/<n>`, `Fixing · attempt <n>`, `Fixing · your hint`, or `<what failed> · <why no fix follows>`."

5. **Decisions, "Checks".** At the end, add: "Each commit of the Ticket's branch gets one record in `state/factory/<ticket>/checks/NNNN.json`. If GitHub reports no check 90 seconds after the push, the station passes on the local commands alone. Waiting for GitHub keeps the Ticket `working`, so it holds its slot."

## Open questions

1. **Which key goes in the PR body?** Task 1.4 copies the desktop's `ticket_key` rule (three letters of the folder name, then the number). Plan 4's index text names `factory.Key(project, number)` for the same thing. Recommended: call plan 4's `Key` once it is merged and drop `TicketKey`. Until then, keep the copy, which gives the same text.
2. **A fix note that was out at a restart.** The engine cannot tell if the lead got it, so the Ticket stays `working` until the lead's next turn ends (Task 3.4). Recommended: accept for v1. The card reads `Fixing · attempt <n>`, and you can stop the lead and send a hint.
3. **Any normal turn end of the lead counts as the fix turn.** A turn that answers something else, sent while the note waits in the mailbox, ends the fix. Recommended: accept. The checks run again on what the checkout holds, and a commit that changed nothing is caught by E6.
4. **Pending GitHub checks have no time limit.** A workflow that never ends keeps the Ticket `working` and holding its slot. Recommended: accept for v1, and add a limit only when the metrics of plan 7 show it happens.
5. **A Mail that fails still uses the attempt.** `Fix` is saved before the note goes out, so a restart grants nothing (US-36). Recommended: accept. The Ticket fails with the Mail's error, and a hint grants one more attempt.
6. **Plan 6 names this plan's rows and helpers in other ways.** Plan 6 calls the base-conflict row "row 19". It is row 17 here. Plan 6 expects `factoryAtPR` to return `*fakeGh`, but plan 3 has a function named `fakeGh`, so Task 3.6 returns `*ghFolder` with the same `put` and `called` methods. Plan 6 also defines `factoryDetail(o, ticket)`, and plan 3 already has `factoryDetail(o, id, ticket)`. Recommended: plan 6 uses row 17, `*ghFolder` and plan 3's `factoryDetail`.
7. **Is 90 seconds the right wait for GitHub to report checks?** Three polls of a watched tree. Recommended: keep it. A project without CI waits 90 seconds once per commit. A shorter wait could pass a Ticket before a slow workflow appears.
