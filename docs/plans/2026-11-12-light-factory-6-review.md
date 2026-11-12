# Light Factory 6: The Reviewer, the PR Review Gate and the Merge Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** A Ticket that passed Checks gets a read-only reviewer session, waits at the PR review gate with the reviewer's findings, and closes as Merged when you merge its PR from the footer bar or on github.com.

**Architecture:** pocketd gains one hook, `broker.Broker.Deny`, checked before `Allow` at the three places where a provider asks. `internal/factory/review.go` holds the review prompt, `ParseReview`, the read-only rule and the record `state/factory/<ticket>/review.json`. The record ties a review to one head of the PR and names the reviewer's session, so a restart finds both. The pure function `atPR` is `Next` for `Station: pr`. It reads `Facts` and returns the Ticket and the actions `startReview`, `repairReview`, `closeReviewer`, `markReady`, `await` and `close`. `Engine.step` runs them. The desktop adds the PR tab (`factory/pr.rs`), the `Decision::Pr` bar, and the merged card. The merge goes out as pocketd's existing `gh.pr.merge` verb.

**Toolset:**
- pocketd, one package (from `packages/pocketd`): `go test ./internal/factory -run TestParseReview -count=1` (swap the package and `-run`)
- pocketd e2e: `go test ./e2e -run TestAReviewer -count=1 -v` (once per machine first: `scripts/build-ghostty.sh` from the repository root)
- pocketd, everything: `go vet ./... && go test ./...`
- The red-team gate (from the repository root): `scripts/red-team.sh`
- Desktop, one test (from `packages/desktop`): `cargo test -p pocket factory::pr`
- Desktop, everything: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
- Capture (from the repository root): `.ui-review/fixture/capture.sh /tmp/lf6 factory-ticket-pr=factory-ticket-pr`

**Read first:**
- `docs/plans/2026-11-12-light-factory.md`: the decisions and the contracts. This plan implements its plan 6 row.
- `docs/designs/2026-11-12-light-factory.md` §5.7 (R1 to R9), E5, E10 and §9 (guardrail tests).
- `docs/designs/2026-11-12-light-factory-stories.md`: US-27, US-28, US-29, US-35, US-36.
- `packages/pocketd/internal/broker/broker.go`: `Allow`, `Auto`, `Ask`.
- `packages/pocketd/internal/daemon/daemon.go:259-300` (`decide`), `internal/codex/session.go:491-503` (`ask`), `internal/acp/request.go:131-156` (`ask`): the three places that call `Auto`.
- `packages/pocketd/internal/team/team.go:251-296` (`Allowed`) and `internal/team/plan.go:37-63` (`Reply`).
- `packages/pocketd/internal/github/service.go:355-418`: `after`, `Merge`, `Ready`.
- `packages/pocketd/e2e/fakeclaude/chat.go:159-278`: how the fake Claude takes a turn and asks `can_use_tool`.
- `packages/desktop/crates/pocket/src/git_ui/pull_request/card.rs`: `fact`, `pr_checks`, `pr_gate`, `pr_merge`.
- `.ui-review/prototypes/light-factory/app.js:347-354` (`prTab`) and `:392-396` (the bar at the PR gate).

---

## What plans 1 to 5 give this plan

This plan runs after plans 1 to 5 merge. The plans were written at the same time, so the code of plans 2 to 5 is not in the repository yet. The index fixes the names in the first table. The second table lists names that the index does not fix. Each one is this plan's guess. Where an earlier plan chose another name, use that name. The shape is what this plan needs.

| From | What (fixed by the index) | Used by |
|---|---|---|
| plan 1 | `proto.Ticket.Station`, `Gate`, `PR`, `Fix`, `proto.StationImplement`, `StationChecks`, `StationPR`, `StationMerged`, `GatePR`, `proto.FactoryConfig`, `proto.CapFactory` | PR 2, PR 3 |
| plan 3 | `factory.Facts`, `Session`, `Action`, `Next`, `ReviewState`, `Engine` with `Home`, `Board`, `Agents`, `Now`, `Launch`, `Engine.step`, `Engine.note`, `Engine.Detail`, `Engine.Recover`, `proto.FactoryTicket`, `proto.FactoryEvent`, the saved spec `state/factory/<ticket>/spec.md` | PR 2, PR 3 |
| plan 4 | `Engine.Never`, `Engine.Profiles.Get`, `proto.LaunchSpec.Profile`, the `Broker.Allow` composition in `serve.go` | Tasks 2.3, 2.5 |
| plan 5 | `Engine.GitHub`, `Engine.PRChanged`, the actions `sendFix` and `commitPush`, `Facts.PR` filled from `github.Service.PR` | Tasks 2.5, 3.2, 3.3 |
| plan 1 | Desktop: `agents::factory` with `Detail`, `Event::FactoryError`, `FACTORY_CAP` in `CAPS` | Task 4.1 |
| plans 2, 3 | Desktop: `factory/station.rs` (`Column`, `column`, `note`), `factory/tabs.rs` (`Tab::Pr`), `factory/bar.rs` (`Decision::Pr { gate, number }`, `decision`), `Agents::detail(ticket)`, `FactoryState` | PR 4 |

| Guess | Shape this plan needs | Used by |
|---|---|---|
| `Facts.Review ReviewState`, `Facts.Reviewer Session`, and the constants `ReviewNone`, `ReviewRunning`, `ReviewRepaired`, `ReviewDone`, `ReviewFailed` | plan 3's `Facts` has neither field. Task 2.2 adds both, with the type and its constants for `none`, `running`, `repaired`, `done`, `failed` | PR 2, PR 3 |
| `e.facts(t proto.Ticket, ev event) Facts` (plan 3) and `e.prs map[string]*proto.PR` (plan 5) | the one place that fills `Facts` before `Next`. Plan 5 sets `f.PR = e.prs[t.Worktree]` there | Task 2.5 |
| `e.act(t proto.Ticket, a Action) bool` (plan 3) | the one place that runs an `Action`, with `t` the Ticket after `Next`. `await` there calls `lead.SetAwaiting(true)` | Tasks 2.5, 3.3 |
| `e.post(event{ticket: id})`, `e.now()`, `readSpec(home, ticket)`, `working`, `asks`, `failed` (plan 3) | queue a step for one Ticket from any goroutine, the clock, the saved spec, and the three status helpers of `station.go` | Tasks 2.5, 3.2 |
| `checksNext(f Facts)`, the actions `sendFix` and `commitPush`, `Engine.PRChanged` refreshing `e.prs` and stepping the Ticket (plan 5) | Checks takes a conflicting PR as a failed check. `commitPush` commits, pushes and starts a check record for the new commit | Tasks 3.2, 3.3 |
| `newWorld(t *testing.T) *world` in `engine_test.go` (plan 3), `newRig(t, cfg) *rig` with `put`, `view`, `called`, `until`, `get` (plan 5) | `newWorld(t).e` is an `Engine` on a temp home whose `note` works. A rig is an engine with one live Ticket in a worktree with a remote, and the fake `gh` | Tasks 2.3, 3.3 |
| `Engine.note` is safe to call from any goroutine | `Engine.Deny` runs on a provider's goroutine | Task 2.3 |
| `factoryAtPR(t *testing.T, cfg proto.FactoryConfig) (h *Harness, o *Phone, gh *fakeGh, ticket proto.Ticket)` in `e2e/` | starts pocketd with the fake `gh` on `PATH`, saves `cfg` for `h.Repo`, starts a Ticket, and returns once the Ticket reads `Station: pr`. `gh.put(name, content)` writes an answer file and `gh.called(prefix)` reads the call log, as `internal/github/service_test.go:36-57` does | Tasks 2.6, 3.5 |
| `factory_id()` and `FACTORY: &str = "factory-"` in `agents.rs` | ids for Factory verbs, beside `board_id()` | Task 4.1 |
| `Desktop::factory_request_changes(&mut self, window, cx)` | opens the **Request changes** note that plan 3 built for the Plan gate | Task 4.4 |
| `Desktop::factory_primary(&mut self, window, cx) -> bool` | runs the bar's primary button on `⌘↵` | Task 4.4 |

## Decisions

- **`Broker.Denied` is the method that the call sites use.** `Deny` is the hook field of the index. `Denied` returns `("", false)` when the field is nil. `Auto` also returns false for an ask that `Deny` takes, so a call site that forgets `Denied` can never allow such an ask.
- **A denied ask does not stop the turn.** Claude gets `behavior: "deny"` with the reason and no `interrupt`. Codex gets `decline`. An ACP agent gets its reject option. The reviewer reads the refusal and still sends its findings. No status changes and nothing is published.
- **`internal/pi/ui.go` is not changed.** It calls `Broker.Ask` without `Auto`, and its asks are a confirmation or a question, never an edit, a write or a shell command. A pi or an omp cannot be a reviewer (see the next decision).
- **A reviewer profile whose provider never asks cannot review.** `providers.NeverAsks` is true for pi and omp. They run every tool without an Ask, so pocketd cannot hold them read-only. The engine does not start such a reviewer. The review fails with the note `The reviewer's agent can't be kept read-only`, and the gate opens.
- **The read-only rule.** `Engine.Deny` refuses an Ask of the reviewer whose `Detail.Kind` is `edit` or `write`, and a `shell` Ask whose command is not read-only. A command is read-only when `team.Allowed` accepts it for the list `git diff`, `git log`, `git show`, `git status`, `ls`, `cat`, `rg`, `grep`, it holds no backslash, and no word of it is `--output`, `--ext-diff`, `--pre` or `--hostname-bin`, alone or with `=value`. `team.Allowed` already refuses chaining, substitution and redirection. The four options are the ones with which `git` writes a file or runs a diff program, and with which `rg` runs a program. A longer option such as `--pretty` is not refused.
- **A read-only command of the reviewer is answered allow without an Ask.** The new `Engine.Reads` says so, and `serve.go` adds it to `Broker.Allow`. Without it, each `git diff` of an ACP reviewer would wait on you while the Ticket reads `working`.
- **Every other Ask of the reviewer is shown as any Ask.** Kinds `read`, `search`, `task` and `other` go to the human. The Ticket stays `working` (see Open questions).
- **The rule cannot catch a tool that the provider's own settings allow without asking pocketd.** The index accepts that risk for the never-allow list. It holds for the reviewer too. Codex adds its own guard: a chat with access `ask` runs in the `read-only` sandbox.
- **The review record.** `state/factory/<ticket>/review.json` holds `head`, `state`, `agent`, `since` and `findings`. `head` is `proto.PR.Head`, the oid of the PR's head commit. A record of another head reads as `none`, so a new head gets a new review (R7). `agent` is the reviewer's agent id. It is how the engine finds the reviewer after a restart. No wire field and no Ticket field is added.
- **`Engine.Deny` reads a map, not the disk.** `Engine.reviewers` maps a reviewer's agent id to its Ticket. `launchReviewer` fills it and `Engine.Recover` fills it again from the records.
- **The reviewer is launched off the engine's goroutine.** A launch waits for the CLI to attach. `startReview` writes the record with state `running` first, so the next `step` does not start a second reviewer. The launch then records the agent id, sends the prompt and queues a step.
- **The reviewer holds no slot of its own.** The Ticket is `working` while it reviews, so the Ticket's own slot covers it (R1).
- **The engine names the reviewer's session `Review · <ticket title>`.** It launches with no `Prompt` and sends the prompt itself, so the launcher's naming job does not name the session after the review prompt.
- **A broken reviewer does not block the Ticket.** One repair prompt is sent. A second bad reply, a failed turn, a launch that fails, or a reviewer that closed all end the review as `failed` with the note `The reviewer returned no findings block`. The gate opens. The reviewer is advisory (R4) and you still decide.
- **With `spec: false` the review prompt ends with the Ticket instead of the spec:** `Ticket: <title>`, a blank line, then the prompt.
- **`gh pr ready` is tried once.** `Next` returns `markReady` in the step that opens the gate, and only while the PR is a draft. A failure is noted in the Activity. The bar then shows `Draft`, and the Changes panel's PR card has **Ready for review**.
- **A lead that starts a turn at PR review sends the Ticket to Implement**, whether the gate is open or the reviewer still reads. A running reviewer is closed and its record removed. At Implement a Ticket that has a PR waits for the lead only. When the lead's turn ends, the engine commits, pushes and enters Checks. A turn that failed or was stopped ends the same way: what it changed is committed and checked. With no change the head is the same, the saved findings still apply, and the gate opens again at once.
- **A head that moves while the gate is open closes the gate.** The record no longer matches, so the reviewer starts again. This covers a push from outside too.
- **E5.** A PR whose `Mergeable` is `CONFLICTING` goes back to Checks from `atPR`. With an attempt left, `Ticket.Fix` goes up by 1 and the lead gets the base-conflict fix note. With none left, the Ticket is `failed` at Checks with `Why: The pull request conflicts with <base>`.
- **Merged here or elsewhere.** `github.Service.Merge` remembers the tree and the number that it merged. The engine asks `Service.MergedHere`. True gives the Activity `Merged` by `you`. False gives `Merged outside Anywhere` by `anywhere`. The memory is not saved, so a pocketd that restarts between the merge and the engine's step notes the merge as outside. A merge from the Changes panel's PR card runs the desktop's own `gh`, so it reads as outside too.
- **A PR that is closed without a merge leaves the Ticket at the gate.** The bar says `Closed on GitHub` and you close the Ticket.
- **The desktop refreshes its own PR cache when pocketd acks the merge.** The desktop runs its own `gh` and keeps `Desktop.prs`. `Requests` tracks `gh.pr.merge`. Its ack becomes `Event::GhMerged(tree)`, and its refusal becomes `Event::FactoryError`.
- **The merge method is the PR card's.** `store::prefs::Git` has no merge method. The bar reads and writes `Desktop.pr.track.method`, which the Changes panel's PR card uses, so both remember the same choice until the app quits.
- **The findings sit above the merge-gate rows** (R4, US-27), grouped critical, minor, nit. `ParseReview` caps them at 50, so the list is not virtualized.
- **`git::github::Pr` gains `additions`, `deletions` and `files`.** The bar, the PR tab and the merged card show the PR's size, and the working tree of a Factory ticket is clean after the engine's commit.

## PR 1: The broker's deny hook

**Scope:** `broker.Broker.Deny` and `Denied`, and the three call sites. Inert: nothing sets `Deny` until PR 2.
**Depends on:** nothing
**Done when:** `go vet ./... && go test ./...` and `scripts/red-team.sh` are green.

### Task 1.1: `Broker.Deny` and `Broker.Denied`

**Files:**
- Modify: `packages/pocketd/internal/broker/broker.go:34-60`
- Test: `packages/pocketd/internal/broker/broker_test.go`

**Context:** `Broker.Allow` is the hook that `Auto` asks. `Deny` is its twin and wins over it.

**Step 1: Write the failing tests** (append to `broker_test.go`)

```go
func TestOnlyAnAskTheDenyHookTakesIsRefusedWithItsReason(t *testing.T) {
	b := New(hub.New())
	write := proto.PermissionRequest{AgentID: "a1", ToolName: "Write", Detail: proto.ToolDetail{Kind: "write", Path: "a.go"}}
	if _, denied := b.Denied(write); denied {
		t.Fatal("denied with no hook")
	}
	b.Deny = func(req proto.PermissionRequest) (string, bool) {
		return "The reviewer is read-only", req.Detail.Kind == "write"
	}
	read := write
	read.Detail.Kind = "read"
	reason, denied := b.Denied(write)
	if _, other := b.Denied(read); !denied || reason != "The reviewer is read-only" || other {
		t.Fatalf("reason %q, denied %v, other %v", reason, denied, other)
	}
}

func TestAnAskTheDenyHookTakesIsNeverAllowedByTheAllowHook(t *testing.T) {
	b := New(hub.New())
	b.Allow = func(proto.PermissionRequest) bool { return true }
	b.Deny = func(proto.PermissionRequest) (string, bool) { return "no", true }
	if b.Auto(proto.PermissionRequest{AgentID: "a1", ToolName: "Bash", Detail: proto.ToolDetail{Kind: "shell", Command: "rm x"}}) {
		t.Fatal("Auto allowed an ask that Deny takes")
	}
}
```

**Step 2: Run and see them fail**

Run: `go test ./internal/broker -run 'TestOnlyAnAskTheDenyHook|TestAnAskTheDenyHook' -count=1`
Expected: FAIL to build with `b.Denied undefined (type *Broker has no field or method Denied)`.

**Step 3: Implement**

Before (`broker.go:37-39` and `:56-60`):

```go
	// Allow is what Auto asks: true answers an ask allow without showing it
	// to anyone. Set it before the first Ask.
	Allow func(proto.PermissionRequest) bool
```

```go
// Auto is whether Allow answers req allow by itself. Callers check it before
// Ask, and before showing the agent as needing you, which it then doesn't.
func (b *Broker) Auto(req proto.PermissionRequest) bool {
	return b.Allow != nil && b.Allow(req)
}
```

After:

```go
	// Allow is what Auto asks: true answers an ask allow without showing it
	// to anyone. Set it before the first Ask.
	Allow func(proto.PermissionRequest) bool
	// Deny is what Denied asks, and it wins over Allow: true answers an ask
	// deny with reason without showing it to anyone. Set it before the first Ask.
	Deny func(proto.PermissionRequest) (reason string, deny bool)
```

```go
// Denied is whether Deny answers req deny by itself, and why. Callers check it before Auto.
func (b *Broker) Denied(req proto.PermissionRequest) (string, bool) {
	if b.Deny == nil {
		return "", false
	}
	return b.Deny(req)
}

// Auto is whether Allow answers req allow by itself. Callers check it before
// Ask, and before showing the agent as needing you, which it then doesn't.
func (b *Broker) Auto(req proto.PermissionRequest) bool {
	if _, denied := b.Denied(req); denied {
		return false
	}
	return b.Allow != nil && b.Allow(req)
}
```

**Step 4: Run and see them pass**

Run: `go test ./internal/broker -count=1`
Expected: `ok  	pocketd/internal/broker`

### Task 1.2: Claude's `can_use_tool` answers a denied ask

**Files:**
- Modify: `packages/pocketd/internal/daemon/daemon.go:264-267`
- Test: `packages/pocketd/internal/daemon/claude_test.go` (after `TestAnAskTheBrokerAllowsByItselfNeverShowsTheAgentNeedingYou`, `:330`)

**Context:** `decide` serves a Claude hook and a chat's `can_use_tool` (`internal/daemon/stream.go:378`). A human's deny sets `Interrupt: true` and clears the agent. The hook's deny must do neither, because the turn goes on.

**Step 1: Write the failing test**

```go
func TestAnAskTheBrokerDeniesIsRefusedWithItsReasonAndNeverShown(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	var statuses []string
	d.Agents.OnStatus = func(_, _, _, to string) { statuses = append(statuses, to) }
	d.Broker.Allow = func(proto.PermissionRequest) bool { return true }
	d.Broker.Deny = func(req proto.PermissionRequest) (string, bool) {
		return "The reviewer is read-only", req.Detail.Kind == "write"
	}
	out := string(hookFrom(d, pr, `{"hook_event_name":"PermissionRequest","tool_name":"Write","tool_input":{"file_path":"/w/a.go","content":"x"}}`))
	if !strings.Contains(out, `"behavior":"deny"`) || !strings.Contains(out, "The reviewer is read-only") || strings.Contains(out, "interrupt") {
		t.Fatalf("out %s", out)
	}
	if slices.Contains(statuses, "needsYou") || len(d.Broker.Open()) != 0 {
		t.Fatalf("statuses %v, open %v", statuses, d.Broker.Open())
	}
}
```

