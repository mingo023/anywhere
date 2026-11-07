# Team Mode 1: Parent Sessions Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** A session can have a parent session: pocketd launches it under a listed parent, keeps that across restarts and upgrades, and the desktop lists it under its parent in the sidebar with a breadcrumb back from its chat.

**Architecture:** `LaunchSpec.Parent` rides the launch the way `Origin` does: `launch.create` checks the parent is listed, puts it on the `terminal.Spec`, and `daemon.startAgent` hands it to the first agent in that Terminal (`TakeParent`). `AgentSummary.Parent` is persisted through `state.Terminal` → `presence.save` → `agent.Restored` → `Registry.Restore`; an upgrade's handoff carries the same `state.Terminal`, so it needs no change of its own. On the desktop, `agents::Summary.parent` and `status::Card.parent` feed two plain functions in `sidebar/sessions.rs` (`with_children`: a child follows its parent into the parent's tree; `nest`: each card followed by its children, one level deeper), and a new `chat/breadcrumb.rs` draws "<parent title> › <this title>" over a child's chat.

**Toolset:**
- pocketd (from `packages/pocketd`; once per machine `scripts/build-ghostty.sh` for the cgo vt): one package `go test ./internal/proto`, one test `go test ./internal/launch -run TestACreateUnder`, everything `go test ./...`, regenerate goldens `go test ./internal/proto -run TestServerGolden -update`
- protocol (from the repo root): `pnpm --filter @pocket/protocol test`
- iOS wire (from `packages/ios/Packages/AnywhereKit`): `swift test --scratch-path /tmp/ak-parent --filter WireTests`
- desktop (from `packages/desktop`): one crate `cargo test -p pocket sessions`, `cargo test -p hosts`, `cargo test -p agents`; everything `cargo build --workspace`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`; capture build `cargo build -p pocket --features capture`
- screens (from the repo root): `.ui-review/fixture/capture.sh <dir> <name>=<steps>…` (needs `bun`)

**Read first:**
- `docs/plans/2026-11-05-team-mode.md`: the contracts this plan implements (`LaunchSpec.Parent`, `AgentSummary.Parent`, `agents::Summary.parent`, `tag_summary`). Don't change them here.
- `CLAUDE.md` and `docs/adr/0003-desktop-code-layout.md`: decisions in plain functions with tests beside them, sentence test names, no mocks, no render tests, one file per component.
- `packages/pocketd/internal/daemon/presence.go` `startAgent` (:65-119) and `internal/terminal/terminal.go` `TakeOrigin` (:427-435): the path `Origin` takes from a launch to its agent. `Parent` copies it.
- `packages/pocketd/internal/daemon/restore.go` `Snapshot` (:74) and `expect` (:148), and `internal/daemon/handoff.go`: restart and upgrade both go through `state.Terminal`.
- `packages/desktop/crates/pocket/src/sidebar/sessions.rs` and `desktop/jump.rs` `walkable`: the session list and the order ⌃n / ⌃1–9 walk.

## Decisions

- **Who sets it.** The owner only: the desktop and pocketd itself (plan 4 launches workers as the owner). A phone create that names a parent is refused with `access_not_allowed`. No new verb; plan 1 adds no desktop UI that sends it.
- **Unknown parent.** `agent.create` with a `parent` that isn't a listed agent fails with code `unknown_parent`, message `The session to start this one under has closed`, before any worktree is made.
- **The agent takes it once.** Like `Origin`, only the first agent in the Terminal takes the parent; one typed later in the same Terminal has none.
- **Restart and upgrade keep it.** Saved in `state.Terminal.Parent`. A restore doesn't check that the parent came back: a missing parent just leaves the child at the top level on the desktop.
- **Sidebar.** A card whose parent is listed in the same session list is drawn under it, indented 16px per level, in the parent's position; the children keep the order the sort gave them. A child running in another worktree still follows its parent into the parent's tree, and is also listed, at the top level, in its own tree (its parent isn't listed there). A child whose parent is gone, or filtered out by the search, is at the top level. The Pinned box holds a pinned parent with its children. ⌃n, ⌃Tab and the ⌃1–9 chips walk the rows in the order drawn.
- **Breadcrumb.** A strip over a child's chat: the parent's title (muted, a button that focuses the parent), `›`, the child's title. None when the parent isn't listed.
- **Phone.** The iOS and TS wire models learn the field so their golden tests keep passing; the phone doesn't nest sessions (out of scope).

---

## PR 1: Parent sessions

**Scope:** pocketd launches, lists, saves and restores a session's parent; protocol schemas (TS, Swift) decode it; the desktop nests children under their parent in the sidebar and shows a breadcrumb on a child's chat. Nothing launches a child yet (plan 4 does), so on a real pocketd the desktop change shows only once a spec names a parent; the fixture shows it now.
**Depends on:** nothing
**Done when:** `cd packages/pocketd && go test ./...`, `pnpm --filter @pocket/protocol test`, `swift test --scratch-path /tmp/ak-parent --filter WireTests` (in `packages/ios/Packages/AnywhereKit`), and from `packages/desktop` `cargo build --workspace`, `cargo clippy --workspace --all-targets` (no new warnings), `cargo test --workspace` all pass, and the `team-sidebar` / `team-worker` captures show the nesting and the breadcrumb.

### Task 1.1: Wire: `LaunchSpec.Parent` and `AgentSummary.Parent`

**Files:**
- Modify: `packages/pocketd/internal/proto/launch.go:31-46` (`LaunchSpec`), `:171` (`decodeSpec` allowlist)
- Modify: `packages/pocketd/internal/proto/proto.go:193-231` (`AgentSummary`)
- Modify: `packages/pocketd/internal/proto/golden_test.go` (`serverGolden`, `TestDecodeClientAcceptsWhatTheSchemaAccepts`, new test)
- Create (generated): `packages/pocketd/internal/proto/testdata/golden/server/agent_update_parent.json`
- Modify: `packages/protocol/src/timeline.ts:112-144` (`AgentSummary`)
- Modify: `packages/ios/Packages/AnywhereKit/Sources/Wire/Agents.swift:36-119` (`AgentSummary`)

**Context:** `decodeSpec` rejects any key not in its `strictObject` allowlist, so a new `LaunchSpec` key must be added there. `AgentSummary` must stay `==`-comparable (`agent.update` compares before/after), so `Parent` is a plain string. Put it last in the struct: JSON keys follow field order and every existing golden stays byte-identical because `omitempty` drops it. The TS test (`packages/protocol/test/golden.test.mjs`) and the iOS `WireTests/GoldenTests.swift` read pocketd's golden files directly: the TS one fails on any excess property, the iOS one round-trips the JSON. So the new server golden turns both red until their schemas gain `parent`. Do **not** add a client golden for a create with `parent`: client goldens are "every message the phone may send", and the phone may not send it; test the decode in Go instead. The TS `LaunchSpec` stays unchanged for the same reason.

**Step 1: Write the failing tests**

In `golden_test.go`, add to `serverGolden` after `"agent_update_chat_only"`:

```go
	"agent_update_parent":          NewAgentUpdate(with(func(s *AgentSummary) { s.Parent = "a0" })),
