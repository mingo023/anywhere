# Light Factory 1: Foundations Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** pocketd and the desktop gain every Factory contract that the later plans build on: the `factory.v1` cap, five server-owned Ticket fields, a per-project config store with `factory.config.get` and `factory.config.save`, the scope rows of all ten Factory verbs, the slot rule (a Ticket that waits on you holds no slot) and the project slot limit. No screen is added. The only change a user can see is the slot rule itself.

**Architecture:** `proto.Ticket.HoldsSlot` drops `needsYou`, and `board.Store.Claim` takes a function that returns a project's own limit. `board.Scheduler.Limit` passes that function in, so `board` never imports `factory`. `proto/factory.go` holds the cap, the station and gate constants, `FactoryConfig` and its strict decoder. `internal/factory/config.go` keeps the configs in `state/factory.json`, written with `atomicfile` as `board.Store` writes `board.json`. `wsserver/factory.go` serves the two verbs. It asks `github.Service.Status` and `github.Service.PR` whether the Factory can be turned on, and it answers a conn's config verbs in the order they came. `peer.Needs` gains the ten rows, and a test proves that a terminal peer reaches no verb that advances a gate. On the desktop, `agents::tickets::Ticket` gains the fields, `agents::factory::Config` mirrors the config, and `Outbox`, `Event` and `Agents` gain the Factory members that the index names.