**Step 2: Run and see it fail**

Run: `go test ./internal/daemon -run TestAnAskTheBrokerDenies -count=1`
Expected: FAIL with `out` holding `"behavior":"allow"`.

**Step 3: Implement**

Before (`daemon.go:264-267`):

```go
	req := proto.PermissionRequest{AgentID: ag.ID(), ToolName: in.ToolName, Detail: timeline.Detail(in.ToolName, in.ToolInput), Options: in.options(), Feedback: true}
	if d.Broker.Auto(req) {
		return &hookDecision{Behavior: "allow"}
	}
```

After:

```go
	req := proto.PermissionRequest{AgentID: ag.ID(), ToolName: in.ToolName, Detail: timeline.Detail(in.ToolName, in.ToolInput), Options: in.options(), Feedback: true}
	if reason, denied := d.Broker.Denied(req); denied {
		// No interrupt: the agent reads the reason and its turn goes on.
		return &hookDecision{Behavior: "deny", Message: reason}
	}
	if d.Broker.Auto(req) {
		return &hookDecision{Behavior: "allow"}
	}
```

**Step 4: Run and see it pass**

Run: `go test ./internal/daemon -run 'TestAnAskTheBroker' -count=1`
Expected: `ok  	pocketd/internal/daemon`

### Task 1.3: Codex's approval answers a denied ask

**Files:**
- Modify: `packages/pocketd/internal/codex/session.go:491-496`
- Test: `packages/pocketd/internal/codex/session_test.go` (after `TestPhoneAnswersApproval`, `:225`)

**Context:** Codex's reply to an approval holds only a decision. The reason is dropped. The review prompt already tells the reviewer why.

**Step 1: Write the failing test**

```go
func TestAnApprovalTheBrokerDeniesIsDeclinedAndNeverShown(t *testing.T) {
	srv, _, _, b := open(t, emptyThread)
	b.Allow = func(proto.PermissionRequest) bool { return true }
	b.Deny = func(req proto.PermissionRequest) (string, bool) {
		return "The reviewer is read-only", req.Detail.Kind == "shell"
	}
	srv.Push("item/commandExecution/requestApproval", 7, `{"threadId":"th1","turnId":"t2","itemId":"c2","startedAtMs":1,"command":"rm -rf build"}`)
	if got := srv.Reply("7"); got != `{"decision":"decline"}` {
		t.Fatal(got)
	}
	if open := b.Open(); len(open) != 0 {
		t.Fatalf("shown: %+v", open)
	}
}
```

**Step 2: Run and see it fail**

Run: `go test ./internal/codex -run TestAnApprovalTheBrokerDenies -count=1`
Expected: FAIL with `{"decision":"accept"}`.

**Step 3: Implement**

Before (`session.go:491-496`):

```go
func (s *Session) ask(id json.RawMessage, name string, detail proto.ToolDetail) {
	req := proto.PermissionRequest{AgentID: s.agentID, ToolName: name, Detail: detail}
	if s.broker.Auto(req) {
		s.c.Reply(id, map[string]string{"decision": "accept"})
		return
	}
```

After:

```go
func (s *Session) ask(id json.RawMessage, name string, detail proto.ToolDetail) {
	req := proto.PermissionRequest{AgentID: s.agentID, ToolName: name, Detail: detail}
	if _, denied := s.broker.Denied(req); denied {
		s.c.Reply(id, map[string]string{"decision": "decline"})
		return
	}
	if s.broker.Auto(req) {
		s.c.Reply(id, map[string]string{"decision": "accept"})
		return
	}
```

**Step 4: Run and see it pass**

Run: `go test ./internal/codex -count=1`
Expected: `ok  	pocketd/internal/codex`

### Task 1.4: An ACP agent's permission request answers a denied ask

**Files:**
- Modify: `packages/pocketd/internal/acp/request.go:137-145`
- Test: `packages/pocketd/internal/acp/session_test.go` (after `TestWhatTheBrokerAllowsIsNotAsked`, `:863`)

**Context:** `Session.ask` runs off the reader and has `reply` answer the agent. For a permission request, `reply` with `Decision: "deny"` picks the agent's reject option (`request.go:99-100`). `hermesPermission` (`session_test.go:735`) asks to run `rm -rf build`, and its reject option has the id `deny`.

**Step 1: Write the failing test**

```go
func TestWhatTheBrokerDeniesIsRejectedAndNotAsked(t *testing.T) {
	answered := make(chan struct{})
	s, ag, sk, b := start(t, opts{provider: "hermes", answer: script(hermesNew, func(a *agent, _ string) string {
		<-answered
		return endTurn
	})})
	b.Allow = func(proto.PermissionRequest) bool { return true }
	b.Deny = func(req proto.PermissionRequest) (string, bool) {
		return "The reviewer is read-only", req.Detail.Kind == "shell"
	}
	s.Prompt("go")
	ag.called("session/prompt", 1)
	ag.ask("5", "session/request_permission", hermesPermission)
	if got := ag.replied("5"); got != `{"outcome":{"optionId":"deny","outcome":"selected"}}` {
		t.Fatalf("answered %s", got)
	}
	close(answered)
	if got := sk.wait(t, "ended:false"); slices.Contains(got, "needsYou") {
		t.Fatalf("needed you: %v", got)
	}
	if open := b.Open(); len(open) != 0 {
		t.Fatalf("shown: %+v", open)
	}
}
```

**Step 2: Run and see it fail**

Run: `go test ./internal/acp -run TestWhatTheBrokerDenies -count=1`
Expected: FAIL with `answered {"outcome":{"optionId":"allow_once","outcome":"selected"}}`.

**Step 3: Implement**

Before (`request.go:137-145`):

```go
	go func() {
		defer cancel()
		if s.broker.Auto(req) {
			s.mu.Lock()
			delete(s.asks, key)
			s.mu.Unlock()
			reply(broker.Answer{Decision: "allow"})
			return
		}
```

After:

```go
	go func() {
		defer cancel()
		answer, settled := broker.Answer{Decision: "allow"}, false
		if reason, denied := s.broker.Denied(req); denied {
			answer, settled = broker.Answer{Decision: "deny", Message: reason}, true
		} else {
			settled = s.broker.Auto(req)
		}
		if settled {
			s.mu.Lock()
			delete(s.asks, key)
			s.mu.Unlock()
			reply(answer)
			return
		}
```

**Step 4: Run and see it pass**

Run: `go test ./internal/acp -count=1`
Expected: `ok  	pocketd/internal/acp`

### Task 1.5: Checks for PR 1

**Files:** none

**Step 1:** From `packages/pocketd`, run `go vet ./... && go test ./...`. Expected: every package `ok`.

**Step 2:** From the repository root, run `scripts/red-team.sh`. Expected: it passes as it does on `main`. The broker changed, so the index requires this run.

**Step 3:** Confirm that `internal/pi/ui.go` is unchanged: `git diff --stat main -- packages/pocketd/internal/pi` prints nothing.

## PR 2: The reviewer in the engine

**Scope:** `internal/factory/review.go`: the review prompt, `ParseReview`, the repair prompt, the record, the read-only rule, `Engine.Deny`, `Engine.Reads`, and the reviewer rows of `Next` at `Station: pr`. The fake Claude learns the review prompt. After this PR a Ticket at PR review gets a reviewer, its findings are saved and travel in `factory.ticket`, and the reviewer is closed. Inert until PR 3: the gate. The Ticket stays at `Station: pr` as it does after plan 5.
**Depends on:** PR 1, plans 1 to 5
**Done when:** `go vet ./... && go test ./...` and `scripts/red-team.sh` are green.

### Task 2.1: The review prompt, `ParseReview` and the repair prompt

**Files:**
- Create: `packages/pocketd/internal/factory/review.go`
- Test: `packages/pocketd/internal/factory/review_test.go`

**Context:** The index's section "Text the agents see" fixes the review prompt. `ParseReview` follows `ParseSpec` of plan 3: it reads the last block of the reply. Plan 1 adds `proto.Finding` to `internal/proto/factory.go` if it is not there yet (index, "Wire").

**Step 1: Write the failing tests** (`review_test.go`)

```go
package factory

import (
	"errors"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

func block(body string) string {
	return "Here is my review.\n<anywhere_factory_review>\n" + body + "\n</anywhere_factory_review>"
}

func TestParseReviewReadsTheFindingsOfTheLastBlock(t *testing.T) {
	text := block(`{"findings":[{"severity":"nit","message":"old"}]}`) + "\nAgain:\n" +
		block(`{"findings":[{"severity":"critical","file":"src/a.go","line":12,"message":" Leaks the handle "},{"severity":"minor","message":"Name it"}]}`)
	got, err := ParseReview(text)
	want := []proto.Finding{{Severity: "critical", File: "src/a.go", Line: 12, Message: "Leaks the handle"}, {Severity: "minor", Message: "Name it"}}
	if err != nil || len(got) != 2 || got[0] != want[0] || got[1] != want[1] {
		t.Fatalf("got %+v, %v", got, err)
	}
}

func TestParseReviewTakesAnEmptyListAsNothingFound(t *testing.T) {
	got, err := ParseReview(block(`{"findings":[]}`))
	if err != nil || got == nil || len(got) != 0 {
		t.Fatalf("got %#v, %v", got, err)
	}
}

func TestParseReviewRefusesWhatItWasNotAskedFor(t *testing.T) {
	many := `{"findings":[` + strings.TrimSuffix(strings.Repeat(`{"severity":"nit","message":"x"},`, 51), ",") + `]}`
	for name, text := range map[string]string{
		"no block":         "Looks good to me.",
		"not JSON":         block(`findings: none`),
		"no list":          block(`{}`),
		"unknown key":      block(`{"findings":[],"verdict":"approve"}`),
		"unknown severity": block(`{"findings":[{"severity":"blocker","message":"x"}]}`),
		"blank message":    block(`{"findings":[{"severity":"minor","message":"  "}]}`),
		"negative line":    block(`{"findings":[{"severity":"minor","line":-1,"message":"x"}]}`),
		"two objects":      block(`{"findings":[]} {"findings":[]}`),
		"too many":         block(many),
	} {
		if got, err := ParseReview(text); err == nil {
			t.Errorf("%s: took %+v", name, got)
		}
	}
}

func TestTheReviewPromptNamesTheBaseAndEndsWithTheSpecOrTheTicket(t *testing.T) {
	tk := proto.Ticket{Title: "Fold hunks", Prompt: "Fold the unchanged hunks."}
	with := ReviewRequest(tk, "main", "## Brief\nFold them.")
	if !strings.HasPrefix(with, "<anywhere_factory_review_request>\nReview the changes of this branch against main.") || !strings.HasSuffix(with, "</anywhere_factory_review_request>\n\nSpec:\n## Brief\nFold them.") {
		t.Fatalf("got %q", with)
	}
	without := ReviewRequest(tk, "main", "")
	if !strings.HasSuffix(without, "</anywhere_factory_review_request>\n\nTicket: Fold hunks\n\nFold the unchanged hunks.") {
		t.Fatalf("got %q", without)
	}
	if _, err := ParseReview(with); err != nil {
		t.Fatalf("the prompt's own example does not parse: %v", err)
	}
}

func TestTheRepairPromptSaysWhatWasWrong(t *testing.T) {
	if got := RepairReview(errors.New("no <anywhere_factory_review> block")); got != "Your review block can't be used: no <anywhere_factory_review> block. Send the corrected <anywhere_factory_review> block only." {
		t.Fatalf("got %q", got)
	}
}
```

**Step 2: Run and see them fail**

Run: `go test ./internal/factory -run 'TestParseReview|TestTheReviewPrompt|TestTheRepairPrompt' -count=1`
Expected: FAIL to build with `undefined: ParseReview`.

**Step 3: Implement** (`review.go`)

```go
package factory

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"slices"
	"strings"

	"pocketd/internal/proto"
)

const (
	reviewOpen  = "<anywhere_factory_review>"
	reviewClose = "</anywhere_factory_review>"
	// maxFindings bounds what one review may report.
	maxFindings = 50
)

var severities = []string{"critical", "minor", "nit"}

// ReviewRequest is the reviewer's first prompt. It ends with spec, or with the Ticket when its project writes no spec.
func ReviewRequest(t proto.Ticket, base, spec string) string {
	against := "Spec:\n" + spec
	if spec == "" {
		against = "Ticket: " + t.Title + "\n\n" + t.Prompt
	}
	return `<anywhere_factory_review_request>
Review the changes of this branch against ` + base + `. You can read and run read-only commands. Don't edit, write or delete anything.
Check the work against the spec below. Reply with a short note, then exactly one block:
<anywhere_factory_review>
{"findings":[{"severity":"critical","file":"path/from/repo/root","line":12,"message":"What is wrong and why it matters"}]}
</anywhere_factory_review>
- "severity" is critical, minor or nit. "file" and "line" are optional.
- An empty list means you found nothing.
</anywhere_factory_review_request>

` + against
}

// ParseReview reads the findings of the last <anywhere_factory_review> block of text. The block is one JSON
// object with the key "findings" and nothing else.
func ParseReview(text string) ([]proto.Finding, error) {
	end := strings.LastIndex(text, reviewClose)
	start := -1
	if end >= 0 {
		start = strings.LastIndex(text[:end], reviewOpen)
	}
	if start < 0 {
		return nil, errors.New("no " + reviewOpen + " block")
	}
	dec := json.NewDecoder(strings.NewReader(text[start+len(reviewOpen) : end]))
	dec.DisallowUnknownFields()
	var block struct {
		Findings *[]proto.Finding `json:"findings"`
	}
	if err := dec.Decode(&block); err != nil {
		return nil, fmt.Errorf("the block is not the JSON that was asked for (%v)", err)
	}
	if _, err := dec.Token(); err != io.EOF {
		return nil, errors.New("the block holds more than one JSON object")
	}
	if block.Findings == nil {
		return nil, errors.New(`the block has no "findings" list`)
	}
	findings := *block.Findings
	if len(findings) > maxFindings {
		return nil, fmt.Errorf("the block has %d findings, and the most is %d", len(findings), maxFindings)
	}
	for i := range findings {
		f := &findings[i]
		f.Message, f.File = strings.TrimSpace(f.Message), strings.TrimSpace(f.File)
		switch {
		case !slices.Contains(severities, f.Severity):
			return nil, fmt.Errorf("finding %d has the severity %q, which is not critical, minor or nit", i+1, f.Severity)
		case f.Message == "":
			return nil, fmt.Errorf("finding %d has no message", i+1)
		case f.Line < 0:
			return nil, fmt.Errorf("finding %d has a negative line", i+1)
		}
	}
	return findings, nil
}

// RepairReview is the one prompt that asks a reviewer for its block again.
func RepairReview(err error) string {
	return "Your review block can't be used: " + err.Error() + ". Send the corrected " + reviewOpen + " block only."
}
```

**Step 4: Run and see them pass**

Run: `go test ./internal/factory -run 'TestParseReview|TestTheReviewPrompt|TestTheRepairPrompt' -count=1`
Expected: `ok  	pocketd/internal/factory`

### Task 2.2: The review record and how a reply settles it

**Files:**
- Modify: `packages/pocketd/internal/factory/review.go`
- Test: `packages/pocketd/internal/factory/review_test.go`

**Context:** The index's `Facts` has `Review ReviewState` and `Reviewer Session`. Plan 3's `station.go` declares neither. Step 3 adds both fields to `Facts`, with the type and its constants. If an earlier plan already added them, keep that code. `atomicfile.Write(path, data, perm)` is in `internal/atomicfile`.

**Step 1: Write the failing tests** (append to `review_test.go`, and add `"os"` and `"path/filepath"` to the imports)

```go
func TestASavedReviewIsReadBackAndARecordOfAnotherHeadIsNoReview(t *testing.T) {
	home := t.TempDir()
	if got := readReview(home, "t1"); got.State != ReviewNone {
		t.Fatalf("got %+v with no file", got)
	}
	rec := reviewRecord{Head: "abc", State: ReviewDone, Agent: "r1", Since: 4, Findings: []proto.Finding{{Severity: "minor", Message: "Name it"}}}
	if err := writeReview(home, "t1", rec); err != nil {
		t.Fatal(err)
	}
	got := readReview(home, "t1")
	if got.Head != "abc" || got.State != ReviewDone || got.Agent != "r1" || got.Since != 4 || len(got.Findings) != 1 {
		t.Fatalf("got %+v", got)
	}
	if same, other, none := got.of("abc"), got.of("def"), got.of(""); same.State != ReviewDone || other.State != ReviewNone || none.State != ReviewNone {
		t.Fatalf("same %+v, other %+v, none %+v", same, other, none)
	}
	os.WriteFile(filepath.Join(home, "state", "factory", "t1", "review.json"), []byte("{"), 0o600)
	if got := readReview(home, "t1"); got.State != ReviewNone {
		t.Fatalf("got %+v from a broken file", got)
	}
}

func TestAReplySettlesAReviewOrAsksForOneRepair(t *testing.T) {
	good, bad := block(`{"findings":[{"severity":"nit","message":"x"}]}`), "Looks good."
	for name, c := range map[string]struct {
		state  ReviewState
		reply  string
		failed bool
		want   ReviewState
		n      int
	}{
		"a valid block":                {ReviewRunning, good, false, ReviewDone, 1},
		"a valid block after a repair": {ReviewRepaired, good, false, ReviewDone, 1},
		"a first bad reply":            {ReviewRunning, bad, false, ReviewRunning, 0},
		"a second bad reply":           {ReviewRepaired, bad, false, ReviewFailed, 0},
		"a turn that failed":           {ReviewRunning, bad, true, ReviewFailed, 0},
	} {
		state, findings, _ := verdict(c.state, c.reply, c.failed)
		if state != c.want || len(findings) != c.n {
			t.Errorf("%s: got %s with %d findings", name, state, len(findings))
		}
	}
}

func TestTheFindingsAreCountedBySeverityForTheActivity(t *testing.T) {
	f := func(s string) proto.Finding { return proto.Finding{Severity: s, Message: "x"} }
	if got := tally([]proto.Finding{f("minor"), f("critical"), f("minor")}); got != "1 critical · 2 minor" {
		t.Fatalf("got %q", got)
	}
	if got := tally(nil); got != "No findings" {
		t.Fatalf("got %q", got)
	}
}
```

**Step 2: Run and see them fail**

Run: `go test ./internal/factory -run 'TestASavedReview|TestAReplySettles|TestTheFindingsAreCounted' -count=1`
Expected: FAIL to build with `undefined: readReview`.

**Step 3: Implement**

In `station.go`, add two fields to `Facts` and the type:

```go
	Review   ReviewState
	Reviewer Session
```