```

Add this row at the end of the list in `TestDecodeClientAcceptsWhatTheSchemaAccepts`:

```go
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"worktree":"/w"},"provider":"claude","access":"ask","plan":false,"chat":true,"parent":"a1"}}`,
```

Add this test after `TestDecodeClientAcceptsWhatTheSchemaAccepts` (it proves the key decodes into the field, not just that it's allowed):

```go
func TestASpecNamesTheSessionItStartsUnder(t *testing.T) {
	m, err := DecodeClient([]byte(`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"worktree":"/w"},"provider":"claude","access":"ask","plan":false,"parent":"a1"}}`))
	if err != nil || m.Spec.Parent != "a1" {
		t.Fatalf("spec = %+v, %v", m.Spec, err)
	}
}
```

**Step 2: Run to verify it fails**

Run: `cd packages/pocketd && go test ./internal/proto`
Expected: build fails with `s.Parent undefined (type *AgentSummary has no field or method Parent)` and `m.Spec.Parent undefined`.

**Step 3: Implement**

`launch.go`, in `LaunchSpec` after `Chat`:

```go
	// Chat runs the agent for the chat where its provider can; without it, in its TUI.
	Chat bool `json:"chat,omitempty"`
	// Parent is the id of the listed agent this one starts under, as a team's
	// worker under its lead; empty for none. Only the owner may set it.
	Parent string `json:"parent,omitempty"`
}
```

`decodeSpec`'s first line becomes:

```go
	spec, ok := strictObject(raw, "project", "checkout", "provider", "model", "effort", "access", "plan", "prompt", "fork", "account", "chat", "parent")
```

`proto.go`, in `AgentSummary` after `Goal`:

```go
	// Goal is what the agent keeps working toward across turns, set by /goal.
	Goal Goal `json:"goal,omitzero"`
	// Parent is the agent this one was started under, as a team's worker under its lead; empty for none.
	Parent string `json:"parent,omitempty"`
}
```

Generate the golden: `go test ./internal/proto -run TestServerGolden -update`. Check `internal/proto/testdata/golden/server/agent_update_parent.json` is exactly:

```json
{
  "type": "agent.update",
  "agent": {
    "id": "a1",
    "terminalId": "t1",
    "title": "fix tests",
    "cwd": "/w",
    "provider": "claude",
    "status": "idle",
    "attached": true,
    "epoch": 1,
    "maxSeq": 3,
    "providerSessionId": "s1",
    "createdAt": 1,
    "updatedAt": 2,
    "project": "pocket",
    "worktree": "pocket",
    "mainWorktree": true,
    "branch": "main",
    "tokensUsed": 48000,
    "contextWindow": 1000000,
    "origin": "desktop",
    "parent": "a0"
  }
}
```

and `git status --short packages/pocketd/internal/proto/testdata` lists only that new file (no other golden changed).

**Step 4: Run to verify it passes**

Run: `cd packages/pocketd && go test ./internal/proto`
Expected: `ok  	pocketd/internal/proto`

**Step 5: The TS and Swift schemas (red, then green)**

Run: `pnpm --filter @pocket/protocol test`
Expected: FAIL on `server/agent_update_parent.json` with `is unexpected` naming `"parent"`.

`packages/protocol/src/timeline.ts`, in `AgentSummary` after `goal`:

```ts
  goal: Schema.optional(Goal),
  parent: Schema.optional(Schema.String),
});
```

Run: `pnpm --filter @pocket/protocol test`
Expected: `# fail 0`

Run: `cd packages/ios/Packages/AnywhereKit && swift test --scratch-path /tmp/ak-parent --filter WireTests`
Expected: FAIL in `serverGolden` for `agent_update_parent.json` (the re-encoded JSON lacks `parent`).

`Agents.swift`, in `AgentSummary`: after `public var goal: Goal?` add

```swift
    /// The agent this one was started under, as a team's worker under its lead.
    public var parent: String?
```

the init's last parameter line becomes

```swift
        switching: String? = nil, chatOnly: Bool? = nil, goal: Goal? = nil, parent: String? = nil
```

and after `self.goal = goal` add `self.parent = parent`.

Run: `swift test --scratch-path /tmp/ak-parent --filter WireTests`
Expected: all WireTests pass.

### Task 1.2: The first agent in a Terminal takes its parent

**Files:**
- Modify: `packages/pocketd/internal/terminal/terminal.go:28-41` (`Spec`), `:70` (struct), `:154` (`Spawn`), after `:435` (`TakeParent`)
- Modify: `packages/pocketd/internal/agent/agent.go` (field, `summary()`, `SetParent`)
- Modify: `packages/pocketd/internal/daemon/presence.go:85-110` (`startAgent`)
- Test: `packages/pocketd/internal/terminal/terminal_test.go`, `packages/pocketd/internal/daemon/place_test.go`

**Context:** pocketd's poller finds an agent process in a Terminal and calls `startAgent`, which creates the `agent.Agent` and copies launch facts onto it. `Origin` gets there through `terminal.Spec.Origin` → `Terminal.TakeOrigin()` (once, so a later agent typed into the same Terminal isn't credited to the launch). `Parent` takes the same route. A restored or handed-down Terminal is spawned without a parent; its agent gets the parent from `agent.Restored` instead (Task 1.4), and `startAgent` leaves it alone because `TakeParent` returns `""`. Agent setters go through `a.update(false, …)`, which publishes one `agent.update` only when the summary changed.

**Step 1: Write the failing tests**

`terminal_test.go`, after `TestTheOriginIsTakenOnce`:

