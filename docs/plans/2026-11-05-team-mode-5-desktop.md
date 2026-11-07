# Team Mode 5: Desktop Model and Team Trigger Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** The desktop keeps every team pocketd reports, can send every team verb, and lets you ask for a team from either composer: the "+" button becomes a menu with **Attach images** and **Team**, and **Team** puts a `Team ×` chip in the box whose next send carries `team: true`.

**Architecture:** `agents` gains `agents::team` (`Team`, `TeamTask`, `TaskPick`), three events (`Event::Team`, `Event::Teams` parsed from `team.update` and the `team.list` reply, and `Event::TeamFailed` for a refused team prompt or verb), `Agents.teams` keyed by lead id, a `team.list` request sent by the connection itself after every `hello.ok`, and the `Outbox` verbs. `hosts::session` tags a host's teams like its agents and routes team verbs by the lead's id. In `pocket`, `composer::Draft` gains `team` and `Menu::Add`; the decisions (who may lead a team, what a create carries, what a send with the chip does, the footer hint) are plain functions with tests; two new components, `composer/add_menu.rs` and `composer/team_chip.rs`, draw the "+" menu and the chip in both composers.

**Toolset (from `packages/desktop`):**
- One crate's tests: `cargo test -p agents team`, `cargo test -p hosts session`, `cargo test -p pocket composer`
- Everything: `cargo build --workspace`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`
- Screens (from the repo root, needs `bun`): `.ui-review/fixture/capture.sh <dir> <name>=<step>,<step> …`

**Read first:**
- `docs/plans/2026-11-05-team-mode.md`: the index. The wire contract (`team.list`, `team.update`, `team.start/stop/merge`, `team` on `agent.prompt` and on the `agent.create` spec, the `Team`/`TeamTask` JSON) and the desktop contracts this plan implements.
- `CLAUDE.md` and `docs/adr/0003-desktop-code-layout.md`: model crates hold logic, `pocket` holds views, one file per component, tests beside the logic named as sentences, no mocks, no render tests.
- `packages/desktop/crates/agents/src/agents.rs`: `Frame` (~line 325), `Event` (~386), `Agents` and `apply_from` (~440–555), the `*_event` frame parsers (~774–831), `Outbox` (~856), `run` (~1069), and the tests' fake pocketd (`pocketd`, `accept`, `read`, ~1428) and `sent_but_id` / `golden!` (~2029).
- `packages/desktop/crates/hosts/src/session.rs`: `tag_summary`, `tag_event`, `Outboxes` and their tests.
- `packages/desktop/crates/pocket/src/composer.rs` (Draft, Menu, `Desktop::composer`), `pocket/src/chat/composer.rs` (`hint`, `chat_command`, `chat_send`, `chat_composer`, `prompt_box`), `pocket/src/chat/composer/attachments.rs` (`attach_button`), `pocket/src/chat/composer/model_pill.rs` (how a chip toggles its menu), `pocket/src/modals/new_session.rs` (`start_session`).

Line numbers drift (plan 1 also edits `agents.rs` and `session.rs`); find each spot by the code quoted, not by number.

## Decisions

- **Teams need `team.v1`.** The desktop puts `team.v1` in its hello `CAPS`, and pocketd (plan 3) advertises it, so it is negotiated only with a pocketd that plans and runs teams. `Agents::teams_offered()` is true when it was negotiated and the connection isn't observe-only, like `access_offered`. A pocketd without it (a host not yet updated) is never asked for teams and never offered **Team**.
- **The connection asks for teams.** `agents::run` sends `{"type":"team.list","id":"teams"}` right after every `hello.ok` that negotiated `team.v1`, as it already asks for each agent's timeline. That covers this Mac's pocketd, every host's and every reconnect with no `pocket` code. `Outbox::team_list()` sends the same frame for anyone who wants a fresh list.
- **Hosts.** A host's pocketd runs teams too, so `tag_event` tags a team like an agent: `lead` and each task's `worker` get `host/`, each task's `worktree` becomes `//host/path`. `Outboxes` routes `team_start/stop/merge` by the lead's id. `Agents::forget(host)` drops that host's teams.
- **Newest wins.** A team replaces the one held for its lead only if its `updatedAt` is not older: a `team.list` reply built before an update can be written after it. A `team.list` from one pocketd replaces only that pocketd's teams.
- **Who can lead.** Team is offered only when pocketd serves teams (`agents.teams_offered()`), for a chat pocketd drives, led by Claude or Codex: `team_offered(served, chat, provider)`. In a running chat, `chat` is `Summary.chat_only`. On the new session page it is the **Start as a chat** setting (`store.agents.start_as_chat`), which is what puts `"chat": true` on the spec.
- **A team send never queues.** The team prompt starts the lead's planning turn, and the chat's queue (`chat::Outgoing`) has no place for the flag. So while the agent is busy, Enter with the chip set sends nothing and the footer reads `Team waits for this turn to end · esc to stop`. Once idle, the send goes with `team: true` and the chip clears. A slash command never carries `team` (pocketd would wrap the command in the team rules); it goes as typed and clears the chip with the rest of the draft.
- **The "+" menu.** Rows: **Attach images** (today's file picker) and, where offered, **Team** with a check while set (picking it again takes the chip off). The menu opens down and right-aligned on the new session page, up and right-aligned in the chat. Its open state is `Draft.menu = Some(Menu::Add)`, so it shares the one-menu-at-a-time rule with the model, effort and access chips.
- **Refused team requests are events.** An `error` answering `team.start`, `team.stop`, `team.merge` or a team prompt (`team_busy`, say), or a team request the socket never took, becomes `Event::TeamFailed { lead, action, message }` (`TeamAction::Plan(request)`, `Start`, `Stop`, `Merge`; `message` is pocketd's, `OUTDATED` for `Malformed message`). A refused team prompt also fails its send in the chat, as `PromptFailed` does. Plan 6 shows each as a toast.
- **Retry keeps the flag.** A failed team send remembers it asked for a team (`Pending.team`), and Retry puts it back in the box with the chip set.
- **The chip.** `Team ×`, tinted `COMMAND` like the command pills, with a new `team` icon (the prototype's `TEAM_ICON`). It sits in the prompt bar after the other chips. × clears `Draft.team`.

---

## PR 1: Teams in the desktop model, and the team verbs

**Scope:** `agents::team`, `Event::Team`/`Event::Teams`, `Agents.teams`, the `team.v1` cap and `Agents::teams_offered`, the `team.list` request after a `hello.ok` that negotiated it, `Outbox::team_list/team_start/team_stop/team_merge`, `team` on `Outbox::prompt`, host tagging and routing, refused team requests as `Event::TeamFailed` (a refused team prompt still fails its send). Every existing `prompt` caller passes `false`. Inert: nothing draws a team or sends a team verb until PR 2 (the flag) and plan 6 (the rest).
**Depends on:** pocketd plans 3 and 4 merged (the verbs exist). The tests here don't need them.
**Done when:** from `packages/desktop`, `cargo build --workspace`, `cargo clippy --workspace --all-targets` (no new warnings) and `cargo test --workspace` pass.

### Task 1.1: `agents::team` and teams kept in `Agents`

**Files:**
- Create: `packages/desktop/crates/agents/src/team.rs`
- Modify: `packages/desktop/crates/agents/src/agents.rs` (module list at the top, `Event`, `Agents`, `Agents::forget`, `Agents::apply_from`, tests)
- Modify: `packages/desktop/crates/hosts/src/session.rs` (`tag_event`, a new `tag_team`, tests)

**Context:** `Agents::apply_from(source, ev)` applies an event from this Mac's pocketd (`source == None`) or host `source`'s (ids already tagged `host/…`); its closure `mine(id)` tells whether an id belongs to `source`. Events sent whole (like `Event::Names`) replace only the sender's share. `hosts::session::tag_event` must handle every `Event` variant (its `match` is exhaustive), so the hosts crate breaks until it tags the new ones; that's why it is in this task. `hosts::place(host, path)` is `crate::place` inside the hosts crate.

**Step 1: Write the failing tests**

Add to the bottom of `mod tests` in `agents.rs`:

```rust
    fn team(lead: &str, state: &str, updated_at: i64) -> Team {
        Team { lead: lead.into(), state: state.into(), updated_at, ..Team::default() }
    }

    fn leads(a: &Agents) -> Vec<&str> {
        let mut leads: Vec<&str> = a.teams.keys().map(String::as_str).collect();
        leads.sort();
        leads
    }

    #[test]
    fn a_team_update_is_kept_by_its_lead_and_replaces_the_one_before() {
        let mut a = Agents::default();
        a.apply(Event::Team(team("a1", "planning", 1)));
        a.apply(Event::Team(team("a1", "proposed", 2)));
        a.apply(Event::Team(team("a2", "planning", 1)));
        assert_eq!(a.team("a1"), Some(&team("a1", "proposed", 2)));
        assert_eq!(a.team("a2"), Some(&team("a2", "planning", 1)));
    }

    #[test]
    fn an_update_older_than_the_team_held_changes_nothing() {
        let mut a = Agents::default();
        a.apply(Event::Team(team("a1", "running", 5)));
        a.apply(Event::Teams(vec![team("a1", "proposed", 4)]));
        a.apply(Event::Team(team("a1", "planning", 3)));
        assert_eq!(a.team("a1"), Some(&team("a1", "running", 5)));
    }

    #[test]
    fn a_team_list_replaces_only_the_teams_of_the_pocketd_that_sent_it() {
        let mut a = Agents::default();
        a.apply(Event::Team(team("a1", "running", 1)));
        a.apply_from(Some("devbox"), Event::Team(team("devbox/b1", "running", 1)));
        a.apply_from(Some("devbox"), Event::Teams(vec![team("devbox/b2", "proposed", 1)]));
        assert_eq!(leads(&a), ["a1", "devbox/b2"]);
    }

    #[test]
    fn a_host_taken_off_this_mac_takes_its_teams_with_it() {
        let mut a = Agents::default();
        a.apply(Event::Team(team("a1", "running", 1)));
        a.apply_from(Some("devbox"), Event::Team(team("devbox/b1", "running", 1)));
        a.forget("devbox");
        assert_eq!(leads(&a), ["a1"]);
    }
```

Add to `mod tests` in `hosts/src/session.rs`, after `a_hosts_agent_events_are_tagged_and_its_phone_and_pairing_news_dropped`:

```rust
    #[test]
    fn a_hosts_team_names_its_lead_workers_and_worktrees_in_the_desktops_terms() {
        let task = |worker: &str, worktree: &str| agents::TeamTask { id: "t1".into(), worker: worker.into(), worktree: worktree.into(), ..Default::default() };
        let mut ev = Event::Team(Team { lead: "a1".into(), tasks: vec![task("w1", "/home/me/.worktrees/team-a1-t1"), task("", "")], ..Team::default() });
        assert!(tag_event("devbox", &mut ev));
        let Event::Team(team) = ev else { unreachable!() };
        assert_eq!(team, Team { lead: "devbox/a1".into(), tasks: vec![task("devbox/w1", "//devbox/home/me/.worktrees/team-a1-t1"), task("", "")], ..Team::default() });

        let mut list = Event::Teams(vec![Team { lead: "a2".into(), ..Team::default() }]);
        assert!(tag_event("devbox", &mut list));
        assert!(matches!(list, Event::Teams(t) if t[0].lead == "devbox/a2"));
    }
```

What they prove: an update replaces its lead's team; an older one never wins; a list replaces only its sender's teams; forgetting a host drops its teams; a host's team ids and paths come in the desktop's terms.

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p agents team`
Expected: FAIL to compile with `cannot find type `Team` in this scope` and `no variant or associated item named `Team` found for enum `Event``.

**Step 3: Write the implementation**

Create `crates/agents/src/team.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A lead chat's team, as `team.list` and `team.update` report it.
#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Team {
    /// The lead's agent id.
    pub lead: String,
    /// planning, proposed, running, review, merging, merged, conflict or failed.
    pub state: String,
    pub error: String,
    pub tasks: Vec<TeamTask>,
    /// The lead said "Ready to merge".
    pub ready: bool,
    /// The files the failed merge conflicted in.
    pub conflict: Vec<String>,
    /// The lead's one repair turn was used.
    pub repaired: bool,
    pub updated_at: i64,
}

/// One task of a team's plan.
#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct TeamTask {
    pub id: String,
    pub title: String,
    /// claude or codex.
    pub agent: String,
    pub model: String,
    /// The only files it may change.
    pub files: Vec<String>,
    /// The tasks whose work it starts from.
    pub after: Vec<String>,
    /// Command prefixes its worker runs without asking.
    pub allow: Vec<String>,
    pub instructions: String,
    /// planned, waiting, running, needsYou, done, failed, stopped or merged.
    pub state: String,
    /// The worker's agent id, once it runs.
    pub worker: String,
    pub branch: String,
    pub worktree: String,
    /// The worker's last text.
    pub summary: String,
    /// `git diff --stat` against its base.
    pub stat: String,
}

/// A task kept at Start, as `team.start` takes it: its agent and model as picked, its allowlist as approved.
/// An empty model or allowlist is left off the frame, as pocketd's `omitempty` writes it.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct TaskPick {
    pub id: String,
    pub agent: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub model: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<String>,
}