```go
// ReviewState is how far the review of the PR's head got.
type ReviewState string

const (
	ReviewNone     ReviewState = "none"
	ReviewRunning  ReviewState = "running"
	ReviewRepaired ReviewState = "repaired"
	ReviewDone     ReviewState = "done"
	ReviewFailed   ReviewState = "failed"
)
```

Append to `review.go` (add `"os"`, `"path/filepath"`, `"strconv"` and `"pocketd/internal/atomicfile"` to the imports):

```go
// NoFindings is the note of a review that ended without a block that could be used.
const NoFindings = "The reviewer returned no findings block"

// reviewRecord is state/factory/<ticket>/review.json: the review of one head of the Ticket's PR.
type reviewRecord struct {
	// Head is the oid of the PR's head that the reviewer read.
	Head  string      `json:"head"`
	State ReviewState `json:"state"`
	// Agent is the reviewer's session. A restart finds it here.
	Agent string `json:"agent,omitempty"`
	// Since is the reviewer's timeline seq before the prompt that it must answer.
	Since    int64           `json:"since,omitempty"`
	Findings []proto.Finding `json:"findings"`
}

func reviewPath(home, ticket string) string {
	return filepath.Join(home, "state", "factory", ticket, "review.json")
}

// readReview is ticket's record. A missing or broken file is no review.
func readReview(home, ticket string) reviewRecord {
	var r reviewRecord
	raw, err := os.ReadFile(reviewPath(home, ticket))
	if err != nil || json.Unmarshal(raw, &r) != nil || r.State == "" {
		return reviewRecord{State: ReviewNone}
	}
	return r
}

func writeReview(home, ticket string, r reviewRecord) error {
	path := reviewPath(home, ticket)
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}
	raw, err := json.MarshalIndent(r, "", "\t")
	if err != nil {
		return err
	}
	return atomicfile.Write(path, raw, 0o600)
}

// of is r as it reads for head: the review of another head is no review of this one.
func (r reviewRecord) of(head string) reviewRecord {
	if head == "" || r.Head != head {
		return reviewRecord{State: ReviewNone}
	}
	return r
}

// verdict is how a reviewer's ended turn settles a review in state: its findings when reply holds a valid
// block, a failure after the one repair or a failed turn, else state again, which asks for the repair.
func verdict(state ReviewState, reply string, failed bool) (ReviewState, []proto.Finding, error) {
	findings, err := ParseReview(reply)
	switch {
	case err == nil:
		return ReviewDone, findings, nil
	case failed || state == ReviewRepaired:
		return ReviewFailed, nil, err
	}
	return state, nil, err
}

// tally counts findings by severity for the Activity: "1 critical · 2 minor".
func tally(findings []proto.Finding) string {
	var parts []string
	for _, s := range severities {
		if n := len(slices.DeleteFunc(slices.Clone(findings), func(f proto.Finding) bool { return f.Severity != s })); n > 0 {
			parts = append(parts, strconv.Itoa(n)+" "+s)
		}
	}
	if len(parts) == 0 {
		return "No findings"
	}
	return strings.Join(parts, " · ")
}
```

**Step 4: Run and see them pass**

Run: `go test ./internal/factory -run 'TestASavedReview|TestAReplySettles|TestTheFindingsAreCounted' -count=1`
Expected: `ok  	pocketd/internal/factory`

### Task 2.3: The read-only rule, `Engine.Deny` and `Engine.Reads`

**Files:**
- Modify: `packages/pocketd/internal/factory/review.go`
- Modify: `packages/pocketd/internal/factory/engine.go` (the `Engine` struct)
- Modify: `packages/pocketd/cmd/pocketd/serve.go` (where plan 4 composes `d.Broker.Allow`)
- Test: `packages/pocketd/internal/factory/review_test.go`

**Context:** `team.Allowed(allow []string, command string) bool` (`internal/team/team.go:254`) accepts one command that starts with a prefix of `allow`. It unwraps `zsh -lc '…'`, accepts ` 2>&1` and a pipe into `tail`, `head`, `grep` or `wc`, and refuses `;`, `&`, a backtick, `<`, `>`, a newline and `$(`. `internal/factory` already imports `internal/team` (the `Engine.Teams` field). `newWorld(t).e` is plan 3's test engine (see the guess table). The test below writes to the Activity through `Engine.note` of plan 3.

**Step 1: Write the failing tests** (append to `review_test.go`, and add `"slices"`, `"pocketd/internal/broker"` and `"pocketd/internal/hub"` to the imports)

```go
func TestOnlyACommandThatReadsIsReadOnly(t *testing.T) {
	for _, c := range []string{"git diff main...HEAD", "git log --oneline -20", "git show HEAD:src/a.go", "git status", "ls -la src", "cat src/a.go", "rg -n TODO src", "grep -rn TODO src", "git diff main | head -50", "/bin/zsh -lc 'git diff main'", "git log --pretty=oneline -5"} {
		if !ReadOnly(c) {
			t.Errorf("%q is refused", c)
		}
	}
	for _, c := range []string{"rm -rf build", "git push", "git commit -m x", "git diff --output=x.patch", `git diff "--output=x"`, "git log --output x", "git diff --ext-diff", "rg --pre ./x.sh TODO", "rg --pre=./x.sh TODO", "rg --hostname-bin ./x.sh TODO", "ls; rm x", "cat a > b", "cat a | tee b", "git diff $(rm x)", `git diff \--output=x`, "gh pr review --approve", "git", ""} {
		if ReadOnly(c) {
			t.Errorf("%q is taken as read-only", c)
		}
	}
}

func TestNoReviewerAskThatEditsWritesOrChangesStateIsEverAllowed(t *testing.T) {
	e := newWorld(t).e
	e.setReviewer("r1", "t1")
	b := broker.New(hub.New())
	b.Deny = e.Deny
	b.Allow = func(proto.PermissionRequest) bool { return true }
	for _, d := range []proto.ToolDetail{
		{Kind: "edit", Path: "src/a.go"},
		{Kind: "write", Path: "src/new.go"},
		{Kind: "shell", Command: "rm -rf build"},
		{Kind: "shell", Command: "git push origin HEAD"},
		{Kind: "shell", Command: "gh pr review --approve"},
		{Kind: "shell", Command: "git diff --output=x.patch"},
	} {
		req := proto.PermissionRequest{AgentID: "r1", Detail: d}
		reason, denied := b.Denied(req)
		if !denied || reason != ReadOnlyReason || b.Auto(req) || e.Reads(req) {
			t.Errorf("%+v: denied %v, reason %q, auto %v, reads %v", d, denied, reason, b.Auto(req), e.Reads(req))
		}
	}
	events, _ := ReadActivity(e.Home, "t1")
	noted := func(text string) bool {
		return slices.ContainsFunc(events, func(ev proto.FactoryEvent) bool { return ev.Actor == "reviewer" && ev.Kind == "denied" && ev.Text == text })
	}
	if !noted("Refused: rm -rf build") || !noted("Refused to write src/new.go") {
		t.Fatalf("activity %+v", events)
	}
}

func TestAReviewerReadsWithoutAnAskAndEveryOtherSessionIsLeftAlone(t *testing.T) {
	e := newWorld(t).e
	e.setReviewer("r1", "t1")
	diff := proto.PermissionRequest{AgentID: "r1", Detail: proto.ToolDetail{Kind: "shell", Command: "git diff main"}}
	if _, denied := e.Deny(diff); denied || !e.Reads(diff) {
		t.Fatal("a reviewer's git diff is not read without an ask")
	}
	other := proto.PermissionRequest{AgentID: "r1", Detail: proto.ToolDetail{Kind: "other"}}
	if _, denied := e.Deny(other); denied || e.Reads(other) {
		t.Fatal("an ask that is no edit, write or shell command is not left to the human")
	}
	lead := proto.PermissionRequest{AgentID: "lead", Detail: proto.ToolDetail{Kind: "write", Path: "a.go"}}
	if _, denied := e.Deny(lead); denied || e.Reads(lead) {
		t.Fatal("a session that is no reviewer is answered")
	}
	e.dropReviewer("r1")
	if _, denied := e.Deny(proto.PermissionRequest{AgentID: "r1", Detail: proto.ToolDetail{Kind: "write"}}); denied {
		t.Fatal("a closed reviewer is still held")
	}
	var off *Engine
	if _, denied := off.Deny(lead); denied || off.Reads(diff) {
		t.Fatal("a nil engine answers")
	}
}
```

**Step 2: Run and see them fail**

Run: `go test ./internal/factory -run 'TestOnlyACommandThatReads|TestNoReviewerAsk|TestAReviewerReads' -count=1`
Expected: FAIL to build with `undefined: ReadOnly`.

**Step 3: Implement**

Add two fields to the `Engine` struct in `engine.go`, after the exported fields (add `"sync"` to its imports if it is missing):

```go
	reviewMu sync.Mutex
	// reviewers is each open reviewer's agent id and its Ticket: whose asks Deny answers.
	reviewers map[string]string
```

Append to `review.go` (add `"pocketd/internal/team"` to the imports):

```go
// ReadOnlyReason is what a reviewer is told when pocketd refuses its ask.
const ReadOnlyReason = "The reviewer is read-only. Report what you found instead of changing it."

// reads are the commands a reviewer runs without an Ask.
var reads = []string{"git diff", "git log", "git show", "git status", "ls", "cat", "rg", "grep"}

// escapes are the options with which a command of reads writes a file or runs a program.
var escapes = []string{"--output", "--ext-diff", "--pre", "--hostname-bin"}

// ReadOnly is whether a reviewer may run command: one command of reads by team.Allowed's rule, with no
// backslash and no option of escapes, alone or with a value.
func ReadOnly(command string) bool {
	if strings.Contains(command, `\`) || !team.Allowed(reads, command) {
		return false
	}
	for _, word := range strings.Fields(command) {
		option, _, _ := strings.Cut(strings.Trim(word, `'"`), "=")
		if slices.Contains(escapes, option) {
			return false
		}
	}
	return true
}

// refuses is whether pocketd answers a reviewer's ask for d deny by itself.
func refuses(d proto.ToolDetail) bool {
	switch d.Kind {
	case "edit", "write":
		return true
	case "shell":
		return !ReadOnly(d.Command)
	}
	return false
}

func (e *Engine) setReviewer(agentID, ticket string) {
	e.reviewMu.Lock()
	defer e.reviewMu.Unlock()
	if e.reviewers == nil {
		e.reviewers = map[string]string{}
	}
	e.reviewers[agentID] = ticket
}

func (e *Engine) dropReviewer(agentID string) {
	e.reviewMu.Lock()
	defer e.reviewMu.Unlock()
	delete(e.reviewers, agentID)
}

func (e *Engine) reviewerOf(agentID string) (ticket string, ok bool) {
	e.reviewMu.Lock()
	defer e.reviewMu.Unlock()
	ticket, ok = e.reviewers[agentID]
	return ticket, ok
}

// Deny is the Broker.Deny hook: a reviewer's ask to edit, to write, or to run a command that is not
// read-only is refused and noted.
func (e *Engine) Deny(req proto.PermissionRequest) (string, bool) {
	if e == nil {
		return "", false
	}
	ticket, ok := e.reviewerOf(req.AgentID)
	if !ok || !refuses(req.Detail) {
		return "", false
	}
	text := "Refused: " + req.Detail.Command
	if req.Detail.Kind != "shell" {
		text = "Refused to " + req.Detail.Kind + " " + req.Detail.Path
	}
	e.note(ticket, proto.FactoryEvent{At: e.now().UnixMilli(), Station: proto.StationPR, Actor: "reviewer", Kind: "denied", Text: text})
	return ReadOnlyReason, true
}

// Reads is whether req is a reviewer's read-only command, which Broker.Allow answers without an Ask.
func (e *Engine) Reads(req proto.PermissionRequest) bool {
	if e == nil {
		return false
	}
	_, ok := e.reviewerOf(req.AgentID)
	return ok && req.Detail.Kind == "shell" && ReadOnly(req.Detail.Command)
}
```

In `serve.go`, plan 4 left this line (index, "Hooks and fields that other packages gain"):

```go
	d.Broker.Allow = func(req proto.PermissionRequest) bool { return !fac.Never(req) && teams.Allow(req) }
```

Replace it with:

```go
	d.Broker.Allow = func(req proto.PermissionRequest) bool {
		return !fac.Never(req) && (teams.Allow(req) || fac.Reads(req))
	}
	d.Broker.Deny = fac.Deny
```

`fac` is the `*factory.Engine` of `serve.go`. Both methods accept a nil engine.

**Step 4: Run and see them pass**

Run: `go test ./internal/factory -run 'TestOnlyACommandThatReads|TestNoReviewerAsk|TestAReviewerReads' -count=1 && go build ./cmd/pocketd`
Expected: `ok  	pocketd/internal/factory`, and the build prints nothing.

### Task 2.4: `Next` at PR review: the reviewer

**Files:**
- Modify: `packages/pocketd/internal/factory/review.go`
- Modify: `packages/pocketd/internal/factory/station.go` (`Next`)
- Test: `packages/pocketd/internal/factory/review_test.go`

**Context:** `Next(f Facts) (proto.Ticket, []Action)` is the one pure decision (index, `station.go`). After plan 5 its case for `proto.StationPR` returns the Ticket unchanged. This task gives that case to `atPR`. `Facts.Reviewer` is a `Session{Present bool; Turn string}`. The engine reports `Turn: "working"` until the reviewer's turn has reached the timeline and ended (Task 2.5), so `atPR` never repairs a reviewer that has not answered yet.

The states at `Station: pr` after this PR. The first row that matches wins. PR 3 adds the rows of the gate, the merge, the rework and the conflict.

| # | Facts | Ticket after | Actions |
|---|---|---|---|
| 1 | `PR == nil` | unchanged | none |
| 2 | `Config.Reviewer` is false | unchanged | none (PR 3 opens the gate) |
| 3 | `Review == none` | `Status: working`, `Gate: ""`, `Why: ""` | `startReview` |
| 4 | `Review == running`, the reviewer's turn ended (`Turn` is `idle` or `done`) | `Status: working` | `repairReview` |
| 5 | `Review` is `running` or `repaired`, any other reviewer state | `Status: working` | none (wait) |
| 6 | `Review` is `done` or `failed`, `Reviewer.Present` | unchanged | `closeReviewer` |
| 7 | `Review` is `done` or `failed`, no reviewer | unchanged | none |

**Step 1: Write the failing tests** (append to `review_test.go`)

```go
// atGate is the Facts of a Ticket that passed Checks: an open draft PR on head abc, the lead idle.
func atGate(edit func(*Facts)) Facts {
	cfg := proto.DefaultFactoryConfig()
	cfg.Enabled = true
	f := Facts{
		Ticket: proto.Ticket{ID: "t1", Title: "Fold hunks", State: proto.TicketLive, Station: proto.StationPR, Status: proto.TicketWorking, AgentID: "lead", PR: 7},
		Config: cfg,
		Lead:   Session{Present: true, Turn: "idle"},
		PR:     &proto.PR{Number: 7, State: "OPEN", Draft: true, Base: "main", Head: "abc", Mergeable: "MERGEABLE"},
		Review: ReviewNone,
	}
	if edit != nil {
		edit(&f)
	}
	return f
}

func kinds(acts []Action) string {
	var out []string
	for _, a := range acts {
		out = append(out, a.Kind)
	}
	return strings.Join(out, ",")
}

func TestATicketAtPRReviewGetsAReviewerThatIsRepairedOnceAndThenClosed(t *testing.T) {
	working, ended := Session{Present: true, Turn: "working"}, Session{Present: true, Turn: "done"}
	for name, c := range map[string]struct {
		edit func(*Facts)
		want string
	}{
		"no review yet starts the reviewer":       {nil, "startReview"},
		"a reviewer at work is waited for":        {func(f *Facts) { f.Review, f.Reviewer = ReviewRunning, working }, ""},
		"a reviewer being launched is waited for": {func(f *Facts) { f.Review = ReviewRunning }, ""},
		"a reviewer that asks you is waited for":  {func(f *Facts) { f.Review, f.Reviewer = ReviewRunning, Session{Present: true, Turn: "needsYou"} }, ""},
		"a first reply without a block":           {func(f *Facts) { f.Review, f.Reviewer = ReviewRunning, ended }, "repairReview"},
		"a repair under way is waited for":        {func(f *Facts) { f.Review, f.Reviewer = ReviewRepaired, working }, ""},
		"a finished review closes the reviewer":   {func(f *Facts) { f.Review, f.Reviewer = ReviewDone, ended }, "closeReviewer"},
		"a failed review closes the reviewer":     {func(f *Facts) { f.Review, f.Reviewer = ReviewFailed, ended }, "closeReviewer"},
		"no pull request yet":                     {func(f *Facts) { f.PR = nil }, ""},
	} {
		got, acts := Next(atGate(c.edit))
		if kinds(acts) != c.want || got.Station != proto.StationPR || got.Status != proto.TicketWorking {
			t.Errorf("%s: got %q, station %s, status %s", name, kinds(acts), got.Station, got.Status)
		}
	}
}
```

**Step 2: Run and see it fail**

Run: `go test ./internal/factory -run TestATicketAtPRReviewGetsAReviewer -count=1`
Expected: FAIL with `no review yet starts the reviewer: got ""`.

**Step 3: Implement**

Append to `review.go`:

```go
// ended is whether s is a session whose turn is over.
func ended(s Session) bool {
	return s.Present && (s.Turn == "idle" || s.Turn == "done")
}

// atPR is Next for a Ticket at PR review.
func atPR(f Facts) (proto.Ticket, []Action) {
	t := f.Ticket
	if f.PR == nil || !f.Config.Reviewer {
		return t, nil
	}
	switch f.Review {
	case ReviewNone:
		t.Status, t.Gate, t.Why = proto.TicketWorking, "", ""
		return t, []Action{{Kind: "startReview"}}
	case ReviewRunning, ReviewRepaired:
		t.Status = proto.TicketWorking
		if f.Review == ReviewRunning && ended(f.Reviewer) {
			return t, []Action{{Kind: "repairReview"}}
		}
		return t, nil
	}
	if f.Reviewer.Present {
		return t, []Action{{Kind: "closeReviewer"}}
	}
	return t, nil
}
```

In `station.go`, in `Next`'s switch on the Ticket's station, make the case for PR review read:

```go
	case proto.StationPR:
		return atPR(f)
