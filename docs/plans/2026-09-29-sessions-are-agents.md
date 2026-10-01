# Sessions Are Agents Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** A session is one agent process; terminals, tabs and splits belong to the worktree they were opened in; New session runs in the selected worktree without a branch picker.

**Architecture:** pocketd keeps an exited agent listed as `closed` until its terminal closes, then forgets it and pushes a fresh `agent.list`; `agent.close` on a closed agent forgets it. The desktop builds one card per agent (placed by its terminal's folder) and keys workspaces by worktree path instead of by top-level terminal, so the store's parent/child terminal links go away. The phone filters closed agents out of lists, since it has no ended cards.

**Toolset** (paths relative to the repo root `/Users/mingo/Developer/self/anywhere`):
- pocketd, one package: `cd packages/pocketd && go test ./internal/<pkg>/ -run <TestName>`
- pocketd, full: `cd packages/pocketd && go vet ./... && go test ./...` (includes `e2e/`)
- phone: `pnpm --filter @pocket/app typecheck && pnpm --filter @pocket/app test`
- desktop, one crate: `cd packages/desktop && cargo test -p <crate> <test_name>` (crates: `pocket`, `workspace`, `store`, `agents`, `ui`, `daemon`)
- desktop, full: `cd packages/desktop && cargo test -p pocket -p workspace -p store -p agents -p ui -p daemon && cargo build -p pocket -p storybook` — no new warnings allowed.
- Full suite (every PR boundary): all three "full" lines above.
- The shell is fish: quote globs and separators (`echo '----'`, `--include='*.go'`).

**Read first:**
- `docs/adr/0002-sessions-are-agents-terminals-belong-to-worktrees.md` — the decision this plan implements.
- `CONTEXT.md` — Session, Terminal, Worktree, Project vocabulary; use these words.
- `docs/designs/2026-09-28-agent-sessions.md` — the desktop layout (rail, column, cards, worktree rows).
- `docs/wayfinder/agent-sessions/05-terminal-agent-card-model.md` — the terminal-as-session model this reverses; explains the code you are deleting.

**Assumptions** (settled; don't re-open):
- Splits are not persisted: after a desktop restart every terminal is its own tab, in list order.
- The worktree row sub-line is the worktree path.
- The new-worktree form (⌘⇧N) keeps create + copy files + run setup; only New session (⌘N) loses the branch picker.
- Palette ⌘↵ on a session just opens it (no "open in split").
- New shells, splits and agent tabs open in the worktree root.
- A crashed shell's card disappears with its terminal (no "failed" card for agentless terminals).
- A new worktree may briefly show as a temporary project until the git refresh lists it (existing behaviour).
- The wide column title reads "main" on the main worktree even though the crumb also says main — it follows the decision.
- CLAUDE.md: no comments unless the WHY is not readable from the code. Keep doc comments this plan gives; add no others.

---

## PR 1: pocketd keeps ended agents until their terminal closes

**Scope:** pocketd marks an exited agent `closed` and keeps listing it until its terminal is gone, then forgets it and pushes `agent.list`. `agent.close` on a closed agent forgets it. The phone drops closed agents from lists. The desktop needs no change (it already ranks closed agents). `LastProvider`/`LastTitle` stay until PR 4.
**Depends on:** nothing
**Done when:** pocketd full and phone full are green; a closed agent is still in `agent.list` while its shell lives.

### Task 1.1: e2e — a closed agent stays listed while its shell lives

**Files:**
- Modify: `packages/pocketd/e2e/pty_test.go` (`TestClaudeTypedInAShellIsAnAgent`, after line 62)

**Context:** The e2e harness runs a real pocketd. `phone` is a WebSocket client (`e2e/phone_test.go`): `Send(map[string]any)`, `WaitFor(what, func(Message) bool) Message`, `WaitStatus(agentID, status)`. `Message` has `Type`, `ID` and `Agents []struct{ ID string }`. The test starts `sh`, types `claude` (a fake), then Ctrl-C; the shell keeps running, so the closed agent must stay listed.

**Step 1: Write the failing test**

Right after `phone.WaitStatus(a.ID, "closed")`, before the existing `c.Send(ops.Msg{Op: "list"})` block (keep that block; PR 4 removes it), insert:

```go
	phone.Send(map[string]any{"type": "agent.list", "id": "l"})
	phone.WaitFor("closed agent still listed", func(m Message) bool {
		return m.Type == "agent.list" && m.ID == "l" && len(m.Agents) == 1 && m.Agents[0].ID == a.ID
	})
```

It proves an exited agent is still listed while its terminal is open.

**Step 2: Run the test to verify it fails**

Run: `cd packages/pocketd && go test ./e2e/ -run TestClaudeTypedInAShellIsAnAgent`
Expected: FAIL — timeout waiting for "closed agent still listed" (today the agent is removed on exit). It passes after Task 1.3.

### Task 1.2: Registry.Close and Registry.Forget

**Files:**
- Modify: `packages/pocketd/internal/agent/agent.go:85-94` (`Remove`)
- Test: `packages/pocketd/internal/agent/agent_test.go:86-106` (`TestRemovePublishesClosed`)

**Context:** `Registry` holds agents in a map; `List()` sorts by `CreatedAt`. `a.update(publish, fn)` applies `fn` and publishes `agent.update`; once `a.closed` is set, `Summary().Status` is `"closed"` and later updates are ignored. `r.hub.Publish(msg)` fans a message out to every phone. `proto.NewAgentList(id, agents)` builds `agent.list`. Test helpers in `agent_test.go`: `fakeDriver{}`, `drain(ch) []msg` (a `msg` has `Type` and `Agent.Status`).

**Step 1: Write the failing tests**

Replace `TestRemovePublishesClosed` with:

```go
func TestClosePublishesClosedAndKeepsTheAgent(t *testing.T) {
	h := hub.New()
	r := NewRegistry(h)
	a := r.Add("a1", "/w", "claude", fakeDriver{})
	ch, _ := h.Subscribe()
	r.Close("a1")
	a.Working()
	a.SetCompacting()
	a.SetTitle("late")
	a.Record(timeline.Event{Kind: "user", Text: "late"})
	if m := drain(ch); len(m) != 1 || m[0].Agent.Status != "closed" {
		t.Fatalf("%+v", m)
	}
	if _, err := r.Get("a1"); err != nil {
		t.Fatal(err)
	}
	if l := r.List(); len(l) != 1 || l[0].Status != "closed" {
		t.Fatalf("%+v", l)
	}
}

func TestForgetDropsTheAgentAndPushesTheList(t *testing.T) {
	h := hub.New()
	r := NewRegistry(h)
	r.Add("a1", "/w", "claude", fakeDriver{})
	r.Close("a1")
	ch, _ := h.Subscribe()
	r.Forget("a1")
	if _, err := r.Get("a1"); err == nil || err.Error() != "Unknown agent: a1" {
		t.Fatal(err)
	}
	if m := drain(ch); len(m) != 1 || m[0].Type != "agent.list" {
		t.Fatalf("%+v", m)
	}
	if len(r.List()) != 0 {
		t.Fatal("still listed")
	}
}
```

The first proves a closed agent is published once, frozen, and still listed; the second proves Forget removes it and tells phones.

**Step 2: Run the tests to verify they fail**

Run: `cd packages/pocketd && go test ./internal/agent/ -run 'TestClose|TestForget'`
Expected: FAIL to compile — `r.Close undefined`, `r.Forget undefined`.

**Step 3: Write the implementation**

Replace `Remove` (with its doc comment) by:

```go
// Close marks the agent closed for every phone; it stays listed until Forget.
func (r *Registry) Close(id string) {
	r.mu.Lock()
	a := r.agents[id]
	r.mu.Unlock()
	if a != nil {
		a.update(true, func() { a.closed = true })
	}
}

func (r *Registry) Forget(id string) {
	r.mu.Lock()
	delete(r.agents, id)
	r.mu.Unlock()
	r.hub.Publish(proto.NewAgentList("", r.List()))
}
```

`presence.go` still calls `Remove`; that is fixed in Task 1.3, so the `daemon` package won't compile until then. Only run the `agent` package here.

**Step 4: Run the tests to verify they pass**

Run: `cd packages/pocketd && go test ./internal/agent/`
Expected: PASS

### Task 1.3: the daemon closes ended agents and forgets them with their terminal

**Files:**
- Modify: `packages/pocketd/internal/daemon/presence.go:110-112` (tail of `endAgent`)
- Modify: `packages/pocketd/internal/daemon/watch.go:28-35` (`poll`)
- Test: `packages/pocketd/internal/daemon/presence_test.go`

**Context:** `poll()` runs every 250 ms: it observes each live terminal (`d.Terminals.All()`), then ends agents whose terminal already closed (`d.exited()`). A terminal leaves `All()` when its process exits (the manager's `onExit` deletes it before `Done` fires), so in one poll an agent can be ended and forgotten. `proto.AgentSummary` has `ID`, `TerminalID`, `Status`. Test helpers: `newDaemon`, `shell`, `fakeAgent`, `waitAgent`, `agentIn`, `eventually(t, what, cond)`.

**Step 1: Update the tests**

In `presence_test.go`:

1. `agentIn` (line 52) must only find a live agent — change its condition to:
   ```go
   		if a.TerminalID == term.Info().ID && a.Status != "closed" {
   ```
2. Replace `waitGone` (lines 73-80) with:
   ```go
   func waitClosed(t *testing.T, d *Daemon, id string) {
   	t.Helper()
   	eventually(t, "agent closed", func() bool {
   		d.poll()
   		a, err := d.Agents.Get(id)
   		return err == nil && a.Summary().Status == "closed"
   	})
   }
   ```
3. `TestClaudeInATerminalIsAnAgentWhileItRuns` and `TestClosingADetectedAgentStopsOnlyItsProcess`: `waitGone(t, d, a.ID)` → `waitClosed(t, d, a.ID)`.
4. `TestANewClaudePidIsANewAgent`: replace the last check
   ```go
   	if _, err := d.Agents.Get(first.ID); err == nil {
   		t.Fatal("the first agent is still listed")
   	}
   ```
   with
   ```go
   	if a, err := d.Agents.Get(first.ID); err != nil || a.Summary().Status != "closed" {
   		t.Fatal("the first agent is not kept closed")
   	}
   ```

`TestAgentEndsWithItsTerminal` stays as is: the terminal is gone, so the agent is ended and forgotten in the same poll.

**Step 2: Run the tests to verify they fail**

Run: `cd packages/pocketd && go test ./internal/daemon/`
Expected: FAIL to compile — `d.Agents.Remove undefined` in `presence.go`.

**Step 3: Write the implementation**

`presence.go`, tail of `endAgent`:

```go
	d.Broker.DenyAll(pr.a.ID())
	// Before Close: a client that sees the agent closed must find its last title.
	pr.t.SetLast(pr.provider, pr.a.Summary().Title)
	d.Agents.Close(pr.a.ID())
}
```

`watch.go`, `poll`:

```go
func (d *Daemon) poll() {
	live := map[string]bool{}
	for _, t := range d.Terminals.All() {
		live[t.Info().ID] = true
		d.observe(t)
	}
	for _, pr := range d.exited() {
		d.endAgent(pr)
	}
	for _, a := range d.Agents.List() {
		if a.Status == "closed" && !live[a.TerminalID] {
			d.Agents.Forget(a.ID)
		}
	}
}
```

**Step 4: Run the tests to verify they pass**

Run: `cd packages/pocketd && go test ./internal/daemon/ && go test ./e2e/ -run TestClaudeTypedInAShellIsAnAgent`
Expected: PASS (both; the e2e test from Task 1.1 now passes).

### Task 1.4: `agent.close` on a closed agent forgets it

**Files:**
- Modify: `packages/pocketd/internal/wsserver/wsserver.go:197-198`
- Test: `packages/pocketd/internal/wsserver/wsserver_test.go`

**Context:** `setup(t)` returns `(reg, driver, phone)` with agent `a1` registered; `p.hello()` consumes `hello.ok` and a one-agent `agent.list`; `p.send(raw)`, `p.recv() map[string]any`. After `reg.Close("a1")` the phone also receives an `agent.update`, and after Forget an `agent.list`, before the `ack`.

**Step 1: Write the failing test**

Append:

```go
func TestClosingAClosedAgentForgetsIt(t *testing.T) {
	reg, _, p := setup(t)
	p.hello()
	reg.Close("a1")
	p.send(`{"type":"agent.close","id":"c","agentId":"a1"}`)
	for m := p.recv(); m["type"] != "ack" || m["id"] != "c"; m = p.recv() {
	}
	if len(reg.List()) != 0 {
		t.Fatal("closed agent still listed")
	}
}
```

It proves the user can dismiss an ended session.

**Step 2: Run the test to verify it fails**

Run: `cd packages/pocketd && go test ./internal/wsserver/ -run TestClosingAClosedAgentForgetsIt`
Expected: FAIL with "closed agent still listed".

**Step 3: Write the implementation**

```go
	case "agent.close":
		if a.Summary().Status == "closed" {
			c.s.Agents.Forget(a.ID())
		} else {
			a.Driver().Close()
		}
```

**Step 4: Run the tests to verify they pass**

Run: `cd packages/pocketd && go test ./internal/wsserver/`
Expected: PASS

### Task 1.5: the phone leaves closed agents out of lists

**Files:**
- Modify: `packages/app/src/agents.ts`
- Modify: `packages/app/src/session.tsx:4,49-50`
- Test: `packages/app/test/agents.test.mts`

**Context:** The phone has no ended cards. `applyAgentUpdate` already drops an agent when an update says `closed`; `agent.list` now also carries closed agents, so the list handler must filter them. Tests run with `node --test`; the file already has an `agent(id, status)` helper.

**Step 1: Write the failing test**

Change the import to `import { applyAgentUpdate, liveAgents } from "../src/agents.ts";` and append:

```ts
test("a listed closed agent is left out", () => {
  assert.deepEqual(liveAgents([agent("a", "idle"), agent("b", "closed")]), [agent("a", "idle")]);
});
```

**Step 2: Run the test to verify it fails**

Run: `pnpm --filter @pocket/app test`
Expected: FAIL — `liveAgents` is not exported.

**Step 3: Write the implementation**

`agents.ts`, append:

```ts
export function liveAgents(agents: readonly AgentSummary[]): readonly AgentSummary[] {
  return agents.filter((a) => a.status !== "closed");
}
```

`session.tsx`: import `import { applyAgentUpdate, liveAgents } from "./agents";` and in the `"agent.list"` case use `setAgents(liveAgents(msg.agents));`.

**Step 4: Run the tests to verify they pass**

Run: `pnpm --filter @pocket/app typecheck && pnpm --filter @pocket/app test`
Expected: PASS, no type errors.

---

## PR 2: desktop cards are agents; terminals belong to worktrees

**Scope:** One card per agent (live or ended), clicking it shows its worktree, tab and pane. Workspaces are keyed by worktree path; every terminal whose folder is in a worktree gets a tab there. The store's parent/child terminal links and every terminal-as-session helper go. Close session closes the agent's terminal, or forgets an ended agent. The New session form still has its branch picker (PR 3).
**Depends on:** PR 1 (ended agents must stay listed for ended cards to exist)
**Done when:** desktop full is green; with pocketd running, starting `claude` in any terminal of a worktree adds a card, clicking it switches to that terminal's tab, and after `claude` exits the card shows a faded badge until the terminal closes.

### Task 2.1: `Workspace::sync`

**Files:**
- Modify: `packages/desktop/crates/workspace/src/workspace.rs`

**Context:** A `Workspace` is a list of tabs; a `Tab::Term(rows)` holds rows of pane ids (pocketd terminal ids). Today one exists per top-level terminal, built by `Workspace::new(id, children)`. From Task 2.3 one exists per worktree and is refreshed from the terminal list: terminals of this worktree not yet shown get their own tab, panes of terminals that belong to another worktree are dropped. A terminal just spawned is not in the list yet, so panes the list doesn't mention must stay. `new` stays until Task 2.3 because `pocket` still calls it. Test helper `term(rows: &[&[&str]]) -> Tab` exists at line 83.

**Step 1: Write the failing tests**

Add `Default` to the struct derive: `#[derive(Debug, PartialEq, Default)]` on `Workspace`. In `mod tests` add:

```rust
    fn with(ids: &[&str]) -> Workspace {
        let mut w = Workspace::default();
        w.sync(&ids.iter().map(|s| s.to_string()).collect::<Vec<_>>(), &[]);
        w
    }

    #[test]
    fn sync_gives_each_new_terminal_a_tab_and_drops_moved_ones() {
        let mut w = with(&["a", "b"]);
        w.split("c".into(), true);
        w.active = 1;
        w.sync(&["a", "b", "c", "d"].map(String::from), &[]);
        assert_eq!(w.tabs, vec![term(&[&["a"], &["c"]]), term(&[&["b"]]), term(&[&["d"]])]);
        w.sync(&[], &["a".to_string()]);
        assert_eq!((w.tabs, w.active), (vec![term(&[&["c"]]), term(&[&["b"]]), term(&[&["d"]])], 1));
    }

    #[test]
    fn sync_keeps_a_pane_not_listed_yet() {
        let mut w = Workspace::default();
        w.add_tab("x".into());
        w.sync(&["a".to_string()], &[]);
        assert_eq!(w.tabs, vec![term(&[&["x"]]), term(&[&["a"]])]);
    }
```

**Step 2: Run the tests to verify they fail**

Run: `cd packages/desktop && cargo test -p workspace sync_`
Expected: FAIL to compile — no method `sync`.

**Step 3: Write the implementation**

In `impl Workspace`, after `active`:

```rust
    /// Gives each of `mine` not yet shown its own tab and drops panes of `theirs`; unknown panes stay, since a just-spawned terminal is not listed yet.
    pub fn sync(&mut self, mine: &[String], theirs: &[String]) {
        for id in theirs {
            self.remove(id);
        }
        for id in mine {
            if self.tab_of(id).is_none() {
                self.tabs.push(Tab::Term(vec![vec![id.clone()]]));
            }
        }
    }
```

**Step 4: Run the tests to verify they pass**

Run: `cd packages/desktop && cargo test -p workspace`
Expected: PASS

### Task 2.2: a card is an agent

**Files:**
- Modify: `packages/desktop/crates/pocket/src/status.rs` (imports, `Kind`, `card`, tests)
- Modify: `packages/desktop/crates/pocket/src/sessions.rs` (`activity`, `exit_seen`, `see_exits`, tests)
- Modify: `packages/desktop/crates/pocket/src/main.rs` (`sync_view`, `cards`, `summary`, `next_waiting`, `submit_comment`)
- Modify: `packages/desktop/crates/pocket/src/view.rs` (`card`, nav rail click ~line 476, `pane_label` ~line 756)
- Modify: `packages/desktop/crates/pocket/src/overlay.rs` (`activate`, its two callers, palette split hint)
- Modify: `packages/desktop/crates/pocket/src/diff.rs:599-603` (`session_chip`)
- Modify: `packages/desktop/crates/pocket/src/capture.rs:11-15` ("session" step)

**Context:** `agents::Summary` is pocketd's agent: `id`, `terminal_id`, `provider`, `title`, `status` (`"closed"` once exited), `attached`, `updated_at`, `cwd`. `Status::of(a)` maps it to a pill status (`None` for closed or not attached). Today `status::card(terms, agents)` builds one card per top-level terminal from its most urgent agent, with `Kind::Shell` for agentless terminals. Now a card is exactly one agent, placed by the folder its terminal started in (`Session.info.cwd`), not the agent's own `cwd` — an agent that `cd`s stays in its worktree. Card ids become agent ids, so everything that took a card id and treated it as a terminal id now goes through `focus_agent`. `focus_agent` is private in `main.rs` (the crate root), so child modules (`view`, `overlay`, `capture`) can call it. After this task `focus_agent` is still the old one (it sets `self.session` to the top terminal); Task 2.3 replaces it, so the selected-card highlight is off until then. That's expected.

**Step 1: Write the failing tests**

`status.rs` tests: delete `use daemon::Info;` (if only `term()` used it), the `term()` helper, and the tests `a_card_shows_its_most_urgent_agent_across_tabs`, `agents_in_other_sessions_do_not_count`, `an_agentless_card_shows_its_exit_but_never_fails`, `a_card_keeps_its_last_agent_after_it_exits`, `a_crashed_agents_card_clears_once_its_exit_is_seen`, `closed_and_detached_agents_rank_below_attached_ones`, `an_untitled_agent_reads_new_session`. Add:

```rust
    #[test]
    fn a_card_is_its_agent_in_its_terminals_folder() {
        let a = Summary { title: "Fix".into(), cwd: "/elsewhere".into(), updated_at: 5, ..agent("t1", "working") };
        let c = card(&a, "/w");
        assert_eq!((c.id.as_str(), c.title.as_str(), c.cwd.as_str(), c.at, c.status, c.kind), ("agent-t1", "Fix", "/w", 5, Status::Working, Kind::Agent));
    }

    #[test]
    fn a_detached_agent_reads_not_attached_and_idle() {
        let c = card(&Summary { attached: false, ..agent("t1", "needsYou") }, "/w");
        assert_eq!((c.kind, c.status), (Kind::NotAttached, Status::Idle));
    }

    #[test]
    fn an_exited_agent_is_an_ended_idle_card() {
        let c = card(&Summary { title: "Fix CI".into(), provider: "claude".into(), ..agent("t1", "closed") }, "/w");
        assert_eq!((c.kind, c.status, c.title.as_str(), c.provider.as_str()), (Kind::Ended, Status::Idle, "Fix CI", "claude"));
    }

    #[test]
    fn an_untitled_agent_reads_new_session() {
        assert_eq!(card(&agent("t1", "working"), "/w").title, "New session");
    }
```

`sessions.rs` tests: delete `activity_reads_the_foreground_the_prompt_or_the_exit_code` and `only_exits_on_screen_are_seen` (their code goes in Step 3).

**Step 2: Run the tests to verify they fail**

Run: `cd packages/desktop && cargo test -p pocket status::`
Expected: FAIL to compile — `card` takes `(&[&Session], &[Summary])`.

**Step 3: Write the implementation**

`status.rs`:
- Remove the imports `crate::sessions::Session`, `crate::view::command_line`, `std::cmp::Reverse` (keep `HashMap`/`HashSet` if still used).
- `Kind` becomes (drop `Shell`):
  ```rust
  #[derive(Clone, Debug, PartialEq)]
  pub enum Kind {
      Agent,
      NotAttached,
      /// The agent exited; its card stays, with a faded provider badge, until its terminal closes.
      Ended,
  }
  ```
- Replace `card` (doc comment and body) with:
  ```rust
  /// An agent's card, placed by the folder its terminal started in.
  pub fn card(a: &Summary, cwd: &str) -> Card {
      let kind = if a.status == "closed" {
          Kind::Ended
      } else if a.attached {
          Kind::Agent
      } else {
          Kind::NotAttached
      };
      Card {
          id: a.id.clone(),
          provider: a.provider.clone(),
          title: if a.title.is_empty() { "New session".into() } else { a.title.clone() },
          cwd: cwd.to_string(),
          at: a.updated_at,
          status: Status::of(a).unwrap_or(Status::Idle),
          kind,
      }
  }
  ```

`sessions.rs`:
- Remove `Session::activity`, the `exit_seen` field, `Sessions::see_exits`; drop `exit_seen: false` from the `Session { .. }` literal in `sync`.

`main.rs`:
- `sync_view`: delete the line `self.sessions.see_exits(&panes);`.
- If `Session` is now unused in `main.rs`, change `use sessions::{Session, Sessions};` to `use sessions::Sessions;`.
- Replace `cards`:
  ```rust
      pub fn cards(&self, project: &str) -> Vec<Card> {
          let projects = self.projects();
          let mut out: Vec<Card> = self
              .agents
              .list
              .iter()
              .filter_map(|a| Some((a, self.sessions.get(&a.terminal_id)?)))
              .filter(|(_, s)| self.project_of(&s.info.cwd, &projects).is_some_and(|p| p == project))
              .map(|(a, s)| status::card(a, &s.info.cwd))
              .collect();
          out.sort_by_key(|c| std::cmp::Reverse(c.at));
          out
      }
  ```
- Replace `summary`:
  ```rust
      /// The live agent in `terminal`.
      pub fn summary(&self, terminal: &str) -> Option<&Summary> {
          self.agents.list.iter().find(|a| a.terminal_id == terminal && a.status != "closed")
      }
  ```
- `next_waiting`: `self.select_session(id, window, cx);` → `self.focus_agent(&id, window, cx);`.
- `submit_comment`: the target is now an agent id; the prompt op needs its terminal. After the `if text.is_empty() { return; }` block add
  ```rust
          let Some(terminal) = self.agents.get(&target).map(|a| a.terminal_id.clone()) else { return };
  ```
  and send `"id": terminal` instead of `"id": target`.

`view.rs`:
- `card`: replace the `pill`/`lead` bindings and the call with
  ```rust
          let pill = match c.kind {
              Kind::NotAttached => State::NotAttached,
              _ => state(c.status, added, removed),
          };
          let lead = ui::provider_label(&c.provider, c.kind == Kind::Ended);
          ui::session_row(("card", i), selected, c.title, Some(pill), lead, branch, ago(c.at, now), Vec::new())
              .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.focus_agent(&id, window, cx)))
  ```
  and delete the `let tags = …` line.
- Nav rail session click (~line 476): `this.select_session(id.clone(), window, cx)` → `this.focus_agent(&id, window, cx)`.
- `pane_label` (~line 756): `match (self.summary(id).filter(|a| a.status != "closed"), self.sessions.get(id))` → `match (self.summary(id), self.sessions.get(id))`.

`overlay.rs`:
- `activate` loses its `split` parameter; the session arm opens the agent:
  ```rust
      fn activate(&mut self, pick: Pick, window: &mut Window, cx: &mut Context<Self>) {
          self.overlay = None;
          match pick {
              Pick::Session(id) => self.focus_agent(&id, window, cx),
  ```
  (other arms unchanged).
- Enter handler (~line 165): `self.activate(e.pick.clone(), window, cx);`.
- Row click (~line 194): `move |this, _: &ClickEvent, window, cx| this.activate(pick.clone(), window, cx)`.
- Palette footer: delete `.child(hint("⌘↵", "open in split"))`. The "Open selected in a split" action entry: `keys: Some("⌘ ↵")` → `keys: None`.

`diff.rs` `session_chip` (id is an agent id):
```rust
    fn session_chip(&self, id: &str) -> (u32, String, String) {
        let a = self.agents.get(id);
        let provider = a.map(|a| a.provider.clone()).unwrap_or_default();
        let branch = a.and_then(|a| self.sessions.get(&a.terminal_id)).and_then(|s| self.repos.get(&s.info.cwd)).map(|r| r.branch.clone()).unwrap_or_default();
        (provider_color(&provider), provider, branch)
    }
```

`capture.rs` "session" step: `d.select_session(card.id, window, cx);` → `d.focus_agent(&card.id, window, cx);`.

Fix any now-unused import the compiler reports in the files you touched (e.g. `command_line` in `view.rs` stays used by `pane_label`; check before removing).

**Step 4: Run the tests to verify they pass**

Run: `cd packages/desktop && cargo test -p pocket && cargo build -p pocket`
Expected: PASS; no warnings from `pocket`.

### Task 2.3: workspaces are worktrees

**Files:**
- Modify: `packages/desktop/crates/pocket/src/main.rs`
- Modify: `packages/desktop/crates/pocket/src/sessions.rs` (`Session.closed`, `failed`)
- Modify: `packages/desktop/crates/pocket/src/view.rs` (`worktree_rows`, `session_list`, `main_view`, `session_page`, `term_tabs`)
- Modify: `packages/desktop/crates/pocket/src/forms.rs` (`Intent::Session` uses)
- Modify: `packages/desktop/crates/store/src/store.rs` (`children`, `parent`, `children_of`)
- Modify: `packages/desktop/crates/workspace/src/workspace.rs` (`new`, doc, tests)

**Context:** `self.workspaces: HashMap<String, Workspace>` is keyed by top-level terminal id today; now by worktree path. A terminal's worktree is `tree_of(cwd)`: the git worktree holding its folder (`worktree_of`), else its project (`project_of` — ad-hoc folders and non-git projects are their own only worktree). `self.worktree: Option<String>` is the worktree picked in the sidebar; `None` means the project's main worktree, so `cwd()` becomes "the worktree on screen". `self.session: Option<String>` is now the selected agent id. `Intent` (the queued answer to pocketd's `spawned` event) now names the worktree the new terminal goes to. `Store.children` (persisted tab links) and `Session.closed` (only used by the old `close_tab`) lose their last users.

**Step 1: Update the tests**

`workspace.rs` tests: delete `restores_children_as_tabs`; replace every remaining `Workspace::new("a", std::iter::empty())` with `with(&["a"])`, `Workspace::new("a", ["b"].into_iter())` with `with(&["a", "b"])`, `Workspace::new("a", ["b", "c"].into_iter())` with `with(&["a", "b", "c"])`.

`store.rs` test `round_trips_through_desktop_json`: delete the `s.children.push(…)` line and the `parent`/`children_of` asserts.

`sessions.rs`: delete the test `a_closed_session_killed_by_its_close_has_not_failed`.

**Step 2: Run the tests to verify they fail**

Run: `cd packages/desktop && cargo test -p workspace -p store`
Expected: PASS — these edits only drop tests of code Step 3 deletes; Step 4's build is the real check.

**Step 3: Write the implementation**

`workspace.rs`: remove `Workspace::new`; the doc on `Tab` becomes `/// The tabs of one worktree: terminal tabs hold rows of panes, each a pocketd terminal id.`

`store.rs`: remove the `children` field with its doc comment, and the methods `parent` and `children_of`. Old `desktop.json` files still load (serde ignores unknown fields unless the struct denies them — check for `deny_unknown_fields`; if present, stop and report).

`sessions.rs`: remove the `closed` field; `failed` becomes (no doc comment)
```rust
    pub fn failed(&self) -> bool {
        self.exit.is_some_and(|c| c != 0)
    }
```
and drop `closed: false` from the `Session { .. }` literal in `sync`.

`main.rs`:
- `Intent`:
  ```rust
  enum Intent {
      Tab(String),
      Split(String, bool),
  }
  ```
- Replace the old private `focus_agent` (~line 295) with:
  ```rust
      /// Shows an agent's session: its worktree, the tab holding its terminal, and that pane focused.
      pub fn focus_agent(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
          let Some(term) = self.agents.get(id).map(|a| a.terminal_id.clone()) else { return };
          let Some(cwd) = self.sessions.get(&term).map(|s| s.info.cwd.clone()) else { return };
          let Some(tree) = self.tree_of(&cwd) else { return };
          if let Some(p) = self.project_of(&cwd, &self.projects()).cloned() {
              self.project = Some(p);
          }
          (self.screen, self.side) = (Screen::Sessions, Side::Sessions);
          self.worktree = Some(tree.clone());
          self.session = Some(id.to_string());
          let w = self.workspace(&tree);
          if let Some(i) = w.tab_of(&term) {
              w.active = i;
          }
          self.focus_pane(term, window, cx);
          self.refresh_git(cx);
      }
  ```
- `visible_panes`:
  ```rust
          let Some(tree) = self.cwd().filter(|_| self.screen == Screen::Sessions && !preview) else { return Vec::new() };
          match self.workspace(&tree).active() {
  ```
- `on_msg` `"spawned"`:
  ```rust
                  match self.intents.pop_front() {
                      Some(Intent::Tab(tree)) => self.adopt(m.id.clone(), tree, None, window, cx),
                      Some(Intent::Split(tree, down)) => self.adopt(m.id.clone(), tree, Some(down), window, cx),
                      None => {}
                  }
  ```
- `close_clean_exits`: keep only the first doc line (`/// Closes panes whose shell exited cleanly, as Terminal.app does; a failed one stays so its error can be read.`), body:
  ```rust
      fn close_clean_exits(&mut self, id: &str, cx: &mut Context<Self>) {
          if self.sessions.get(id).is_some_and(|s| s.exit == Some(0)) {
              self.close_pane(id, cx);
          }
      }
  ```
- `adopt`:
  ```rust
      fn adopt(&mut self, id: String, tree: String, split: Option<bool>, window: &mut Window, cx: &mut Context<Self>) {
          let w = self.workspace(&tree);
          match split {
              Some(down) => w.split(id.clone(), down),
              None => w.add_tab(id.clone()),
          }
          self.worktree = Some(tree);
          self.focus_pane(id, window, cx);
      }
  ```
- `projects`: the sessions iterator loses the parent filter:
  ```rust
          let sessions = self.sessions.items.iter().map(|s| &s.info.cwd);
  ```
- After `worktree_of` add:
  ```rust
      /// The worktree a folder belongs to: the git worktree holding it, else the project it is in.
      pub fn tree_of(&self, cwd: &str) -> Option<String> {
          self.worktree_of(cwd).map(|w| w.path.clone()).or_else(|| self.project_of(cwd, &self.projects()).cloned())
      }
  ```
- `cwd`:
  ```rust
      /// The worktree on screen: the one picked, else the project's main one.
      pub fn cwd(&self) -> Option<String> {
          self.worktree.clone().or_else(|| self.tree_of(self.project.as_deref()?))
      }
  ```
- `workspace`:
  ```rust
      fn workspace(&mut self, tree: &str) -> &mut Workspace {
          let (mut mine, mut theirs) = (Vec::new(), Vec::new());
          for s in &self.sessions.items {
              match self.tree_of(&s.info.cwd) {
                  Some(t) if t == tree => mine.push(s.info.id.clone()),
                  Some(_) => theirs.push(s.info.id.clone()),
                  None => {}
              }
          }
          let w = self.workspaces.entry(tree.to_string()).or_default();
          w.sync(&mine, &theirs);
          w
      }
  ```
- Delete `select_session` and `open_session`.
- `select_tab`: its first two lines become
  ```rust
          let Some(tree) = self.cwd() else { return };
          let w = self.workspace(&tree);
  ```
- Replace `new_shell`'s doc with `/// Opens a login shell in the worktree's folder, as a new tab or a split of the active one.`; in `new_shell` and `new_agent_tab` call `self.run_in_tree(…)`; replace `run_in_session` with:
  ```rust
      fn run_in_tree(&mut self, split: Option<bool>, op: impl FnOnce(&str) -> serde_json::Value, cx: &mut Context<Self>) {
          let Some(tree) = self.cwd() else { return };
          let intent = match split {
              Some(down) => Intent::Split(tree.clone(), down),
              None => Intent::Tab(tree.clone()),
          };
          self.send_spawn(op(&tree), intent, cx);
      }
  ```
- `close_pane`: delete `self.workspaces.remove(id);`, `self.store.children.retain(|(c, _)| c != id);` and `self.store.save();`.
- `close_tab`:
  ```rust
      pub fn close_tab(&mut self, i: usize, cx: &mut Context<Self>) {
          let Some(tree) = self.cwd() else { return };
          for id in self.workspace(&tree).close_tab(i) {
              self.close_pane(&id, cx);
          }
          cx.notify();
      }
  ```
- `Tab` stays imported (used by `visible_panes`/`select_tab`); remove any import the compiler reports unused.

`forms.rs` `start_session`: `Mode::Current` → `Intent::Tab(repo.clone())`, `Mode::Existing` → `Intent::Tab(path.clone())`, `Mode::NewWorktree` success → `Intent::Tab(path)`. (`agent_op(&argv, &repo)` borrows `repo`, so pass `Intent::Tab(repo.clone())`; for Existing use `daemon::agent_op(&argv, &path), Intent::Tab(path.clone())`; for NewWorktree `d.send_spawn(daemon::agent_op(&argv, &path), Intent::Tab(path), cx)` works as `agent_op` returns before `path` moves — if the borrow checker disagrees, clone.)

`view.rs`:
- `worktree_rows`: before the `for` loop add `let current = self.cwd();`; `let selected = self.worktree.as_ref() == Some(&w.path);` → `let selected = current.as_ref() == Some(&w.path);`; the click handler body becomes
  ```rust
                          this.worktree = Some(path.clone());
                          this.session = None;
                          cx.notify();
  ```
- `session_list`: `let tree = self.worktree.clone();` → `let tree = self.cwd();`; the filter becomes `.filter(|c| self.tree_of(&c.cwd) == tree)`.
- `main_view`, last arm:
  ```rust
              _ => match self.cwd().filter(|t| !self.workspace(t).tabs.is_empty()) {
                  Some(tree) => self.session_page(&tree, cx),
                  None => self.blank_page(cx),
              },
  ```
- `session_page(&mut self, tree: &str, …)`: rename the parameter `id` → `tree` (it's passed to `term_tabs` and `workspace`).
- `term_tabs(&mut self, tree: &str, …)`: rename `parent` → `tree`; every tab can close — delete `let closable = …;`, make `let close = div()…` unconditional (drop the `closable.then(|| …)` wrapper), `.pr(px(if closable { 6. } else { 10. }))` → `.pr(px(6.))`, `.children(close)` → `.child(close)`.

Leave unchanged: `cwd_of` (used by `inbox.rs`), `explore_root`, capture's "worktree" step, `overlay.rs` ~line 350.

**Step 4: Run the tests to verify they pass**

Run: `cd packages/desktop && cargo test -p pocket -p workspace -p store && cargo build -p pocket`
Expected: PASS; no warnings.

### Task 2.4: Close session

**Files:**
- Modify: `packages/desktop/crates/agents/src/agents.rs` (`impl Outbox`, test `sends_queued_messages_while_pocketd_is_quiet`)
- Modify: `packages/desktop/crates/pocket/src/main.rs` (new `close_session`)
- Modify: `packages/desktop/crates/pocket/src/overlay.rs` (~line 395, "Close session")

**Context:** `Outbox` queues phone-protocol messages to pocketd (`self.send(json!(…))`, see `view`). The "Close session" menu row still calls `close_pane` with what is now an agent id. A running agent's session ends with its terminal; an ended one is only dismissed via `agent.close` (PR 1 makes pocketd forget it).

**Step 1: Write the failing test**

In `sends_queued_messages_while_pocketd_is_quiet`, after the last existing send/assert pair, add:

```rust
        out.close("a1");
        assert_eq!(read(&mut ws), json!({"type": "agent.close", "id": "close", "agentId": "a1"}));
```

**Step 2: Run the test to verify it fails**

Run: `cd packages/desktop && cargo test -p agents sends_queued_messages_while_pocketd_is_quiet`
Expected: FAIL to compile — no method `close` on `Outbox`.

**Step 3: Write the implementation**

`agents.rs`, in `impl Outbox`:

```rust
    pub fn close(&self, id: &str) {
        self.send(json!({"type": "agent.close", "id": "close", "agentId": id}));
    }
```

`main.rs`, after `close_pane`:

```rust
    /// Ends a session: a running agent goes with its terminal, an ended one only leaves the list.
    pub fn close_session(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(a) = self.agents.get(id) else { return };
        if a.status == "closed" {
            self.outbox.close(id);
        } else {
            let term = a.terminal_id.clone();
            self.close_pane(&term, cx);
        }
    }
```

`overlay.rs` "Close session" row: `this.close_pane(&id, cx);` → `this.close_session(&id, cx);`.

**Step 4: Run the tests to verify they pass**

Run: `cd packages/desktop && cargo test -p agents && cargo build -p pocket`
Expected: PASS; no warnings.

---

## PR 3: New session runs in the worktree; cards and labels

**Scope:** ⌘N New session has no branch picker and spawns in the worktree on screen; ⌘⇧N keeps the new-worktree flow. Cards show title + status pill + provider only. Worktree rows and the wide column title show the branch; UI says "Project".
**Depends on:** PR 2
**Done when:** desktop full is green; ⌘N in a worktree starts the agent as a new tab there.

### Task 3.1: New session form

**Files:**
- Modify: `packages/desktop/crates/pocket/src/forms.rs` (`Mode`, `NewForm`, `reset_new_form`, `pick_repo`, `existing_worktree`, `session_ready`, `start_session`, `new_worktree`, `branch_picker`, `new_session_view`)
- Modify: `packages/desktop/crates/pocket/src/view.rs:~130` (`open` → `reset_new_form`)
- Modify: `packages/desktop/crates/pocket/src/explore.rs:~305`
- Modify: `packages/desktop/crates/pocket/src/capture.rs:~33` ("prompt" step)

**Context:** One overlay (`Overlay::NewSession`) serves both forms; `NewForm.worktree: bool` says which. New session: prompt + agent/permission chip, spawns `daemon::agent_op(&argv, &tree)` with `Intent::Tab(tree)` where `tree = self.cwd()`. New worktree: the existing create-worktree flow (branch input, base picker, copy, setup), minus the Existing/Current modes. There are no unit tests for views; the check is compile + manual.

**Step 1: No new test** — pure UI; verified by build and by the manual check in Step 4.

**Step 2: n/a**

**Step 3: Write the implementation**

- Delete the `Mode` enum. In `NewForm` delete `mode`, `current`, `existing`; add `worktree: bool`. In `NewForm::new` drop those three initialisers and add `worktree: false`.
- `reset_new_form(&mut self, prompt: Option<String>, worktree: bool, window, cx)`: replace `f.mode = Mode::NewWorktree;` with `f.worktree = worktree;`.
- `pick_repo`: delete the `let existing = …;` line, `f.current.clear();` and `f.existing = …;`. In the async tail, `(f.current, f.branches) = (current, branches);` → `f.branches = branches;` (the local `current` still feeds `default_base`).
- Delete `existing_worktree`.
- `session_ready`:
  ```rust
      fn session_ready(&self, cx: &App) -> bool {
          let f = &self.new_form;
          if f.worktree {
              f.repo.is_some() && !self.new_branch(cx).is_empty() && !f.branches.is_empty()
          } else {
              self.cwd().is_some()
          }
      }
  ```
- `start_session`: remove `let Some(repo) = f.repo.clone() else { return };` from the top; replace the `match f.mode { … }` with
  ```rust
          let worktree = f.worktree;
          match self.cwd().filter(|_| !worktree) {
              Some(tree) => self.send_spawn(daemon::agent_op(&argv, &tree), Intent::Tab(tree), cx),
              None => {
                  let f = &self.new_form;
                  let Some(repo) = f.repo.clone() else { return };
                  // …the former Mode::NewWorktree body, unchanged, from `let branch = self.new_branch(cx);`
                  // through `.detach();`, with its success arm
                  // `Ok(path) => d.send_spawn(daemon::agent_op(&argv, &path), Intent::Tab(path), cx),`
              }
          }
  ```
  (The two `//` lines are instructions, not code to keep — paste the real body there.) `self.close_overlay(window, cx);` stays last.
- `new_worktree`:
  ```rust
      pub fn new_worktree(&mut self, _: &crate::NewWorktree, window: &mut Window, cx: &mut Context<Self>) {
          self.overlay = Some(Overlay::NewSession);
          self.reset_new_form(None, true, window, cx);
          cx.notify();
      }
  ```
  Check `open(Overlay::NewSession)` in `view.rs` does nothing else `new_worktree` now skips (focus, palette state); if it does, copy that too and report it.
- `branch_picker`: keep only the Default/Recent loop; delete everything from `let repo = f.repo.clone()…` to the "Current checkout" row. In the loop, the selected flag is `f.base == i` and the click sets `(this.new_form.base, this.new_form.picker) = (i, None);`.
- `new_session_view`:
  - Header title: `.child(div().text_size(px(16.)).font_weight(FontWeight::BOLD).child(if f.worktree { "New worktree" } else { "New session" }))`.
  - Delete `let target = match f.mode { … };`; the branch chip shows `base.clone()` instead of `target`, and is built only for the worktree form:
    ```rust
            let branch = f.worktree.then(|| {
                chip("form-branch", f.picker == Some(Picker::Branch))
                    .child(icon("branch", 14., TEXT_3))
                    .child(div().font_family(MONO).text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child(base.clone()))
                    .child(icon("chevron-down", 12., TEXT_4))
                    .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.toggle_picker(Picker::Branch, cx);
                    }))
            });
    ```
    and in the composer row `.child(div().relative().child(branch).children(branch_menu))` → `.children(branch.map(|b| div().relative().child(b).children(branch_menu)))`.
  - Summary line — move `let width = …;` inside the worktree arm:
    ```rust
            let summary: Vec<AnyElement> = if f.worktree {
                let width = self.new_branch(cx).chars().count().max(8);
                vec![
                    div().child("New worktree on").into_any_element(),
                    // GPUI inputs don't size to their text; Geist Mono advances 0.6em.
                    div().w(px(width as f32 * 7.2 + 2.)).font_family(MONO).child(Input::new(&f.branch).appearance(false).p_0().max_h(px(16.)).text_size(px(12.)).line_height(px(16.)).text_color(rgba(TEXT_2))).into_any_element(),
                    div().child("from").into_any_element(),
                    mono(base).into_any_element(),
                ]
            } else {
                let place = self.repo().map(|r| r.branch.clone()).or_else(|| self.cwd().map(|c| tilde(&c))).unwrap_or_default();
                vec![div().child("In").into_any_element(), mono(place).into_any_element()]
            };
    ```
  - `repo` is still used by the header name; drop any binding the compiler reports unused (e.g. `basename` import if `branch_picker` was its last user).
- Callers of `reset_new_form`: `view.rs` `open` → `self.reset_new_form(None, false, window, cx)`; `explore.rs` → `(Some(prompt.clone()), false, window, cx)`; `capture.rs` "prompt" step → `(Some("…".into()), false, window, cx)`.

**Step 4: Verify**

Run: `cd packages/desktop && cargo test -p pocket && cargo build -p pocket`
Expected: PASS; no warnings.
Manual (report if you can't run it): `cargo run -p pocket` with the test pocketd; ⌘N shows no branch chip and "In main"; submit opens a new tab running the agent in the selected worktree; ⌘⇧N shows "New worktree" with the branch chip.

### Task 3.2: a card is title, pill and provider

**Files:**
- Modify: `packages/desktop/crates/ui/src/ui.rs:361-412` (`session_row`)
- Modify: `packages/desktop/crates/pocket/src/view.rs` (`card`, `session_list`)
- Modify: `packages/desktop/crates/storybook/src/main.rs:149-152`

**Context:** The card's second line today reads provider · worktree icon · branch … time, then tag chips. The decision: title + status pill + provider dot only (the worktree is already selected in the sidebar).

**Step 1: No new test** — layout only; verified by build and storybook.

**Step 3: Write the implementation**

`ui.rs`: delete `#[allow(clippy::too_many_arguments)]`; the signature becomes
```rust
pub fn session_row(id: impl Into<ElementId>, selected: bool, title: String, state: Option<State>, lead: impl IntoElement) -> Stateful<Div> {
```
In the second line keep the container and `.child(lead)`; delete the `·` child, the `worktree` icon, the branch child and the `when` child. Delete the trailing `.when(!tags.is_empty(), …)`.

`view.rs` `card` becomes:
```rust
    fn card(&self, i: usize, c: Card, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = self.session.as_ref() == Some(&c.id);
        let (added, removed) = self.repos.get(&c.cwd).map(|r| r.totals()).unwrap_or_default();
        let id = c.id.clone();
        let pill = match c.kind {
            Kind::NotAttached => State::NotAttached,
            _ => state(c.status, added, removed),
        };
        let lead = ui::provider_label(&c.provider, c.kind == Kind::Ended);
        ui::session_row(("card", i), selected, c.title, Some(pill), lead)
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.focus_agent(&id, window, cx)))
    }
```
`session_list`: delete `let now = now_ms();`; `self.card(i, c, now, cx)` → `self.card(i, c, cx)`.

Storybook, the four rows:
```rust
                    .child(ui::session_row("s1", true, "Fix stale terminal reveal".into(), Some(State::NeedsYou), ui::provider_label("codex", false)))
                    .child(ui::session_row("s2", false, "Split restore hook into two files".into(), Some(State::Working), ui::provider_label("claude", false)))
                    .child(ui::session_row("s3", false, "Upgrade to RN 0.81".into(), Some(State::Failed), ui::provider_label("codex", false)))
                    .child(ui::session_row("s4", false, "Migrate legacy hooks".into(), Some(State::Idle(28, 11)), ui::provider_label("codex", true))),
```

`ui::tag`, `ago`, `now_ms` and `command_line` are used elsewhere — keep them; remove only imports the compiler reports unused.

**Step 4: Verify**

Run: `cd packages/desktop && cargo test -p ui -p pocket && cargo build -p pocket -p storybook`
Expected: PASS; no warnings.

### Task 3.3: branch labels and "Project"

**Files:**
- Modify: `packages/desktop/crates/pocket/src/view.rs` (`worktree_rows`, `column_header`, strings)
- Modify: `packages/desktop/crates/pocket/src/overlay.rs` (strings ~364, ~373)
- Modify: `packages/desktop/crates/pocket/src/forms.rs` (strings ~909, ~917)
- Modify: `packages/desktop/crates/storybook/src/main.rs:~189-192`

**Context:** Worktree rows used the newest card's title as the label; now the branch ("main" for the main worktree), with the path below. The wide column title does the same for the worktree on screen. User-facing "repository/repositories" becomes "project/projects"; git terms (branch, worktree, clone URL) stay.

**Step 1: No new test** — labels only.

**Step 3: Write the implementation**

`worktree_rows`:
```rust
            let label = if w.main { "main".to_string() } else { w.branch.clone() };
```
delete `let branch = …;`, and pass `tilde(&w.path)` where `branch` was passed to `ui::worktree_row`.

`column_header`, wide branch — replace the `tree` and `title` bindings:
```rust
        let tree = self.cwd().and_then(|t| self.worktrees.values().flatten().find(|w| w.path == t)).cloned();
        let repo = tree.as_ref().and_then(|t| self.repos.get(&t.path)).or_else(|| self.repos.get(&project));
        let title = match &tree {
            Some(t) if t.main => "main".to_string(),
            Some(t) => t.branch.clone(),
            None => name.clone(),
        };
```

Strings (find each with the fff grep tool before editing; line numbers are approximate):

| File | From | To |
|---|---|---|
| `view.rs` ~216 | `"Add repository"` | `"Add project"` |
| `view.rs` ~271 | `"Repositories"` | `"Projects"` |
| `view.rs` ~299 | `"Add repository"` | `"Add project"` |
| `view.rs` session_list | `"Add a repository with + to start."` | `"Add a project with + to start."` |
| `view.rs` blank_page | `"Add a repository to begin."` | `"Add a project to begin."` |
| `overlay.rs` ~364 | `format!("Repositories in {name}")` | `format!("Projects in {name}")` |
| `overlay.rs` ~373 | `"Add repository to project…"` | `"Add project…"` |
| `forms.rs` ~909 | `"Add repository"` | `"Add project"` |
| `forms.rs` ~917 | `"Repository settings"` / `"Add repository"` | `"Project settings"` / `"Add project"` |
| `storybook` ~189 | `"Nested under the selected repository"` | `"Nested under the selected project"` |

Leave doc comments and git-term strings (`forms.rs` ~747-757 clone/URL fields, `diff.rs` ~229) as they are. If a listed string isn't where the table says, grep for it; if it doesn't exist, report it.

Storybook worktree rows:
```rust
                    .child(ui::worktree_row("w2", "fix/restore-handoff".into(), "~/.worktrees/app-android/fix-restore-handoff".into(), false, true, Some(State::NeedsYou)))
                    .child(ui::worktree_row("w3", "chore/migrate-hooks".into(), "~/.worktrees/app-android/chore-migrate-hooks".into(), false, false, Some(State::Merged))),
```

**Step 4: Verify**

Run: `cd packages/desktop && cargo test -p pocket && cargo build -p pocket -p storybook`
Expected: PASS; no warnings. `rg -n -i 'repositor' packages/desktop/crates/pocket/src packages/desktop/crates/storybook/src` lists only doc comments and git-term strings.

---

## PR 4: drop the terminal's last-agent fields

**Scope:** Ended agents now carry their own title, so pocketd's `Terminal.LastProvider/LastTitle/SetLast` and the desktop's `Info.last_provider/last_title` have no readers. Remove them.
**Depends on:** PR 2 (the desktop's last reader goes there)
**Done when:** pocketd full and desktop full are green; `rg -n 'LastProvider|LastTitle|SetLast|last_provider|last_title' packages` finds nothing outside `docs/`.

### Task 4.1: pocketd

**Files:**
- Modify: `packages/pocketd/internal/terminal/terminal.go` (fields ~30-31, `SetLast` ~253-257)
- Modify: `packages/pocketd/internal/daemon/presence.go` (tail of `endAgent`)
- Modify: `packages/pocketd/internal/terminal/terminal_test.go` (~139, `TestSetLastLandsInInfo`)
- Modify: `packages/pocketd/internal/daemon/claude_test.go` (~113, the test that checks `LastTitle`)
- Modify: `packages/pocketd/internal/daemon/presence_test.go` (`TestClaudeInATerminalIsAnAgentWhileItRuns`)
- Modify: `packages/pocketd/e2e/pty_test.go` (`TestClaudeTypedInAShellIsAnAgent`)

**Context:** `claudeIn(t, d)` (in `claude_test.go`) starts a fake claude in a terminal and returns `(term, presence)`; `pr.a` is the agent.

**Step 1: Rewrite the tests**

- `terminal_test.go`: delete `TestSetLastLandsInInfo`.
- `claude_test.go`: `TestAnEndingClaudeLeavesItsLastTitle` (~113) proves `endAgent` reads the transcript tail (a title behind 20000 progress lines) before closing. Replace it with:
  ```go
  func TestAnEndingClaudeKeepsItsLastTitle(t *testing.T) {
  	d := newDaemon(t)
  	_, pr := claudeIn(t, d)
  	path := transcript(t, "hi")
  	hookFrom(d, pr, sessionStart("s1", path))
  	eventually(t, "the transcript", func() bool { return pr.a.Summary().Title == "hi" })
  	appendLine(t, path, strings.Repeat(`{"type":"progress"}`+"\n", 20000)+`{"type":"ai-title","aiTitle":"Fix the login bug"}`)
  	d.endAgent(pr)
  	if s := pr.a.Summary(); s.Status != "closed" || s.Title != "Fix the login bug" {
  		t.Fatalf("summary = %+v", s)
  	}
  }
  ```
- `presence_test.go` `TestClaudeInATerminalIsAnAgentWhileItRuns`: delete `ag, _ := d.Agents.Get(a.ID)`, `ag.SetTitle("Fix the login bug")` and the trailing `if i := term.Info(); i.LastProvider … { … }` block.
- `pty_test.go`: delete the `c.Send(ops.Msg{Op: "list"})` line and the `if m, _ := c.Recv(); … LastProvider … { … }` block after it.

**Step 2: Run to see the old API is still referenced**

Run: `cd packages/pocketd && go vet ./...`
Expected: PASS (nothing removed yet).

**Step 3: Remove the fields**

- `terminal.go`: delete the `LastProvider`/`LastTitle` fields from `Info` and the `SetLast` method.
- `presence.go`: delete `// Before Close: a client that sees the agent closed must find its last title.` and `pr.t.SetLast(pr.provider, pr.a.Summary().Title)` (`pr.provider` stays; `presence.go:60` reads it).

**Step 4: Run the tests to verify they pass**

Run: `cd packages/pocketd && go vet ./... && go test ./...`
Expected: PASS

### Task 4.2: desktop

**Files:**
- Modify: `packages/desktop/crates/daemon/src/daemon.rs:19-22` (fields), test `decodes_terminal_activity` (~181)
- Modify: `packages/desktop/crates/pocket/src/sessions.rs` test `sync_refreshes_what_the_list_says_about_known_sessions`

**Step 1: Rewrite the tests**

- `decodes_terminal_activity`, first half becomes:
  ```rust
          let m: Msg = serde_json::from_str(r#"{"ev":"terminals","items":[{"id":"a","cmd":"zsh","cwd":"/w","foreground":"npm run dev"}]}"#).unwrap();
          assert_eq!(m.items[0].foreground, "npm run dev");
  ```
  (the `"foreground"` event half stays).
- `sync_refreshes_what_the_list_says_about_known_sessions`, its last two lines become:
  ```rust
          assert_eq!(s.sync(vec![Info { foreground: "npm run dev".into(), ..info("a") }]), Vec::<String>::new());
          assert_eq!(s.get("a").unwrap().info.foreground, "npm run dev");
  ```

**Step 2: Run to confirm they compile against the current struct**

Run: `cd packages/desktop && cargo test -p daemon -p pocket`
Expected: PASS

**Step 3: Remove the fields**

`daemon.rs` `Info`: delete `last_provider` and `last_title` with their `#[serde(rename…)]` attributes.

**Step 4: Run the tests to verify they pass**

Run: `cd packages/desktop && cargo test -p pocket -p workspace -p store -p agents -p ui -p daemon && cargo build -p pocket -p storybook`
Expected: PASS; no warnings. `rg -n 'LastProvider|LastTitle|SetLast|last_provider|last_title' packages` prints nothing.