/// Holds `team` unless the one held for its lead is newer: a list read before an update can arrive after it.
pub(crate) fn keep(teams: &mut HashMap<String, Team>, team: Team) {
    if teams.get(&team.lead).is_none_or(|held| held.updated_at <= team.updated_at) {
        teams.insert(team.lead.clone(), team);
    }
}
```

In `agents.rs`, at the top:

```rust
pub mod automations;
pub mod locals;
pub mod providers;
pub mod team;

pub use team::{TaskPick, Team, TeamTask};
```

In `pub enum Event`, after `McpActFailed { agent: String, reason: String },`:

```rust
    /// A lead's team changed.
    Team(Team),
    /// Every team one pocketd holds, sent whole.
    Teams(Vec<Team>),
```

In `pub struct Agents`, after `pub automations: Automations,`:

```rust
    /// Each team by its lead's id, from `team.list` and `team.update`.
    pub teams: HashMap<String, Team>,
```

In `impl Agents`, after `pub fn revision`:

```rust
    pub fn team(&self, lead: &str) -> Option<&Team> {
        self.teams.get(lead)
    }
```

In `forget`, after the line that applies `Event::Agents(Vec::new())`:

```rust
        self.apply_from(Some(source), Event::Teams(Vec::new()));
```

In `apply_from`, before the final `Event::AutomationError(_) | … => {}` arm:

```rust
            Event::Team(t) => team::keep(&mut self.teams, t),
            Event::Teams(list) => {
                self.teams.retain(|lead, _| !mine(lead) || list.iter().any(|t| &t.lead == lead));
                list.into_iter().for_each(|t| team::keep(&mut self.teams, t));
            }
```

In `hosts/src/session.rs`, change the import to:

```rust
use agents::{Decision, Event, McpAction, Outbox, Summary, Team};
```

After `fn tag_summary`, add:

```rust
fn tag_team(host: &str, team: &mut Team) {
    team.lead = tag(host, &team.lead);
    for t in &mut team.tasks {
        if !t.worker.is_empty() {
            t.worker = tag(host, &t.worker);
        }
        if !t.worktree.is_empty() {
            t.worktree = crate::place(host, &t.worktree);
        }
    }
}
```

In `tag_event`, after `Event::Agent(a) => tag_summary(host, a),`:

```rust
        Event::Team(team) => tag_team(host, team),
        Event::Teams(teams) => teams.iter_mut().for_each(|t| tag_team(host, t)),
```

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p agents team && cargo test -p hosts session`
Expected: PASS (`test result: ok`), including the four new `agents` tests and `a_hosts_team_names_its_lead_workers_and_worktrees_in_the_desktops_terms`.

### Task 1.2: Team frames become events, and every connect asks for the teams

**Files:**
- Modify: `packages/desktop/crates/agents/src/agents.rs` (`Frame`, a new `team_event`, constants by `const PAIR_CAP`, `CAPS`, `Agents::teams_offered`, `run`, tests)
- Modify: `.ui-review/fixture/fixture.ts` (its `hello.ok` caps)

**Context:** `run` reads each frame into `Frame` (every field `#[serde(default)]`), then tries `requests.answer`, `launch_event`, `names_event`, `automation_event` in turn before its own `match`. Its `"hello.ok"` arm sends `Event::Connected` (moving `f.caps` into it), then `continue`s when the frame carries no host; the `team.list` request must go out before that `continue`, and whether `team.v1` was negotiated must be read before `f.caps` moves. pocketd's `hello.ok` caps are the intersection of the client's and its own, so the desktop must ask for `team.v1` in `CAPS` (~line 1048, `[&str; 15]` today) to ever see it. `*_offered()` methods (~line 591–657) read `self.caps`. The tests' fake pocketd: `pocketd(name)` binds a socket, `connect(&sock)` starts the client, `accept(&server)` gives the server's websocket, `read(&mut ws)` reads one JSON frame. `frame(json)` (~line 1746) builds a `Frame`. `sends_queued_messages_while_pocketd_is_quiet` (~line 1447) asserts the whole hello, `CAPS` included. The capture fixture pocketd (`.ui-review/fixture/fixture.ts`, ~line 344) answers `hello` with a fixed caps list, which must hold `team.v1` for the screens to show **Team**.

**Step 1: Write the failing tests**

Add to `mod tests` in `agents.rs`, after `fn frame(raw: Value) -> Frame { … }`:

```rust
    #[test]
    fn team_frames_become_team_events() {
        let raw = json!({"lead": "a1", "state": "proposed", "updatedAt": 7, "tasks": [{"id": "t1", "title": "Add the parser", "agent": "codex", "files": ["src/parse.rs"], "after": [], "allow": ["cargo test"], "instructions": "Write it", "state": "planned"}]});
        let want = Team {
            lead: "a1".into(),
            state: "proposed".into(),
            updated_at: 7,
            tasks: vec![TeamTask { id: "t1".into(), title: "Add the parser".into(), agent: "codex".into(), files: vec!["src/parse.rs".into()], allow: vec!["cargo test".into()], instructions: "Write it".into(), state: "planned".into(), ..TeamTask::default() }],
            ..Team::default()
        };
        assert!(matches!(team_event(&frame(json!({"type": "team.update", "team": raw}))), Some(Event::Team(t)) if t == want));
        assert!(matches!(team_event(&frame(json!({"type": "team.list", "id": "teams", "teams": [raw]}))), Some(Event::Teams(t)) if t == [want.clone()]));
        assert!(team_event(&frame(json!({"type": "agent.list", "agents": []}))).is_none());
    }

    #[test]
    fn hello_ok_asks_for_the_teams_and_their_list_reaches_the_app() {
        let (server, sock) = pocketd("teams");
        let (_out, events) = connect(&sock);
        let mut ws = accept(&server);
        read(&mut ws);
        ws.send(Message::text(json!({"type": "hello.ok", "id": "h", "caps": ["team.v1"]}).to_string())).unwrap();
        assert_eq!(read(&mut ws), json!({"type": "team.list", "id": "teams"}));
        let list = json!({"type": "team.list", "id": "teams", "teams": [{"lead": "a1", "state": "planning", "tasks": [], "updatedAt": 1}]});
        ws.send(Message::text(list.to_string())).unwrap();
        let mut events = futures::executor::block_on_stream(events);
        assert!(matches!(events.next(), Some(Event::Connected { .. })));
        assert!(matches!(events.next(), Some(Event::Teams(t)) if t == [Team { lead: "a1".into(), state: "planning".into(), updated_at: 1, ..Team::default() }]));
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn a_pocketd_without_teams_is_not_asked_for_them() {
        let (server, sock) = pocketd("noteams");
        let (out, _events) = connect(&sock);
        let mut ws = accept(&server);
        read(&mut ws);
        ws.send(Message::text(json!({"type": "hello.ok", "id": "h", "caps": []}).to_string())).unwrap();
        out.view(&["a1".into()]);
        assert_eq!(read(&mut ws), json!({"type": "agent.view", "id": "view", "agentIds": ["a1"]}));
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn team_is_offered_only_by_a_pocketd_that_serves_teams_to_an_owner() {
        let mut a = Agents::default();
        assert!(!a.teams_offered());
        let caps = vec![TEAM_CAP.to_string()];
        a.apply(Event::Connected { scopes: vec!["owner".into()], caps: caps.clone(), version: String::new() });
        assert!(a.teams_offered());
        a.apply(Event::Connected { scopes: vec!["observe".into()], caps, version: String::new() });
        assert!(!a.teams_offered());
        a.apply(Event::Connected { scopes: vec!["owner".into()], caps: vec![], version: String::new() });
        assert!(!a.teams_offered());
    }
```

In `sends_queued_messages_while_pocketd_is_quiet`, the expected hello's `caps` gains `"team.v1"` at the end:

```rust
            json!({"type": "hello", "id": "h", "clientId": "desktop", "protocolVersion": 3, "caps": ["pair.v1", "scopes.v1", "summary.v2", "host.v1", "open.v1", "names.v1", "automations.v1", "locals.v1", "fork.v1", "accounts.v1", "subagents.v1", "settings.v1", "commands.v1", "access.v1", "skills.v1", "team.v1"]})
```

What they prove: both server frames decode into the contract's fields, other frames don't; a connect that negotiated `team.v1` asks for the teams and the reply reaches the app; one that didn't sends nothing before the next queued message; Team is offered only with the cap and an owner's connection; the desktop asks for the cap.

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p agents team`
Expected: FAIL to compile with `cannot find function `team_event` in this scope`, `cannot find value `TEAM_CAP` in this scope` and `no method named `teams_offered``.

**Step 3: Write the implementation**

In `struct Frame`, after `runs: Vec<Run>,`:

```rust
    team: Option<Team>,
    teams: Vec<Team>,
```

After `const INTERRUPT: &str = "interrupt-";`:

```rust
const TEAMS: &str = "teams";
```

After `const ACCESS_CAP: &str = "access.v1";`:

```rust
const TEAM_CAP: &str = "team.v1";
```

Replace `CAPS`:

```rust
const CAPS: [&str; 16] = [PAIR_CAP, "scopes.v1", "summary.v2", "host.v1", OPEN_CAP, NAMES_CAP, AUTOMATIONS_CAP, LOCALS_CAP, FORK_CAP, ACCOUNTS_CAP, "subagents.v1", SETTINGS_CAP, "commands.v1", ACCESS_CAP, "skills.v1", TEAM_CAP];
```

In `impl Agents`, after `pub fn access_offered`:

```rust
    /// pocketd plans and runs teams for this owner.
    pub fn teams_offered(&self) -> bool {
        self.caps.iter().any(|c| c == TEAM_CAP) && !self.observe_only()
    }
```

After `fn automation_event`:

```rust
fn team_event(f: &Frame) -> Option<Event> {
    match f.kind.as_str() {
        "team.update" => f.team.clone().map(Event::Team),
        "team.list" => Some(Event::Teams(f.teams.clone())),
        _ => None,
    }
}