```

**Step 4: Run and see it pass**

Run: `go test ./internal/factory -count=1`
Expected: `ok  	pocketd/internal/factory`

### Task 2.5: The engine starts, repairs and closes the reviewer

**Files:**
- Modify: `packages/pocketd/internal/factory/review.go`
- Modify: `packages/pocketd/internal/factory/engine.go` (the facts and the action switch of `Engine.step`, and `Engine.Detail`)
- Modify: `packages/pocketd/internal/factory/recover.go` (`Engine.Recover`)

**Context:** This task is glue over the plain functions of Tasks 2.1 to 2.4. The e2e test of Task 2.6 covers it, as the index's `Engine.step` is covered. Names used:
- `Engine.Launch(key, requestID string, s proto.LaunchSpec, created func(cwd string)) launch.Result` (index). `launch.Result` has `AgentID` and `Err *launch.Failure`, and `Failure` has `Message` (`internal/launch/launch.go:50-64`). A `Checkout.Worktree` launches in an existing worktree.
- `proto.LaunchSpec.Profile` (plan 4) carries the profile id. The launcher writes the system prompt and copies the skills from it.
- `Engine.Profiles.Get(id) (proto.Profile, bool)` (plan 4). `proto.Profile` embeds `ProfileInfo` with `Provider` and `Model`.
- `providers.NeverAsks(id string) bool` (`internal/providers/providers.go:92`).
- `agent.Registry.Get(id) (*agent.Agent, error)`, `Agent.Turn()`, `Agent.Driver()`, `Agent.ClaimNaming()`, `Agent.SetNamed(claim, title)`, `Agent.SetAwaiting(bool)`, `Agent.Timeline.State()` and `Agent.Timeline.Page(since, limit)` (`internal/agent/agent.go`, `internal/timeline/timeline.go:131,466`).
- `team.Reply(items) (text string, ended, failed bool)` reads the last turn of a page (`internal/team/plan.go:39`).
- `e.post(event{ticket: id})` (plan 3) queues a step for one Ticket from any goroutine.
- `readSpec(home, ticket)` (plan 3) is the saved spec, or `""`.
- `e.prs[tree]` (plan 5) is the newest PR of a worktree. Only the engine's goroutine reads it, and `e.act` runs there.

**Step 1: Implement the engine's side** (append to `review.go`, and add `"log"`, `"pocketd/internal/agent"` and `"pocketd/internal/providers"` to the imports)

```go
// reviewItems bounds the items a reviewer's turn is read from.
const reviewItems = 500

// reviewer is the open session with id, or nil.
func (e *Engine) reviewer(id string) *agent.Agent {
	if id == "" {
		return nil
	}
	a, err := e.Agents.Get(id)
	if err != nil || a.Turn() == "closed" {
		return nil
	}
	return a
}

// reviewFacts is how far the review of pr's head has got, and the reviewer's session. It settles a turn that
// ended: a valid block is saved as the findings, and a second bad reply fails the review. A turn that has
// not reached the timeline yet reads as working, so Next never repairs a reviewer that has not answered.
func (e *Engine) reviewFacts(t proto.Ticket, pr *proto.PR) (ReviewState, Session) {
	if pr == nil {
		return ReviewNone, Session{}
	}
	rec := readReview(e.Home, t.ID).of(pr.Head)
	a := e.reviewer(rec.Agent)
	if rec.State != ReviewRunning && rec.State != ReviewRepaired {
		return rec.State, Session{Present: a != nil}
	}
	if rec.Agent == "" {
		// launchReviewer records the agent once the session exists.
		return rec.State, Session{}
	}
	if a == nil {
		e.settle(t, rec, ReviewFailed, nil)
		return ReviewFailed, Session{}
	}
	items, _ := a.Timeline.Page(rec.Since, reviewItems)
	reply, over, failed := team.Reply(items)
	if !over {
		return rec.State, Session{Present: true, Turn: "working"}
	}
	state, findings, _ := verdict(rec.State, reply, failed)
	if state != rec.State {
		e.settle(t, rec, state, findings)
	}
	return state, Session{Present: true, Turn: "done"}
}

// settle saves how rec's review ended and notes it.
func (e *Engine) settle(t proto.Ticket, rec reviewRecord, state ReviewState, findings []proto.Finding) {
	rec.State, rec.Findings = state, findings
	if err := writeReview(e.Home, t.ID, rec); err != nil {
		log.Printf("factory: saving the review of %s: %v", t.ID, err)
	}
	text := tally(findings)
	if state == ReviewFailed {
		text = NoFindings
	}
	e.note(t.ID, proto.FactoryEvent{At: e.now().UnixMilli(), Station: proto.StationPR, Actor: "reviewer", Kind: "finding", Text: text})
}

// startReview records that the review of pr's head began, then launches the reviewer off the engine's
// goroutine: a launch waits for the CLI to attach.
func (e *Engine) startReview(t proto.Ticket, cfg proto.FactoryConfig, pr *proto.PR) {
	// A reviewer of an older head may still be open.
	e.closeReviewer(t)
	rec := reviewRecord{Head: pr.Head, State: ReviewRunning, Findings: []proto.Finding{}}
	if err := writeReview(e.Home, t.ID, rec); err != nil {
		log.Printf("factory: starting the review of %s: %v", t.ID, err)
		return
	}
	if lead, err := e.Agents.Get(t.AgentID); err == nil {
		lead.SetAwaiting(false)
	}
	go e.launchReviewer(t, cfg, pr.Base, rec)
}

func (e *Engine) launchReviewer(t proto.Ticket, cfg proto.FactoryConfig, base string, rec reviewRecord) {
	fail := func(why string) {
		rec.State = ReviewFailed
		writeReview(e.Home, t.ID, rec)
		e.note(t.ID, proto.FactoryEvent{At: e.now().UnixMilli(), Station: proto.StationPR, Actor: "anywhere", Kind: "failed", Text: why})
		e.post(event{ticket: t.ID})
	}
	p, ok := e.Profiles.Get(cfg.Review)
	switch {
	case !ok:
		fail("The reviewer profile " + cfg.Review + " is gone")
		return
	case providers.NeverAsks(p.Provider):
		fail("The reviewer's agent can't be kept read-only")
		return
	}
	r := e.Launch("factory:"+t.ID, "review-"+t.ID+"-"+strconv.FormatInt(e.now().UnixNano(), 36), proto.LaunchSpec{
		Project: t.Folder, Checkout: proto.Checkout{Worktree: t.Worktree}, Provider: p.Provider, Model: p.Model,
		Access: "ask", Chat: true, Parent: t.AgentID, Profile: cfg.Review,
	}, func(string) {})
	if r.Err != nil {
		fail("The reviewer didn't start: " + r.Err.Message)
		return
	}
	a, err := e.Agents.Get(r.AgentID)
	if err != nil {
		fail("The reviewer didn't start: " + err.Error())
		return
	}
	if now := readReview(e.Home, t.ID); now.Head != rec.Head || now.State != ReviewRunning {
		// The lead took the Ticket back to Implement meanwhile.
		a.Driver().Close()
		return
	}
	e.setReviewer(r.AgentID, t.ID)
	if claim, ok := a.ClaimNaming(); ok {
		a.SetNamed(claim, "Review · "+t.Title)
	}
	rec.Agent = r.AgentID
	_, rec.Since = a.Timeline.State()
	if err := writeReview(e.Home, t.ID, rec); err != nil {
		fail("The review couldn't be saved: " + err.Error())
		return
	}
	if err := a.Driver().Prompt(ReviewRequest(t, base, readSpec(e.Home, t.ID))); err != nil {
		fail("The reviewer didn't take its prompt: " + err.Error())
		return
	}
	e.post(event{ticket: t.ID})
}

// repairReview sends the one repair prompt to a reviewer whose reply held no block that could be used.
func (e *Engine) repairReview(t proto.Ticket, pr *proto.PR) {
	rec := readReview(e.Home, t.ID).of(pr.Head)
	a := e.reviewer(rec.Agent)
	if rec.State != ReviewRunning || a == nil {
		return
	}
	items, _ := a.Timeline.Page(rec.Since, reviewItems)
	reply, _, _ := team.Reply(items)
	_, err := ParseReview(reply)
	if err == nil {
		return
	}
	rec.State = ReviewRepaired
	_, rec.Since = a.Timeline.State()
	if writeReview(e.Home, t.ID, rec) != nil || a.Driver().Prompt(RepairReview(err)) != nil {
		e.settle(t, rec, ReviewFailed, nil)
	}
}

// closeReviewer closes t's reviewer. A review that it had not finished is forgotten, so the next head, or
// the same one, is reviewed from the start.
func (e *Engine) closeReviewer(t proto.Ticket) {
	rec := readReview(e.Home, t.ID)
	if a := e.reviewer(rec.Agent); a != nil {
		a.Driver().Close()
	}
	e.dropReviewer(rec.Agent)
	if rec.State == ReviewRunning || rec.State == ReviewRepaired {
		os.Remove(reviewPath(e.Home, t.ID))
	}
}

// recoverReview gives Deny its reviewer back after a restart, and forgets a review whose launch the restart cut.
func (e *Engine) recoverReview(t proto.Ticket) {
	rec := readReview(e.Home, t.ID)
	running := rec.State == ReviewRunning || rec.State == ReviewRepaired
	switch {
	case e.reviewer(rec.Agent) != nil:
		e.setReviewer(rec.Agent, t.ID)
	case running && rec.Agent == "":
		os.Remove(reviewPath(e.Home, t.ID))
	}
}
```

**Step 2: Wire it into the engine**

In `e.facts` of `engine.go`, after plan 5's line `f.PR = e.prs[t.Worktree]`, add:

```go
	if t.Station == proto.StationPR {
		f.Review, f.Reviewer = e.reviewFacts(t, f.PR)
	}
```

In `e.act`, add the arms:

```go
	case "startReview":
		if pr := e.prs[t.Worktree]; pr != nil {
			e.startReview(t, e.Configs.Get(t.Folder), pr)
		}
	case "repairReview":
		if pr := e.prs[t.Worktree]; pr != nil {
			e.repairReview(t, pr)
		}
	case "closeReviewer":
		e.closeReviewer(t)
```

In `Engine.Detail`, set the findings on the `proto.FactoryTicket` before it is returned (`out` is that value):

```go
	out.Findings = readReview(e.Home, ticket).Findings
```

In `Engine.Recover` (`recover.go`), in its loop over the Board's live Tickets, add:

```go
	if t.Station == proto.StationPR {
		e.recoverReview(t)
	}
```

**Step 3: Build**

Run: `go vet ./internal/factory && go test ./internal/factory -count=1`
Expected: `ok  	pocketd/internal/factory`

### Task 2.6: End to end: a reviewer that tries to write is refused and its findings are saved

**Files:**
- Modify: `packages/pocketd/e2e/fakeclaude/chat.go:187-210` (the turn switch) and after `assignment` (`:278`)
- Create: `packages/pocketd/e2e/factory_review_test.go`

**Context:** The fake Claude echoes a prompt as `echo: <prompt>`. The review prompt holds an example block, so the echo is a valid review with one critical finding. This task makes the fake reviewer also try to write a file first, as a reviewer that ignores its rules would. `factoryAtPR` is plan 5's path to `Station: pr` as a helper (see the guess table). If plan 5 wrote that path inside one test, move it into the helper first. That is a refactor with no change of behaviour. `Harness.eventually(what, ok)` is at `e2e/harness_test.go:219`. The profile `claude-sonnet` makes the reviewer a fake Claude.

**Step 1: Write the failing test** (`factory_review_test.go`)

```go
package e2e

import (
	"encoding/json"
	"os"
	"path/filepath"
	"slices"
	"testing"

	"pocketd/internal/proto"
)

// factoryDetail asks for ticket's detail as the desktop does.
func factoryDetail(o *Phone, ticket string) proto.FactoryTicket {
	o.t.Helper()
	o.Send(map[string]any{"type": "factory.ticket", "id": "detail", "ticketId": ticket})
	m := o.WaitFor("the ticket's detail", func(m Message) bool { return m.Type == "factory.ticket" && m.ID == "detail" })
	var reply struct {
		Ticket proto.FactoryTicket `json:"ticket"`
	}
	json.Unmarshal([]byte(m.Raw), &reply)
	return reply.Ticket
}

func reviewed() proto.FactoryConfig {
	cfg := proto.DefaultFactoryConfig()
	cfg.Enabled, cfg.Review = true, "claude-sonnet"
	return cfg
}

func TestAReviewerThatTriesToWriteIsRefusedAndItsFindingsAreSaved(t *testing.T) {
	h, o, _, ticket := factoryAtPR(t, reviewed())
	var d proto.FactoryTicket
	h.eventually("the reviewer's findings", func() bool {
		d = factoryDetail(o, ticket.ID)
		return len(d.Findings) > 0
	})
	if f := d.Findings[0]; f.Severity != "critical" || f.File != "path/from/repo/root" || f.Line != 12 {
		t.Fatalf("got %+v", d.Findings)
	}
	said := func(actor, kind string) bool {
		return slices.ContainsFunc(d.Activity, func(ev proto.FactoryEvent) bool { return ev.Actor == actor && ev.Kind == kind })
	}
	if !said("reviewer", "denied") || !said("reviewer", "finding") {
		t.Fatalf("activity %+v", d.Activity)
	}
	if _, err := os.Stat(filepath.Join(ticket.Worktree, "reviewer-wrote.txt")); !os.IsNotExist(err) {
		t.Fatalf("the reviewer wrote a file: %v", err)
	}
}
```

**Step 2: Run and see it fail**

Run: `go test ./e2e -run TestAReviewerThatTriesToWrite -count=1 -v`
Expected: FAIL with `activity` holding no `denied` event, because the fake reviewer does not try to write yet.

**Step 3: Teach the fake Claude the review prompt**

In `chat.go`, add a case to the turn switch, before `case strings.HasPrefix(text, "run "):` (`:189`):

```go
	case strings.HasPrefix(text, "<anywhere_factory_review_request>"):
		c.review(model, text)
```

After `assignment` (`:278`), add:

```go
// review is a reviewer's turn: it asks to write a file, as a reviewer that ignores its rules would, then
// echoes the prompt, whose example block is its findings.
func (c *chatter) review(model, text string) {
	c.next++
	id := fmt.Sprintf("toolu_%d", c.next)
	path := filepath.Join(c.s.cwd, "reviewer-wrote.txt")
	input := map[string]any{"file_path": path, "content": "x"}
	c.s.write(assistant(map[string]any{"type": "tool_use", "id": id, "name": "Write", "input": input}))
	d := c.ask(map[string]any{"subtype": "can_use_tool", "tool_name": "Write", "input": input, "tool_use_id": id})
	note("can_use_tool: %v", d["behavior"])
	if d["behavior"] == "allow" {
		os.WriteFile(path, []byte("x\n"), 0o644)
		c.s.write(toolResult(id, "wrote", false))
	} else {
		msg, _ := d["message"].(string)
		c.s.write(toolResult(id, msg, true))
	}
	c.reply(model, text)
}
```

**Step 4: Run and see it pass**

Run: `go test ./e2e -run TestAReviewerThatTriesToWrite -count=1 -v`
Expected: `--- PASS: TestAReviewerThatTriesToWriteIsRefusedAndItsFindingsAreSaved`

The test does not check that no `permission.request` was published. Tasks 1.1 to 1.4 pin that: a denied ask never reaches `Broker.Ask`.

### Task 2.7: Checks for PR 2

**Files:** none

**Step 1:** From `packages/pocketd`, run `go vet ./... && go test ./...`. Expected: every package `ok`.

**Step 2:** From the repository root, run `scripts/red-team.sh`. Expected: it passes. `serve.go` now sets a broker hook.

## PR 3: The gate, the merge and the rework

**Scope:** `github.Service.MergedHere`. The rest of `atPR`: the PR review gate, a merged PR, a lead that starts a turn, a closed PR and a PR that conflicts with its base. The rework rows at Implement. The actions `markReady` and `close`. Two guardrail tests of PRD §9. After this PR the pocketd side of M5 is complete. Inert until PR 4: no desktop button sends `gh.pr.merge` yet. A merge on github.com or from the Changes panel's PR card already closes the Ticket.
**Depends on:** PR 2
**Done when:** `go vet ./... && go test ./...` is green.

### Task 3.1: `github.Service.MergedHere`

**Files:**
- Modify: `packages/pocketd/internal/github/service.go:39-55` (the struct) and `:403-410` (`Merge`)
- Test: `packages/pocketd/internal/github/service_test.go` (after `TestMergingNeverDeletesTheBranchAndShowsThePRMergedAtOnce`, `:345`)

**Context:** `Service.Merge` serves the `gh.pr.merge` verb. The engine must tell a merge that went through this verb from one made on github.com (E10). `newFake`, `repo`, `ctx` and `fixture` are helpers of `service_test.go`. The fake `gh` fails every acting verb while the file `fail` exists.

**Step 1: Write the failing tests**

```go
func TestAMergeMadeHereIsRememberedForItsTreeAndItsNumber(t *testing.T) {
	f := newFake(t)
	f.put("view.json", `{"isDraft":false,"number":7,"state":"OPEN","title":"t","url":"u","mergeable":"MERGEABLE","statusCheckRollup":[]}`)
	f.put("threads.json", fixture(t, "threads.json"))
	_, tree, reg := repo(t, false)
	s := New(f.env(true), reg)
	s.PR(ctx(t), tree)
	if s.MergedHere(tree, 7) {
		t.Fatal("merged before the merge")
	}
	if err := s.Merge(ctx(t), tree, 7, "squash"); err != nil {
		t.Fatal(err)
	}
	if !s.MergedHere(tree, 7) || s.MergedHere(tree, 8) || s.MergedHere(tree+"-other", 7) {
		t.Fatal("the merge is not remembered for its tree and number only")
	}
}

func TestAMergeThatGhRefusesIsNotRemembered(t *testing.T) {
	f := newFake(t)
	f.put("fail", "GraphQL: Pull Request is not mergeable")
	_, tree, reg := repo(t, false)
	s := New(f.env(true), reg)
	if err := s.Merge(ctx(t), tree, 7, "squash"); err == nil || s.MergedHere(tree, 7) {
		t.Fatalf("err %v, remembered %v", err, s.MergedHere(tree, 7))
	}
}
```

**Step 2: Run and see them fail**

Run: `go test ./internal/github -run 'TestAMergeMadeHere|TestAMergeThatGhRefuses' -count=1`
Expected: FAIL to build with `s.MergedHere undefined`.

**Step 3: Implement**

Add a field to `Service`, after `wake chan struct{}` (`service.go:50`):

```go
	// merged is the PR that Merge merged in each tree. It is not saved.
	merged map[string]int
```

Before (`service.go:403-410`):

```go
// Merge merges PR number by method, never deleting its branch.
func (s *Service) Merge(ctx context.Context, tree string, number int, method string) error {
	if _, err := s.act(ctx, tree, mergeArgv(number, method), ""); err != nil {
		return err
	}
	s.after(tree, func(pr *PR) { pr.State = "MERGED" })
	return nil
}
```

After:

```go
// Merge merges PR number by method, never deleting its branch.
func (s *Service) Merge(ctx context.Context, tree string, number int, method string) error {
	if _, err := s.act(ctx, tree, mergeArgv(number, method), ""); err != nil {
		return err
	}
	// Before after: its listeners ask MergedHere.
	s.mu.Lock()
	if s.merged == nil {
		s.merged = map[string]int{}
	}
	s.merged[tree] = number
	s.mu.Unlock()
	s.after(tree, func(pr *PR) { pr.State = "MERGED" })
	return nil
}