```go
func TestTheParentIsTakenOnce(t *testing.T) {
	s, err := NewManager().Spawn(Spec{Cmd: "sleep", Args: []string{"5"}, Parent: "a-lead"})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(s.Close)
	if first, second := s.TakeParent(), s.TakeParent(); first != "a-lead" || second != "" {
		t.Fatalf("took %q then %q", first, second)
	}
}
```

`place_test.go`, after `TestTheTerminalsOriginGoesToItsFirstAgent`:

```go
func TestTheTerminalsParentGoesToItsFirstAgent(t *testing.T) {
	d := newDaemon(t)
	term, err := d.Terminals.Spawn(terminal.Spec{Cmd: fakeAgent(t, "claude"), Cwd: t.TempDir(), Env: []string{"PATH=/bin:/usr/bin"}, Parent: "a-lead"})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(term.Close)
	if a := waitAgent(t, d, term); a.Parent != "a-lead" {
		t.Fatalf("%+v", a)
	}
}
```

**Step 2: Run to verify they fail**

Run: `cd packages/pocketd && go test ./internal/terminal ./internal/daemon -run 'TestTheParentIsTakenOnce|TestTheTerminalsParentGoesToItsFirstAgent'`
Expected: build fails with `unknown field Parent in struct literal of type terminal.Spec` / `Spec`.

**Step 3: Implement**

`terminal.go`, in `Spec` after `Origin`:

```go
	// Origin is who asked for the Terminal; its first agent takes it.
	Origin string
	// Parent is the agent its first agent was started under; empty for none.
	Parent string
```

In the `Terminal` struct, after `origin string`:

```go
	origin string
	parent string
```

In `Spawn`'s `&Terminal{…}` literal, after `origin: spec.Origin,`:

```go
		origin: spec.Origin,
		parent: spec.Parent,
```

After `TakeOrigin`:

```go
// TakeParent is Spec.Parent once, then "": only the agent the Terminal was
// launched for starts under another.
func (s *Terminal) TakeParent() string {
	s.mu.Lock()
	defer s.mu.Unlock()
	p := s.parent
	s.parent = ""
	return p
}
```

`agent.go`, in the `Agent` struct after `origin string`:

```go
	origin          string
	parent          string // the agent it was started under; empty for none
```

(run `gofmt -w internal/agent/agent.go` to realign). In `summary()`, the last field line becomes:

```go
		TokensUsed: a.tokensUsed, ContextWindow: a.contextWindow, Origin: a.origin, Activity: a.activity, ChatOnly: a.chatOnly, Goal: a.goal, Parent: a.parent,
```

After `SetOrigin`:

```go
func (a *Agent) SetParent(id string) { a.update(false, func() { a.parent = id }) }
```

`presence.go`, in `startAgent`: after `origin := t.TakeOrigin()` add `parent := t.TakeParent()`, and after the `if origin != "" { pr.a.SetOrigin(origin) }` block add:

```go
	if parent != "" {
		pr.a.SetParent(parent)
	}
```

**Step 4: Run to verify they pass**

Run: `cd packages/pocketd && go test ./internal/terminal ./internal/daemon -run 'TestTheParentIsTakenOnce|TestTheTerminalsParentGoesToItsFirstAgent|TestTheTerminalsOriginGoesToItsFirstAgent'`
Expected: `ok` for both packages.

### Task 1.3: Launch: only the owner nests, under a listed parent

**Files:**
- Modify: `packages/pocketd/internal/launch/policy.go:9-21` (`Check`)
- Modify: `packages/pocketd/internal/launch/launch.go:132-140` (`create`, after the project check), `:193-194` (`terminal.Spec`)
- Test: `packages/pocketd/internal/launch/policy_test.go`, `packages/pocketd/internal/launch/launch_test.go`

**Context:** `Launcher.Create` → `create` runs `Check` (what a non-owner may ask for; the owner passes everything), then the project check, then accounts, the executable, `checkout` (which may make a worktree), then spawns the Terminal. The parent check goes right after the project check so a bad parent never makes a worktree. `l.d.Agents.Get(id)` returns an error for an agent that isn't listed; a closed agent is removed from the registry, so it fails too. The `launcher(t, …)` helper builds a `Launcher` over `&daemon.Daemon{}`, whose `Agents` is nil: tests that reach the parent check must set `l.d` with a registry, as `TestACreateInALocalRunsInTheProjectAndCarriesTheLocal` does.

**Step 1: Write the failing tests**

`policy_test.go`, in `TestThePhoneMayStartOnlyWhatTheOwnerAllows`: after the `newTree` helper add

```go
	under := func(parent string) proto.LaunchSpec {
		s := spec("claude", "ask", false)
		s.Parent = parent
		return s
	}
```

and add these rows at the end of the table:

```go
		{"owner parent", owner, under("a1"), "ask", ""},
		{"phone parent", phone, under("a1"), "ask", "Only your Mac starts a session under another"},
```

`launch_test.go`, after `TestACreateInADeletedOrForeignLocalFails`:

```go
func TestACreateUnderASessionThatClosedFails(t *testing.T) {
	project := t.TempDir()
	l := launcher(t, `["`+project+`"]`)
	l.d = &daemon.Daemon{Agents: agent.NewRegistry(hub.New())}
	s := spec("claude", "ask", false)
	s.Project, s.Parent = project, "gone"
	r := l.Create(Who{Owner: true, Key: "owner"}, "r1", s, quiet, func(Creating) { t.Fatal("creating") })
	if r.Err == nil || r.Err.Code != "unknown_parent" || r.Err.Message != "The session to start this one under has closed" {
		t.Fatalf("got %+v", r.Err)
	}
}

func TestACreateUnderAListedSessionHandsItsTerminalTheParent(t *testing.T) {
	defer func(w time.Duration) { createWait = w }(createWait)
	createWait = 500 * time.Millisecond
	onPath(t, "claude")
	t.Setenv("SHELL", "/bin/sh")
	project, _ := filepath.EvalSymlinks(t.TempDir())
	git(t, project, "init", "-q", "-b", "main")
	git(t, project, "commit", "-q", "--allow-empty", "-m", "init")
	l := launcher(t, `["`+project+`"]`)
	l.d = &daemon.Daemon{Terminals: terminal.NewManager(), Agents: agent.NewRegistry(hub.New()), Exe: "/bin/true"}
	t.Cleanup(func() { l.d.Terminals.CloseAll(0) })
	l.d.Agents.Add("lead", project, "claude", nil)
	s := spec("claude", "ask", false)
	s.Project, s.Checkout.Worktree, s.Parent = project, project, "lead"
	var parent string
	l.Create(Who{Owner: true, Key: "owner"}, "r1", s, quiet, func(c Creating) { parent = l.d.Terminals.Get(c.Terminal).TakeParent() })
	if parent != "lead" {
		t.Fatalf("parent = %q", parent)
	}
}
```