fn team_list_request() -> Value {
    json!({"type": "team.list", "id": TEAMS})
}
```

In `run`, extend the parser chain:

```rust
        if let Some(ev) = requests.answer(&f).or_else(|| launch_event(&f)).or_else(|| names_event(&f)).or_else(|| automation_event(&f)).or_else(|| team_event(&f)) {
```

and in the `"hello.ok"` arm, send the request after `Connected` when `team.v1` was negotiated:

```rust
            "hello.ok" => {
                let host = host_event(&f);
                let teams = f.caps.iter().any(|c| c == TEAM_CAP);
                tx.unbounded_send(Event::Connected { scopes: f.scopes, caps: f.caps, version: f.version }).ok()?;
                if teams {
                    ws.send(Message::text(team_list_request().to_string())).ok()?;
                }
                let Some(ev) = host else { continue };
                ev
            }
```

In `.ui-review/fixture/fixture.ts`, the `hello.ok` reply's caps gain `"team.v1"` at the end, so the capture screens show **Team**:

```ts
caps: ["pair.v1", "scopes.v1", "summary.v2", "host.v1", "automations.v1", "locals.v1", "settings.v1", "access.v1", "team.v1"],
```

The fixture ignores verbs it doesn't know, `team.list` included; plan 6's capture steps apply teams in-process.

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p agents`
Expected: PASS. The older `hello.ok` tests (`connected_carries_the_scopes_from_hello_ok`, `the_host_from_hello_ok_outlives_the_connect`, …) still pass: they never read past `hello.ok`, and their goldens carry no `team.v1`.

### Task 1.3: `Outbox` team verbs and `team` on prompt

**Files:**
- Modify: `packages/desktop/crates/agents/src/agents.rs` (`Outbox`, a `TEAM` constant and `team_id`, tests)
- Modify: `packages/desktop/crates/hosts/src/session.rs` (`Outboxes::prompt`, new `Outboxes::team_start/team_stop/team_merge`, tests)
- Modify (`prompt` gains `, false`): `packages/desktop/crates/pocket/src/chat/composer.rs` (3 calls: in `chat_command`'s `Command::Send` arm and twice in `chat_send`), `pocket/src/chat/transcript.rs` (2 calls, `self.outbox.prompt(&id, &text)`), `pocket/src/chat/composer/queue.rs` (`self.outbox.prompt(agent, &text)`), `pocket/src/chat/composer/goal.rs` (`this.outbox.prompt(&agent, text)`), `pocket/src/capture.rs` (`d.outbox.prompt(&agent, &format!("/fixture {scenario}"))`)

**Context:** `sent_but_id(|o| …)` runs one `Outbox` call against a channel and returns the frame without its `id`. Pocket never holds an `Outbox` directly: `self.outbox` is `hosts::session::Outboxes`, which untags an id and sends to the pocketd holding it (`by_id`). Ids of team verbs are `team-<n>`; Task 1.4 tells their refusals apart by the request each answers. `"team"` goes on `agent.prompt` only when set, so a plain prompt's frame stays byte-for-byte what pocketd's golden `client/agent_prompt.json` holds.

**Step 1: Write the failing tests**

In `agents.rs` tests, change `prompt_sends_the_golden_agent_prompt_frame_but_its_id`'s call to `o.prompt("a1", "hi", false);`, change the two calls in `each_prompt_goes_out_under_its_own_id_and_its_refusal_reaches_the_app` to `out.prompt("a1", "Fix it", false)`, and add after `prompt_sends_the_golden_agent_prompt_frame_but_its_id`:

```rust
    #[test]
    fn a_team_prompt_carries_team_and_a_plain_one_leaves_it_out() {
        assert_eq!(sent_but_id(|o| _ = o.prompt("a1", "Split the parser", true)), json!({"type": "agent.prompt", "agentId": "a1", "text": "Split the parser", "team": true}));
        assert_eq!(sent_but_id(|o| _ = o.prompt("a1", "hi", false)), json!({"type": "agent.prompt", "agentId": "a1", "text": "hi"}));
    }

    #[test]
    fn team_messages_name_the_lead_and_the_tasks_kept() {
        let pick = TaskPick { id: "t1".into(), agent: "codex".into(), model: "gpt-6-sol".into(), allow: vec!["cargo test".into()] };
        let bare = TaskPick { id: "t2".into(), agent: "claude".into(), model: String::new(), allow: vec![] };
        assert_eq!(sent_but_id(|o| o.team_list()), json!({"type": "team.list"}));
        assert_eq!(
            sent_but_id(|o| o.team_start("a1", &[pick, bare])),
            json!({"type": "team.start", "lead": "a1", "tasks": [{"id": "t1", "agent": "codex", "model": "gpt-6-sol", "allow": ["cargo test"]}, {"id": "t2", "agent": "claude"}]})
        );
        assert_eq!(sent_but_id(|o| o.team_stop("a1", "t2")), json!({"type": "team.stop", "lead": "a1", "task": "t2"}));
        assert_eq!(sent_but_id(|o| o.team_merge("a1")), json!({"type": "team.merge", "lead": "a1"}));
    }
```

In `session.rs` tests, change `out.prompt("devbox/a1", "hi");` in `a_host_agents_prompt_goes_to_that_host_untagged` to `out.prompt("devbox/a1", "hi", false);` and add after that test (`use super::*;` already brings in `json!` and `Value`):

```rust
    #[test]
    fn a_host_leads_team_ops_reach_that_host_with_its_own_ids() {
        let (out, here, there) = outboxes();
        out.prompt("devbox/a1", "Plan it", true);
        out.team_start("devbox/a1", &[agents::TaskPick { id: "t1".into(), agent: "claude".into(), model: String::new(), allow: vec![] }]);
        out.team_stop("devbox/a1", "t1");
        out.team_merge("a2");
        let sent: Vec<Value> = there.try_iter().collect();
        assert_eq!((&sent[0]["agentId"], &sent[0]["team"]), (&json!("a1"), &json!(true)));
        assert_eq!((&sent[1]["type"], &sent[1]["lead"]), (&json!("team.start"), &json!("a1")));
        assert_eq!((&sent[2]["type"], &sent[2]["lead"], &sent[2]["task"]), (&json!("team.stop"), &json!("a1"), &json!("t1")));
        let mine = here.try_iter().next().unwrap();
        assert_eq!((&mine["type"], &mine["lead"]), (&json!("team.merge"), &json!("a2")));
    }
```

What they prove: the wire frames match the index's contract; `team` appears only when asked; a host lead's verbs reach that host untagged.

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p agents team`
Expected: FAIL to compile with `this method takes 2 arguments but 3 arguments were supplied` and `no method named `team_list` found`.

**Step 3: Write the implementation**

In `agents.rs`, after `const TEAMS: &str = "teams";`:

```rust
const TEAM: &str = "team-";
```

After `fn auto_id()`:

```rust
fn team_id() -> String {
    format!("{TEAM}{}", NEXT.fetch_add(1, Ordering::Relaxed))
}
```

Replace `Outbox::prompt` with:

```rust
    /// Types `text` into the agent's terminal and presses Enter; `team` asks it for a team plan. Returns the request id a refusal names.
    pub fn prompt(&self, agent_id: &str, text: &str, team: bool) -> String {
        let id = format!("{PROMPT}{}", NEXT.fetch_add(1, Ordering::Relaxed));
        let mut msg = json!({"type": "agent.prompt", "id": id, "agentId": agent_id, "text": text});
        if team {
            msg["team"] = true.into();
        }
        self.send(msg);
        id
    }
```

After `automation_run`:

```rust
    /// Asks for every team pocketd holds; `Event::Teams` answers. Each connect asks on its own.
    pub fn team_list(&self) {
        self.send(team_list_request());
    }

    /// Starts `lead`'s team with the tasks kept; a task left out is removed.
    pub fn team_start(&self, lead: &str, tasks: &[TaskPick]) {
        self.send(json!({"type": "team.start", "id": team_id(), "lead": lead, "tasks": tasks}));
    }

    pub fn team_stop(&self, lead: &str, task: &str) {
        self.send(json!({"type": "team.stop", "id": team_id(), "lead": lead, "task": task}));
    }

    /// Merges each task's branch into the lead's checkout.
    pub fn team_merge(&self, lead: &str) {
        self.send(json!({"type": "team.merge", "id": team_id(), "lead": lead}));
    }
```

In `session.rs`, change the import to:

```rust
use agents::{Decision, Event, McpAction, Outbox, Summary, TaskPick, Team};
```

Replace `Outboxes::prompt` with:

```rust
    /// The request id a refusal names, once sent.
    pub fn prompt(&self, agent_id: &str, text: &str, team: bool) -> Option<String> {
        let (host, theirs) = untag(agent_id);
        self.to(host).map(|out| out.prompt(theirs, text, team))
    }
```

After `Outboxes::reply`:

```rust
    pub fn team_start(&self, lead: &str, tasks: &[TaskPick]) {
        self.by_id(lead, |o, id| o.team_start(id, tasks));
    }

    pub fn team_stop(&self, lead: &str, task: &str) {
        self.by_id(lead, |o, id| o.team_stop(id, task));
    }

    pub fn team_merge(&self, lead: &str) {
        self.by_id(lead, |o, id| o.team_merge(id));
    }
```

In `pocket`, add `, false` as the last argument of every `outbox.prompt(` call listed under **Files** (7 calls). Find them with `grep -rn "outbox.prompt(" crates/pocket/src`; after the change every hit ends in `, false)`.

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p agents && cargo test -p hosts session && cargo build -p pocket`
Expected: PASS, and `pocket` builds.

### Task 1.4: Refused team requests become `Event::TeamFailed`

**Files:**
- Modify: `packages/desktop/crates/agents/src/team.rs` (new `TeamAction`)
- Modify: `packages/desktop/crates/agents/src/agents.rs` (the `pub use team::…` line, `Event`, `apply_from`, `Requests::sent`, `refused`, tests)
- Modify: `packages/desktop/crates/hosts/src/session.rs` (`tag_event`, tests)
- Modify: `packages/desktop/crates/pocket/src/desktop/alerts.rs` (`on_agents_from`, the `agents` import)

**Context:** `run` writes each queued message to the socket, then `Requests::sent` keeps the prompts and stops so that `Requests::answer` can turn the `error` that answers one into `PromptFailed` or `InterruptFailed` (`refused`). A message the socket never took goes through `refused(&m, UNSENT)` at once. Today a team verb's `error` reaches nothing: `launch_event`'s catch-all `"error"` arm returns `None` for ids not starting `create-`. Plan 6 shows each refusal as a toast; this task carries it to the app. A refused team prompt (`agent.prompt` with `"team": true`, refused e.g. `team_busy`) still fails its send in the chat, as `PromptFailed` did, so `TeamAction::Plan` carries its request id. `readable` turns pocketd's `Malformed message` (an older pocketd) into `OUTDATED`. `tag_event`'s and `apply_from`'s matches are exhaustive. A host's request ids stay in that host's terms, as `PromptFailed`'s do: the chat noted the id `Outboxes::prompt` returned.

**Step 1: Write the failing tests**

Add to `mod tests` in `agents.rs`, after `a_prompt_or_stop_the_socket_never_took_fails_and_nothing_else_does`:

```rust
    #[test]
    fn a_refused_team_request_names_its_lead_what_it_asked_and_why() {
        let mut requests = Requests::default();
        requests.sent(&json!({"type": "agent.prompt", "id": "prompt-1", "agentId": "a1", "text": "Split it", "team": true}));
        requests.sent(&json!({"type": "team.start", "id": "team-2", "lead": "a1", "tasks": []}));
        requests.sent(&json!({"type": "team.stop", "id": "team-3", "lead": "a1", "task": "t1"}));
        requests.sent(&json!({"type": "team.merge", "id": "team-4", "lead": "a1"}));
        let busy = requests.answer(&frame(json!({"type": "error", "id": "prompt-1", "code": "team_busy", "message": "This session's team is still running or waiting to merge"})));
        assert!(matches!(busy, Some(Event::TeamFailed { lead, action: TeamAction::Plan(request), message }) if lead == "a1" && request == "prompt-1" && message == "This session's team is still running or waiting to merge"));
        let start = requests.answer(&frame(json!({"type": "error", "id": "team-2", "message": "Malformed message"})));
        assert!(matches!(start, Some(Event::TeamFailed { lead, action: TeamAction::Start, message }) if lead == "a1" && message == OUTDATED));
        let stop = requests.answer(&frame(json!({"type": "error", "id": "team-3", "message": "Task t1 isn't running"})));
        assert!(matches!(stop, Some(Event::TeamFailed { action: TeamAction::Stop, message, .. }) if message == "Task t1 isn't running"));
        assert!(requests.answer(&frame(json!({"type": "ack", "id": "team-4"}))).is_none());
    }

    #[test]
    fn a_team_request_the_socket_never_took_fails_and_a_plain_prompt_still_fails_as_before() {
        let merge = json!({"type": "team.merge", "id": "team-1", "lead": "a1"});
        assert!(matches!(refused(&merge, UNSENT), Some(Event::TeamFailed { lead, action: TeamAction::Merge, message }) if lead == "a1" && message == UNSENT));
        let plain = json!({"type": "agent.prompt", "id": "prompt-1", "agentId": "a1", "text": "hi"});
        assert!(matches!(refused(&plain, UNSENT), Some(Event::PromptFailed { agent, request }) if agent == "a1" && request == "prompt-1"));
    }
```

Add to `mod tests` in `hosts/src/session.rs`, after `a_hosts_team_names_its_lead_workers_and_worktrees_in_the_desktops_terms`:

```rust
    #[test]
    fn a_hosts_refused_team_request_names_its_lead_in_the_desktops_terms() {
        let mut ev = Event::TeamFailed { lead: "a1".into(), action: agents::TeamAction::Plan("prompt-1".into()), message: "busy".into() };
        assert!(tag_event("devbox", &mut ev));
        assert!(matches!(ev, Event::TeamFailed { lead, action: agents::TeamAction::Plan(request), .. } if lead == "devbox/a1" && request == "prompt-1"));
    }
```

What they prove: each team request's refusal names its lead, the action and pocketd's message (an outdated pocketd's in words); an ack reports nothing; a request the socket never took fails the same way; a plain prompt still fails as `PromptFailed`; a host's refusal names its lead in the desktop's terms and keeps its request id.

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p agents team && cargo test -p hosts session`
Expected: FAIL to compile with `no variant or associated item named `TeamFailed` found for enum `Event`` and `cannot find type `TeamAction` in this scope`.

**Step 3: Write the implementation**

In `team.rs`, append:

```rust
/// The team request pocketd refused.
#[derive(Clone, Debug, PartialEq)]
pub enum TeamAction {
    /// A team prompt, sent under this request id.
    Plan(String),
    Start,
    Stop,
    Merge,
}
```

In `agents.rs`, the re-export becomes:

```rust
pub use team::{TaskPick, Team, TeamAction, TeamTask};
```

In `pub enum Event`, after `Teams(Vec<Team>),`:

```rust
    /// pocketd refused a team request from `lead`'s chat, or the socket never took it; `message` says why.
    TeamFailed { lead: String, action: TeamAction, message: String },
```

In `apply_from`, the final no-op arm gains `| Event::TeamFailed { .. }`:

```rust
            Event::AutomationError(_) | Event::PromptFailed { .. } | Event::InterruptFailed { .. } | Event::Commands { .. } | Event::Skills { .. } | Event::Btw { .. } | Event::Mcp { .. } | Event::McpActFailed { .. } | Event::TeamFailed { .. } => {}
```

Replace the `Requests` doc comment, `Requests::sent`, `answer`'s doc comment and `refused`:

```rust
/// Prompts, stops and team verbs written to pocketd and not yet answered, so a refusal names the one it refused.
#[derive(Default)]
struct Requests(VecDeque<Value>);

impl Requests {
    fn sent(&mut self, m: &Value) {
        if matches!(m["type"].as_str(), Some("agent.prompt" | "agent.interrupt" | "team.start" | "team.stop" | "team.merge")) {
            self.0.push_back(m.clone());
        }
    }

    /// Drops the request `f` answers; a refusal becomes `PromptFailed`, `InterruptFailed` or `TeamFailed`.
```

```rust
/// The event for a prompt, stop or team verb `m` that failed for `reason`; none for other messages.
fn refused(m: &Value, reason: &str) -> Option<Event> {
    let field = |key: &str| m[key].as_str().unwrap_or_default().to_string();
    let team = |lead: String, action| Some(Event::TeamFailed { lead, action, message: readable(reason) });
    match m["type"].as_str()? {
        "agent.prompt" if m["team"] == true => team(field("agentId"), TeamAction::Plan(field("id"))),
        "agent.prompt" => Some(Event::PromptFailed { agent: field("agentId"), request: field("id") }),
        "agent.interrupt" => Some(Event::InterruptFailed { agent: field("agentId"), reason: reason.into() }),
        "team.start" => team(field("lead"), TeamAction::Start),
        "team.stop" => team(field("lead"), TeamAction::Stop),
        "team.merge" => team(field("lead"), TeamAction::Merge),
        _ => None,
    }
}
```

In `hosts/src/session.rs`'s `tag_event`, the `PromptFailed` line becomes:

```rust
        Event::PromptFailed { agent, .. } | Event::InterruptFailed { agent, .. } | Event::Commands { agent, .. } | Event::Mcp { agent, .. } | Event::McpActFailed { agent, .. } | Event::TeamFailed { lead: agent, .. } => t(agent),
```

In `pocket/src/desktop/alerts.rs`, the import becomes `use agents::{Agents, Decision, Event, Host, Permission, Summary, TeamAction};`, and in `on_agents_from`, right after the `if let Event::PromptFailed { agent, request } = &ev { … }` block:

```rust
        if let Event::TeamFailed { lead, action, .. } = &ev {
            // A refused team prompt fails its send, as a refused prompt does.
            if let (TeamAction::Plan(request), Some(v)) = (action, self.chats.views.get_mut(lead)) {
                v.session.outgoing.fail(request);
            }
            return cx.notify();
        }
```

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p agents && cargo test -p hosts session && cargo build -p pocket`
Expected: PASS, and `pocket` builds.

### Task 1.5: Cross-check against pocketd's goldens, then the workspace

**Files:**
- Test: `packages/desktop/crates/agents/src/agents.rs` (tests module, only for goldens that exist)

**Context:** pocketd keeps a golden JSON per frame in `packages/pocketd/internal/proto/testdata/golden/{client,server}/`; the desktop tests read them with `golden!("client/…")`. Plan 3 adds `client/team_list.json`, `client/agent_prompt_team.json`, `client/agent_create_team.json`, `server/team_update.json`, `server/team_list.json` and `server/team_list_empty.json`; plan 4 adds `client/team_start.json` (its second task has no `model` or `allow`), `client/team_stop.json` and `client/team_merge.json`. `agent_create_team.json` is a spec, not an `Outbox` call, so it gets no test here.

**Step 1: List the team goldens**

Run (from the repo root): `ls packages/pocketd/internal/proto/testdata/golden/client packages/pocketd/internal/proto/testdata/golden/server | grep -i team`
Expected: the files above, those of a plan not merged yet missing. Nothing listed means skip to Step 3.

**Step 2: Add a test per golden found**

For `client/team_list.json`, exactly:

```rust
    #[test]
    fn team_list_sends_the_golden_team_list_frame_but_its_id() {
        assert_eq!(sent_but_id(|o| o.team_list()), without_id(golden!("client/team_list.json")));
    }
```

For each other client golden, the same shape, calling the method with the golden's own values (open the file; e.g. for `{"type":"team.stop","id":"x","lead":"a1","task":"t2"}` call `o.team_stop("a1", "t2")`; for `team.start` build one `TaskPick` per entry of its `tasks`; for a prompt golden with `"team":true` call `o.prompt(<agentId>, <text>, true)`). For `server/team_update.json` and `server/team_list.json`:

```rust
    #[test]
    fn the_golden_team_frames_decode_into_team_events() {
        let update = golden!("server/team_update.json");
        let lead = update["team"]["lead"].as_str().unwrap().to_string();
        assert!(matches!(team_event(&frame(update.clone())), Some(Event::Team(t)) if t.lead == lead && t.tasks.len() == update["team"]["tasks"].as_array().unwrap().len()));
        let list = golden!("server/team_list.json");
        assert!(matches!(team_event(&frame(list.clone())), Some(Event::Teams(t)) if t.len() == list["teams"].as_array().unwrap().len()));
    }
```

(drop the half whose golden doesn't exist). A test that fails here means this plan and pocketd disagree on the wire: fix the side that departs from the index, don't loosen the test.

Run: `cargo test -p agents golden`
Expected: PASS.

**Step 3: Run the workspace checks**

Run (from `packages/desktop`): `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: builds, clippy prints no warning in a file this PR touched, every test passes.

---

## PR 2: The "+" menu and the Team chip

**Scope:** `Draft.team`, `Menu::Add`, the decisions (`team_offered`, `ask_team`, `team_send`, the footer hint), the `add_menu` and `team_chip` components, the `team` icon, both composers wired, sends carrying `team` (a failed team send's Retry sets the chip again), capture steps, before/after screens, the ADR map. Not the plan sheet, island or anything that shows a team (plan 6).
**Depends on:** PR 1.
**Done when:** the workspace checks pass, and the after screens show the "+" menu with Attach images and Team in both composers and the purple `Team ×` chip in both bars, with the plain screens unchanged from before.

### Task 2.1: Capture the screens before

**Files:** none changed.

**Context:** `capture.sh` builds the release app with the `capture` feature, starts the fixture pocketd for the screen's scenario (`chat*` names get the chat scenario, others the sessions scenario) and runs the named steps. Step `chat` opens the fixture's Claude session as a chat; `chat-access` marks it `chat_only` (a chat pocketd drives); `compose` opens the New session page.

**Step 1: Capture**

Run (from the repo root): `.ui-review/fixture/capture.sh /tmp/team-trigger/before chat=chat,chat-access compose=compose`
Expected output (after the build):
```
/tmp/team-trigger/before/impl-chat.png
/tmp/team-trigger/before/impl-compose.png
```

### Task 2.2: The draft's Team flag and the decisions behind it

**Files:**
- Modify: `packages/desktop/crates/pocket/src/composer.rs` (`Menu`, `Draft`, new `team_offered` and `ask_team`, tests)

**Context:** `Draft` is a composer's choices besides its words; `Draft::clear()` resets it to `Default`, which is what a send does, so a cleared draft has no chip. `Draft.menu` is the one open menu. Both composers use this `Draft`: the new session page's (`new_form.composer.draft`) and the chat's (`chats.composer.draft`, parked per chat by `chat::drafts::Drafts` while another chat holds the input, so `team` moves with its chat). The spec a create sends gets `"chat": true` from `store::prefs::Agents::launch` only when **Start as a chat** is on. Until Task 2.3 uses them, `Menu::Add`, `toggle_team`, `team_offered` and `ask_team` raise dead-code warnings; that's expected mid-PR.

**Step 1: Write the failing tests**

In `composer.rs`'s `mod tests`, change the first `use` to:

```rust
    use super::{Attachment, Draft, Menu, ask_team, attachment_path, copied_files, pasted_image, save_image, split_images, team_offered};
    use serde_json::json;
```

and add:

```rust
    #[test]
    fn the_add_menus_team_row_sets_the_chip_and_closes_the_menu_and_a_sent_draft_drops_it() {
        let mut draft = Draft::default();
        assert!(draft.toggle(Menu::Add));
        draft.toggle_team();
        assert!(draft.team && draft.menu.is_none());
        draft.toggle(Menu::Add);
        draft.toggle_team();
        assert!(!draft.team, "picked again, it comes off");
        draft.toggle_team();
        draft.clear();
        assert!(!draft.team, "the next draft starts without it");
    }

    #[test]
    fn team_is_offered_only_to_a_chat_claude_or_codex_drives_on_a_pocketd_that_serves_teams() {
        assert!(team_offered(true, true, "claude"));
        assert!(team_offered(true, true, "codex"));
        assert!(!team_offered(false, true, "claude"), "pocketd doesn't advertise team.v1");
        assert!(!team_offered(true, false, "claude"), "a terminal session can't lead");
        assert!(!team_offered(true, true, "opencode"), "a provider this build doesn't know");
    }

    #[test]
    fn a_create_asks_for_a_team_only_when_the_chip_is_set_on_a_chat_that_can_lead() {
        let ask = |mut spec: serde_json::Value, team: bool, served: bool| {
            ask_team(&mut spec, team, served);
            spec.get("team").cloned()
        };
        assert_eq!(ask(json!({"provider": "claude", "chat": true}), true, true), Some(json!(true)));
        assert_eq!(ask(json!({"provider": "codex", "chat": true}), true, true), Some(json!(true)));
        assert_eq!(ask(json!({"provider": "claude"}), true, true), None);
        assert_eq!(ask(json!({"provider": "claude", "chat": true}), false, true), None);
        assert_eq!(ask(json!({"provider": "claude", "chat": true}), true, false), None);
    }
```

What they prove: the Team row toggles the chip and closes the menu, a cleared draft loses it; only a chat Claude or Codex drives can lead, and only on a pocketd that serves teams; a create carries `team` only then.

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p pocket composer::tests`
Expected: FAIL to compile with `no variant named `Add``, `no method named `toggle_team``, `unresolved imports `super::ask_team`, `super::team_offered``.

**Step 3: Write the implementation**

In `pub(crate) enum Menu`, after `Switch,`:

```rust
    /// The "+" button's: Attach images, and Team.
    Add,
```

In `pub(crate) struct Draft`, after `pub(crate) images: Vec<Attachment>,`:

```rust
    /// The Team chip: the next send asks the agent for a team plan.
    pub(crate) team: bool,
```

In `impl Draft`, after `toggle`:

```rust
    /// The "+" menu's Team row: sets the Team chip, or takes it off, and closes the menu.
    pub(crate) fn toggle_team(&mut self) {
        (self.team, self.menu) = (!self.team, None);
    }
```

After `pub(crate) fn split_images`:

```rust
/// A team's lead plans in a chat pocketd drives, which only Claude and Codex have; `served` is `Agents::teams_offered`.
pub(crate) fn team_offered(served: bool, chat: bool, provider: &str) -> bool {
    served && chat && matches!(provider, "claude" | "codex")
}

/// Has a create's first prompt ask for a team plan, when `team` is set and the session the spec starts can lead one.
pub(crate) fn ask_team(spec: &mut serde_json::Value, team: bool, served: bool) {
    if team && team_offered(served, spec["chat"] == true, spec["provider"].as_str().unwrap_or_default()) {
        spec["team"] = true.into();
    }
}
```

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p pocket composer::tests`
Expected: PASS.

### Task 2.3: The "+" menu and the Team chip in both composers

**Files:**
- Create: `packages/desktop/crates/pocket/src/composer/add_menu.rs`
- Create: `packages/desktop/crates/pocket/src/composer/team_chip.rs`
- Create: `packages/desktop/crates/theme/assets/icons/team.svg`
- Modify: `packages/desktop/crates/theme/src/theme.rs` (the `embed!` icon list)
- Modify: `packages/desktop/crates/pocket/src/composer.rs` (module list, imports, `Desktop::composer`)
- Modify: `packages/desktop/crates/pocket/src/chat/composer.rs` (`prompt_box`, new `chat_team`)
- Modify: `packages/desktop/crates/pocket/src/chat/composer/attachments.rs` (remove `attach_button` and the imports only it used)
- Modify: `docs/adr/0003-desktop-code-layout.md` (the map)

**Context:** View code, so no tests here (no render tests); the decisions it calls were tested in Task 2.2, and Task 2.5 checks the screens. Chips toggle their menu on `capture_any_mouse_down` with `cx.stop_propagation()` (see `composer/launch.rs::chip`, `chat/composer/model_pill.rs`): the capture runs before the open menu's `on_mouse_down_out`, so pressing the chip again closes the menu instead of closing and reopening it. Stopping propagation also stops the chat box's own `on_mouse_down`, which hands the one chat input (and its draft) to the pressed chat, so the chat's toggle calls `chat_take_input` itself. Rows use `on_click`. Menus are built like `chip_menu` (chat) and `composer::menu` (new session): `ui::pop`, `occlude`, `on_mouse_down_out` closing, `ui::menu_in`. `GAP`/`PAD` come from `composer::model_menu`. `Desktop::pick_path(false, window, cx, then)` opens the file picker (today's Attach images). The chat composer's `prompt_box` already computes `held` (this chat holds the input). Icons are compiled in by name through `embed!` in `theme.rs`; an icon missing from that list draws nothing.

**Step 1: Add the icon**

Create `crates/theme/assets/icons/team.svg` (the prototype's `TEAM_ICON`, `.ui-review/prototypes/orchestrator-team.html` line ~554):

```svg
<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 14 14" fill="none"><circle cx="7" cy="3.2" r="1.9" fill="#000"/><circle cx="3" cy="10.4" r="1.9" fill="#000"/><circle cx="11" cy="10.4" r="1.9" fill="#000"/><path d="M7 5.3v2.1M7 7.4 4.2 9M7 7.4 9.8 9" stroke="#000" stroke-width="1.1" stroke-linecap="round"/></svg>
```

In `theme.rs`'s `const ICONS: &[(&str, &[u8])] = embed!(…)`, insert `"team",` between `"stop",` and `"terminal",`.

**Step 2: Write the components**

Create `crates/pocket/src/composer/add_menu.rs`:

```rust
use super::model_menu::{GAP, PAD};
use super::{Draft, Menu};
use crate::desktop::Desktop;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

/// Which way the menu opens: down on the new session page, up from the chat's box at the window's foot.
#[derive(Clone, Copy)]
pub(crate) enum Opens {
    Down,
    Up,
}

/// The "+" button and, while `open`, its menu: Attach images, and Team, checked while set, where `team` isn't `None`.
/// `toggle` opens or closes the menu; `draft` reaches the box's draft.
pub(crate) fn add_button(
    id: impl Into<ElementId>,
    open: bool,
    team: Option<bool>,
    opens: Opens,
    draft: impl Fn(&mut Desktop) -> &mut Draft + Copy + 'static,
    toggle: impl Fn(&mut Desktop, &mut Window, &mut Context<Desktop>) + 'static,
    cx: &mut Context<Desktop>,
) -> Div {
    let button = div()
        .id(id)
        .size(px(32.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(10.))
        .cursor_pointer()
        .when(open, |d| d.bg(FILL_3))
        .hover(|s| s.bg(FILL_3))
        .active(|s| s.bg(FILL_4))
        .child(icon("plus", 17., TEXT_2))
        .capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            toggle(this, window, cx);
        }));
    div().relative().flex_none().child(button).children(open.then(|| menu(team, opens, draft, cx)))
}

fn menu(team: Option<bool>, opens: Opens, draft: impl Fn(&mut Desktop) -> &mut Draft + Copy + 'static, cx: &mut Context<Desktop>) -> Div {
    let attach = ui::menu_row("composer-add-images", "file-plus", "Attach images", None).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
        draft(this).menu = None;
        this.pick_path(false, window, cx, move |d, paths, _, _| {
            draft(d).drop_images(&paths);
        });
        cx.notify();
    }));
    let team = team.map(|on| {
        ui::menu_pick("composer-add-team", icon("team", 14., TEXT_2), "Team", on).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            draft(this).toggle_team();
            cx.notify();
        }))
    });
    let menu = ui::pop(div().id("composer-add-menu"))
        .w(px(200.))
        .p(px(PAD))
        .flex()
        .flex_col()
        .gap(px(GAP))
        // Menus hang over what's under them; without this a click on one also lands there.
        .occlude()
        .child(attach)
        .children(team)
        .on_mouse_down_out(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            draft(this).menu = None;
            cx.notify();
        }));
    let menu = ui::menu_in("composer-add-menu-in", menu);
    match opens {
        Opens::Down => ui::dropdown_right(36., menu),
        Opens::Up => div().absolute().top(px(-6.)).right_0().child(deferred(anchored().anchor(Anchor::BottomRight).snap_to_window_with_margin(px(8.)).child(menu)).with_priority(1)),
    }
}
```

Create `crates/pocket/src/composer/team_chip.rs`:

```rust
use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;