// MergedHere is whether Merge merged PR number of tree since pocketd started, which tells a merge that a
// client asked for from one made on github.com.
func (s *Service) MergedHere(tree string, number int) bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	return number != 0 && s.merged[tree] == number
}
```

**Step 4: Run and see them pass**

Run: `go test ./internal/github -count=1`
Expected: `ok  	pocketd/internal/github`

### Task 3.2: `Next` at PR review: the gate, the merge, the rework and the conflict

**Files:**
- Modify: `packages/pocketd/internal/factory/review.go` (`atPR`)
- Modify: `packages/pocketd/internal/factory/station.go` (`Next`, the Implement case)
- Modify: `packages/pocketd/internal/factory/activity.go` (`Notes`)
- Test: `packages/pocketd/internal/factory/review_test.go`

**Context:** Plan 3's `station.go` has the helpers `working(t)`, `asks(t)` and `failed(t, why)`. Plan 5 adds `checksNext(f Facts) (proto.Ticket, []Action)` in `checks.go`. Its row 19 takes a PR whose `Mergeable` is `CONFLICTING` as a failed check: with an attempt left it adds 1 to `Ticket.Fix` and returns `sendFix` with the base-conflict note, and with none left it fails the Ticket (E5). This task only sends the Ticket back to Checks. Plan 5's `commitPush` action commits the checkout, pushes, and starts a check record for the new commit. Plan 3's `Notes(before, after, team, at)` in `activity.go` turns one change of a Ticket into Activity events, and already notes `moved` when the station changes.

The states at `Station: pr`. The first row that matches wins.

| # | Facts | Ticket after | Actions |
|---|---|---|---|
| 1 | `PR == nil` | unchanged | none |
| 2 | `PR.State` is `MERGED` | `Station: merged`, `Status: done`, `Gate: ""`, `Why: ""` | `closeReviewer` if one is open, `close` |
| 3 | `Lead.Turn` is `working` | `Station: implement`, `Status: working`, `Gate: ""` | `closeReviewer` if one is open |
| 4 | `PR.State` is `CLOSED` | `Status: failed`, `Why: The pull request was closed` | `closeReviewer` if one is open |
| 5 | `PR.Mergeable` is `CONFLICTING` | `Station: checks`, then as `checksNext` decides | `closeReviewer` if one is open, then `checksNext`'s |
| 6 | `Config.Reviewer`, `Review == none` | `Status: working`, `Gate: ""` | `startReview` |
| 7 | `Config.Reviewer`, `Review == running`, the reviewer's turn ended | `Status: working` | `repairReview` |
| 8 | `Config.Reviewer`, `Review` is `running` or `repaired` | `Status: working` | none (wait) |
| 9 | otherwise: the review is `done` or `failed`, or the reviewer is off | `Gate: pr`, `Status: needsYou`, `Why: ""` | `closeReviewer` if one is open, `markReady` if the PR is a draft and the gate was closed, `await` |

"One is open" means `Reviewer.Present`, or `Review` is `running` or `repaired` (a launch may be under way).

The rework rows at `Station: implement`, for a Ticket whose `PR` is not 0. Such a Ticket came back from PR review. They run before plan 4's Implement rows.

| # | Facts | Ticket after | Actions |
|---|---|---|---|
| R1 | `Lead.Turn` is `working` | `Status: working` | none |
| R2 | `Lead.Turn` is `needsYou` | `Gate: ask`, `Status: needsYou` | none |
| R3 | otherwise (the lead's turn ended) | `Station: checks`, `Status: working` | `commitPush` |

**Step 1: Write the failing tests**

In `review_test.go`, delete the rows `a finished review closes the reviewer` and `a failed review closes the reviewer` from `TestATicketAtPRReviewGetsAReviewerThatIsRepairedOnceAndThenClosed`. The tests below cover them. Then append:

```go
// gateOpen is atGate with the gate open: the review is done and the Ticket waits on you.
func gateOpen(edit func(*Facts)) Facts {
	return atGate(func(f *Facts) {
		f.Review = ReviewDone
		f.Ticket.Gate, f.Ticket.Status = proto.GatePR, proto.TicketNeedsYou
		f.PR.Draft = false
		if edit != nil {
			edit(f)
		}
	})
}

func TestThePRReviewGateOpensOnceTheReviewIsOver(t *testing.T) {
	ended := Session{Present: true, Turn: "done"}
	for name, c := range map[string]struct {
		facts Facts
		want  string
	}{
		"a finished review":                   {atGate(func(f *Facts) { f.Review, f.Reviewer = ReviewDone, ended }), "closeReviewer,markReady,await"},
		"a failed review does not block":      {atGate(func(f *Facts) { f.Review, f.Reviewer = ReviewFailed, ended }), "closeReviewer,markReady,await"},
		"a project with the reviewer off":     {atGate(func(f *Facts) { f.Config.Reviewer = false }), "markReady,await"},
		"a pull request that is ready":        {atGate(func(f *Facts) { f.Review = ReviewDone; f.PR.Draft = false }), "await"},
		"a draft is marked ready once":        {gateOpen(func(f *Facts) { f.PR.Draft = true }), "await"},
		"after a restart the wait is set again": {gateOpen(nil), "await"},
	} {
		got, acts := Next(c.facts)
		if kinds(acts) != c.want || got.Station != proto.StationPR || got.Gate != proto.GatePR || got.Status != proto.TicketNeedsYou || got.Why != "" {
			t.Errorf("%s: got %q, %+v", name, kinds(acts), got)
		}
	}
}

func TestAMergedPullRequestMovesTheTicketToMergedAndClosesIt(t *testing.T) {
	for name, c := range map[string]struct {
		facts Facts
		want  string
	}{
		"at the gate":              {gateOpen(func(f *Facts) { f.PR.State = "MERGED" }), "close"},
		"while the reviewer reads": {atGate(func(f *Facts) { f.Review, f.Reviewer, f.PR.State = ReviewRunning, Session{Present: true, Turn: "working"}, "MERGED" }), "closeReviewer,close"},
	} {
		got, acts := Next(c.facts)
		if kinds(acts) != c.want || got.Station != proto.StationMerged || got.Gate != "" || got.Status != proto.TicketDone {
			t.Errorf("%s: got %q, %+v", name, kinds(acts), got)
		}
	}
}

func TestALeadThatStartsATurnAtPRReviewTakesTheTicketBackToImplement(t *testing.T) {
	got, acts := Next(gateOpen(func(f *Facts) { f.Lead.Turn = "working" }))
	if got.Station != proto.StationImplement || got.Gate != "" || got.Status != proto.TicketWorking || kinds(acts) != "" {
		t.Fatalf("got %q, %+v", kinds(acts), got)
	}
	got, acts = Next(atGate(func(f *Facts) {
		f.Review, f.Reviewer, f.Lead.Turn = ReviewRunning, Session{Present: true, Turn: "working"}, "working"
	}))
	if got.Station != proto.StationImplement || kinds(acts) != "closeReviewer" {
		t.Fatalf("got %q, %+v", kinds(acts), got)
	}
}

func TestAReworkedTicketIsCommittedPushedAndCheckedOnceTheLeadsTurnEnds(t *testing.T) {
	at := func(turn string) Facts {
		return atGate(func(f *Facts) { f.Ticket.Station, f.Lead.Turn = proto.StationImplement, turn })
	}
	if got, acts := Next(at("working")); got.Station != proto.StationImplement || got.Status != proto.TicketWorking || len(acts) != 0 {
		t.Fatalf("working: got %q, %+v", kinds(acts), got)
	}
	if got, acts := Next(at("needsYou")); got.Station != proto.StationImplement || got.Gate != proto.GateAsk || len(acts) != 0 {
		t.Fatalf("needsYou: got %q, %+v", kinds(acts), got)
	}
	if got, acts := Next(at("done")); got.Station != proto.StationChecks || got.Status != proto.TicketWorking || kinds(acts) != "commitPush" {
		t.Fatalf("done: got %q, %+v", kinds(acts), got)
	}
}

func TestAClosedOrConflictingPullRequestLeavesTheGate(t *testing.T) {
	got, _ := Next(gateOpen(func(f *Facts) { f.PR.State = "CLOSED" }))
	if got.Station != proto.StationPR || got.Status != proto.TicketFailed || got.Why != "The pull request was closed" || got.Gate != "" {
		t.Fatalf("closed: got %+v", got)
	}
	got, _ = Next(gateOpen(func(f *Facts) { f.PR.Mergeable = "CONFLICTING" }))
	if got.Station != proto.StationChecks || got.Gate != "" {
		t.Fatalf("conflicting: got %+v", got)
	}
}

func TestChangesYouAskForAtThePRGateAreNoted(t *testing.T) {
	before := proto.Ticket{Station: proto.StationPR, Gate: proto.GatePR, Status: proto.TicketNeedsYou}
	after := proto.Ticket{Station: proto.StationImplement, Status: proto.TicketWorking}
	got := Notes(before, after, nil, 5)
	want := proto.FactoryEvent{At: 5, Station: proto.StationPR, Actor: "you", Kind: "changes"}
	if len(got) != 2 || got[0] != want || got[1].Kind != "moved" {
		t.Fatalf("got %+v", got)
	}
}
```

**Step 2: Run and see them fail**

Run: `go test ./internal/factory -run 'TestThePRReviewGate|TestAMergedPullRequest|TestALeadThatStarts|TestAReworkedTicket|TestAClosedOrConflicting|TestChangesYouAskFor' -count=1`
Expected: FAIL. The first failure reads `a finished review: got "closeReviewer"`.

**Step 3: Implement**

In `review.go`, replace `atPR` of Task 2.4 with:

```go
// closing closes a reviewer that is open or being launched.
func closing(f Facts) []Action {
	if f.Reviewer.Present || f.Review == ReviewRunning || f.Review == ReviewRepaired {
		return []Action{{Kind: "closeReviewer"}}
	}
	return nil
}

// atPR is Next for a Ticket at PR review. The table in docs/plans/2026-11-12-light-factory-6-review.md lists
// its rows in this order.
func atPR(f Facts) (proto.Ticket, []Action) {
	t := f.Ticket
	if f.PR == nil {
		return t, nil
	}
	switch {
	case f.PR.State == "MERGED":
		t.Station, t.Status, t.Gate, t.Why = proto.StationMerged, proto.TicketDone, "", ""
		return t, append(closing(f), Action{Kind: "close"})
	case f.Lead.Turn == "working":
		t = working(t)
		t.Station = proto.StationImplement
		return t, closing(f)
	case f.PR.State == "CLOSED":
		return failed(t, "The pull request was closed"), closing(f)
	case f.PR.Mergeable == "CONFLICTING":
		f.Ticket.Station = proto.StationChecks
		next, acts := checksNext(f)
		return next, append(closing(f), acts...)
	}
	if f.Config.Reviewer {
		switch f.Review {
		case ReviewNone:
			return working(t), []Action{{Kind: "startReview"}}
		case ReviewRunning, ReviewRepaired:
			if f.Review == ReviewRunning && ended(f.Reviewer) {
				return working(t), []Action{{Kind: "repairReview"}}
			}
			return working(t), nil
		}
	}
	acts := closing(f)
	if f.PR.Draft && t.Gate != proto.GatePR {
		acts = append(acts, Action{Kind: "markReady"})
	}
	t.Status, t.Gate, t.Why = proto.TicketNeedsYou, proto.GatePR, ""
	return t, append(acts, Action{Kind: "await"})
}

// rework is Next for a Ticket that came back from PR review to Implement: it has a PR, and no Team is planned
// again. False when t is not such a Ticket.
func rework(f Facts) (proto.Ticket, []Action, bool) {
	t := f.Ticket
	if t.Station != proto.StationImplement || t.PR == 0 {
		return t, nil, false
	}
	switch f.Lead.Turn {
	case "working":
		return working(t), nil, true
	case "needsYou":
		return asks(t), nil, true
	}
	t = working(t)
	t.Station = proto.StationChecks
	return t, []Action{{Kind: "commitPush"}}, true
}
```

In `station.go`, in `Next`, make the Implement case try `rework` first. Plan 4 wrote the case. Keep its body after the new lines:

```go
	case proto.StationImplement:
		if next, acts, ok := rework(f); ok {
			return next, acts
		}
```

In `activity.go`, in `Notes`, add a case to the first `switch`, after the two cases of the Plan gate:

```go
	case before.Gate == proto.GatePR && after.Station == proto.StationImplement:
		add(proto.StationPR, "you", "changes", "")
```

**Step 4: Run and see them pass**

Run: `go test ./internal/factory -count=1`
Expected: `ok  	pocketd/internal/factory`

### Task 3.3: The engine marks the PR ready and closes a merged Ticket

**Files:**
- Modify: `packages/pocketd/internal/factory/review.go`
- Modify: `packages/pocketd/internal/factory/engine.go` (`e.act`)
- Test: `packages/pocketd/internal/factory/review_test.go`

**Context:** Names used:
- Plan 5's rig (`checks_test.go`): `newRig(t, cfg)` is an engine with one live Ticket at Checks in the worktree `r.tree`, a lead `leadID` that takes prompts and does nothing, and the fake `gh`. `r.put(name, content)` writes an answer file, `r.called(prefix)` reads the call log, `r.until(what, ok)` waits for the Ticket, `gitIn(t, dir, args...)` runs git. The fake `gh` reports no PR while the file `nopr` exists, and replaces `@HEAD@` in `view.json` with the commit of the checkout. The fake's PR has the number 57.
- Plan 5's `Engine.PRChanged(tree)` asks `github.Service.PR` for the worktree's PR, keeps it in `e.prs`, and steps the Ticket of that worktree. Plan 5 registers the engine with `github.Service.Listen` and `Watch` for the worktrees of Tickets at Checks and at PR review (index, Decisions, "Checks"). Confirm that its list of watched Tickets takes `Station: pr`. If it takes only `Station: checks`, add `proto.StationPR` there.
- `github.Service.PR` returns the cached PR for 30 seconds (`WatchedEvery`), so a test cannot change `view.json` and see the change at once. The tests below set `view.json` before the first read, or change the PR through `Service.Merge`, which edits the cache.
- `board.Store.Close(id, why string, now time.Time) error` closes a live Ticket and keeps its `Status` (`internal/board/store.go:266`).
- `github.Service.Ready(ctx, tree, number)` runs `gh pr ready <number>` (`internal/github/service.go:412`).
- `Agent.Summary().Status` reads `needsYou` while `SetAwaiting(true)` holds and the agent is idle (`internal/agent/agent.go:327`).

**Step 1: Write the failing tests** (append to `review_test.go`, and add `"context"` to the imports)

```go
// atReview puts the rig's Ticket at PR review, with a finished review of the checkout's head. view is what
// gh reports for the PR.
func atReview(t *testing.T, r *rig, view string) {
	t.Helper()
	os.Remove(filepath.Join(r.fake, "nopr"))
	r.put("view.json", view)
	rec := reviewRecord{Head: gitIn(t, r.tree, "rev-parse", "HEAD"), State: ReviewDone, Findings: []proto.Finding{}}
	if err := writeReview(r.home, r.ticket, rec); err != nil {
		t.Fatal(err)
	}
	r.e.Board.Update(r.ticket, func(x *proto.Ticket) { x.Station, x.PR = proto.StationPR, 57 })
	r.e.PRChanged(r.tree)
}

func prView(state string, draft bool) string {
	return fmt.Sprintf(`{"isDraft":%t,"number":57,"state":%q,"title":"Fix it","url":"https://github.com/acme/app/pull/57","headRefOid":"@HEAD@","baseRefName":"main","mergeable":"MERGEABLE","statusCheckRollup":[]}`, draft, state)
}

func lastEvent(t *testing.T, r *rig) proto.FactoryEvent {
	t.Helper()
	events, err := ReadActivity(r.home, r.ticket)
	if err != nil || len(events) == 0 {
		t.Fatalf("activity %+v, %v", events, err)
	}
	return events[len(events)-1]
}

func TestTheGateMarksThePRReadyAndSetsTheWaitOnTheLead(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Enabled: true, Reviewer: true, Fix: 2})
	atReview(t, r, prView("OPEN", true))
	got := r.until("the PR review gate", func(x proto.Ticket) bool { return x.Gate == proto.GatePR })
	if got.Station != proto.StationPR || got.Status != proto.TicketNeedsYou {
		t.Fatalf("got %+v", got)
	}
	lead, _ := r.e.Agents.Get(leadID)
	r.until("gh pr ready and the wait", func(proto.Ticket) bool {
		return r.called("pr ready 57") && lead.Summary().Status == "needsYou"
	})
	if r.called("pr merge") || r.called("pr review") {
		t.Fatal("the engine merged or reviewed on GitHub")
	}
}

func TestAPRMergedOnGitHubClosesTheTicketAsMergedOutsideAnywhere(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Enabled: true, Reviewer: true, Fix: 2})
	atReview(t, r, prView("MERGED", false))
	got := r.until("the closed ticket", func(x proto.Ticket) bool { return x.State == proto.TicketClosed })
	if got.ClosedWhy != proto.ClosedMerged || got.Station != proto.StationMerged || got.Gate != "" {
		t.Fatalf("got %+v", got)
	}
	r.until("the note", func(proto.Ticket) bool { return lastEvent(t, r).Kind == "merged" })
	if ev := lastEvent(t, r); ev.Actor != "anywhere" || ev.Text != "Merged outside Anywhere" {
		t.Fatalf("got %+v", ev)
	}
	if r.called("pr merge") {
		t.Fatal("the engine merged")
	}
}

func TestAMergeYouAskedForClosesTheTicketAndTakesTheWaitOffTheLead(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Enabled: true, Reviewer: true, Fix: 2})
	atReview(t, r, prView("OPEN", false))
	r.until("the PR review gate", func(x proto.Ticket) bool { return x.Gate == proto.GatePR })
	// What wsserver does for gh.pr.merge.
	if err := r.e.GitHub.Merge(context.Background(), r.tree, 57, "squash"); err != nil {
		t.Fatal(err)
	}
	r.e.PRChanged(r.tree)
	got := r.until("the closed ticket", func(x proto.Ticket) bool { return x.State == proto.TicketClosed })
	if got.ClosedWhy != proto.ClosedMerged || got.Station != proto.StationMerged {
		t.Fatalf("got %+v", got)
	}
	r.until("the note", func(proto.Ticket) bool { return lastEvent(t, r).Kind == "merged" })
	lead, _ := r.e.Agents.Get(leadID)
	if ev := lastEvent(t, r); ev.Actor != "you" || ev.Text != "Merged" || lead.Summary().Status == "needsYou" {
		t.Fatalf("got %+v, lead %s", ev, lead.Summary().Status)
	}
}