(The second create ends at `createWait` with `spawn_failed`, as no agent ever shows up; the test only reads what the Terminal was given.)

**Step 2: Run to verify they fail**

Run: `cd packages/pocketd && go test ./internal/launch -run 'TestThePhoneMayStartOnlyWhatTheOwnerAllows|TestACreateUnder'`
Expected: FAIL: `phone parent: got <nil>, want "Only your Mac starts a session under another"`; `TestACreateUnderASessionThatClosedFails` fails with a code other than `unknown_parent`; `parent = ""`.

**Step 3: Implement**

`policy.go`, in `Check` right after `if w.Owner { return nil }`:

```go
	if s.Parent != "" {
		return fail("access_not_allowed", "Only your Mac starts a session under another")
	}
```

`launch.go`, in `create` right after the `unknown_project` check:

```go
	if s.Parent != "" {
		if _, err := l.d.Agents.Get(s.Parent); err != nil {
			return Result{Err: fail("unknown_parent", "The session to start this one under has closed")}
		}
	}
```

and the spawn's `terminal.Spec` literal gains `Parent`:

```go
	t, err := l.d.Terminals.Spawn(terminal.Spec{ID: id, Cmd: shell, Args: Wrap(shell, l.d.Exe, setup, argv, account), Cwd: cwd,
		Env: worktree.ScriptEnv(l.d.Env(l.d.GhEnv(env, cwd), id, local), s.Project, cwd), Cols: 100, Rows: 30, Origin: origin, Parent: s.Parent, Local: local})
```

**Step 4: Run to verify they pass**

Run: `cd packages/pocketd && go test ./internal/launch`
Expected: `ok  	pocketd/internal/launch`

### Task 1.4: A restart or an upgrade keeps the parent

**Files:**
- Modify: `packages/pocketd/internal/state/state.go:25-49` (`Terminal`)
- Modify: `packages/pocketd/internal/daemon/presence.go:369-381` (`save`)
- Modify: `packages/pocketd/internal/agent/agent.go:157-181` (`Restored`, `Registry.Restore`)
- Modify: `packages/pocketd/internal/daemon/restore.go:150-151` (`expect`)
- Test: `packages/pocketd/internal/daemon/restore_test.go`

**Context:** On shutdown pocketd writes `state/terminals.json` from `Snapshot()`, which asks each live agent's presence to `save` into a `state.Terminal` (only once the agent has a conversation). On start, `Restore` → `expect` builds an `agent.Restored` from each entry and `Registry.Restore` lists the agent under its old id. An upgrade's handoff (`daemon/handoff.go`) writes `Snapshot()`'s entries as `handoff.Terminal.Saved` and the new binary's `Adopt` calls the same `expect`, so these two edits cover it; `handoff.Agent` doesn't change. A new optional JSON field needs no `state.Version` bump.

**Step 1: Write the failing tests**

`restore_test.go`, after `TestTheFileKeepsWhoStartedTheAgent`:

```go
func TestTheFileKeepsTheSessionAnAgentWasStartedUnder(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	pr.a.SetParent("a-lead")
	hookFrom(d, pr, sessionStart("s1", transcript(t, "hi")))
	if e := d.Snapshot().Terminals[0]; e.Parent != "a-lead" {
		t.Fatalf("entry = %+v", e)
	}
}

func TestAnAgentComesBackUnderTheSessionItWasStartedUnder(t *testing.T) {
	d := newDaemon(t)
	resuming(d, "/bin/sh", "-c", "sleep 30")
	saved := savedAgent(t.TempDir(), "claude")
	saved.Terminals[0].Parent = "a-lead"
	d.Restore(saved, "/bin/sh")
	_, a := restoredIn(t, d)
	if s := a.Summary(); s.Parent != "a-lead" {
		t.Fatalf("parent = %q", s.Parent)
	}
}
```

**Step 2: Run to verify they fail**

Run: `cd packages/pocketd && go test ./internal/daemon -run 'TestTheFileKeepsTheSessionAnAgentWasStartedUnder|TestAnAgentComesBackUnderTheSessionItWasStartedUnder'`
Expected: build fails with `e.Parent undefined (type state.Terminal has no field or method Parent)`.

**Step 3: Implement**

`state.go`, in `Terminal` after `Chat`:

```go
	// Chat is whether the agent ran behind `pocketd pipe`, driven only by the chat.
	Chat bool `json:"chat,omitempty"`
	// Parent is the agent this one was started under; empty for none.
	Parent string `json:"parent,omitempty"`
```

`presence.go`, the last line of `save` becomes:

```go
	e.Account, e.Typed, e.Chat, e.Parent = s.Account, pr.typed, pr.stream != nil, s.Parent
```

`agent.go`, in `Restored` after `ChatOnly bool`:

```go
	ChatOnly                                  bool
	Parent                                    string
```

(gofmt realigns), and in `Registry.Restore`'s `&Agent{…}` literal, the line ending `chatOnly: x.ChatOnly, attached: true,` becomes `chatOnly: x.ChatOnly, parent: x.Parent, attached: true,`.

`restore.go`, in `expect`, the `agent.Restored{…}` literal ends `… Account: e.Account, ChatOnly: e.Chat, Parent: e.Parent}`.

**Step 4: Run to verify they pass**

Run: `cd packages/pocketd && go test ./internal/daemon ./internal/agent ./internal/state ./internal/handoff`
Expected: `ok` for each package.

Then the whole daemon: `cd packages/pocketd && go test ./...` → every package `ok`.

### Task 1.5: Desktop model, fixture, and the "before" screens

**Files:**
- Modify: `packages/desktop/crates/agents/src/agents.rs:23-57` (`Summary`), tests near `:1355`
- Modify: `packages/desktop/crates/hosts/src/session.rs:101-109` (`tag_summary`), tests
- Modify: `packages/desktop/crates/pocket/src/status.rs:52-91` (`Card`, `card`, new `title`)
- Modify (test literals gain `parent: String::new()`): `packages/desktop/crates/pocket/src/add_to_chat.rs:361`, `packages/desktop/crates/pocket/src/palette.rs:518`
- Modify: `packages/desktop/crates/pocket/src/capture.rs:28-36` (two steps)
- Modify: `.ui-review/fixture/fixture.ts` (`Agent` type, `team` scenario, `summaries`), `.ui-review/fixture/capture.sh` (scenario case)

