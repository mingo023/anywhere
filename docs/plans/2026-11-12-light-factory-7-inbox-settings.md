# Light Factory 7: Inbox, Profiles Editor, Metrics and Grouped List Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** Finish the Light Factory on the desktop and the command line. The Inbox lists every decision a Factory ticket waits on and lets you make it without opening the Board. Settings › Factory gains the Profiles editor with import and preview, and a strip of four numbers for the last 14 days. The List layout groups by station. `anywhere ticket list` and `anywhere ticket read` show where a Factory ticket stands.

**Architecture:** `pocket::inbox::notes` stays a plain function. It gains one pass over the Board that turns a Factory ticket's gates, Asks and failures into rows, and `inbox::count` counts the same rows. The Inbox list becomes a GPUI `list` over rows cached in `InboxState`, and the detail pane of a Factory row draws the Factory ticket page's panel and the footer bar from `factory/bar.rs`. The Profiles editor is `factory/profile_sheet.rs` (an `Overlay::Profile` modal over a plain `Draft`) plus `factory/import.rs` (a plain `read` that runs on the background executor). The metrics are a pure Go function in `internal/factory/metrics.go` over activity logs, sent by a new verb `factory.metrics`, and drawn by `factory/metrics.rs`. The grouped List is a plain `station_rows` in `board/logic.rs` beside the existing `rows`.

**Toolset:**
- Desktop tests, from `packages/desktop`: `cargo test -p pocket inbox::`, `cargo test -p pocket dock`, `cargo test -p pocket factory::import`, `cargo test -p pocket factory::profile_sheet`, `cargo test -p pocket factory::metrics`, `cargo test -p pocket board::logic`, `cargo test -p agents factory`
- Desktop, everything: `cargo build --workspace`, `cargo clippy --workspace --all-targets` (no new warnings), `cargo test --workspace`
- pocketd tests, from `packages/pocketd`: `go test ./internal/factory/ -run Metrics`, `go test ./internal/proto/ -run FactoryMetrics`, `go test ./internal/peer/`, `go test ./cmd/pocketd/ -run Ticket`, `go test ./internal/cliskill/`
- pocketd, everything: `go vet ./...`, `go test ./...` (run `scripts/build-ghostty.sh` once first)
- Scope gate: not needed. Plan 1 already gives `ws:factory.metrics` its scope, and this plan changes no `peer.Needs` row.
- Screens, from the repo root: `.ui-review/fixture/capture.sh <dir> <name>=<step>,<step> …`. The picture is `<dir>/impl-<name>.png`.

**Read first:**
- `docs/plans/2026-11-12-light-factory.md`: the index. Its wire shapes, Go signatures and Rust names are the contract. Sections "Wire", "pocketd" and "Desktop".
- `docs/designs/2026-11-12-light-factory.md`: N1 to N4 (Inbox), PF4, PF5 and PF11 (Profiles), B11 (List), section 5.11 (CLI), section 9 (metrics), section 11 (design rules).
- `docs/designs/2026-11-12-light-factory-stories.md`: US-07, US-30, US-34, US-37, US-39, US-40.
- `.ui-review/prototypes/light-factory/app.js` and `styles.css`: the Inbox (`.inrow`), the metrics strip (`.metrics`), the Profiles list (`.set.link`), the profile sheet (`.dialog.profile`), the grouped List (`.lgroup-h`, `.lrow`).
- `CLAUDE.md` and `docs/adr/0003-desktop-code-layout.md`: the rules for desktop code.
- `packages/desktop/crates/pocket/src/inbox.rs`, `inbox/list.rs`, `inbox/detail.rs`: the Inbox as it is today.
- `packages/desktop/crates/pocket/src/board/logic.rs` (`rows`, `Row`, `Slot`, `stuck_head`) and `board/row.rs`: the List layout.
- `packages/desktop/crates/pocket/src/team/sheet.rs`: the shape of a sheet. `settings/git.rs:92`: a model dropdown.
- `packages/pocketd/cmd/pocketd/anywhere.go` (`ticketListCmd`, `ticketReadCmd`) and `internal/wsserver/board.go`: the CLI and a verb handler.

---

## Decisions

- **One Inbox row per decision.** A Factory ticket at the Plan gate or the PR gate is one row. Each session of that Ticket with an open Ask is one row. A Factory ticket that failed is one row. A session of a Factory ticket that is done or failed gets no row of its own, because the Ticket's row covers it.
- **Gate rows come from the Ticket, not from a session.** `Ticket.gate` says the Ticket waits. The row shows even when no session reads `needsYou`, which is the case at the PR gate.
- **The selection follows a row's key.** A plain row's key is its terminal. A Factory row's key names the Ticket and the decision. When the selected row leaves, the row that took its place is selected. This one rule gives "after a decision the next row is selected" for `⌘↵` and for a click on the bar.
- **A Factory row never focuses a terminal.** Its detail pane is the Ticket's panel, so no keystroke may reach a session.
- **The Inbox list is virtualized.** A Ticket that waits on you holds no slot, so the number of rows has no limit. The rows are cached in `InboxState` and rebuilt when sessions or the Board change, never in `render`.
- **The Status layout does not change.** Its columns are statuses. A board whose columns are stations is the Factory layout. The prototype's Status view has no station groups, and the PRD's non-goal N3 says the Status layout stays.
- **The List groups by station when the Board holds a Ticket with a station.** Otherwise it groups by status as today. A row is the same 36px row, so every prop, menu and selection keeps working.
- **The profile sheet offers Claude and Codex.** They are the providers of the built-in profiles.
- **Skills on this Mac are read by the desktop.** It reads `~/.claude/skills/<name>` and `~/.agents/skills/<name>`. Skills on a remote host are left out.
- **Import adds the `zip` crate.** The workspace has no zip reader. `flate2` with `zlib-rs` is already in `Cargo.lock`, so the only new crate is `typed-path`.
- **The metrics need one new activity event, `gate`.** No existing event says when a Ticket started to wait at a gate. See "Contract changes needed".
- **`anywhere` gets no verb that decides.** It cannot approve a plan, answer an Ask, merge or send a hint.
- **No hand edit of `CHANGELOG.md`.** `scripts/changelog-draft.sh` writes one line per merged PR from its title at release. Each PR below names its title and carries the label `type: feature`.
- **Left out:** the `⌘K` command menu tabs (B10) and the tray for hidden stations (B12). Neither is needed for a story this plan covers.

## Contracts this plan relies on (from plans 1 to 6)

The code below uses these names. They are not in the repository at the time of writing. **The first task of each PR checks them.** If an earlier plan shipped a different name, use that name everywhere in this plan's code. Do not rename an earlier plan's API.

```rust
// crates/agents/src/tickets.rs (plan 1)
pub enum Station { Spec, Plan, Implement, Checks, Pr, Merged, None }   // None is the default
pub enum Gate { Plan, Pr, Ask, None }
pub struct Ticket { /* … */ pub station: Station, pub gate: Gate, pub pr: u32, pub fix: u8, pub hint: bool }

// crates/agents/src/factory.rs (plans 1, 3, 4)
pub struct ProfileInfo { pub id: String, pub name: String, pub provider: String, pub model: String, pub skills: usize, pub built_in: bool }
pub struct Profile { pub id: String, pub name: String, pub provider: String, pub model: String, pub skills: usize, pub built_in: bool, pub soul: String, pub instructions: String, pub files: Vec<ProfileFile> }
pub struct ProfileFile { pub path: String, pub text: String, pub binary: bool }

// crates/agents/src/agents.rs
Event::FactoryUpdate(String), Event::FactoryProfiles(Vec<ProfileInfo>), Event::FactoryProfile(Profile), Event::FactoryError(String)
Agents::profiles(&self) -> &[ProfileInfo]
Outbox::factory_ticket(&self, ticket: &str), profile_get(&self, id: &str), profile_save(&self, p: &Profile), profile_delete(&self, id: &str)

// crates/pocket/src/factory.rs and factory/ (plans 2 and 3)
pub struct FactoryState { /* … */ }                       // the field on Desktop is `factory`
factory::station::{Column, COLUMNS}                       // Column { Todo, Spec, Plan, Implement, Checks, Pr, Merged }, with `name()`
board::parts::station_glyph(column: Column, gate: Gate, failed: bool)
factory::station::{Stations, arrange, column, rank, on_line}   // Stations = [Vec<usize>; 7]
FactoryState.columns: Stations                            // what `arrange` returned at the last regroup
Desktop::regroup_stations(&mut self)                      // fills FactoryState.columns
```

```go
// packages/pocketd/internal/proto (plans 1 and 3)
const CapFactory = "factory.v1"
const StationPlan, StationPR = "plan", "pr"; const GatePlan, GatePR = "plan", "pr"; const MaxFix = 2
type FactoryEvent struct { At int64; Station, Actor, Kind, Text string }

// packages/pocketd/internal/factory (plan 3)
type Engine struct { Home string; Board *board.Store; Now func() time.Time /* … */ }
func ReadActivity(home, ticket string) ([]proto.FactoryEvent, error)
// packages/pocketd/internal/wsserver: Server.Factory *factory.Engine
```

This plan needs three names that the index does not have. They are listed under "Contract changes needed": the three `Desktop` methods that let the Inbox draw the page's panel and bar, the `gate` activity event, and `Agents::metrics`.

## Design to GPUI

| Prototype | GPUI |
|---|---|
| `.inrow` grid 18px, 1fr, auto. Padding 9px 10px, radius 12 | `div().px(px(10.)).py(px(9.)).flex().gap(px(10.)).rounded(px(12.))` with an 18px `agent_avatar` first |
| `.inrow` title 13.5 semibold. Meta 12px: key, type in text-2, detail in mono 11.5 for an Ask | two stacked `div`s, `gap(px(3.))` |
| `.inrow` end: age and a dot | `ago(n.at, now)` and `ui::indicator(…, Some(state(n.status, 0, 0)))` |
| `.empty` "Nothing waiting" 13 medium text-2, one 12px text-4 line | `nothing_waiting()` in `inbox/detail.rs` |
| `.metrics` 4 columns, radius 9, surface, row shadow. Value 15 medium tabular, label 11.5 text-3 | `metrics_strip` in `factory/metrics.rs` |
| `.set.link` min-height 48: avatar, name, meta, "Built-in", chevron | `settings::row(title, hint, control)` inside `settings::group("Profiles", …)` |
| `.dialog.profile` width 560, max-height 90vh | `ui::pop(div().id("profile-sheet")).w(px(560.)).max_h(relative(0.9))` |
| `.lgroup-h` height 32, radius 7, fill-1: chevron, glyph, name, count | `station_bar` in `board/row.rs`, the same shape as `section_bar` |

---

## PR 1: Factory rows in the Inbox

**PR title:** `The Inbox lists every Factory decision and lets you make it there`. Label: `type: feature`.

**Scope:** US-30, and the Inbox part of US-34. `inbox::notes` and `inbox::count` gain Factory rows. The list is virtualized. The detail pane of a Factory row shows the Ticket's panel and the footer bar. `⌘↵` runs the bar's primary action.

**Depends on:** plans 1 to 6 merged.

**Done when:** the `inbox::` and `dock` tests pass, `inbox-factory` shows three Factory rows with the bar under the panel, and an Inbox with no Factory ticket looks as it did before.

### Task 1.0: Check the contracts and take the "before" pictures

**Files:**
- Modify (only where a check fails): `packages/desktop/crates/pocket/src/board.rs:14`, `board/parts.rs:68`, `board/card.rs:34`, `board.rs:292`, `factory/page.rs`, `factory/bar.rs`

**Context:** Plan 2 makes the board parts that the Inbox needs `pub(crate)` (index, section "Desktop"). Plan 3 adds the Factory ticket page and its bar. This plan draws both inside the Inbox, so it needs three entry points on `Desktop`.

**Step 1: Take the pictures**

From the repo root:

```sh
.ui-review/fixture/capture.sh /tmp/lf7-before inbox=inbox board-list=board,board-list settings-factory=settings,settings-factory
```

Expected: three files `impl-inbox.png`, `impl-board-list.png` and `impl-settings-factory.png` in `/tmp/lf7-before`.

**Step 2: Check the names**

From `packages/desktop`:

```sh
grep -n "pub enum Station\|pub enum Gate" crates/agents/src/tickets.rs
grep -n "pub(crate) mod parts\|pub(crate) fn agent_avatar\|pub(crate) fn station_glyph" crates/pocket/src/board.rs crates/pocket/src/board/parts.rs
grep -n "fn ticket_key\|fn open_ticket" crates/pocket/src/board/card.rs crates/pocket/src/board.rs
grep -n "fn factory_panel\|fn factory_bar\|fn factory_primary" -r crates/pocket/src/factory
grep -n "fn factory_ticket\|FactoryUpdate" crates/agents/src/agents.rs
```

Expected: every line of the list below has a match.

**Step 3: Fix only what is missing**

- `board.rs:14` reads `mod parts;`. Make it `pub(crate) mod parts;`.
- `board/parts.rs:68` reads `pub(super) fn agent_avatar`. Make it `pub(crate) fn agent_avatar`.
- `board/card.rs:34` reads `pub(super) fn ticket_key(&self, t: &Ticket)`. Make it `pub(crate)`.
- `board.rs:292` reads `fn open_ticket(&mut self, id: &str, …)`. Make it `pub(crate)`. Check that it opens the Factory ticket page for a Ticket with a station. Plan 3 owns that behaviour.
- If `factory_panel`, `factory_bar` and `factory_primary` are missing, add them to `factory/page.rs` and `factory/bar.rs` by moving code, with no change in behaviour. These are the shapes this plan calls:

```rust
impl Desktop {
    /// The Factory ticket page without its top bar, its Properties panel and its footer: title, status line, tabs and the tab's body.
    pub(crate) fn factory_panel(&mut self, ticket: &str, window: &mut Window, cx: &mut Context<Self>) -> Div;

    /// The footer decision bar for `ticket`. `ask` picks the Ask to answer when several sessions of the Ticket ask.
    /// None when `bar::decision` says there is nothing to decide.
    pub(crate) fn factory_bar(&mut self, ticket: &str, ask: Option<&agents::Permission>, window: &mut Window, cx: &mut Context<Self>) -> Option<Div>;

    /// Runs the bar's primary action: approve the plan, allow the Ask, merge, or start the fix. False when the bar has none.
    pub(crate) fn factory_primary(&mut self, ticket: &str, ask: Option<&agents::Permission>, window: &mut Window, cx: &mut Context<Self>) -> bool;
}
```

The page's own render must call the same three methods, so that the page and the Inbox cannot drift apart.

**Step 4: Run the checks**

Run from `packages/desktop`: `cargo build --workspace && cargo test -p pocket factory::`
Expected: PASS. No test changes, because nothing changed behaviour.

### Task 1.1: `notes` and `count` gain Factory rows

**Files:**
- Modify: `packages/desktop/crates/pocket/src/inbox.rs` (`Note` at line 16, `notes` at line 34, `count` at line 79, `readable` at line 98)
- Modify: `packages/desktop/crates/pocket/src/desktop/dock.rs:7-27`, `desktop/alerts.rs:270`, `settings/notifications.rs:19`
- Test: the `tests` module at the bottom of `inbox.rs` and of `desktop/dock.rs`

**Context:** Plan 1 adds `station` and `gate` to `agents::tickets::Ticket`. A Ticket is in the Factory when `station` is not `Station::None`. Its lead is `Ticket.agent_id`. A worker and the reviewer have `Summary.parent` set to the lead (index, section "Decisions"). `Agents.pending` holds the open Asks. The bell and the Dock badge call `inbox::count` on every frame, so the pass must not build strings.

**Step 1: Write the failing tests**

In `inbox.rs`, change the `use` lines of the `tests` module to:

```rust
    use super::{FactoryRow, Kind, Note, arrange, count, first_line, heading, notes, readable, reselect, select, step};
    use crate::status::Status;
    use agents::tickets::{Station, Ticket};
    use agents::{Agents, Item, Permission, Summary};
    use serde_json::json;
    use store::prefs::sidebar::{InboxFilter, InboxSort};
```

`Note` gains a field in this task, so add `factory: None` to the two `Note { … }` literals in the existing tests (the closure in `selection_follows_the_focused_note_else_clamps_and_drops_focus` and the helper `note`).

Add these tests:

```rust
    /// A Factory ticket led by `lead`. `more` overrides fields, in the wire's names.
    fn ticket(id: &str, lead: &str, more: serde_json::Value) -> Ticket {
        let mut raw = json!({"id": id, "title": format!("Ticket {id}"), "prompt": "", "provider": "claude", "folder": "/code/api", "state": "live", "agentId": lead, "terminalId": format!("t-{lead}"), "station": "plan", "changedAt": 5});
        raw.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
        serde_json::from_value(raw).unwrap()
    }

    fn worker(id: &str, lead: &str, status: &str, at: i64) -> Summary {
        Summary { parent: lead.into(), ..agent(id, status, at) }
    }

    fn row(ticket: &str, kind: Kind, station: Station, request: &str) -> Option<FactoryRow> {
        Some(FactoryRow { ticket: ticket.into(), kind, station, request: request.into() })
    }

    #[test]
    fn a_ticket_at_the_plan_gate_is_one_row_that_carries_the_ticket() {
        let mut agents = Agents::default();
        agents.list = vec![agent("lead", "needsYou", 9)];
        agents.board.tickets = vec![ticket("T1", "lead", json!({"gate": "plan", "status": "needsYou", "changedAt": 4}))];
        let got = notes(&agents);
        assert_eq!(got.len(), 1);
        let n = &got[0];
        assert_eq!((n.agent.as_str(), n.status, n.title.as_str(), n.subtitle.as_str(), n.at), ("lead", Status::NeedsYou, "Ticket T1", "api", 4));
        assert_eq!(n.factory, row("T1", Kind::Plan, Station::Plan, ""));
        assert_eq!(n.factory.as_ref().unwrap().label(), "Plan approval");
    }

    #[test]
    fn a_worker_that_asks_is_a_row_with_the_command() {
        let mut agents = Agents::default();
        agents.list = vec![agent("lead", "working", 1), worker("w1", "lead", "needsYou", 7)];
        let mut ask = Permission { request_id: "r1".into(), agent_id: "w1".into(), tool_name: "Bash".into(), ..Default::default() };
        (ask.detail.kind, ask.detail.command) = ("shell".into(), "npm install".into());
        agents.pending = vec![ask];
        agents.board.tickets = vec![ticket("T1", "lead", json!({"station": "implement", "gate": "ask", "status": "needsYou"}))];
        let got = notes(&agents);
        assert_eq!(got.len(), 1);
        let n = &got[0];
        assert_eq!((n.agent.as_str(), n.terminal.as_str(), n.title.as_str(), n.subtitle.as_str(), n.at), ("w1", "t-w1", "Ticket T1", "npm install", 7));
        assert_eq!(n.factory, row("T1", Kind::Ask, Station::Implement, "r1"));
        assert_eq!(n.factory.as_ref().unwrap().label(), "Ask");
    }

    #[test]
    fn a_ticket_at_the_pr_gate_is_a_row_even_when_no_session_needs_you() {
        let mut agents = Agents::default();
        agents.list = vec![agent("lead", "done", 9)];
        agents.board.tickets = vec![ticket("T1", "lead", json!({"station": "pr", "gate": "pr", "status": "done", "changedAt": 6}))];
        let got = notes(&agents);
        assert_eq!(got.len(), 1, "the lead's done row is left out");
        assert_eq!((got[0].status, got[0].at), (Status::NeedsYou, 6));
        assert_eq!(got[0].factory.as_ref().unwrap().label(), "PR review");
    }

    #[test]
    fn a_factory_ticket_that_failed_is_one_failed_row() {
        let mut agents = Agents::default();
        agents.list = vec![Summary { failed: true, ..agent("lead", "done", 9) }];
        agents.board.tickets = vec![ticket("T1", "lead", json!({"station": "checks", "status": "failed", "changedAt": 3}))];
        let got = notes(&agents);
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].status, got[0].at), (Status::Failed, 3));
        assert_eq!(got[0].factory, row("T1", Kind::Failed, Station::Checks, ""));
        assert_eq!(got[0].factory.as_ref().unwrap().label(), "Checks failed");
    }

    #[test]
    fn factory_rows_sort_with_the_rest_and_the_bell_counts_one_per_row() {
        let mut agents = Agents::default();
        agents.list = vec![agent("solo", "needsYou", 5), agent("lead", "needsYou", 8), Summary { failed: true, ..agent("lead2", "done", 9) }, agent("old", "done", 1)];
        agents.board.tickets = vec![ticket("T1", "lead", json!({"gate": "plan", "status": "needsYou", "changedAt": 2})), ticket("T2", "lead2", json!({"station": "checks", "status": "failed", "changedAt": 3}))];
        let got: Vec<(String, Status)> = notes(&agents).into_iter().map(|n| (n.factory.map_or(n.agent, |f| f.ticket), n.status)).collect();
        let want = [("T1", Status::NeedsYou), ("solo", Status::NeedsYou), ("T2", Status::Failed), ("old", Status::Done)];
        assert_eq!(got, want.map(|(id, s)| (id.to_string(), s)));
        assert_eq!(count(&agents), 4);
    }

    #[test]
    fn mark_all_seen_leaves_factory_rows() {
        let failed = Note { factory: row("T1", Kind::Failed, Station::Checks, ""), ..note("a", Status::Failed) };
        assert_eq!(readable(vec![failed, note("b", Status::Done)]), vec!["B"]);
    }
```

In `desktop/dock.rs`, replace the `tests` module's two tests. The badge now takes `&Agents`:

```rust
#[cfg(test)]
mod tests {
    use super::badge_label;
    use agents::{Agents, Summary};
    use store::DockBadge;

    fn agent(id: &str, status: &str) -> Summary {
        Summary { id: id.into(), status: status.into(), attached: true, ..Default::default() }
    }

    fn agents(list: Vec<Summary>) -> Agents {
        Agents { list, ..Default::default() }
    }

    #[test]
    fn the_badge_counts_what_the_bell_counts_and_clears_at_zero() {
        let noted = agents(vec![agent("a", "needsYou"), agent("b", "done"), Summary { failed: true, ..agent("c", "done") }, agent("w", "working"), Summary { attached: false, ..agent("d", "needsYou") }]);
        assert_eq!(badge_label(&noted, DockBadge::Inbox).as_deref(), Some("3"));
        assert_eq!(badge_label(&noted, DockBadge::Inbox).map(|n| n.parse::<usize>().unwrap()), Some(crate::inbox::count(&noted)));
        assert_eq!(badge_label(&agents(vec![agent("a", "working")]), DockBadge::Inbox), None);
    }

    #[test]
    fn the_badge_can_count_only_sessions_that_need_you_or_nothing() {
        let list = agents(vec![agent("a", "needsYou"), agent("b", "done"), agent("c", "needsYou")]);
        assert_eq!(badge_label(&list, DockBadge::NeedsYou).as_deref(), Some("2"));
        assert_eq!(badge_label(&list, DockBadge::Off), None);
    }
}
```

**Step 2: Run the tests to see them fail**

Run from `packages/desktop`: `cargo test -p pocket inbox::`
Expected: FAIL to compile with `cannot find type FactoryRow in module super` and `struct Note has no field named factory`.

**Step 3: Implement**

In `inbox.rs`, replace the `use` block's last four lines and the `Note` struct (lines 7 to 23):

```rust
use crate::status::Status;
use crate::util::basename;
use agents::tickets::{self, Gate, Station, Ticket};
use agents::{Agents, Permission, Summary};
use gpui_kit::*;
use std::cmp::Reverse;
use std::collections::HashMap;
use store::prefs::sidebar::{InboxFilter, InboxSort};

pub(crate) const CONTEXT: &str = "Inbox";

/// The decision a Factory row asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Plan,
    Ask,
    Pr,
    Failed,
}

/// What makes a row a Factory row: its Ticket and the decision.
#[derive(Clone, Debug, PartialEq)]
pub struct FactoryRow {
    pub ticket: String,
    pub kind: Kind,
    pub station: Station,
    /// The Ask's request id, empty when the row is no Ask or the agent asks in its chat.
    pub request: String,
}

impl FactoryRow {
    pub fn label(&self) -> String {
        let station = match self.station {
            Station::Spec => "Spec",
            Station::Plan => "Plan",
            Station::Implement => "Implement",
            Station::Checks => "Checks",
            Station::Pr => "PR review",
            Station::Merged | Station::None => "Ticket",
        };
        match self.kind {
            Kind::Plan => "Plan approval".into(),
            Kind::Ask => "Ask".into(),
            Kind::Pr => "PR review".into(),
            Kind::Failed => format!("{station} failed"),
        }
    }
}

#[derive(Clone)]
pub struct Note {
    pub agent: String,
    pub terminal: String,
    pub status: Status,
    pub title: String,
    pub subtitle: String,
    pub at: i64,
    /// Set when the row is a Factory ticket's decision, not a session's.
    pub factory: Option<FactoryRow>,
}
```

Replace `notes` (lines 33 to 50) with:

```rust
/// One row before its text is built, so the bell can count rows without building any.
enum Found<'a> {
    Session(&'a Summary, Status),
    Ask(&'a Summary, &'a Ticket, Option<&'a Permission>),
    Gate(&'a Ticket, Kind),
}

fn in_factory(t: &Ticket) -> bool {
    t.station != Station::None && t.state == tickets::State::Live && !t.agent_id.is_empty()
}

/// Sessions outside the Factory that alert, then one row per decision a Factory ticket waits on.
/// A session of a Factory ticket only gets a row for its Ask: the Ticket's own row covers the rest.
fn found(agents: &Agents) -> Vec<Found<'_>> {
    let leads: HashMap<&str, &Ticket> = agents.board.tickets.iter().filter(|t| in_factory(t)).map(|t| (t.agent_id.as_str(), t)).collect();
    let mut out = Vec::new();
    for a in &agents.list {
        let Some(status) = noted(a) else { continue };
        let Some(t) = leads.get(a.id.as_str()).or_else(|| leads.get(a.parent.as_str())).copied() else {
            out.push(Found::Session(a, status));
            continue;
        };
        if status != Status::NeedsYou {
            continue;
        }
        let ask = agents.pending.iter().find(|p| p.agent_id == a.id);
        if ask.is_some() || !matches!(t.gate, Gate::Plan | Gate::Pr) {
            out.push(Found::Ask(a, t, ask));
        }
    }
    for t in agents.board.tickets.iter().filter(|t| in_factory(t)) {
        match (t.status == tickets::Status::Failed, t.gate) {
            (true, _) => out.push(Found::Gate(t, Kind::Failed)),
            (_, Gate::Plan) => out.push(Found::Gate(t, Kind::Plan)),
            (_, Gate::Pr) => out.push(Found::Gate(t, Kind::Pr)),
            _ => {}
        }
    }
    out
}

/// What needs you, then what failed, then the done sessions nobody has seen yet, each oldest first.
pub fn notes(agents: &Agents) -> Vec<Note> {
    let row = |t: &Ticket, kind, request: String| Some(FactoryRow { ticket: t.id.clone(), kind, station: t.station, request });
    let mut out: Vec<Note> = found(agents)
        .into_iter()
        .map(|f| match f {
            Found::Session(a, status) => {
                let (title, subtitle) = match (status, agents.pending.iter().find(|p| p.agent_id == a.id)) {
                    (Status::NeedsYou, Some(p)) => (p.ask(), a.title.clone()),
                    (Status::Failed, _) => (format!("{} failed", a.title), first_line(agents.last_result(&a.id).map_or("", |r| r.error.as_str()))),
                    _ => (a.title.clone(), first_line(agents.last_text(&a.id).unwrap_or_default())),
                };
                Note { agent: a.id.clone(), terminal: a.terminal_id.clone(), status, title, subtitle, at: a.updated_at, factory: None }
            }
            Found::Ask(a, t, ask) => Note {
                agent: a.id.clone(),
                terminal: a.terminal_id.clone(),
                status: Status::NeedsYou,
                title: t.title.clone(),
                subtitle: ask.map_or_else(|| basename(&t.folder), |p| p.parts().1),
                at: a.updated_at,
                factory: row(t, Kind::Ask, ask.map(|p| p.request_id.clone()).unwrap_or_default()),
            },
            Found::Gate(t, kind) => Note {
                agent: t.agent_id.clone(),
                terminal: t.terminal_id.clone(),
                status: if kind == Kind::Failed { Status::Failed } else { Status::NeedsYou },
                title: t.title.clone(),
                subtitle: basename(&t.folder),
                at: t.changed_at,
                factory: row(t, kind, String::new()),
            },
        })
        .collect();
    out.sort_by_key(|n| (n.status, n.at));
    out
}
```

Replace `count` (lines 79 to 81) and `readable` (lines 97 to 100):

```rust
pub fn count(agents: &Agents) -> usize {
    found(agents).len()
}
```

```rust
/// The agents "Mark all seen" marks seen. A Factory row stays until its decision is made.
fn readable(notes: Vec<Note>) -> Vec<String> {
    notes.into_iter().filter(|n| !asks(n) && n.factory.is_none()).map(|n| n.agent).collect()
}
```

In `desktop/dock.rs`, lines 1 to 27 become:

```rust
use crate::inbox;
use crate::status::Status;
use agents::Agents;
use store::DockBadge;

/// The Dock badge: by default the bell's count, so every row in the Inbox; none at zero.
pub fn badge_label(agents: &Agents, counts: DockBadge) -> Option<String> {
    let n = match counts {
        DockBadge::Off => 0,
        DockBadge::NeedsYou => agents.list.iter().filter(|a| inbox::noted(a) == Some(Status::NeedsYou)).count(),
        DockBadge::Inbox => inbox::count(agents),
    };
    (n > 0).then(|| n.to_string())
}

/// The label on the Dock icon, kept so the FFI call runs only on a change.
#[derive(Default)]
pub struct Badge(Option<String>);

impl Badge {
    pub fn show(&mut self, agents: &Agents, counts: DockBadge) {
        let label = badge_label(agents, counts);
```

The two callers pass the whole `Agents`:

```rust
// desktop/alerts.rs:270, before
            self.badge.show(&self.agents.list, self.store.notifications.badge);
// after
            self.badge.show(&self.agents, self.store.notifications.badge);
```

```rust
// settings/notifications.rs:19, before
        d.badge.show(&d.agents.list, d.store.notifications.badge);
// after
        d.badge.show(&d.agents, d.store.notifications.badge);
```

**Step 4: Run the tests to see them pass**

Run: `cargo test -p pocket inbox:: && cargo test -p pocket dock`
Expected: PASS. The older tests `lists_agents_that_need_you_then_failed_then_done` and `the_bell_counts_the_agents_that_need_you_and_the_unseen_failed_and_done` pass unchanged, which shows that a Board with no Factory ticket reads as before.

### Task 1.2: The selection follows a row's key, and the rows are cached

**Files:**
- Modify: `packages/desktop/crates/pocket/src/inbox.rs` (`reselect` at line 85, `select` at line 103, `InboxState` at line 117, `impl Desktop` at line 130)
- Modify: `packages/desktop/crates/pocket/src/desktop/alerts.rs:259-261`
- Test: the `tests` module of `inbox.rs`

**Context:** Today the selection follows `terminal.focused`, and `shown_notes()` rebuilds every row three times per frame. A Factory row has no terminal to follow, and the list is about to be virtualized. So the selection follows a key, and `InboxState` keeps the rows. `InboxState` holds a `FocusHandle`, so it cannot be built in a test. The decisions live in free functions.

**Step 1: Write the failing tests**

In the `tests` module, add `Item, item_of, items, ticket_of` to the `use super::{…}` line. Replace the test `selection_follows_the_focused_note_else_clamps_and_drops_focus` with the first test below, and add the other two:

```rust
    #[test]
    fn the_selection_stays_on_its_row_and_takes_the_next_row_when_that_row_leaves() {
        let rows = |ids: &[&str]| ids.iter().map(|t| note(t, Status::NeedsYou)).collect::<Vec<_>>();
        assert_eq!(reselect(&rows(&["new", "a", "b"]), Some("a"), 0), (1, true));
        assert_eq!(reselect(&rows(&["a", "c"]), Some("b"), 1), (1, false), "b left and c took its place");
        assert_eq!(reselect(&rows(&["a"]), Some("c"), 2), (0, false));
        assert_eq!(reselect(&[], None, 4), (0, false));
    }

    #[test]
    fn a_factory_row_has_its_own_key_and_focuses_no_terminal() {
        let gate = Note { factory: row("T1", Kind::Plan, Station::Plan, ""), ..note("lead", Status::NeedsYou) };
        let ask = Note { factory: row("T1", Kind::Ask, Station::Implement, "r1"), ..note("w1", Status::NeedsYou) };
        let plain = note("a", Status::Done);
        assert_eq!((gate.key(), ask.key(), plain.key()), ("factory:T1:Plan".to_string(), "factory:T1:W1".to_string(), "a".to_string()));
        let notes = [gate, plain];
        assert_eq!((select(&notes, 0), select(&notes, 1)), ((0, None), (1, Some("a".to_string()))));
        assert_eq!((ticket_of(&notes, 0), ticket_of(&notes, 1)), (Some("T1"), None));
    }

    #[test]
    fn the_list_puts_a_heading_before_each_status_when_sorted_by_urgency() {
        let notes = [note("a", Status::NeedsYou), note("b", Status::NeedsYou), note("c", Status::Failed)];
        let urgent = items(&notes, true);
        assert_eq!(urgent, vec![Item::Head("NEEDS YOU".into(), 2), Item::Note(0), Item::Note(1), Item::Head("FAILED".into(), 1), Item::Note(2)]);
        assert_eq!(items(&notes, false), vec![Item::Note(0), Item::Note(1), Item::Note(2)]);
        assert_eq!((item_of(&urgent, 2), item_of(&urgent, 9)), (Some(4), None));
    }
```

**Step 2: Run the tests to see them fail**

Run: `cargo test -p pocket inbox::`
Expected: FAIL to compile with `cannot find function items` and `no method named key found for struct Note`.

**Step 3: Implement**

In `inbox.rs`, add after the `Note` struct:

```rust
impl Note {
    /// What the selection follows: the terminal for a session's row, the Ticket and the decision for a Factory row.
    pub fn key(&self) -> String {
        match &self.factory {
            None => self.terminal.clone(),
            Some(f) if f.kind == Kind::Ask => format!("factory:{}:{}", f.ticket, self.agent),
            Some(f) => format!("factory:{}:{:?}", f.ticket, f.kind),
        }
    }
}
```

Replace `reselect` (lines 83 to 90) and `select` (lines 102 to 106):

```rust
/// Keeps the selection on the row with `key` as the list changes, and says whether that row is still there.
/// If it left, the row that took its place is selected.
pub fn reselect(notes: &[Note], key: Option<&str>, i: usize) -> (usize, bool) {
    match notes.iter().position(|n| key == Some(n.key().as_str())) {
        Some(j) => (j, true),
        None => (i.min(notes.len().saturating_sub(1)), false),
    }
}
```

```rust
/// Note `i`, clamped to the list, and the terminal to focus for it. A Factory row focuses none.
fn select(notes: &[Note], i: usize) -> (usize, Option<String>) {
    let i = i.min(notes.len().saturating_sub(1));
    (i, notes.get(i).filter(|n| n.factory.is_none()).map(|n| n.terminal.clone()))
}

/// The Ticket of row `i`, when it is a Factory row.
pub(crate) fn ticket_of(notes: &[Note], i: usize) -> Option<&str> {
    notes.get(i)?.factory.as_ref().map(|f| f.ticket.as_str())
}

/// What the virtualized list draws: a heading, or note `i`.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Head(String, usize),
    Note(usize),
}

/// The list's rows. Headings only show with Sort inbox by Most urgent.
pub fn items(notes: &[Note], urgent: bool) -> Vec<Item> {
    let mut out = Vec::with_capacity(notes.len() + 3);
    for i in 0..notes.len() {
        if let Some((label, count)) = heading(notes, i).filter(|_| urgent) {
            out.push(Item::Head(label, count));
        }
        out.push(Item::Note(i));
    }
    out
}

/// Where note `i` is in the list.
pub fn item_of(items: &[Item], i: usize) -> Option<usize> {
    items.iter().position(|it| *it == Item::Note(i))
}
```

Replace `InboxState` and its `impl` (lines 117 to 128):

```rust
pub struct InboxState {
    pub(crate) selected: usize,
    /// The selected row's key.
    pub(crate) key: Option<String>,
    /// The rows shown. Rebuilt when sessions or the Board change, never in render.
    pub(crate) notes: Vec<Note>,
    pub(crate) items: Vec<Item>,
    pub(crate) list: ListState,
    /// The sort `notes` was built with.
    pub(crate) sort: InboxSort,
    pub(crate) focus: FocusHandle,
    pub(crate) filter: InboxFilter,
    pub(crate) filter_open: bool,
}

impl InboxState {
    pub fn new(cx: &mut Context<Desktop>) -> Self {
        Self {
            selected: 0,
            key: None,
            notes: Vec::new(),
            items: Vec::new(),
            list: ListState::new(0, ListAlignment::Top, px(400.)),
            sort: InboxSort::Urgent,
            focus: cx.focus_handle(),
            filter: InboxFilter::All,
            filter_open: false,
        }
    }

    /// Takes the rows to show with row `i` selected. The list keeps its place.
    fn show(&mut self, notes: Vec<Note>, i: usize, sort: InboxSort) {
        let items = items(&notes, sort == InboxSort::Urgent);
        if items != self.items {
            let top = self.list.logical_scroll_top();
            self.list.reset(items.len());
            self.list.scroll_to(top);
            self.items = items;
        }
        (self.selected, self.key, self.sort) = (i, notes.get(i).map(Note::key), sort);
        self.notes = notes;
    }
}
```

In `impl Desktop`, replace `select_note` and `shown_notes` (lines 138 to 146) and `open_selected` (lines 159 to 166):

```rust
    fn arranged(&self) -> Vec<Note> {
        arrange(notes(&self.agents), self.store.sidebar.inbox_sort, self.inbox.filter)
    }

    pub(crate) fn select_note(&mut self, i: usize, cx: &mut Context<Self>) {
        let notes = self.arranged();
        let (i, terminal) = select(&notes, i);
        self.terminal.focused = terminal;
        self.inbox.show(notes, i, self.store.sidebar.inbox_sort);
        if let Some(at) = item_of(&self.inbox.items, i) {
            self.inbox.list.scroll_to_reveal_item(at);
        }
        cx.notify();
    }

    /// The Inbox follows the sessions and the Board. If the selected row left, the focus drops
    /// so keystrokes never reach a terminal the user didn't pick.
    pub(crate) fn inbox_changed(&mut self) {
        let notes = self.arranged();
        let (i, kept) = reselect(&notes, self.inbox.key.as_deref(), self.inbox.selected);
        if !kept {
            self.terminal.focused = None;
        }
        self.inbox.show(notes, i, self.store.sidebar.inbox_sort);
    }
```

```rust
    pub(crate) fn open_selected(&mut self, _: &OpenSession, window: &mut Window, cx: &mut Context<Self>) {
        if self.screen != Screen::Inbox {
            return;
        }
        if let Some(n) = self.inbox.notes.get(self.inbox.selected).cloned() {
            self.focus_agent(&n.agent, window, cx);
        }
    }
```

`open_inbox` (line 131) stays as it is: it calls `select_note(0, cx)`, which now fills the cache.

In `desktop/alerts.rs`:

```rust
// lines 259-261, before
        if self.screen == Screen::Inbox {
            (self.inbox.selected, self.terminal.focused) = inbox::reselect(&self.shown_notes(), self.terminal.focused.as_deref(), self.inbox.selected);
        }
// after
        if self.screen == Screen::Inbox {
            self.inbox_changed();
        }
```

Remove the `use` of `inbox` in `alerts.rs` if nothing else there uses it.

The two view files still call `shown_notes()`. Point them at the cache now, so this task compiles. Task 1.3 rewrites the list.

```rust
// inbox/list.rs:19, before
        let notes = self.shown_notes();
// after
        if self.inbox.sort != self.store.sidebar.inbox_sort {
            self.inbox_changed();
        }
        let notes = self.inbox.notes.clone();
```

```rust
// inbox/detail.rs:13, before
        let Some(n) = self.shown_notes().into_iter().nth(self.inbox.selected) else {
// after
        let Some(n) = self.inbox.notes.get(self.inbox.selected).cloned() else {
```

The check on `sort` covers the one change that arrives with no agent event: you change Sort inbox by in Settings and come back.

**Step 4: Run the tests to see them pass**

Run: `cargo test -p pocket inbox::`
Expected: PASS.

### Task 1.3: The virtualized list and the Factory row

**Files:**
- Modify: `packages/desktop/crates/pocket/src/inbox/list.rs` (`inbox_list` at lines 18 to 75, and a new `factory_note_row`)

**Context:** The list builds every row on every frame today (`list.rs:65`). `CLAUDE.md` says a list that grows with the repo is virtualized. Rows have two heights (a heading, a row), so this is `list` with a `ListState`, as `board/row.rs:119` does it. There is no render test. The logic that decides what the list holds is `items`, tested in Task 1.2. The picture in Task 1.5 checks the view.

**Step 1: Rewrite `inbox_list`**

Change the first line of `list.rs` and add two imports:

```rust
use super::{FILTERS, FactoryRow, Item, Kind, Note, asks, readable, step};
use crate::board::parts::agent_avatar;
```

Replace lines 19 to 23 (the lines Task 1.2 touched and the `headings` and `now` lines) with:

```rust
        if self.inbox.sort != self.store.sidebar.inbox_sort {
            self.inbox_changed();
        }
        let urgent = self.store.sidebar.inbox_sort == InboxSort::Urgent;
```

In the header, add the hint between the title and the filter:

```rust
// list.rs:32-33, before
            .child(div().flex_1().text_size(px(17.)).font_weight(FontWeight::BOLD).child("Inbox"))
            .child(self.inbox_filter(cx))
// after
            .child(div().flex_none().text_size(px(17.)).font_weight(FontWeight::BOLD).child("Inbox"))
            .child(div().flex_1().min_w_0().pl(px(6.)).truncate().text_size(px(12.)).text_color(TEXT_4).children(urgent.then_some("Needs you first, then failed")))
            .child(self.inbox_filter(cx))
```

Replace lines 43 to 74 (the `section` closure, the `list` builder, the loop, the empty state and the return) with:

```rust
        let body = if self.inbox.items.is_empty() {
            empty("Nothing waiting").into_any_element()
        } else {
            let now = now_ms();
            list(self.inbox.list.clone(), cx.processor(move |this, ix, _, cx| match this.inbox.items.get(ix).cloned() {
                Some(Item::Head(label, count)) => section(&label, count).into_any_element(),
                Some(Item::Note(i)) => match this.inbox.notes.get(i).cloned() {
                    Some(n) => div().w_full().px(px(8.)).pb(px(2.)).child(this.note_row(i, n, now, cx)).into_any_element(),
                    None => div().into_any_element(),
                },
                None => div().into_any_element(),
            }))
            .size_full()
            .into_any_element()
        };
        let notes = div().id("notes").flex_1().min_h_0().pt(px(8.)).track_focus(&self.inbox.focus).on_key_down(cx.listener(Self::on_inbox_key)).child(body);
        column().child(header).child(notes)
    }
```

Add the heading as a free function at the bottom of the file:

```rust
fn section(label: &str, count: usize) -> Div {
    div()
        .w_full()
        .pt(px(12.))
        .pb(px(4.))
        .px(px(20.))
        .flex()
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(TEXT_3)
        .child(div().flex_1().child(label.to_string()))
        .child(div().font_family(MONO).font_weight(FontWeight::MEDIUM).child(count.to_string()))
}
```

The heading's padding grows from 12 to 20 because the list's own 8px padding moved into each row.

**Step 2: Draw a Factory row**

At the top of `note_row` (line 107), hand a Factory row to its own view:

```rust
    fn note_row(&self, i: usize, n: Note, now: i64, cx: &mut Context<Self>) -> Stateful<Div> {
        if let Some(f) = n.factory.clone() {
            return self.factory_note_row(i, &n, &f, now, cx);
        }
        let selected = i == self.inbox.selected;
```

Add after `note_row`:

```rust
    /// A Factory ticket's decision: the agent, the Ticket's title, the decision, and what it is about.
    fn factory_note_row(&self, i: usize, n: &Note, f: &FactoryRow, now: i64, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = i == self.inbox.selected;
        let ticket = self.agents.board.get(&f.ticket);
        let key = ticket.and_then(|t| self.ticket_key(t));
        let provider = self.agents.get(&n.agent).map(|a| a.provider.clone()).or_else(|| ticket.map(|t| t.provider.clone())).unwrap_or_default();
        let detail = div().flex_1().min_w_0().truncate().when(f.kind == Kind::Ask && !f.request.is_empty(), |d| d.font_family(MONO).text_size(px(11.5))).child(n.subtitle.clone());
        div()
            .id(("note", i))
            .px(px(10.))
            .py(px(9.))
            .flex()
            .flex_none()
            .gap(px(10.))
            .rounded(px(12.))
            .cursor_pointer()
            .when(selected, |d| d.bg(FILL_3))
            .when(!selected, |d| d.hover(|s| s.bg(FILL_1)))
            .child(agent_avatar(("note-agent", i), &provider, self.agent_name(&provider)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(div().truncate().text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).child(n.title.clone()))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(12.))
                            .text_color(TEXT_3)
                            .children(key.map(|k| div().flex_none().child(k)))
                            .child(div().flex_none().text_color(TEXT_2).child(f.label()))
                            .child(detail),
                    ),
            )
            .child(div().flex_none().flex().items_center().gap(px(6.)).text_size(px(12.)).text_color(TEXT_4).child(ago(n.at, now)).children(ui::indicator(("note-mark", i), Some(state(n.status, 0, 0)))))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.select_note(i, cx);
                window.focus(&this.inbox.focus, cx);
            }))
    }
```

A click selects the row and keeps the focus on the list, so `J`, `K` and `⌘↵` keep working.

**Step 3: Build**

Run: `cargo build -p pocket && cargo test -p pocket inbox::`
Expected: PASS, with no warning about an unused `heading` import in `list.rs` (the first line no longer imports it).

### Task 1.4: The detail pane, `⌘↵`, and the empty state

**Files:**
- Create: `packages/desktop/crates/pocket/src/inbox/ticket.rs`
- Modify: `packages/desktop/crates/pocket/src/inbox.rs` (`mod` lines, `select_note`, `inbox_changed`, `open_selected`)
- Modify: `packages/desktop/crates/pocket/src/inbox/detail.rs:13-15`
- Modify: `packages/desktop/crates/pocket/src/desktop/alerts.rs:126` (top of `on_agents_from`)
- Test: the `tests` module of `inbox.rs`

**Context:** Plan 3 adds `Outbox::factory_ticket(ticket)`, which asks pocketd for a Ticket's `Detail`, and `Event::FactoryUpdate(ticket)`, which says a Ticket's detail changed (index, section "Wire"). The Factory ticket page asks again on that event for the Ticket it shows. The Inbox must do the same for the Ticket it shows. Task 1.0 made `factory_panel`, `factory_bar` and `factory_primary` available.

**Step 1: Write the failing test**

Add `ask_of` to the `use super::{…}` line of the `tests` module, and add:

```rust
    #[test]
    fn a_factory_ask_row_finds_its_ask_by_request_id() {
        let asks = [Permission { request_id: "r1".into(), agent_id: "w1".into(), ..Default::default() }, Permission { request_id: "r2".into(), agent_id: "w2".into(), ..Default::default() }];
        let f = |request: &str| row("T1", Kind::Ask, Station::Implement, request).unwrap();
        assert_eq!(ask_of(&asks, &f("r2")).map(|p| p.agent_id.as_str()), Some("w2"));
        assert_eq!(ask_of(&asks, &f("")), None);
        assert_eq!(ask_of(&asks, &f("gone")), None);
    }
```

**Step 2: Run the test to see it fail**

Run: `cargo test -p pocket inbox::`
Expected: FAIL to compile with `cannot find function ask_of`.

**Step 3: Implement**

In `inbox.rs`, add `mod ticket;` after `mod list;`, and add the free function after `ticket_of`:

```rust
/// The Ask a Factory row answers, while it is still open.
pub(crate) fn ask_of<'a>(pending: &'a [Permission], f: &FactoryRow) -> Option<&'a Permission> {
    pending.iter().find(|p| !f.request.is_empty() && p.request_id == f.request)
}
```

Ask pocketd for the Ticket's detail when a Factory row gets selected. In `select_note`, before `cx.notify()`:

```rust
        self.load_inbox_ticket();
```

In `inbox_changed`, load it only when the selection moved to another row:

```rust
// before
        self.inbox.show(notes, i, self.store.sidebar.inbox_sort);
    }
// after
        self.inbox.show(notes, i, self.store.sidebar.inbox_sort);
        if !kept {
            self.load_inbox_ticket();
        }
    }

    fn load_inbox_ticket(&self) {
        if let Some(ticket) = ticket_of(&self.inbox.notes, self.inbox.selected) {
            self.outbox.local.factory_ticket(ticket);
        }
    }

    /// The selected Factory row's detail is stale: ask for it again.
    pub(crate) fn inbox_ticket_updated(&self, ticket: &str) {
        if self.screen == Screen::Inbox && ticket_of(&self.inbox.notes, self.inbox.selected) == Some(ticket) {
            self.outbox.local.factory_ticket(ticket);
        }
    }
```

Replace `open_selected`:

```rust
    /// `⌘↵`: decides a Factory row with its bar's primary action, opens the session of any other row.
    pub(crate) fn open_selected(&mut self, _: &OpenSession, window: &mut Window, cx: &mut Context<Self>) {
        if self.screen != Screen::Inbox {
            return;
        }
        let Some(n) = self.inbox.notes.get(self.inbox.selected).cloned() else { return };
        match n.factory {
            Some(f) => {
                let ask = ask_of(&self.agents.pending, &f).cloned();
                self.factory_primary(&f.ticket, ask.as_ref(), window, cx);
            }
            None => self.focus_agent(&n.agent, window, cx),
        }
    }
```