// PRD §9: the pipeline never merges and never reviews on GitHub. never.go lists the commands in order to
// refuse them, and the Factory rules name them for the agent.
func TestNoFactoryCodeMergesOrReviewsAPullRequest(t *testing.T) {
	files, err := filepath.Glob("*.go")
	if err != nil || len(files) == 0 {
		t.Fatalf("files %v, %v", files, err)
	}
	for _, name := range files {
		if strings.HasSuffix(name, "_test.go") || name == "never.go" {
			continue
		}
		raw, err := os.ReadFile(name)
		if err != nil {
			t.Fatal(err)
		}
		text := string(raw)
		if from, to := strings.Index(text, "<anywhere_factory_rules>"), strings.Index(text, "</anywhere_factory_rules>"); from >= 0 && to > from {
			text = text[:from] + text[to:]
		}
		for _, banned := range []string{"pr review", "pr merge", "GitHub.Merge(", "--approve"} {
			if strings.Contains(text, banned) {
				t.Errorf("%s holds %q", name, banned)
			}
		}
	}
}
```

Add `"fmt"` to the imports if the file does not have it.

**Step 2: Run and see them fail**

Run: `go test ./internal/factory -run 'TestTheGateMarks|TestAPRMergedOnGitHub|TestAMergeYouAskedFor|TestNoFactoryCode' -count=1`
Expected: FAIL. `TestTheGateMarksThePRReadyAndSetsTheWaitOnTheLead` times out waiting for `gh pr ready and the wait`, and the two merge tests time out waiting for `the closed ticket`. `TestNoFactoryCodeMergesOrReviewsAPullRequest` passes already. It guards what later changes add.

**Step 3: Implement**

Append to `review.go` (add `"context"` to the imports):

```go
// markReady takes t's PR out of draft. A failure is noted and the gate opens anyway: the bar then says Draft,
// and the Changes panel's PR card can mark it ready.
func (e *Engine) markReady(t proto.Ticket) {
	if err := e.GitHub.Ready(context.Background(), t.Worktree, t.PR); err != nil {
		e.note(t.ID, proto.FactoryEvent{At: e.now().UnixMilli(), Station: proto.StationPR, Actor: "anywhere", Kind: "failed", Text: "The pull request couldn't be marked ready: " + err.Error()})
	}
}

// merged closes t, whose PR was merged, and notes who merged it.
func (e *Engine) merged(t proto.Ticket) {
	actor, text := "anywhere", "Merged outside Anywhere"
	if e.GitHub.MergedHere(t.Worktree, t.PR) {
		actor, text = "you", "Merged"
	}
	if lead, err := e.Agents.Get(t.AgentID); err == nil {
		lead.SetAwaiting(false)
	}
	if err := e.Board.Close(t.ID, proto.ClosedMerged, e.now()); err != nil {
		log.Printf("factory: closing %s: %v", t.ID, err)
		return
	}
	e.note(t.ID, proto.FactoryEvent{At: e.now().UnixMilli(), Station: proto.StationMerged, Actor: actor, Kind: "merged", Text: text})
}
```

`e.act` starts with `lead, ok := e.lead(t.AgentID)` and stops when the lead is gone. A Ticket whose lead closed must still close when its PR is merged. Put a `switch` before that check, and delete the `closeReviewer` arm that Task 2.5 added further down:

```go
	switch a.Kind {
	case "closeReviewer":
		e.closeReviewer(t)
		return true
	case "markReady":
		e.markReady(t)
		return true
	case "close":
		e.merged(t)
		return true
	}
```

`await` is plan 3's arm. It calls `lead.SetAwaiting(true)`. `atPR` returns it on every step while the gate is open, and `Engine.Recover` steps every live Ticket, so a restart sets the wait again from `Gate` (US-36).

**Step 4: Run and see them pass**

Run: `go test ./internal/factory -count=1`
Expected: `ok  	pocketd/internal/factory`

### Task 3.4: End to end: from Checks through the reviewer to the gate, and closed when you merge

**Files:**
- Modify: `packages/pocketd/e2e/factory_review_test.go`

**Context:** `factoryAtPR` and `factoryDetail` are in Task 2.6. The owner connection of `factoryAtPR` must negotiate `proto.CapGitHub` beside `proto.CapBoard`, `proto.CapTeam` and `proto.CapFactory`, because `gh.pr.merge` is refused without it (`internal/wsserver/gh.go`, `ghVerb`). The verb needs `tree`, `number` and `method` (`internal/proto/gh.go`). `Service.Merge` marks the cached PR merged at once, so the engine sees the merge without waiting for the next poll. `Phone.WaitTicket(what, ok)` reads `board` frames until a Ticket matches (`e2e/board_test.go:23`).

**Step 1: Write the test** (append to `factory_review_test.go`)

```go
func TestAReviewedTicketWaitsAtThePRGateAndClosesWhenYouMerge(t *testing.T) {
	h, o, gh, ticket := factoryAtPR(t, reviewed())
	gate := o.WaitTicket("the PR review gate", func(x proto.Ticket) bool { return x.ID == ticket.ID && x.Gate == proto.GatePR })
	if gate.Station != proto.StationPR || gate.Status != proto.TicketNeedsYou || gate.PR == 0 {
		t.Fatalf("got %+v", gate)
	}
	if d := factoryDetail(o, ticket.ID); len(d.Findings) != 1 || d.Findings[0].Severity != "critical" {
		t.Fatalf("findings %+v", d.Findings)
	}
	h.eventually("gh pr ready", func() bool { return gh.called("pr ready") })
	if gh.called("pr review") || gh.called("pr merge") {
		t.Fatal("the pipeline reviewed or merged on GitHub")
	}

	o.Send(map[string]any{"type": "gh.pr.merge", "id": "m1", "tree": gate.Worktree, "number": gate.PR, "method": "squash"})
	closed := o.WaitTicket("the merged ticket", func(x proto.Ticket) bool { return x.ID == ticket.ID && x.State == proto.TicketClosed })
	if closed.ClosedWhy != proto.ClosedMerged || closed.Station != proto.StationMerged || closed.Gate != "" {
		t.Fatalf("got %+v", closed)
	}
	h.eventually("the merge in the activity", func() bool {
		d := factoryDetail(o, ticket.ID)
		last := d.Activity[len(d.Activity)-1]
		return last.Kind == "merged" && last.Actor == "you" && last.Text == "Merged"
	})
	if _, err := os.Stat(filepath.Join(closed.Worktree, ".git")); err != nil {
		t.Fatalf("the merge removed the worktree: %v", err)
	}
}
```

**Step 2: Run it**

Run: `go test ./e2e -run 'TestAReviewedTicket|TestAReviewerThatTriesToWrite' -count=1 -v`
Expected: both PASS. Tasks 3.1 to 3.3 already built what the test drives. If `the PR review gate` times out, read pocketd's log in the harness output for `factory:` lines. The usual cause is a `factoryAtPR` whose fake `gh` still reports a pending check.

### Task 3.5: Checks for PR 3

**Files:** none

**Step 1:** From `packages/pocketd`, run `go vet ./... && go test ./...`. Expected: every package `ok`.

**Step 2:** From the repository root, run `scripts/red-team.sh`. Expected: it passes. This PR changes no broker hook and no `peer.Needs` row, so the run only confirms that.

**Step 3:** Confirm the guardrails of PRD §9 that this plan owns. Both are tests that now exist: `TestNoReviewerAskThatEditsWritesOrChangesStateIsEverAllowed` and `TestNoFactoryCodeMergesOrReviewsAPullRequest`. Run `go test ./internal/factory -run 'TestNoReviewerAsk|TestNoFactoryCode' -count=1 -v` and see both PASS.

## PR 4: The desktop: the PR tab, the merge bar and the Merged rail

**Scope:** `agents`: `factory::Finding`, `Detail.findings`, `gh.v1`, `Outbox::gh_merge`, `Event::GhMerged`. `git`: the PR's size. `pocket`: `factory/pr.rs` (the PR tab), the `Decision::Pr` bar, the merged card, and the capture step `factory-ticket-pr`. After this PR you can review and merge a Factory ticket from its page.
**Depends on:** PR 3 for the behaviour. The code builds on plans 1 to 5 alone.
**Done when:** `cargo build --workspace`, `cargo clippy --workspace --all-targets` with no new warning and `cargo test --workspace` are green from `packages/desktop`, and the capture of Task 4.6 matches the prototype.

### Task 4.1: `agents`: findings, `gh.v1`, `gh_merge` and its answer

**Files:**
- Modify: `packages/desktop/crates/agents/src/factory.rs` (plan 1 creates it)
- Modify: `packages/desktop/crates/agents/src/agents.rs:400-461` (`Event`), `:950-1160` (`Outbox`), `:1187` (`CAPS`), `:1284-1319` (`Requests`, `refused`)
- Modify: `packages/desktop/crates/hosts/src/session.rs:152-161` (`tag_event`)

**Context:** Plan 3 adds `agents::factory::Detail`, the wire's `FactoryTicket`. Plan 1 adds `Event::FactoryError(String)`, `FACTORY_CAP` in `CAPS`, and `factory_id()` beside `board_id()` (`agents.rs:835`). pocketd decodes `gh.pr.merge` with `tree`, `number` and `method`, where `method` is `squash`, `merge` or `rebase` (`packages/pocketd/internal/proto/gh.go`). It answers with an `ack`, or with an `error` whose `code` is `gh_failed`, `gh_missing`, `gh_signed_out` or `not_a_tree` and whose `message` says why (`internal/wsserver/gh.go`). `Requests` (`agents.rs:1286`) keeps each request that was sent and not answered, so an answer can name what it answers. The `agents` crate does not depend on `git`, so the method travels as its wire word. With `gh.v1` negotiated, pocketd also pushes `gh.changed` frames. The reader drops a frame whose type it does not know.

**Step 1: Write the failing tests**

In the `tests` module of `agents/src/factory.rs`:

```rust
    #[test]
    fn a_ticket_detail_reads_its_findings_and_a_severity_orders_worst_first() {
        let raw = serde_json::json!({"ticketId": "t1", "activity": [], "findings": [
            {"severity": "critical", "file": "src/a.rs", "line": 12, "message": "Leaks the handle"},
            {"severity": "nit", "message": "Name it"}
        ]});
        let d: Detail = serde_json::from_value(raw).unwrap();
        assert_eq!(d.findings, vec![
            Finding { severity: Severity::Critical, file: "src/a.rs".into(), line: 12, message: "Leaks the handle".into() },
            Finding { severity: Severity::Nit, file: String::new(), line: 0, message: "Name it".into() },
        ]);
        assert!(Severity::Critical < Severity::Minor && Severity::Minor < Severity::Nit);
    }
```

In the `tests` module of `agents/src/agents.rs`, after `team_merge_sends_the_golden_team_merge_frame_but_its_id` (`:2380`):

```rust
    #[test]
    fn gh_merge_sends_the_golden_gh_pr_merge_frame_but_its_id() {
        assert_eq!(sent_but_id(|o| o.gh_merge("/Users/me/dev/pocket", 118, "squash")), without_id(golden!("client/gh_pr_merge.json")));
    }

    #[test]
    fn an_acked_merge_names_its_worktree_and_a_refused_one_says_why() {
        let mut requests = Requests::default();
        requests.sent(&json!({"type": "gh.pr.merge", "id": "factory-1", "tree": "/w/fix", "number": 7, "method": "squash"}));
        requests.sent(&json!({"type": "gh.pr.merge", "id": "factory-2", "tree": "/w/fix", "number": 7, "method": "squash"}));
        let merged = requests.answer(&frame(json!({"type": "ack", "id": "factory-1"})));
        assert!(matches!(merged, Some(Event::GhMerged(tree)) if tree == "/w/fix"));
        let refused = requests.answer(&frame(json!({"type": "error", "id": "factory-2", "code": "gh_failed", "message": "Pull request is not mergeable"})));
        assert!(matches!(refused, Some(Event::FactoryError(message)) if message == "Pull request is not mergeable"));
    }

    #[test]
    fn the_hello_offers_the_github_verbs() {
        assert!(hello(None)["caps"].as_array().unwrap().iter().any(|c| c == "gh.v1"));
    }
```

**Step 2: Run and see them fail**

Run: `cargo test -p agents gh_merge`
Expected: FAIL to build with `no method named gh_merge found for reference &Outbox`.

**Step 3: Implement**

In `agents/src/factory.rs`, add the two types and the field (`Detail` is plan 3's struct):

```rust
/// How bad the reviewer says a finding is, the worst first.
#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Critical,
    Minor,
    #[default]
    #[serde(other)]
    Nit,
}

/// One thing the reviewer found in a Ticket's PR.
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Finding {
    pub severity: Severity,
    pub file: String,
    pub line: u32,
    pub message: String,
}
```

```rust
    // in Detail
    pub findings: Vec<Finding>,
```

Re-export both from `agents.rs` where plan 1 re-exports the Factory types: add `Finding` and `Severity` to that `pub use factory::{…}` list.

In `agents.rs`, add to `Event`, after `TeamFailed` (`:460`):

```rust
    /// pocketd merged the PR of the worktree, as `Outbox::gh_merge` asked.
    GhMerged(String),
```

Add to `impl Outbox`, after `team_merge` (`:1155`):

```rust
    /// Merges PR `number` of the worktree `tree` through pocketd. `method` is squash, merge or rebase.
    /// `Event::GhMerged` answers, or `Event::FactoryError` with why not.
    pub fn gh_merge(&self, tree: &str, number: u32, method: &str) {
        self.send(json!({"type": "gh.pr.merge", "id": factory_id(), "tree": tree, "number": number, "method": method}));
    }
```

Add the cap beside `TEAM_CAP` (`:888`), put `GH_CAP` last in `CAPS` (`:1187`), and raise the array's length by one:

```rust
/// pocketd's GitHub verbs. The desktop sends only `gh.pr.merge`, for the PR of a Factory ticket.
const GH_CAP: &str = "gh.v1";
```

Before (`agents.rs:1289-1293`, `:1301-1302`, `:1316-1317`):

```rust
    fn sent(&mut self, m: &Value) {
        if matches!(m["type"].as_str(), Some("agent.prompt" | "agent.interrupt" | "team.start" | "team.stop" | "team.merge")) {
            self.0.push_back(m.clone());
        }
    }
```

```rust
        let m = self.0.remove(i)?;
        if f.kind == "error" { refused(&m, &f.message) } else { None }
```

```rust
        "team.merge" => team(field("lead"), TeamAction::Merge),
        _ => None,
```

After:

```rust
    fn sent(&mut self, m: &Value) {
        if matches!(m["type"].as_str(), Some("agent.prompt" | "agent.interrupt" | "team.start" | "team.stop" | "team.merge" | "gh.pr.merge")) {
            self.0.push_back(m.clone());
        }
    }
```

```rust
        let m = self.0.remove(i)?;
        if f.kind == "error" { refused(&m, &f.message) } else { acked(&m) }
```

```rust
        "team.merge" => team(field("lead"), TeamAction::Merge),
        "gh.pr.merge" => Some(Event::FactoryError(readable(reason))),
        _ => None,
```

Add after `refused`:

```rust
/// The event for a request `m` that pocketd acked: only a merge has one.
fn acked(m: &Value) -> Option<Event> {
    (m["type"] == "gh.pr.merge").then(|| Event::GhMerged(m["tree"].as_str().unwrap_or_default().to_string()))
}
```

In `hosts/src/session.rs`, `tag_event`, add the event to the arm that returns false, after `Event::BoardError(_)` (`:161`). The Board talks only to this Mac's pocketd, so the tree is a local path:

```rust
        | Event::BoardError(_)
        | Event::GhMerged(_) => return false,
```

**Step 4: Run and see them pass**

Run: `cargo test -p agents && cargo build -p hosts`
Expected: `test result: ok`, and the build prints no error. The `match` of `tag_event` is exhaustive, so a missing arm fails the build.

### Task 4.2: `git`: a PR knows its size

**Files:**
- Modify: `packages/desktop/crates/git/src/github.rs:5-24` (`Pr`), `:104-110` (`VIEW`), `:377-404` (the `Pr` literal of the parser)

**Context:** The bar shows `#203 · +96 -4 · 7 files` and the merged card shows `Merged · #201 · +96 -4`. The engine commits a Factory ticket's work, so the working tree's own totals (`Repo::totals`) are zero there. `gh pr view --json` has the fields `additions`, `deletions` and `changedFiles`. The tests of `github.rs` build a `Pr` from JSON with the helper `pr(json)` (see `merged_closed_and_draft_prs_keep_their_state`, `:620`).

**Step 1: Write the failing test** (in the `tests` module of `github.rs`)

```rust
    #[test]
    fn a_pr_reads_how_much_it_changes() {
        let sized = pr(r#"{"isDraft":false,"number":7,"state":"OPEN","title":"t","url":"u","statusCheckRollup":[],"additions":96,"deletions":4,"changedFiles":7}"#);
        assert_eq!((sized.additions, sized.deletions, sized.files), (96, 4, 7));
        let bare = pr(r#"{"isDraft":false,"number":7,"state":"OPEN","title":"t","url":"u","statusCheckRollup":[]}"#);
        assert_eq!((bare.additions, bare.deletions, bare.files), (0, 0, 0));
        assert!(VIEW[4].ends_with(",additions,deletions,changedFiles"));
    }
```

**Step 2: Run and see it fail**

Run: `cargo test -p git a_pr_reads_how_much_it_changes`
Expected: FAIL to build with `no field additions on type Pr`.

**Step 3: Implement**

Add to `Pr`, after `head` (`github.rs:22`):

```rust
    /// Lines added and removed against the base, and how many files changed.
    pub additions: usize,
    pub deletions: usize,
    pub files: usize,
```

Before (`github.rs:109`):

```rust
    "number,title,url,state,isDraft,statusCheckRollup,reviewDecision,latestReviews,reviewRequests,mergeable,mergeStateStatus,headRefOid,baseRefName",
```

After:

```rust
    "number,title,url,state,isDraft,statusCheckRollup,reviewDecision,latestReviews,reviewRequests,mergeable,mergeStateStatus,headRefOid,baseRefName,additions,deletions,changedFiles",
```

In the parser's `Pr` literal, after `head: str(&v["headRefOid"]),` (`:402`):

```rust
        additions: count(&v["additions"]),
        deletions: count(&v["deletions"]),
        files: count(&v["changedFiles"]),
```

Add below the parser:

```rust
fn count(v: &Value) -> usize {
    v.as_u64().and_then(|n| usize::try_from(n).ok()).unwrap_or(0)
}
```

Every other `Pr` literal in the workspace ends with `..Pr::default()`. If the build names one that does not, add the three fields there with 0.

**Step 4: Run and see it pass**

Run: `cargo test -p git github`
Expected: `test result: ok`

### Task 4.3: `factory/pr.rs`: the PR tab

**Files:**
- Create: `packages/desktop/crates/pocket/src/factory/pr.rs`
- Modify: `packages/desktop/crates/pocket/src/factory.rs` (`mod pr;`)
- Modify: `packages/desktop/crates/pocket/src/factory/page.rs` (the tab body)
- Modify: `packages/desktop/crates/pocket/src/git_ui/pull_request/card.rs:54`, `:171`, `:253` (visibility)

**Context:** The prototype's `prTab` (`.ui-review/prototypes/light-factory/app.js:347-354`) draws the PR's number and title, a line `<branch> into <base> · N files · +a -b`, then the merge-gate rows and the findings. US-27 puts the findings above the merge-gate rows, so this tab draws them first. A critical finding disables nothing (R4). The quiet states are `Opens after Checks`, `#N · the reviewer is reading the diff` and `#N · merged`.

The PR card of the Changes panel already draws the checks and the merge row. This task widens three functions of `card.rs` to `pub(crate)` instead of copying them: `fact` (`:54`), `Desktop::pr_checks` (`:171`) and `Desktop::pr_gate` (`:253`). `row`, `glyph_slot` and `review_icon` are `pub(crate)` already, and so is `review_lines` (`git_ui/pull_request.rs:180`). `pr_checks` marks a failed check as "Fixing" unless the check is in `unsent`, so the tab passes every failed check as `unsent`.