**Context:** `agents::Summary` is serde `default` + camelCase, so `parent` decodes from the wire with no other change. A host's pocketd ids are tagged `host/id` by `hosts::session::tag_summary` so they don't collide with this Mac's; a parent is an agent id on the same pocketd, so it's tagged the same way (only when non-empty: an empty parent must stay empty, not become `devbox/`). `status::Card` is the sidebar's plain row type; it gains `parent` so the nesting functions in Task 1.6 can work on cards alone. The capture fixture (`.ui-review/fixture/fixture.ts`) fakes pocketd from a scenario: each agent with a `branch` gets its own worktree. A new `team` scenario puts a lead in `refactor/fold` and two children in their own worktrees `team-parser-t1` / `team-parser-t2` (the plan-4 naming, `team-<lead6>-<task>`). `capture.sh` picks the scenario from the screen name's prefix. Two capture steps are added: `lead` (focus the first child's parent) and `worker-chat` (focus the first child and show its chat). They're added before the view changes so the same steps capture "before" and "after".

**Step 1: Write the failing tests**

`agents.rs` tests, after `a_summary_shows_the_account_it_switches_to`:

```rust
    #[test]
    fn a_summary_names_the_session_it_was_started_under() {
        let f: Frame = serde_json::from_str(include_str!("../../../../pocketd/internal/proto/testdata/golden/server/agent_update_parent.json")).unwrap();
        assert_eq!(f.agent.unwrap().parent, "a0");
    }
```

`hosts/src/session.rs` tests, after `a_hosts_agent_events_are_tagged_and_its_phone_and_pairing_news_dropped`:

```rust
    #[test]
    fn a_hosts_session_names_its_parent_by_the_tagged_id_and_one_without_stays_without() {
        let mut child = Event::Agent(Summary { id: "w1".into(), parent: "a1".into(), ..Summary::default() });
        assert!(tag_event("devbox", &mut child));
        assert!(matches!(child, Event::Agent(a) if a.id == "devbox/w1" && a.parent == "devbox/a1"));

        let mut lead = Event::Agent(Summary { id: "a1".into(), ..Summary::default() });
        assert!(tag_event("devbox", &mut lead));
        assert!(matches!(lead, Event::Agent(a) if a.parent.is_empty()));
    }
```

**Step 2: Run to verify they fail**

Run: `cd packages/desktop && cargo test -p agents a_summary_names_the_session && cargo test -p hosts a_hosts_session_names_its_parent`
Expected: build fails with `no field 'parent' on type 'Summary'`.

**Step 3: Implement**

`agents.rs`, in `Summary` after `goal`:

```rust
    /// What `/goal` set it working toward; none when no goal is set.
    pub goal: Option<Goal>,
    /// The session it was started under, as a team's worker under its lead; empty for none.
    pub parent: String,
}
```

`session.rs`, `tag_summary` gains, after the `terminal_id` block:

```rust
    if !a.parent.is_empty() {
        a.parent = tag(host, &a.parent);
    }
```

`status.rs`: in `Card` after `account`:

```rust
    /// The account it runs on; empty is the built-in login.
    pub account: String,
    /// The session it was started under; empty for none.
    pub parent: String,
}
```

In `card`, replace the `title:` line with `title: title(a),` and add `parent: a.parent.clone(),` after `account: …`. Add above `fn model`:

```rust
/// The title a session shows: its own, else "New session".
pub fn title(a: &Summary) -> String {
    if a.title.is_empty() { "New session".into() } else { a.title.clone() }
}
```

`add_to_chat.rs:361` and `palette.rs:518`: append `, parent: String::new()` after `account: String::new()` in each `Card { … }` literal. If the compiler names another `Card` or `Summary` literal without `..`, add `parent: String::new()` there too.

Run: `cd packages/desktop && cargo test -p agents && cargo test -p hosts && cargo test -p pocket status`
Expected: all pass.

**Step 4: Fixture and capture steps**

`fixture.ts`: in the `Agent` type, add `parent?: string;` after `restore?: string;`:

```ts
  diff?: [string, number, number][]; children?: string[][]; term?: string; restore?: string; parent?: string;
```

After the `SCENARIOS.chat = {…};` block add:

```ts
// A lead with two workers, each in its own worktree as team mode makes them.
SCENARIOS.team = {
  repos: ["app-android", "app-ios", "app-hook", "marketing"],
  agents: [
    { id: "parser", title: "Refactor the transcript fold", provider: "claude", model: "opus", effort: "high", repo: "app-android", branch: "refactor/fold", status: "idle", ago: 1 },
    { id: "fold-split", parent: "parser", title: "Split fold.rs by block kind", provider: "claude", repo: "app-android", branch: "team-parser-t1", status: "running", ago: 0 },
    { id: "fold-tests", parent: "parser", title: "Cover the fold with tests", provider: "codex", repo: "app-android", branch: "team-parser-t2", status: "idle", ago: 0 },
    ...OTHERS,
  ],
};
```

In `const summaries = scenario.agents.map((a) => ({ … }))`, add `parent: a.parent,` after `restore: a.restore,` (an `undefined` value is left out of the JSON).

`capture.sh`, in the `case`, add a line before `*) scenario=sessions ;;`:

```sh
    team*) scenario=team ;;
```

`capture.rs`: change `const STEPS: [(&str, Step); 229]` to `231`, and after the `("chat", …)` entry add:

```rust
    ("lead", |d, window, cx| {
        if let Some(lead) = d.agents.list.iter().find(|a| !a.parent.is_empty()).map(|a| a.parent.clone()) {
            d.focus_agent(&lead, window, cx);
        }
    }),
    ("worker-chat", |d, window, cx| {
        let Some(worker) = d.agents.list.iter().find(|a| !a.parent.is_empty()).map(|a| a.id.clone()) else { return };
        d.focus_agent(&worker, window, cx);
        if let Some(Tab::Term(t)) = d.pane_tab(d.focused_pane()) {
            d.store.chats.insert(t.clone());
            d.focus_chat_or_terminal(t, window, cx);
        }
    }),
```

Run: `cd packages/desktop && cargo build -p pocket --features capture`
Expected: builds (if the count is off, the error names the right one).

**Step 5: Capture "before"**

Run (repo root): `.ui-review/fixture/capture.sh /tmp/team-parent-before team-sidebar=lead team-worker=worker-chat chat=chat`
Expected: `/tmp/team-parent-before/impl-team-sidebar.png` shows the `refactor/fold` tree's Sessions list with only "Refactor the transcript fold" (the workers sit in their own trees); `/tmp/team-parent-before/impl-team-worker.png` shows "Split fold.rs by block kind"'s chat with no breadcrumb. Keep both for Task 1.7.

### Task 1.6: Sidebar: children listed under their parent

**Files:**
- Modify: `packages/desktop/crates/pocket/src/sidebar/sessions.rs` (new `with_children`, `nest`, `pinned_len`; `session_list`; `card`; tests)
- Modify: `packages/desktop/crates/pocket/src/desktop/jump.rs:1-24` (`walkable`), `:51-59`, `:86`, `:91`, tests
- Modify: `packages/desktop/crates/pocket/src/sidebar/panel.rs:89`

**Context:** The Sessions column (`session_list`) draws `visible_sessions`, which is `walkable(self.cards(project), tree, tree_of, query)`: the project's cards (already sorted by the Sort sessions pref), kept to the shown tree by `sidebar::in_tree`, filtered by the search (`matching`), pinned first. The same order drives ⌃n / ⌃Tab (`step_session`) and the ⌃1–9 chips (`jump_to`, and `card(i, …)` draws chip `i + 1`), so nesting must happen inside `walkable` and the rows keep their drawn index. `walkable` now returns `(Card, depth)` pairs. Three plain functions carry the decisions, tested beside them:
- `with_children(cards, tree, tree_of)`: `in_tree`, plus every card started under a card already kept (repeatedly), wherever it runs. Order of `cards` is kept.
- `nest(cards)`: each card whose parent isn't among `cards` is a top-level row; right after each row come its children, a level deeper, in the order given. (Parents are always older than their children, so ids never form a cycle.)
- `pinned_len(rows)`: the Pinned box takes the leading rows up to the first unpinned top-level row, so a pinned parent's children go in the box with it.

**Step 1: Write the failing tests**

In `sessions.rs`, the tests module becomes:

```rust
#[cfg(test)]
mod tests {
    use super::{matching, nest, pinned_len, with_children};
    use crate::status::{self, Card};
    use agents::Summary;

    fn card(title: &str, status: &str) -> Card {
        status::card(&Summary { id: title.into(), title: title.into(), status: status.into(), attached: true, ..Default::default() }, "/p")
    }

    fn child(id: &str, parent: &str) -> Card {
        Card { parent: parent.into(), ..card(id, "idle") }
    }

    fn at(c: Card, cwd: &str) -> Card {
        Card { cwd: cwd.into(), ..c }
    }

    fn titles(cards: Vec<Card>) -> Vec<String> {
        cards.into_iter().map(|c| c.title).collect()
    }

    fn rows(rows: Vec<(Card, usize)>) -> Vec<(String, usize)> {
        rows.into_iter().map(|(c, depth)| (c.id, depth)).collect()
    }

    #[test]
    fn search_keeps_the_sessions_whose_title_matches_in_any_case() {
        let cards = vec![card("Fix CI", "idle"), card("Docs", "working"), card("fix login", "done")];
        assert_eq!(titles(matching(cards, "FIX")), vec!["Fix CI", "fix login"]);
    }

    #[test]
    fn a_status_change_keeps_the_session_order() {
        let before = vec![card("a", "working"), card("b", "idle"), card("c", "working")];
        let after = vec![card("a", "done"), card("b", "needsYou"), card("c", "idle")];
        assert_eq!(titles(matching(before, "")), vec!["a", "b", "c"]);
        assert_eq!(titles(matching(after, "")), vec!["a", "b", "c"]);
    }

    #[test]
    fn children_are_listed_under_their_parent_one_level_in_in_the_order_given() {
        let cards = vec![child("w2", "lead"), card("lead", "idle"), card("other", "idle"), child("w1", "lead")];
        assert_eq!(rows(nest(cards)), vec![("lead".to_string(), 0), ("w2".into(), 1), ("w1".into(), 1), ("other".into(), 0)]);
    }

    #[test]
    fn a_child_whose_parent_isnt_listed_stays_at_the_top_level() {
        let cards = vec![child("w1", "gone"), card("other", "idle")];
        assert_eq!(rows(nest(cards)), vec![("w1".to_string(), 0), ("other".into(), 0)]);
    }

    #[test]
    fn a_child_in_its_own_worktree_is_listed_with_its_parent_and_in_its_own_tree() {
        let tree_of = |cwd: &str| Some(cwd.to_string());
        let cards = vec![at(card("lead", "idle"), "/p"), at(child("w1", "lead"), "/wt"), at(card("x", "idle"), "/wt")];
        assert_eq!(titles(with_children(cards.clone(), Some("/p"), tree_of)), vec!["lead", "w1"]);
        assert_eq!(titles(with_children(cards, Some("/wt"), tree_of)), vec!["w1", "x"]);
    }

    #[test]
    fn the_pinned_box_holds_a_pinned_parent_with_its_children() {
        let listed = vec![(Card { pinned: true, ..card("lead", "idle") }, 0), (child("w1", "lead"), 1), (card("other", "idle"), 0)];
        assert_eq!(pinned_len(&listed), 2);
    }
}
```

In `jump.rs` tests, add (the child comes through `status::card`, so this also proves a summary's parent reaches its card):

```rust
    #[test]
    fn ctrl_n_walks_a_child_right_after_its_parent() {
        let child = status::card(&Summary { id: "w".into(), title: "Fix w".into(), status: "idle".into(), attached: true, parent: "lead".into(), ..Default::default() }, "/app-wt");
        let cards = vec![child, card("a", "idle", "/app"), card("lead", "idle", "/app")];
        let order: Vec<String> = walkable(cards, Some("/app"), |cwd| Some(cwd.to_string()), "").into_iter().map(|(c, _)| c.id).collect();
        assert_eq!(order, vec!["a", "lead", "w"]);
    }
```

and in its existing `ids` helper and `pinned_sessions_come_first_in_their_own_order`, change `.map(|c| c.id)` to `.map(|(c, _)| c.id)`.

**Step 2: Run to verify they fail**

Run: `cd packages/desktop && cargo test -p pocket sessions`
Expected: build fails with `unresolved imports super::nest, super::pinned_len, super::with_children`.

**Step 3: Implement**

`sessions.rs`, imports become:

```rust
use crate::desktop::Desktop;
use crate::desktop::chrome::{RowMenu, empty, state};
use crate::sidebar::in_tree;
use crate::status::{Card, Kind};
use crate::util::{ago, now_ms};
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashSet;
use theme::*;
use ui::{self, State};
```

After `matching`, add:

```rust
/// The cards in `tree`, and every card started under one kept, wherever it runs: a child in its own worktree is listed with its parent.
pub(crate) fn with_children(cards: Vec<Card>, tree: Option<&str>, tree_of: impl Fn(&str) -> Option<String>) -> Vec<Card> {
    let mut kept: HashSet<String> = in_tree(cards.clone(), tree, tree_of).into_iter().map(|c| c.id).collect();
    while let Some(child) = cards.iter().find(|c| !kept.contains(&c.id) && kept.contains(&c.parent)) {
        kept.insert(child.id.clone());
    }
    cards.into_iter().filter(|c| kept.contains(&c.id)).collect()
}

/// Each card followed by the cards started under it, a level deeper, in the order given; a card whose parent isn't among them is at the top level.
pub(crate) fn nest(cards: Vec<Card>) -> Vec<(Card, usize)> {
    let ids: HashSet<String> = cards.iter().map(|c| c.id.clone()).collect();
    let (top, mut rest): (Vec<Card>, Vec<Card>) = cards.into_iter().partition(|c| !ids.contains(&c.parent));
    let mut out = Vec::new();
    for card in top {
        place(card, 0, &mut rest, &mut out);
    }
    out
}

fn place(card: Card, depth: usize, rest: &mut Vec<Card>, out: &mut Vec<(Card, usize)>) {
    let (children, others): (Vec<Card>, Vec<Card>) = std::mem::take(rest).into_iter().partition(|c| c.parent == card.id);
    *rest = others;
    out.push((card, depth));
    for child in children {
        place(child, depth + 1, rest, out);
    }
}

/// How many of the rows go in the Pinned box: those up to the first unpinned top-level row, so a pinned parent keeps its children.
pub(crate) fn pinned_len(rows: &[(Card, usize)]) -> usize {
    rows.iter().position(|(c, depth)| *depth == 0 && !c.pinned).unwrap_or(rows.len())
}
```

In `session_list`, replace the three lines from `let (pinned, rest)` through `let rest: Vec<_> = …` and the `let cards = …` line with:

```rust
        let sessions = self.visible_sessions(cx);
        let pinned = pinned_len(&sessions);
        let mut rows: Vec<_> = sessions.into_iter().enumerate().map(|(i, (c, depth))| self.card(i, c, depth, chips, cx)).collect();
        let rest = rows.split_off(pinned);
        let cards = div().p(px(8.)).flex().flex_col().gap(px(2.)).when(!rows.is_empty(), |d| d.child(ui::pinned_group(rows))).children(rest);
```

`card` takes the depth: its signature becomes `fn card(&self, i: usize, c: Card, depth: usize, chips: bool, cx: &mut Context<Self>) -> Stateful<Div>`, and its last expression starts:

```rust
        row.relative()
            .when(depth > 0, |r| r.ml(px(16. * depth as f32)))
            .child(more)
```

`jump.rs`: drop `use crate::sidebar::in_tree;`, change `use crate::sidebar::sessions::matching;` to `use crate::sidebar::sessions::{matching, nest, with_children};`, and `walkable` becomes:

```rust
/// The sessions ⌃n and ⌃Tab walk, in the order the Sessions column shows them: pinned first, each followed by the sessions started under it, with how deep each sits.
pub(crate) fn walkable(cards: Vec<Card>, tree: Option<&str>, tree_of: impl Fn(&str) -> Option<String>, query: &str) -> Vec<(Card, usize)> {
    let mut cards = matching(with_children(cards, tree, tree_of), query);
    cards.sort_by_key(|c| !c.pinned);
    nest(cards)
}
```

`visible_sessions` and `sessions_matching` return `Vec<(Card, usize)>`. In `jump_to`: `.nth(a.0.saturating_sub(1)).map(|(c, _)| c.id)`. In `step_session`: `.map(|(c, _)| c.id)`.

`panel.rs:89`: `let waiting = self.sessions_matching("").iter().any(|(c, _)| c.status == Status::NeedsYou);`

**Step 4: Run to verify they pass**

Run: `cd packages/desktop && cargo test -p pocket sessions && cargo test -p pocket jump && cargo test -p pocket sidebar`
Expected: all pass.

### Task 1.7: Chat breadcrumb, docs, and the "after" screens

**Files:**
- Create: `packages/desktop/crates/pocket/src/chat/breadcrumb.rs`
- Modify: `packages/desktop/crates/pocket/src/chat.rs:1` (`mod breadcrumb;`), `:160-186` (`chat_view`)
- Modify: `docs/adr/0003-desktop-code-layout.md` (the `chat` row of the `pocket` map)

**Context:** The chat view (`Desktop::chat_view` in `chat.rs`) is the pane body of an agent's terminal tab while its chat shows: transcript region over the composer. It has no header of its own (the pane's tab strip names it), so the breadcrumb is a slim strip at the top of `chat_view`, drawn only for a session whose parent is listed (`agents::Agents::get` finds only listed agents; a closed one is dropped from `list`). Its decision is the plain function `trail`, tested beside it; the render is thin. Clicking the parent's title calls `Desktop::focus_agent`, which switches to the parent's tree and focuses its tab, as a sidebar row does. Styling follows `.ui-review/prototypes/orchestrator-team.html` `.crumb` (muted text, `FILL_3` on hover, `TEXT_5` separator, the title semibold).

**Step 1: Write the failing tests**

Create `chat/breadcrumb.rs` with only the tests and a stub, and register it in `chat.rs` with `mod breadcrumb;` after `mod agent_map;`:

```rust
//! The strip over a child session's chat: its parent, which it focuses, then the session.

use agents::Agents;

/// What the breadcrumb over a chat names.
#[derive(Debug, PartialEq)]
pub(crate) struct Trail {
    pub(crate) parent: String,
    pub(crate) parent_title: String,
    pub(crate) title: String,
}

pub(crate) fn trail(_agents: &Agents, _agent: &str) -> Option<Trail> {
    None
}

#[cfg(test)]
mod tests {
    use super::{Trail, trail};
    use agents::{Agents, Summary};

    fn listing(list: Vec<Summary>) -> Agents {
        let mut agents = Agents::default();
        agents.list = list;
        agents
    }

    fn session(id: &str, title: &str, parent: &str) -> Summary {
        Summary { id: id.into(), title: title.into(), parent: parent.into(), ..Default::default() }
    }

    #[test]
    fn a_childs_chat_leads_back_to_its_parent() {
        let agents = listing(vec![session("lead", "Refactor the fold", ""), session("w1", "Split fold.rs", "lead")]);
        assert_eq!(trail(&agents, "w1"), Some(Trail { parent: "lead".into(), parent_title: "Refactor the fold".into(), title: "Split fold.rs".into() }));
    }

    #[test]
    fn a_session_without_a_listed_parent_has_no_breadcrumb() {
        let agents = listing(vec![session("lead", "Refactor the fold", ""), session("w1", "Split fold.rs", "gone")]);
        assert_eq!((trail(&agents, "lead"), trail(&agents, "w1")), (None, None));
    }
}
```

**Step 2: Run to verify it fails**

Run: `cd packages/desktop && cargo test -p pocket breadcrumb`
Expected: `a_childs_chat_leads_back_to_its_parent` FAILS (`left: None`); the other passes.

**Step 3: Implement**

Replace the stub `trail` and add the render; the file's code above the tests becomes:

```rust
//! The strip over a child session's chat: its parent, which it focuses, then the session.

use crate::desktop::Desktop;
use crate::status;
use agents::Agents;
use gpui_kit::*;
use theme::*;

/// What the breadcrumb over a chat names.
#[derive(Debug, PartialEq)]
pub(crate) struct Trail {
    pub(crate) parent: String,
    pub(crate) parent_title: String,
    pub(crate) title: String,
}

/// The breadcrumb over `agent`'s chat; none unless the session it was started under is listed.
pub(crate) fn trail(agents: &Agents, agent: &str) -> Option<Trail> {
    let a = agents.get(agent)?;
    let parent = agents.get(&a.parent)?;
    Some(Trail { parent: parent.id.clone(), parent_title: status::title(parent), title: status::title(a) })
}

impl Desktop {
    pub(super) fn chat_breadcrumb(&self, agent: &str, cx: &mut Context<Self>) -> Option<Div> {
        let Trail { parent, parent_title, title } = trail(&self.agents, agent)?;
        let back = div()
            .id("chat-parent")
            .h(px(24.))
            .px(px(6.))
            .rounded(px(6.))
            .max_w(px(220.))
            .truncate()
            .line_height(px(24.))
            .text_color(TEXT_3)
            .cursor_pointer()
            .hover(|s| s.bg(FILL_3).text_color(TEXT))
            .child(parent_title)
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.focus_agent(&parent, window, cx)));
        Some(
            div()
                .h(px(36.))
                .flex_none()
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .border_b(px(0.5))
                .border_color(SEPARATOR)
                .child(back)
                .child(div().text_color(TEXT_5).child("›"))
                .child(div().min_w_0().truncate().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(title)),
        )
    }
}
```

In `chat.rs` `chat_view`, before `let files = …` add:

```rust
        let crumb = agent.as_deref().and_then(|a| self.chat_breadcrumb(a, cx));
```

and the final expression becomes:

```rust
        div().flex_1().min_h_0().flex().flex_col().border_t(px(0.5)).border_color(SEPARATOR).children(crumb).child(transcript).child(composer).children(files)
```

`docs/adr/0003-desktop-code-layout.md`, in the `pocket` map's `chat` row, replace `in place of its terminal: \`chat/transcript\`` with `in place of its terminal: \`chat/breadcrumb\` the way back from a child session's chat to its parent; \`chat/transcript\``.

**Step 4: Run to verify it passes**

Run: `cd packages/desktop && cargo test -p pocket breadcrumb`
Expected: both tests pass.

**Step 5: Capture "after" and compare**

Run (repo root): `.ui-review/fixture/capture.sh /tmp/team-parent-after team-sidebar=lead team-worker=worker-chat`
Expected, against `/tmp/team-parent-before`:
- `impl-team-sidebar.png`: under "Refactor the transcript fold", two rows indented 16px, "Split fold.rs by block kind" (working) and "Cover the fold with tests"; nothing else in the list moved.
- `impl-team-worker.png`: a 36px strip at the top of the chat reading "Refactor the transcript fold › Split fold.rs by block kind", the parent's title muted; the transcript and composer below unchanged.

Also check a session without a parent is unchanged: `.ui-review/fixture/capture.sh /tmp/team-parent-after chat=chat` against the same command run into `/tmp/team-parent-before` at Task 1.5 Step 5 — no breadcrumb strip, same layout.

### Final checks

From `packages/pocketd`: `go test ./...` (every package `ok`).
From the repo root: `pnpm --filter @pocket/protocol test` (`# fail 0`).
From `packages/ios/Packages/AnywhereKit`: `swift test --scratch-path /tmp/ak-parent --filter WireTests` (passes).
From `packages/desktop`: `cargo build --workspace`, `cargo clippy --workspace --all-targets` (no new warnings), `cargo test --workspace`.

## Contract changes needed

None. The plan implements `LaunchSpec.Parent`, `AgentSummary.Parent`, `agents::Summary.parent` and the `tag_summary` rule as the index states. Its TS and iOS `AgentSummary.parent` fields follow the index's wire-mirror rule (their golden tests read pocketd's goldens). `state.Terminal.Parent` (persistence) stays inside this plan.

## Open questions

- **A child in its own worktree is listed twice:** under its parent in the parent's tree, and at the top level in its own tree. The index puts hiding worker worktrees out of scope; plan 6 may want the own-tree listing gone.
- **First update without the parent.** `Registry.AddFunc` publishes the new agent before `startAgent` sets its parent (as with `Origin`), so for one update a child is a top-level row. Measured in microseconds on the same goroutine; if plan 4 shows a visible flash, pass the parent into `AddFunc` instead.
- **Pinned child of an unpinned parent** is drawn with the pinned card style inside the unpinned list. Rare; left as is.
- **Closing a parent** leaves its children running at the top level; nothing cascades. Plan 4 closes workers only after a full merge and lists closing the lead as still open.
- **Phone.** The phone decodes `parent` but doesn't nest sessions or show the breadcrumb.