**Toolset:**
- pocketd (from `packages/pocketd`. Run `scripts/build-ghostty.sh` once per machine before the first test of `board`, `peer`, `wsserver` or `e2e`): one test `go test ./internal/board/ -run '^TestName$'`, one package `go test ./internal/factory/`, everything `go vet ./...` and `go test ./...`, formatting `gofmt -l .`, goldens `go test ./internal/proto -run TestServerGolden -update`
- red-team gate (from the repository root): `scripts/red-team.sh`
- desktop (from `packages/desktop`): one test `cargo test -p agents <name>`, `cargo test -p hosts`, `cargo test -p pocket board::logic::tests`, everything `cargo build --workspace`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`
- screens (from the repository root): `.ui-review/fixture/capture.sh <dir> <name>=<steps>` (needs `bun`)

**Read first:**
- `docs/plans/2026-11-12-light-factory.md`: the index. It holds the contracts this plan implements. Do not change them here.
- `docs/designs/2026-11-12-light-factory-stories.md`: US-23 (slots), US-33 (turn the Factory on), US-35 (only you advance a gate). Their acceptance criteria are this plan's test cases.
- `CLAUDE.md` and `docs/adr/0003-desktop-code-layout.md`: logic in model crates, tests beside the logic and named as sentences, no mocks.
- `packages/pocketd/internal/board/store.go` `Open` (:55), `commit` (:119) and `Claim` (:318): the store pattern that `factory.Configs` copies, and the function that gains the project limit.
- `packages/pocketd/internal/wsserver/board.go` and `gh.go`: how a verb group checks its cap, maps errors to codes, and runs `gh` off the read loop.
- `packages/pocketd/internal/wsserver/gh_test.go` `ghFixture` (:22-74) and `internal/github/testdata/fakegh/gh`: the fake `gh` that the config tests reuse.
- `packages/pocketd/internal/peer/check.go` `Needs` (:15) and `check_test.go` (:17): the verb allowlist and its matrix test.
- `packages/desktop/crates/agents/src/agents.rs` `board_event` (:898), `Outbox::board_limit` (:1117) and `CAPS` (:1187): the Board's wire, which the Factory's wire copies.

---

## Decisions

- **Four PRs.** PR 1 is the slot rule on both sides. PR 2 adds the wire types to `proto` and serves nothing. PR 3 adds the store, the two verbs and the scope rows. PR 4 is the desktop's wire. PR 1 and PR 2 do not depend on each other.
- **The cap is served when the config store exists.** `factory.Engine` arrives with plan 3, and plan 2 needs `factory.v1` negotiated. So pocketd drops `factory.v1` from the served caps when `Server.FactoryConfigs` is nil.
- **`factory_off`.** A config verb answers `factory_off` when `Server.FactoryConfigs` is nil or the conn did not negotiate `factory.v1`. The scope check runs first, as it does for every verb.
- **Shape errors are `Malformed message`, range errors are `invalid_config`.** `decodeFactoryConfig` requires all nine keys and refuses any other key. `FactoryConfig.Valid` is checked by the handler and again by `Configs.Save`, which returns `factory.ErrInvalid`.
- **`project` must be a project on this Mac.** The handler calls `conn.usableFolder`, the check that `board.save` uses for `Ticket.Folder`. A folder that fails it answers `unknown_project`.
- **When pocketd asks `gh`.** `factory.config.get` always asks. `factory.config.save` asks unless the Factory was on and stays on, so a signed-out `gh` cannot lock the settings of a project whose Factory is already on. `github.Service.Status` reports a missing or signed-out `gh`. `github.Service.PR` on the project folder reports `github.NoRemote`. `github.Failed` (offline, GitHub down) does not block.
- **`problem` is the sentence the page shows.** The three texts: `Install GitHub CLI (gh) to use the Factory`, `Run gh auth login to use the Factory`, `This project has no GitHub remote`. A save that would turn the Factory on while a problem exists stores nothing and answers an `error` frame with code `factory_unavailable` and that sentence as its message.
- **A conn's config verbs answer in the order they came.** They run off the read loop, because `gh` takes seconds. Each waits for the one before it (`conn.factoryNext`), so an autosave cannot overtake the save before it.
- **Project limit.** A queued Ticket whose project is at its own limit is skipped, and the next queued Ticket of another project can start. A Ticket that was asked to start again ignores the project limit, as it ignores the Board limit.
- **`board.Scheduler.Limit`** carries `Configs.Limit` to `Store.Claim`. A nil `Limit` means no project has a limit.
- **`Scheduler.sweep`** keeps its old test through `inHand`, a new unexported function in `scheduler.go`.
- **Server goldens under `testdata/factory/server/` are generated** from a `factoryServerGolden` map by `TestServerGolden -update`. Client goldens under `testdata/factory/client/` are written by hand. No file under `testdata/golden/` changes.
- **Red-team probes.** This plan adds probes for `team.start`, `gh.pr.merge` and `factory.config.save`. The other five gate verbs do not decode yet, so a probe would only prove `Malformed message`. The plan that adds each decoder adds its probe.
- **Desktop.** `Ticket::sample()` is a plain `pub fn`, because the desktop workspace has no test-support feature. `agents::Config` is the re-export of `agents::factory::Config`. Factory events of a linked host are dropped in `hosts::session::tag_event`, as its Board is.
- **The composer's wait hint** names the real count (`6 of 4 working`), because a Ticket that resumes can put the count above the limit.
- **Story ids.** The slot story is US-23. US-22 is the never-allow list, which plan 4 owns.

---

## PR 1: The slot rule

**Scope:** A live Ticket holds a slot only while it is `starting` or `working`. `Store.Claim` takes a project limit, and `Scheduler.Limit` passes one in. The desktop's `busy` counts the same way. `CONTEXT.md` **Queued** says the new rule. Nothing sets `Scheduler.Limit` until PR 3, so no project has a limit of its own yet.
**Depends on:** nothing
**Done when:** from `packages/pocketd`, `go vet ./...` and `go test ./...` pass. From `packages/desktop`, `cargo build --workspace`, `cargo clippy --workspace --all-targets` (no new warnings) and `cargo test --workspace` pass. The `board` capture shows `2/4` in the Working heading where it showed `3/4`.

### Task 1.1: A Ticket that waits on you holds no slot

**Files:**
- Modify: `packages/pocketd/internal/proto/ticket.go:103-106` (`HoldsSlot`)
- Modify: `packages/pocketd/internal/proto/ticket_test.go:59-68`
- Modify: `packages/pocketd/internal/board/scheduler.go:189-196` (`sweep`), plus a new `inHand`
- Modify: `packages/pocketd/internal/board/scheduler_test.go:78-102` (`world`), plus three new tests
- Modify: `packages/pocketd/internal/board/store_test.go:96`
- Modify: `packages/pocketd/e2e/board_test.go:62-77`, plus one new test
- Modify: `CONTEXT.md:114-116` (**Queued**)

**Context:** `HoldsSlot` has two callers. `Store.Claim` (`store.go:325`) counts the Tickets that hold a slot. `Scheduler.sweep` (`scheduler.go:194`) skips them, so it never closes a Ticket whose session is still in use. The slot rule must change the first and not the second, so `sweep` moves to a new function `inHand` that keeps the old test. The scheduler's fake world (`scheduler_test.go:78`) returns one `launch.Result` for every Ticket. The new tests need one agent per Ticket, so `world` gains a flag `each` and a setter `see`. The e2e test `TestOverTheLimitATicketWaitsInQueued` holds its slot with a Ticket that asks (`run ls`). Under the new rule that Ticket frees its slot, so the test switches to `hang`, which keeps the fake claude working. Stories: US-23, criteria 1 to 4.

**Step 1: Write the failing tests**

In `internal/proto/ticket_test.go`, change the map in `TestOnlyTicketsWithWorkInHandHoldASlot` (:60). Before:

```go
	for status, want := range map[string]bool{TicketStarting: true, TicketWorking: true, TicketNeedsYou: true, TicketDone: false, TicketIdle: false, TicketFailed: false, TicketStopped: false} {
```

After:

```go
	for status, want := range map[string]bool{TicketStarting: true, TicketWorking: true, TicketNeedsYou: false, TicketDone: false, TicketIdle: false, TicketFailed: false, TicketStopped: false} {
```

In `internal/board/scheduler_test.go`, replace `world.start` (:87-95) and add `see` after `watch`. Before:

```go
type world struct {
	mu     sync.Mutex
	result launch.Result
	seen   map[string]Seen
	starts []proto.Ticket
	// cwd is the Worktree told about while launching.
	cwd string
}

func (w *world) start(_ string, t proto.Ticket, created func(string)) launch.Result {
	if w.cwd != "" {
		created(w.cwd)
	}
	w.mu.Lock()
	defer w.mu.Unlock()
	w.starts = append(w.starts, t)
	return w.result
}
```

After:

```go
type world struct {
	mu     sync.Mutex
	result launch.Result
	// each gives every ticket an agent named after its title instead of result.
	each   bool
	seen   map[string]Seen
	starts []proto.Ticket
	// cwd is the Worktree told about while launching.
	cwd string
}

func (w *world) start(_ string, t proto.Ticket, created func(string)) launch.Result {
	if w.cwd != "" {
		created(w.cwd)
	}
	w.mu.Lock()
	defer w.mu.Unlock()
	w.starts = append(w.starts, t)
	if w.each {
		return launch.Result{AgentID: t.Title, TerminalID: "tm-" + t.Title}
	}
	return w.result
}

func (w *world) see(agentID, status string) {
	w.mu.Lock()
	defer w.mu.Unlock()
	w.seen[agentID] = Seen{Status: status}
}
```

Add these three tests at the end of `scheduler_test.go`:

```go
func TestATicketThatWaitsOnYouFreesItsSlotForTheNextInLine(t *testing.T) {
	s := Open(t.TempDir())
	s.SetLimit(1)
	w := &world{each: true, seen: map[string]Seen{"a": {Status: "working"}, "b": {Status: "working"}}}
	a := saved(t, s, "a", true)
	b := saved(t, s, "b", true)
	running(t, s, w, nil)
	waitFor(t, "a working", func() bool { return got(t, s, a.ID).Status == proto.TicketWorking })
	if g := got(t, s, b.ID); g.State != proto.TicketQueued {
		t.Fatalf("b started under a full limit: %+v", g)
	}
	w.see("a", "needsYou")
	waitFor(t, "b working", func() bool { return got(t, s, b.ID).Status == proto.TicketWorking })
	if g := got(t, s, a.ID); g.State != proto.TicketLive || g.Status != proto.TicketNeedsYou {
		t.Fatalf("got %+v", g)
	}
}

func TestAnAnsweredTicketResumesAboveTheLimitAndNothingStartsUntilTheCountIsBelowIt(t *testing.T) {
	s := Open(t.TempDir())
	s.SetLimit(1)
	w := &world{each: true, seen: map[string]Seen{"a": {Status: "needsYou"}, "b": {Status: "working"}, "c": {Status: "working"}}}
	a := saved(t, s, "a", true)
	b := saved(t, s, "b", true)
	c := saved(t, s, "c", true)
	running(t, s, w, nil)
	waitFor(t, "a asking and b working", func() bool {
		return got(t, s, a.ID).Status == proto.TicketNeedsYou && got(t, s, b.ID).Status == proto.TicketWorking
	})
	w.see("a", "working")
	waitFor(t, "a working again", func() bool { return got(t, s, a.ID).Status == proto.TicketWorking })
	w.see("b", "done")
	waitFor(t, "b done", func() bool { return got(t, s, b.ID).Status == proto.TicketDone })
	time.Sleep(40 * time.Millisecond)
	if g := got(t, s, c.ID); g.State != proto.TicketQueued {
		t.Fatalf("c started while a held the only slot: %+v", g)
	}
	w.see("a", "done")
	waitFor(t, "c working", func() bool { return got(t, s, c.ID).Status == proto.TicketWorking })
}

func TestADeletedWorktreeLeavesATicketThatWaitsOnYouAlone(t *testing.T) {
	s := Open(t.TempDir())
	x := saved(t, s, "a", true)
	s.Claim(at)
	s.Update(x.ID, func(t *proto.Ticket) { t.AgentID, t.Status, t.Worktree = "a", proto.TicketNeedsYou, "/nowhere/at/all" })
	(&Scheduler{Store: s, Now: time.Now}).sweep(at)
	if g := got(t, s, x.ID); g.State != proto.TicketLive || g.Worktree != "/nowhere/at/all" {
		t.Fatalf("got %+v", g)
	}
}
```

The third test passes today. It pins `sweep`, so it must still pass after Step 3.

In `internal/board/store_test.go`, `TestAClosedTicketStopsHoldingItsSlot` sets `TicketNeedsYou` before it closes the Ticket (:96). Under the new rule that Ticket holds no slot even before the close, so the test would prove nothing. Before:

```go
	s.Update(a.ID, func(x *proto.Ticket) { x.Status = proto.TicketNeedsYou })
```

After:

```go
	s.Update(a.ID, func(x *proto.Ticket) { x.Status = proto.TicketWorking })
```

In `e2e/board_test.go`, change `TestOverTheLimitATicketWaitsInQueued` (:68-69). Before:

```go
	o.Board("save", "s1", map[string]any{"ticket": ticketDraft(h, "run ls"), "start": true})
	first := o.WaitTicket("the first asking", func(x proto.Ticket) bool { return x.Status == proto.TicketNeedsYou })
```

After:

```go
	o.Board("save", "s1", map[string]any{"ticket": ticketDraft(h, "hang"), "start": true})
	first := o.WaitTicket("the first working", func(x proto.Ticket) bool { return x.Status == proto.TicketWorking })
```

Add this test after it:

```go
func TestATicketThatAsksLetsTheNextInLineStart(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner(proto.CapBoard)
	if a := o.Board("limit", "l1", map[string]any{"limit": 1}); a.Type != "ack" {
		t.Fatalf("got %s", a.Raw)
	}
	o.Board("save", "s1", map[string]any{"ticket": ticketDraft(h, "run ls"), "start": true})
	first := o.WaitTicket("the first asking", func(x proto.Ticket) bool { return x.Status == proto.TicketNeedsYou })
	o.Board("save", "s2", map[string]any{"ticket": ticketDraft(h, "hello"), "start": true})
	o.WaitTicket("the second started", func(x proto.Ticket) bool { return x.ID != first.ID && x.State == proto.TicketLive })
}
```

**Step 2: Run the tests and see them fail**

From `packages/pocketd`:

```sh
go test ./internal/proto -run '^TestOnlyTicketsWithWorkInHandHoldASlot$'
go test ./internal/board/ -run 'TestATicketThatWaitsOnYou|TestAnAnsweredTicketResumes|TestADeletedWorktreeLeavesATicketThatWaitsOnYouAlone'
go test ./e2e -count=1 -run '^(TestOverTheLimitATicketWaitsInQueued|TestATicketThatAsksLetsTheNextInLineStart)$'
```

Expected: the first command fails with `needsYou: true`. The second fails with `waited for b working` and `waited for a asking and b working`, and `TestADeletedWorktreeLeavesATicketThatWaitsOnYouAlone` passes. The third fails in `TestATicketThatAsksLetsTheNextInLineStart` with `waiting for the second started: ... context deadline exceeded`, and `TestOverTheLimitATicketWaitsInQueued` passes.

**Step 3: Implement**

`internal/proto/ticket.go:103-106`. Before:

```go
// HoldsSlot is whether t counts against the Board's limit.
func (t Ticket) HoldsSlot() bool {
	return t.State == TicketLive && slices.Contains([]string{TicketStarting, TicketWorking, TicketNeedsYou}, t.Status)
}
```

After:

```go
// HoldsSlot is whether t counts against the Board's limit. One that waits on
// the user doesn't, so the user's delay holds no other ticket back.
func (t Ticket) HoldsSlot() bool {
	return t.State == TicketLive && (t.Status == TicketStarting || t.Status == TicketWorking)
}
```

`ticket.go` still uses `slices` in `validLabels`, so the import stays.

`internal/board/scheduler.go:191-196`. Before:

```go
func (s *Scheduler) sweep(now time.Time) {
	tickets, _ := s.Store.Snapshot()
	for _, t := range tickets {
		if t.Worktree == "" || t.HoldsSlot() {
			continue
		}
```

After:

```go
func (s *Scheduler) sweep(now time.Time) {
	tickets, _ := s.Store.Snapshot()
	for _, t := range tickets {
		if t.Worktree == "" || inHand(t) {
			continue
		}
```

Add `inHand` above `sweep`'s doc comment:

```go
// inHand is whether t's session still works or waits on the user. sweep
// leaves such a ticket alone, though one that waits holds no slot.
func inHand(t proto.Ticket) bool {
	return t.HoldsSlot() || t.State == proto.TicketLive && t.Status == proto.TicketNeedsYou
}
```

`CONTEXT.md:114-116`. Before:

```markdown
**Queued**:
Started and waiting for a slot. pocketd starts the first in line while fewer Tickets than the limit are Starting, Working or Needs you.
_Avoid_: pending, backlog
```

After:

```markdown
**Queued**:
Started and waiting for a slot. pocketd starts the first in line while fewer Tickets than the limit are Starting or Working. A Ticket that waits on you holds no slot. When you answer, it works again at once, even if that puts the count above the limit, and no queued Ticket starts until the count is below the limit again.
_Avoid_: pending, backlog
```

**Step 4: Run the tests and see them pass**

Run the three commands of Step 2 again. Expected: `ok` for each package. Then run the two packages whole:

```sh
go test ./internal/proto ./internal/board/
```

Expected: `ok  	pocketd/internal/proto` and `ok  	pocketd/internal/board`.

### Task 1.2: `Store.Claim` takes a project limit

**Files:**
- Modify: `packages/pocketd/internal/board/store.go:316-356` (`Claim`)
- Modify: `packages/pocketd/internal/board/store_test.go` (a helper, three new tests, every `s.Claim(at)` call)
- Modify: `packages/pocketd/internal/board/scheduler_test.go` (every `s.Claim(at)` call)
- Modify: `packages/pocketd/internal/board/scheduler.go:119` (the one caller outside tests)

**Context:** The index fixes the signature: `Claim(now time.Time, limit func(folder string) int) []proto.Ticket`, and `limit` may be nil. `limit` returns 0 for a project with no limit of its own. The project limit can only lower the Board limit, so `Claim` checks the Board limit first and stops there. A project at its own limit is skipped, so its queued Tickets do not hold back another project's Ticket. `Claim` calls `limit` while it holds the store's lock. PR 3 passes `Configs.Limit`, which takes only its own lock and never calls the store, so the lock order is safe. `store_test.go` and `scheduler_test.go` call `s.Claim(at)` 31 times after Task 1.1. Stories: US-23, criterion 4, and PRD I5.

**Step 1: Write the failing tests**

In `internal/board/store_test.go`, add these helpers after `titles` (:45):

```go
func savedIn(t *testing.T, s *Store, title, folder string) proto.Ticket {
	t.Helper()
	d := draft(title)
	d.Folder = folder
	x, err := s.Save(d, true, at)
	if err != nil {
		t.Fatal(err)
	}
	return x
}

func limits(by map[string]int) func(string) int {
	return func(folder string) int { return by[folder] }
}
```

Add these tests after `TestClaimStartsTheOldestQueuedUpToTheLimit` (:76):

```go
func TestAProjectLimitOnlyLowersTheBoardLimit(t *testing.T) {
	s := Open(t.TempDir())
	s.SetLimit(2)
	for _, n := range []string{"a", "b", "c"} {
		saved(t, s, n, true)
	}
	if c := titles(s.Claim(at, limits(map[string]int{"/repo": 1}))); !slices.Equal(c, []string{"a"}) {
		t.Fatalf("a project limit of 1 under a Board limit of 2: %v", c)
	}
	if c := titles(s.Claim(at, limits(map[string]int{"/repo": 8}))); !slices.Equal(c, []string{"b"}) {
		t.Fatalf("a project limit of 8 under a Board limit of 2: %v", c)
	}
}

func TestTheBoardLimitStillCapsAllProjectsTogether(t *testing.T) {
	s := Open(t.TempDir())
	s.SetLimit(2)
	savedIn(t, s, "a", "/one")
	savedIn(t, s, "b", "/two")
	savedIn(t, s, "c", "/three")
	if c := titles(s.Claim(at, limits(map[string]int{"/one": 2, "/two": 2, "/three": 2}))); !slices.Equal(c, []string{"a", "b"}) {
		t.Fatalf("got %v", c)
	}
}

func TestAProjectAtItsLimitDoesNotHoldBackAnotherProjectsTicket(t *testing.T) {
	s := Open(t.TempDir())
	savedIn(t, s, "a", "/one")
	savedIn(t, s, "b", "/one")
	savedIn(t, s, "c", "/two")
	if c := titles(s.Claim(at, limits(map[string]int{"/one": 1}))); !slices.Equal(c, []string{"a", "c"}) {
		t.Fatalf("got %v", c)
	}
}
```

Then give every other call the nil limit. From `packages/pocketd`:

```sh
perl -pi -e 's/\.Claim\(at\)/.Claim(at, nil)/g' internal/board/store_test.go internal/board/scheduler_test.go
```

**Step 2: Run the tests and see them fail**

```sh
go test ./internal/board/ -run 'TestAProjectLimit|TestTheBoardLimitStillCaps|TestAProjectAtItsLimit'
```

Expected: the package does not compile. The first error reads `too many arguments in call to s.Claim` with `have (time.Time, func(string) int)` and `want (time.Time)`.

**Step 3: Implement**

`internal/board/store.go:316-356`. Before:

```go
// Claim returns the tickets to launch now: every one asked to start again,
// then the first in line that fit under the limit, marked as starting.
func (s *Store) Claim(now time.Time) []proto.Ticket {
	var claimed []proto.Ticket
	err := s.commit(func(f *File) error {
		claimed = nil
		busy := 0
		var queued []int
		for i, t := range f.Tickets {
			if t.HoldsSlot() {
				busy++
			}
			if t.State == proto.TicketQueued {
				queued = append(queued, i)
			}
			if t.Relaunch {
				f.Tickets[i].Relaunch = false
				claimed = append(claimed, f.Tickets[i])
			}
		}
		slices.SortFunc(queued, func(a, b int) int { return cmp.Compare(f.Tickets[a].Order, f.Tickets[b].Order) })
		for _, i := range queued {
			if busy >= f.Limit {
				break
			}
			t := &f.Tickets[i]
			t.State, t.Status, t.ChangedAt = proto.TicketLive, proto.TicketStarting, now.UnixMilli()
			claimed = append(claimed, *t)
			busy++
		}
```

After:

```go
// Claim returns the tickets to launch now: every one asked to start again,
// then the first in line that fit under the limit, marked as starting. limit
// is a project folder's own limit, 0 for none; it only lowers the Board's. A
// nil limit gives no project one.
func (s *Store) Claim(now time.Time, limit func(folder string) int) []proto.Ticket {
	var claimed []proto.Ticket
	err := s.commit(func(f *File) error {
		claimed = nil
		busy := 0
		busyIn := map[string]int{}
		var queued []int
		for i, t := range f.Tickets {
			if t.HoldsSlot() {
				busy++
				busyIn[t.Folder]++
			}
			if t.State == proto.TicketQueued {
				queued = append(queued, i)
			}
			if t.Relaunch {
				f.Tickets[i].Relaunch = false
				claimed = append(claimed, f.Tickets[i])
			}
		}
		slices.SortFunc(queued, func(a, b int) int { return cmp.Compare(f.Tickets[a].Order, f.Tickets[b].Order) })
		for _, i := range queued {
			if busy >= f.Limit {
				break
			}
			t := &f.Tickets[i]
			if limit != nil {
				if n := limit(t.Folder); n > 0 && busyIn[t.Folder] >= n {
					continue
				}
			}
			t.State, t.Status, t.ChangedAt = proto.TicketLive, proto.TicketStarting, now.UnixMilli()
			claimed = append(claimed, *t)
			busy++
			busyIn[t.Folder]++
		}
```

The rest of the function (from `if len(claimed) == 0`) does not change.

`internal/board/scheduler.go:119`. Before:

```go
		for _, t := range s.Store.Claim(now) {
```

After:

```go
		for _, t := range s.Store.Claim(now, nil) {
```

Task 1.3 replaces that `nil`.

**Step 4: Run the tests and see them pass**

```sh
go test ./internal/board/
go vet ./...
```

Expected: `ok  	pocketd/internal/board`, and `go vet` prints nothing. `go vet` proves that no other caller of `Store.Claim` exists.

### Task 1.3: `Scheduler.Limit` carries the project limit

**Files:**
- Modify: `packages/pocketd/internal/board/scheduler.go:44-55` (`Scheduler`), `:119` (`Run`)
- Modify: `packages/pocketd/internal/board/scheduler_test.go` (one new test)

**Context:** `board` must not import `internal/factory`, because plan 3's `factory.Engine` imports `board`. So the scheduler takes the limit as a function field, as it takes `Start`, `Watch` and `Merged`. PR 3 sets it to `Configs.Limit` in `cmd/pocketd/serve.go`. `Scheduler` has no constructor: every caller builds the struct, and a caller that sets no `Limit` gets nil.

**Step 1: Write the failing test**

Add at the end of `internal/board/scheduler_test.go`:

```go
func TestTheSchedulerHoldsAProjectToItsOwnLimit(t *testing.T) {
	s := Open(t.TempDir())
	w := &world{each: true, seen: map[string]Seen{"a": {Status: "working"}, "b": {Status: "working"}}}
	a := saved(t, s, "a", true)
	b := saved(t, s, "b", true)
	ctx, cancel := context.WithCancel(context.Background())
	sch := &Scheduler{Store: s, Now: time.Now, Poll: 5 * time.Millisecond, Start: w.start, Watch: w.watch, Limit: func(string) int { return 1 }}
	done := make(chan struct{})
	go func() { sch.Run(ctx); close(done) }()
	t.Cleanup(func() { cancel(); <-done })
	waitFor(t, "a working", func() bool { return got(t, s, a.ID).Status == proto.TicketWorking })
	time.Sleep(40 * time.Millisecond)
	if g := got(t, s, b.ID); g.State != proto.TicketQueued {
		t.Fatalf("b started over its project's limit: %+v", g)
	}
	w.see("a", "done")
	waitFor(t, "b working", func() bool { return got(t, s, b.ID).Status == proto.TicketWorking })
}
```

**Step 2: Run the test and see it fail**

```sh
go test ./internal/board/ -run '^TestTheSchedulerHoldsAProjectToItsOwnLimit$'
```

Expected: the package does not compile, with `unknown field Limit in struct literal of type Scheduler`.

**Step 3: Implement**

`internal/board/scheduler.go:44-55`. Before:

```go
	// Merged says tree's PR is merged; nil never closes a ticket for it.
	Merged func(ctx context.Context, tree string) bool

	asking atomic.Bool
}
```

After:

```go
	// Merged says tree's PR is merged; nil never closes a ticket for it.
	Merged func(ctx context.Context, tree string) bool
	// Limit is a project folder's own slot limit, 0 for none; nil gives no project one.
	Limit func(folder string) int

	asking atomic.Bool
}
```

`scheduler.go:119`. Before:

```go
		for _, t := range s.Store.Claim(now, nil) {
```

After:

```go
		for _, t := range s.Store.Claim(now, s.Limit) {
```

**Step 4: Run the tests and see them pass**

```sh
go test ./internal/board/
```

Expected: `ok  	pocketd/internal/board`.

### Task 1.4: Desktop: `busy` follows the slot rule

**Files:**
- Modify: `packages/desktop/crates/pocket/src/board/logic.rs:211-219` (`busy`, `wait_hint`), `:881-891` (tests)

**Context:** `busy` (`logic.rs:212`) has two callers. `board/column.rs:17` writes `busy` over the limit in the Working heading. `board/composer.rs:89` passes it to `Values::hint`, which warns that a new Ticket will wait. Both must count as pocketd counts. `wait_hint` writes the limit twice (`4 of 4 working`). A Ticket that resumes can now put the count above the limit, so the hint names the real count. No other text in the UI states the old rule: `Tickets wait here until a slot frees up.` (`logic.rs:60`) stays true. The fixture (`.ui-review/fixture/fixture.ts:265-277`) has one Ticket working, one starting and one that needs you, with a limit of 4.

**Step 1: Capture the screen before the change**

From the repository root:

```sh
.ui-review/fixture/capture.sh /tmp/lf1-before board=board
```

Expected: `/tmp/lf1-before/board.png`, with `3/4` in the Working heading.

**Step 2: Write the failing tests**

In `logic.rs`, change `only_live_tickets_with_work_in_hand_count_against_the_limit` (:881-885) and add a test after it. Before:

```rust
        assert_eq!(busy(&tickets), 3);
```

After:

```rust
        assert_eq!(busy(&tickets), 2);
```

```rust
    #[test]
    fn a_ticket_that_waits_on_you_holds_no_slot() {
        assert_eq!(busy(&[live(Status::NeedsYou)]), 0);
        assert_eq!(busy(&[live(Status::Working), live(Status::NeedsYou)]), 1);
    }
```

Add this line at the end of `the_composer_warns_only_when_every_slot_is_taken` (:887):

```rust
        assert_eq!(wait_hint(6, 4).as_deref(), Some("6 of 4 working, so it will wait in Queued"));
```

**Step 3: Run the tests and see them fail**

From `packages/desktop`:

```sh
cargo test -p pocket board::logic::tests
```

Expected: three failures. `only_live_tickets_with_work_in_hand_count_against_the_limit` reports `left: 3` and `right: 2`. `a_ticket_that_waits_on_you_holds_no_slot` reports `left: 1` and `right: 0`. `the_composer_warns_only_when_every_slot_is_taken` reports `4 of 4 working` against `6 of 4 working`.

**Step 4: Implement**

`logic.rs:211-219`. Before:

```rust
/// The tickets counted against the Board's limit.
pub(crate) fn busy(tickets: &[Ticket]) -> usize {
    tickets.iter().filter(|t| t.state == State::Live && matches!(t.status, Status::Starting | Status::Working | Status::NeedsYou)).count()
}

/// Said in the composer when a new ticket would not start right away.
pub(crate) fn wait_hint(busy: usize, limit: usize) -> Option<String> {
    (busy >= limit).then(|| format!("{limit} of {limit} working, so it will wait in Queued"))
}
```

After:

```rust
/// The tickets counted against the Board's limit. One that waits on you holds no slot, as in pocketd's `Ticket.HoldsSlot`.
pub(crate) fn busy(tickets: &[Ticket]) -> usize {
    tickets.iter().filter(|t| t.state == State::Live && matches!(t.status, Status::Starting | Status::Working)).count()
}

/// Said in the composer when a new ticket would not start right away. A ticket that resumes can put `busy` above `limit`.
pub(crate) fn wait_hint(busy: usize, limit: usize) -> Option<String> {
    (busy >= limit).then(|| format!("{busy} of {limit} working, so it will wait in Queued"))
}
```

**Step 5: Run the tests and see them pass**

```sh
cargo test -p pocket board::
```

Expected: `test result: ok` with no failure. `board::values::tests::the_hint_names_a_folder_that_is_no_repository_first` still passes, because it asks for `4 of 4`.

**Step 6: Capture the screen after the change and compare**

From the repository root:

```sh
.ui-review/fixture/capture.sh /tmp/lf1-after board=board
```

Open `/tmp/lf1-before/board.png` and `/tmp/lf1-after/board.png`. Expected: the Working heading reads `2/4` where it read `3/4`. Nothing else differs.

### PR 1 final checks

From `packages/pocketd`: `go vet ./...` (no output), `go test ./...` (every package `ok`), `gofmt -l .` (no output).
From `packages/desktop`: `cargo build --workspace`, `cargo clippy --workspace --all-targets` (no new warnings), `cargo test --workspace`.
The TypeScript and Swift golden suites read only `internal/proto/testdata/golden/`. This PR changes no file there: `git status --short packages/pocketd/internal/proto/testdata` prints nothing.

---

## PR 2: Factory wire types in `proto`

**Scope:** `proto` gains `CapFactory`, the station and gate constants, the five Ticket fields, `FactoryConfig`, the decoders of the two config verbs, the `factory.config` reply and three error codes, with goldens under `testdata/factory/`. `CONTEXT.md` gains the Factory terms. The cap is not in `ServerCaps` yet, and the two verbs have no row in `peer.Needs`, so pocketd refuses them for every peer with `scope_denied`. Nothing sets a Ticket field.
**Depends on:** nothing
**Done when:** from `packages/pocketd`, `go vet ./...` and `go test ./...` pass, and `git status --short internal/proto/testdata/golden` prints nothing.

### Task 2.1: Ticket fields, the constants, and the Factory golden directory

**Files:**
- Create: `packages/pocketd/internal/proto/factory.go`
- Create: `packages/pocketd/internal/proto/factory_test.go`
- Modify: `packages/pocketd/internal/proto/ticket.go:54-81` (`Ticket`)
- Modify: `packages/pocketd/internal/proto/ticket_test.go:75-80`
- Modify: `packages/pocketd/internal/proto/golden_test.go:168-190` (`TestServerGolden`)
- Modify: `packages/pocketd/internal/board/store_test.go` (one new test)
- Create (generated): `packages/pocketd/internal/proto/testdata/factory/server/board.json`

**Context:** The TypeScript suite (`packages/protocol/test/golden.test.mjs`) and the Swift suite (`GoldenTests.swift`) read every file under `testdata/golden/` and mirror no Factory wire. So Factory goldens live under `testdata/factory/`, which only Go and Rust read. Each new Ticket field is `omitempty` and comes last in the struct, so every existing golden stays the same byte for byte. `decodeTicket` (`ticket.go:122`) refuses any key outside its allowlist, and `Store.Save` (`board/store.go:152-189`) copies only the fields that the ticket page edits. Neither changes. The two pin tests below pass before Step 3. They exist so that a later edit of the allowlist or of `Save` fails a test.

**Step 1: Write the failing tests**

Create `internal/proto/factory_test.go`:

```go
package proto

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func factoryGolden(t *testing.T, side, name string) []byte {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join("testdata", "factory", side, name))
	if err != nil {
		t.Fatal(err)
	}
	return bytes.TrimSpace(raw)
}

var factoryServerGolden = map[string]any{
	"board": NewBoard([]Ticket{
		{ID: "t1", Title: "Fold hunks", Prompt: "Fold unchanged hunks", Provider: "claude", Folder: "/app", Base: "main", State: TicketLive, Order: 3, Status: TicketNeedsYou,
			AgentID: "a1", TerminalID: "tm1", Worktree: "/wt/fold", CreatedAt: 1000, ChangedAt: 2000, Number: 7,
			Station: StationChecks, Gate: GateAsk, PR: 12, Fix: 1, Hint: true},
		{ID: "t2", Title: "Reveal in Finder", Prompt: "Add it", Provider: "codex", Folder: "/app", State: TicketTodo, Order: 1, CreatedAt: 1000, ChangedAt: 1000},
	}, 4),
}

func TestAFactoryTicketCarriesItsStationOnTheBoard(t *testing.T) {
	var snap Board
	dec := json.NewDecoder(bytes.NewReader(factoryGolden(t, "server", "board.json")))
	dec.DisallowUnknownFields()
	if err := dec.Decode(&snap); err != nil {
		t.Fatal(err)
	}
	if f := snap.Tickets[0]; f.Station != StationChecks || f.Gate != GateAsk || f.PR != 12 || f.Fix != 1 || !f.Hint {
		t.Fatalf("got %+v", f)
	}
	if plain := snap.Tickets[1]; plain.Station != "" || plain.Gate != "" || plain.PR != 0 || plain.Fix != 0 || plain.Hint {
		t.Fatalf("got %+v", plain)
	}
}

func TestATicketWithoutAStationSendsNoFactoryField(t *testing.T) {
	raw, _ := json.Marshal(Ticket{ID: "t", Title: "T", Prompt: "P", Provider: "claude", Folder: "/app", State: TicketTodo})
	for _, key := range []string{`"station"`, `"gate"`, `"pr"`, `"fix"`, `"hint"`} {
		if strings.Contains(string(raw), key) {
			t.Errorf("%s in %s", key, raw)
		}
	}
}

func TestTheStationsAndGatesHaveTheirWireNames(t *testing.T) {
	stations := []string{StationSpec, StationPlan, StationImplement, StationChecks, StationPR, StationMerged}
	gates := []string{GatePlan, GatePR, GateAsk}
	if got := strings.Join(stations, " ") + " | " + strings.Join(gates, " "); got != "spec plan implement checks pr merged | plan pr ask" {
		t.Fatalf("got %s", got)
	}
	if MaxFix != 2 || CapFactory != "factory.v1" {
		t.Fatalf("MaxFix %d, CapFactory %s", MaxFix, CapFactory)
	}
}
```

In `internal/proto/ticket_test.go`, add five rows to the list in `TestDecodeTicketRefusesFieldsAClientMayNotSet`, after the `agentId` row (:77):

```go
		`{"title":"T","prompt":"P","provider":"claude","folder":"/app","station":"plan"}`,
		`{"title":"T","prompt":"P","provider":"claude","folder":"/app","gate":"plan"}`,
		`{"title":"T","prompt":"P","provider":"claude","folder":"/app","pr":7}`,
		`{"title":"T","prompt":"P","provider":"claude","folder":"/app","fix":1}`,
		`{"title":"T","prompt":"P","provider":"claude","folder":"/app","hint":true}`,
```

In `internal/board/store_test.go`, add after `TestAStartedTicketStillTakesAPriorityAndLabels`:

```go
func TestSavingATicketTakesNoFactoryFieldFromIt(t *testing.T) {
	s := Open(t.TempDir())
	d := draft("a")
	d.Station, d.Gate, d.PR, d.Fix, d.Hint = proto.StationPlan, proto.GatePlan, 7, 2, true
	x, err := s.Save(d, false, at)
	if err != nil {
		t.Fatal(err)
	}
	if g := got(t, s, x.ID); g.Station != "" || g.Gate != "" || g.PR != 0 || g.Fix != 0 || g.Hint {
		t.Fatalf("a new ticket took them: %+v", g)
	}
	s.Update(x.ID, func(t *proto.Ticket) { t.Station, t.Fix = proto.StationSpec, 1 })
	d.ID, d.Title, d.Station, d.Fix = x.ID, "b", proto.StationMerged, 0
	if _, err := s.Save(d, false, at); err != nil {
		t.Fatal(err)
	}
	if g := got(t, s, x.ID); g.Title != "b" || g.Station != proto.StationSpec || g.Fix != 1 {
		t.Fatalf("an edit changed them: %+v", g)
	}
}
```

**Step 2: Run the tests and see them fail**

```sh
go test ./internal/proto -run 'Factory|TestTheStationsAndGates|TestATicketWithoutAStation|TestDecodeTicketRefuses'
```

Expected: the package does not compile. The errors name `unknown field Station in struct literal of type Ticket` and `undefined: StationChecks`.

**Step 3: Implement**

Create `internal/proto/factory.go`:

```go
package proto

// CapFactory: factory.* verbs, and a Ticket's station and gate.
const CapFactory = "factory.v1"

// A Factory ticket's Station.
const (
	StationSpec      = "spec"
	StationPlan      = "plan"
	StationImplement = "implement"
	StationChecks    = "checks"
	StationPR        = "pr"
	StationMerged    = "merged"
)

// What a Factory ticket's Gate says it waits on the user for.
const (
	GatePlan = "plan"
	GatePR   = "pr"
	GateAsk  = "ask"
)

// MaxFix is the most auto-fix attempts a project can allow a ticket.
const MaxFix = 2
```

`internal/proto/ticket.go:78-81`. Before:

```go
	// Priority is 1 (urgent) to 4 (low); 0 is none. It never moves a ticket in line.
	Priority int      `json:"priority,omitempty"`
	Labels   []string `json:"labels,omitempty"`
}
```

After:

```go
	// Priority is 1 (urgent) to 4 (low); 0 is none. It never moves a ticket in line.
	Priority int      `json:"priority,omitempty"`
	Labels   []string `json:"labels,omitempty"`
	// The Factory's fields. Only pocketd sets them: no client message carries one.
	// Station is where the Factory has the ticket; empty for a ticket it doesn't run.
	Station string `json:"station,omitempty"`
	// Gate is what waits on the user: a plan, a PR or an ask.
	Gate string `json:"gate,omitempty"`
	// PR is its pull request's number once one is open.
	PR int `json:"pr,omitempty"`
	// Fix is how many auto-fix attempts it has used.
	Fix int `json:"fix,omitempty"`
	// Hint: the user's hint granted one more attempt.
	Hint bool `json:"hint,omitempty"`
}
```

`internal/proto/golden_test.go:168-190`. Before:

```go
// TestServerGolden pins the exact JSON the phone decodes.
func TestServerGolden(t *testing.T) {
	for name, msg := range all(serverGolden, phoneServerGolden) {
		got, err := json.MarshalIndent(msg, "", "  ")
		if err != nil {
			t.Fatalf("%s: %v", name, err)
		}
		path := filepath.Join("testdata", "golden", "server", name+".json")
```

After:

```go
// TestServerGolden pins the exact JSON the phone decodes and, under
// testdata/factory, the Factory's, which only the desktop decodes.
func TestServerGolden(t *testing.T) {
	serverGoldens(t, "golden", all(serverGolden, phoneServerGolden))
	serverGoldens(t, "factory", factoryServerGolden)
}

func serverGoldens(t *testing.T, dir string, goldens map[string]any) {
	t.Helper()
	for name, msg := range goldens {
		got, err := json.MarshalIndent(msg, "", "  ")
		if err != nil {
			t.Fatalf("%s: %v", name, err)
		}
		path := filepath.Join("testdata", dir, "server", name+".json")
```

The body of the loop below `path` does not change.

**Step 4: Generate the golden, then run the tests and see them pass**

```sh
mkdir -p internal/proto/testdata/factory/server internal/proto/testdata/factory/client
go test ./internal/proto -run TestServerGolden -update
git status --short internal/proto/testdata
```

Expected: `git status` lists only `?? internal/proto/testdata/factory/`. Nothing under `testdata/golden/` changed. The first Ticket in `testdata/factory/server/board.json` ends with `"station": "checks"`, `"gate": "ask"`, `"pr": 12`, `"fix": 1`, `"hint": true`. The second has none of the five keys.

```sh
go test ./internal/proto ./internal/board/
```

Expected: `ok` for both packages.

### Task 2.2: `FactoryConfig`

**Files:**
- Modify: `packages/pocketd/internal/proto/factory.go`
- Modify: `packages/pocketd/internal/proto/factory_test.go`

**Context:** The index fixes the struct, `DefaultFactoryConfig` (off, spec and reviewer on, `claude-opus`, `claude-sonnet`, `codex`, 0 slots, 2 fixes, no commands) and `Valid` (ranges, at most 16 commands of at most 400 runes, no blank command). `Slots` 0 means the Board limit. `Valid` does not check that a profile exists: plan 4 adds the profile store and its `unknown_profile` code. `DefaultFactoryConfig` returns an empty `Checks` slice, not nil, so the default reads the same before and after a round trip through JSON.

**Step 1: Write the failing tests**

Add `"reflect"` and `"slices"` to the imports of `factory_test.go`, and add:

```go
func TestTheFactoryStartsOffWithItsSpecAndReviewerOn(t *testing.T) {
	want := FactoryConfig{Spec: true, Reviewer: true, Lead: "claude-opus", Workers: "claude-sonnet", Review: "codex", Fix: MaxFix, Checks: []string{}}
	if got := DefaultFactoryConfig(); !reflect.DeepEqual(got, want) || !got.Valid() {
		t.Fatalf("got %+v", got)
	}
}

func TestAFactoryConfigKeepsItsLimitsAndCommandsInRange(t *testing.T) {
	for name, change := range map[string]func(*FactoryConfig){
		"negative slots":         func(c *FactoryConfig) { c.Slots = -1 },
		"slots over the Board's": func(c *FactoryConfig) { c.Slots = MaxBoardLimit + 1 },
		"negative fix":           func(c *FactoryConfig) { c.Fix = -1 },
		"fix over the cap":       func(c *FactoryConfig) { c.Fix = MaxFix + 1 },
		"blank command":          func(c *FactoryConfig) { c.Checks = []string{"go test ./...", "  "} },
		"long command":           func(c *FactoryConfig) { c.Checks = []string{strings.Repeat("é", MaxFactoryCheck+1)} },
		"too many commands":      func(c *FactoryConfig) { c.Checks = slices.Repeat([]string{"true"}, MaxFactoryChecks+1) },
	} {
		bad := DefaultFactoryConfig()
		change(&bad)
		if bad.Valid() {
			t.Errorf("%s: accepted", name)
		}
	}
	most := DefaultFactoryConfig()
	most.Slots, most.Fix, most.Checks = MaxBoardLimit, 0, slices.Repeat([]string{strings.Repeat("é", MaxFactoryCheck)}, MaxFactoryChecks)
	if !most.Valid() {
		t.Fatal("refused a config at its limits")
	}
}
```

**Step 2: Run the tests and see them fail**

```sh
go test ./internal/proto -run 'TestTheFactoryStartsOff|TestAFactoryConfigKeeps'
```

Expected: the package does not compile, with `undefined: FactoryConfig` and `undefined: DefaultFactoryConfig`.

**Step 3: Implement**

In `internal/proto/factory.go`, add the imports after the `package` line:

```go
import (
	"strings"
	"unicode/utf8"
)
```

Replace the `MaxFix` constant with this block, and add the type below it:

```go
const (
	// MaxFix is the most auto-fix attempts a project can allow a ticket.
	MaxFix = 2
	// MaxFactoryChecks is how many check commands a project can list, and
	// MaxFactoryCheck how long one can be, in runes.
	MaxFactoryChecks = 16
	MaxFactoryCheck  = 400
)

// FactoryConfig is one project's Factory settings.
type FactoryConfig struct {
	Enabled  bool `json:"enabled"`
	Spec     bool `json:"spec"`
	Reviewer bool `json:"reviewer"`
	// Lead, Workers and Review are profile ids. Workers is the default for each task.
	Lead    string `json:"lead"`
	Workers string `json:"workers"`
	Review  string `json:"review"`
	// Slots is the project's own slot limit, 1 to MaxBoardLimit; 0 is the Board's.
	Slots int `json:"slots"`
	// Fix is how many auto-fix attempts a ticket gets, 0 to MaxFix.
	Fix int `json:"fix"`
	// Checks are the local commands the Checks station runs, in order.
	Checks []string `json:"checks"`
}

func DefaultFactoryConfig() FactoryConfig {
	return FactoryConfig{Spec: true, Reviewer: true, Lead: "claude-opus", Workers: "claude-sonnet", Review: "codex", Fix: MaxFix, Checks: []string{}}
}

// Valid is what a client's factory.config.save must satisfy. Whether its
// profiles exist is the profile store's to say.
func (c FactoryConfig) Valid() bool {
	if c.Slots < 0 || c.Slots > MaxBoardLimit || c.Fix < 0 || c.Fix > MaxFix || len(c.Checks) > MaxFactoryChecks {
		return false
	}
	for _, command := range c.Checks {
		if strings.TrimSpace(command) == "" || utf8.RuneCountInString(command) > MaxFactoryCheck {
			return false
		}
	}
	return true
}
```

**Step 4: Run the tests and see them pass**

```sh
go test ./internal/proto
```

Expected: `ok  	pocketd/internal/proto`.

### Task 2.3: Decode `factory.config.get` and `factory.config.save`

**Files:**
- Modify: `packages/pocketd/internal/proto/messages.go:107-111` (`ClientMessage`), `:245-253` (`DecodeClient`)
- Modify: `packages/pocketd/internal/proto/factory.go`
- Modify: `packages/pocketd/internal/proto/factory_test.go`
- Modify: `packages/pocketd/internal/proto/golden_test.go:192-204` (`TestClientGolden`)
- Create: `packages/pocketd/internal/proto/testdata/factory/client/factory_config_get.json`
- Create: `packages/pocketd/internal/proto/testdata/factory/client/factory_config_save.json`

**Context:** `ClientMessage` is flat, and it already has `Project string` (`skills.list` and `gh.prs` use it). It gains `Config *FactoryConfig`. `strictInto` (`phone.go:124`) decodes an object whose keys match exactly, with no null and with every required key. All nine keys of `config` are required, so a client cannot turn a setting off by leaving its key out. The decoder does not call `Valid`: a config out of range must reach the handler, which answers `invalid_config`. `f.text` (`phone.go:60`) reads a string that is not empty.

**Step 1: Write the failing tests**

Create `testdata/factory/client/factory_config_get.json` (one line, then a newline):

```json
{"type":"factory.config.get","id":"factory-1","project":"/app"}
```

Create `testdata/factory/client/factory_config_save.json`:

```json
{"type":"factory.config.save","id":"factory-2","project":"/app","config":{"enabled":true,"spec":true,"reviewer":true,"lead":"claude-opus","workers":"claude-sonnet","review":"codex","slots":2,"fix":1,"checks":["cargo test","cargo clippy --all-targets"]}}
```

Add to `factory_test.go`:

```go
func TestTheFactoryConfigGoldensDecodeToTheirFields(t *testing.T) {
	m, err := DecodeClient(factoryGolden(t, "client", "factory_config_get.json"))
	if err != nil || m.Type != "factory.config.get" || m.ID != "factory-1" || m.Project != "/app" || m.Config != nil {
		t.Fatalf("%+v %v", m, err)
	}
	m, err = DecodeClient(factoryGolden(t, "client", "factory_config_save.json"))
	want := FactoryConfig{Enabled: true, Spec: true, Reviewer: true, Lead: "claude-opus", Workers: "claude-sonnet", Review: "codex", Slots: 2, Fix: 1, Checks: []string{"cargo test", "cargo clippy --all-targets"}}
	if err != nil || m.Project != "/app" || m.Config == nil || !reflect.DeepEqual(*m.Config, want) {
		t.Fatalf("%+v %v", m.Config, err)
	}
}

func TestAFactoryConfigOffTheContractIsMalformed(t *testing.T) {
	full := `"enabled":false,"spec":true,"reviewer":true,"lead":"claude-opus","workers":"claude-sonnet","review":"codex","slots":0,"fix":2,"checks":[]`
	save := func(config string) string {
		return `{"type":"factory.config.save","id":"1","project":"/app","config":` + config + `}`
	}
	for _, raw := range []string{
		`{"type":"factory.config.get","id":"1"}`,
		`{"type":"factory.config.get","id":"1","project":""}`,
		`{"type":"factory.config.save","id":"1","project":"/app"}`,
		`{"type":"factory.config.save","id":"1","config":{` + full + `}}`,
		save(`"on"`),
		save(`{` + strings.Replace(full, `,"checks":[]`, ``, 1) + `}`),
		save(`{` + full + `,"station":"plan"}`),
		save(`{` + strings.Replace(full, `"slots":0`, `"slots":1.5`, 1) + `}`),
		save(`{` + strings.Replace(full, `"checks":[]`, `"checks":null`, 1) + `}`),
		save(`{` + strings.Replace(full, `"enabled":false`, `"Enabled":false`, 1) + `}`),
	} {
		if _, err := DecodeClient([]byte(raw)); err != ErrMalformed {
			t.Errorf("%s: got %v, want ErrMalformed", raw, err)
		}
	}
}

func TestAConfigOutOfRangeStillDecodesSoItsRefusalCanSayWhy(t *testing.T) {
	m, err := DecodeClient([]byte(`{"type":"factory.config.save","id":"1","project":"/app","config":{"enabled":false,"spec":true,"reviewer":true,"lead":"claude-opus","workers":"claude-sonnet","review":"codex","slots":99,"fix":2,"checks":[]}}`))
	if err != nil || m.Config == nil || m.Config.Valid() {
		t.Fatalf("%+v %v", m.Config, err)
	}
}
```

In `golden_test.go`, change `TestClientGolden` (:192-204). Before:

```go
// TestClientGolden proves every message the phone may send decodes here.
func TestClientGolden(t *testing.T) {
	files, _ := filepath.Glob(filepath.Join("testdata", "golden", "client", "*.json"))
	if len(files) == 0 {
		t.Fatal("no client golden files")
	}
	for _, f := range files {
		raw, _ := os.ReadFile(f)
		if _, err := DecodeClient(raw); err != nil {
			t.Errorf("%s: %v", filepath.Base(f), err)
		}
	}
}
```

After:

```go
// TestClientGolden proves every message the phone may send decodes here, and
// under testdata/factory every Factory message the desktop may.
func TestClientGolden(t *testing.T) {
	for _, dir := range []string{"golden", "factory"} {
		files, _ := filepath.Glob(filepath.Join("testdata", dir, "client", "*.json"))
		if len(files) == 0 {
			t.Fatalf("no client golden files in testdata/%s", dir)
		}
		for _, f := range files {
			raw, _ := os.ReadFile(f)
			if _, err := DecodeClient(raw); err != nil {
				t.Errorf("%s: %v", filepath.Base(f), err)
			}
		}
	}
}
```

**Step 2: Run the tests and see them fail**

```sh
go test ./internal/proto -run 'TestTheFactoryConfigGoldens|TestAConfigOutOfRange|TestClientGolden'
```

Expected: the package does not compile, with `m.Config undefined (type ClientMessage has no field or method Config)`.

**Step 3: Implement**

`internal/proto/messages.go:107-111`. Before:

```go
	// team.start, team.stop and team.merge's.
	Lead  string
	Task  string
	Tasks []TaskPick
}
```

After:

```go
	// team.start, team.stop and team.merge's.
	Lead  string
	Task  string
	Tasks []TaskPick
	// factory.config.save's.
	Config *FactoryConfig
}
```

`messages.go:251-253`. Before:

```go
	case m.Type == "team.merge":
		ok = get("lead", &m.Lead)
	default:
```

After:

```go
	case m.Type == "team.merge":
		ok = get("lead", &m.Lead)
	case m.Type == "factory.config.get":
		ok = f.text("project", &m.Project)
	case m.Type == "factory.config.save":
		ok = f.text("project", &m.Project) && decodeFactoryConfig(f["config"], &m.Config)
	default:
```

In `internal/proto/factory.go`, add `"encoding/json"` to the imports and add at the end of the file:

```go
// decodeFactoryConfig takes a config only whole: a key left out would switch
// its setting off without the user having chosen that.
func decodeFactoryConfig(raw json.RawMessage, dst **FactoryConfig) bool {
	var c FactoryConfig
	if !strictInto(raw, &c, []string{"enabled", "spec", "reviewer", "lead", "workers", "review", "slots", "fix", "checks"}) {
		return false
	}
	*dst = &c
	return true
}
```

**Step 4: Run the tests and see them pass**

```sh
go test ./internal/proto
```

Expected: `ok  	pocketd/internal/proto`.

### Task 2.4: The `factory.config` reply and the error codes

**Files:**
- Modify: `packages/pocketd/internal/proto/factory.go`
- Modify: `packages/pocketd/internal/proto/factory_test.go`
- Create (generated): `packages/pocketd/internal/proto/testdata/factory/server/factory_config.json`, `factory_config_problem.json`, `error_factory_unavailable.json`

**Context:** The index fixes the reply frame: `factory.config` with `id`, `project`, `config` and an optional `problem`. `problem` is the sentence that says why the Factory cannot be turned on for the project. A refused save is an ordinary `error` frame (`NewErrorCode`, `messages.go:505`) with code `factory_unavailable`. `orEmpty` (`gh.go:167`) turns a nil list into an empty one, so `checks` is always a list on the wire and the desktop never reads `null`. PR 4's Rust tests read the three goldens.

**Step 1: Write the failing tests**

In `factory_test.go`, add three entries to `factoryServerGolden`, after `"board"`:

```go
	"factory_config": NewFactoryConfig("factory-1", "/app", FactoryConfig{Enabled: true, Spec: true, Reviewer: true, Lead: "claude-opus", Workers: "claude-sonnet", Review: "codex",
		Slots: 2, Fix: 1, Checks: []string{"cargo test", "cargo clippy --all-targets"}}, ""),
	"factory_config_problem":    NewFactoryConfig("factory-2", "/app", DefaultFactoryConfig(), "Install GitHub CLI (gh) to use the Factory"),
	"error_factory_unavailable": NewErrorCode("factory-3", CodeFactoryUnavailable, "This project has no GitHub remote"),
```

Add this test:

```go
func TestAFactoryConfigReplyAlwaysListsItsCommands(t *testing.T) {
	got, _ := json.Marshal(NewFactoryConfig("g", "/app", FactoryConfig{}, ""))
	want := `{"type":"factory.config","id":"g","project":"/app","config":{"enabled":false,"spec":false,"reviewer":false,"lead":"","workers":"","review":"","slots":0,"fix":0,"checks":[]}}`
	if string(got) != want {
		t.Fatalf("got  %s\nwant %s", got, want)
	}
	if CodeFactoryOff != "factory_off" || CodeFactoryUnavailable != "factory_unavailable" || CodeInvalidConfig != "invalid_config" {
		t.Fatal("a Factory error code changed its wire name")
	}
}
```

**Step 2: Run the tests and see them fail**

```sh
go test ./internal/proto -run 'TestAFactoryConfigReply|TestServerGolden'
```

Expected: the package does not compile, with `undefined: NewFactoryConfig` and `undefined: CodeFactoryUnavailable`.

**Step 3: Implement**

In `internal/proto/factory.go`, add below `CapFactory`:

```go
const (
	// CodeFactoryOff: this pocketd serves no Factory to the connection.
	CodeFactoryOff = "factory_off"
	// CodeFactoryUnavailable: the Factory can't be turned on for the project; the message says why.
	CodeFactoryUnavailable = "factory_unavailable"
	CodeInvalidConfig      = "invalid_config"
)
```

Add at the end of the file:

```go
// FactoryConfigReply answers factory.config.get and factory.config.save.
type FactoryConfigReply struct {
	Type    string        `json:"type"`
	ID      string        `json:"id"`
	Project string        `json:"project"`
	Config  FactoryConfig `json:"config"`
	// Problem says why the Factory can't be turned on for the project.
	Problem string `json:"problem,omitempty"`
}

func NewFactoryConfig(id, project string, config FactoryConfig, problem string) FactoryConfigReply {
	config.Checks = orEmpty(config.Checks)
	return FactoryConfigReply{"factory.config", id, project, config, problem}
}
```

**Step 4: Generate the goldens, then run the tests and see them pass**

```sh
go test ./internal/proto -run TestServerGolden -update
git status --short internal/proto/testdata/golden
go test ./internal/proto
```

Expected: `git status` prints nothing. `go test` prints `ok  	pocketd/internal/proto`. `testdata/factory/server/` now holds `board.json`, `factory_config.json`, `factory_config_problem.json` and `error_factory_unavailable.json`. `factory_config_problem.json` has `"checks": []` and `"problem": "Install GitHub CLI (gh) to use the Factory"`.

### Task 2.5: `CONTEXT.md` gains the Factory terms

**Files:**
- Modify: `CONTEXT.md` (a new section after `### Teams`, at the end of the file)

**Context:** `CONTEXT.md` is the vocabulary for code, UI text and plans. Each entry is a `**Word**:` line, one paragraph, and an `_Avoid_:` line. **Run** stays an Automation's firing, so no entry may call the Factory's progress a run. **Mailbox** keeps `inbox` in its `_Avoid_` line. Task 1.1 already edited **Queued**. This task has no test: it changes prose only.

**Step 1: Add the section**

Append after the **Mailbox** entry, with one blank line before `### Factory`:

```markdown
### Factory

**Factory**:
A project's pipeline for its Tickets, and the Board layout that shows it. With the Factory on for a project, pocketd takes each started Ticket of that project through the Stations and stops at each Gate. The project can set a slot limit of its own, which only lowers the Board's. The Factory is off for every project until the user turns it on.
_Avoid_: run (a Run is one firing of an automation), line, workflow, conveyor

**Station**:
One step of the Factory: To do, Spec, Plan, Implement, Checks, PR review or Merged. A Ticket is in exactly one.
_Avoid_: stage, phase, column (a column is how the Board draws a Station)

**Gate**:
A Station where only the user moves the Ticket on: Plan, where the user approves the plan, and PR review, where the user merges. No agent passes a Gate.
_Avoid_: approval step, checkpoint

**Ask**:
A permission request from an agent, such as a command outside its allowlist. The Ticket waits on the user until the Ask is answered. An Ask is not a Gate: it can come at any Station.
_Avoid_: prompt, approval, question (a question is one kind of Ask)

**Auto-fix**:
One attempt by a Ticket's lead to repair failed checks, started by pocketd. A project allows 0 to 2 for each Ticket. After the last one the Ticket is Failed until the user sends a hint.
_Avoid_: retry, self-heal

**Reviewer**:
A second session in a Ticket's Worktree that reads the changes and reports findings before the PR review Gate. It cannot edit, write or run shell commands, and it cannot pass a Gate.
_Avoid_: approver, critic

**Profile**:
A named setup for one agent session: a provider and model, a soul (who the agent is), instructions (how it works) and skills. Every project shares the Profiles. A project picks one for its lead, one for its workers and one for its Reviewer.
_Avoid_: persona, preset, role
```

**Step 2: Check the file**

```sh
grep -c '^\*\*' CONTEXT.md
```

Expected: the count is 7 higher than before this task.

### PR 2 final checks

From `packages/pocketd`: `go vet ./...` (no output), `go test ./...` (every package `ok`), `gofmt -l .` (no output).
`git status --short internal/proto/testdata/golden` prints nothing, so the TypeScript and Swift golden suites see no new file.

---

## PR 3: The config store, the two verbs, and the scope rows

**Scope:** `internal/factory` is created with `Configs`. pocketd serves `factory.v1`, `factory.config.get` and `factory.config.save`, and refuses to turn the Factory on without a usable `gh` and a GitHub remote. `peer.Needs` gains the rows of all ten Factory verbs, with the test that no terminal peer reaches a gate. `serve.go` opens the store and gives its `Limit` to the Board's scheduler. The red-team probe gains three verbs. No client sends `factory.v1` until PR 4, and no Ticket gets a station until plan 3, so a saved config changes only the project slot limit.
**Depends on:** PR 1 (`Scheduler.Limit`), PR 2 (the wire types)
**Done when:** from `packages/pocketd`, `go vet ./...` and `go test ./...` pass, and `scripts/red-team.sh` passes.

### Task 3.1: `factory.Configs`

**Files:**
- Create: `packages/pocketd/internal/factory/config.go`
- Create: `packages/pocketd/internal/factory/config_test.go`

**Context:** The index fixes the API: `OpenConfigs(home)`, `Get(project)`, `Save(project, cfg)`, `Limit(project)`. The file is `home/state/factory.json`. The store copies `board.Store` (`board/store.go:55-143`): read once at open, rename a file it cannot use to `.bad`, and on each save write the whole file with `atomicfile.Write` before the memory changes. `Limit` is what `Store.Claim` calls for each queued Ticket's folder: `Slots` while the Factory is on for the project, else 0. The package imports only `proto` and `atomicfile`, so its tests need no libghostty. Plan 3 adds the engine to this package. Stories: US-33, criteria 1 and 2.

**Step 1: Write the failing tests**

Create `internal/factory/config_test.go`:

```go
package factory

import (
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"pocketd/internal/proto"
)

func on(slots int) proto.FactoryConfig {
	c := proto.DefaultFactoryConfig()
	c.Enabled, c.Slots = true, slots
	return c
}

func TestAProjectWithNothingSavedHasTheDefaults(t *testing.T) {
	c := OpenConfigs(t.TempDir())
	if got := c.Get("/app"); got.Enabled || !reflect.DeepEqual(got, proto.DefaultFactoryConfig()) {
		t.Fatalf("got %+v", got)
	}
}

func TestSavedSettingsSurviveARestartAndStayWithTheirProject(t *testing.T) {
	home := t.TempDir()
	want := on(2)
	want.Checks = []string{"go test ./..."}
	if err := OpenConfigs(home).Save("/app", want); err != nil {
		t.Fatal(err)
	}
	again := OpenConfigs(home)
	if got := again.Get("/app"); !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
	if got := again.Get("/other"); !reflect.DeepEqual(got, proto.DefaultFactoryConfig()) {
		t.Fatalf("another project took the settings: %+v", got)
	}
}

func TestSettingsOutOfRangeAreRefusedAndNothingIsSaved(t *testing.T) {
	home := t.TempDir()
	c := OpenConfigs(home)
	if err := c.Save("/app", on(proto.MaxBoardLimit+1)); !errors.Is(err, ErrInvalid) {
		t.Fatalf("got %v", err)
	}
	if err := c.Save("", on(1)); !errors.Is(err, ErrInvalid) {
		t.Fatalf("a config with no project: %v", err)
	}
	if c.Get("/app").Enabled {
		t.Fatal("the refused settings were kept")
	}
	if _, err := os.Stat(filepath.Join(home, "state", "factory.json")); !errors.Is(err, fs.ErrNotExist) {
		t.Fatalf("a file was written: %v", err)
	}
}

func TestAProjectLimitCountsOnlyWhileItsFactoryIsOn(t *testing.T) {
	c := OpenConfigs(t.TempDir())
	if n := c.Limit("/app"); n != 0 {
		t.Fatalf("a project with nothing saved had a limit of %d", n)
	}
	off := on(2)
	off.Enabled = false
	c.Save("/app", off)
	if n := c.Limit("/app"); n != 0 {
		t.Fatalf("a project with the Factory off had a limit of %d", n)
	}
	c.Save("/app", on(2))
	if n := c.Limit("/app"); n != 2 {
		t.Fatalf("got %d, want 2", n)
	}
	c.Save("/app", on(0))
	if n := c.Limit("/app"); n != 0 {
		t.Fatalf("got %d, want the Board's limit (0)", n)
	}
}

func TestAnUnreadableFileIsKeptAsideAndEveryProjectStartsOff(t *testing.T) {
	home := t.TempDir()
	path := filepath.Join(home, "state", "factory.json")
	os.MkdirAll(filepath.Dir(path), 0o700)
	os.WriteFile(path, []byte("{not json"), 0o600)
	c := OpenConfigs(home)
	if c.Get("/app").Enabled {
		t.Fatal("a project was on")
	}
	if _, err := os.Stat(path + ".bad"); err != nil {
		t.Fatalf("the file wasn't kept: %v", err)
	}
	if err := c.Save("/app", on(1)); err != nil {
		t.Fatal(err)
	}
}
```

**Step 2: Run the tests and see them fail**

```sh
go test ./internal/factory/
```

Expected: the package does not compile, with `undefined: OpenConfigs` and `undefined: ErrInvalid`.

**Step 3: Implement**

Create `internal/factory/config.go`:

```go
// Package factory takes a project's Tickets through the Factory's stations.
package factory

import (
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"log"
	"maps"
	"os"
	"path/filepath"
	"sync"

	"pocketd/internal/atomicfile"
	"pocketd/internal/proto"
)

const configVersion = 1

var ErrInvalid = errors.New("These Factory settings aren't valid")

type configFile struct {
	Version  int                            `json:"version"`
	Projects map[string]proto.FactoryConfig `json:"projects"`
}

// Configs is each project's Factory settings by its folder, saved whole to
// Home/state/factory.json on every change.
type Configs struct {
	path string

	mu sync.Mutex
	by map[string]proto.FactoryConfig
}

// OpenConfigs reads home/state/factory.json. A file it can't use is renamed
// to .bad, so the next save doesn't erase it, and every project starts off.
func OpenConfigs(home string) *Configs {
	c := &Configs{path: filepath.Join(home, "state", "factory.json"), by: map[string]proto.FactoryConfig{}}
	raw, err := os.ReadFile(c.path)
	if errors.Is(err, fs.ErrNotExist) {
		return c
	}
	var f configFile
	if err == nil {
		err = json.Unmarshal(raw, &f)
	}
	if err == nil && f.Version != configVersion {
		err = fmt.Errorf("version %d, want %d", f.Version, configVersion)
	}
	if err != nil {
		log.Printf("factory %s: %v; every project starts off", c.path, err)
		if rerr := os.Rename(c.path, c.path+".bad"); rerr != nil {
			log.Print(rerr)
		}
		return c
	}
	if f.Projects != nil {
		c.by = f.Projects
	}
	return c
}

// Get is project's settings, the defaults when none were saved.
func (c *Configs) Get(project string) proto.FactoryConfig {
	c.mu.Lock()
	defer c.mu.Unlock()
	if cfg, ok := c.by[project]; ok {
		return cfg
	}
	return proto.DefaultFactoryConfig()
}

// Save keeps cfg for project; the store changes only if the write worked.
func (c *Configs) Save(project string, cfg proto.FactoryConfig) error {
	if project == "" || !cfg.Valid() {
		return ErrInvalid
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	next := maps.Clone(c.by)
	next[project] = cfg
	raw, err := json.MarshalIndent(configFile{Version: configVersion, Projects: next}, "", "  ")
	if err == nil {
		err = os.MkdirAll(filepath.Dir(c.path), 0o700)
	}
	if err == nil {
		err = atomicfile.Write(c.path, raw, 0o600)
	}
	if err != nil {
		return err
	}
	c.by = next
	return nil
}

// Limit is how many of project's tickets may hold a slot at once; 0 is no
// limit of its own, as while its Factory is off.
func (c *Configs) Limit(project string) int {
	if cfg := c.Get(project); cfg.Enabled {
		return cfg.Slots
	}
	return 0
}
```

**Step 4: Run the tests and see them pass**

```sh
go test ./internal/factory/
```

Expected: `ok  	pocketd/internal/factory`.

### Task 3.2: `peer.Needs` rows and the gate test

**Files:**
- Modify: `packages/pocketd/internal/peer/check.go:15-134` (`Needs`)
- Modify: `packages/pocketd/internal/peer/check_test.go:17-104`, plus one new test

**Context:** `Needs` is the allowlist of verbs. A verb with no row is refused for everyone, and `conn.dispatch` (`wsserver.go:549`) checks the row before anything else. The index's verb table fixes the scope of each of the ten Factory verbs. All ten rows go in now, so that no later plan can add a verb with a weaker scope without changing a test here. Only `factory.config.get` and `factory.config.save` decode after this plan. The other eight answer `Malformed message` before the scope check until their plan adds the decoder. A terminal peer holds only `Observe` (`peer/scope.go:33`). A reviewer is a session in a terminal or a chat that pocketd runs, so it has no stronger principal than a terminal peer. `gofmt` aligns the values of a map literal within a block of lines. A blank line before the new rows starts a new block, so the rows above keep their alignment. Stories: US-35, criteria 1 to 4 and 6. PRD §4.4.

**Step 1: Write the failing tests**

In `internal/peer/check_test.go`, add these rows at the end of the map in `TestEveryVerbAndPrincipalMatchesTheMatrix`, after the `"ops:upgrade"` row (:96) and one blank line. The columns are owner, phone, legacy phone, terminal peer:

```go

		"ws:factory.config.get":     {"", "", "", ""},
		"ws:factory.config.save":    {"", "scope_denied", "scope_denied", "scope_denied"},
		"ws:factory.ticket":         {"", "scope_denied", "scope_denied", "scope_denied"},
		"ws:factory.advance":        {"", "", "", "scope_denied"},
		"ws:factory.hint":           {"", "", "", "scope_denied"},
		"ws:factory.profile.list":   {"", "", "", ""},
		"ws:factory.profile.get":    {"", "scope_denied", "scope_denied", "scope_denied"},
		"ws:factory.profile.save":   {"", "scope_denied", "scope_denied", "scope_denied"},
		"ws:factory.profile.delete": {"", "scope_denied", "scope_denied", "scope_denied"},
		"ws:factory.metrics":        {"", "scope_denied", "scope_denied", "scope_denied"},
```

Add this test after `TestEveryVerbAndPrincipalMatchesTheMatrix`:

```go
func TestNoAgentInATerminalCanAdvanceAGate(t *testing.T) {
	pty := Principal{Kind: PTY, Pid: 2, Terminal: "t1", Scopes: PTYScopes}
	for _, verb := range []string{"team.start", "permission.resolve", "gh.pr.merge", "factory.hint", "factory.advance", "factory.config.save", "factory.profile.save", "factory.profile.delete"} {
		if _, listed := Needs["ws:"+verb]; !listed {
			t.Errorf("%s has no row in Needs", verb)
		}
		if got := code(pty.Check("ws:"+verb, "", "")); got != "scope_denied" {
			t.Errorf("%s: %q, want scope_denied", verb, got)
		}
	}
	for _, verb := range []string{"factory.config.get", "factory.profile.list"} {
		if got := code(pty.Check("ws:"+verb, "", "")); got != "" {
			t.Errorf("%s: %q, want a terminal agent to read it", verb, got)
		}
	}
}
```

**Step 2: Run the tests and see them fail**

```sh
go test ./internal/peer/ -run 'TestEveryVerbAndPrincipalMatchesTheMatrix|TestNoAgentInATerminalCanAdvanceAGate'
```

Expected: both fail. The matrix test reports `ws:factory.config.get by principal 0: "scope_denied", want ""` and the like for each row the owner may use. The gate test reports `factory.hint has no row in Needs` for five verbs, and `factory.config.get: "scope_denied", want a terminal agent to read it`.

**Step 3: Implement**

In `internal/peer/check.go`, add these rows at the end of `Needs`, after `"ops:accounts.autoswitch": Own,` (:133) and one blank line:

```go

	"ws:factory.config.get":     Observe,
	"ws:factory.config.save":    Own,
	"ws:factory.ticket":         Own,
	"ws:factory.advance":        Drive,
	"ws:factory.hint":           Drive,
	"ws:factory.profile.list":   Observe,
	"ws:factory.profile.get":    Own,
	"ws:factory.profile.save":   Own,
	"ws:factory.profile.delete": Own,
	"ws:factory.metrics":        Own,
```

**Step 4: Run the tests and see them pass**

```sh
go test ./internal/peer/
gofmt -l internal/peer
```

Expected: `ok  	pocketd/internal/peer`, and `gofmt` prints nothing. If `gofmt` lists a file, run `gofmt -w` on it.

### Task 3.3: Serve `factory.v1` and `factory.config.get`

**Files:**
- Modify: `packages/pocketd/internal/proto/version.go:39` (`ServerCaps`)
- Modify: `packages/pocketd/internal/proto/factory_test.go` (one new test)
- Modify: `packages/pocketd/internal/wsserver/wsserver.go:107-110` (`Server`), `:141-145` (`conn`), `:262-265` (hello), `:616-617` (`dispatch`)
- Create: `packages/pocketd/internal/wsserver/factory.go`
- Create: `packages/pocketd/internal/wsserver/factory_test.go`

**Context:** pocketd offers a cap only when it can serve it. For `team.v1` the hello handler drops the cap when `Server.Teams` is nil (`wsserver.go:263`). `factory.v1` follows the same rule with `Server.FactoryConfigs`. `conn.dispatch` checks the scope first (`wsserver.go:549`), so a peer without the scope gets `scope_denied` whatever its caps. `conn.usableFolder` (`automation.go:85`) is the check `board.save` uses for a Ticket's folder: the folder must be a project on this Mac and must exist. It calls `Server.Projects`, which the test setup leaves nil, so the fixture below sets it. `github.Service.Status(ctx, tree)` runs `gh --version` and `gh auth status` and returns `State` `missing`, `signedOut` or `ready`. `github.Service.PR(ctx, tree)` runs `gh pr view` and returns the problem `github.NoRemote` when the repository has no GitHub remote (`github/pr.go:79-95`). Each takes seconds, and the conn must keep reading meanwhile, as `gh.go:96` says. `ghFixture` (`gh_test.go:22`) gives a project folder, a registry and a fake `gh`: `f.withGh.Store(false)` removes `gh` from the path, a file `auth.fail` signs it out, and a file `view.err` makes `gh pr view` fail with that text. Stories: US-33, criterion 3. PRD C9 and E12.

**Step 1: Write the failing tests**

Add `"slices"` to the imports of `internal/proto/factory_test.go` if Task 2.2 did not, and add:

```go
func TestTheServerOffersTheFactoryCap(t *testing.T) {
	_, caps, code := Negotiate(Range{Min: 3, Max: 3}, []string{CapFactory}, ServerCaps)
	if code != "" || !slices.Equal(caps, []string{CapFactory}) {
		t.Fatalf("caps %v, code %q", caps, code)
	}
}
```

Create `internal/wsserver/factory_test.go`:

```go
//go:build unix

package wsserver

import (
	"fmt"
	"testing"

	"pocketd/internal/factory"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
)

// factoryFixture is ghFixture's project with a Factory config store that
// every peer of the fixture shares.
type factoryFixture struct {
	*ghFixture
	configs *factory.Configs
}

func newFactoryFixture(t *testing.T) *factoryFixture {
	return &factoryFixture{newGhFixture(t), factory.OpenConfigs(t.TempDir())}
}

func (f *factoryFixture) peer(who peer.Principal, caps ...string) *phone {
	_, _, p := local(f.t, who, f.server, func(s *Server) {
		s.Projects = func() []proto.Project { return []proto.Project{{Path: f.project}} }
		s.FactoryConfigs = f.configs
	})
	helloWith(p, caps...)
	return p
}

func (f *factoryFixture) get() string {
	return fmt.Sprintf(`{"type":"factory.config.get","id":"g","project":%q}`, f.project)
}

// outOfReach is each way GitHub can be out of reach, and what pocketd says about it.
var outOfReach = map[string]struct {
	cut     func(f *factoryFixture)
	problem string
}{
	"no gh":      {func(f *factoryFixture) { f.withGh.Store(false) }, "Install GitHub CLI (gh) to use the Factory"},
	"signed out": {func(f *factoryFixture) { f.put("auth.fail", "") }, "Run gh auth login to use the Factory"},
	"no remote": {func(f *factoryFixture) {
		f.put("view.err", "none of the git remotes configured for this repository point to a known GitHub host. To tell gh about a new GitHub host, please use `gh auth login`\n")
	}, "This project has no GitHub remote"},
}

func TestTheFactoryCapIsServedOnlyWithAConfigStore(t *testing.T) {
	hello := `{"type":"hello","id":"h","clientId":"c","protocolVersion":3,"caps":["factory.v1"]}`
	_, _, without := local(t, peer.OwnerOf(1))
	without.send(hello)
	if m := without.recv(); m["type"] != "hello.ok" || fmt.Sprint(m["caps"]) != "[]" {
		t.Fatalf("%v", m)
	}
	_, _, with := local(t, peer.OwnerOf(1), func(s *Server) { s.FactoryConfigs = factory.OpenConfigs(t.TempDir()) })
	with.send(hello)
	if m := with.recv(); m["type"] != "hello.ok" || fmt.Sprint(m["caps"]) != "[factory.v1]" {
		t.Fatalf("%v", m)
	}
}

func TestFactoryConfigGetAnswersTheDefaultsWhenNothingIsSaved(t *testing.T) {
	f := newFactoryFixture(t)
	p := f.peer(peer.OwnerOf(1), proto.CapFactory)
	p.send(f.get())
	m := p.recv()
	cfg, _ := m["config"].(map[string]any)
	if m["type"] != "factory.config" || m["id"] != "g" || m["project"] != f.project || m["problem"] != nil ||
		cfg["enabled"] != false || cfg["spec"] != true || cfg["lead"] != "claude-opus" || cfg["fix"] != 2.0 || fmt.Sprint(cfg["checks"]) != "[]" {
		t.Fatalf("%v", m)
	}
}

func TestFactoryConfigGetSaysWhyTheFactoryCannotBeTurnedOn(t *testing.T) {
	for name, c := range outOfReach {
		f := newFactoryFixture(t)
		c.cut(f)
		p := f.peer(peer.OwnerOf(1), proto.CapFactory)
		p.send(f.get())
		if m := p.recv(); m["type"] != "factory.config" || m["problem"] != c.problem {
			t.Errorf("%s: %v", name, m)
		}
	}
}

func TestAFactoryVerbWithoutTheCapSaysTheFactoryIsOff(t *testing.T) {
	f := newFactoryFixture(t)
	p := f.peer(peer.OwnerOf(1))
	p.send(f.get())
	if m := p.recv(); m["type"] != "error" || m["code"] != proto.CodeFactoryOff || m["message"] != "The Factory is off" {
		t.Fatalf("%v", m)
	}
}

func TestAFactoryVerbForAFolderThatIsNoProjectIsRefused(t *testing.T) {
	f := newFactoryFixture(t)
	p := f.peer(peer.OwnerOf(1), proto.CapFactory)
	p.send(`{"type":"factory.config.get","id":"g","project":"/elsewhere"}`)
	if m := p.recv(); m["type"] != "error" || m["code"] != proto.CodeUnknownProject {
		t.Fatalf("%v", m)
	}
}

func TestAnAgentInATerminalReadsTheFactoryConfig(t *testing.T) {
	f := newFactoryFixture(t)
	p := f.peer(peer.Principal{Kind: peer.PTY, Terminal: "t1", Scopes: peer.PTYScopes}, proto.CapFactory)
	p.send(f.get())
	if m := p.recv(); m["type"] != "factory.config" || m["project"] != f.project {
		t.Fatalf("%v", m)
	}
}
```

**Step 2: Run the tests and see them fail**

```sh
go test ./internal/proto -run '^TestTheServerOffersTheFactoryCap$'
go test ./internal/wsserver/ -run 'Factory'
```

Expected: the first fails with `caps [], code ""`. The second does not compile, with `unknown field FactoryConfigs in struct literal of type Server` (the message may read `s.FactoryConfigs undefined`).

**Step 3: Implement**

`internal/proto/version.go:39`. Append `CapFactory` to the list. Before:

```go
... CapCompare, CapGhState, CapFront, CapTeam, CapBoard}
```

After:

```go
... CapCompare, CapGhState, CapFront, CapTeam, CapBoard, CapFactory}
```

`internal/wsserver/wsserver.go`: add `"pocketd/internal/factory"` to the imports, in alphabetical order. Then `:107-110`. Before:

```go
	// Skills keeps what skills.list last found; nil serves none.
	Skills *skills.Store
	// Teams serves team prompts and team.list; nil refuses them.
	Teams *team.Teams
```

After:

```go
	// Skills keeps what skills.list last found; nil serves none.
	Skills *skills.Store
	// Teams serves team prompts and team.list; nil refuses them.
	Teams *team.Teams
	// FactoryConfigs serves the factory.config verbs; nil drops factory.v1.
	FactoryConfigs *factory.Configs
```

`wsserver.go:141-145`, in `conn`. Before:

```go
	// automationMu keeps a verb's ack ahead of the snapshot its change triggers.
	automationMu sync.Mutex
```

After:

```go
	// automationMu keeps a verb's ack ahead of the snapshot its change triggers.
	automationMu sync.Mutex
	// factoryTurn closes when this conn's last factory.config verb has answered.
	factoryTurn chan struct{}
```

`wsserver.go:262-265`, in `handle`. Before:

```go
		served := proto.ServerCaps
		if c.s.Teams == nil {
			served = slices.DeleteFunc(slices.Clone(served), func(c string) bool { return c == proto.CapTeam })
		}
```

After:

```go
		served := proto.ServerCaps
		if c.s.Teams == nil {
			served = slices.DeleteFunc(slices.Clone(served), func(c string) bool { return c == proto.CapTeam })
		}
		if c.s.FactoryConfigs == nil {
			served = slices.DeleteFunc(slices.Clone(served), func(c string) bool { return c == proto.CapFactory })
		}
```

`wsserver.go:616-617`, in `dispatch`. Before:

```go
	case "team.start", "team.stop", "team.merge":
		return c.teamRun(m)
```

After:

```go
	case "team.start", "team.stop", "team.merge":
		return c.teamRun(m)
	case "factory.config.get":
		return c.factoryVerb(m)
```

Create `internal/wsserver/factory.go`:

```go
package wsserver

import (
	"errors"
	"slices"

	"pocketd/internal/factory"
	"pocketd/internal/github"
	"pocketd/internal/proto"
)

// Why the Factory can't be turned on for a project, as its settings page says it.
const (
	factoryNoGh      = "Install GitHub CLI (gh) to use the Factory"
	factorySignedOut = "Run gh auth login to use the Factory"
	factoryNoRemote  = "This project has no GitHub remote"
)

// factoryVerb serves the Factory's config verbs for a project on this Mac.
func (c *conn) factoryVerb(m proto.ClientMessage) error {
	if c.s.FactoryConfigs == nil || !slices.Contains(c.caps, proto.CapFactory) {
		c.send(proto.NewErrorCode(m.ID, proto.CodeFactoryOff, "The Factory is off"))
		return nil
	}
	if err := c.usableFolder(m.Project); err != nil {
		c.send(factoryError(m.ID, err))
		return nil
	}
	c.factoryNext(func() {
		c.send(proto.NewFactoryConfig(m.ID, m.Project, c.s.FactoryConfigs.Get(m.Project), c.factoryProblem(m.Project)))
	})
	return nil
}

// factoryNext runs do off the read loop, after this conn's earlier
// factory.config verbs have answered: asking gh takes seconds, and a save
// that overtook the one before it would leave the older settings saved.
func (c *conn) factoryNext(do func()) {
	before, done := c.factoryTurn, make(chan struct{})
	c.factoryTurn = done
	go func() {
		defer close(done)
		if before != nil {
			<-before
		}
		do()
	}()
}

// factoryProblem is why the Factory can't be turned on for project, or "".
// A gh that fails for any other reason, as it does offline, is no problem.
func (c *conn) factoryProblem(project string) string {
	if c.s.GitHub == nil {
		return factoryNoGh
	}
	switch c.s.GitHub.Status(c.ctx, project).State {
	case string(github.Missing):
		return factoryNoGh
	case string(github.SignedOut):
		return factorySignedOut
	}
	if _, problem := c.s.GitHub.PR(c.ctx, project); problem == github.NoRemote {
		return factoryNoRemote
	}
	return ""
}

func factoryError(id string, err error) proto.Error {
	switch {
	case errors.Is(err, factory.ErrInvalid):
		return proto.NewErrorCode(id, proto.CodeInvalidConfig, err.Error())
	case errors.Is(err, errUnknownProject), errors.Is(err, errFolderUnusable):
		return proto.NewErrorCode(id, proto.CodeUnknownProject, err.Error())
	}
	return proto.NewError(id, err.Error())
}
```

Only the conn's read loop calls `dispatch`, so `factoryTurn` needs no lock.

**Step 4: Run the tests and see them pass**

```sh
go test ./internal/proto ./internal/wsserver/
```

Expected: `ok` for both packages.

### Task 3.4: `factory.config.save`

**Files:**
- Modify: `packages/pocketd/internal/wsserver/factory.go` (`factoryVerb`, new `factorySave`)
- Modify: `packages/pocketd/internal/wsserver/wsserver.go` (the `dispatch` case of Task 3.3)
- Modify: `packages/pocketd/internal/wsserver/factory_test.go`

**Context:** A save replaces the project's whole config and answers the config that pocketd kept. Three refusals exist. A config out of range answers `invalid_config` before `gh` is asked. A save that would turn the Factory on while a problem exists stores nothing and answers `factory_unavailable` with the problem as the message (C9, E12). A peer without the `owner` scope gets `scope_denied` from `dispatch`. While the Factory is on and stays on, the save does not ask `gh`: the user must be able to change or turn off a project whose `gh` signed out later. The fake `gh` waits before every call when a file `sleep` holds a number of seconds, which the ordering test uses. Stories: US-33, criterion 3, and US-35, criterion 2.

**Step 1: Write the failing tests**

Add to `internal/wsserver/factory_test.go`:

```go
func (f *factoryFixture) save(enabled bool, slots int) string {
	return fmt.Sprintf(`{"type":"factory.config.save","id":"s","project":%q,"config":{"enabled":%v,"spec":true,"reviewer":true,"lead":"claude-opus","workers":"claude-sonnet","review":"codex","slots":%d,"fix":2,"checks":["go test ./..."]}}`, f.project, enabled, slots)
}

func slotsOf(m map[string]any) any {
	cfg, _ := m["config"].(map[string]any)
	return cfg["slots"]
}

func TestFactoryConfigSaveKeepsTheConfigAndAnswersIt(t *testing.T) {
	f := newFactoryFixture(t)
	p := f.peer(peer.OwnerOf(1), proto.CapFactory)
	p.send(f.save(true, 2))
	m := p.recv()
	cfg, _ := m["config"].(map[string]any)
	if m["type"] != "factory.config" || m["id"] != "s" || m["problem"] != nil || cfg["enabled"] != true || cfg["slots"] != 2.0 || fmt.Sprint(cfg["checks"]) != "[go test ./...]" {
		t.Fatalf("%v", m)
	}
	if got := f.configs.Get(f.project); !got.Enabled || f.configs.Limit(f.project) != 2 {
		t.Fatalf("kept %+v", got)
	}
}

func TestTheFactoryCannotBeTurnedOnWhileGitHubIsOutOfReach(t *testing.T) {
	for name, c := range outOfReach {
		f := newFactoryFixture(t)
		c.cut(f)
		p := f.peer(peer.OwnerOf(1), proto.CapFactory)
		p.send(f.save(true, 0))
		if m := p.recv(); m["type"] != "error" || m["id"] != "s" || m["code"] != proto.CodeFactoryUnavailable || m["message"] != c.problem {
			t.Errorf("%s: %v", name, m)
		}
		if f.configs.Get(f.project).Enabled {
			t.Errorf("%s: the Factory was turned on", name)
		}
	}
}

func TestASaveWithTheFactoryOffIsKeptAndSaysWhyItCannotBeTurnedOn(t *testing.T) {
	f := newFactoryFixture(t)
	f.withGh.Store(false)
	p := f.peer(peer.OwnerOf(1), proto.CapFactory)
	p.send(f.save(false, 3))
	if m := p.recv(); m["type"] != "factory.config" || m["problem"] != "Install GitHub CLI (gh) to use the Factory" || slotsOf(m) != 3.0 {
		t.Fatalf("%v", m)
	}
	if got := f.configs.Get(f.project); got.Enabled || got.Slots != 3 {
		t.Fatalf("kept %+v", got)
	}
}

func TestASaveThatLeavesTheFactoryOnDoesNotAskGhAgain(t *testing.T) {
	f := newFactoryFixture(t)
	p := f.peer(peer.OwnerOf(1), proto.CapFactory)
	p.send(f.save(true, 1))
	if m := p.recv(); m["type"] != "factory.config" {
		t.Fatalf("%v", m)
	}
	f.withGh.Store(false)
	p.send(f.save(true, 2))
	if m := p.recv(); m["type"] != "factory.config" || m["problem"] != nil || slotsOf(m) != 2.0 {
		t.Fatalf("%v", m)
	}
	p.send(f.save(false, 2))
	if m := p.recv(); m["type"] != "factory.config" || m["problem"] != "Install GitHub CLI (gh) to use the Factory" {
		t.Fatalf("turning it off: %v", m)
	}
}

func TestAFactoryConfigOutOfRangeIsRefusedAsInvalid(t *testing.T) {
	f := newFactoryFixture(t)
	p := f.peer(peer.OwnerOf(1), proto.CapFactory)
	p.send(f.save(false, proto.MaxBoardLimit+1))
	if m := p.recv(); m["type"] != "error" || m["code"] != proto.CodeInvalidConfig || m["message"] != "These Factory settings aren't valid" {
		t.Fatalf("%v", m)
	}
	if got := f.configs.Get(f.project); got.Slots != 0 {
		t.Fatalf("kept %+v", got)
	}
}

func TestSavesSentBackToBackAreKeptInTheOrderSent(t *testing.T) {
	f := newFactoryFixture(t)
	f.put("sleep", "0.1")
	p := f.peer(peer.OwnerOf(1), proto.CapFactory)
	p.send(f.save(false, 1))
	p.send(f.save(false, 2))
	if first, second := p.recv(), p.recv(); slotsOf(first) != 1.0 || slotsOf(second) != 2.0 {
		t.Fatalf("answered %v then %v", first, second)
	}
	if got := f.configs.Get(f.project); got.Slots != 2 {
		t.Fatalf("kept %+v", got)
	}
}

func TestAnAgentInATerminalCannotSaveTheFactoryConfig(t *testing.T) {
	f := newFactoryFixture(t)
	p := f.peer(peer.Principal{Kind: peer.PTY, Terminal: "t1", Scopes: peer.PTYScopes}, proto.CapFactory)
	p.send(f.save(false, 1))
	if m := p.recv(); m["type"] != "error" || m["code"] != "scope_denied" {
		t.Fatalf("%v", m)
	}
	if got := f.configs.Get(f.project); got.Slots != 0 {
		t.Fatalf("kept %+v", got)
	}
}
```

**Step 2: Run the tests and see them fail**

```sh
go test ./internal/wsserver/ -run 'Factory|TestSavesSentBackToBack'
```

Expected: every new test but the last fails, because `dispatch` has no case for `factory.config.save` and falls through to the agent lookup. The replies are `error` frames with no `code`. `TestAnAgentInATerminalCannotSaveTheFactoryConfig` passes: Task 3.2's row already refuses the peer.

**Step 3: Implement**

`internal/wsserver/wsserver.go`, the case that Task 3.3 added. Before:

```go
	case "factory.config.get":
		return c.factoryVerb(m)
```

After:

```go
	case "factory.config.get", "factory.config.save":
		return c.factoryVerb(m)
```

`internal/wsserver/factory.go`, in `factoryVerb`. Before:

```go
	if err := c.usableFolder(m.Project); err != nil {
		c.send(factoryError(m.ID, err))
		return nil
	}
	c.factoryNext(func() {
		c.send(proto.NewFactoryConfig(m.ID, m.Project, c.s.FactoryConfigs.Get(m.Project), c.factoryProblem(m.Project)))
	})
	return nil
}
```

After:

```go
	if err := c.usableFolder(m.Project); err != nil {
		c.send(factoryError(m.ID, err))
		return nil
	}
	if m.Config != nil && !m.Config.Valid() {
		c.send(factoryError(m.ID, factory.ErrInvalid))
		return nil
	}
	c.factoryNext(func() {
		if m.Type == "factory.config.save" {
			c.factorySave(m)
			return
		}
		c.send(proto.NewFactoryConfig(m.ID, m.Project, c.s.FactoryConfigs.Get(m.Project), c.factoryProblem(m.Project)))
	})
	return nil
}

// factorySave keeps a project's settings, unless they turn the Factory on
// where it can't work. While the Factory is on and stays on gh isn't asked, so
// a gh that signed out later can't lock the settings.
func (c *conn) factorySave(m proto.ClientMessage) {
	configs, cfg := c.s.FactoryConfigs, *m.Config
	problem := ""
	if !cfg.Enabled || !configs.Get(m.Project).Enabled {
		problem = c.factoryProblem(m.Project)
	}
	if cfg.Enabled && problem != "" {
		c.send(proto.NewErrorCode(m.ID, proto.CodeFactoryUnavailable, problem))
		return
	}
	if err := configs.Save(m.Project, cfg); err != nil {
		c.send(factoryError(m.ID, err))
		return
	}
	c.send(proto.NewFactoryConfig(m.ID, m.Project, configs.Get(m.Project), problem))
}
```

**Step 4: Run the tests and see them pass**

```sh
go test ./internal/wsserver/
```

Expected: `ok  	pocketd/internal/wsserver`.

### Task 3.5: Wire the store in `serve.go`

**Files:**
- Modify: `packages/pocketd/cmd/pocketd/serve.go:24-50` (imports), `:247-251`, `:257`
- Create: `packages/pocketd/e2e/factory_test.go`

**Context:** `serve.go` composes pocketd. It opens the Board at `:247` and builds the Board's scheduler at `:248`. The config store opens beside the Board, from the same `home`. Its `Limit` method becomes `Scheduler.Limit`, and the store itself becomes `Server.FactoryConfigs`. The e2e harness has no fake `gh`, and a real `gh` on the test machine would make a reply depend on the network. So the e2e test asks for a folder that is no project. That request never reaches `gh`, and its answer tells a wired store (`unknown_project`) from a missing one (`factory_off`). No e2e test turns the Factory on, so the line that sets `Scheduler.Limit` is covered by `go vet` and by Task 1.3's unit test only.

**Step 1: Write the failing test**

Create `e2e/factory_test.go`:

```go
//go:build unix

package e2e

import (
	"testing"

	"pocketd/internal/proto"
)

func TestAServedPocketdTakesFactoryVerbs(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner(proto.CapFactory)
	o.Send(map[string]any{"type": "factory.config.get", "id": "g1", "project": "/elsewhere"})
	if e := o.WaitFor("the answer to g1", func(m Message) bool { return m.ID == "g1" }); e.Type != "error" || e.Code != proto.CodeUnknownProject {
		t.Fatalf("got %s", e.Raw)
	}
}
```

**Step 2: Run the test and see it fail**

```sh
go test ./e2e -count=1 -run '^TestAServedPocketdTakesFactoryVerbs$'
```

Expected: FAIL with `got {"type":"error","id":"g1","message":"The Factory is off","code":"factory_off"}`.

**Step 3: Implement**

In `cmd/pocketd/serve.go`, add the import after `"pocketd/internal/events"`:

```go
	"pocketd/internal/factory"
```

`serve.go:247-251`. Before:

```go
	tickets := board.Open(home)
	boardScheduler := &board.Scheduler{Store: tickets, Now: time.Now, Watch: board.AgentWatcher(d.Agents),
```

After:

```go
	tickets := board.Open(home)
	factoryConfigs := factory.OpenConfigs(home)
	boardScheduler := &board.Scheduler{Store: tickets, Now: time.Now, Watch: board.AgentWatcher(d.Agents), Limit: factoryConfigs.Limit,
```

`serve.go:257`, the end of the `wsserver.Server` literal. Before:

```go
... Git: gitview.New(d.LoginEnv), Skills: skillStore, Teams: teams}
```

After:

```go
... Git: gitview.New(d.LoginEnv), Skills: skillStore, Teams: teams, FactoryConfigs: factoryConfigs}
```

**Step 4: Run the test and see it pass**

```sh
go vet ./...
go test ./e2e -count=1 -run '^(TestAServedPocketdTakesFactoryVerbs|TestOverTheLimitATicketWaitsInQueued|TestATicketThatAsksLetsTheNextInLineStart)$'
```

Expected: `go vet` prints nothing, and the e2e package is `ok`. The two Board tests prove that a scheduler with `Limit` set still starts Tickets of a project whose Factory is off.

### Task 3.6: Red-team probes for the gate verbs

**Files:**
- Modify: `packages/pocketd/e2e/redteam/main.go:158-172` (`wsOutcomes`)
- Modify: `packages/pocketd/e2e/redteam_test.go:32-59`

**Context:** `scripts/red-team.sh` is the no-self-approval gate. It runs `TestRedTeamFromInsideATerminal`, which spawns `e2e/redteam` inside a Pocket Terminal while a claude waits on an ask. The probe sends verbs over the socket as a terminal peer and records each outcome. Task 3.2 proves the scope rows in a unit test. This task proves the same refusal over a real socket for the three gate verbs that decode today: `team.start` (approve a plan), `gh.pr.merge` (merge) and `factory.config.save` (change the Factory's settings). `permission.resolve` has its probe already (`ws.resolve`). The scope check runs before the cap check, so the probe needs no new cap in its hello. Each message must decode, or the outcome would be `error` for `Malformed message` and prove nothing. Stories: US-35, criterion 6.

**Step 1: Write the failing test**

In `e2e/redteam_test.go`, add these rows at the end of the expected map, after `"ops.fs.write"` (:58) and one blank line:

```go

		"ws.team.start":          "scope_denied",
		"ws.gh.pr.merge":         "scope_denied",
		"ws.factory.config.save": "scope_denied",
```

**Step 2: Run the test and see it fail**

From the repository root:

```sh
scripts/red-team.sh
```

Expected: FAIL with `ws.team.start: "", want "scope_denied"` and the same line for the other two probes.

**Step 3: Implement**

In `e2e/redteam/main.go`, add three probes at the end of the list in `wsOutcomes`, after the `ws.terminal.input` row (:171):

```go
		{"ws.team.start", map[string]any{"type": "team.start", "id": "10", "lead": agentID, "tasks": []map[string]any{{"id": "t1", "agent": "claude"}, {"id": "t2", "agent": "claude"}}}},
		{"ws.gh.pr.merge", map[string]any{"type": "gh.pr.merge", "id": "11", "tree": "/", "number": 1, "method": "squash"}},
		{"ws.factory.config.save", map[string]any{"type": "factory.config.save", "id": "12", "project": "/", "config": map[string]any{
			"enabled": true, "spec": false, "reviewer": false, "lead": "claude-opus", "workers": "claude-sonnet", "review": "codex", "slots": 0, "fix": 0, "checks": []string{}}}},
```

**Step 4: Run the test and see it pass**

```sh
scripts/red-team.sh
```

Expected: `--- PASS: TestRedTeamFromInsideATerminal` and `ok  	pocketd/e2e`. Then, from `packages/pocketd`, `gofmt -l e2e` prints nothing. If it lists a file, run `gofmt -w` on it.

### PR 3 final checks

From `packages/pocketd`: `go vet ./...` (no output), `go test ./...` (every package `ok`), `gofmt -l .` (no output).
From the repository root: `scripts/red-team.sh` passes. This PR changed `peer.Needs`, so the gate is required.
`git status --short packages/pocketd/internal/proto/testdata/golden` prints nothing.

---

## PR 4: The desktop's Factory wire

**Scope:** The `agents` crate reads the five Ticket fields, offers `factory.v1`, sends the two config verbs, and turns `factory.config` and Factory errors into events. `Agents` keeps each project's config. `hosts` drops Factory events of a linked host. A refused Factory request is a toast. Nothing calls `Outbox::factory_config` or reads `Agents::factory_offered` until plans 2 and 3, so no screen changes.
**Depends on:** PR 2 (the goldens under `testdata/factory/`). PR 3 for a pocketd that answers.
**Done when:** from `packages/desktop`, `cargo build --workspace`, `cargo clippy --workspace --all-targets` (no new warnings) and `cargo test --workspace` pass.

### Task 4.1: Ticket fields and `Ticket::sample()`

**Files:**
- Modify: `packages/desktop/crates/agents/src/tickets.rs:30-80` (`Station`, `Gate`, `Ticket`)
- Modify: `packages/desktop/crates/agents/src/agents.rs` (one new test, after `board_frames_become_board_events` at :2213)
- Modify: `packages/desktop/crates/pocket/src/board/logic.rs:764-787` (the test helper `ticket`)
- Modify: `packages/desktop/crates/pocket/src/board/values.rs:131`, `:198-221` (the test helper `saved`)

**Context:** `Ticket` (`tickets.rs:41`) has no `Default`, because `State` has none. Two test helpers write every field by hand (`logic.rs:764` and `values.rs:198`), so each new field breaks both. The index asks for one shared helper, `Ticket::sample()`. It is a plain `pub fn` in `tickets.rs`: the desktop workspace has no test-support feature, and `pocket`'s tests cannot see a `#[cfg(test)]` item of `agents`. `Status` (`tickets.rs:14`) shows the pattern for an enum that must read a value from a newer pocketd: `#[default] #[serde(other)] None`. `Frame.tickets` is read with `automations::readable_items` (`agents.rs:378`), which drops a Ticket it cannot read, so a wrong type in a new field would hide the Ticket. The only other place that builds a `Ticket` is `pocket/src/capture.rs:509`, with `..t`, and it needs no change.

**Step 1: Write the failing test**

In `agents/src/agents.rs`, add after `board_frames_become_board_events` (:2213):

```rust
    #[test]
    fn a_factory_ticket_carries_its_station_gate_pr_and_fixes() {
        let snapshot = include_str!("../../../../pocketd/internal/proto/testdata/factory/server/board.json");
        let Some(Event::Board { tickets, limit: 4 }) = board_event(&serde_json::from_str::<Frame>(snapshot).unwrap()) else { panic!("not a board") };
        let t = &tickets[0];
        assert_eq!((t.station, t.gate, t.pr, t.fix, t.hint), (tickets::Station::Checks, tickets::Gate::Ask, 12, 1, true));
        let plain = &tickets[1];
        assert_eq!((plain.station, plain.gate, plain.pr, plain.fix, plain.hint), (tickets::Station::None, tickets::Gate::None, 0, 0, false));
        let newer: Ticket = serde_json::from_value(json!({"id": "t", "title": "T", "prompt": "P", "provider": "claude", "folder": "/app", "state": "live", "station": "deploy", "gate": "legal"})).unwrap();
        assert_eq!((newer.station, newer.gate), (tickets::Station::None, tickets::Gate::None));
        let sample = Ticket::sample();
        assert_eq!((sample.state, sample.station, sample.gate), (tickets::State::Todo, tickets::Station::None, tickets::Gate::None));
    }
```

**Step 2: Run the test and see it fail**

From `packages/desktop`:

```sh
cargo test -p agents a_factory_ticket
```

Expected: the crate does not compile. The errors name `no field `station` on type `&Ticket``, `could not find `Station` in `tickets`` and `no function or associated item named `sample``.

**Step 3: Implement**

In `agents/src/tickets.rs`, add after `ClosedWhy` (:39):

```rust
/// Where the Factory has a ticket.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Station {
    Spec,
    Plan,
    Implement,
    Checks,
    Pr,
    Merged,
    /// A ticket the Factory doesn't run, or a station a newer pocketd added.
    #[default]
    #[serde(other)]
    None,
}

/// What a Factory ticket waits on the user for.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Gate {
    Plan,
    Pr,
    Ask,
    /// Nothing, or a gate a newer pocketd added.
    #[default]
    #[serde(other)]
    None,
}
```

In `Ticket`, add after `labels` (:78-79). Before:

```rust
    #[serde(default)]
    pub labels: Vec<String>,
}
```

After:

```rust
    #[serde(default)]
    pub labels: Vec<String>,
    /// Only pocketd sets the five below; `TicketDraft` carries none.
    #[serde(default)]
    pub station: Station,
    #[serde(default)]
    pub gate: Gate,
    /// Its pull request's number; 0 until one is open.
    #[serde(default)]
    pub pr: u32,
    /// Auto-fix attempts used.
    #[serde(default)]
    pub fix: u8,
    /// The user's hint granted one more attempt.
    #[serde(default)]
    pub hint: bool,
}

impl Ticket {
    /// A ticket in To do with nothing else set, for tests to build on with `..Ticket::sample()`: a new field is added here once.
    pub fn sample() -> Self {
        Self {
            id: "t".into(),
            title: "T".into(),
            prompt: "P".into(),
            provider: "claude".into(),
            folder: "/app".into(),
            base: String::new(),
            state: State::Todo,
            order: 0,
            status: Status::None,
            why: String::new(),
            agent_id: String::new(),
            terminal_id: String::new(),
            worktree: String::new(),
            created_at: 0,
            changed_at: 0,
            closed_at: 0,
            closed_why: ClosedWhy::default(),
            number: 0,
            priority: 0,
            labels: Vec::new(),
            station: Station::None,
            gate: Gate::None,
            pr: 0,
            fix: 0,
            hint: false,
        }
    }
}
```

`pocket/src/board/logic.rs:764-787`. Before: the helper `ticket` with a 20-field literal. After:

```rust
    fn ticket(id: &str, state: State, status: Status) -> Ticket {
        Ticket { id: id.into(), title: id.into(), prompt: "do".into(), folder: "/repo".into(), state, status, agent_id: "a".into(), ..Ticket::sample() }
    }
```

`pocket/src/board/values.rs:198-221`. Before: the helper `saved` with a 20-field literal. After:

```rust
    fn saved(state: State) -> Ticket {
        Ticket { state, ..Ticket::sample() }
    }
```

`values.rs:131`. `ClosedWhy` is no longer used in the test module. Before:

```rust
    use agents::tickets::{ClosedWhy, Status};
```

After:

```rust
    use agents::tickets::Status;
```

Both helpers return the same values as before: `sample()` holds what `saved(State::Todo)` held, and `ticket` overrides the six fields that differed.

**Step 4: Run the tests and see them pass**

```sh
cargo test -p agents a_factory_ticket
cargo test -p pocket board::
cargo build --workspace
```

Expected: `test result: ok` for both test commands, and the build has no error and no new warning.

### Task 4.2: `agents::factory::Config`, the events, and `Agents.factory`

**Files:**
- Create: `packages/desktop/crates/agents/src/factory.rs`
- Modify: `packages/desktop/crates/agents/src/agents.rs:1-8` (modules), `:336-383` (`Frame`), `:400-461` (`Event`), `:463-486` (`Agents`), `:524-587` (`apply_from`), `:697-699`, `:813-825` (id prefixes), `:906-912`, `:1232` (the event chain), tests
- Modify: `packages/desktop/crates/hosts/src/session.rs:152-161` (`tag_event`), tests
- Modify: `packages/desktop/crates/pocket/src/desktop/alerts.rs:143`

**Context:** Every frame from pocketd is read into one `Frame` (`agents.rs:336`). A function for each area turns a frame into an `Event` (`board_event` at :898), and `run` tries them in a chain (:1232). An `error` frame belongs to the area whose id prefix it carries: `board-` for the Board (:817), `factory-` for the Factory. `Agents::apply_from` (:524) and `hosts::session::tag_event` (`session.rs:127`) each match every `Event` variant, so a new variant must be added to both or the workspace does not compile. `tag_event` returns false for news that only this Mac's pocketd gives, such as the Board. The Factory belongs to that group. `desktop/alerts.rs:143` shows a refused Board or Automation request as a toast, and the index says a refused Factory request is one too. `Frame` has `project: String` already. No other server frame has a `config` key, and `gh.pr`'s `problem` is a string too, so the two new `Frame` fields cannot break another frame. The Go goldens of PR 2 are the test data.

**Step 1: Write the failing tests**

In `agents/src/agents.rs`, add after `a_factory_ticket_carries_its_station_gate_pr_and_fixes`:

```rust
    #[test]
    fn factory_frames_become_factory_events() {
        let ev = |raw: &str| factory_event(&serde_json::from_str::<Frame>(raw).unwrap());
        let reply = include_str!("../../../../pocketd/internal/proto/testdata/factory/server/factory_config.json");
        assert!(matches!(ev(reply), Some(Event::FactoryConfig { project, config, problem })
            if project == "/app" && config.enabled && config.spec && config.lead == "claude-opus" && (config.slots, config.fix) == (2, 1) && config.checks == ["cargo test", "cargo clippy --all-targets"] && problem.is_empty()));
        let blocked = include_str!("../../../../pocketd/internal/proto/testdata/factory/server/factory_config_problem.json");
        assert!(matches!(ev(blocked), Some(Event::FactoryConfig { config, problem, .. })
            if !config.enabled && config.reviewer && config.checks.is_empty() && problem == "Install GitHub CLI (gh) to use the Factory"));
        let refused = include_str!("../../../../pocketd/internal/proto/testdata/factory/server/error_factory_unavailable.json");
        assert!(matches!(ev(refused), Some(Event::FactoryError(m)) if m == "This project has no GitHub remote"));
        assert!(ev(r#"{"type":"error","id":"board-3","message":"Nope"}"#).is_none());
        assert!(ev(r#"{"type":"factory.config","id":"factory-1","project":"/app"}"#).is_none());
    }

    #[test]
    fn a_projects_factory_settings_are_kept_as_pocketd_last_sent_them() {
        let mut a = Agents::default();
        assert!(a.factory_config("/app").is_none());
        let on = factory::Config { enabled: true, slots: 2, ..Default::default() };
        a.apply(Event::FactoryConfig { project: "/app".into(), config: on.clone(), problem: String::new() });
        assert_eq!(a.factory_config("/app"), Some(&on));
        a.apply(Event::FactoryConfig { project: "/app".into(), config: factory::Config::default(), problem: "Run gh auth login to use the Factory".into() });
        assert_eq!(a.factory_config("/app").map(|c| c.enabled), Some(false));
        assert!(a.factory_config("/other").is_none());
        a.apply(Event::FactoryError("Nope".into()));
        assert!(a.factory_config("/app").is_some());
    }
```

In `hosts/src/session.rs`, add after `a_hosts_agent_events_are_tagged_and_its_phone_and_pairing_news_dropped` (:448):

```rust
    #[test]
    fn a_hosts_factory_news_is_dropped_as_its_board_is() {
        assert!(!tag_event("devbox", &mut Event::FactoryConfig { project: "/home/me/app".into(), config: Default::default(), problem: String::new() }));
        assert!(!tag_event("devbox", &mut Event::FactoryError("Nope".into())));
    }
```

**Step 2: Run the tests and see them fail**

```sh
cargo test -p agents factory
```

Expected: the crate does not compile. The errors name `cannot find function `factory_event``, `no variant named `FactoryConfig`` and `use of undeclared crate or module `factory``.

**Step 3: Implement**

Create `agents/src/factory.rs`:

```rust
use serde::{Deserialize, Serialize};

/// One project's Factory settings, as `factory.config` carries them and `factory.config.save` sends them whole.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub enabled: bool,
    pub spec: bool,
    pub reviewer: bool,
    /// The lead's profile id.
    pub lead: String,
    /// The profile id each task's worker gets unless the plan names another.
    pub workers: String,
    /// The reviewer's profile id.
    pub review: String,
    /// The project's own slot limit; 0 is the Board's.
    pub slots: u8,
    /// Auto-fix attempts a ticket gets.
    pub fix: u8,
    /// The local commands the Checks station runs, in order.
    pub checks: Vec<String>,
}
```

`agents/src/agents.rs:1-8`. Before:

```rust
pub mod automations;
pub mod catalogs;
pub mod tickets;
pub mod locals;
pub mod providers;
pub mod team;

pub use team::{TaskPick, Team, TeamAction, TeamTask};
```

After:

```rust
pub mod automations;
pub mod catalogs;
pub mod factory;
pub mod tickets;
pub mod locals;
pub mod providers;
pub mod team;

pub use factory::Config;
pub use team::{TaskPick, Team, TeamAction, TeamTask};
```

`Frame` (:381-383). Before:

```rust
    team: Option<Team>,
    teams: Vec<Team>,
}
```

After:

```rust
    team: Option<Team>,
    teams: Vec<Team>,
    config: Option<Config>,
    problem: String,
}
```

`Event` (:459-461). Before:

```rust
    /// pocketd refused a team request from `lead`'s chat, or the socket never took it; `message` says why.
    TeamFailed { lead: String, action: TeamAction, message: String },
}
```

After:

```rust
    /// pocketd refused a team request from `lead`'s chat, or the socket never took it; `message` says why.
    TeamFailed { lead: String, action: TeamAction, message: String },
    /// `project`'s Factory settings as pocketd keeps them; `problem` says why the Factory can't be turned on there, empty when it can.
    FactoryConfig { project: String, config: Config, problem: String },
    /// pocketd refused a Factory message.
    FactoryError(String),
}
```

`Agents` (:484-486). Before:

```rust
    /// Each team by its lead's id, from `team.list` and `team.update`.
    pub teams: HashMap<String, Team>,
}
```

After:

```rust
    /// Each team by its lead's id, from `team.list` and `team.update`.
    pub teams: HashMap<String, Team>,
    /// Each project's Factory settings by its folder, from `factory.config`.
    pub factory: HashMap<String, Config>,
}
```

`apply_from` (:585). Before:

```rust
            Event::AutomationError(_) | Event::BoardError(_) | Event::PromptFailed { .. } | Event::InterruptFailed { .. } | Event::Commands { .. } | Event::Skills { .. } | Event::Btw { .. } | Event::Mcp { .. } | Event::McpActFailed { .. } | Event::TeamFailed { .. } => {}
```

After:

```rust
            Event::FactoryConfig { project, config, .. } => {
                self.factory.insert(project, config);
            }
            Event::AutomationError(_) | Event::BoardError(_) | Event::PromptFailed { .. } | Event::InterruptFailed { .. } | Event::Commands { .. } | Event::Skills { .. } | Event::Btw { .. } | Event::Mcp { .. } | Event::McpActFailed { .. } | Event::TeamFailed { .. } | Event::FactoryError(_) => {}
```

Add after `teams_offered` (:697-699):

```rust
    /// `project`'s Factory settings, once pocketd has sent them.
    pub fn factory_config(&self, project: &str) -> Option<&Config> {
        self.factory.get(project)
    }
```

Id prefixes (:824-825). Before:

```rust
const TEAMS: &str = "teams";
const TEAM: &str = "team-";
```

After:

```rust
const TEAMS: &str = "teams";
const TEAM: &str = "team-";
const FACTORY: &str = "factory-";
```

Add after `team_event` (:906-912):

```rust
fn factory_event(f: &Frame) -> Option<Event> {
    match f.kind.as_str() {
        "factory.config" => Some(Event::FactoryConfig { project: f.project.clone(), config: f.config.clone()?, problem: f.problem.clone() }),
        "error" if f.id.starts_with(FACTORY) => Some(Event::FactoryError(readable(&f.message))),
        _ => None,
    }
}
```

The chain in `run` (:1232). Before:

```rust
        if let Some(ev) = requests.answer(&f).or_else(|| launch_event(&f)).or_else(|| names_event(&f)).or_else(|| automation_event(&f)).or_else(|| board_event(&f)).or_else(|| team_event(&f)) {
```

After:

```rust
        if let Some(ev) = requests.answer(&f).or_else(|| launch_event(&f)).or_else(|| names_event(&f)).or_else(|| automation_event(&f)).or_else(|| board_event(&f)).or_else(|| team_event(&f)).or_else(|| factory_event(&f)) {
```

`hosts/src/session.rs:160-161`. Before:

```rust
        | Event::Board { .. }
        | Event::BoardError(_) => return false,
```

After:

```rust
        | Event::Board { .. }
        | Event::BoardError(_)
        | Event::FactoryConfig { .. }
        | Event::FactoryError(_) => return false,
```

`pocket/src/desktop/alerts.rs:143`. Before:

```rust
        if let Event::AutomationError(message) | Event::BoardError(message) = ev {
```

After:

```rust
        if let Event::AutomationError(message) | Event::BoardError(message) | Event::FactoryError(message) = ev {
```

**Step 4: Run the tests and see them pass**

```sh
cargo test -p agents factory
cargo test -p hosts
cargo build --workspace
```

Expected: `test result: ok` for both test commands, and the build has no error. If the build names another `match` on `Event` that is not exhaustive, add `Event::FactoryConfig { .. } | Event::FactoryError(_)` to the arm that holds `Event::BoardError(_)`.

### Task 4.3: `FACTORY_CAP` and `Agents::factory_offered`

**Files:**
- Modify: `packages/desktop/crates/agents/src/agents.rs:696-699`, `:888`, `:1187` (`CAPS`), `:1629` (the hello test), plus one new test

**Context:** The desktop sends `CAPS` in its hello (`agents.rs:1187-1190`), and pocketd answers with the caps both sides speak. `teams_offered` (:697) is the pattern: the cap was negotiated and the connection is not observe-only. The index asks for the same rule for `factory_offered`. Plan 2 reads it to offer the Factory layout. The test `sends_queued_messages_while_pocketd_is_quiet` (:1622) pins the hello frame with every cap by name. The capture fixture's `hello.ok` (`.ui-review/fixture/fixture.ts:404`) lists its caps by hand and does not list `factory.v1`, so no capture changes.

**Step 1: Write the failing tests**

In `agents.rs`, change the hello frame in `sends_queued_messages_while_pocketd_is_quiet` (:1629). Before:

```rust
... "commands.v1", "access.v1", "skills.v1", "team.v1"]})
```

After:

```rust
... "commands.v1", "access.v1", "skills.v1", "team.v1", "factory.v1"]})
```

Add after `team_is_offered_only_by_a_pocketd_that_serves_teams_to_an_owner` (:1975):

```rust
    #[test]
    fn the_factory_is_offered_only_by_a_pocketd_that_serves_it_to_an_owner() {
        let mut a = Agents::default();
        assert!(!a.factory_offered());
        let caps = vec![FACTORY_CAP.to_string()];
        a.apply(Event::Connected { scopes: vec!["owner".into()], caps: caps.clone(), version: String::new() });
        assert!(a.factory_offered());
        a.apply(Event::Connected { scopes: vec!["observe".into()], caps, version: String::new() });
        assert!(!a.factory_offered());
        a.apply(Event::Connected { scopes: vec!["owner".into()], caps: vec![], version: String::new() });
        assert!(!a.factory_offered());
    }
```

**Step 2: Run the tests and see them fail**

```sh
cargo test -p agents factory_is_offered
```

Expected: the crate does not compile, with `cannot find value `FACTORY_CAP`` and `no method named `factory_offered``.

**Step 3: Implement**

`agents.rs:888`. Before:

```rust
const TEAM_CAP: &str = "team.v1";
```

After:

```rust
const TEAM_CAP: &str = "team.v1";
const FACTORY_CAP: &str = "factory.v1";
```

`agents.rs:1187`. Before:

```rust
const CAPS: [&str; 17] = [PAIR_CAP, "scopes.v1", "summary.v2", "host.v1", OPEN_CAP, NAMES_CAP, AUTOMATIONS_CAP, BOARD_CAP, LOCALS_CAP, FORK_CAP, ACCOUNTS_CAP, "subagents.v1", SETTINGS_CAP, "commands.v1", ACCESS_CAP, "skills.v1", TEAM_CAP];
```

After:

```rust
const CAPS: [&str; 18] = [PAIR_CAP, "scopes.v1", "summary.v2", "host.v1", OPEN_CAP, NAMES_CAP, AUTOMATIONS_CAP, BOARD_CAP, LOCALS_CAP, FORK_CAP, ACCOUNTS_CAP, "subagents.v1", SETTINGS_CAP, "commands.v1", ACCESS_CAP, "skills.v1", TEAM_CAP, FACTORY_CAP];
```

Add after `teams_offered` (:697-699), before `factory_config`:

```rust
    /// pocketd runs the Factory for this owner.
    pub fn factory_offered(&self) -> bool {
        self.caps.iter().any(|c| c == FACTORY_CAP) && !self.observe_only()
    }
```

**Step 4: Run the tests and see them pass**

```sh
cargo test -p agents
```

Expected: `test result: ok`, with `sends_queued_messages_while_pocketd_is_quiet` and `the_factory_is_offered_only_by_a_pocketd_that_serves_it_to_an_owner` among the passed tests.

### Task 4.4: `Outbox::factory_config` and `Outbox::factory_save`

**Files:**
- Modify: `packages/desktop/crates/agents/src/agents.rs:839-841` (ids), `:1152-1155` (`Outbox`), tests near `:2303`

**Context:** `Outbox` queues client messages for pocketd (`agents.rs:950`). Each area numbers its request ids with its own prefix (`board_id` at :835), and the prefix is how an `error` reply finds its area (Task 4.2). The index fixes the two names and the id prefix `factory-`. `sent_but_id` and `without_id` (:2309-2320) compare a sent message with a golden, both without their `id`. The client goldens of PR 2 are what pocketd's decoder accepts, so one file pins both sides. `Config` serializes all nine keys, and pocketd's decoder requires all nine.

**Step 1: Write the failing test**

In `agents.rs`, add after the `golden!` macro (:2303-2307):

```rust
    macro_rules! factory_golden {
        ($name:literal) => {
            serde_json::from_str::<Value>(include_str!(concat!("../../../../pocketd/internal/proto/testdata/factory/", $name))).unwrap()
        };
    }

    #[test]
    fn factory_messages_are_the_golden_frames_with_a_factory_request_id() {
        assert_eq!(sent_but_id(|o| o.factory_config("/app")), without_id(factory_golden!("client/factory_config_get.json")));
        let config = Config { enabled: true, spec: true, reviewer: true, lead: "claude-opus".into(), workers: "claude-sonnet".into(), review: "codex".into(), slots: 2, fix: 1, checks: vec!["cargo test".into(), "cargo clippy --all-targets".into()] };
        assert_eq!(sent_but_id(|o| o.factory_save("/app", &config)), without_id(factory_golden!("client/factory_config_save.json")));
        let (tx, rx) = channel();
        let out = Outbox(tx);
        out.factory_config("/app");
        out.factory_save("/app", &config);
        let ids: Vec<String> = rx.try_iter().map(|m| m["id"].as_str().unwrap().to_string()).collect();
        assert!(ids.len() == 2 && ids[0] != ids[1] && ids.iter().all(|id| id.starts_with(FACTORY)));
    }
```

**Step 2: Run the test and see it fail**

```sh
cargo test -p agents factory_messages
```

Expected: the crate does not compile, with `no method named `factory_config` found for reference `&Outbox`` and the same for `factory_save`.

**Step 3: Implement**

Add after `team_id` (:839-841):

```rust
fn factory_id() -> String {
    format!("{FACTORY}{}", NEXT.fetch_add(1, Ordering::Relaxed))
}
```

Add to `impl Outbox`, after `team_merge` (:1152-1155):

```rust
    /// Asks for `project`'s Factory settings; `Event::FactoryConfig` answers.
    pub fn factory_config(&self, project: &str) {
        self.send(json!({"type": "factory.config.get", "id": factory_id(), "project": project}));
    }

    /// Saves `project`'s Factory settings whole; `Event::FactoryConfig` answers with what pocketd kept, or `Event::FactoryError` says why it kept nothing.
    pub fn factory_save(&self, project: &str, config: &Config) {
        self.send(json!({"type": "factory.config.save", "id": factory_id(), "project": project, "config": config}));
    }
```

**Step 4: Run the tests and see them pass**

```sh
cargo test -p agents
```

Expected: `test result: ok`.

### PR 4 final checks

From `packages/desktop`: `cargo build --workspace`, `cargo clippy --workspace --all-targets` (no new warnings), `cargo test --workspace`.
This PR changes no view, so it has no capture task. PR 1 holds the one capture of this plan.

---

## Contract changes needed

- **Wire, the cap paragraph.** Replace "pocketd drops it from the served caps when the engine is nil, as it does for `team.v1`." with: "pocketd drops it from the served caps when `Server.FactoryConfigs` is nil, as it drops `team.v1` when `Server.Teams` is nil. The cap can be served while `Server.Factory` is nil. A verb that needs the engine then answers `factory_off`."
- **Wire, the error codes.** Replace "`factory_off` (the engine is nil or the cap is missing)" with: "`factory_off` (the cap was not negotiated, or the store or the engine that the verb needs is nil)". Add to the list: "`unknown_project` (a verb's `project` is not a project on this Mac, by the check that `board.save` uses)".
- **Wire, the `factory.config` row.** Replace its "Sent" cell with: "reply to `get` and `save`. `problem` is the sentence that says why the Factory can't be turned on (C9), and it is left out when it can. A save that would turn the Factory on while a problem exists stores nothing and answers `error` with `factory_unavailable` and that sentence as `message`. A save that leaves the Factory on does not ask `gh` and sends no `problem`. A conn's config verbs answer in the order they came."
- **pocketd packages, the hooks list.** Add: "`board.Scheduler.Limit func(folder string) int` (plan 1): `Run` passes it to `Store.Claim`. `serve.go` sets it to `Configs.Limit`. Nil gives no project a limit."
- **Checks every plan runs.** Add: "A plan that adds the decoder of a gate verb (`factory.advance`, `factory.profile.save` and `factory.profile.delete` in plan 4, `factory.hint` in plan 5) adds a probe for it to `e2e/redteam/main.go` and its `scope_denied` row to `e2e/redteam_test.go`."

## Open questions

- **The ADR of milestone M0.** The PRD lists an ADR in M0, and no plan in the index owns one. Recommended: plan 3 writes `docs/adr/0008-light-factory.md` when the engine exists, because the decisions worth recording (the engine owns a Factory ticket's status, one pure `Next` function) are plan 3's.
- **Offline.** `github.Failed` does not block turning the Factory on, so a user who is offline can turn it on for a project that has no GitHub remote. The first Ticket then fails at Checks. Recommended: keep it. Blocking on `Failed` would refuse every user whose network is down.
- **Profile ids.** `FactoryConfig.Valid` accepts any string in `lead`, `workers` and `review`, an empty one too. Recommended: plan 4 checks the three ids against its profile store in `conn.factorySave` and answers `unknown_profile`.
- **No e2e test turns the Factory on.** The e2e harness has no fake `gh`, so the line `Limit: factoryConfigs.Limit` in `serve.go` is covered by `go vet` and a unit test only. Recommended: plan 3 puts `internal/github/testdata/fakegh` on the harness's path for its first engine test, and adds a test that a project limit of 1 keeps a second Ticket of that project in Queued.
- **`gh` runs on every save while the Factory is off.** Each such save runs `gh --version` and `gh auth status`, and `gh pr view` when its 30-second cache is stale. Recommended: plan 3's settings page waits about 400ms after the last change before it calls `Outbox::factory_save`.
- **`Agents.factory` after a reconnect.** The map keeps what pocketd last sent and is not cleared when the desktop connects again. Recommended: plan 3's settings page calls `Outbox::factory_config` each time it opens a project.