Names of earlier plans: `agents::tickets::Ticket` with `station`, `gate` and `pr` (plan 1). `Agents::detail(ticket) -> Option<&Detail>` (plan 3). `Tab::Pr` in `factory/tabs.rs` and the `match` over the open `Tab` in `factory/page.rs` (plan 3). `Desktop.prs.get(tree)` is the desktop's own PR cache (`git_ui/pull_requests.rs:79`), and `Desktop.repos.get(tree)` has the worktree's `branch`.

**Step 1: Write the failing tests** (the `tests` module at the bottom of the new `pr.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use agents::tickets::Ticket;

    fn finding(severity: Severity, file: &str, line: u32, message: &str) -> Finding {
        Finding { severity, file: file.into(), line, message: message.into() }
    }

    #[test]
    fn findings_list_critical_first_and_keep_the_reviewers_order_within_a_severity() {
        let all = [finding(Severity::Nit, "", 0, "n1"), finding(Severity::Minor, "", 0, "m1"), finding(Severity::Critical, "", 0, "c1"), finding(Severity::Minor, "", 0, "m2")];
        let order: Vec<&str> = sorted(&all).iter().map(|f| f.message.as_str()).collect();
        assert_eq!(order, ["c1", "m1", "m2", "n1"]);
    }

    #[test]
    fn a_finding_points_at_its_file_and_line_when_it_has_them() {
        assert_eq!(place(&finding(Severity::Minor, "src/a.rs", 12, "x")).as_deref(), Some("src/a.rs:12"));
        assert_eq!(place(&finding(Severity::Minor, "src/a.rs", 0, "x")).as_deref(), Some("src/a.rs"));
        assert_eq!(place(&finding(Severity::Minor, "", 12, "x")), None);
    }

    #[test]
    fn the_reviewer_line_always_counts_critical_and_the_rest_only_when_there_are_some() {
        assert_eq!(summary(&[finding(Severity::Minor, "", 0, "a"), finding(Severity::Minor, "", 0, "b")]), "0 critical · 2 minor");
        assert_eq!(summary(&[finding(Severity::Critical, "", 0, "a"), finding(Severity::Nit, "", 0, "b")]), "1 critical · 1 nit");
        assert_eq!(summary(&[]), "No findings");
    }

    #[test]
    fn the_merge_button_says_why_it_is_off_and_nothing_when_the_pr_can_merge() {
        assert_eq!(blocked(Gate::Ready), None);
        assert_eq!(blocked(Gate::ChecksRunning), Some("Checks running"));
        assert_eq!(blocked(Gate::Conflicts), Some("Conflicts"));
        assert_eq!(blocked(Gate::Draft), Some("Draft"));
    }

    #[test]
    fn a_pr_says_its_size_for_the_bar_and_for_a_merged_card() {
        let pr = Pr { number: 203, additions: 96, deletions: 4, files: 7, ..Pr::default() };
        assert_eq!(size(&pr), "#203 · +96 -4 · 7 files");
        assert_eq!(size(&Pr { files: 1, ..pr.clone() }), "#203 · +96 -4 · 1 file");
        assert_eq!(merged_line(201, Some(&pr)), "Merged · #201 · +96 -4");
        assert_eq!(merged_line(201, None), "Merged · #201");
    }

    #[test]
    fn each_merge_method_goes_out_as_pocketds_word() {
        assert_eq!(Method::ALL.map(wire), ["squash", "merge", "rebase"]);
    }

    #[test]
    fn the_tab_is_quiet_until_the_gate_opens_and_after_the_merge() {
        let at = |pr, station, gate| Ticket { pr, station, gate, ..Ticket::sample() };
        assert_eq!(quiet(&at(0, Station::Checks, Waits::None)).as_deref(), Some("Opens after Checks"));
        assert_eq!(quiet(&at(203, Station::Pr, Waits::None)).as_deref(), Some("#203 · the reviewer is reading the diff"));
        assert_eq!(quiet(&at(203, Station::Pr, Waits::Pr)), None);
        assert_eq!(quiet(&at(203, Station::Checks, Waits::None)), None);
        assert_eq!(quiet(&at(201, Station::Merged, Waits::None)).as_deref(), Some("#201 · merged"));
    }
}
```

`Ticket::sample()` is plan 1's test helper (index, "Desktop crates").

**Step 2: Run and see them fail**

Add `mod pr;` to `factory.rs`, then run: `cargo test -p pocket factory::pr`
Expected: FAIL to build with `cannot find function sorted in this scope`.

**Step 3: Implement** (the top of `pr.rs`)

```rust
use crate::desktop::Desktop;
use crate::git_ui::pull_request::card::{fact, glyph_slot, review_icon, row};
use crate::git_ui::pull_request::review_lines;
use agents::factory::{Finding, Severity};
use agents::tickets::{Gate as Waits, Station, Ticket};
use git::github::{Check, Gate, Method, Outcome, Pr};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

/// The findings, the worst first. Findings of one severity keep the reviewer's order.
pub(crate) fn sorted(findings: &[Finding]) -> Vec<&Finding> {
    let mut out: Vec<&Finding> = findings.iter().collect();
    out.sort_by_key(|f| f.severity);
    out
}

fn severity_label(s: Severity) -> &'static str {
    match s {
        Severity::Critical => "Critical",
        Severity::Minor => "Minor",
        Severity::Nit => "Nit",
    }
}

/// Where a finding points: `src/a.rs:12`, the file alone without a line, nothing without a file.
pub(crate) fn place(f: &Finding) -> Option<String> {
    match (f.file.is_empty(), f.line) {
        (true, _) => None,
        (false, 0) => Some(f.file.clone()),
        (false, line) => Some(format!("{}:{line}", f.file)),
    }
}

/// The findings counted for one line: `0 critical · 2 minor`.
pub(crate) fn summary(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return "No findings".into();
    }
    let count = |s: Severity| findings.iter().filter(|f| f.severity == s).count();
    let mut parts = vec![format!("{} critical", count(Severity::Critical))];
    for (s, name) in [(Severity::Minor, "minor"), (Severity::Nit, "nit")] {
        if count(s) > 0 {
            parts.push(format!("{} {name}", count(s)));
        }
    }
    parts.join(" · ")
}

/// Why the merge button is off, in a few words. None when the PR can merge.
pub(crate) fn blocked(gate: Gate) -> Option<&'static str> {
    match gate {
        Gate::Ready => None,
        Gate::Done => Some("Merged or closed on GitHub"),
        Gate::Draft => Some("Draft"),
        Gate::Conflicts => Some("Conflicts"),
        Gate::Checking => Some("Checking for conflicts"),
        Gate::ChecksFailed => Some("Checks failed"),
        Gate::ChecksRunning => Some("Checks running"),
        Gate::ChangesRequested => Some("Changes requested"),
        Gate::ReviewRequired => Some("Needs an approving review"),
        Gate::Blocked => Some("A branch rule holds the merge"),
    }
}

/// The PR's number and size for the bar: `#203 · +96 -4 · 7 files`.
pub(crate) fn size(pr: &Pr) -> String {
    let files = if pr.files == 1 { "1 file".to_string() } else { format!("{} files", pr.files) };
    format!("#{} · +{} -{} · {files}", pr.number, pr.additions, pr.deletions)
}

/// What the card of a merged Ticket says: `Merged · #201 · +96 -4`, without the size while the PR is not read.
pub(crate) fn merged_line(number: u32, pr: Option<&Pr>) -> String {
    match pr {
        Some(pr) => format!("Merged · #{number} · +{} -{}", pr.additions, pr.deletions),
        None => format!("Merged · #{number}"),
    }
}

/// The merge method as `gh.pr.merge` takes it.
pub(crate) fn wire(method: Method) -> &'static str {
    match method {
        Method::Squash => "squash",
        Method::Merge => "merge",
        Method::Rebase => "rebase",
    }
}

/// What the tab says while it has nothing to list.
pub(crate) fn quiet(t: &Ticket) -> Option<String> {
    match (t.pr, t.station, t.gate) {
        (0, _, _) => Some("Opens after Checks".into()),
        (n, Station::Merged, _) => Some(format!("#{n} · merged")),
        (n, Station::Pr, gate) if gate != Waits::Pr => Some(format!("#{n} · the reviewer is reading the diff")),
        _ => None,
    }
}

fn heading(label: &'static str, note: String) -> Div {
    div().flex().items_baseline().gap(px(8.)).child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(label)).child(div().text_size(px(12.)).text_color(TEXT_3).child(note))
}

fn rows() -> Div {
    div().flex().flex_col().rounded(px(8.)).shadow(vec![ui::ring(SEPARATOR, 0.5)])
}

fn finding_row(f: &Finding) -> Div {
    let tone = if f.severity == Severity::Critical { FAILED_TEXT } else { TEXT_3 };
    let text = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(div().text_size(px(12.5)).text_color(TEXT).child(f.message.clone()))
        .children(place(f).map(|at| div().font_family(MONO).text_size(px(11.5)).text_color(TEXT_3).truncate().child(at)));
    div().px(px(12.)).py(px(9.)).flex().items_start().gap(px(8.)).child(div().w(px(52.)).flex_none().text_size(px(12.)).text_color(tone).child(severity_label(f.severity))).child(text)
}

impl Desktop {
    /// The PR tab of a Factory ticket's page: the reviewer's findings, then what holds the merge back.
    pub(crate) fn factory_pr_tab(&self, t: &Ticket, cx: &mut Context<Self>) -> Div {
        let wrap = div().flex().flex_col().gap(px(10.));
        let still = |text: String| div().py(px(28.)).flex().justify_center().text_size(px(12.5)).text_color(TEXT_3).child(text);
        if let Some(text) = quiet(t) {
            return wrap.child(still(text));
        }
        let Some(pr) = self.prs.get(&t.worktree) else {
            return wrap.child(still(format!("#{} · reading the pull request", t.pr)));
        };
        let findings = self.agents.detail(&t.id).map(|d| d.findings.as_slice()).unwrap_or_default();
        let branch = self.repos.get(&t.worktree).map(|r| r.branch.clone()).unwrap_or_default();
        let files = if pr.files == 1 { "1 file".to_string() } else { format!("{} files", pr.files) };
        let mono = |text: String| div().font_family(MONO).child(text);
        let head = div().flex().items_baseline().gap(px(8.)).child(div().font_family(MONO).text_size(px(12.5)).text_color(TEXT_3).child(format!("#{}", pr.number))).child(div().min_w_0().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(pr.title.clone()));
        let meta = div().flex().flex_wrap().items_center().gap(px(5.)).text_size(px(12.)).text_color(TEXT_3).when(!branch.is_empty(), |d| d.child(mono(branch)).child("into")).child(mono(pr.base.clone())).child(format!("· {files} ·")).child(ui::meta_diff(pr.additions, pr.deletions, 12.));

        let found = if findings.is_empty() {
            rows().child(div().px(px(12.)).py(px(9.)).text_size(px(12.5)).text_color(TEXT_3).child("No findings"))
        } else {
            rows().children(sorted(findings).into_iter().map(finding_row))
        };

        let failed: Vec<&Check> = pr.runs.iter().filter(|c| c.outcome == Outcome::Failed).collect();
        let reviews = div().flex().flex_col().children(review_lines(pr).into_iter().map(|(r, text)| row().child(review_icon(r)).child(div().flex_1().min_w_0().truncate().child(text))));
        let reviewer = row().child(glyph_slot(ui::dot(6., TEXT_4))).child(summary(findings));
        let gate = rows()
            .px(px(12.))
            .py(px(6.))
            .child(fact("Checks", self.pr_checks(pr, false, &failed, cx)))
            .child(fact("Merge", self.pr_gate(pr, false, "")))
            .child(fact("Reviewer", reviewer))
            .child(fact("Review", reviews));

        wrap.child(head).child(meta).child(heading("Reviewer", summary(findings)).mt(px(8.))).child(found).child(heading("Merge gate", String::new()).mt(px(8.))).child(gate)
    }
}
```

In `card.rs`, widen the three functions. Before (`:54`, `:171`, `:253`):

```rust
fn fact(label: &'static str, value: Div) -> Div {
```

```rust
    fn pr_checks(&self, pr: &Pr, can_send: bool, unsent: &[&Check], cx: &mut Context<Self>) -> Div {
```

```rust
    fn pr_gate(&self, pr: &Pr, resolving: bool, agent: &str) -> Div {
```

After:

```rust
pub(crate) fn fact(label: &'static str, value: Div) -> Div {
```

```rust
    pub(crate) fn pr_checks(&self, pr: &Pr, can_send: bool, unsent: &[&Check], cx: &mut Context<Self>) -> Div {
```

```rust
    pub(crate) fn pr_gate(&self, pr: &Pr, resolving: bool, agent: &str) -> Div {
```

If `card` is a private module of `git_ui::pull_request`, make its declaration `pub(crate) mod card;`.

In `factory/page.rs`, in the `match` that draws the open tab, replace the placeholder arm of `Tab::Pr` with:

```rust
            Tab::Pr => self.factory_pr_tab(t, cx),
```

The tab draws at most 50 findings and the checks that `pr_checks` already caps, so it needs no virtual list. It reads only `self.prs`, `self.repos` and `self.agents`, and does no IO in `render`.

**Step 4: Run and see them pass**

Run: `cargo test -p pocket factory::pr && cargo clippy -p pocket --all-targets`
Expected: `test result: ok. 7 passed`, and clippy prints no new warning.

### Task 4.4: The footer bar at the PR gate: Request changes and the merge

**Files:**
- Modify: `packages/desktop/crates/pocket/src/factory/bar.rs` (`decision`, the `Decision::Pr` arm, the `⌘↵` handler)
- Modify: `packages/desktop/crates/pocket/src/factory/pr.rs`
- Modify: `packages/desktop/crates/pocket/src/factory.rs` (`FactoryState`)
- Modify: `packages/desktop/crates/pocket/src/desktop/alerts.rs:143` (the events)

**Context:** The prototype's bar at the PR gate (`app.js:392-396`): a dot, **PR review**, `#203 · +96 -4 · 7 files`, the button **Request changes**, and a split primary button `Squash and merge ⌘↵` whose chevron opens the method menu. R6: the merge button is enabled only when `Pr::gate()` is `Ready`, and the bar says why when it is off.

Names of plan 3 (index, `factory/bar.rs`): `decision(t, team, pr, ask, detail) -> Option<Decision>` and `Decision::Pr { gate: git::github::Gate, number: u32 }`. `Decision` derives `Debug` and `PartialEq`. Plan 3 built **Request changes** for the Plan gate: a note that it sends to the lead with `Outboxes::prompt(lead, text, true)` (`hosts/src/session.rs:237`). This plan calls its opener `Desktop::factory_request_changes(window, cx)` and its `⌘↵` handler `Desktop::factory_primary(window, cx) -> bool` (see the guess table). At the PR gate the note is a plain prompt (R7), so the `team` flag must be false there.

Names of the repository: `split_button(id, lead, label, ready, open, main, toggle, menu, cx)` (`git_ui/changes/commit_box.rs:15`) draws a primary button with a chevron menu. It runs `main` on every click, so `main` must check for itself. `Desktop.pr.track.method` is the merge method that the Changes panel's PR card uses (`git_ui/pull_request.rs:234`). `bar_button` and `start_kbd` are in `board/parts.rs:86-92`, and plan 2 made the parts that the Factory page needs `pub(crate)`. `Desktop.outbox.local` is this Mac's pocketd, which owns the Board. `Desktop::poll_prs` (`git_ui/pull_requests.rs:243`) asks `gh` again for what expired.

**Step 1: Write the failing tests**

Append to the `tests` module of `pr.rs` (add `use git::github::{Mergeable, PrState};` to the module):

```rust
    #[test]
    fn the_merge_fires_only_at_the_pr_gate_for_a_pr_that_can_merge_and_only_once() {
        let t = Ticket { pr: 203, station: Station::Pr, gate: Waits::Pr, ..Ticket::sample() };
        let ready = Pr { number: 203, mergeable: Mergeable::Clean, ..Pr::default() };
        assert_eq!(can_merge(&t, Some(&ready), false), Some(203));
        assert_eq!(can_merge(&t, Some(&ready), true), None);
        assert_eq!(can_merge(&t, Some(&Pr { draft: true, ..ready.clone() }), false), None);
        assert_eq!(can_merge(&t, Some(&Pr { state: PrState::Merged, ..ready.clone() }), false), None);
        assert_eq!(can_merge(&t, None, false), None);
        assert_eq!(can_merge(&Ticket { gate: Waits::None, ..t.clone() }, Some(&ready), false), None);
    }

    #[test]
    fn a_critical_finding_does_not_hold_the_merge() {
        let t = Ticket { pr: 203, station: Station::Pr, gate: Waits::Pr, ..Ticket::sample() };
        let ready = Pr { number: 203, mergeable: Mergeable::Clean, ..Pr::default() };
        // can_merge takes no findings: R4 leaves the decision to you.
        assert_eq!(can_merge(&t, Some(&ready), false), Some(203));
    }

    #[test]
    fn request_changes_asks_for_a_new_plan_only_at_the_plan_gate() {
        assert!(replans(Waits::Plan));
        assert!(!replans(Waits::Pr));
    }
```

Append to the `tests` module of `bar.rs`:

```rust
    #[test]
    fn a_ticket_at_the_pr_gate_offers_the_merge_with_what_holds_it_back() {
        let t = Ticket { pr: 203, station: Station::Pr, gate: Gate::Pr, ..Ticket::sample() };
        let pr = git::github::Pr { number: 203, draft: true, ..Default::default() };
        assert_eq!(decision(&t, None, Some(&pr), None, None), Some(Decision::Pr { gate: git::github::Gate::Draft, number: 203 }));
        assert_eq!(decision(&t, None, None, None, None), Some(Decision::Pr { gate: git::github::Gate::Checking, number: 203 }));
    }
```

`Gate` in `bar.rs` is `agents::tickets::Gate`, as plan 3 imports it there. If the file imports it under another name, use that name.

**Step 2: Run and see them fail**

Run: `cargo test -p pocket factory::`
Expected: FAIL to build with `cannot find function can_merge in this scope`.

**Step 3: Implement**

Add two fields to `FactoryState` in `factory.rs`, and set both to their defaults in `FactoryState::new`:

```rust
    /// The merge method menu of the bar is open.
    pub(crate) merge_menu: bool,
    /// The worktree whose PR pocketd is merging, until it answers.
    pub(crate) merging: Option<String>,
```

Append to `pr.rs`, above `impl Desktop`. The code below needs `PrState`, `std::time::Instant`, `crate::board::parts::{bar_button, start_kbd}`, `crate::git_ui::changes::commit_box::split_button` and `ui::Variant` in the `use` lines of the file. `Mergeable` goes only in the test imports:

```rust
/// The PR to merge when the bar's merge button may fire: the Ticket waits at the PR gate, its PR can merge, and
/// no merge is under way. The findings are not asked: a critical one does not hold the merge.
pub(crate) fn can_merge(t: &Ticket, pr: Option<&Pr>, merging: bool) -> Option<u32> {
    let pr = pr?;
    (t.gate == Waits::Pr && pr.gate() == Gate::Ready && !merging).then_some(pr.number)
}

/// Whether **Request changes** asks the lead for a new team plan. At the PR gate it is a plain prompt.
pub(crate) fn replans(gate: Waits) -> bool {
    gate == Waits::Plan
}
```

Add to `impl Desktop` in `pr.rs`:

```rust
    /// Merges the PR of the Factory ticket `id` through pocketd, with the method the PR card remembers.
    pub(crate) fn factory_merge(&mut self, id: &str, cx: &mut Context<Self>) {
        let merging = self.factory.merging.is_some();
        let Some((tree, number)) = self.agents.board.get(id).and_then(|t| Some((t.worktree.clone(), can_merge(t, self.prs.get(&t.worktree), merging)?))) else { return };
        self.outbox.local.gh_merge(&tree, number, wire(self.pr.track.method));
        self.factory.merging = Some(tree);
        self.factory.merge_menu = false;
        cx.notify();
    }

    /// pocketd merged `tree`'s PR: show it merged now, and ask gh for it again.
    pub(crate) fn factory_merged(&mut self, tree: &str, cx: &mut Context<Self>) {
        self.factory.merging = None;
        self.prs.edit(tree, |pr| pr.state = PrState::Merged);
        self.prs.expire(tree, Instant::now());
        self.poll_prs(cx);
        cx.notify();
    }

    /// What the bar says beside **PR review**: `#203 · +96 -4 · 7 files`.
    pub(crate) fn factory_pr_detail(&self, t: &Ticket) -> String {
        self.prs.get(&t.worktree).map_or(format!("#{}", t.pr), size)
    }

    /// The bar's buttons at the PR gate: why the merge is off, **Request changes**, and the merge button with its
    /// method menu.
    pub(crate) fn factory_pr_buttons(&self, t: &Ticket, gate: Gate, cx: &mut Context<Self>) -> Div {
        let method = self.pr.track.method;
        let busy = self.factory.merging.as_deref() == Some(t.worktree.as_str());
        let ready = !self.agents.observe_only() && can_merge(t, self.prs.get(&t.worktree), self.factory.merging.is_some()).is_some();
        let id = t.id.clone();
        let lead = if busy { spinner("factory-merging", 13., ON_PRIMARY).into_any_element() } else { icon("merge", 14., ON_PRIMARY).into_any_element() };
        let label = div().flex().items_center().gap(px(6.)).child(if busy { "Merging…" } else { method.label() }).child(start_kbd());
        let menu = div()
            .id("factory-merge-menu")
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.factory.merge_menu = false;
                cx.notify();
            }))
            .children(Method::ALL.into_iter().enumerate().map(|(i, m)| {
                ui::menu_row(("factory-merge-method", i), "merge", m.label(), None).children((m == method).then(|| icon("check", 13., TEXT_2))).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.pr.track.method = m;
                    this.factory.merge_menu = false;
                    cx.notify();
                }))
            }));
        let merge = split_button(
            "factory-merge",
            lead,
            label,
            ready,
            self.factory.merge_menu,
            Box::new(move |this, _, cx| this.factory_merge(&id, cx)),
            Box::new(|this, _, _| this.factory.merge_menu = !this.factory.merge_menu),
            menu,
            cx,
        );
        let why = blocked(gate).filter(|_| !busy).map(|why| div().flex_none().text_size(px(12.)).text_color(TEXT_3).child(why));
        let changes = bar_button("factory-pr-changes", Variant::Secondary, None, "Request changes").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.factory_request_changes(window, cx)));
        div().flex().flex_none().items_center().gap(px(8.)).children(why).child(changes).child(merge.w(px(232.)))
    }