The row leaves the list when pocketd's next snapshot no longer has the gate, the Ask or the failure. `inbox_changed` then selects the row that took its place. Nothing is removed ahead of pocketd, so a decision that pocketd refuses keeps its row.

In `desktop/alerts.rs`, at the top of `on_agents_from` (after line 129):

```rust
        if let Event::FactoryUpdate(ticket) = &ev {
            self.inbox_ticket_updated(ticket);
        }
```

Create `inbox/ticket.rs`:

```rust
use super::{FactoryRow, ask_of};
use crate::desktop::Desktop;
use crate::desktop::chrome::drag_area;
use gpui_kit::*;
use theme::*;
use ui::{self, Variant, kbd};

impl Desktop {
    /// A Factory row's detail: the Ticket's panel and the same footer bar the Factory ticket page draws.
    pub(super) fn inbox_ticket(&mut self, f: &FactoryRow, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let key = self.agents.board.get(&f.ticket).and_then(|t| self.ticket_key(t));
        let ask = ask_of(&self.agents.pending, f).cloned();
        let id = f.ticket.clone();
        let header = drag_area(div())
            .h(px(52.))
            .flex_none()
            .px(px(20.))
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(12.))
            .text_color(TEXT_2)
            .children(key.map(|k| div().pr(px(6.)).text_size(px(13.)).text_color(TEXT_3).child(k)))
            .child(div().flex_1())
            .child(kbd("J"))
            .child(kbd("K"))
            .child("move ·")
            .child(kbd("⌘ ↵"))
            .child(div().pr(px(8.)).child("decide"))
            .child(ui::button("open-ticket", Variant::Secondary, None, "Open ticket").child(icon("chevron-right", 14., TEXT)).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.open_board(&crate::actions::OpenBoard, window, cx);
                this.open_ticket(&id, window, cx);
            })));
        let panel = self.factory_panel(&f.ticket, window, cx);
        let bar = self.factory_bar(&f.ticket, ask.as_ref(), window, cx);
        div().flex_1().min_h_0().flex().flex_col().child(header).child(div().flex_1().min_h_0().flex().flex_col().child(panel)).children(bar)
    }
}

/// The detail pane with nothing selected.
pub(super) fn nothing_waiting() -> Div {
    drag_area(div())
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(4.))
        .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(TEXT_2).child("Nothing waiting"))
        .child(div().text_size(px(12.)).text_color(TEXT_4).child("Plans, Asks and pull requests that need you show up here."))
}
```

In `inbox/detail.rs`, lines 13 to 15 become:

```rust
        let Some(n) = self.inbox.notes.get(self.inbox.selected).cloned() else {
            return super::ticket::nothing_waiting();
        };
        if let Some(f) = &n.factory {
            return self.inbox_ticket(f, window, cx);
        }
```

**Step 4: Run the tests to see them pass**

Run: `cargo test -p pocket inbox:: && cargo test -p pocket actions`
Expected: PASS. The `actions` tests at `actions.rs:243-244` show that `⌘↵` inside a text field of the bar still submits the field, and outside it reaches `OpenSession`.

### Task 1.5: Capture `inbox-factory` and run the checks

**Files:**
- Modify: `packages/desktop/crates/pocket/src/capture.rs` (`STEPS` at line 31, a new step after `inbox` at line 493)

**Context:** Plan 3 adds Factory tickets to the capture fixture and the step `factory-ticket-plan`, which shows how a capture gets a Ticket's `Detail` (index, section "Capture"). This step puts three live Tickets of the fixture at a gate and opens the Inbox.

**Step 1: Add the step**

Bump the length of `STEPS` by one (the number in `const STEPS: [(&str, Step); N]`). Add after the `inbox` step:

```rust
    ("inbox-factory", |d, window, cx| {
        use agents::tickets::{Gate, State, Station, Status};
        let at = [(Station::Plan, Gate::Plan, Status::NeedsYou), (Station::Pr, Gate::Pr, Status::Done), (Station::Checks, Gate::None, Status::Failed)];
        for (t, (station, gate, status)) in d.agents.board.tickets.iter_mut().filter(|t| t.state == State::Live).zip(at) {
            (t.station, t.gate, t.status) = (station, gate, status);
        }
        d.open_inbox(window, cx);
    }),
```

If the fixture's `sessions` scenario has fewer than three live Tickets, add Tickets to `.ui-review/fixture/fixture.ts` the way plan 3 added its Factory tickets. Supply the first Ticket's `Detail` the way `factory-ticket-plan` does.

**Step 2: Take the pictures**

From the repo root:

```sh
.ui-review/fixture/capture.sh /tmp/lf7-after inbox=inbox inbox-factory=inbox-factory
```

Expected:
- `impl-inbox.png` matches `/tmp/lf7-before/impl-inbox.png`, except the muted hint `Needs you first, then failed` beside the title.
- `impl-inbox-factory.png` shows a `NEEDS YOU` heading with two Factory rows (`Plan approval`, `PR review`) and a `FAILED` heading with one (`Checks failed`). Each row has the agent's avatar, the Ticket's title, the key, the type, the project, the age and the status mark. The first row is selected. The pane shows the Ticket's title, status line and tabs, with the bar `Approve plan` at the bottom.

**Step 3: Run the checks**

From `packages/desktop`: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: PASS with no new warning.

---

## PR 2: The Profiles editor

**PR title:** `Edit agent profiles in Settings, and import a soul, instructions and skills from a folder or zip`. Label: `type: feature`.

**Scope:** PF4, PF5, PF11 and the desktop half of US-40. `factory/import.rs` reads a folder, a `.zip` or a skill on this Mac into a preview. `factory/profile_sheet.rs` is the sheet. Settings › Factory lists the profiles.