/// The Team chip: the next send asks the agent for a team plan. `remove` is its ×.
pub(crate) fn team_chip(id: impl Into<ElementId>, remove: impl Fn(&mut Desktop, &mut Window, &mut Context<Desktop>) + 'static, cx: &mut Context<Desktop>) -> Div {
    let ink = Hsla::from(COMMAND);
    let x = div()
        .id(id)
        .size(px(18.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(9.))
        .cursor_pointer()
        .hover(move |s| s.bg(ink.opacity(0.15)))
        .child(icon("x", 10., ink.opacity(0.8)))
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            remove(this, window, cx);
            cx.notify();
        }));
    div()
        .h(px(26.))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(5.))
        .pl(px(8.))
        .pr(px(4.))
        .rounded(px(8.))
        .bg(ink.opacity(0.1))
        .border_1()
        .border_color(ink.opacity(0.25))
        .text_size(px(12.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(ink)
        .child(icon("team", 13., ink))
        .child("Team")
        .child(x)
}
```

**Step 3: Wire the new session composer**

In `composer.rs`, change the module list at the top to:

```rust
pub(crate) mod add_menu;
pub(crate) mod launch;
pub(crate) mod model_menu;
pub(crate) mod pills;
pub(crate) mod team_chip;
```

delete the line `use crate::chat::composer::attachments::attach_button;` (keep every other `use`), and add after the existing imports:

```rust
use add_menu::Opens;
```

In `Desktop::composer`, change the doc's last words "`trailing` comes before attach and `send`" to "`trailing` comes before the + and `send`", add after `let pick = …;`:

```rust
        let team = team_offered(self.agents.teams_offered(), self.store.agents.start_as_chat, &pick.provider).then_some(c.draft.team);
```

and replace the `let bar = prompt_bar()…` chain with:

```rust
        let bar = prompt_bar()
            .child(self.model_pill(c, &pick, project.clone(), cx))
            .children(self.effort_chip(c, &pick, project.clone(), cx))
            .child(self.access_chip(c, &pick, project, cx))
            .children((team == Some(true)).then(|| team_chip::team_chip("composer-team-remove", move |this, _, _| slot(this).draft.team = false, cx)))
            .child(div().flex_1())
            .children(trailing)
            .child(add_menu::add_button("composer-add", c.draft.menu == Some(Menu::Add), team, Opens::Down, move |d| &mut slot(d).draft, move |this, _, cx| this.toggle_composer(slot, Menu::Add, cx), cx))
            .child(send);
```

**Step 4: Wire the chat composer**

In `chat/composer.rs`, add to the imports:

```rust
use crate::composer::add_menu::{self, Opens};
use crate::composer::{team_chip, team_offered};
```

After `chat_draft` in `impl Desktop`:

```rust
    /// Whether `agent`'s draft has the Team chip set, held in the input or parked.
    fn chat_team(&self, agent: &str) -> bool {
        if self.chats.drafts.owner() == Some(agent) {
            return self.chats.composer.draft.team;
        }
        self.chats.drafts.parked(agent).is_some_and(|(_, draft)| draft.team)
    }
```

In `prompt_box`, replace the line `let attach = (!view_only).then(|| attachments::attach_button(…));` with:

```rust
        let team = team_offered(self.agents.teams_offered(), agent.chat_only, &agent.provider).then(|| self.chat_team(&agent.id));
        let add = (!view_only).then(|| {
            let (agent, open) = (agent.id.clone(), held && self.chats.composer.draft.menu == Some(Menu::Add));
            let toggle = move |this: &mut Desktop, window: &mut Window, cx: &mut Context<Desktop>| {
                this.chat_take_input(&agent, window, cx);
                this.chats.composer.draft.toggle(Menu::Add);
                cx.notify();
            };
            add_menu::add_button(id(format!("chat-add:{terminal}")), open, team, Opens::Up, |d| &mut d.chats.composer.draft, toggle, cx)
        });
        let chip = (!view_only && team == Some(true)).then(|| {
            let agent = agent.id.clone();
            team_chip::team_chip(id(format!("chat-team-remove:{terminal}")), move |this, window, cx| {
                this.chat_take_input(&agent, window, cx);
                this.chats.composer.draft.team = false;
            }, cx)
        });
```

and in the `let bar = crate::composer::prompt_bar()…` chain, add `.children(chip)` right after the `chat_agents_chip` line and change `.children(attach)` to `.children(add)`.

In `chat/composer/attachments.rs`, delete `attach_button` (the whole `pub(crate) fn attach_button … }`), change `use crate::composer::{Attachment, Draft};` to `use crate::composer::Attachment;`, and delete `use gpui_kit::component::tooltip::Tooltip;`.

**Step 5: Update the ADR map**

In `docs/adr/0003-desktop-code-layout.md`:
- in the `agents` row, replace "`agents/providers` the built-in table" with "`agents/team` each lead's team as pocketd reports it and the tasks Start keeps; `agents/providers` the built-in table";
- in the `composer` row, replace "`composer/pills` the commands picked or typed into a prompt, shown as pills" with "`composer/pills` the commands picked or typed into a prompt, shown as pills; `composer/add_menu` the + button's menu, Attach images and Team; `composer/team_chip` the Team chip the next send carries".

**Step 6: Build**

Run: `cargo build -p pocket && cargo clippy -p pocket --all-targets`
Expected: builds; clippy warns only about `ask_team` being unused (Task 2.4 uses it).

### Task 2.4: Sends carry the flag

**Files:**
- Modify: `packages/desktop/crates/pocket/src/chat/composer.rs` (`hint`, new `TeamSend`/`team_send`, `chat_command`, new `chat_prompt`, `chat_send`, `chat_composer`, tests)
- Modify: `packages/desktop/crates/pocket/src/modals/new_session.rs` (`start_session`)
- Modify: `packages/desktop/crates/chat/src/outgoing.rs` (`Pending`, new `asks_team`, `withdraw`, `retry`, `take`, tests)
- Modify: `packages/desktop/crates/pocket/src/chat/transcript.rs` (`chat_retry`)

**Context:** `chat_send` reads the typed text, works out `slash` (a `/command`), refuses terminal-only commands, then clears the draft and either runs a local command, sends a queue-skipping command, sends (`chat_command(Command::Send)`, which holds the send in `Outgoing` while the agent works) or steers into the running turn. A team send goes only when the agent is idle, so it always takes the plain-send path; the `Send` arm becomes `chat_prompt` so that path can pass `team`. `hint` picks the footer's text; it gains `team` (the chip is set and offered). In `start_session`, `store.agents.launch` is what adds `"chat": true`, so `ask_team` runs after it. A send that failed (refused, as `team_busy` is through Task 1.4, or never seen) shows Retry; `chat_retry` takes it back out of `chat::Outgoing` (`retry`) and puts its text back in the box (`chat_take_back`) for the user to send again. So the send remembers it asked for a team (`Pending.team`, set by `asks_team`), and Retry sets the chip again. `retry` has three callers in `outgoing.rs`'s tests and one in `chat_retry`.

**Step 1: Write the failing tests**

In `chat/composer.rs`'s `mod tests`, change the first `use` to:

```rust
    use super::{Mode, TeamSend, added, busy, composer_mode, hint, placeholder, said_in_terminal, stop_beside_steer, team_send, terminal_only_said, text_view_focused, typed_text, working};