```

In `bar.rs`:

1. In `decision`, return the PR decision for a Ticket whose gate is `Pr`. Put it beside the arm of the Plan gate:

```rust
    if t.gate == Gate::Pr {
        return Some(Decision::Pr { gate: pr.map_or(git::github::Gate::Checking, git::github::Pr::gate), number: t.pr });
    }
```

2. Where the bar draws a `Decision`, give `Decision::Pr { gate, .. }` the same shell as the `Decision::Plan` arm: the waiting dot, the label `PR review`, the detail `self.factory_pr_detail(t)`, and the buttons `self.factory_pr_buttons(t, gate, cx)`.

3. In `factory_primary`, the `⌘↵` handler, add the arm (`id` is the open Ticket's id):

```rust
            Some(Decision::Pr { .. }) => {
                self.factory_merge(&id, cx);
                true
            }
```

4. Where plan 3 sends the **Request changes** note, replace the literal `true` of `Outboxes::prompt(lead, text, true)` with `super::pr::replans(t.gate)`. The Ticket then goes back to Implement by itself: pocketd sees the lead start a turn (Task 3.2, row 3).

In `desktop/alerts.rs`, before the arm of `Event::AutomationError` (`:143`), add the merge's answer. In the arm where plan 1 turns `Event::FactoryError` into a toast, add the line `self.factory.merging = None;` first, so a refused merge gives the button back:

```rust
        if let Event::GhMerged(tree) = &ev {
            return self.factory_merged(tree, cx);
        }
```

In `Desktop::close_menus`, add `self.factory.merge_menu = false;`.

The method menu opens below the button, as `split_button` places it. `ui::dropdown` keeps a menu inside the window, so at the bottom of the page it moves up over the bar.

**Step 4: Run and see them pass**

Run: `cargo test -p pocket factory:: && cargo clippy -p pocket --all-targets`
Expected: `test result: ok`, and clippy prints no new warning.

### Task 4.5: The Merged rail and the card of a merged Ticket

**Files:**
- Modify: `packages/desktop/crates/pocket/src/factory/station.rs` (`note`)
- Modify: `packages/desktop/crates/pocket/src/board/card.rs:110-122` (`ticket_pills`)

**Context:** Plan 2's `factory/station.rs` has `column(t, team, pr) -> Column`, which reads the Ticket's `station` when pocketd set one, and `note(t, team, pr, ask) -> Option<(String, Tone)>`, the card's note in the Factory layout. `Tone` is `board::logic::Tone` (`Dim`, `Asking`, `Failed`, `Quiet`). The prototype's merged card shows the merge icon in the `MERGED` colour with `#201`, and the PR's size. `ticket_pills` draws the PR pill from the desktop's PR cache, and the diff pill from the working tree's totals (`card.rs:111-119`). After the engine's commit the working tree is clean, and the cache may not hold a merged PR after the app restarts, so neither pill is sure to show. The note carries the size, and the PR pill reads the number from the Ticket.

**Step 1: Write the failing test** (in the `tests` module of `station.rs`)

```rust
    #[test]
    fn a_merged_ticket_sits_on_the_merged_rail_and_says_its_pr_and_size() {
        let t = Ticket { pr: 201, station: Station::Merged, state: State::Closed, closed_why: ClosedWhy::Merged, ..Ticket::sample() };
        let pr = git::github::Pr { number: 201, state: git::github::PrState::Merged, additions: 96, deletions: 4, files: 7, ..Default::default() };
        assert_eq!(column(&t, None, Some(&pr)), Column::Merged);
        assert_eq!(note(&t, None, Some(&pr), None), Some(("Merged · #201 · +96 -4".to_string(), Tone::Quiet)));
        assert_eq!(note(&t, None, None, None), Some(("Merged · #201".to_string(), Tone::Quiet)));
    }
```

`State` and `ClosedWhy` are the Board's types in `agents::tickets`. If the test module of `station.rs` does not import them, add them to its `use` line.

**Step 2: Run and see it fail**

Run: `cargo test -p pocket factory::station::tests::a_merged_ticket`
Expected: FAIL. `note` returns `None` for a merged Ticket, or plan 2's text for it.

**Step 3: Implement**

In `station.rs`, make the first lines of `note` read:

```rust
    if column(t, team, pr) == Column::Merged && t.pr != 0 {
        return Some((super::pr::merged_line(t.pr, pr), Tone::Quiet));
    }
```

In `card.rs`, before (`:111-115`):

```rust
        let pr = self.prs.get(&t.worktree).map(|pr| {
            let (name, color) = pr_glyph(pr);
            let green = pr.checks.failed == 0 && pr.checks.pending == 0 && pr.checks.passed > 0;
            pill().child(icon(name, 12., color)).child(format!("#{}", pr.number)).when(green, |d| d.child(icon("check", 11., SUCCESS)))
        });
```

After:

```rust
        let merged = t.station == Station::Merged && t.pr != 0;
        let pr = if merged {
            Some(pill().child(icon("merge", 12., MERGED)).child(format!("#{}", t.pr)))
        } else {
            self.prs.get(&t.worktree).map(|pr| {
                let (name, color) = pr_glyph(pr);
                let green = pr.checks.failed == 0 && pr.checks.pending == 0 && pr.checks.passed > 0;
                pill().child(icon(name, 12., color)).child(format!("#{}", pr.number)).when(green, |d| d.child(icon("check", 11., SUCCESS)))
            })
        };
```

Add `Station` to the `agents::tickets` import of `card.rs`. The card already draws a finished column's title in `TEXT_3`, and the Merged rail is plan 2's (`board/stations.rs`, `station_rail`). Nothing else changes there.

**Step 4: Run and see it pass**

Run: `cargo test -p pocket factory::station && cargo clippy -p pocket --all-targets`
Expected: `test result: ok`, and clippy prints no new warning.

### Task 4.6: Capture `factory-ticket-pr`, and the Board before and after

**Files:**
- Modify: `.ui-review/fixture/fixture.ts` (`TICKETS`, the `hello.ok` caps, the `factory.ticket` answer)
- Modify: `packages/desktop/crates/pocket/src/capture.rs` (the step table, after the `prs` step, `:862`)

**Context:** The index lists the capture step `factory-ticket-pr` for this plan. `capture.sh <dir> <name>=<steps>` builds the release app with the `capture` feature, starts the fixture, and runs the steps. A name that starts with `factory` uses the `sessions` scenario, whose `board` frame carries `TICKETS`. The fixture has no `gh`, so the step puts the PR into the desktop's cache the way the `prs` step does (`capture.rs:862-874`): `d.prs.start(&tree, now)` then `d.prs.apply(tree, now, Ok(Some(pr)), now)`. Plan 3's `Desktop::open_factory_ticket(id, window, cx)` opens the page and sends `factory.ticket`, and plan 3's `default_tab(Column::Pr)` opens the PR tab. Plan 3 adds Factory tickets and a `factory.ticket` answer to the fixture. Where its answer covers every Ticket, add the findings below to its answer for `t13` instead of a second handler.

**Step 1: Take the "before" picture**

On the commit before this PR, from the repository root:

```bash
.ui-review/fixture/capture.sh /tmp/lf6-before board-factory=board-factory factory-ticket-checks=factory-ticket-checks
```

Expected: two PNGs in `/tmp/lf6-before`. `board-factory` is plan 2's step and `factory-ticket-checks` is plan 5's. If a step has another name in the merged code, use that name.

**Step 2: Add the fixture's Tickets and answer**

In `fixture.ts`, append two Tickets to `TICKETS` (`:265-278`):

```ts
  ticket("t13", "Fold unchanged hunks in the diff", "live", "needsYou", 6, { station: "pr", gate: "pr", pr: 203, agentId: "lead-t13", worktree: "/tmp/wt/fold-hunks-pr", labels: ["Feature"] }),
  ticket("t14", "Reveal worktree in Finder from the card", "closed", "done", 20, { station: "merged", pr: 201, closedWhy: "merged", closedAt: now - 20 * MIN, worktree: "/tmp/wt/reveal-card" }),
```

In the `hello.ok` frame (`:404`), add `"gh.v1"` to `caps`, after `"team.v1"`.

In the websocket `message` handler, after the `hello` block:

```ts
      if (f.type === "factory.ticket" && f.ticketId === "t13") {
        const at = now - 4 * MIN;
        ws.send(JSON.stringify({ type: "factory.ticket", id: f.id, ticket: {
          ticketId: "t13",
          activity: [{ at, station: "pr", actor: "reviewer", kind: "finding", text: "0 critical · 2 minor · 1 nit" }],
          findings: [
            { severity: "minor", file: "crates/pocket/src/git_ui/diff.rs", line: 214, message: "The fold count is read before the hunks are sorted, so the label can be off by one." },
            { severity: "minor", file: "crates/git/src/diff.rs", line: 88, message: "A hunk of only context lines folds to an empty row." },
            { severity: "nit", message: "Name the threshold FOLD_AFTER instead of 6." },
          ],
        } }));
      }
```

**Step 3: Add the capture step** (in the step table of `capture.rs`, after the `prs` entry)

```rust
    ("factory-ticket-pr", |d, window, cx| {
        let tree = "/tmp/wt/fold-hunks-pr".to_string();
        let run = |name: &str| Check { name: name.into(), outcome: Outcome::Passed, url: String::new() };
        let pr = Pr {
            number: 203,
            title: "Fold unchanged hunks in the diff".into(),
            url: "https://github.com/acme/app/pull/203".into(),
            checks: Checks { passed: 4, ..Checks::default() },
            runs: vec![run("lint"), run("typecheck"), run("test"), run("build")],
            mergeable: Mergeable::Clean,
            base: "main".into(),
            additions: 96,
            deletions: 4,
            files: 7,
            ..Pr::default()
        };
        let now = Instant::now();
        d.prs.start(&tree, now);
        d.prs.apply(tree, now, Ok(Some(pr)), now);
        d.open_factory_ticket("t13", window, cx);
    }),
    ("factory-merge-menu", |d, _, _| d.factory.merge_menu = true),
```

`Check`, `Checks`, `Mergeable`, `Outcome`, `Pr` and `Instant` are already imported in `capture.rs` for the `prs` and `pr-loop` steps.

**Step 4: Take the "after" pictures and compare**

```bash
.ui-review/fixture/capture.sh /tmp/lf6 board-factory=board-factory factory-ticket-checks=factory-ticket-checks factory-ticket-pr=factory-ticket-pr factory-merge-menu=factory-ticket-pr,factory-merge-menu
```

Expected, in `/tmp/lf6`:
- `board-factory.png`: `t13` in the PR review column with its waiting glyph, and `t14` on the Merged rail with the note `Merged · #201` and the merge pill `#201` in the merged colour. The fixture has no PR for `t14`, so its note has no size. The other cards match `/tmp/lf6-before/board-factory.png`.
- `factory-ticket-checks.png`: the same as the "before" picture.
- `factory-ticket-pr.png`, against the prototype's ticket at the PR gate (`.ui-review/prototypes/light-factory/index.html`, the ticket with `gate: 'pr'`): `#203` in mono with the title, the line `into main · 7 files · +96 -4`, the **Reviewer** heading with `0 critical · 2 minor · 1 nit`, the three findings with `crates/…/diff.rs:214` in mono under each message, then the **Merge gate** rows. The bar shows **PR review**, `#203 · +96 -4 · 7 files`, **Request changes**, and `Squash and merge ⌘↵` enabled.

- `factory-merge-menu.png`: the same page with the method menu open, listing `Squash and merge`, `Create a merge commit` and `Rebase and merge`, with a check beside the first.

### Task 4.7: Checks for PR 4

**Files:** none

**Step 1:** From `packages/desktop`, run `cargo build --workspace`, then `cargo clippy --workspace --all-targets`, then `cargo test --workspace`. Expected: the build and the tests pass, and clippy prints no warning that `main` does not print.

**Step 2:** From `packages/pocketd`, run `go test ./internal/proto -count=1`. Expected: `ok`. The desktop's new test reads `client/gh_pr_merge.json`, which pocketd already owns. No golden changes.

**Step 3:** The TypeScript and Swift golden suites: this plan adds no golden and no wire field to a mirrored type, so they fail only on `server/board.json`, as on `main` (issue 227).

**Step 4:** Confirm that the captures of Task 4.6 match the prototype, and keep `/tmp/lf6-before` and `/tmp/lf6` for the PR description.

## Contract changes needed

- **`pocketd packages`, "Hooks and fields that other packages gain", the `broker.Broker.Deny` bullet.** New text: "`broker.Broker.Deny func(proto.PermissionRequest) (reason string, deny bool)` and `Broker.Denied(req) (reason string, deny bool)`, which is nil-safe (plan 6). Each of the three call sites (`daemon.decide`, `codex.Session.ask`, `acp/request.go`) checks `Denied` before `Auto`, answers deny with the reason, never shows the ask, and does not stop the turn. `Auto` returns false for an ask that `Deny` takes."
- **The same section, the `serve.go` bullet.** New text for its `Broker.Allow` part: "`Broker.Allow` is `func(req) bool { return !fac.Never(req) && (teams.Allow(req) || fac.Reads(req)) }` (plans 4 and 6), and `Broker.Deny` is `fac.Deny` (plan 6)."
- **`engine.go`, after `Engine.Deny`.** Add: "`func (e *Engine) Reads(req proto.PermissionRequest) bool // plan 6: a reviewer's read-only command, allowed without an Ask`".
- **"Hooks and fields that other packages gain".** Add: "`github.Service.MergedHere(tree string, number int) bool` (plan 6): `Merge` remembers the PR it merged in each tree, in memory, so the engine notes `Merged` by `you` and anything else as `Merged outside Anywhere`."
- **`Desktop crates`, the Events bullet.** Add: "`Event::GhMerged(String)`, the worktree of a `gh.pr.merge` that pocketd acked. A refused merge is `Event::FactoryError` (plan 6)". In the `agents::factory` bullet, add `Severity` (`Critical`, `Minor`, `Nit`, ordered worst first) beside `Finding`.
- **`Desktop crates`.** Add: "`git::github::Pr` gains `additions`, `deletions` and `files`, read from `gh pr view --json additions,deletions,changedFiles` (plan 6)."

## Open questions

- **How does a test bring a Ticket to `Station: pr` end to end?** The e2e tests of Tasks 2.6 and 3.4 need plan 5's path from Checks to PR review as a helper. This plan calls it `factoryAtPR(t, cfg) (h, o, gh, ticket)`. Recommended: plan 5 writes its e2e path as that helper. If it does not, Task 2.6 extracts it first, with no change of behaviour.
- **Should a reviewer read files without an Ask?** `Engine.Reads` allows only shell commands of the read-only list. An ACP reviewer with access `ask` asks before each `read` and `search`, and each of those asks waits on you while the Ticket reads `working`. Recommended: `Reads` also allows the reviewer's `read` and `search` asks whose path is inside the Ticket's worktree. Reads outside it stay an Ask.
- **Should a reviewer's Ask that pocketd leaves to you set `Gate: ask`?** In this plan the Ticket stays `working`, and the Ask shows in the Inbox as any Ask. Recommended: no gate in v1. Revisit with plan 7's Inbox rows if such Asks prove common.
- **Should the merge method last across launches?** `store::prefs::Git` has no merge method, so the bar and the Changes panel's PR card share one in memory. Recommended: add `merge_method` to `store::prefs::Git` in a follow-up that changes both places together.
- **Should a failed `gh pr ready` offer a retry on the bar?** This plan notes the failure and the bar says `Draft`. Recommended: no button in v1. The Changes panel's PR card has **Ready for review**, and a new head tries `gh pr ready` again.