**Depends on:** plan 4 merged (the profile store in pocketd, the four `factory.profile.*` verbs, and the desktop's `Profile`, `ProfileInfo`, `Outbox::profile_*` and `Event::FactoryProfiles`). It does not depend on PR 1.

**Done when:** the `factory::import` and `factory::profile_sheet` tests pass, `settings-factory-profiles` shows the list, and `profile-sheet-import` shows a preview with "Files the agent can run".

**What stays inert:** nothing. pocketd already copies a profile's skills into a worker's checkout (plan 4).

### Task 2.0: Check the profile wire on the desktop, and add the `zip` crate

**Files:**
- Modify: `packages/desktop/Cargo.toml` (`[workspace.dependencies]`), `packages/desktop/crates/pocket/Cargo.toml` (`[dependencies]`)
- Modify (only where a check fails): `packages/desktop/crates/agents/src/factory.rs`, `agents/src/agents.rs`, `packages/desktop/crates/hosts/src/session.rs`

**Context:** The index gives plan 4 the desktop's profile wire: `ProfileInfo`, `Profile`, `ProfileFile` in `agents::factory`, `Event::FactoryProfiles`, `Event::FactoryProfile`, `Agents::profiles()` and four `Outbox` methods. Plan 4's pocketd side writes the goldens `testdata/factory/client/profile_{list,get,save,delete}.json` and `testdata/factory/server/{profiles,profile}.json`. Plan 1 adds `factory_event`, `factory_id`, the `factory_golden!` macro and the `FACTORY` id prefix to `agents.rs`. The workspace has no zip reader: `grep -n "^zip" packages/desktop/Cargo.toml` prints nothing.

**Step 1: Check the names**

From `packages/desktop`:

```sh
grep -n "pub struct ProfileInfo\|pub struct Profile \|pub struct ProfileFile" crates/agents/src/factory.rs
grep -n "FactoryProfiles\|FactoryProfile(\|pub fn profiles\|pub fn profile_list\|pub fn profile_get\|pub fn profile_save\|pub fn profile_delete" crates/agents/src/agents.rs
grep -n "FactoryProfiles" crates/hosts/src/session.rs
```

Expected: three structs, both events, `profiles` and the four `Outbox` methods, and one line in `tag_event`. If all are there, go to Step 3. If the Rust `Profile` nests a `ProfileInfo` where this plan's code reads flat fields, keep plan 4's shape and change this plan's field paths.

**Step 2: Add what is missing, test first**

In the `tests` module of `agents/src/agents.rs`, add:

```rust
    #[test]
    fn profile_frames_become_profile_events_and_the_list_is_kept() {
        let ev = |raw: &str| factory_event(&serde_json::from_str::<Frame>(raw).unwrap());
        let list = include_str!("../../../../pocketd/internal/proto/testdata/factory/server/profiles.json");
        let Some(Event::FactoryProfiles(profiles)) = ev(list) else { panic!("no list") };
        assert_eq!(profiles.iter().map(|p| (p.id.as_str(), p.built_in, p.skills)).collect::<Vec<_>>(), [("claude-opus", true, 0), ("go-backend", false, 1)]);
        let one = include_str!("../../../../pocketd/internal/proto/testdata/factory/server/profile.json");
        assert!(matches!(ev(one), Some(Event::FactoryProfile(p)) if p.id == "go-backend" && p.soul.starts_with("You are") && p.files[0].path == "skills/review/SKILL.md" && !p.files[0].binary));
        let mut a = Agents::default();
        a.apply(Event::FactoryProfiles(profiles));
        assert_eq!(a.profiles().len(), 2);
    }

    #[test]
    fn profile_messages_are_the_golden_frames() {
        assert_eq!(sent_but_id(|o| o.profile_list()), without_id(factory_golden!("client/profile_list.json")));
        assert_eq!(sent_but_id(|o| o.profile_get("go-backend")), without_id(factory_golden!("client/profile_get.json")));
        assert_eq!(sent_but_id(|o| o.profile_delete("go-backend")), without_id(factory_golden!("client/profile_delete.json")));
        let file = factory::ProfileFile { path: "skills/review/SKILL.md".into(), text: "# Review\n".into(), binary: false };
        let profile = factory::Profile { name: "Go backend".into(), provider: "claude".into(), model: "sonnet".into(), soul: "You are a careful backend engineer.".into(), instructions: "Write a failing test first.".into(), files: vec![file], ..Default::default() };
        assert_eq!(sent_but_id(|o| o.profile_save(&profile)), without_id(factory_golden!("client/profile_save.json")));
    }
```

Run: `cargo test -p agents profile_`
Expected: FAIL to compile, with `cannot find struct ProfileFile` or `no method named profile_list`.

In `agents/src/factory.rs`, add:

```rust
fn is_false(b: &bool) -> bool {
    !*b
}

/// A profile as a list shows it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct ProfileInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub skills: usize,
    #[serde(skip_serializing_if = "is_false")]
    pub built_in: bool,
}

/// A whole profile: who the agent is, how it works, and its skills' files. An empty `id` is a new one.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub skills: usize,
    #[serde(skip_serializing_if = "is_false")]
    pub built_in: bool,
    pub soul: String,
    pub instructions: String,
    pub files: Vec<ProfileFile>,
}

/// One file under `skills/`, by its path from the profile's folder. `text` is base64 when `binary`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct ProfileFile {
    pub path: String,
    pub text: String,
    #[serde(skip_serializing_if = "is_false")]
    pub binary: bool,
}
```

In `agents/src/agents.rs`:

- `Frame` gains two fields, beside `config` and `problem` that plan 1 added:

```rust
    pub profiles: Option<Vec<factory::ProfileInfo>>,
    pub profile: Option<factory::Profile>,
```

- `Event` gains two variants, beside `FactoryConfig`:

```rust
    /// Every profile, after `factory.profile.list`, a save or a delete.
    FactoryProfiles(Vec<factory::ProfileInfo>),
    /// One whole profile, after `factory.profile.get`.
    FactoryProfile(factory::Profile),
```

- `factory_event` gains two arms, before its `"error"` arm:

```rust
        "factory.profiles" => f.profiles.clone().map(Event::FactoryProfiles),
        "factory.profile" => f.profile.clone().map(Event::FactoryProfile),
```

- `Agents` gains a field and a reader, beside `factory`:

```rust
    /// Every profile, built-in ones first, from `factory.profiles`.
    pub profiles: Vec<factory::ProfileInfo>,
```

```rust
    pub fn profiles(&self) -> &[factory::ProfileInfo] {
        &self.profiles
    }
```

- `apply_from` gains one arm, and `Event::FactoryProfile(_)` joins the arm that does nothing (the one that lists `Event::BoardError(_)`):

```rust
            Event::FactoryProfiles(list) => self.profiles = list,
```

- `impl Outbox` gains four methods, after `factory_save`:

```rust
    /// Asks for every profile; `Event::FactoryProfiles` answers.
    pub fn profile_list(&self) {
        self.send(json!({"type": "factory.profile.list", "id": factory_id()}));
    }

    /// Asks for one whole profile; `Event::FactoryProfile` answers.
    pub fn profile_get(&self, id: &str) {
        self.send(json!({"type": "factory.profile.get", "id": factory_id(), "profile": id}));
    }

    /// Saves a profile whole. An empty `id` makes a new one. `Event::FactoryProfiles` answers, or `Event::FactoryError` says why not.
    pub fn profile_save(&self, profile: &factory::Profile) {
        self.send(json!({"type": "factory.profile.save", "id": factory_id(), "profile": profile}));
    }

    pub fn profile_delete(&self, id: &str) {
        self.send(json!({"type": "factory.profile.delete", "id": factory_id(), "profile": id}));
    }
```

In `hosts/src/session.rs`, `tag_event` drops a host's profile news as it drops its Board. Add `| Event::FactoryProfiles(_) | Event::FactoryProfile(_)` to the group that ends in `=> return false` (the group with `Event::BoardError(_)` at line 161).

Run: `cargo test -p agents profile_ && cargo test -p hosts`
Expected: PASS.

**Step 3: Add the `zip` crate**

`packages/desktop/Cargo.toml`, in `[workspace.dependencies]`, in alphabetical place:

```toml
zip = { version = "7", default-features = false, features = ["deflate-flate2-zlib-rs"] }
```

`packages/desktop/crates/pocket/Cargo.toml`, in `[dependencies]`:

```toml
zip.workspace = true
```

Run: `cargo build -p pocket && git diff --stat Cargo.lock`
Expected: the build passes. `Cargo.lock` gains `zip` and `typed-path` and no other new package, because `flate2`, `crc32fast`, `indexmap` and `memchr` are already there.

### Task 2.1: `import::read` reads a folder into a preview

**Files:**
- Create: `packages/desktop/crates/pocket/src/factory/import.rs`
- Modify: `packages/desktop/crates/pocket/src/factory.rs` (the `mod` lines)
- Test: the `tests` module at the bottom of `import.rs`

**Context:** PF5 names the layout: `SOUL.md`, `INSTRUCTIONS.md` or `AGENTS.md`, and `skills/<name>/…` where each skill has a `SKILL.md`. pocketd refuses a soul plus instructions over 64 KB (`proto.MaxProfileText`) and a profile over 2 MB (`proto.MaxProfileBytes`), counted in bytes before base64 (plan 4, `factory.Profiles`). The desktop checks the same limits first, so the preview can say why. A file that is not UTF-8 travels as base64 with `binary` set. `read` touches the disk, so the sheet calls it on the background executor (Task 2.4). It takes a path and returns plain data, so the tests need no GPUI. The crate has no `tempfile`. Tests make their folders under `std::env::temp_dir()`.

**Step 1: Write the failing tests**

In `factory.rs`, add beside the other `mod` lines:

```rust
pub(crate) mod import;
```

Create `factory/import.rs` with only the tests:

```rust
#[cfg(test)]
mod tests {
    use super::{MAX_PROFILE, MAX_TEXT, read};
    use std::fs;
    use std::path::PathBuf;

    /// A fresh folder under the temp dir that holds `files`.
    fn folder(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("pocket-import-{}-{name}", std::process::id()));
        _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        for (path, text) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        root
    }

    #[test]
    fn a_folder_gives_the_soul_the_instructions_and_each_skills_files() {
        let root = folder("whole", &[("SOUL.md", "You are careful."), ("INSTRUCTIONS.md", "Test first."), ("skills/review/SKILL.md", "# Review"), ("skills/review/notes/style.md", "Short.")]);
        let p = read(&root).unwrap();
        assert_eq!((p.profile.soul.as_str(), p.profile.instructions.as_str(), p.profile.skills, p.left_out), ("You are careful.", "Test first.", 1, 0));
        let files: Vec<_> = p.profile.files.iter().map(|f| (f.path.as_str(), f.text.as_str(), f.binary)).collect();
        assert_eq!(files, [("skills/review/SKILL.md", "# Review", false), ("skills/review/notes/style.md", "Short.", false)]);
        let entries: Vec<_> = p.entries.iter().map(|e| (e.path.as_str(), e.size)).collect();
        assert_eq!(entries, [("SOUL.md", 16), ("INSTRUCTIONS.md", 11), ("skills/review/SKILL.md", 8), ("skills/review/notes/style.md", 6)]);
        assert_eq!(p.size(), 41);
        assert!(p.from.ends_with("-whole"));
    }

    #[test]
    fn agents_md_is_read_as_the_instructions_unless_instructions_md_is_there() {
        let p = read(&folder("agents", &[("AGENTS.md", "From agents.")])).unwrap();
        assert_eq!((p.profile.instructions.as_str(), p.entries[0].path.as_str(), p.left_out), ("From agents.", "AGENTS.md", 0));
        let p = read(&folder("both", &[("AGENTS.md", "From agents."), ("INSTRUCTIONS.md", "Mine.")])).unwrap();
        assert_eq!((p.profile.instructions.as_str(), p.entries.len(), p.left_out), ("Mine.", 1, 1));
    }

    #[test]
    fn files_that_are_not_markdown_are_the_ones_the_agent_can_run() {
        let root = folder("runs", &[("skills/deploy/SKILL.md", "# Deploy"), ("skills/deploy/run.sh", "#!/bin/sh\n"), ("skills/deploy/README.MD", "Read me")]);
        let p = read(&root).unwrap();
        assert_eq!(p.runs().map(|e| e.path.as_str()).collect::<Vec<_>>(), ["skills/deploy/run.sh"]);
    }

    #[test]
    fn a_file_that_is_not_text_is_carried_as_base64() {
        let root = folder("binary", &[("skills/logo/SKILL.md", "# Logo")]);
        fs::write(root.join("skills/logo/logo.png"), [0xff, 0xd8, 0x00]).unwrap();
        let p = read(&root).unwrap();
        let png = p.profile.files.iter().find(|f| f.path.ends_with("logo.png")).unwrap();
        assert_eq!((png.text.as_str(), png.binary), ("/9gA", true));
    }

    #[test]
    fn files_outside_the_layout_are_left_out_and_counted() {
        let root = folder("extra", &[("SOUL.md", "Me."), ("README.md", "Hello"), ("src/main.rs", "fn main() {}"), ("skills/loose.md", "No skill"), (".DS_Store", "x")]);
        let p = read(&root).unwrap();
        assert_eq!((p.entries.len(), p.left_out), (1, 3));
    }

    #[test]
    fn a_soul_and_instructions_over_64_kb_together_are_refused() {
        let half = "x".repeat(MAX_TEXT / 2 + 1);
        let err = read(&folder("text", &[("SOUL.md", half.as_str()), ("INSTRUCTIONS.md", half.as_str())])).unwrap_err();
        assert_eq!(err, "The soul and the instructions are more than 64 KB together");
    }

    #[test]
    fn a_folder_over_2_mb_is_refused() {
        let big = "x".repeat(MAX_PROFILE / 2 + 1);
        let err = read(&folder("big", &[("skills/a/SKILL.md", big.as_str()), ("skills/a/data.txt", big.as_str())])).unwrap_err();
        assert_eq!(err, "These files are more than 2 MB. A profile holds 2 MB at most");
    }

    #[test]
    fn a_skill_without_skill_md_is_refused() {
        let err = read(&folder("noskill", &[("skills/review/notes.md", "x")])).unwrap_err();
        assert_eq!(err, "skills/review has no SKILL.md");
    }

    #[test]
    fn a_folder_with_nothing_to_import_is_refused() {
        let root = folder("empty", &[("README.md", "Hello")]);
        assert!(read(&root).unwrap_err().ends_with("has no SOUL.md, INSTRUCTIONS.md, AGENTS.md or skills folder"));
        assert!(read(&root.join("README.md")).unwrap_err().ends_with("is not a folder or a .zip"));
    }

    #[cfg(unix)]
    #[test]
    fn a_link_inside_the_folder_is_refused() {
        let root = folder("link", &[("skills/a/SKILL.md", "# A")]);
        std::os::unix::fs::symlink("/etc/hosts", root.join("skills/a/hosts")).unwrap();
        assert_eq!(read(&root).unwrap_err(), "hosts is a link. Import needs the file itself");
    }
}
```

**Step 2: Run the tests to see them fail**

Run from `packages/desktop`: `cargo test -p pocket factory::import`
Expected: FAIL to compile with `unresolved imports super::MAX_PROFILE, super::MAX_TEXT, super::read`.

**Step 3: Implement**

Put this above the `tests` module in `factory/import.rs`:

```rust
use agents::factory::{Profile, ProfileFile};
use base64::Engine as _;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// The soul and the instructions together, in bytes. pocketd refuses more.
pub const MAX_TEXT: usize = 64 * 1024;
/// A whole profile, in bytes. pocketd refuses more.
pub const MAX_PROFILE: usize = 2 * 1024 * 1024;

const SOUL: &str = "SOUL.md";
const INSTRUCTIONS: &str = "INSTRUCTIONS.md";
const AGENTS: &str = "AGENTS.md";
const SKILLS: &str = "skills";

/// One file the import takes.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub path: String,
    pub size: usize,
    /// Not markdown, so the agent can run it.
    pub runs: bool,
}

/// What an import would put into a profile. Nothing is saved until the user takes it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Preview {
    /// The folder's or the archive's name.
    pub from: String,
    /// `soul`, `instructions`, `files` and `skills` are filled. The rest is empty.
    pub profile: Profile,
    pub entries: Vec<Entry>,
    /// How many files sit outside the layout. The import leaves them out.
    pub left_out: usize,
}

impl Preview {
    pub fn size(&self) -> usize {
        self.entries.iter().map(|e| e.size).sum()
    }

    /// The files that aren't markdown.
    pub fn runs(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|e| e.runs)
    }
}

/// A file's path in the layout, and its bytes.
type Loaded = (String, Vec<u8>);

/// Reads a folder laid out as `SOUL.md`, `INSTRUCTIONS.md` or `AGENTS.md`, and `skills/<name>/…`.
/// It reads the disk, so call it off the UI thread.
pub fn read(path: &Path) -> Result<Preview, String> {
    let from = name_of(path);
    if path.is_dir() {
        let (files, left_out) = folder(path)?;
        return build(from, files, left_out);
    }
    Err(format!("{from} is not a folder or a .zip"))
}

fn name_of(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn too_big() -> String {
    "These files are more than 2 MB. A profile holds 2 MB at most".into()
}

/// Files macOS and git add, which are no part of a profile.
fn ignored(name: &str) -> bool {
    matches!(name, ".DS_Store" | "__MACOSX" | ".git")
}

/// A folder's entries by name.
fn children(dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("Can't read {}: {e}", name_of(dir)))?;
    let mut out: Vec<(String, PathBuf)> = entries.filter_map(Result::ok).map(|e| (e.file_name().to_string_lossy().into_owned(), e.path())).filter(|(name, _)| !ignored(name)).collect();
    out.sort();
    Ok(out)
}

/// Whether `path` is a folder, and its size. A link is refused, because it can point outside the folder.
fn real(path: &Path) -> Result<(bool, usize), String> {
    let meta = fs::symlink_metadata(path).map_err(|e| format!("Can't read {}: {e}", name_of(path)))?;
    if meta.file_type().is_symlink() {
        return Err(format!("{} is a link. Import needs the file itself", name_of(path)));
    }
    Ok((meta.is_dir(), meta.len() as usize))
}

/// Reads one file unless it takes the import over the limit. The size on disk is checked first, so a huge file is never read.
fn load(path: &Path, size: usize, total: &mut usize) -> Result<Vec<u8>, String> {
    *total += size;
    if *total > MAX_PROFILE {
        return Err(too_big());
    }
    fs::read(path).map_err(|e| format!("Can't read {}: {e}", name_of(path)))
}

/// The layout's files in `root`, and how many of its other entries are left out. Only `skills` is walked.
fn folder(root: &Path) -> Result<(Vec<Loaded>, usize), String> {
    let (mut files, mut left_out, mut total) = (Vec::new(), 0, 0);
    for (name, path) in children(root)? {
        let (dir, size) = real(&path)?;
        match name.as_str() {
            SKILLS if dir => walk(&path, SKILLS, &mut files, &mut total)?,
            SOUL | INSTRUCTIONS | AGENTS if !dir => files.push((name, load(&path, size, &mut total)?)),
            _ => left_out += 1,
        }
    }
    Ok((files, left_out))
}

fn walk(dir: &Path, at: &str, files: &mut Vec<Loaded>, total: &mut usize) -> Result<(), String> {
    for (name, path) in children(dir)? {
        let at = format!("{at}/{name}");
        let (dir, size) = real(&path)?;
        if dir {
            walk(&path, &at, files, total)?;
        } else {
            files.push((at, load(&path, size, total)?));
        }
    }
    Ok(())
}

/// The skill a path under `skills/<name>/` belongs to. A file right under `skills/` belongs to none.
fn skill_of(path: &str) -> Option<&str> {
    let (name, rest) = path.strip_prefix("skills/")?.split_once('/')?;
    (!name.is_empty() && !rest.is_empty()).then_some(name)
}

fn entry(path: &str, size: usize) -> Entry {
    Entry { path: path.into(), size, runs: !path.to_lowercase().ends_with(".md") }
}

fn text(name: &str, bytes: Vec<u8>) -> Result<String, String> {
    String::from_utf8(bytes).map_err(|_| format!("{name} is not text"))
}

fn file(path: String, bytes: Vec<u8>) -> ProfileFile {
    match String::from_utf8(bytes) {
        Ok(text) => ProfileFile { path, text, binary: false },
        Err(e) => ProfileFile { path, text: base64::engine::general_purpose::STANDARD.encode(e.into_bytes()), binary: true },
    }
}

/// Sorts loaded files into a profile, and refuses what pocketd would refuse.
fn build(from: String, files: Vec<Loaded>, mut left_out: usize) -> Result<Preview, String> {
    let (mut soul, mut instructions, mut agents, mut skills) = (None, None, None, Vec::new());
    for (path, bytes) in files {
        match path.as_str() {
            SOUL => soul = Some(bytes),
            INSTRUCTIONS => instructions = Some(bytes),
            AGENTS => agents = Some(bytes),
            _ if skill_of(&path).is_some() => skills.push((path, bytes)),
            _ => left_out += 1,
        }
    }
    if instructions.is_some() && agents.is_some() {
        left_out += 1;
    }
    let instructions = instructions.map(|b| (INSTRUCTIONS, b)).or(agents.map(|b| (AGENTS, b)));
    let (mut profile, mut entries) = (Profile::default(), Vec::new());
    if let Some(bytes) = soul {
        entries.push(entry(SOUL, bytes.len()));
        profile.soul = text(SOUL, bytes)?;
    }
    if let Some((name, bytes)) = instructions {
        entries.push(entry(name, bytes.len()));
        profile.instructions = text(name, bytes)?;
    }
    if profile.soul.len() + profile.instructions.len() > MAX_TEXT {
        return Err("The soul and the instructions are more than 64 KB together".into());
    }
    skills.sort();
    let names: BTreeSet<&str> = skills.iter().filter_map(|(path, _)| skill_of(path)).collect();
    for name in &names {
        if !skills.iter().any(|(path, _)| *path == format!("{SKILLS}/{name}/SKILL.md")) {
            return Err(format!("skills/{name} has no SKILL.md"));
        }
    }
    profile.skills = names.len();
    for (path, bytes) in skills {
        entries.push(entry(&path, bytes.len()));
        profile.files.push(file(path, bytes));
    }
    if entries.is_empty() {
        return Err(format!("{from} has no SOUL.md, INSTRUCTIONS.md, AGENTS.md or skills folder"));
    }
    if entries.iter().map(|e| e.size).sum::<usize>() > MAX_PROFILE {
        return Err(too_big());
    }
    Ok(Preview { from, profile, entries, left_out })
}
```

`folder` walks only `skills`. So a pick of a large folder by mistake reads its top level and nothing more.

**Step 4: Run the tests to see them pass**

Run: `cargo test -p pocket factory::import`
Expected: PASS, 10 tests.

### Task 2.2: `import` reads a `.zip`, and a skill on this Mac

**Files:**
- Modify: `packages/desktop/crates/pocket/src/factory/import.rs`
- Test: the `tests` module at the bottom of `import.rs`

**Context:** A zip made by Finder wraps its files in one folder and adds `__MACOSX/` entries. An entry's name comes from the archive's author, so it can hold `..` or start with `/`. `read` refuses the whole archive when any name could land outside it, and when any entry is a link. It checks every name before it reads one byte. `zip::ZipWriter::start_file` stores a name as given, so a test can write an unsafe one. Skills on this Mac live in `~/.claude/skills/<name>` (Claude) and `~/.agents/skills/<name>` (Codex), each with a `SKILL.md` (`packages/pocketd/internal/skills/claude.go`). An entry of `~/.claude/skills` is often a link to a folder elsewhere, so `read_skill` follows the link of the skill's own folder and still refuses a link inside it. The desktop reads these folders itself. A project on a remote host shows this Mac's skills, not the host's.

**Step 1: Write the failing tests**

In the `tests` module, change the first `use` line to:

```rust
    use super::{MAX_PROFILE, MAX_TEXT, installed, read, read_skill};
```

Add a helper and these tests:

```rust
    /// A zip under the temp dir that holds `files`.
    fn zipped(name: &str, files: &[(&str, &str)]) -> PathBuf {
        use std::io::Write as _;
        let path = std::env::temp_dir().join(format!("pocket-import-{}-{name}.zip", std::process::id()));
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        for (name, text) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    #[test]
    fn a_zip_reads_as_the_folder_it_holds() {
        let files = [("SOUL.md", "You are careful."), ("AGENTS.md", "Test first."), ("skills/review/SKILL.md", "# Review"), ("skills/review/run.sh", "#!/bin/sh\n")];
        let from_folder = read(&folder("same", &files)).unwrap();
        let flat = read(&zipped("flat", &files)).unwrap();
        assert_eq!((&flat.profile, &flat.entries, flat.left_out), (&from_folder.profile, &from_folder.entries, 0));
        let wrapped: Vec<(String, &str)> = files.iter().map(|(path, text)| (format!("go-backend/{path}"), *text)).collect();
        let mut entries: Vec<(&str, &str)> = wrapped.iter().map(|(path, text)| (path.as_str(), *text)).collect();
        entries.push(("__MACOSX/go-backend/._SOUL.md", "x"));
        let from_zip = read(&zipped("same", &entries)).unwrap();
        assert_eq!((&from_zip.profile, &from_zip.entries, from_zip.left_out), (&from_folder.profile, &from_folder.entries, 0));
        assert!(from_zip.from.ends_with("-same.zip"));
    }

    #[test]
    fn an_entry_that_points_outside_the_archive_is_refused() {
        for name in ["../evil.md", "skills/a/../../evil.md", "/etc/evil.md"] {
            let err = read(&zipped("outside", &[("SOUL.md", "Me."), (name, "x")])).unwrap_err();
            assert_eq!(err, format!("{name} points outside the archive"));
        }
    }

    #[test]
    fn a_link_in_an_archive_is_refused() {
        let path = std::env::temp_dir().join(format!("pocket-import-{}-link.zip", std::process::id()));
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("skills/a/SKILL.md", options).unwrap();
        zip.add_symlink("skills/a/hosts", "/etc/hosts", options).unwrap();
        zip.finish().unwrap();
        assert_eq!(read(&path).unwrap_err(), "skills/a/hosts is a link. Import needs the file itself");
    }

    #[test]
    fn an_archive_whose_files_are_over_2_mb_is_refused() {
        let big = "x".repeat(MAX_PROFILE / 2 + 1);
        let err = read(&zipped("big", &[("skills/a/SKILL.md", big.as_str()), ("skills/a/data.txt", big.as_str())])).unwrap_err();
        assert_eq!(err, "These files are more than 2 MB. A profile holds 2 MB at most");
    }

    #[cfg(unix)]
    #[test]
    fn a_skill_on_this_mac_is_listed_once_and_read_through_its_link() {
        let home = folder("home", &[(".agents/skills/review/SKILL.md", "# Review"), (".agents/skills/review/run.sh", "x"), (".claude/skills/mine/SKILL.md", "# Mine"), (".claude/skills/notes.md", "x")]);
        std::os::unix::fs::symlink(home.join(".agents/skills/review"), home.join(".claude/skills/review")).unwrap();
        let found = installed(&home);
        assert_eq!(found.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(), ["mine", "review"]);
        let p = read_skill(&found[1].1).unwrap();
        assert_eq!(p.entries.iter().map(|e| (e.path.as_str(), e.runs)).collect::<Vec<_>>(), [("skills/review/SKILL.md", false), ("skills/review/run.sh", true)]);
        assert_eq!((p.from.as_str(), p.profile.skills), ("review", 1));
    }
```

**Step 2: Run the tests to see them fail**

Run: `cargo test -p pocket factory::import`
Expected: FAIL to compile with `unresolved imports super::installed, super::read_skill`.

**Step 3: Implement**

Add `use std::io::Read as _;` to the imports. In `read`, add the archive before the last line:

```rust
// before
    Err(format!("{from} is not a folder or a .zip"))
// after
    if path.is_file() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
        let (files, left_out) = archive(path)?;
        return build(from, files, left_out);
    }
    Err(format!("{from} is not a folder or a .zip"))
```

Add after `walk`:

```rust
/// An archive entry's name as its parts. None when the name could land outside the folder the archive stands for.
fn split(name: &str) -> Option<Vec<&str>> {
    if name.starts_with('/') || name.contains(['\\', '\0', ':']) {
        return None;
    }
    let parts: Vec<&str> = name.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
    (!parts.contains(&"..")).then_some(parts)
}

/// The layout's files in a `.zip`. Every name is checked before any file is read. One folder that wraps everything is dropped.
fn archive(path: &Path) -> Result<(Vec<Loaded>, usize), String> {
    let unread = |e: &dyn std::fmt::Display| format!("Can't read {}: {e}", name_of(path));
    let mut zip = zip::ZipArchive::new(fs::File::open(path).map_err(|e| unread(&e))?).map_err(|e| unread(&e))?;
    let mut kept: Vec<(usize, Vec<String>, usize)> = Vec::new();
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| unread(&e))?;
        let name = entry.name().to_string();
        let Some(parts) = split(&name) else { return Err(format!("{name} points outside the archive")) };
        if entry.is_symlink() {
            return Err(format!("{name} is a link. Import needs the file itself"));
        }
        if entry.is_dir() || parts.is_empty() || parts.iter().any(|p| ignored(p)) {
            continue;
        }
        kept.push((i, parts.into_iter().map(String::from).collect(), entry.size() as usize));
    }
    let wrapper = kept.first().map(|(_, parts, _)| parts[0].clone()).filter(|top| top != SKILLS && kept.iter().all(|(_, parts, _)| parts.len() > 1 && parts[0] == *top));
    let skip = usize::from(wrapper.is_some());
    let (mut files, mut left_out, mut total) = (Vec::new(), 0, 0);
    for (i, parts, size) in kept {
        let at = parts[skip..].join("/");
        if !matches!(at.as_str(), SOUL | INSTRUCTIONS | AGENTS) && skill_of(&at).is_none() {
            left_out += 1;
            continue;
        }
        total += size;
        if total > MAX_PROFILE {
            return Err(too_big());
        }
        // An archive can say a file is smaller than it is, so the read stops at the limit too.
        let mut bytes = Vec::new();
        zip.by_index(i).map_err(|e| unread(&e))?.take(MAX_PROFILE as u64 + 1).read_to_end(&mut bytes).map_err(|e| unread(&e))?;
        if bytes.len() > MAX_PROFILE {
            return Err(too_big());
        }
        files.push((at, bytes));
    }
    Ok((files, left_out))
}

/// A skill on this Mac, as an import of `skills/<name>/…`. `dir` may be a link, as the entries of `~/.claude/skills` often are.
pub fn read_skill(dir: &Path) -> Result<Preview, String> {
    let name = name_of(dir);
    let real = fs::canonicalize(dir).map_err(|e| format!("Can't read {name}: {e}"))?;
    let (mut files, mut total) = (Vec::new(), 0);
    walk(&real, &format!("{SKILLS}/{name}"), &mut files, &mut total)?;
    build(name, files, 0)
}

/// The skills on this Mac by name: each folder with a `SKILL.md` under `~/.claude/skills` and `~/.agents/skills`.
pub fn installed(home: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    for root in [".claude/skills", ".agents/skills"] {
        for (name, path) in children(&home.join(root)).unwrap_or_default() {
            if path.join("SKILL.md").is_file() && !out.iter().any(|(known, _)| *known == name) {
                out.push((name, path));
            }
        }
    }
    out.sort();
    out
}
```

A colon is refused because a Windows drive prefix holds one.

**Step 4: Run the tests to see them pass**

Run: `cargo test -p pocket factory::import`
Expected: PASS, 15 tests. If `an_entry_that_points_outside_the_archive_is_refused` fails because the installed `zip` cleans a name on write, test `split` on the three names in place of the archive, and keep the assertion on the message.

### Task 2.3: `Draft`, the profile being edited

**Files:**
- Create: `packages/desktop/crates/pocket/src/factory/profile_sheet.rs`
- Modify: `packages/desktop/crates/pocket/src/factory.rs` (the `mod` lines), `factory/import.rs` (`skill_of` becomes `pub(crate)`)
- Test: the `tests` module at the bottom of `profile_sheet.rs`

**Context:** The sheet's state holds text field entities, so it cannot be built in a test. Its decisions live on `Draft`, a plain value, and in two free functions. pocketd keeps a profile's name unique without regard to case, and answers `invalid_profile` otherwise (index, section "Wire"). The sheet says so before you save. pocketd also checks the two size limits again on save, so the sheet does not repeat them for typed text. Taking an import replaces a skill that has the same name. Nothing reaches pocketd until **Save profile**.

**Step 1: Write the failing tests**

In `factory.rs`, add `pub(crate) mod profile_sheet;`. In `factory/import.rs`, change `fn skill_of` to `pub(crate) fn skill_of`.

Create `factory/profile_sheet.rs` with only the tests:

```rust
#[cfg(test)]
mod tests {
    use super::{Draft, Skill, can_save, taken};
    use crate::factory::import::Preview;
    use agents::factory::{Profile, ProfileFile, ProfileInfo};

    fn file(path: &str) -> ProfileFile {
        ProfileFile { path: path.into(), text: "x".into(), binary: false }
    }

    fn info(id: &str, name: &str) -> ProfileInfo {
        ProfileInfo { id: id.into(), name: name.into(), ..Default::default() }
    }

    fn paths(draft: &Draft) -> Vec<&str> {
        draft.files.iter().map(|f| f.path.as_str()).collect()
    }

    #[test]
    fn a_name_another_profile_has_cannot_be_saved_whatever_its_case() {
        let profiles = [info("claude-opus", "Claude · Opus"), info("go", "Go backend")];
        assert!(taken(" go BACKEND ", "", &profiles));
        assert!(!taken("Go backend", "go", &profiles), "a profile keeps its own name");
        assert!(!taken("Rust", "", &profiles));
        let saves = [can_save("Rust", "", &profiles, false), can_save("  ", "", &profiles, false), can_save("go backend", "", &profiles, false), can_save("Rust", "", &profiles, true)];
        assert_eq!(saves, [true, false, false, false]);
    }

    #[test]
    fn skills_are_listed_by_folder_with_their_file_count_and_whether_one_can_run() {
        let draft = Draft { files: vec![file("skills/review/SKILL.md"), file("skills/review/run.sh"), file("skills/notes/SKILL.md")], ..Draft::new() };
        assert_eq!(draft.skills(), [Skill { name: "review".into(), files: 2, runs: true }, Skill { name: "notes".into(), files: 1, runs: false }]);
    }

    #[test]
    fn removing_a_skill_drops_its_files_only() {
        let mut draft = Draft { files: vec![file("skills/review/SKILL.md"), file("skills/review/run.sh"), file("skills/notes/SKILL.md")], ..Draft::new() };
        draft.remove_skill("review");
        assert_eq!(paths(&draft), ["skills/notes/SKILL.md"]);
    }

    #[test]
    fn taking_an_import_adds_its_skills_and_replaces_one_with_the_same_name() {
        let mut draft = Draft { files: vec![file("skills/review/SKILL.md"), file("skills/review/old.sh"), file("skills/notes/SKILL.md")], ..Draft::new() };
        let preview = Preview { profile: Profile { files: vec![file("skills/review/SKILL.md"), file("skills/deploy/SKILL.md")], ..Default::default() }, ..Default::default() };
        draft.take(&preview);
        assert_eq!(paths(&draft), ["skills/notes/SKILL.md", "skills/review/SKILL.md", "skills/deploy/SKILL.md"]);
    }

    #[test]
    fn the_saved_profile_carries_the_trimmed_name_the_texts_and_the_skill_count() {
        let draft = Draft { id: "go".into(), provider: "codex".into(), model: "gpt-5".into(), built_in: false, files: vec![file("skills/review/SKILL.md")] };
        let p = draft.profile(" Go backend ", "Soul", "How");
        let want = Profile { id: "go".into(), name: "Go backend".into(), provider: "codex".into(), model: "gpt-5".into(), skills: 1, built_in: false, soul: "Soul".into(), instructions: "How".into(), files: vec![file("skills/review/SKILL.md")] };
        assert_eq!(p, want);
        assert_eq!(Draft::of(&p), draft);
        assert_eq!(Draft::new().provider, "claude");
    }
}
```

**Step 2: Run the tests to see them fail**

Run: `cargo test -p pocket factory::profile_sheet`
Expected: FAIL to compile with `unresolved imports super::Draft, super::Skill, super::can_save, super::taken`.

**Step 3: Implement**

Put this above the `tests` module:

```rust
use super::import::{Preview, skill_of};
use agents::factory::{Profile, ProfileFile, ProfileInfo};

/// The providers the sheet offers: the ones the built-in profiles run on.
pub(crate) const PROVIDERS: [&str; 2] = ["claude", "codex"];

/// A skill of the profile being edited, as its row shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Skill {
    pub name: String,
    pub files: usize,
    /// It has a file that isn't markdown.
    pub runs: bool,
}

/// The profile being edited, without the three texts its fields hold.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Draft {
    /// Empty for a new profile.
    pub id: String,
    pub provider: String,
    pub model: String,
    pub built_in: bool,
    pub files: Vec<ProfileFile>,
}

impl Draft {
    pub fn new() -> Self {
        Self { provider: PROVIDERS[0].into(), ..Default::default() }
    }

    pub fn of(p: &Profile) -> Self {
        Self { id: p.id.clone(), provider: p.provider.clone(), model: p.model.clone(), built_in: p.built_in, files: p.files.clone() }
    }

    pub fn skills(&self) -> Vec<Skill> {
        let mut out: Vec<Skill> = Vec::new();
        for f in &self.files {
            let Some(name) = skill_of(&f.path) else { continue };
            let runs = !f.path.to_lowercase().ends_with(".md");
            match out.iter_mut().find(|s| s.name == name) {
                Some(s) => (s.files, s.runs) = (s.files + 1, s.runs || runs),
                None => out.push(Skill { name: name.into(), files: 1, runs }),
            }
        }
        out
    }

    pub fn remove_skill(&mut self, name: &str) {
        self.files.retain(|f| skill_of(&f.path) != Some(name));
    }

    /// Takes an import's skills. One whose name the draft already has replaces it.
    pub fn take(&mut self, preview: &Preview) {
        for name in preview.profile.files.iter().filter_map(|f| skill_of(&f.path)) {
            self.remove_skill(name);
        }
        self.files.extend(preview.profile.files.iter().cloned());
    }

    /// What `factory.profile.save` carries.
    pub fn profile(&self, name: &str, soul: &str, instructions: &str) -> Profile {
        Profile {
            id: self.id.clone(),
            name: name.trim().into(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            skills: self.skills().len(),
            built_in: false,
            soul: soul.into(),
            instructions: instructions.into(),
            files: self.files.clone(),
        }
    }
}

/// Whether a profile other than `id` has `name`, whatever its case.
pub fn taken(name: &str, id: &str, profiles: &[ProfileInfo]) -> bool {
    let name = name.trim().to_lowercase();
    profiles.iter().any(|p| p.id != id && p.name.to_lowercase() == name)
}

/// Save needs a name no other profile has, and no import waiting to be taken or dropped.
pub fn can_save(name: &str, id: &str, profiles: &[ProfileInfo], previewing: bool) -> bool {
    !name.trim().is_empty() && !taken(name, id, profiles) && !previewing
}
```

**Step 4: Run the tests to see them pass**

Run: `cargo test -p pocket factory::profile_sheet && cargo test -p pocket factory::import`
Expected: PASS.

### Task 2.4: The profile sheet

**Files:**
- Modify: `packages/desktop/crates/pocket/src/factory/profile_sheet.rs`
- Modify: `packages/desktop/crates/pocket/src/factory.rs` (`FactoryState` and its `new`)
- Modify: `packages/desktop/crates/pocket/src/desktop/chrome.rs:55-64` (`Overlay`), `modals.rs:15-21`, `modals.rs:30-36`, `modals.rs:57-65`
- Modify: `packages/desktop/crates/pocket/src/settings.rs:360` (`action` becomes `pub(crate)`)
- Modify: `packages/desktop/crates/pocket/src/desktop/alerts.rs:126` (top of `on_agents_from`)
- Test: the `tests` module of `profile_sheet.rs`

**Context:** A sheet is an `Overlay` variant drawn by `overlay_view` (`modals.rs:53`), as `team/sheet.rs:52` is. The model menu is the one Settings uses: `settings::dropdown::dropdown` with `settings::models::model_rows` (`settings/git.rs:92`). It opens while `settings.menu` holds its id. `Desktop::offered_models(provider)` gives a provider's models and `Desktop::load_models_of(provider, cx)` asks for them (`models.rs:239`, `:262`). A built-in profile has no soul, instructions or skills and cannot change (PF11), so the sheet shows it read-only from its `ProfileInfo` and asks pocketd for nothing. A custom profile's texts and files come with `Event::FactoryProfile` after `Outbox::profile_get`. A preview can hold hundreds of files, so its rows go through `uniform_list`. pocketd answers a save or a delete with `factory.profiles` (the sheet closes) or with an error (the sheet stays and shows the message, which for a delete is the `profile_in_use` text that lists each user).

**Step 1: Write the failing tests**

In the `tests` module, change the first `use` line to `use super::{Draft, Line, Skill, can_save, lines, size, taken, title};` and add `use crate::factory::import::Entry;`. Add:

```rust
    #[test]
    fn the_sheet_is_named_for_what_it_edits() {
        let (new, mine, fixed) = (Draft::new(), Draft { id: "go".into(), ..Draft::new() }, Draft { id: "codex".into(), built_in: true, ..Draft::new() });
        assert_eq!((title(&new, ""), title(&mine, "Go backend"), title(&fixed, "Codex")), ("New profile".to_string(), "Edit profile".to_string(), "Codex".to_string()));
    }

    #[test]
    fn the_preview_lists_markdown_first_then_the_files_the_agent_can_run() {
        let entry = |path: &str, runs| Entry { path: path.into(), size: 1, runs };
        let preview = Preview { entries: vec![entry("SOUL.md", false), entry("skills/a/run.sh", true), entry("skills/a/SKILL.md", false)], ..Default::default() };
        assert_eq!(lines(&preview), [Line::File(0), Line::File(2), Line::Runs, Line::File(1)]);
        let plain = Preview { entries: vec![entry("SOUL.md", false)], ..Default::default() };
        assert_eq!(lines(&plain), [Line::File(0)]);
    }

    #[test]
    fn sizes_read_in_bytes_then_kilobytes_then_megabytes() {
        assert_eq!([size(512), size(1024), size(1500), size(1_572_864)], ["512 B", "1 KB", "2 KB", "1.5 MB"]);
    }
```

**Step 2: Run the tests to see them fail**

Run: `cargo test -p pocket factory::profile_sheet`
Expected: FAIL to compile with `unresolved imports super::Line, super::lines, super::size, super::title`.

**Step 3: Implement the plain parts**

Add to `profile_sheet.rs`, after `can_save`:

```rust
pub fn title(draft: &Draft, name: &str) -> String {
    match (draft.built_in, draft.id.is_empty()) {
        (true, _) => name.to_string(),
        (_, true) => "New profile".into(),
        _ => "Edit profile".into(),
    }
}

/// A row of the import preview: entry `i`, or the heading over the files that aren't markdown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Line {
    File(usize),
    Runs,
}

pub fn lines(p: &Preview) -> Vec<Line> {
    let (runs, reads): (Vec<usize>, Vec<usize>) = (0..p.entries.len()).partition(|&i| p.entries[i].runs);
    let mut out: Vec<Line> = reads.into_iter().map(Line::File).collect();
    if !runs.is_empty() {
        out.push(Line::Runs);
        out.extend(runs.into_iter().map(Line::File));
    }
    out
}

pub fn size(bytes: usize) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{} KB", bytes.div_ceil(1024)),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.),
    }
}
```

Run: `cargo test -p pocket factory::profile_sheet`
Expected: PASS.

**Step 4: Add the overlay**

`desktop/chrome.rs:61-64`:

```rust
// before
    /// A lead's team plan, to edit and start.
    Team,
}
// after
    /// A lead's team plan, to edit and start.
    Team,
    /// A Factory profile, to edit and save.
    Profile,
}
```

`modals.rs:19`, `:35` and `:64`:

```rust
// before
        Overlay::Palette | Overlay::AddRepo | Overlay::Clone | Overlay::More | Overlay::Confirm => true,
// after
        Overlay::Palette | Overlay::AddRepo | Overlay::Clone | Overlay::More | Overlay::Confirm | Overlay::Profile => true,
```

```rust
// before
            Overlay::More | Overlay::Confirm | Overlay::Team => {}
// after
            Overlay::More | Overlay::Confirm | Overlay::Team | Overlay::Profile => {}
```

```rust
// before
            Overlay::Team => (self.team_sheet(cx), 0x29, Some((240, 8.))),
// after
            Overlay::Team => (self.team_sheet(cx), 0x29, Some((240, 8.))),
            Overlay::Profile => (self.profile_sheet(cx), 0x29, Some((240, 8.))),
```

`settings.rs:360`: change `fn action(` to `pub(crate) fn action(`.

`factory.rs`: `FactoryState` gains a field, and `FactoryState::new` sets it to `None`:

```rust
    /// The profile sheet, while it is open.
    pub(crate) profile: Option<profile_sheet::ProfileSheet>,
```

`desktop/alerts.rs`, at the top of `on_agents_from` (beside the `FactoryUpdate` lines of Task 1.4):

```rust
        self.profile_event(&ev, window, cx);
```

**Step 5: Implement the sheet**

Add to the top of `profile_sheet.rs`, replacing its two `use` lines:

```rust
use super::import::{self, Entry, Preview, skill_of};
use crate::composer::model_menu::listed_name;
use crate::desktop::Desktop;
use crate::desktop::chrome::Overlay;
use crate::settings::action;
use crate::settings::dropdown::dropdown;
use crate::settings::models::model_rows;
use agents::Event;
use agents::factory::{Profile, ProfileFile, ProfileInfo};
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::ops::Range;
use std::path::PathBuf;
use theme::*;
use ui::{self, Segment, Variant};

const WIDTH: f32 = 560.;
const LINE: f32 = 22.;
/// The preview rows shown before the list scrolls.
const LINES: usize = 10;

pub struct ProfileSheet {
    pub(crate) draft: Draft,
    pub(crate) name: Entity<InputState>,
    pub(crate) soul: Entity<TextareaState>,
    pub(crate) instructions: Entity<TextareaState>,
    /// An import that was read and is not yet taken, and its rows.
    pub(crate) preview: Option<Preview>,
    pub(crate) lines: Vec<Line>,
    /// Why the last import, save or delete was refused.
    pub(crate) refused: Option<String>,
    /// The skills on this Mac, while their list is open.
    pub(crate) installed: Option<Vec<(String, PathBuf)>>,
    /// The profile asked for and not yet here.
    pub(crate) loading: Option<String>,
    /// A save or a delete that pocketd hasn't answered.
    pub(crate) sent: bool,
    _subs: Vec<Subscription>,
}
```

Add at the bottom, above the `tests` module:

```rust
impl Desktop {
    /// Opens the sheet on profile `id`, or on a new one.
    pub(crate) fn edit_profile(&mut self, id: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        let info = id.and_then(|id| self.agents.profiles().iter().find(|p| p.id == id)).cloned();
        let draft = info.as_ref().map_or_else(Draft::new, |p| Draft { id: p.id.clone(), provider: p.provider.clone(), model: p.model.clone(), built_in: p.built_in, files: Vec::new() });
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Go backend"));
        let soul = cx.new(|cx| TextareaState::new(window, cx).placeholder("You are a careful engineer who…").auto_grow(3, 8));
        let instructions = cx.new(|cx| TextareaState::new(window, cx).placeholder("Write a failing test first…").auto_grow(3, 8));
        if let Some(p) = &info {
            name.update(cx, |s, cx| s.set_value(p.name.clone(), window, cx));
        }
        let loading = info.filter(|p| !p.built_in).map(|p| p.id);
        if let Some(id) = &loading {
            self.outbox.local.profile_get(id);
        }
        self.load_models_of(&draft.provider.clone(), cx);
        let subs = vec![cx.subscribe(&name, |_, _, _: &InputEvent, cx| cx.notify())];
        self.factory.profile = Some(ProfileSheet { draft, name, soul, instructions, preview: None, lines: Vec::new(), refused: None, installed: None, loading, sent: false, _subs: subs });
        self.open(Overlay::Profile, window, cx);
    }

    fn close_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        (self.factory.profile, self.settings.menu) = (None, None);
        self.close_overlay(window, cx);
    }

    /// What pocketd says while the sheet is open.
    pub(crate) fn profile_event(&mut self, ev: &Event, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlay != Some(Overlay::Profile) {
            return;
        }
        let Some(sheet) = self.factory.profile.as_mut() else { return };
        match ev {
            Event::FactoryProfile(p) if sheet.loading.as_deref() == Some(p.id.as_str()) => {
                (sheet.loading, sheet.draft) = (None, Draft::of(p));
                sheet.name.update(cx, |s, cx| s.set_value(p.name.clone(), window, cx));
                sheet.soul.update(cx, |s, cx| s.set_value(p.soul.clone(), window, cx));
                sheet.instructions.update(cx, |s, cx| s.set_value(p.instructions.clone(), window, cx));
            }
            Event::FactoryProfiles(_) if sheet.sent => self.close_profile(window, cx),
            Event::FactoryError(why) if sheet.sent => (sheet.sent, sheet.refused) = (false, Some(why.clone())),
            _ => {}
        }
    }

    fn save_profile(&mut self, cx: &mut Context<Self>) {
        let Some(sheet) = self.factory.profile.as_mut() else { return };
        let name = sheet.name.read(cx).value().to_string();
        if sheet.sent || !can_save(&name, &sheet.draft.id, self.agents.profiles(), sheet.preview.is_some()) {
            return;
        }
        let (soul, instructions) = (sheet.soul.read(cx).value().to_string(), sheet.instructions.read(cx).value().to_string());
        (sheet.sent, sheet.refused) = (true, None);
        self.outbox.local.profile_save(&sheet.draft.profile(&name, &soul, &instructions));
        cx.notify();
    }

    fn delete_profile(&mut self, cx: &mut Context<Self>) {
        let Some(sheet) = self.factory.profile.as_mut().filter(|s| !s.sent && !s.draft.built_in && !s.draft.id.is_empty()) else { return };
        (sheet.sent, sheet.refused) = (true, None);
        self.outbox.local.profile_delete(&sheet.draft.id);
        cx.notify();
    }

    fn set_profile_provider(&mut self, provider: &'static str, cx: &mut Context<Self>) {
        if let Some(sheet) = self.factory.profile.as_mut() {
            (sheet.draft.provider, sheet.draft.model) = (provider.into(), String::new());
        }
        self.settings.menu = None;
        self.load_models_of(provider, cx);
        cx.notify();
    }

    /// Reads off the UI thread what `read` reads, and shows it as a preview or says why not.
    fn preview_import(&mut self, read: impl FnOnce() -> Result<Preview, String> + Send + 'static, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { read() }).await;
            this.update(cx, |d, cx| {
                if let Some(sheet) = d.factory.profile.as_mut() {
                    (sheet.lines, sheet.preview, sheet.refused) = match result {
                        Ok(p) => (lines(&p), Some(p), None),
                        Err(why) => (Vec::new(), None, Some(why)),
                    };
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// One panel takes a folder or a `.zip`, which `pick_path` can't ask for.
    fn import_profile(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: true, multiple: false, prompt: None });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else { return };
            let Some(path) = paths.into_iter().next() else { return };
            this.update(cx, |d, cx| d.preview_import(move || import::read(&path), cx)).ok();
        })
        .detach();
    }

    fn toggle_installed(&mut self, cx: &mut Context<Self>) {
        let Some(sheet) = self.factory.profile.as_mut() else { return };
        if sheet.installed.take().is_some() {
            return cx.notify();
        }
        let home = PathBuf::from(crate::modals::form::home());
        cx.spawn(async move |this, cx| {
            let found = cx.background_executor().spawn(async move { import::installed(&home) }).await;
            this.update(cx, |d, cx| {
                if let Some(sheet) = d.factory.profile.as_mut() {
                    sheet.installed = Some(found);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn use_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(sheet) = self.factory.profile.as_mut() else { return };
        let Some(preview) = sheet.preview.take() else { return };
        sheet.draft.take(&preview);
        let Profile { soul, instructions, .. } = preview.profile;
        if !soul.is_empty() {
            sheet.soul.update(cx, |s, cx| s.set_value(soul, window, cx));
        }
        if !instructions.is_empty() {
            sheet.instructions.update(cx, |s, cx| s.set_value(instructions, window, cx));
        }
        cx.notify();
    }

    pub(crate) fn profile_sheet(&mut self, cx: &mut Context<Self>) -> Div {
        // Taken out for this frame, so the parts can read the sheet while they borrow `self` for their listeners.
        let Some(sheet) = self.factory.profile.take() else { return div() };
        let view = self.profile_view(&sheet, cx);
        self.factory.profile = Some(sheet);
        view
    }

    fn profile_view(&mut self, sheet: &ProfileSheet, cx: &mut Context<Self>) -> Div {
        let draft = &sheet.draft;
        let name = sheet.name.read(cx).value().to_string();
        let savable = !sheet.sent && sheet.loading.is_none() && can_save(&name, &draft.id, self.agents.profiles(), sheet.preview.is_some());
        let head = div().pt(px(20.)).px(px(22.)).pb(px(14.)).text_size(px(17.)).line_height(px(22.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(title(draft, &name));
        let close = ui::button("profile-close", Variant::Ghost, None, if draft.built_in { "Close" } else { "Cancel" }).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_profile(window, cx)));
        let delete = (!draft.built_in && !draft.id.is_empty()).then(|| ui::button("profile-delete", Variant::Ghost, None, "Delete").text_color(FAILED).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.delete_profile(cx))));
        let save = (!draft.built_in).then(|| {
            ui::button("profile-save", Variant::Primary, None, "Save profile")
                .when(savable, |b| b.on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.save_profile(cx))))
                .when(!savable, |b| b.opacity(0.4))
        });
        let foot = div().pt(px(14.)).pb(px(16.)).px(px(16.)).flex().items_center().gap(px(8.)).children(delete).child(div().flex_1()).child(close).children(save);
        let body = if draft.built_in { fixed(&name, self.agent_name(&draft.provider), &draft.model) } else { self.profile_fields(sheet, &name, cx) };
        div().absolute().inset_0().flex().items_center().justify_center().child(
            ui::pop(div().id("profile-sheet"))
                .w(px(WIDTH))
                .max_h(relative(0.9))
                .rounded(px(R_DIALOG))
                .occlude()
                .flex()
                .flex_col()
                .child(head)
                .child(div().id("profile-body").flex_1().min_h_0().overflow_y_scroll().px(px(22.)).pb(px(14.)).child(body))
                .child(div().flex_none().border_t(px(0.5)).border_color(HAIRLINE).child(foot)),
        )
    }

    fn profile_fields(&mut self, sheet: &ProfileSheet, name: &str, cx: &mut Context<Self>) -> Div {
        let draft = &sheet.draft;
        let name_taken = taken(name, &draft.id, self.agents.profiles());
        let name_field = field("Name", "")
            .child(ui::field_box().child(Input::new(&sheet.name).appearance(false).p_0().text_size(px(13.5))))
            .children(name_taken.then(|| div().text_size(px(12.)).text_color(FAILED).child("Another profile has this name")));
        let active = PROVIDERS.into_iter().find(|p| *p == draft.provider).unwrap_or(PROVIDERS[0]);
        let segments = PROVIDERS.into_iter().map(|p| Segment { icon: None, value: p, label: self.agent_name(p).into(), badge: None }).collect();
        let providers = ui::segmented(segments, active, true, false, |this: &mut Self, provider, cx| this.set_profile_provider(provider, cx), cx);
        let (provider, model) = (draft.provider.as_str(), draft.model.clone());
        let offered = self.offered_models(provider);
        let trigger = action("profile-model", "").child(listed_name(&model, offered)).gap(px(6.)).child(icon("chevron-down", 12., TEXT_3));
        let from = (provider, self.models.revision, self.installed(provider), &model);
        let pick = |d: &mut Desktop, value: String, _: &mut Context<Desktop>| {
            if let Some(sheet) = d.factory.profile.as_mut() {
                sheet.draft.model = value;
            }
        };
        let models = dropdown("profile-model", trigger, from, || model_rows(provider, offered, &model), &model, self, pick, cx);
        let area = |state: &Entity<TextareaState>| ui::field_box().h_auto().min_h(px(72.)).px(px(2.)).py(px(4.)).items_start().child(div().flex_1().min_w_0().text_size(px(13.5)).child(Textarea::new(state).appearance(false)));
        let skills = draft.skills();
        let rows: Vec<Div> = if skills.is_empty() {
            vec![div().py(px(6.)).text_size(px(12.5)).text_color(TEXT_3).child("No skills")]
        } else {
            skills.into_iter().enumerate().map(|(i, s)| self.profile_skill(i, s, cx)).collect()
        };
        let bring = match &sheet.preview {
            Some(p) => self.profile_preview(p, sheet.lines.len(), cx),
            None => self.profile_import(sheet.installed.as_deref(), cx),
        };
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(name_field)
            .child(field("Model", "").child(div().flex().items_center().gap(px(10.)).child(providers).child(models)))
            .child(field("Soul", "Who the agent is: tone, habits, limits").child(area(&sheet.soul)))
            .child(field("Instructions", "How it works on a task").child(area(&sheet.instructions)))
            .child(field("Skills", "Copied into the agent's checkout, never into a commit").children(rows).child(bring))
            .children(sheet.refused.clone().map(|why| div().text_size(px(12.5)).text_color(FAILED).child(why)))
    }

    fn profile_skill(&self, i: usize, s: Skill, cx: &mut Context<Self>) -> Div {
        let files = if s.files == 1 { "1 file".to_string() } else { format!("{} files", s.files) };
        let meta = if s.runs { format!("{files} · has files the agent can run") } else { files };
        let name = s.name.clone();
        let remove = ui::icon_button_sized(("profile-skill-remove", i), "x", 20., TEXT_3).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            if let Some(sheet) = this.factory.profile.as_mut() {
                sheet.draft.remove_skill(&name);
            }
            cx.notify();
        }));
        div()
            .h(px(30.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(icon("folder", 14., TEXT_3))
            .child(div().flex_none().font_family(MONO).text_size(px(12.5)).child(s.name))
            .child(div().flex_1().min_w_0().truncate().text_size(px(12.)).text_color(TEXT_3).child(meta))
            .child(remove)
    }

    /// The two ways to bring files in, and the skills on this Mac while that list is open.
    fn profile_import(&self, installed: Option<&[(String, PathBuf)]>, cx: &mut Context<Self>) -> Div {
        let folder = ui::button("profile-import", Variant::Secondary, None, "Import folder or zip").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.import_profile(cx)));
        let mine = ui::button("profile-installed", Variant::Secondary, None, "Add an installed skill").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_installed(cx)));
        let list = installed.map(|found| {
            let rows: Vec<AnyElement> = if found.is_empty() {
                vec![div().p(px(8.)).text_size(px(12.5)).text_color(TEXT_3).child("No skills in ~/.claude/skills or ~/.agents/skills").into_any_element()]
            } else {
                found
                    .iter()
                    .enumerate()
                    .map(|(i, (name, dir))| {
                        let dir = dir.clone();
                        div()
                            .id(("profile-installed-skill", i))
                            .h(px(26.))
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .rounded(px(5.))
                            .cursor_pointer()
                            .hover(|s| s.bg(FILL_2))
                            .font_family(MONO)
                            .text_size(px(12.5))
                            .child(name.clone())
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                if let Some(sheet) = this.factory.profile.as_mut() {
                                    sheet.installed = None;
                                }
                                let dir = dir.clone();
                                this.preview_import(move || import::read_skill(&dir), cx);
                            }))
                            .into_any_element()
                    })
                    .collect()
            };
            div().id("profile-installed-list").max_h(px(156.)).overflow_y_scroll().p(px(4.)).flex().flex_col().rounded(px(9.)).bg(FILL_1).children(rows)
        });
        div().pt(px(6.)).flex().flex_col().gap(px(8.)).child(div().flex().gap(px(8.)).child(folder).child(mine)).children(list)
    }

    /// What an import holds: every file with its size, the ones the agent can run under their own heading.
    fn profile_preview(&self, p: &Preview, count: usize, cx: &mut Context<Self>) -> Div {
        let files = uniform_list(
            "profile-preview-files",
            count,
            cx.processor(|this, range: Range<usize>, _, _| {
                let Some((p, lines)) = this.factory.profile.as_ref().and_then(|s| Some((s.preview.as_ref()?, &s.lines))) else { return Vec::new() };
                lines.get(range).unwrap_or_default().iter().map(|line| preview_line(p, *line)).collect::<Vec<_>>()
            }),
        )
        .h(px(LINE * count.min(LINES) as f32));
        let left_out = (p.left_out > 0).then(|| div().text_size(px(12.)).text_color(TEXT_3).child(format!("{} left out. They are no part of a profile.", if p.left_out == 1 { "1 file".to_string() } else { format!("{} files", p.left_out) })));
        let cancel = ui::button("profile-preview-cancel", Variant::Ghost, None, "Cancel").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            if let Some(sheet) = this.factory.profile.as_mut() {
                sheet.preview = None;
            }
            cx.notify();
        }));
        let take = ui::button("profile-preview-use", Variant::Primary, None, "Use these files").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.use_import(window, cx)));
        div()
            .mt(px(6.))
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .rounded(px(9.))
            .bg(FILL_1)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(12.5))
                    .child(icon("folder", 14., TEXT_3))
                    .child(div().flex_1().min_w_0().truncate().font_weight(FontWeight::MEDIUM).child(p.from.clone()))
                    .child(div().flex_none().text_color(TEXT_3).child(format!("{} of 2 MB", size(p.size())))),
            )
            .child(files)
            .children(left_out)
            .child(div().text_size(px(12.)).text_color(TEXT_3).child("AGENTS.md is read as the instructions. Nothing is saved until you save the profile."))
            .child(div().flex().justify_end().gap(px(8.)).child(cancel).child(take))
    }
}

fn field(label: &'static str, hint: &'static str) -> Div {
    div().flex().flex_col().gap(px(6.)).child(div().flex().items_baseline().gap(px(8.)).child(ui::field_label(label)).child(div().text_size(px(12.)).text_color(TEXT_4).child(hint)))
}

/// A built-in profile: nothing in it can change.
fn fixed(name: &str, agent: String, model: &str) -> Div {
    let line = |label: &'static str, value: String| field(label, "").child(div().text_size(px(13.5)).text_color(TEXT).child(value));
    div()
        .flex()
        .flex_col()
        .gap(px(14.))
        .child(div().text_size(px(13.)).line_height(px(19.)).text_color(TEXT_2).child("A built-in profile has no soul, instructions or skills, and can't be changed."))
        .child(line("Name", name.to_string()))
        .child(line("Model", if model.is_empty() { agent } else { format!("{agent} · {model}") }))
}

fn preview_line(p: &Preview, line: Line) -> Div {
    let row = div().w_full().h(px(LINE)).flex().items_center().gap(px(8.)).text_size(px(12.));
    match line {
        Line::Runs => row.font_weight(FontWeight::SEMIBOLD).text_color(TEXT_2).child("Files the agent can run"),
        Line::File(i) => {
            let Some(Entry { path, size: bytes, .. }) = p.entries.get(i) else { return row };
            row.child(div().flex_1().min_w_0().truncate().font_family(MONO).child(path.clone())).child(div().flex_none().text_color(TEXT_3).child(size(*bytes)))
        }
    }
}
```

**Step 6: Build and run the tests**

Run: `cargo build -p pocket && cargo test -p pocket factory::profile_sheet && cargo test -p pocket modals`
Expected: PASS. The `modals` tests still pass, because `may_open` lets `Overlay::Profile` open for anyone who can see Settings › Factory. pocketd checks the Own scope on a save.

### Task 2.5: The Profiles list in Settings › Factory

**Files:**
- Modify: `packages/desktop/crates/pocket/src/settings/factory.rs` (its rows, and its `tests` module)
- Modify: `packages/desktop/crates/pocket/src/settings.rs:467-482` (`load_section`)
- Test: the `tests` module at the bottom of `settings/factory.rs`

**Context:** Plan 3 creates `settings/factory.rs` with the section's rows as `Setting::custom` entries, and plan 4 adds a profile picker per role (index, section "Desktop"). A custom row's `draw` returns the whole row, and `setting_group` puts the rows of one group into one card (`settings.rs:681`). `settings/factory.rs` is a child of `settings`, so it can call the private `row` and `card` helpers there. The list reads `Agents::profiles()`, which `Event::FactoryProfiles` fills. pocketd sends the list again after every save and delete (plan 4), so the list follows without a request of its own. `ProfileInfo` says how many skills a profile has, not whether it has a soul, so the meta line names the agent, the model and the skills.

**Step 1: Write the failing test**

Add to the `tests` module of `settings/factory.rs` (create the module if plan 3 left none):

```rust
    use super::profile_meta;
    use agents::factory::ProfileInfo;

    #[test]
    fn a_profile_row_names_its_agent_its_model_and_its_skills() {
        let p = |model: &str, skills, built_in| ProfileInfo { id: "p".into(), name: "P".into(), provider: "claude".into(), model: model.into(), skills, built_in };
        assert_eq!(profile_meta(&p("sonnet", 2, false), "Claude Code"), "Claude Code · sonnet · 2 skills");
        assert_eq!(profile_meta(&p("", 1, false), "Claude Code"), "Claude Code · 1 skill");
        assert_eq!(profile_meta(&p("opus", 0, true), "Claude Code"), "Claude Code · opus · No soul, instructions or skills");
        assert_eq!(profile_meta(&p("", 0, false), "Claude Code"), "Claude Code · No skills");
    }
```

**Step 2: Run the test to see it fail**

Run: `cargo test -p pocket settings::factory`
Expected: FAIL to compile with `unresolved import super::profile_meta`.

**Step 3: Implement**

Add to `settings/factory.rs`, beside its other rows. If plan 3 named the rows array differently than `ROWS`, add the row to that array and bump its length:

```rust
    Setting::custom("factory-profiles", "Profiles", "Profiles", profiles_row),
```

Add the functions:

```rust
/// The line under a profile's name.
pub(crate) fn profile_meta(p: &ProfileInfo, agent: &str) -> String {
    let model = (!p.model.is_empty()).then(|| format!(" · {}", p.model)).unwrap_or_default();
    let skills = match (p.built_in, p.skills) {
        (true, _) => "No soul, instructions or skills".to_string(),
        (_, 0) => "No skills".into(),
        (_, 1) => "1 skill".into(),
        (_, n) => format!("{n} skills"),
    };
    format!("{agent}{model} · {skills}")
}

/// Every profile, then a row that makes a new one. Shared by all projects.
fn profiles_row(d: &mut Desktop, _: &'static Setting, cx: &mut Context<Desktop>) -> Div {
    let profiles = d.agents.profiles().to_vec();
    let lead = div().px(px(14.)).pt(px(10.)).pb(px(4.)).text_size(px(12.)).text_color(TEXT_3).child("Shared by all projects. Factory rules always win over a profile.");
    let rows = profiles.iter().enumerate().map(|(i, p)| {
        let id = p.id.clone();
        let title = div().flex().items_center().gap(px(10.)).child(agent_avatar(("profile-avatar", i), &p.provider, d.agent_name(&p.provider))).child(p.name.clone());
        let tag = p.built_in.then(|| div().px(px(7.)).h(px(20.)).flex().items_center().rounded(px(6.)).bg(FILL_2).text_size(px(11.5)).text_color(TEXT_2).child("Built-in"));
        let end = div().flex().items_center().gap(px(8.)).children(tag).child(icon("chevron-right", 12., TEXT_4));
        div()
            .id(("profile-row", i))
            .min_h(px(48.))
            .cursor_pointer()
            .hover(|s| s.bg(FILL_1))
            .border_t(px(0.5))
            .border_color(SEPARATOR)
            .child(row(title, Some(&profile_meta(p, &d.agent_name(&p.provider))), end))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.edit_profile(Some(&id), window, cx)))
    });
    let new = div()
        .id("profile-new")
        .min_h(px(40.))
        .px(px(14.))
        .flex()
        .items_center()
        .gap(px(8.))
        .cursor_pointer()
        .hover(|s| s.bg(FILL_1))
        .border_t(px(0.5))
        .border_color(SEPARATOR)
        .text_size(px(13.))
        .text_color(ACCENT)
        .child(icon("plus", 13., ACCENT))
        .child("New profile")
        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.edit_profile(None, window, cx)));
    div().flex().flex_col().child(lead).children(rows).child(new)
}
```

Add the imports the file lacks: `use crate::board::parts::agent_avatar;`, `use agents::factory::ProfileInfo;`, `use super::row;`, and `gpui_kit::*`, `theme::*` if plan 3 did not import them. `agent_avatar` became `pub(crate)` in Task 1.0. If PR 2 merges before PR 1, make that one change here.

In `load_section` (`settings.rs:472-478`), ask for the list when the section opens:

```rust
// before
            Section::Provider(i) => self.load_models_of(providers::IDS[i], cx),
            _ => {}
// after
            Section::Provider(i) => self.load_models_of(providers::IDS[i], cx),
            Section::Factory => self.outbox.local.profile_list(),
            _ => {}
```

If plan 4 already asks for the list there (for its profile per role), leave its line and add nothing.

**Step 4: Run the tests to see them pass**

Run: `cargo test -p pocket settings::`
Expected: PASS.

### Task 2.6: Capture `settings-factory-profiles` and `profile-sheet-import`, and run the checks

**Files:**
- Modify: `packages/desktop/crates/pocket/src/settings.rs` (`SettingsState` at line 233, `new` at line 265, `reset` at line 300, the `settings-body` line at 813)
- Modify: `packages/desktop/crates/pocket/src/capture.rs` (`STEPS` at line 31, new steps after `settings` at line 559, a helper fn beside `show_team`)

**Context:** The Profiles card sits at the bottom of Settings › Factory, below the window's fold. The settings body has no scroll handle today (`settings.rs:813`), so a capture cannot scroll it. A capture applies pocketd's events in-process with `d.on_agents(event, window, cx)` (`capture.rs:1204`). The import step reads a small folder it writes under the temp dir. It calls `import::read` on the UI thread, which is fine in a capture.

**Step 1: Give the settings body a scroll handle**

`SettingsState` gains a field after `glass`:

```rust
    /// Where the section's page is scrolled to.
    pub(crate) body_scroll: ScrollHandle,
```

In `new`, add `body_scroll: ScrollHandle::new()` to the `Self { … }` literal. In `reset`, add `self.body_scroll.set_offset(point(px(0.), px(0.)));`.

`settings.rs:813`:

```rust
// before
            .child(div().id("settings-body").flex_1().min_h_0().overflow_y_scroll().child(div().w_full().flex().justify_center().child(column)))
// after
            .child(div().id("settings-body").flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.settings.body_scroll).child(div().w_full().flex().justify_center().child(column)))
```

**Step 2: Add the steps**

Bump the length of `STEPS` by two. Add after `("settings", …)`:

```rust
    ("settings-factory-profiles", |d, window, cx| {
        d.settings.section = Section::Factory;
        show_profiles(d, window, cx);
        d.settings.body_scroll.scroll_to_bottom();
    }),
    ("profile-sheet-import", |d, window, cx| {
        show_profiles(d, window, cx);
        d.edit_profile(None, window, cx);
        let root = std::env::temp_dir().join(format!("pocket-capture-import-{}", std::process::id()));
        let files = [("SOUL.md", "You are a careful backend engineer."), ("AGENTS.md", "Write a failing test first."), ("skills/review/SKILL.md", "# Review\n"), ("skills/review/checklist.md", "- Tests\n"), ("skills/deploy/SKILL.md", "# Deploy\n"), ("skills/deploy/deploy.sh", "#!/bin/sh\nmake deploy\n")];
        for (path, text) in files {
            let path = root.join(path);
            _ = std::fs::create_dir_all(path.parent().unwrap());
            _ = std::fs::write(path, text);
        }
        let preview = crate::factory::import::read(&root).ok();
        if let Some(sheet) = d.factory.profile.as_mut() {
            sheet.name.update(cx, |s, cx| s.set_value("Go backend", window, cx));
            sheet.lines = preview.as_ref().map(crate::factory::profile_sheet::lines).unwrap_or_default();
            sheet.preview = preview;
        }
    }),
```

Add the helper beside `show_team` (`capture.rs:1176`):

```rust
/// The profiles of the Profiles captures: the three built-in ones and two of the user's.
fn show_profiles(d: &mut Desktop, window: &mut Window, cx: &mut Context<Desktop>) {
    let p = |id: &str, name: &str, provider: &str, model: &str, skills, built_in| agents::factory::ProfileInfo { id: id.into(), name: name.into(), provider: provider.into(), model: model.into(), skills, built_in };
    let list = vec![
        p("claude-opus", "Claude · Opus", "claude", "opus", 0, true),
        p("claude-sonnet", "Claude · Sonnet", "claude", "sonnet", 0, true),
        p("codex", "Codex", "codex", "", 0, true),
        p("go-backend", "Go backend", "claude", "sonnet", 2, false),
        p("reviewer", "Strict reviewer", "codex", "", 1, false),
    ];
    d.on_agents(agents::Event::FactoryProfiles(list), window, cx);
}
```

In `reset` (`capture.rs:1393`), add `d.agents.profiles.clear();` and `d.factory.profile = None;`. `close_overlay` there already closes the sheet.

**Step 3: Take the pictures**

From the repo root:

```sh
.ui-review/fixture/capture.sh /tmp/lf7-after settings-factory=settings,settings-factory settings-factory-profiles=settings,settings-factory-profiles profile-sheet-import=settings,settings-factory,profile-sheet-import
```

Expected:
- `impl-settings-factory.png` matches `/tmp/lf7-before/impl-settings-factory.png` above the fold.
- `impl-settings-factory-profiles.png` shows the `Profiles` card: the line `Shared by all projects. Factory rules always win over a profile.`, five rows with an avatar, a name, a meta line and a chevron, `Built-in` on the first three, and `New profile` last.
- `impl-profile-sheet-import.png` shows the sheet `New profile` with the name `Go backend`, the Claude and Codex segments, empty soul and instructions, `No skills`, and the preview box: the folder's name, a size of 2 MB, `SOUL.md`, `AGENTS.md` and three skill files, the heading `Files the agent can run` over `skills/deploy/deploy.sh`, the hint, and `Cancel` and `Use these files`. `Save profile` is dimmed.

**Step 4: Run the checks**

From `packages/desktop`: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: PASS with no new warning.

---

## PR 3: The metrics strip

**PR title:** `Settings › Factory shows how a project's Factory did over the last 14 days`. Label: `type: feature`.

**Scope:** US-39 and PRD section 9. pocketd gains `internal/factory/metrics.go`, the verb `factory.metrics` and its reply. The desktop gains `Agents::metrics`, `Outbox::factory_metrics`, and the four tiles at the top of Settings › Factory.

**Depends on:** plans 3, 5 and 6 merged (the activity events `moved`, `approved`, `changes`, `asked`, `allowed`, `denied`, `fix`, `hint`, `merged`, `failed`). It does not depend on PR 1 or PR 2.

**Done when:** the metrics tests pass in Go and Rust, and `settings-factory` shows the strip.

**What stays inert:** for Tickets that finished before this PR, the waits at the Plan and PR gates read as a dash. Their logs have no `gate` event. The tile fills as new Tickets pass the gates.

### Task 3.0: Check the engine's names, and note when a Ticket starts to wait at a gate

**Files:**
- Modify: `packages/pocketd/internal/factory/activity.go` (`Notes`)
- Test: `packages/pocketd/internal/factory/activity_test.go`

**Context:** Plan 3 writes every change of a Ticket to its activity through `Notes(before, after proto.Ticket, team *proto.Team, at int64) []proto.FactoryEvent` in `activity.go`. Plan 6 adds a case for changes at the PR gate there. No event says when a gate opens: `Gate` turns `plan` when the lead proposes a plan (plan 3), and `pr` after `github.Service.Ready` (plan 6), with no note. The waits at the gates need that moment. This task adds the kind `gate` (see "Contract changes needed"). Its Station is the Ticket's station and its Actor is `anywhere`. The Ask gate needs no new kind: `asked` already opens it.

**Step 1: Check the names**

From `packages/pocketd`:

```sh
grep -n "func Notes\|func ReadActivity\|func (e \*Engine) note" internal/factory/activity.go
grep -n "GatePlan\|GatePR\|StationPR\|StationChecks\|StationMerged" internal/proto/*.go | head
grep -n "func (s \*Store) Snapshot" internal/board/store.go
```

Expected: all three functions, the five constants, and `Snapshot() (tickets []proto.Ticket, limit int)`. If a name differs, use the one in the repository in this PR's code.

**Step 2: Write the failing test**

Add to `internal/factory/activity_test.go`:

```go
func TestATicketThatStartsToWaitAtAGateNotesTheGate(t *testing.T) {
	planning := proto.Ticket{Station: proto.StationPlan}
	atPlan := proto.Ticket{Station: proto.StationPlan, Gate: proto.GatePlan}
	if got := Notes(planning, atPlan, nil, 5); len(got) != 1 || got[0] != (proto.FactoryEvent{At: 5, Station: proto.StationPlan, Actor: "anywhere", Kind: "gate"}) {
		t.Fatalf("plan gate: got %+v", got)
	}
	inReview := proto.Ticket{Station: proto.StationPR}
	atPR := proto.Ticket{Station: proto.StationPR, Gate: proto.GatePR}
	if got := Notes(inReview, atPR, nil, 6); len(got) != 1 || got[0].Kind != "gate" || got[0].Station != proto.StationPR {
		t.Fatalf("pr gate: got %+v", got)
	}
	if got := Notes(atPlan, atPlan, nil, 7); len(got) != 0 {
		t.Fatalf("a gate that stays open is noted once: got %+v", got)
	}
	if got := Notes(planning, proto.Ticket{Station: proto.StationPlan, Gate: proto.GateAsk}, nil, 8); len(got) != 0 {
		t.Fatalf("the Ask gate is noted by asked: got %+v", got)
	}
}
```

**Step 3: Run the test to see it fail**

Run from `packages/pocketd`: `go test ./internal/factory/ -run TestATicketThatStartsToWaitAtAGateNotesTheGate`
Expected: FAIL with `plan gate: got []`.

**Step 4: Implement**

In `Notes`, add before the `return`:

```go
// before
	if fails && (before.Status != proto.TicketFailed || before.Why != after.Why) {
		add(after.Station, "anywhere", "failed", after.Why)
	}
	return out
// after
	if fails && (before.Status != proto.TicketFailed || before.Why != after.Why) {
		add(after.Station, "anywhere", "failed", after.Why)
	}
	if after.Gate != before.Gate && (after.Gate == proto.GatePlan || after.Gate == proto.GatePR) {
		add(after.Station, "anywhere", "gate", "")
	}
	return out
```

Update the comment on `FactoryEvent.Kind` in `internal/proto` so it lists `gate` after `moved`.

If plan 6 sets `Gate: "pr"` outside `Engine.step`, so that `Notes` never sees the change, note the event where it sets the gate instead:

```go
	e.note(t.ID, proto.FactoryEvent{At: e.now().UnixMilli(), Station: proto.StationPR, Actor: "anywhere", Kind: "gate"})
```

**Step 5: Run the tests to see them pass**

Run: `go test ./internal/factory/`
Expected: `ok  	pocketd/internal/factory`. A test of plan 3 or plan 6 that compares a whole list of notes after a gate opens now sees one more event. Add the `gate` event to its expected list. Do not change what it checks otherwise.

### Task 3.1: `metrics.go`: the numbers from the activity logs

**Files:**
- Create: `packages/pocketd/internal/factory/metrics.go`
- Modify: `packages/pocketd/internal/proto/factory.go` (plan 1's file of Factory types), `internal/factory/engine.go`
- Test: `packages/pocketd/internal/factory/metrics_test.go`

**Context:** The index gives `proto.Metrics` to this plan (section "Wire"). This task adds it. Durations are seconds, and 0 means no data. The numbers are a pure function of the logs and the time, so the tests need no engine. Each number is defined here. The window holds the last 14 days, and a sample counts when the event that ends it falls in the window.

- **MergedPerDay:** Tickets with a `merged` event in the window, divided by 14.
- **StartToReady:** per Ticket, from its first `moved` event (the start) to its first `gate` event at Station `pr`. The median.
- **WaitPlan:** from each `gate` event at `plan` to the next `approved` or `changes` at `plan`. A `moved` or `failed` event first ends the wait with no sample. The median.
- **WaitPR:** from each `gate` event at `pr` to the next `merged`, `changes` at `pr`, or `moved` to `merged`. A `failed` event or another `moved` first ends the wait with no sample. The median.
- **WaitAsk:** from each `asked` event to an `allowed` or `denied` event by `you`, oldest Ask first, per Ticket. The reviewer's own `denied` events are no answer. The median.
- **FixRate and FixMean:** the first auto-fix of each Ticket, from its first `fix` event at `checks`. It repaired the Ticket when a `moved` event to `pr` comes next. It did not when a `failed` or `hint` event comes first. A fix still under way is not counted. FixRate is the share that repaired. FixMean is the mean count of `fix` events up to the outcome.
- **Median:** the middle sample, or the mean of the two middle ones.

**Step 1: Write the failing tests**

Create `internal/factory/metrics_test.go`:

```go
package factory

import (
	"math"
	"testing"
	"time"

	"pocketd/internal/proto"
)

var metricsNow = time.UnixMilli(1_800_000_000_000)

// act is an event by anywhere, min minutes before now.
func act(min int, kind, station string) proto.FactoryEvent {
	return proto.FactoryEvent{At: metricsNow.Add(-time.Duration(min) * time.Minute).UnixMilli(), Station: station, Actor: "anywhere", Kind: kind}
}

func actedBy(actor string, e proto.FactoryEvent) proto.FactoryEvent {
	e.Actor = actor
	return e
}

func nearly(a, b float64) bool { return math.Abs(a-b) < 1e-9 }

func TestNoActivityGivesNoNumbersOverFourteenDays(t *testing.T) {
	if got := Compute(nil, metricsNow); got != (proto.Metrics{Days: 14}) {
		t.Fatalf("got %+v", got)
	}
}

func TestMergedPerDayCountsTicketsMergedInTheLastFourteenDays(t *testing.T) {
	var logs [][]proto.FactoryEvent
	for i := range 7 {
		logs = append(logs, []proto.FactoryEvent{act(600*i+10, "moved", proto.StationSpec), act(600*i, "merged", proto.StationMerged)})
	}
	logs = append(logs, []proto.FactoryEvent{act(15*24*60, "merged", proto.StationMerged)})
	if got := Compute(logs, metricsNow); !nearly(got.MergedPerDay, 0.5) {
		t.Fatalf("got %v", got.MergedPerDay)
	}
}

func TestStartToReadyIsTheMedianFromTheStartToThePRGate(t *testing.T) {
	ready := func(minutes int) []proto.FactoryEvent {
		return []proto.FactoryEvent{act(minutes+100, "moved", proto.StationSpec), act(minutes+50, "moved", proto.StationPlan), act(100, "gate", proto.StationPR), act(90, "gate", proto.StationPR)}
	}
	if got := Compute([][]proto.FactoryEvent{ready(60), ready(120), ready(600)}, metricsNow); got.StartToReady != 7200 {
		t.Fatalf("odd count: got %d", got.StartToReady)
	}
	if got := Compute([][]proto.FactoryEvent{ready(60), ready(120)}, metricsNow); got.StartToReady != 5400 {
		t.Fatalf("even count: got %d", got.StartToReady)
	}
}

func TestAWaitAtAGateRunsFromTheGateToYourDecision(t *testing.T) {
	plan := []proto.FactoryEvent{
		act(100, "gate", proto.StationPlan), actedBy("you", act(90, "changes", proto.StationPlan)),
		act(70, "gate", proto.StationPlan), actedBy("you", act(68, "approved", proto.StationPlan)), act(68, "moved", proto.StationImplement),
		act(40, "gate", proto.StationPR), act(5, "moved", proto.StationMerged), actedBy("you", act(5, "merged", proto.StationMerged)),
	}
	failed := []proto.FactoryEvent{act(50, "gate", proto.StationPlan), act(45, "failed", proto.StationPlan), act(30, "gate", proto.StationPR)}
	got := Compute([][]proto.FactoryEvent{plan, failed}, metricsNow)
	if got.WaitPlan != 360 {
		t.Fatalf("plan: got %d, want the median of 600 and 120", got.WaitPlan)
	}
	if got.WaitPR != 2100 {
		t.Fatalf("pr: got %d, want 35 minutes; a wait still open is no sample", got.WaitPR)
	}
}

func TestAnAskWaitsUntilYouAnswerOldestFirst(t *testing.T) {
	log := []proto.FactoryEvent{
		actedBy("worker:t1", act(10, "asked", proto.StationImplement)),
		actedBy("lead", act(9, "asked", proto.StationImplement)),
		actedBy("you", act(6, "allowed", proto.StationImplement)),
		actedBy("reviewer", act(6, "denied", proto.StationPR)),
		actedBy("you", act(5, "denied", proto.StationImplement)),
		actedBy("worker:t2", act(4, "asked", proto.StationImplement)),
	}
	if got := Compute([][]proto.FactoryEvent{log}, metricsNow); got.WaitAsk != 240 {
		t.Fatalf("got %d, want the median of 240 and 240", got.WaitAsk)
	}
}

func TestAutoFixSuccessIsTheShareOfFirstFixesThatRepairedTheTicket(t *testing.T) {
	fixes := func(n int, end string) []proto.FactoryEvent {
		log := []proto.FactoryEvent{act(100, "check", proto.StationChecks)}
		for i := range n {
			log = append(log, act(90-i, "fix", proto.StationChecks))
		}
		switch end {
		case "repaired":
			log = append(log, act(50, "moved", proto.StationPR))
		case "failed":
			log = append(log, act(50, "failed", proto.StationChecks))
		}
		return log
	}
	logs := [][]proto.FactoryEvent{fixes(1, "repaired"), fixes(2, "repaired"), fixes(2, "failed"), fixes(1, "")}
	got := Compute(logs, metricsNow)
	if !nearly(got.FixRate, 2.0/3) || !nearly(got.FixMean, 5.0/3) {
		t.Fatalf("got rate %v and mean %v", got.FixRate, got.FixMean)
	}
}

func TestAHintEndsAFixThatDidNotRepair(t *testing.T) {
	log := []proto.FactoryEvent{act(30, "fix", proto.StationChecks), act(20, "fix", proto.StationChecks), actedBy("you", act(10, "hint", proto.StationChecks)), act(5, "moved", proto.StationPR)}
	if got := Compute([][]proto.FactoryEvent{log}, metricsNow); got.FixRate != 0 || got.FixMean != 2 {
		t.Fatalf("got rate %v and mean %v", got.FixRate, got.FixMean)
	}
}
```

**Step 2: Run the tests to see them fail**

Run: `go test ./internal/factory/ -run 'Fourteen|StartToReady|Wait|Ask|Fix|Hint'`
Expected: FAIL to compile with `undefined: Compute`.

**Step 3: Implement**

Add to `internal/proto/factory.go`, exactly as the index has it:

```go
// Metrics is one project's numbers over the last Days days. Durations are seconds, and 0 means no data.
type Metrics struct {
	MergedPerDay float64 `json:"mergedPerDay"`
	StartToReady int64   `json:"startToReady"` // median
	WaitPlan     int64   `json:"waitPlan"`     // median
	WaitPR       int64   `json:"waitPr"`
	WaitAsk      int64   `json:"waitAsk"`
	FixRate      float64 `json:"fixRate"` // share of failed Tickets repaired within the cap
	FixMean      float64 `json:"fixMean"` // mean attempts
	Days         int     `json:"days"`
}
```

Create `internal/factory/metrics.go`:

```go
package factory

import (
	"slices"
	"time"

	"pocketd/internal/proto"
)

// MetricsDays is how many days the metrics look back.
const MetricsDays = 14

type eventMatch func(proto.FactoryEvent) bool

func isKind(k string) eventMatch { return func(e proto.FactoryEvent) bool { return e.Kind == k } }

func isAt(k, station string) eventMatch {
	return func(e proto.FactoryEvent) bool { return e.Kind == k && e.Station == station }
}

func isBy(actor string, m eventMatch) eventMatch {
	return func(e proto.FactoryEvent) bool { return e.Actor == actor && m(e) }
}

func anyOf(ms ...eventMatch) eventMatch {
	return func(e proto.FactoryEvent) bool { return slices.ContainsFunc(ms, func(m eventMatch) bool { return m(e) }) }
}

// nextOf is the index of the first event from i on that m matches, or -1.
func nextOf(log []proto.FactoryEvent, i int, m eventMatch) int {
	for ; i < len(log); i++ {
		if m(log[i]) {
			return i
		}
	}
	return -1
}

func secondsBetween(a, b proto.FactoryEvent) int64 { return (b.At - a.At) / 1000 }

// Compute is a project's numbers from the activity logs of its Tickets, over the MetricsDays before now.
func Compute(logs [][]proto.FactoryEvent, now time.Time) proto.Metrics {
	since, until := now.AddDate(0, 0, -MetricsDays).UnixMilli(), now.UnixMilli()
	in := func(e proto.FactoryEvent) bool { return e.At >= since && e.At <= until }
	var merged, decided, repaired, tries int
	var ready, plan, pr, ask []int64
	for _, log := range logs {
		if i := nextOf(log, 0, isKind("merged")); i >= 0 && in(log[i]) {
			merged++
		}
		if s := nextOf(log, 0, isKind("moved")); s >= 0 {
			if r := nextOf(log, s+1, isAt("gate", proto.StationPR)); r >= 0 && in(log[r]) {
				ready = append(ready, secondsBetween(log[s], log[r]))
			}
		}
		plan = append(plan, gateWaits(log, in, isAt("gate", proto.StationPlan), anyOf(isAt("approved", proto.StationPlan), isAt("changes", proto.StationPlan)), anyOf(isKind("moved"), isKind("failed")))...)
		pr = append(pr, gateWaits(log, in, isAt("gate", proto.StationPR), anyOf(isKind("merged"), isAt("changes", proto.StationPR), isAt("moved", proto.StationMerged)), anyOf(isKind("moved"), isKind("failed")))...)
		ask = append(ask, askWaits(log, in)...)
		if fixed, n, ok := firstFix(log, in); ok {
			decided, tries = decided+1, tries+n
			if fixed {
				repaired++
			}
		}
	}
	m := proto.Metrics{
		MergedPerDay: float64(merged) / MetricsDays,
		StartToReady: medianOf(ready),
		WaitPlan:     medianOf(plan),
		WaitPR:       medianOf(pr),
		WaitAsk:      medianOf(ask),
		Days:         MetricsDays,
	}
	if decided > 0 {
		m.FixRate, m.FixMean = float64(repaired)/float64(decided), float64(tries)/float64(decided)
	}
	return m
}

// gateWaits is how long each wait lasted, from an event that opens it to one that closes it. An event that drops
// it, or opens another, coming first ends the wait with no sample.
func gateWaits(log []proto.FactoryEvent, in func(proto.FactoryEvent) bool, opens, closes, drops eventMatch) []int64 {
	var out []int64
	for i, e := range log {
		if !opens(e) {
			continue
		}
		if j := nextOf(log, i+1, anyOf(closes, drops, opens)); j >= 0 && closes(log[j]) && in(log[j]) {
			out = append(out, secondsBetween(e, log[j]))
		}
	}
	return out
}

// askWaits pairs each answer of yours with the oldest Ask still open.
func askWaits(log []proto.FactoryEvent, in func(proto.FactoryEvent) bool) []int64 {
	var open []proto.FactoryEvent
	var out []int64
	answer := isBy("you", anyOf(isKind("allowed"), isKind("denied")))
	for _, e := range log {
		switch {
		case e.Kind == "asked":
			open = append(open, e)
		case answer(e) && len(open) > 0:
			if in(e) {
				out = append(out, secondsBetween(open[0], e))
			}
			open = open[1:]
		}
	}
	return out
}

// firstFix is how the first auto-fix of a Ticket ended: whether it repaired the Ticket, and after how many
// attempts. ok is false while it is under way, or when it ended before the window.
func firstFix(log []proto.FactoryEvent, in func(proto.FactoryEvent) bool) (repaired bool, attempts int, ok bool) {
	i := nextOf(log, 0, isAt("fix", proto.StationChecks))
	if i < 0 {
		return false, 0, false
	}
	attempts = 1
	for _, e := range log[i+1:] {
		switch {
		case e.Kind == "fix":
			attempts++
		case e.Kind == "moved" && e.Station == proto.StationPR:
			return true, attempts, in(e)
		case e.Kind == "failed", e.Kind == "hint":
			return false, attempts, in(e)
		}
	}
	return false, 0, false
}

func medianOf(samples []int64) int64 {
	if len(samples) == 0 {
		return 0
	}
	s := slices.Sorted(slices.Values(samples))
	mid := len(s) / 2
	if len(s)%2 == 1 {
		return s[mid]
	}
	return (s[mid-1] + s[mid]) / 2
}
```

Add to `engine.go`, beside `Detail`:

```go
// Metrics is project's numbers over the last MetricsDays, from the activity of its Tickets. A log that can't be
// read counts as empty.
func (e *Engine) Metrics(project string) proto.Metrics {
	tickets, _ := e.Board.Snapshot()
	var logs [][]proto.FactoryEvent
	for _, t := range tickets {
		if t.Folder != project {
			continue
		}
		if log, err := ReadActivity(e.Home, t.ID); err == nil && len(log) > 0 {
			logs = append(logs, log)
		}
	}
	return Compute(logs, e.now())
}
```

`e.now()` is plan 3's reader of `Engine.Now`. The Board snapshot holds the closed Tickets that are still listed, so a Ticket merged and closed within the window counts.

**Step 4: Run the tests to see them pass**

Run: `go test ./internal/factory/`
Expected: `ok  	pocketd/internal/factory`.

### Task 3.2: The verb `factory.metrics` and its reply

**Files:**
- Modify: `packages/pocketd/internal/proto/messages.go` (`DecodeClient` at line 176), `internal/proto/factory.go`, `internal/proto/factory_test.go` (`factoryServerGolden`)
- Create: `packages/pocketd/internal/proto/testdata/factory/client/factory_metrics.json`
- Create (generated): `packages/pocketd/internal/proto/testdata/factory/server/factory_metrics.json`
- Modify: `packages/pocketd/internal/wsserver/wsserver.go` (`dispatch`, the Factory cases), `internal/wsserver/factory.go`
- Test: `internal/proto/factory_test.go`, `internal/factory/metrics_test.go`

**Context:** Plan 1 already gives `ws:factory.metrics` the scope Own in `peer.Needs`, with its row in `TestEveryVerbAndPrincipalMatchesTheMatrix`. So this PR changes no scope, and the red-team gate does not need to run. Plan 1 keeps Factory goldens under `testdata/factory/`. `TestClientGolden` decodes every file in `testdata/factory/client/`, and `TestServerGolden -update` writes `testdata/factory/server/` from the map `factoryServerGolden`. Plan 1's `factoryVerb` answers `factory_off` when the conn did not negotiate `factory.v1`. Reading every Ticket's log takes time, so the handler answers off the read loop, as plan 1's `factoryNext` does.

**Step 1: Write the failing tests**

Create `testdata/factory/client/factory_metrics.json` (one line, then a newline):

```json
{"type":"factory.metrics","id":"factory-9","project":"/app"}
```

Add to `factoryServerGolden` in `internal/proto/factory_test.go`:

```go
	"factory_metrics": NewFactoryMetrics("factory-9", "/app", Metrics{MergedPerDay: 2.4, StartToReady: 11400, WaitPlan: 720, WaitPR: 3900, WaitAsk: 240, FixRate: 0.78, FixMean: 1.3, Days: 14}),
```

Add to `internal/proto/factory_test.go`:

```go
func TestFactoryMetricsAsksForOneProject(t *testing.T) {
	m, err := DecodeClient(factoryGolden(t, "client", "factory_metrics.json"))
	if err != nil || m.Type != "factory.metrics" || m.ID != "factory-9" || m.Project != "/app" {
		t.Fatalf("got %+v, %v", m, err)
	}
	if _, err := DecodeClient([]byte(`{"type":"factory.metrics","id":"1","project":""}`)); err == nil {
		t.Fatal("a request without a project decoded")
	}
}

func TestAProjectWithNoDataSendsZeroesNotNothing(t *testing.T) {
	got, _ := json.Marshal(NewFactoryMetrics("1", "/app", Metrics{Days: 14}))
	want := `{"type":"factory.metrics","id":"1","project":"/app","metrics":{"mergedPerDay":0,"startToReady":0,"waitPlan":0,"waitPr":0,"waitAsk":0,"fixRate":0,"fixMean":0,"days":14}}`
	if string(got) != want {
		t.Fatalf("got  %s\nwant %s", got, want)
	}
}
```

Add to `internal/factory/metrics_test.go` (new imports `pocketd/internal/board`):

```go
func TestMetricsReadTheLogsOfTheProjectsTicketsOnly(t *testing.T) {
	home := t.TempDir()
	e := &Engine{Home: home, Board: board.Open(home), Now: func() time.Time { return metricsNow }}
	mine, err := e.Board.Save(proto.Ticket{Title: "Mine", Prompt: "p", Provider: "claude", Folder: "/app"}, false, metricsNow)
	if err != nil {
		t.Fatal(err)
	}
	other, _ := e.Board.Save(proto.Ticket{Title: "Other", Prompt: "p", Provider: "claude", Folder: "/other"}, false, metricsNow)
	for _, id := range []string{mine.ID, other.ID} {
		if err := appendEvent(home, id, act(60, "merged", proto.StationMerged)); err != nil {
			t.Fatal(err)
		}
	}
	if got := e.Metrics("/app"); !nearly(got.MergedPerDay, 1.0/14) || got.Days != 14 {
		t.Fatalf("got %+v", got)
	}
}
```

**Step 2: Run the tests to see them fail**

Run from `packages/pocketd`: `go test ./internal/proto/ ./internal/factory/`
Expected: FAIL to compile with `undefined: NewFactoryMetrics`. If the proto package compiles first, `TestClientGolden` fails with `factory_metrics.json: unknown message type`.

**Step 3: Implement**

Add to `internal/proto/factory.go`:

```go
// FactoryMetrics answers factory.metrics.
type FactoryMetrics struct {
	Type    string  `json:"type"`
	ID      string  `json:"id"`
	Project string  `json:"project"`
	Metrics Metrics `json:"metrics"`
}

func NewFactoryMetrics(id, project string, m Metrics) FactoryMetrics {
	return FactoryMetrics{"factory.metrics", id, project, m}
}
```

In `DecodeClient` (`messages.go`), add a case beside the `factory.config.get` case that plan 1 added:

```go
	case m.Type == "factory.metrics":
		ok = get("project", &m.Project) && m.Project != ""
```

In `dispatch` (`wsserver.go`), add beside the Factory cases:

```go
	case "factory.metrics":
		return c.factoryMetrics(m)
```

Add to `internal/wsserver/factory.go`:

```go
// factoryMetrics answers with a project's numbers. Reading every Ticket's log takes time, so it answers off the read loop.
func (c *conn) factoryMetrics(m proto.ClientMessage) error {
	if c.s.Factory == nil || !slices.Contains(c.caps, proto.CapFactory) {
		c.send(proto.NewErrorCode(m.ID, proto.CodeFactoryOff, "The Factory is off"))
		return nil
	}
	if err := c.usableFolder(m.Project); err != nil {
		c.send(factoryError(m.ID, err))
		return nil
	}
	go c.send(proto.NewFactoryMetrics(m.ID, m.Project, c.s.Factory.Metrics(m.Project)))
	return nil
}
```

**Step 4: Write the server golden, then run the tests to see them pass**

```sh
go test ./internal/proto/ -run TestServerGolden -update
go test ./internal/proto/ ./internal/factory/ ./internal/wsserver/ ./internal/peer/
git status --short internal/proto/testdata
```

Expected: `ok` for the four packages. `git status` lists the two new files under `testdata/factory/` and nothing under `testdata/golden/`. `testdata/factory/server/factory_metrics.json` holds `"waitPr": 3900` and `"days": 14`.

### Task 3.3: The desktop asks for the metrics and keeps them

**Files:**
- Modify: `packages/desktop/crates/agents/src/factory.rs`, `agents/src/agents.rs` (`Frame`, `Event`, `Agents`, `apply_from`, `factory_event`, `impl Outbox`, `tests`)
- Modify: `packages/desktop/crates/hosts/src/session.rs:152-161` (`tag_event`)

**Context:** The index names `agents::factory::Metrics`, `Event::FactoryMetrics { project, metrics }` and `Outbox::factory_metrics(project)` for this plan. It does not say where the desktop keeps them. This plan keeps them in `Agents` by project, as plan 1 keeps `factory` configs (see "Contract changes needed"). The goldens are the two files of Task 3.2.

**Step 1: Write the failing tests**

Add to the `tests` module of `agents/src/agents.rs`:

```rust
    #[test]
    fn the_metrics_reply_is_kept_by_project() {
        let reply = include_str!("../../../../pocketd/internal/proto/testdata/factory/server/factory_metrics.json");
        let Some(Event::FactoryMetrics { project, metrics }) = factory_event(&serde_json::from_str::<Frame>(reply).unwrap()) else { panic!("no metrics") };
        assert_eq!((project.as_str(), metrics.wait_pr, metrics.start_to_ready, metrics.days), ("/app", 3900, 11400, 14));
        assert!((metrics.merged_per_day - 2.4).abs() < 1e-9 && (metrics.fix_rate - 0.78).abs() < 1e-9);
        let mut a = Agents::default();
        assert!(a.metrics("/app").is_none());
        a.apply(Event::FactoryMetrics { project, metrics: metrics.clone() });
        assert_eq!(a.metrics("/app"), Some(&metrics));
    }

    #[test]
    fn asking_for_metrics_sends_the_golden_frame() {
        assert_eq!(sent_but_id(|o| o.factory_metrics("/app")), without_id(factory_golden!("client/factory_metrics.json")));
    }
```

Add to the tests of `hosts/src/session.rs`, beside `a_hosts_factory_news_is_dropped_as_its_board_is` (plan 1):

```rust
    #[test]
    fn a_hosts_metrics_are_dropped() {
        assert!(!tag_event("devbox", &mut Event::FactoryMetrics { project: "/home/me/app".into(), metrics: Default::default() }));
    }
```

**Step 2: Run the tests to see them fail**

Run from `packages/desktop`: `cargo test -p agents metrics && cargo test -p hosts metrics`
Expected: FAIL to compile with `no variant named FactoryMetrics`.

**Step 3: Implement**

Add to `agents/src/factory.rs`:

```rust
/// A project's numbers over the last `days` days, from pocketd. Durations are seconds, and 0 means no data.
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Metrics {
    pub merged_per_day: f64,
    pub start_to_ready: i64,
    pub wait_plan: i64,
    pub wait_pr: i64,
    pub wait_ask: i64,
    pub fix_rate: f64,
    pub fix_mean: f64,
    pub days: u32,
}
```

In `agents/src/agents.rs`:

- `Frame` gains `pub metrics: Option<factory::Metrics>,`.
- `Event` gains, beside `FactoryProfiles`:

```rust
    /// A project's numbers, after `factory.metrics`.
    FactoryMetrics { project: String, metrics: factory::Metrics },
```

- `factory_event` gains an arm before its `"error"` arm:

```rust
        "factory.metrics" => f.metrics.clone().map(|metrics| Event::FactoryMetrics { project: f.project.clone(), metrics }),
```

- `Agents` gains a field and a reader beside `factory`:

```rust
    /// Each project's numbers by its folder, from `factory.metrics`.
    pub metrics: HashMap<String, factory::Metrics>,
```

```rust
    pub fn metrics(&self, project: &str) -> Option<&factory::Metrics> {
        self.metrics.get(project)
    }
```

- `apply_from` gains an arm beside `Event::FactoryConfig`:

```rust
            Event::FactoryMetrics { project, metrics } => {
                self.metrics.insert(project, metrics);
            }
```

- `impl Outbox` gains, after `profile_delete`:

```rust
    /// Asks for `project`'s numbers; `Event::FactoryMetrics` answers.
    pub fn factory_metrics(&self, project: &str) {
        self.send(json!({"type": "factory.metrics", "id": factory_id(), "project": project}));
    }
```

In `hosts/src/session.rs`, add `| Event::FactoryMetrics { .. }` to the group that ends in `=> return false`.

**Step 4: Run the tests to see them pass**

Run: `cargo test -p agents && cargo test -p hosts`
Expected: PASS.

### Task 3.4: The strip at the top of Settings › Factory, capture, and the checks

**Files:**
- Create: `packages/desktop/crates/pocket/src/factory/metrics.rs`
- Modify: `packages/desktop/crates/pocket/src/factory.rs` (the `mod` lines)
- Modify: `packages/desktop/crates/pocket/src/settings.rs` (`load_section` at line 467, the body `match` of `settings_page` at line 785)
- Modify: `packages/desktop/crates/pocket/src/settings/factory.rs` (the project picker's change handler)
- Modify: `packages/desktop/crates/pocket/src/capture.rs` (`STEPS`, a step after `settings-factory-profiles`, `reset`)
- Test: the `tests` module at the bottom of `factory/metrics.rs`

**Context:** Plan 3 gives Settings › Factory a project picker (index, section "Desktop"). This task calls the picked project `self.factory_project()`, an `Option<String>`. If plan 3 named it differently, use its name. The strip shows four tiles: merged per day, start to PR ready, the waits at Plan, PR and Ask, and the auto-fix success with its mean attempts (prototype `METRICS`). A number of 0 is no data and reads as a dash. Under the strip, the line `Last 14 days · computed on this Mac` says the numbers never leave the Mac. The text logic is plain and tested. The view is a free function over `Option<&Metrics>`.

**Step 1: Write the failing tests**

In `factory.rs`, add `pub(crate) mod metrics;`. Create `factory/metrics.rs` with only the tests:

```rust
#[cfg(test)]
mod tests {
    use super::{span, tiles};
    use agents::factory::Metrics;

    #[test]
    fn a_span_reads_in_its_largest_two_units_and_none_reads_as_a_dash() {
        assert_eq!([span(0), span(45), span(720), span(3600), span(3900), span(11_400), span(187_200)], ["–", "45s", "12m", "1h", "1h 5m", "3h 10m", "2d 4h"]);
    }

    #[test]
    fn the_four_tiles_read_as_the_design_has_them() {
        let m = Metrics { merged_per_day: 2.4, start_to_ready: 11_400, wait_plan: 720, wait_pr: 3900, wait_ask: 240, fix_rate: 0.78, fix_mean: 1.3, days: 14 };
        let values: Vec<String> = tiles(Some(&m)).into_iter().map(|(v, _)| v).collect();
        assert_eq!(values, ["2.4", "3h 10m", "12m · 1h 5m · 4m", "78% · 1.3"]);
        let labels: Vec<&str> = tiles(Some(&m)).into_iter().map(|(_, l)| l).collect();
        assert_eq!(labels, ["Merged per day", "Start → PR ready", "Wait at Plan · PR · Ask", "Auto-fix success · tries"]);
    }

    #[test]
    fn a_project_with_no_data_reads_as_dashes() {
        let none: Vec<String> = tiles(None).into_iter().map(|(v, _)| v).collect();
        assert_eq!(none, ["–", "–", "– · – · –", "–"]);
        let empty: Vec<String> = tiles(Some(&Metrics { days: 14, ..Default::default() })).into_iter().map(|(v, _)| v).collect();
        assert_eq!(empty, none);
        let never_failed = Metrics { merged_per_day: 1., wait_plan: 60, days: 14, ..Default::default() };
        assert_eq!(tiles(Some(&never_failed))[3].0, "–", "no fix ended, so no share");
    }
}
```

**Step 2: Run the tests to see them fail**

Run from `packages/desktop`: `cargo test -p pocket factory::metrics`
Expected: FAIL to compile with `unresolved imports super::span, super::tiles`.

**Step 3: Implement**

Put this above the `tests` module of `factory/metrics.rs`:

```rust
use agents::factory::Metrics;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

const DASH: &str = "–";

/// A duration in its largest two units: `45s`, `12m`, `1h 5m`, `2d 4h`.
pub fn span(secs: i64) -> String {
    let (d, h, m) = (secs / 86_400, secs % 86_400 / 3600, secs % 3600 / 60);
    match secs {
        ..=0 => DASH.into(),
        1..60 => format!("{secs}s"),
        60..3600 => format!("{m}m"),
        3600..86_400 if m == 0 => format!("{h}h"),
        3600..86_400 => format!("{h}h {m}m"),
        _ if h == 0 => format!("{d}d"),
        _ => format!("{d}d {h}h"),
    }
}

/// Each tile's value and label.
pub fn tiles(m: Option<&Metrics>) -> [(String, &'static str); 4] {
    let m = m.cloned().unwrap_or_default();
    let merged = if m.merged_per_day > 0. { format!("{:.1}", m.merged_per_day) } else { DASH.into() };
    let fixed = if m.fix_mean > 0. { format!("{:.0}% · {:.1}", m.fix_rate * 100., m.fix_mean) } else { DASH.into() };
    [
        (merged, "Merged per day"),
        (span(m.start_to_ready), "Start → PR ready"),
        (format!("{} · {} · {}", span(m.wait_plan), span(m.wait_pr), span(m.wait_ask)), "Wait at Plan · PR · Ask"),
        (fixed, "Auto-fix success · tries"),
    ]
}

/// The four tiles and the line under them.
pub fn metrics_strip(m: Option<&Metrics>) -> Div {
    let cells = tiles(m).into_iter().enumerate().map(|(i, (value, label))| {
        div()
            .flex_1()
            .min_w_0()
            .px(px(12.))
            .py(px(10.))
            .flex()
            .flex_col()
            .gap(px(3.))
            .when(i > 0, |d| d.border_l(px(0.5)).border_color(SEPARATOR))
            .child(div().truncate().text_size(px(15.)).line_height(px(19.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(value))
            .child(div().truncate().text_size(px(11.5)).text_color(TEXT_3).child(label))
    });
    let strip = div().flex().rounded(px(9.)).bg(SURFACE).shadow(vec![ui::ring(SEPARATOR, 0.5), ui::row_shadow()]).overflow_hidden().children(cells);
    div().flex().flex_col().gap(px(6.)).child(strip).child(div().ml(px(2.)).text_size(px(11.5)).text_color(TEXT_4).child("Last 14 days · computed on this Mac"))
}
```

If `ui::row_shadow` takes arguments in this tree, use the shadow that `settings::card` uses: `ui::shadow(rgba(0x00000009), 1., 2.)`.

In `settings_page`, draw the strip first in the Factory body (`settings.rs:785-792`):

```rust
// before
                Section::Provider(i) => self.provider_page(i, cx),
                _ => div().flex().flex_col().gap(px(22.)).children(self.setting_groups(section, cx)),
// after
                Section::Provider(i) => self.provider_page(i, cx),
                Section::Factory => {
                    let metrics = self.factory_project().and_then(|p| self.agents.metrics(&p).cloned());
                    div().flex().flex_col().gap(px(22.)).child(crate::factory::metrics::metrics_strip(metrics.as_ref())).children(self.setting_groups(section, cx))
                }
                _ => div().flex().flex_col().gap(px(22.)).children(self.setting_groups(section, cx)),
```

If plan 3 already gave `Section::Factory` its own arm, put the strip first inside that arm.

In `load_section`, ask for the numbers when the section opens. Extend the arm that Task 2.5 added:

```rust
// before
            Section::Factory => self.outbox.local.profile_list(),
// after
            Section::Factory => {
                self.outbox.local.profile_list();
                if let Some(project) = self.factory_project() {
                    self.outbox.local.factory_metrics(&project);
                }
            }
```

In `settings/factory.rs`, where plan 3's project picker stores a new pick, ask for that project's numbers too:

```rust
    d.outbox.local.factory_metrics(&project);
```

The numbers are read when the page opens or the project changes. They are not polled. A page left open shows numbers up to a few minutes old, which a 14-day window does not notice.

**Step 4: Run the tests to see them pass**

Run: `cargo test -p pocket factory::metrics && cargo build -p pocket`
Expected: PASS.

**Step 5: Capture**

Bump the length of `STEPS` by one. Add after `settings-factory-profiles`:

```rust
    ("factory-metrics", |d, window, cx| {
        let Some(project) = d.factory_project() else { return };
        let metrics = agents::factory::Metrics { merged_per_day: 2.4, start_to_ready: 11_400, wait_plan: 720, wait_pr: 3900, wait_ask: 240, fix_rate: 0.78, fix_mean: 1.3, days: 14 };
        d.on_agents(agents::Event::FactoryMetrics { project, metrics }, window, cx);
    }),
```

In `reset`, add `d.agents.metrics.clear();`.

From the repo root:

```sh
.ui-review/fixture/capture.sh /tmp/lf7-after settings-factory=settings,settings-factory settings-factory-metrics=settings,settings-factory,factory-metrics
```

Expected:
- `impl-settings-factory.png` shows the strip at the top with four dashes and the line `Last 14 days · computed on this Mac`. Below it, the page is as in `/tmp/lf7-before/impl-settings-factory.png`.
- `impl-settings-factory-metrics.png` shows `2.4`, `3h 10m`, `12m · 1h 5m · 4m` and `78% · 1.3` over their labels.

**Step 6: Run the checks**

From `packages/pocketd`: `go vet ./... && go test ./...`
From `packages/desktop`: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: PASS, with no new warning. The TypeScript and Swift golden suites read only `testdata/golden/`, so the new goldens add no failure there.

---

## PR 4: Station sections in the List layout, the `anywhere` CLI, and the docs

**PR title:** `The List layout groups Factory tickets by station, and anywhere shows where a ticket stands`. Label: `type: feature`.

**Scope:** B11 and US-07 (the List layout), section 5.11 and US-37 (the CLI), the README and a short page on the Factory.

**Depends on:** plans 1 and 2 merged (`Ticket.station`, `factory::station::{Column, COLUMNS, column, rank}`, `station_glyph`). It does not depend on PR 1, 2 or 3.

**Done when:** the `board::logic` and `ticket` tests pass, `board-list-stations` shows station sections, and `board-list` without Factory tickets looks as before.

**What stays inert:** nothing.

### Task 4.0: Check plan 2's names and take the "before" pictures

**Files:**
- Modify (only where a check fails): `packages/desktop/crates/pocket/src/factory/station.rs`, `board/parts.rs`

**Context:** Plan 2 adds `factory/station.rs` with `Column`, `COLUMNS`, `Stations`, `arrange`, `column`, `rank` and `on_line`, the state `FactoryState { columns, lists, open }` with `Desktop::regroup_stations`, and `parts::station_glyph(column, gate, failed)`. Plan 2 regroups the stations only in the Factory layout (its `board_changed`). Its `arrange` already drops a Ticket closed without a merge, keeps the Merged column to its latest 50, and orders the rest by `rank`.

**Step 1: Take the pictures**

From the repo root:

```sh
.ui-review/fixture/capture.sh /tmp/lf7-before board-list=board,board-list board-factory=board,board-factory
```

Expected: `impl-board-list.png` and `impl-board-factory.png` in `/tmp/lf7-before`.

**Step 2: Check the names**

From `packages/desktop`:

```sh
grep -n "pub(crate) type Stations\|pub(crate) fn arrange\|pub(crate) fn column\|pub(crate) fn on_line\|pub(crate) fn failed\|pub(crate) fn gate" crates/pocket/src/factory/station.rs
grep -n "pub(crate) fn station_glyph" crates/pocket/src/board/parts.rs
grep -n "fn regroup_stations\|pub(crate) columns: Stations" crates/pocket/src/factory.rs
grep -n "Clone, Copy, PartialEq, Eq, Debug" -A 2 crates/pocket/src/factory/station.rs | head -3
```

Expected: every name has a match, and `Column` derives `Clone, Copy, PartialEq, Eq, Debug`. If a name differs, use plan 2's name in this PR's code.

### Task 4.1: `station_rows`: the List's sections follow the stations

**Files:**
- Modify: `packages/desktop/crates/pocket/src/board/logic.rs` (`Row` at line 515, a new `station_rows` after `rows` at line 522, `stuck_head` at line 437, the imports at line 1, `on_page` at line 192)
- Test: the `tests` module of `board/logic.rs` (`a_section_head_sticks_once_it_reaches_the_top_of_the_list` at line 911, new tests)

**Context:** The List draws `BoardState.rows`, a `Vec<Row>` of section heads and ticket rows, through one GPUI `list` (`board/row.rs:117`). A ticket row is a `Slot { col, pos, index }` whose `col` is the ticket's status column. `shown_ticket` checks that column (`board.rs:375`), and the row's props, menus and quick actions read it. So a station section keeps the status column in each `Slot`, and only the heads change. `pos` is the ticket's place in its status column: it gives a queued ticket its `Next` or `2nd in line` note. The tab still filters by status column (`on_page`), as it does in the status sections.

**Step 1: Write the failing tests**

Add to the `tests` module of `board/logic.rs`:

```rust
    const OPEN: [bool; COLUMNS.len()] = [false; COLUMNS.len()];

    fn at(id: &str, station: Station, state: State, status: Status) -> Ticket {
        Ticket { station, ..ticket(id, state, status) }
    }

    fn stations(lines: &[(Column, &[usize])]) -> Stations {
        let mut out: Stations = Default::default();
        for (column, line) in lines {
            out[*column as usize] = line.to_vec();
        }
        out
    }

    #[test]
    fn the_list_groups_factory_tickets_by_station_and_keeps_each_status_column() {
        let merged = Ticket { closed_why: ClosedWhy::Merged, ..at("old", Station::Merged, State::Closed, Status::Idle) };
        let tickets = [at("todo", Station::None, State::Queued, Status::None), at("plan", Station::Plan, State::Live, Status::NeedsYou), at("pr", Station::Pr, State::Live, Status::Done), merged];
        let columns = arrange(&tickets, &Filters::default(), Order::Activity);
        let lines = stations(&[(Column::Todo, &[0]), (Column::Plan, &[1]), (Column::Pr, &[2]), (Column::Merged, &[3])]);
        let got = station_rows(&lines, &tickets, &columns, Tab::All, false, &OPEN);
        let want = vec![
            Row::Station(Column::Todo, 1),
            Row::Ticket(Slot { col: Col::Queued, pos: 0, index: 0 }),
            Row::Station(Column::Plan, 1),
            Row::Ticket(Slot { col: Col::NeedsYou, pos: 0, index: 1 }),
            Row::Station(Column::Pr, 1),
            Row::Ticket(Slot { col: Col::Done, pos: 0, index: 2 }),
            Row::Station(Column::Merged, 1),
            Row::Ticket(Slot { col: Col::Merged, pos: 0, index: 3 }),
        ];
        assert_eq!(got, want);
    }

    #[test]
    fn a_folded_station_keeps_its_bar_and_the_tab_filters_by_status() {
        let tickets = [at("todo", Station::None, State::Todo, Status::None), at("plan", Station::Plan, State::Live, Status::NeedsYou)];
        let columns = arrange(&tickets, &Filters::default(), Order::Activity);
        let lines = stations(&[(Column::Todo, &[0]), (Column::Plan, &[1])]);
        let mut folded = OPEN;
        folded[Column::Todo as usize] = true;
        assert_eq!(station_rows(&lines, &tickets, &columns, Tab::All, false, &folded), vec![Row::Station(Column::Todo, 1), Row::Station(Column::Plan, 1), Row::Ticket(Slot { col: Col::NeedsYou, pos: 0, index: 1 })]);
        assert_eq!(station_rows(&lines, &tickets, &columns, Tab::Todo, false, &OPEN), vec![Row::Station(Column::Todo, 1), Row::Ticket(Slot { col: Col::Todo, pos: 0, index: 0 })]);
    }

    #[test]
    fn a_queued_ticket_keeps_its_place_in_line_in_a_station_section() {
        let tickets = [Ticket { order: 2, ..at("second", Station::None, State::Queued, Status::None) }, Ticket { order: 1, ..at("first", Station::None, State::Queued, Status::None) }];
        let columns = arrange(&tickets, &Filters::default(), Order::Activity);
        let got = station_rows(&stations(&[(Column::Todo, &[0, 1])]), &tickets, &columns, Tab::All, false, &OPEN);
        assert_eq!(got[1], Row::Ticket(Slot { col: Col::Queued, pos: 1, index: 0 }));
        assert_eq!(got[2], Row::Ticket(Slot { col: Col::Queued, pos: 0, index: 1 }));
    }

    #[test]
    fn the_list_groups_by_station_only_when_a_ticket_has_one() {
        let plain = [ticket("a", State::Todo, Status::None)];
        assert!(!by_station(&plain));
        assert!(by_station(&[plain[0].clone(), at("b", Station::Checks, State::Live, Status::Working)]));
    }
```

`use super::*` already brings `Column`, `COLUMNS`, `Stations` and `Station` into the tests once Step 3 imports them.

Replace the body of `a_section_head_sticks_once_it_reaches_the_top_of_the_list` (lines 912 to 918). `stuck_head` now returns the head row, so a station head can stick too:

```rust
    #[test]
    fn a_section_head_sticks_once_it_reaches_the_top_of_the_list() {
        let slot = Slot { col: Col::Todo, pos: 0, index: 0 };
        let rows = [Row::Head(Col::Todo), Row::Ticket(slot), Row::Head(Col::Done), Row::Ticket(slot)];
        assert_eq!(stuck_head(&rows, 0, false), None);
        assert_eq!(stuck_head(&rows, 0, true), Some(Row::Head(Col::Todo)));
        assert_eq!(stuck_head(&rows, 1, false), Some(Row::Head(Col::Todo)));
        assert_eq!(stuck_head(&rows, 2, false), None);
        assert_eq!(stuck_head(&rows, 3, false), Some(Row::Head(Col::Done)));
        let rows = [Row::Station(Column::Plan, 1), Row::Ticket(slot)];
        assert_eq!(stuck_head(&rows, 1, false), Some(Row::Station(Column::Plan, 1)));
    }
```

**Step 2: Run the tests to see them fail**

Run from `packages/desktop`: `cargo test -p pocket board::logic`
Expected: FAIL to compile with `no variant named Station found for enum Row` and `cannot find function station_rows`.

**Step 3: Implement**

`board/logic.rs:1-3` gains the imports:

```rust
use crate::factory::station::{COLUMNS, Column, Stations};
use agents::tickets::{ClosedWhy, State, Station, Status, Ticket};
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
```

`Row` (line 515):

```rust
// before
pub(crate) enum Row {
    Head(Col),
    Ticket(Slot),
}
// after
pub(crate) enum Row {
    Head(Col),
    /// A station's section and how many tickets it holds, when the List groups by station.
    Station(Column, usize),
    Ticket(Slot),
}
```

`stuck_head` (lines 436 to 442):

```rust
/// The section head the list's top row is under, once that head is off the top.
pub(crate) fn stuck_head(rows: &[Row], top: usize, past_start: bool) -> Option<Row> {
    let head = |r: &Row| matches!(r, Row::Head(_) | Row::Station(..));
    if !past_start && rows.get(top).is_some_and(head) {
        return None;
    }
    rows.iter().take(top + 1).rev().find(|r| head(r)).copied()
}
```

Add after `rows` (line 535):

```rust
/// Whether the List groups by station: the Board holds a ticket the Factory moves.
pub(crate) fn by_station(tickets: &[Ticket]) -> bool {
    tickets.iter().any(|t| t.station != Station::None)
}

/// The List's sections by station, in the order `stations` holds them. An empty one is left out.
/// Each row keeps the ticket's status column and its place there, so it acts as in the status sections.
pub(crate) fn station_rows(stations: &Stations, tickets: &[Ticket], columns: &Columns, tab: Tab, show_closed: bool, folded: &[bool; COLUMNS.len()]) -> Vec<Row> {
    let place: HashMap<usize, usize> = columns.iter().flat_map(|line| line.iter().enumerate().map(|(pos, &index)| (index, pos))).collect();
    let mut out = Vec::new();
    for (station, line) in COLUMNS.iter().zip(stations) {
        let slots: Vec<Slot> = line
            .iter()
            .filter_map(|&index| Some(Slot { col: column(tickets.get(index)?), pos: place.get(&index).copied().unwrap_or(0), index }))
            .filter(|s| on_page(s.col, tab, show_closed))
            .collect();
        if slots.is_empty() {
            continue;
        }
        out.push(Row::Station(*station, slots.len()));
        if !folded[*station as usize] {
            out.extend(slots.into_iter().map(Row::Ticket));
        }
    }
    out
}
```

**Step 4: Run the tests to see them pass**

Run: `cargo test -p pocket board::`
Expected: PASS. The `board::selection` tests pass unchanged, because `walk` keeps only `Row::Ticket`. The build fails in `board/row.rs` until Task 4.2 adds the new arm. If you run Task 4.2 in the same commit, run this step after it.

### Task 4.2: Draw the station sections

**Files:**
- Modify: `packages/desktop/crates/pocket/src/board.rs` (`BoardState` fields and `new`, `regroup` at line 134, `board_changed` at line 198)
- Modify: `packages/desktop/crates/pocket/src/board/row.rs` (`board_list` at line 117, new `station_bar`, `station_head`, `stuck_station`)

**Context:** `BoardState::regroup` builds `rows` from the status columns (`board.rs:145`). It now takes the station columns when the Board holds a Factory ticket. Plan 2's `board_changed` calls `regroup_stations` only in the Factory layout. It now also calls it when `by_station` holds, so the List has station columns to draw. The rows are built whatever the layout, as today, so a switch to the List needs no regroup. A station bar is the status bar's shape (`section_bar`, `row.rs:11`): 32px high, radius 7, `FILL_1`, a fold chevron, the glyph, the name and the count, and `+` on To do. B11 gives exactly that.

**Step 1: Keep the station folds and build the rows**

`BoardState` gains a field beside `folded`, set to `Default::default()` in `new`:

```rust
    /// The List's station sections folded away, per column of `factory::station::COLUMNS`.
    pub(crate) folded_stations: [bool; crate::factory::station::COLUMNS.len()],
```

`regroup` (`board.rs:134` and `:145`):

```rust
// before
    fn regroup(&mut self, board: &Board) {
// after
    /// `stations` is set when the List groups by station.
    fn regroup(&mut self, board: &Board, stations: Option<&Stations>) {
```

```rust
// before
        self.rows = rows(&self.columns, self.tab, self.show_closed, &self.folded);
// after
        self.rows = match stations {
            Some(lines) => station_rows(lines, &board.tickets, &self.columns, self.tab, self.show_closed, &self.folded_stations),
            None => rows(&self.columns, self.tab, self.show_closed, &self.folded),
        };
```

Add `by_station` and `station_rows` to the `use logic::{…}` line of `board.rs`, and `use crate::factory::station::Stations;`.

`board_changed`, as plan 2 left it:

```rust
// before
        if self.screen == Screen::Board {
            self.board.regroup(&self.agents.board);
            if self.board.layout == Layout::Factory {
                self.regroup_stations();
            }
// after
        if self.screen == Screen::Board {
            let grouped = by_station(&self.agents.board.tickets);
            if grouped || self.board.layout == Layout::Factory {
                self.regroup_stations();
            }
            self.board.regroup(&self.agents.board, grouped.then_some(&self.factory.columns));
```

`regroup_stations` reads only the filters, so it may run first. The Board's filters and Factory's columns are two fields, so the borrow is fine.

**Step 2: Draw the bars**

In `board/row.rs`, add `use crate::board::parts::station_glyph;`, `use crate::factory::station::Column;` and `use agents::tickets::Gate;`. Add after `stuck_section`:

```rust
    fn station_bar(&self, column: Column, n: usize, cx: &mut Context<Self>) -> Div {
        let add = (column == Column::Todo).then(|| self.add_ticket_button("board-list-add", cx));
        let folded = self.board.folded_stations[column as usize];
        let fold = ui::icon_button_sized(("station-fold", column as usize), if folded { "chevron-right" } else { "chevron-down" }, 18., TEXT_4).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.board.folded_stations[column as usize] = !folded;
            this.board_changed();
            cx.notify();
        }));
        let heading = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.5))
            .font_weight(FontWeight::MEDIUM)
            .child(station_glyph(column, Gate::None, false))
            .child(column.name())
            .child(div().font_weight(FontWeight::NORMAL).text_color(TEXT_3).child(n.to_string()));
        div().h(px(32.)).pl(px(4.)).pr(px(6.)).flex().items_center().gap(px(8.)).rounded(px(7.)).bg(FILL_1).child(fold).child(heading).child(div().flex_1()).children(add)
    }

    fn station_head(&self, column: Column, n: usize, first: bool, cx: &mut Context<Self>) -> AnyElement {
        div().w_full().px(px(8.)).when(!first, |d| d.pt(px(6.))).child(self.station_bar(column, n, cx)).into_any_element()
    }
```

Replace `board_list` (lines 117 to 127):

```rust
    pub(super) fn board_list(&mut self, cx: &mut Context<Self>) -> Div {
        let rows = list(self.board.rows_list.clone(), cx.processor(|this, ix, _, cx| match this.board.rows.get(ix).copied() {
            Some(Row::Head(col)) => this.section_head(col, ix == 0, cx),
            Some(Row::Station(column, n)) => this.station_head(column, n, ix == 0, cx),
            Some(Row::Ticket(slot)) => this.ticket_row(slot, cx),
            None => div().into_any_element(),
        }))
        .size_full();
        let top = self.board.rows_list.logical_scroll_top();
        let stuck = stuck_head(&self.board.rows, top.item_ix, top.offset_in_item > px(0.)).map(|head| {
            let bar = match head {
                Row::Station(column, n) => self.station_bar(column, n, cx),
                Row::Head(col) => self.section_bar(col, cx),
                Row::Ticket(_) => div(),
            };
            self.stuck(bar)
        });
        div().relative().flex_1().min_h_0().child(rows).children(stuck)
    }
```

Change `stuck_section` (`row.rs:27-30`) to take the bar, so a station bar sticks the same way:

```rust
// before
    fn stuck_section(&self, col: Col, cx: &mut Context<Self>) -> Div {
        div().absolute().top_0().left_0().right_0().px(px(8.)).child(div().rounded(px(7.)).bg(POPOVER).occlude().child(self.section_bar(col, cx)))
    }
// after
    fn stuck(&self, bar: Div) -> Div {
        div().absolute().top_0().left_0().right_0().px(px(8.)).child(div().rounded(px(7.)).bg(POPOVER).occlude().child(bar))
    }
```

Keep its doc comment about `CANVAS` in dark mode.

**Step 3: Build and run the tests**

Run: `cargo build -p pocket && cargo test -p pocket board::`
Expected: PASS. The `board::selection` and `board::logic` tests pass, and the status sections are drawn as before when no ticket has a station.

### Task 4.3: `anywhere ticket list` and `ticket read` show where a Factory ticket stands

**Files:**
- Modify: `packages/pocketd/cmd/pocketd/anywhere.go` (`ticketListCmd` at line 694, `ticketReadCmd` at line 718, new helpers after `ticketState` at line 687)
- Modify: `packages/pocketd/internal/cliskill/SKILL.md` (section "Put work on the Board", line 51)
- Test: `packages/pocketd/cmd/pocketd/anywhere_test.go`

**Context:** US-37: an agent that reads the Board sees each ticket's station, gate, fix attempts and pull request, and can make none of your decisions. Plan 1 adds `Station`, `Gate`, `PR` and `Fix` to `proto.Ticket`, so `--json` already carries them. The two plain outputs do not. The tests in `anywhere_test.go` test plain helpers (`TestALiveTicketShowsWhatItsSessionIsDoing` at line 287), so this task moves each output line into a helper. `internal/cliskill/SKILL.md` lists the commands and lists no output columns. It gains two sentences, so an agent knows the fields exist and that deciding is not its job. `TestInstall` compares the installed skill to the embedded bytes, so it needs no change.

**Step 1: Write the failing tests**

Add to `cmd/pocketd/anywhere_test.go`:

```go
func TestTicketListShowsAFactoryTicketsStationAndADashForAnyOther(t *testing.T) {
	factory := proto.Ticket{ID: "t1", State: proto.TicketLive, Status: proto.TicketNeedsYou, Provider: "claude", Folder: "/code/api", Title: "Fold hunks", Station: proto.StationPlan}
	if got, want := ticketRow(factory), "t1\tneedsYou\tplan\tclaude\tapi\tFold hunks"; got != want {
		t.Fatalf("got %q, want %q", got, want)
	}
	plain := proto.Ticket{ID: "t2", State: proto.TicketTodo, Provider: "codex", Folder: "/code/web", Title: "Dark mode"}
	if got, want := ticketRow(plain), "t2\ttodo\t-\tcodex\tweb\tDark mode"; got != want {
		t.Fatalf("got %q, want %q", got, want)
	}
}

func TestTicketReadShowsTheStationGateFixesAndPROfAFactoryTicket(t *testing.T) {
	full := proto.Ticket{Station: proto.StationChecks, Gate: proto.GateAsk, Fix: 1, PR: 12}
	if got, want := factoryLine(full), "station checks · gate ask · fix 1 · PR #12"; got != want {
		t.Fatalf("got %q, want %q", got, want)
	}
	if got, want := factoryLine(proto.Ticket{Station: proto.StationImplement}), "station implement"; got != want {
		t.Fatalf("got %q, want %q", got, want)
	}
	if got := factoryLine(proto.Ticket{State: proto.TicketTodo}); got != "" {
		t.Fatalf("a ticket outside the Factory got %q", got)
	}
}
```

**Step 2: Run the tests to see them fail**

Run from `packages/pocketd`: `go test ./cmd/pocketd/ -run 'TestTicketList|TestTicketRead'`
Expected: FAIL to compile with `undefined: ticketRow` and `undefined: factoryLine`.

**Step 3: Implement**

Add after `ticketState` (`anywhere.go:692`):

```go
// ticketRow is a ticket's line in ticket list. A ticket outside the Factory has no station.
func ticketRow(t proto.Ticket) string {
	station := t.Station
	if station == "" {
		station = "-"
	}
	return strings.Join([]string{t.ID, ticketState(t), station, t.Provider, filepath.Base(t.Folder), t.Title}, "\t")
}

// factoryLine is where a Factory ticket stands, as ticket read shows it, or "" outside the Factory.
func factoryLine(t proto.Ticket) string {
	if t.Station == "" {
		return ""
	}
	parts := []string{"station " + t.Station}
	if t.Gate != "" {
		parts = append(parts, "gate "+t.Gate)
	}
	if t.Fix > 0 {
		parts = append(parts, fmt.Sprintf("fix %d", t.Fix))
	}
	if t.PR > 0 {
		parts = append(parts, fmt.Sprintf("PR #%d", t.PR))
	}
	return strings.Join(parts, " · ")
}
```

`ticketListCmd` (`anywhere.go:712-714`):

```go
// before
	for _, t := range list {
		fmt.Fprintf(tw, "%s\t%s\t%s\t%s\t%s\n", t.ID, ticketState(t), t.Provider, filepath.Base(t.Folder), t.Title)
	}
// after
	for _, t := range list {
		fmt.Fprintln(tw, ticketRow(t))
	}
```

`ticketReadCmd` (`anywhere.go:736`):

```go
// before
	_, err = fmt.Fprintf(w, "%s\n%s %s %s\n\n%s\n", t.Title, ticketState(t), t.Provider, t.Folder, t.Prompt)
	return err
// after
	head := fmt.Sprintf("%s\n%s %s %s\n", t.Title, ticketState(t), t.Provider, t.Folder)
	if line := factoryLine(t); line != "" {
		head += line + "\n"
	}
	_, err = fmt.Fprintf(w, "%s\n%s\n", head, t.Prompt)
	return err
```

The usage text (`anywhereUsage`) does not change: no flag is new, and no verb is added.

In `internal/cliskill/SKILL.md`, add a paragraph after the one that ends `` `delete` refuses a ticket with a live session.`` (line 67):

```markdown
In a project with the Factory on, `ticket list` adds the ticket's station, and `ticket read` adds its station, the gate it waits at, its auto-fix attempts and its pull request. `--json` carries `station`, `gate`, `fix` and `pr`. No `anywhere` command approves a plan, answers an Ask, merges or sends a hint. The user decides those in the app.
```

**Step 4: Run the tests to see them pass**

Run: `go test ./cmd/pocketd/ ./internal/cliskill/`
Expected: `ok` for both packages.

### Task 4.4: The README, and the changelog lines

**Files:**
- Modify: `README.md` (section "Features", lines 17 to 26)

**Context:** The README's "Features" list is the only user documentation in the repository. `docs/` holds ADRs, designs, plans and research, not pages for users. Each bullet is a bold lead-in and one or two sentences. `CHANGELOG.md` is drafted at release by `scripts/changelog-draft.sh <version>`, one line per merged PR from its squash title, so the four PR titles above are the changelog lines. Do not edit `CHANGELOG.md` in these PRs. The PRD asks to dogfood the Factory for a week before announcing it, so the README bullet ships in this last PR.

**Step 1: Add the bullet**

`README.md`, after the "Automations" bullet (line 23):

```markdown
- **Light Factory.** Turn it on for a project, and each ticket goes from spec to plan, to code, to checks, to a reviewed pull request on its own. It stops where you decide: the plan, a command it wants to run, and the merge. The Inbox lists each decision, and Settings › Factory holds the agent profiles and two weeks of numbers. It needs the GitHub CLI signed in.
```

**Step 2: Check it**

Run: `git diff --stat README.md`
Expected: `README.md | 1 +`.

### Task 4.5: Capture `board-list-stations` and run the checks

**Files:**
- Modify: `packages/desktop/crates/pocket/src/capture.rs` (`STEPS`, a step after `board-list` at line 500)

**Context:** Plan 2 adds Factory tickets to the fixture (its Task 3.1) and the step `board-factory`. In the `sessions` scenario every fixture ticket that plan 2 gave a station gets one here only if it has none, so the step works with or without plan 2's fixture. `board_changed` regroups, because the step changes tickets after the Board opened.

**Step 1: Add the step**

Bump the length of `STEPS` by one. Add after `board-list`:

```rust
    ("board-list-stations", |d, _, cx| {
        use agents::tickets::{Gate, State, Station};
        let at = [(Station::Spec, Gate::None), (Station::Plan, Gate::Plan), (Station::Implement, Gate::None), (Station::Checks, Gate::None), (Station::Pr, Gate::Pr)];
        let mut next = at.into_iter().cycle();
        for t in d.agents.board.tickets.iter_mut().filter(|t| t.state == State::Live && t.station == Station::None) {
            (t.station, t.gate) = next.next().unwrap_or((Station::Implement, Gate::None));
        }
        d.board.layout = crate::board::Layout::List;
        d.board_changed();
        cx.notify();
    }),
```

**Step 2: Take the pictures**

From the repo root:

```sh
.ui-review/fixture/capture.sh /tmp/lf7-after board-list=board,board-list board-list-stations=board,board-list-stations board-factory=board,board-factory
```

Expected:
- `impl-board-list.png` matches `/tmp/lf7-before/impl-board-list.png` if the fixture holds no ticket with a station. If plan 2's fixture gave some tickets a station, the List already shows station sections here. Compare it with `impl-board-list-stations.png` instead.
- `impl-board-list-stations.png` shows foldable bars `To do`, `Spec`, `Plan`, `Implement`, `Checks`, `PR review` and `Merged` (each one that holds a ticket), with the station glyph, the name and the count, and `+` on To do. The rows under them are the List's 36px rows.
- `impl-board-factory.png` matches `/tmp/lf7-before/impl-board-factory.png`.

**Step 3: Run the checks**

From `packages/pocketd`: `go vet ./... && go test ./...`
From `packages/desktop`: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: PASS, with no new warning. This PR changes no scope and no golden.

---

## Stories this plan covers

| Story | Where |
|---|---|
| US-07, grouped List | PR 4, Tasks 4.1, 4.2, 4.5. The Status layout stays (see Decisions) |
| US-30, the Inbox | PR 1 |
| US-34, what is left: decide from the Inbox | PR 1, Task 1.4 |
| US-37, the `anywhere` CLI | PR 4, Task 4.3 |
| US-39, the metrics | PR 3 |
| US-40, the desktop half | PR 2 |

## Contract changes needed

- **`FactoryEvent.Kind` gains `gate`.** In the index's `FactoryEvent` comment, replace `Kind string // moved | approved | …` with `Kind string // moved | gate | approved | changes | asked | allowed | denied | autoAllowed | handback | check | fix | hint | finding | skill | merged | failed`, and add under it: "`gate`: the Ticket started to wait at the Plan or the PR gate. `Engine.step` notes it when `Gate` turns `plan` or `pr`. The metrics read it (plan 7)."
- **The Factory ticket page shares its panel and bar.** In the index's `factory/page.rs` and `factory/bar.rs` bullets, add: "`Desktop::factory_panel(&mut self, ticket: &str, window, cx) -> Div` draws the title, status line, tabs and tab body. `Desktop::factory_bar(&mut self, ticket: &str, ask: Option<&agents::Permission>, window, cx) -> Option<Div>` draws the footer bar. `Desktop::factory_primary(&mut self, ticket: &str, ask: Option<&agents::Permission>, window, cx) -> bool` runs the bar's primary action. The page and the Inbox (plan 7) both call them. The `Ask` in `bar::decision` is `agents::Permission`."
- **`Agents` keeps the metrics.** In the index's `Agents` bullet, add: "`metrics: HashMap<String, Metrics>` with `Agents::metrics(project) -> Option<&Metrics>` (plan 7)."

## Open questions

- **Should a Factory ticket that failed raise a banner and a sound?** The Inbox lists it, and the bell and the Dock badge count it. Alerts follow sessions (`sync_alerts`), so a failure the Ticket reports, not a session, alerts nothing. Recommended: leave it out of this plan, and add it when someone misses a failure in dogfooding.
- **Should the done rows of a Factory ticket's sessions stay out of the Inbox?** This plan leaves them out, because the Ticket's own row covers what needs you. A lead's "done" means little while the Factory moves the Ticket on. Recommended: keep them out.
- **Which tickets count for the auto-fix numbers?** This plan counts each Ticket's first auto-fix at Checks that ended in the window. A second failure later in the same Ticket is not counted. Recommended: keep it, because PRD section 9 asks for the share repaired within the cap, and a Ticket's first episode is the one the cap governs.
- **Is `zip` 7 still the newest major version when PR 2 is built?** The registry cache holds 7.2.0. Recommended: take the newest major whose feature `deflate-flate2-zlib-rs` exists, and keep `default-features = false`.
- **Should "Add an installed skill" list a remote host's skills?** This plan reads only this Mac's `~/.claude/skills` and `~/.agents/skills`. Recommended: leave remote hosts out until a profile can be used on one. Profiles are stored by this Mac's pocketd.
- **Is the Rust `Profile` flat?** Plan 4's Go `Profile` embeds `ProfileInfo`. This plan's Rust `Profile` is flat, which reads the same JSON. Recommended: keep it flat. If plan 4 shipped a nested Rust type, Task 2.0 says to follow it.