```

append `, false` as the last argument of every existing `hint(` call in the module (10 calls: 1 in `sub_agents_left_in_the_background_leave_the_composer_free`, 6 in `the_footer_hints_at_what_enter_does_now`, 3 in `a_turn_being_stopped_reads_stopping_and_takes_sends_only_into_the_queue`), and add:

```rust
    #[test]
    fn a_team_prompt_asks_for_a_plan_only_from_an_idle_agent_and_never_for_a_command() {
        assert_eq!(team_send(true, false, false), TeamSend::On);
        assert_eq!(team_send(true, false, true), TeamSend::Wait, "the plan starts a turn of its own");
        assert_eq!(team_send(true, true, false), TeamSend::Off, "a command goes as typed");
        assert_eq!(team_send(false, false, true), TeamSend::Off);
    }

    #[test]
    fn the_footer_says_a_team_prompt_waits_for_the_turn_to_end() {
        assert_eq!(hint(Mode::Prompt, true, false, false, true, true), Some("Team waits for this turn to end · esc to stop"));
        assert_eq!(hint(Mode::Prompt, false, false, false, true, true), None, "an idle agent takes it now");
        assert_eq!(hint(Mode::Prompt, true, true, false, true, true), Some("Stopping…"));
    }
```

In `crates/chat/src/outgoing.rs`'s `mod tests`: in `retry_hands_back_a_failed_send` the second assert becomes `assert_eq!(o.retry(hello), Some(("hello".into(), false)));`; in `a_send_getting_through_after_a_refusal_lets_the_queue_go`, `let retried = o.retry(id(&o, "a")).unwrap();` becomes `let (retried, _) = o.retry(id(&o, "a")).unwrap();`; in `retrying_one_failed_send_leaves_the_others_alone`, `assert_eq!(o.retry(id(&o, "b")), Some("b".into()));` becomes `assert_eq!(o.retry(id(&o, "b")), Some(("b".into(), false)));`. Add after `retrying_one_failed_send_leaves_the_others_alone`:

```rust
    #[test]
    fn a_retried_team_send_asks_for_a_team_again() {
        let mut o = Outgoing::default();
        sent(&mut o, "Split the parser", "prompt-1");
        o.asks_team(id(&o, "Split the parser"));
        sent(&mut o, "hi", "prompt-2");
        o.fail("prompt-1");
        o.fail("prompt-2");
        assert_eq!(o.retry(id(&o, "Split the parser")), Some(("Split the parser".into(), true)));
        assert_eq!(o.retry(id(&o, "hi")), Some(("hi".into(), false)));
    }
```

What they prove: a team send goes only to an idle agent and never with a command; the footer says why Enter waits; a failed team send comes back as a team send, and a plain one as plain.

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p pocket chat::composer::tests`
Expected: FAIL to compile with `this function takes 5 arguments but 6 arguments were supplied` and `unresolved imports `super::TeamSend`, `super::team_send``.

Run: `cargo test -p chat outgoing`
Expected: FAIL to compile with `no method named `asks_team` found` and `expected `String`, found `(_, bool)``.

**Step 3: Write the implementation**

Replace `hint` with:

```rust
/// The footer's hint at what Enter does now; `team` while the Team chip is set.
pub(super) fn hint(mode: Mode, working: bool, stopping: bool, empty: bool, can_queue: bool, team: bool) -> Option<&'static str> {
    match mode {
        Mode::ViewOnly => None,
        Mode::Permission { .. } | Mode::Question { .. } => Some("Answer the request above to continue"),
        Mode::Prompt if !working => None,
        Mode::Prompt if stopping => Some("Stopping…"),
        Mode::Prompt if team => Some("Team waits for this turn to end · esc to stop"),
        Mode::Prompt if empty || !can_queue => Some("Working · esc to stop"),
        Mode::Prompt => Some("⏎ send into this turn · ⌘⏎ queue for after"),
    }
}

/// What the Team chip does to a send.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum TeamSend {
    /// The send goes as it would without the chip.
    Off,
    /// The send asks the agent for a team plan.
    On,
    /// The send stays in the box: the plan starts a turn of its own, and the queue can't carry the flag.
    Wait,
}

/// A command goes as typed, since pocketd would wrap it in the team rules.
pub(super) fn team_send(team: bool, command: bool, busy: bool) -> TeamSend {
    match (team && !command, busy) {
        (false, _) => TeamSend::Off,
        (true, true) => TeamSend::Wait,
        (true, false) => TeamSend::On,
    }
}
```

In `chat_command`, replace the whole `Command::Send(text) => { … }` arm with:

```rust
            Command::Send(text) => self.chat_prompt(agent, text, false),
```

and add after `chat_command`:

```rust
    /// Sends `text`, or holds it in the queue while the agent works; `team` asks for a team plan.
    fn chat_prompt(&mut self, agent: &str, text: String, team: bool) {
        let working = self.agents.get(agent).is_some_and(|a| self.chat_busy(a));
        let view = self.chats.views.get_mut(agent);
        let held = working && view.is_some();
        let request = if held { None } else { self.outbox.prompt(agent, &text, team) };
        if let Some(v) = view {
            let send = v.session.outgoing.push(text, working, now_ms());
            if let (Some(send), true) = (send, team) {
                v.session.outgoing.asks_team(send);
            }
            if let Some((send, request)) = send.zip(request) {
                v.session.outgoing.sent_as(send, request);
            }
            if !working {
                v.pin();
            }
        }
    }
```

In `chat_send`, right after the `if let Some((Kind::Unsupported, name, _)) = slash { … }` block and before `self.chats.composer.draft.clear();`:

```rust
        let team = team_send(self.chats.composer.draft.team && team_offered(self.agents.teams_offered(), chat_only, &provider), slash.is_some(), busy);
        if team == TeamSend::Wait {
            return cx.notify();
        }
```

and replace

```rust
        if !steer {
            return self.chat_command(&agent, Command::Send(text), cx);
        }
```

with

```rust
        if !steer {
            self.chat_prompt(&agent, text, team == TeamSend::On);
            return cx.notify();
        }
```

In `chat_composer`, replace the `let footer = …` line with:

```rust
        let team = team_offered(self.agents.teams_offered(), agent.chat_only, &agent.provider) && self.chat_team(&agent.id);
        let footer = self.composer_footer(agent, account, hint(mode, busy, stopping, empty, can_queue, team), said, asking, cx.reduce_motion());
```

In `modals/new_session.rs`'s `start_session`, after `self.store.agents.launch(&mut spec, info.plan, &info.efforts);`:

```rust
        composer::ask_team(&mut spec, self.new_form.composer.draft.team, self.agents.teams_offered());
```

In `crates/chat/src/outgoing.rs`, `Pending` gains after `stopped: bool,`:

```rust
    /// It asked for a team plan; a retry asks again.
    team: bool,
```

Both `Pending { … }` literals (in `push` and `steer_text`) end `stopped: false, team: false }`. After `sent_as`, add:

```rust
    /// Marks send `id` as asking for a team plan.
    pub fn asks_team(&mut self, id: u64) {
        if let Some(send) = self.sends.iter_mut().find(|s| s.id == id) {
            send.team = true;
        }
    }
```

Replace `withdraw`, `retry` and `take` with:

```rust
    /// Drops a queued send unsent, handing back its text.
    pub fn withdraw(&mut self, id: u64) -> Option<String> {
        self.take(id, Delivery::Queued).map(|s| s.text)
    }

    /// Takes a failed send back for resending: its text, and whether it asked for a team plan.
    pub fn retry(&mut self, id: u64) -> Option<(String, bool)> {
        self.take(id, Delivery::Failed).map(|s| (s.text, s.team))
    }

    fn take(&mut self, id: u64, delivery: Delivery) -> Option<Pending> {
        let ix = self.sends.iter().position(|s| s.id == id && s.delivery == delivery)?;
        Some(self.sends.remove(ix))
    }
```

In `pocket/src/chat/transcript.rs`, replace `chat_retry` with:

```rust
    fn chat_retry(&mut self, agent: &str, send: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some((text, team)) = self.chats.views.get_mut(agent).and_then(|v| v.session.outgoing.retry(send)) else { return };
        self.chat_take_back(agent, &text, window, cx);
        self.chats.composer.draft.team |= team;
    }
```

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p chat outgoing && cargo test -p pocket chat::composer::tests && cargo test -p pocket composer::tests && cargo clippy -p pocket --all-targets`
Expected: PASS, and clippy reports nothing in the files this PR touched.

### Task 2.5: Capture steps, the screens after, and the workspace

**Files:**
- Modify: `packages/desktop/crates/pocket/src/capture.rs` (`STEPS`)

**Context:** `STEPS` is a fixed-size array; its length must equal its entries (the compiler names the right count if it's off). The chat fixture's session is Claude's; `chat-access` makes it `chat_only`, so Team is offered there. The new session page offers Team only with **Start as a chat** on, so the compose steps turn it on.

**Step 1: Add the steps**

Change `const STEPS: [(&str, Step); 229] = [` to `const STEPS: [(&str, Step); 233] = [`. After the `("compose-local", …)` entry add:

```rust
    ("compose-add-open", |d, _, cx| {
        d.store.agents.start_as_chat = true;
        d.new_form.composer.draft.toggle(Menu::Add);
        cx.notify();
    }),
    ("compose-team", |d, _, cx| {
        d.store.agents.start_as_chat = true;
        d.new_form.composer.draft.team = true;
        cx.notify();
    }),
```

After the `("chat-effort-open", …)` entry add:

```rust
    ("chat-add-open", |d, _, cx| {
        d.chats.composer.draft.toggle(Menu::Add);
        cx.notify();
    }),
    ("chat-team", |d, _, cx| {
        d.chats.composer.draft.team = true;
        cx.notify();
    }),
```

**Step 2: Capture after and compare**

Run (from the repo root):
`.ui-review/fixture/capture.sh /tmp/team-trigger/after chat=chat,chat-access chat-add=chat,chat-access,chat-add-open chat-team=chat,chat-access,chat-team compose=compose compose-add=compose,compose-add-open compose-team=compose,compose-team`
Expected: six paths printed, `/tmp/team-trigger/after/impl-chat.png` … `/tmp/team-trigger/after/impl-compose-team.png`.

Open each with the Read tool and check:
- `impl-chat.png` and `impl-compose.png` match their `/tmp/team-trigger/before/` twins (the "+" looks as it did).
- `impl-chat-add.png`: a menu opening upward from the "+", right edges aligned, rows **Attach images** and **Team** (no check).
- `impl-compose-add.png`: the same menu hanging down from the "+", right-aligned.
- `impl-chat-team.png`, `impl-compose-team.png`: a purple `Team ×` chip (team icon, "Team", ×) in the bar after the other chips, the bar's height unchanged.

Anything off is a bug in Task 2.3's view code: fix it and capture again.

**Step 3: Run the workspace checks**

Run (from `packages/desktop`): `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: builds, clippy prints no warning in a file this plan touched, every test passes.

---

## Contract changes needed

All folded into the index (`docs/plans/2026-11-05-team-mode.md`): `Outbox::prompt(agent_id, text, team: bool)` with `"team"` only when true; `team.list` asked by `agents::run` after every `hello.ok` that negotiated `team.v1`; `TaskPick` in `agents::team`, re-exported; `tag_event` tagging `lead`, `worker` and `worktree`, and `Outboxes` routing `team_start/team_stop/team_merge` by the lead; team verb ids `team-<n>` (`team.list` as `teams`); `team.v1` in `CAPS` and `Agents::teams_offered`; `agents::TeamAction` and `Event::TeamFailed { lead, action, message }` for a refused team prompt or verb, tagged by lead; `chat::Outgoing::asks_team` and `retry` returning `(text, team)`.

## Open questions

- **Which pocketd's caps.** `Agents.caps` holds the caps of whichever connection's `hello.ok` arrived last (this Mac's or a host's), as for every other `*_offered`. With a host on an older pocketd, Team can be offered for its chats, or hidden for this Mac's, until the caps are kept per host.

Resolved: **No capability for team** (pocketd advertises `team.v1`, plan 3; `teams_offered` gates the menu row, the chip, the send and the create). **Busy agents**: plan 3 sends a team prompt as a plain prompt, so the `Wait` rule stays. **Resolved by the UX decisions:** refused team requests become `Event::TeamFailed` (Task 1.4), which plan 6 shows as a toast; Retry keeps the team flag (Task 2.4).
