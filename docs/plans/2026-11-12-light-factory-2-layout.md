# Light Factory 2: The Factory Layout of the Board Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** The Board gains a third layout, **Factory**. It shows every Ticket under its station in seven fixed columns: To do, Spec, Plan, Implement, Checks, PR review, Merged. To do and Merged are 44px rails. A segmented control **Factory · Status · List** in the Board's top bar picks the layout. The layout is read-only: it places and describes Tickets, and a card opens what it opens today.

**Architecture:** `store` replaces the pref `board_list: bool` with `board_layout: BoardLayout` and reads the old key once. A new feature module `pocket/src/factory.rs` plus `pocket/src/factory/station.rs` owns `FactoryState` (the seven columns, one GPUI `list` per column, the open rails, the chip counts). Every decision is a plain function over data in `factory/station.rs`: the column of a Ticket, its gate, the order inside a column, its note, the chip counts and the column widths. `pocket/src/board/stations.rs` draws the columns and rails and reuses `ticket_card`, the filters and the selection. `Desktop::board_changed` regroups the stations. `render` only reads what `regroup` computed.

**Toolset (from `packages/desktop`):**
- One module's tests: `cargo test -p pocket factory::station`, `cargo test -p pocket board::`, `cargo test -p store board`
- Everything: `cargo build --workspace`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`
- Screens (from the repository root, needs `bun`): `.ui-review/fixture/capture.sh <dir> <name>=<step>,<step> …`. A name must start with `board` so that the script picks the `sessions` fixture.

**Read first:**
- `docs/plans/2026-11-12-light-factory.md`: the index. Read the decisions "Layout switch", "Sidebar" and "Status colours", and the section "Desktop crates".
- `docs/plans/2026-11-12-light-factory-1-foundations.md`, PR 4: `agents::tickets::{Station, Gate}`, the five Ticket fields, `Ticket::sample()` and `Agents::factory_offered()`. PR 1, Task 1.4: `board::logic::busy` counts `Starting` and `Working` only.
- `docs/designs/2026-11-12-light-factory.md` §5.1: requirements B1 to B9 and B13. `docs/designs/2026-11-12-light-factory-stories.md`: US-01 to US-04 and US-38.
- `.ui-review/prototypes/light-factory/app.js` (`factoryHTML`, `railHTML`, `cardHTML`, `GLYPH`, `upNext`) and `styles.css` (`.board`, `.col`, `.col-h`, `.rail-tab`, `.chip`, `.slots`).
- `CLAUDE.md` and `docs/adr/0003-desktop-code-layout.md`: the rules for desktop code and for performance.
- `packages/desktop/crates/pocket/src/board.rs` (`Layout`, `BoardState`, `regroup`, `walk_slots`, `open_board`, `board_changed`), `board/logic.rs` (`Col`, `column`, `Filters`, `keeper`, `arrange`, `Slot`, `Tone`, `note`), `board/card.rs`, `board/column.rs`, `board/page.rs`, `board/tabs.rs`, `board/top_bar.rs`, `board/display_menu.rs`, `board/selection.rs`.
- `packages/desktop/crates/store/src/store.rs` (`Store`, `Store::load`, `lenient`, the tests that write a `desktop.json`).
- `packages/desktop/crates/pocket/src/capture.rs` (`STEPS`, the `board*` steps) and `.ui-review/fixture/fixture.ts` (`TICKETS`, the `hello.ok` frame).

Line numbers are those of `origin/main` at commit `c638cf38`. Plan 1 edits some of the same files, so find each place by the code quoted, not by the number.

## Decisions

- **Two layout enums.** `store::BoardLayout { Factory, Board, List }` is the pref. `pocket::board::Layout { Factory, Board, List }` is what the page draws. `shown_layout(saved, offered)` turns one into the other: a saved `Factory` opens as `Board` while pocketd does not offer the Factory, and the pref is not overwritten. The segmented control calls `Layout::Board` **Status**, as the index does.
- **The default layout stays Status.** A new install, and an old `desktop.json` without `board_list`, open the Status layout. The Factory layout shows only after you pick it.
- **The switch is always in the top bar.** Without `factory.v1` it offers **Status · List**. With it, **Factory · Status · List**. Plan 1 serves the cap as soon as its config store exists, so the Factory segment shows after plan 1 and this plan are merged, before plan 3's engine sets a station.
- **The station is derived until pocketd sets it.** `station::column` returns the column of `Ticket.station` when it is set. Otherwise: a Ticket in To do or Queued sits in **To do**. A live Ticket whose Team is `planning` or `proposed` sits in **Plan**. A live Ticket whose Team is `running` or `review` sits in **Implement**. A live Ticket with an open PR sits in **PR review**, with a merged PR in **Merged**. Any other live Ticket sits in **Implement**. A Ticket closed as merged sits in **Merged**.
- **A Ticket that closed without a merge has no column.** The Factory layout does not draw it and does not count it as hidden. The Status and List layouts show it with **Show closed tickets**, as today.
- **The gate is derived the same way.** `station::gate` returns `Ticket.gate` when pocketd set one. Otherwise a live Ticket with status `needsYou` waits at the Plan gate when its Team is `proposed`, and has an Ask in every other case.
- **Order in a column (B13).** Tickets that wait on you come first, then failed Tickets, then the rest, each group oldest first by `changed_at`. **Merged** is the exception: it shows its latest 50 Tickets, newest first, as the Status layout's Merged column does. **Display › Ordering** does not apply to the Factory layout, so the Display menu hides it there, with **Show closed tickets**.
- **One note per card (B6, B7, B8).** `Plan ready · 3 tasks`, `Ask · cargo test -p daemon`, `Ready to merge · #203`, or `Failed · <reason>`. The note is one text run in the note colour. The command is not set in mono. A working Ticket keeps today's activity line.
- **A card in the Factory layout has no Deny or Allow once button (US-03).** **Start**, **Start again** and **Create PR** stay, because they decide nothing at a gate. A card cannot be dragged in the Factory layout.
- **The chips filter every layout (US-04).** `Filters` gains `needs: Option<Needs>`, so `keeper` applies it to the Status and List layouts too. The chips are drawn only in the Factory layout, in the row that holds the tabs in the other layouts. In the other layouts the notice "N tickets hidden by filters · Clear filters" clears the chip.
- **The chip counts read the Ticket only.** `counts` runs over every Ticket and looks at no Team. A Ticket without a station that needs you counts under `ask`, also when its Team is `proposed`. Once pocketd sets `Ticket.gate` (plan 3), the count is exact.
- **B9's four pickers are today's Filter menu.** Project, Label, Agent and Priority stay behind the **Filter** button, next to **Display**. This plan adds no second set of pickers.
- **Fit (B3).** `station::widths(available, open)` gives each of the five middle columns the same width between 196 and 248px. With a 1280px window and the 56px rail, the page is 1224px wide and each middle column is 208px, with no sideways scroll. Below 1164px of page width the columns keep 196px and the layout scrolls sideways. The sidebar does not collapse by itself (index, decision "Sidebar").
- **A rail opens on click** into a 232px column, as the prototype does. A click on the heading of an open rail closes it. The open state is not saved.
- **The rail's name is drawn one upright letter per line.** GPUI rotates SVGs only, not text.
- **Left out:** B12, the hidden-column tray for stations (P2). B10, the `⌘K` command menu (P1). B14's sidebar rows (index, decision "Sidebar"), so of US-08 this plan keeps only "no change to the sidebar". The Factory ticket page, the footer bar and **Settings › Factory** belong to plan 3. Grouped Status and List layouts belong to plan 7.
- **`station.rs` is dead code until PR 3.** PR 1 and PR 2 add functions that only tests call. `factory.rs` carries `#[allow(dead_code)]` on `mod station` until Task 3.5 removes it.

## Design → GPUI translation

| Prototype | App |
|---|---|
| `.board` (gap 12, padding `0 12px 8px`) | `div().px(px(PAD)).pb(px(8.)).flex().gap(px(GAP))` with `overflow_x_scroll` |
| `.col` (196 to 248px, radius 10, `--fill-1`) | `station_column`: `w(px(width))`, `rounded(px(10.))`, `bg(FILL_1)` |
| `.col-h` (38px, padding 0 12, gap 8, 12.5px, weight 500) | `station_heading` |
| `.col-h .n` | the count, weight 400, `TEXT_3` |
| `.col-h .who` (`margin-left: auto`, 12px) | `"You"`, `ml_auto()`, 12px, `TEXT_3` |
| `.rail-tab` (44px, column, gap 8, padding-top 12, vertical name) | `station_rail`: glyph, the name one letter per line, the count |
| `RAIL_W = 44`, `RAIL_OPEN = 232` | `station::RAIL_W`, `station::RAIL_OPEN` |
| `.chip` (26px, radius 13, 12.5px, weight 500) | `tabs.rs` `tab_chip` |
| `.chip.on.you` | `bg(WAITING_BG)`, `text_color(WAITING_TEXT)` |
| `.chip .n` | the count before the label, weight 400, `TEXT_4` |
| `.slots` (`<b class="mono">3/4</b> slots`) | `3/4` in `MONO`, then `slots`, 12.5px, `TEXT_3` |
| `GLYPH` (`app.js:157`) | `parts::station_mark`: todo `status-todo` `TEXT_4`, spec `status-queued` `WORKING`, plan `status-quarter` `WORKING`, implement `status-half` `WORKING`, checks `status-three-quarters` `SUCCESS`, pr `status-full` `GOAL_TEXT`, merged `status-merged` `INDIGO`, a gate `status-needs` `WAITING_DOT`, failed `status-closed` `FAILED` |
| `.note.asking`, `.note.failed` | `Tone::Asking` → `WAITING_TEXT`, `Tone::Failed` → `FAILED_TEXT` |
| `upNext` (`app.js:83`) | `station::rank` |

---

## PR 1: Stations as plain data

**Scope:** `pocket/src/factory.rs` and `pocket/src/factory/station.rs` with the pure rules: `Column`, `COLUMNS`, `column`, `on_line`, `gate`, `waits`, `failed`, `rank`, `arrange`, `walk`, `note`, `asker`, `pr_number`, `progress` and `widths`. `board::logic::Filters::keeper` and `FINISHED_SHOWN` become `pub(crate)`. Inert: nothing calls these functions outside their tests until PR 2 and PR 3. No screen changes.
**Depends on:** plan 1, PR 4 (Tasks 4.1 and 4.3) and PR 1 (Task 1.4), merged.
**Done when:** from `packages/desktop`, `cargo build --workspace`, `cargo clippy --workspace --all-targets` (no new warnings) and `cargo test --workspace` pass.

### Task 1.0: Check the plan 1 contracts

**Files:** none changed.

**Context:** This plan builds on names that plan 1 adds (index, section "Desktop crates"). They are not in `origin/main` at `c638cf38`.

**Step 1:** From `packages/desktop` run:

```bash
grep -n "pub enum Station\|pub enum Gate\|pub station: Station\|pub gate: Gate\|pub pr: u32\|pub fn sample" crates/agents/src/tickets.rs
grep -n "pub fn factory_offered" crates/agents/src/agents.rs
grep -n "matches!(t.status, Status::Starting | Status::Working))" crates/pocket/src/board/logic.rs
```

Expected: six hits in `tickets.rs`, one in `agents.rs`, one in `logic.rs`.

**Step 2:** If a hit is missing, stop. Plan 1's PR 4 (Tasks 4.1 and 4.3) or PR 1 (Task 1.4) is not merged. Do not add the names here.

### Task 1.1: `Column`, `column` and `gate`

**Files:**
- Create: `packages/desktop/crates/pocket/src/factory.rs`
- Create: `packages/desktop/crates/pocket/src/factory/station.rs`
- Modify: `packages/desktop/crates/pocket/src/main.rs:15` (the `mod` list)

**Context:** Plan 1 adds `Ticket.station: Station`, `Ticket.gate: Gate` and `Ticket::sample()` (index, section "Desktop crates"). `Station::None` and `Gate::None` mean "pocketd set nothing". A Team is keyed by its lead's agent id, which is `Ticket.agent_id`. `agents::Team.state` is one of `planning`, `proposed`, `running`, `review`, `merging`, `merged`, `conflict` or `failed`. `crate::team::PROPOSED` and `agents::team::state::RUNNING` are the two states that have a constant. `git::github::Pr.state` is `PrState::{Open, Merged, Closed}`. The Status layout's `board::logic::column` maps a Ticket closed for any reason other than a merge to `Col::Closed`.

**Step 1: Write the failing tests.** Create `factory.rs` with one line:

```rust
#[allow(dead_code)]
pub(crate) mod station;
```

In `main.rs`, add the module after `mod explorer;`:

```rust
mod explorer;
mod factory;
mod field;
```

Create `factory/station.rs` with only these tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn live(status: Status) -> Ticket {
        Ticket { state: State::Live, status, agent_id: "lead".into(), ..Ticket::sample() }
    }

    fn team(state: &str) -> Team {
        Team { lead: "lead".into(), state: state.into(), ..Team::default() }
    }

    fn pr(state: PrState) -> Pr {
        Pr { number: 203, state, ..Pr::default() }
    }

    #[test]
    fn a_station_set_by_pocketd_names_the_column() {
        let stations = [(Station::Spec, Column::Spec), (Station::Plan, Column::Plan), (Station::Implement, Column::Implement), (Station::Checks, Column::Checks), (Station::Pr, Column::Pr), (Station::Merged, Column::Merged)];
        for (station, want) in stations {
            let t = Ticket { station, ..live(Status::Working) };
            assert_eq!(column(&t, Some(&team("running")), Some(&pr(PrState::Open))), want);
        }
    }

    #[test]
    fn a_ticket_without_a_station_takes_its_column_from_its_team_and_its_pr() {
        let t = live(Status::Working);
        assert_eq!(column(&t, Some(&team("planning")), None), Column::Plan);
        assert_eq!(column(&t, Some(&team("proposed")), None), Column::Plan);
        assert_eq!(column(&t, Some(&team("running")), None), Column::Implement);
        assert_eq!(column(&t, Some(&team("review")), Some(&pr(PrState::Open))), Column::Implement);
        assert_eq!(column(&t, None, Some(&pr(PrState::Open))), Column::Pr);
        assert_eq!(column(&t, Some(&team("merged")), Some(&pr(PrState::Open))), Column::Pr);
        assert_eq!(column(&t, None, Some(&pr(PrState::Merged))), Column::Merged);
        assert_eq!(column(&t, None, Some(&pr(PrState::Closed))), Column::Implement);
        assert_eq!(column(&t, None, None), Column::Implement);
    }

    #[test]
    fn a_queued_ticket_sits_in_to_do() {
        let queued = Ticket { state: State::Queued, ..Ticket::sample() };
        assert_eq!(column(&queued, None, None), Column::Todo);
        assert_eq!(column(&Ticket::sample(), None, None), Column::Todo);
    }

    #[test]
    fn a_ticket_closed_as_merged_sits_in_merged_and_any_other_closed_ticket_has_no_column() {
        let merged = Ticket { state: State::Closed, closed_why: ClosedWhy::Merged, ..Ticket::sample() };
        let closed = Ticket { state: State::Closed, closed_why: ClosedWhy::Closed, station: Station::Checks, ..Ticket::sample() };
        assert_eq!((on_line(&merged), column(&merged, None, None)), (true, Column::Merged));
        assert!(!on_line(&closed));
        assert!(on_line(&live(Status::Working)) && on_line(&Ticket::sample()));
    }

    #[test]
    fn the_columns_keep_their_order_with_a_rail_at_each_end_and_you_on_the_two_gates() {
        assert_eq!(COLUMNS.map(Column::name), ["To do", "Spec", "Plan", "Implement", "Checks", "PR review", "Merged"]);
        assert_eq!(COLUMNS.map(Column::rail), [true, false, false, false, false, false, true]);
        assert_eq!(COLUMNS.map(Column::yours), [false, false, true, false, false, true, false]);
    }

    #[test]
    fn a_gate_set_by_pocketd_is_the_gate() {
        for g in [Gate::Plan, Gate::Pr, Gate::Ask] {
            assert_eq!(gate(&Ticket { gate: g, ..live(Status::NeedsYou) }, None), g);
        }
        assert_eq!(gate(&Ticket { gate: Gate::Pr, ..Ticket::sample() }, None), Gate::None, "only a live ticket waits");
    }

    #[test]
    fn a_ticket_without_a_gate_that_needs_you_waits_on_its_plan_or_on_an_ask() {
        let t = live(Status::NeedsYou);
        assert_eq!(gate(&t, Some(&team("proposed"))), Gate::Plan);
        assert_eq!(gate(&t, Some(&team("running"))), Gate::Ask);
        assert_eq!(gate(&t, None), Gate::Ask);
        assert_eq!(gate(&live(Status::Working), Some(&team("proposed"))), Gate::None);
        assert!(waits(&t) && !waits(&live(Status::Failed)));
        assert!(failed(&live(Status::Failed)) && !failed(&Ticket { status: Status::Failed, ..Ticket::sample() }));
    }
}
```

**Step 2: Run** `cargo test -p pocket factory::station`

Expected: FAIL to compile. The errors name `cannot find type \`Ticket\``, `cannot find function \`column\`` and `cannot find type \`Column\``.

**Step 3: Implement.** Put this above the tests in `factory/station.rs`:

```rust
use crate::team::PROPOSED;
use agents::Team;
use agents::team::state::RUNNING;
use agents::tickets::{ClosedWhy, Gate, State, Station, Status, Ticket};
use git::github::{Pr, PrState};

/// A column of the Factory layout.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Column {
    Todo,
    Spec,
    Plan,
    Implement,
    Checks,
    Pr,
    Merged,
}

pub(crate) const COLUMNS: [Column; 7] = [Column::Todo, Column::Spec, Column::Plan, Column::Implement, Column::Checks, Column::Pr, Column::Merged];

impl Column {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Column::Todo => "To do",
            Column::Spec => "Spec",
            Column::Plan => "Plan",
            Column::Implement => "Implement",
            Column::Checks => "Checks",
            Column::Pr => "PR review",
            Column::Merged => "Merged",
        }
    }

    /// Drawn as a narrow rail until it is opened.
    pub(crate) fn rail(self) -> bool {
        matches!(self, Column::Todo | Column::Merged)
    }

    /// You decide here.
    pub(crate) fn yours(self) -> bool {
        matches!(self, Column::Plan | Column::Pr)
    }
}

/// Where the Factory layout puts a ticket: the station pocketd set, or one read from its state, its team and its PR.
pub(crate) fn column(t: &Ticket, team: Option<&Team>, pr: Option<&Pr>) -> Column {
    match t.station {
        Station::Spec => return Column::Spec,
        Station::Plan => return Column::Plan,
        Station::Implement => return Column::Implement,
        Station::Checks => return Column::Checks,
        Station::Pr => return Column::Pr,
        Station::Merged => return Column::Merged,
        Station::None => {}
    }
    match t.state {
        State::Todo | State::Queued => Column::Todo,
        State::Closed => Column::Merged,
        State::Live => match (team.map(|t| t.state.as_str()), pr.map(|p| p.state)) {
            (Some("planning" | PROPOSED), _) => Column::Plan,
            (Some(RUNNING | "review"), _) => Column::Implement,
            (_, Some(PrState::Open)) => Column::Pr,
            (_, Some(PrState::Merged)) => Column::Merged,
            _ => Column::Implement,
        },
    }
}

/// Whether the Factory layout shows the ticket. One closed without a merge left the line.
pub(crate) fn on_line(t: &Ticket) -> bool {
    t.state != State::Closed || t.closed_why == ClosedWhy::Merged
}

/// What a ticket waits on you for: the gate pocketd set, or one read from its status and its team.
pub(crate) fn gate(t: &Ticket, team: Option<&Team>) -> Gate {
    if t.state != State::Live {
        return Gate::None;
    }
    match (t.gate, t.status) {
        (Gate::None, Status::NeedsYou) if team.is_some_and(|t| t.state == PROPOSED) => Gate::Plan,
        (Gate::None, Status::NeedsYou) => Gate::Ask,
        (gate, _) => gate,
    }
}

pub(crate) fn waits(t: &Ticket) -> bool {
    gate(t, None) != Gate::None
}

pub(crate) fn failed(t: &Ticket) -> bool {
    t.state == State::Live && t.status == Status::Failed
}
```

**Step 4: Run** `cargo test -p pocket factory::station`

Expected: PASS, `7 passed`.

### Task 1.2: `rank`, `arrange` and `walk`

**Files:**
- Modify: `packages/desktop/crates/pocket/src/factory/station.rs`
- Modify: `packages/desktop/crates/pocket/src/board/logic.rs:329` (`keeper`), `:474` (`FINISHED_SHOWN`)

**Context:** `board::logic::arrange` (`logic.rs:480`) is the Status layout's grouping. It returns indices into the snapshot per column, after `Filters::keeper` dropped what the filters and the search hide. The Factory layout needs the same shape for seven columns. `board::logic::Slot { col, pos, index }` is where a card is shown. `ticket_card` and the selection read `Slot.col` as the Status layout's `Col` (`shown_ticket` checks it, `Col::finished` decides what can be picked). So a slot of the Factory layout carries the Ticket's Status column in `col`, its place in the station column in `pos`, and its index in the snapshot. `activity(t)` (`logic.rs:469`) is `closed_at` for a closed Ticket and `changed_at` otherwise. The Team and the PR of a Ticket come from two lookups, so that the function needs no `Agents` and no `PullRequests`.

**Step 1: Write the failing tests.** In `station.rs`, add `use crate::board::logic::Col;` under `use super::*;` in `mod tests`, and add these tests at the bottom of the module:

```rust
    fn at(t: Ticket, changed_at: i64) -> Ticket {
        Ticket { changed_at, ..t }
    }

    #[test]
    fn rank_puts_a_gate_before_a_failure_before_the_rest_and_the_oldest_first() {
        assert!(rank(&at(live(Status::NeedsYou), 9)) < rank(&at(live(Status::Failed), 1)));
        assert!(rank(&at(live(Status::Failed), 9)) < rank(&at(live(Status::Working), 1)));
        assert!(rank(&at(live(Status::Working), 1)) < rank(&at(live(Status::Working), 2)));
    }

    #[test]
    fn a_column_lists_what_waits_on_you_then_what_failed_then_the_oldest_first() {
        let tickets = [at(live(Status::Working), 30), at(live(Status::Failed), 40), at(live(Status::NeedsYou), 50), at(live(Status::Working), 10), at(live(Status::NeedsYou), 20)];
        let columns = arrange(&tickets, &Filters::default(), |_| None, |_| None);
        assert_eq!(columns[Column::Implement as usize], [4, 2, 1, 3, 0]);
    }

    #[test]
    fn every_ticket_on_the_line_lands_in_exactly_one_column() {
        let tickets = [
            Ticket::sample(),
            Ticket { state: State::Queued, ..Ticket::sample() },
            Ticket { station: Station::Spec, ..live(Status::Working) },
            Ticket { station: Station::Checks, ..live(Status::Failed) },
            live(Status::Working),
            Ticket { state: State::Closed, closed_why: ClosedWhy::Merged, ..Ticket::sample() },
            Ticket { state: State::Closed, ..Ticket::sample() },
        ];
        let columns = arrange(&tickets, &Filters::default(), |_| None, |_| None);
        assert_eq!(columns, [vec![0, 1], vec![2], vec![], vec![4], vec![3], vec![], vec![5]]);
    }

    #[test]
    fn a_ticket_is_placed_by_the_team_and_the_pr_that_the_lookups_find() {
        let (proposed, open) = (team("proposed"), pr(PrState::Open));
        let tickets = [live(Status::NeedsYou), Ticket { agent_id: "solo".into(), worktree: "/w/pr".into(), ..live(Status::Working) }];
        let columns = arrange(&tickets, &Filters::default(), |t| (t.agent_id == "lead").then_some(&proposed), |t| (t.worktree == "/w/pr").then_some(&open));
        assert_eq!(columns[Column::Plan as usize], [0]);
        assert_eq!(columns[Column::Pr as usize], [1]);
    }

    #[test]
    fn merged_shows_its_latest_fifty_newest_first() {
        let tickets: Vec<Ticket> = (1..=60).map(|n| Ticket { state: State::Closed, closed_why: ClosedWhy::Merged, closed_at: n, ..Ticket::sample() }).collect();
        let columns = arrange(&tickets, &Filters::default(), |_| None, |_| None);
        let merged = &columns[Column::Merged as usize];
        assert_eq!((merged.len(), merged[0], merged[49]), (50, 59, 10));
    }

    #[test]
    fn the_search_hides_a_ticket_from_its_column() {
        let tickets = [Ticket { title: "Fix scroll".into(), ..live(Status::Working) }, Ticket { title: "Add search".into(), ..live(Status::Working) }];
        let found = Filters { query: "scroll".into(), ..Filters::default() };
        assert_eq!(arrange(&tickets, &found, |_| None, |_| None)[Column::Implement as usize], [0]);
    }

    #[test]
    fn the_walk_reads_each_column_top_to_bottom_and_skips_a_closed_rail() {
        let tickets = [Ticket::sample(), Ticket { station: Station::Spec, ..live(Status::Working) }, live(Status::NeedsYou), Ticket { state: State::Closed, closed_why: ClosedWhy::Merged, ..Ticket::sample() }];
        let columns = arrange(&tickets, &Filters::default(), |_| None, |_| None);
        assert_eq!(walk(&columns, &[false; COLUMNS.len()], &tickets), [Slot { col: Col::Working, pos: 0, index: 1 }, Slot { col: Col::NeedsYou, pos: 0, index: 2 }]);
        let mut open = [false; COLUMNS.len()];
        open[Column::Todo as usize] = true;
        assert_eq!(walk(&columns, &open, &tickets).first(), Some(&Slot { col: Col::Todo, pos: 0, index: 0 }));
    }
```

**Step 2: Run** `cargo test -p pocket factory::station`

Expected: FAIL to compile. The errors name `cannot find function \`rank\``, `cannot find function \`arrange\``, `cannot find function \`walk\`` and `cannot find type \`Filters\``.

**Step 3: Implement.** In `board/logic.rs`, widen two items. `logic.rs:329`, before:

```rust
    fn keeper(&self) -> impl Fn(&Ticket) -> bool + '_ {
```

After:

```rust
    pub(crate) fn keeper(&self) -> impl Fn(&Ticket) -> bool + '_ {
```

`logic.rs:473-474`, before:

```rust
/// How many finished tickets a column shows, the latest ones.
const FINISHED_SHOWN: usize = 50;
```

After:

```rust
/// How many finished tickets a column shows, the latest ones.
pub(crate) const FINISHED_SHOWN: usize = 50;
```

In `station.rs`, the imports at the top become:

```rust
use crate::board::logic::{FINISHED_SHOWN, Filters, Slot, activity, column as status_column};
use crate::team::PROPOSED;
use agents::Team;
use agents::team::state::RUNNING;
use agents::tickets::{ClosedWhy, Gate, State, Station, Status, Ticket};
use git::github::{Pr, PrState};
use std::cmp::Reverse;
```

Add after `failed`:

```rust
/// A ticket's place in its column: what waits on you, then what failed, then the rest, each oldest first.
pub(crate) fn rank(t: &Ticket) -> (u8, i64) {
    let group = match (waits(t), failed(t)) {
        (true, _) => 0,
        (_, true) => 1,
        _ => 2,
    };
    (group, t.changed_at)
}

/// Indices into the snapshot, per column of `COLUMNS`.
pub(crate) type Stations = [Vec<usize>; COLUMNS.len()];

/// The tickets the filters keep, per column and in `rank` order. Merged keeps its latest ones, newest first.
pub(crate) fn arrange<'a>(tickets: &[Ticket], filters: &Filters, team: impl Fn(&Ticket) -> Option<&'a Team>, pr: impl Fn(&Ticket) -> Option<&'a Pr>) -> Stations {
    let mut columns: Stations = Default::default();
    let keeps = filters.keeper();
    for (i, t) in tickets.iter().enumerate().filter(|(_, t)| on_line(t) && keeps(t)) {
        columns[column(t, team(t), pr(t)) as usize].push(i);
    }
    for (col, line) in COLUMNS.iter().zip(columns.iter_mut()) {
        if *col == Column::Merged {
            line.sort_by_key(|&i| Reverse(activity(&tickets[i])));
            line.truncate(FINISHED_SHOWN);
        } else {
            line.sort_by_key(|&i| rank(&tickets[i]));
        }
    }
    columns
}

/// The cards on the page in reading order: each column top to bottom, without a rail that is closed.
pub(crate) fn walk(columns: &Stations, open: &[bool; COLUMNS.len()], tickets: &[Ticket]) -> Vec<Slot> {
    let mut slots = Vec::new();
    for (column, line) in COLUMNS.iter().zip(columns) {
        if column.rail() && !open[*column as usize] {
            continue;
        }
        for (pos, &index) in line.iter().enumerate() {
            if let Some(t) = tickets.get(index) {
                slots.push(Slot { col: status_column(t), pos, index });
            }
        }
    }
    slots
}
```

**Step 4: Run** `cargo test -p pocket factory::station && cargo test -p pocket board::`

Expected: PASS. `factory::station` reports `14 passed`. The `board::` tests pass as before, because only two visibilities changed.

### Task 1.3: `note`, `asker`, `pr_number` and `progress`

**Files:**
- Modify: `packages/desktop/crates/pocket/src/factory/station.rs`

**Context:** B6 to B8 and US-03 fix the one line a card says for itself. `board::logic::Tone` (`logic.rs:711`) is the note's colour: `Asking` is amber, `Failed` is red. `crate::team::plural(n, one, many)` (`team.rs:22`) writes "1 task" and "3 tasks". An Ask is an `agents::Permission` in `Agents.pending`. Its `agent_id` is the lead (`Ticket.agent_id`) or a worker of the lead's Team (`TeamTask.worker`). `Permission::parts()` (`agents.rs:315`) returns the verb and the thing asked for. For a shell command the second part is the command. `crate::team::progress::counts(tasks).done` (`team/progress.rs:51`) counts the tasks that are `done` or `merged`. Plan 1 adds `Ticket.pr`, the PR's number as pocketd knows it (index, section "Wire").

**Step 1: Write the failing tests.** In `station.rs`, add `use agents::TeamTask;` to the imports of `mod tests`, and add at the bottom of the module:

```rust
    fn planned(n: usize, state: &str) -> Team {
        Team { tasks: (0..n).map(|i| TeamTask { id: format!("t{i}"), state: "planned".into(), ..TeamTask::default() }).collect(), ..team(state) }
    }

    fn ask(agent: &str, command: &str) -> Permission {
        Permission { agent_id: agent.into(), detail: agents::Detail { kind: "shell".into(), command: command.into(), ..Default::default() }, ..Default::default() }
    }

    #[test]
    fn a_ticket_at_the_plan_gate_reads_plan_ready_with_its_task_count() {
        let t = Ticket { station: Station::Plan, gate: Gate::Plan, ..live(Status::NeedsYou) };
        assert_eq!(note(&t, Some(&planned(3, "proposed")), None, None), Some(("Plan ready · 3 tasks".into(), Tone::Asking)));
        assert_eq!(note(&t, Some(&planned(1, "proposed")), None, None), Some(("Plan ready · 1 task".into(), Tone::Asking)));
        assert_eq!(note(&t, None, None, None), Some(("Plan ready".into(), Tone::Asking)));
    }

    #[test]
    fn a_ticket_with_an_ask_reads_ask_with_the_command() {
        let t = Ticket { station: Station::Implement, gate: Gate::Ask, ..live(Status::NeedsYou) };
        assert_eq!(note(&t, None, None, Some("cargo test -p daemon".into())), Some(("Ask · cargo test -p daemon".into(), Tone::Asking)));
        assert_eq!(note(&t, None, None, None), Some(("Ask".into(), Tone::Asking)));
    }

    #[test]
    fn a_ticket_at_the_pr_gate_reads_ready_to_merge_with_the_pr_number() {
        let t = Ticket { station: Station::Pr, gate: Gate::Pr, pr: 203, ..live(Status::NeedsYou) };
        assert_eq!(note(&t, None, None, None), Some(("Ready to merge · #203".into(), Tone::Asking)));
        let unnumbered = Ticket { pr: 0, ..t };
        assert_eq!(note(&unnumbered, None, Some(&pr(PrState::Open)), None), Some(("Ready to merge · #203".into(), Tone::Asking)));
        assert_eq!(note(&unnumbered, None, None, None), Some(("Ready to merge".into(), Tone::Asking)));
    }

    #[test]
    fn a_failed_ticket_reads_failed_with_the_reason() {
        let t = Ticket { station: Station::Checks, why: "cargo test failed twice".into(), ..live(Status::Failed) };
        assert_eq!(note(&t, None, None, None), Some(("Failed · cargo test failed twice".into(), Tone::Failed)));
        assert_eq!(note(&live(Status::Failed), None, None, None), Some(("Failed".into(), Tone::Failed)));
    }

    #[test]
    fn a_card_says_one_thing_and_a_working_ticket_says_nothing() {
        let both = Ticket { gate: Gate::Ask, why: "boom".into(), ..live(Status::Failed) };
        assert_eq!(note(&both, None, None, None), Some(("Ask".into(), Tone::Asking)));
        assert_eq!(note(&live(Status::Working), Some(&planned(3, "running")), Some(&pr(PrState::Open)), None), None);
        assert_eq!(note(&Ticket::sample(), None, None, None), None);
    }

    #[test]
    fn the_ask_of_a_ticket_is_its_leads_or_one_of_its_workers() {
        let pending = [ask("other", "ls"), ask("w-t1", "cargo test -p daemon"), ask("lead", "git status")];
        let mut running = planned(2, "running");
        running.tasks[1].worker = "w-t1".into();
        let t = live(Status::NeedsYou);
        assert_eq!(asker(&t, Some(&running), &pending).map(|p| p.parts().1), Some("cargo test -p daemon".to_string()));
        assert_eq!(asker(&t, None, &pending).map(|p| p.agent_id.as_str()), Some("lead"));
        assert!(asker(&Ticket { agent_id: "solo".into(), ..t }, None, &pending).is_none());
    }

    #[test]
    fn the_sub_task_count_shows_once_the_team_runs() {
        let mut running = planned(3, "running");
        (running.tasks[0].state, running.tasks[1].state) = ("done".into(), "merged".into());
        assert_eq!(progress(&running), Some("2/3".to_string()));
        assert_eq!(progress(&planned(3, "proposed")), None);
        assert_eq!(progress(&planned(0, "running")), None);
    }
```

**Step 2: Run** `cargo test -p pocket factory::station`

Expected: FAIL to compile. The errors name `cannot find function \`note\``, `cannot find function \`asker\``, `cannot find function \`progress\``, `cannot find type \`Permission\`` and `use of undeclared type \`Tone\``.

**Step 3: Implement.** In `station.rs`, the imports at the top become:

```rust
use crate::board::logic::{FINISHED_SHOWN, Filters, Slot, Tone, activity, column as status_column};
use crate::team::progress::counts as task_counts;
use crate::team::{PROPOSED, plural};
use agents::team::state::RUNNING;
use agents::tickets::{ClosedWhy, Gate, State, Station, Status, Ticket};
use agents::{Permission, Team};
use git::github::{Pr, PrState};
use std::cmp::Reverse;
```

Add after `walk`:

```rust
/// The PR's number: what pocketd wrote on the ticket, else the PR the desktop found for its worktree. 0 when there is none.
pub(crate) fn pr_number(t: &Ticket, pr: Option<&Pr>) -> u32 {
    if t.pr > 0 { t.pr } else { pr.map_or(0, |p| p.number) }
}

/// The one line a card says in the Factory layout: what waits on you, or that it failed. `ask` is the pending command.
pub(crate) fn note(t: &Ticket, team: Option<&Team>, pr: Option<&Pr>, ask: Option<String>) -> Option<(String, Tone)> {
    let text = match gate(t, team) {
        Gate::Plan => team.map_or_else(|| "Plan ready".to_string(), |team| format!("Plan ready · {}", plural(team.tasks.len(), "task", "tasks"))),
        Gate::Ask => ask.map_or_else(|| "Ask".to_string(), |command| format!("Ask · {command}")),
        Gate::Pr => match pr_number(t, pr) {
            0 => "Ready to merge".to_string(),
            n => format!("Ready to merge · #{n}"),
        },
        Gate::None if failed(t) && t.why.is_empty() => return Some(("Failed".to_string(), Tone::Failed)),
        Gate::None if failed(t) => return Some((format!("Failed · {}", t.why), Tone::Failed)),
        Gate::None => return None,
    };
    Some((text, Tone::Asking))
}

/// The Ask that holds the ticket: its lead's, or one of its team's workers'.
pub(crate) fn asker<'a>(t: &Ticket, team: Option<&Team>, pending: &'a [Permission]) -> Option<&'a Permission> {
    pending.iter().find(|p| p.agent_id == t.agent_id || team.is_some_and(|team| team.tasks.iter().any(|task| task.worker == p.agent_id)))
}

/// The tasks done out of the plan's, as `2/3`, once the team runs.
pub(crate) fn progress(team: &Team) -> Option<String> {
    let started = !team.tasks.is_empty() && !matches!(team.state.as_str(), "planning" | PROPOSED);
    started.then(|| format!("{}/{}", task_counts(&team.tasks).done, team.tasks.len()))
}
```

**Step 4: Run** `cargo test -p pocket factory::station`

Expected: PASS, `21 passed`.

### Task 1.4: `widths`: the columns fit at 1280px

**Files:**
- Modify: `packages/desktop/crates/pocket/src/factory/station.rs`

**Context:** B2 and B3 fix the sizes: two 44px rails, five middle columns of 196 to 248px, a 12px gap, and no sideways scroll at a 1280px window with the sidebar collapsed. The app's collapsed sidebar is `crate::desktop::chrome::RAIL`, 56px (`chrome.rs:42`), not the PRD's 48px (index, decision "Sidebar"). The Board page is as wide as the window less the sidebar. The prototype pads the columns' row by 12px on each side and opens a rail to 232px. The view (Task 3.3) passes the page's width to `widths` and sets each column to what it returns. The row scrolls sideways when the widths add up to more than the page.

**Step 1: Write the failing tests.** In `station.rs`, add `use crate::desktop::chrome::RAIL;` to the imports of `mod tests`, and add at the bottom of the module:

```rust
    const CLOSED: [bool; COLUMNS.len()] = [false; COLUMNS.len()];

    /// The width the columns take with their gaps and the row's padding.
    fn taken(widths: [f32; COLUMNS.len()]) -> f32 {
        widths.iter().sum::<f32>() + (COLUMNS.len() - 1) as f32 * GAP + 2. * PAD
    }

    #[test]
    fn all_seven_columns_fit_a_1280px_window_beside_the_collapsed_sidebar() {
        let page = 1280. - RAIL;
        let widths = widths(page, &CLOSED);
        assert_eq!(widths, [44., 208., 208., 208., 208., 208., 44.]);
        assert!(taken(widths) <= page);
    }

    #[test]
    fn a_middle_column_is_never_narrower_than_196px_so_a_small_page_scrolls_sideways() {
        let page = 1280. - 272.;
        let widths = widths(page, &CLOSED);
        assert_eq!(widths[Column::Implement as usize], COL_MIN);
        assert!(taken(widths) > page);
        assert_eq!(taken(widths), 1164., "the narrowest page that needs no scroll");
    }

    #[test]
    fn a_middle_column_is_never_wider_than_248px() {
        assert_eq!(widths(1424., &CLOSED)[Column::Spec as usize], COL_MAX);
        assert_eq!(widths(2560., &CLOSED)[Column::Spec as usize], COL_MAX);
    }

    #[test]
    fn an_open_rail_takes_232px_from_the_middle_columns() {
        let mut open = CLOSED;
        open[Column::Todo as usize] = true;
        let widths = widths(1384., &open);
        assert_eq!((widths[Column::Todo as usize], widths[Column::Plan as usize], widths[Column::Merged as usize]), (RAIL_OPEN, 202., RAIL_W));
        assert!(taken(widths) <= 1384.);
    }
```

**Step 2: Run** `cargo test -p pocket factory::station`

Expected: FAIL to compile. The errors name `cannot find function \`widths\`` and `cannot find value \`GAP\``.

**Step 3: Implement.** Add after `progress` in `station.rs`:

```rust
pub(crate) const RAIL_W: f32 = 44.;
pub(crate) const RAIL_OPEN: f32 = 232.;
pub(crate) const COL_MIN: f32 = 196.;
pub(crate) const COL_MAX: f32 = 248.;
pub(crate) const GAP: f32 = 12.;
/// The space on each side of the columns' row.
pub(crate) const PAD: f32 = 12.;

/// Each column's width on a page `available` wide. The middle columns share what the rails, the gaps and the padding leave.
pub(crate) fn widths(available: f32, open: &[bool; COLUMNS.len()]) -> [f32; COLUMNS.len()] {
    let rail = |c: Column| if open[c as usize] { RAIL_OPEN } else { RAIL_W };
    let rails: f32 = COLUMNS.into_iter().filter(|c| c.rail()).map(rail).sum();
    let middle = COLUMNS.iter().filter(|c| !c.rail()).count() as f32;
    let each = ((available - 2. * PAD - (COLUMNS.len() - 1) as f32 * GAP - rails) / middle).floor().clamp(COL_MIN, COL_MAX);
    COLUMNS.map(|c| if c.rail() { rail(c) } else { each })
}
```

The arithmetic for 1280px: the page is 1280 − 56 = 1224. The padding takes 24, the six gaps 72, the two rails 88. That leaves 1040, so each of the five middle columns is 208.

**Step 4: Run** `cargo test -p pocket factory::station`

Expected: PASS, `25 passed`.

**Step 5: Run the PR's checks** (from `packages/desktop`): `cargo build --workspace`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`

Expected: all green. Clippy adds no warning: `#[allow(dead_code)]` on `mod station` covers the functions that only tests call.

---

## PR 2: The pref, the filter and the state

**Scope:** `store::BoardLayout` replaces `board_list`, with the old key read once. `board::Layout` gains `Factory`, and `shown_layout` picks the layout the Board opens in. `station::Needs` and `station::counts`, with `Filters.needs` applied by `keeper`. `FactoryState` in `Desktop`, regrouped by `Desktop::board_changed` and by a Team or PR change. Inert: nothing sets `Layout::Factory` or `Filters.needs` until PR 3, so no screen changes and the stations are never regrouped.
**Depends on:** PR 1.
**Done when:** from `packages/desktop`, `cargo build --workspace`, `cargo clippy --workspace --all-targets` (no new warnings) and `cargo test --workspace` pass.

### Task 2.1: `store::BoardLayout` and `Layout::Factory`

**Files:**
- Modify: `packages/desktop/crates/store/src/store.rs:332` (new enum before `Layout`), `:389-390` (the field), `:435-440` (`Store::load`), tests
- Modify: `packages/desktop/crates/pocket/src/board.rs:35-40` (`Layout`), `:187` (`open_board`), new tests at the bottom
- Modify: `packages/desktop/crates/pocket/src/board/display_menu.rs:44`

**Context:** `Store` (`store.rs:345`) is `desktop.json`. It derives `Serialize` and `Deserialize` with `#[serde(default)]`, and `Store::load` forgets everything when one field fails to parse. So a field whose value may be unknown to this build is read with `lenient` (`store.rs:596`), which falls back to the default. `board_list: bool` (`store.rs:390`) is true when the Board shows its list. It has three users: `board.rs:187` reads it, `display_menu.rs:44` writes it, and `store.rs` holds it. The index replaces it with `board_layout: BoardLayout { Factory, Board, List }` and asks that the old key be read once. After the first `save`, `desktop.json` no longer holds `board_list`, because the field is gone. Plan 1 adds `Agents::factory_offered()` (index, section "Desktop crates").

**Step 1: Write the failing tests.** In `store.rs`, add to the bottom of `mod tests`:

```rust
    fn loaded(name: &str, json: &str) -> Store {
        let dir = std::env::temp_dir().join(format!("pocket-store-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("desktop.json"), json).unwrap();
        let s = Store::load(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        s
    }

    #[test]
    fn an_old_board_list_pref_opens_the_list_layout() {
        assert_eq!(loaded("board-list", r#"{"board_list":true}"#).board_layout, BoardLayout::List);
        assert_eq!(loaded("board-columns", r#"{"board_list":false}"#).board_layout, BoardLayout::Board);
        assert_eq!(loaded("board-unset", r#"{"projects":["/w"]}"#).board_layout, BoardLayout::Board);
    }

    #[test]
    fn the_board_layout_key_wins_over_the_old_one() {
        assert_eq!(loaded("board-both", r#"{"board_list":true,"board_layout":"factory"}"#).board_layout, BoardLayout::Factory);
        assert_eq!(loaded("board-new", r#"{"board_layout":"list"}"#).board_layout, BoardLayout::List);
    }

    #[test]
    fn a_save_writes_the_layout_and_drops_the_old_key() {
        let dir = std::env::temp_dir().join(format!("pocket-store-board-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("desktop.json"), r#"{"board_list":true}"#).unwrap();
        Store::load(&dir).save();
        let raw = std::fs::read_to_string(dir.join("desktop.json")).unwrap();
        assert!(raw.contains(r#""board_layout": "list""#) && !raw.contains("board_list"), "{raw}");
        assert_eq!(Store::load(&dir).board_layout, BoardLayout::List);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_board_layout_this_build_does_not_know_opens_the_status_layout() {
        let s = loaded("board-newer", r#"{"projects":["/w"],"board_layout":"timeline"}"#);
        assert_eq!((s.board_layout, s.projects.len()), (BoardLayout::Board, 1));
    }
```

In `pocket/src/board.rs`, add at the bottom of the file:

```rust
#[cfg(test)]
mod tests {
    use super::{Layout, shown_layout};
    use store::BoardLayout;

    #[test]
    fn the_board_opens_in_the_saved_layout_and_in_status_when_the_factory_is_not_offered() {
        assert_eq!(shown_layout(BoardLayout::Factory, true), Layout::Factory);
        assert_eq!(shown_layout(BoardLayout::Factory, false), Layout::Board);
        assert_eq!(shown_layout(BoardLayout::List, false), Layout::List);
        assert_eq!(shown_layout(BoardLayout::Board, true), Layout::Board);
    }

    #[test]
    fn a_layout_reads_back_from_the_pref_it_was_saved_as() {
        for layout in [Layout::Factory, Layout::Board, Layout::List] {
            assert_eq!(Layout::from(BoardLayout::from(layout)), layout);
        }
    }
}
```

**Step 2: Run** `cargo test -p store board`

Expected: FAIL to compile, with `cannot find type \`BoardLayout\`` (or `failed to resolve: use of undeclared type`) and `no field \`board_layout\` on type \`Store\``.

**Step 3: Implement.** In `store.rs`, add before `pub enum Layout` (`:332`):

```rust
/// How the Board lays its tickets out. Board is the Status layout.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum BoardLayout {
    Factory,
    #[default]
    Board,
    List,
}
```

`store.rs:389-390`, before:

```rust
    /// The Board shows its list rather than its columns.
    pub board_list: bool,
```

After:

```rust
    #[serde(deserialize_with = "lenient")]
    pub board_layout: BoardLayout,
```

`store.rs:435-440`, before:

```rust
    pub fn load(home: &Path) -> Self {
        let path = home.join("desktop.json");
        let mut s: Self = std::fs::read(&path).ok().and_then(|raw| serde_json::from_slice(&raw).ok()).unwrap_or_default();
        s.path = path;
        s
    }
```

After:

```rust
    pub fn load(home: &Path) -> Self {
        let path = home.join("desktop.json");
        let mut s = std::fs::read(&path).ok().and_then(|raw| Self::parse(&raw)).unwrap_or_default();
        s.path = path;
        s
    }

    /// A `desktop.json` from before `board_layout` says List with `board_list`.
    fn parse(raw: &[u8]) -> Option<Self> {
        let json: serde_json::Value = serde_json::from_slice(raw).ok()?;
        let listed = json.get("board_layout").is_none() && json.get("board_list").and_then(serde_json::Value::as_bool) == Some(true);
        let mut s: Self = serde_json::from_value(json).ok()?;
        if listed {
            s.board_layout = BoardLayout::List;
        }
        Some(s)
    }
```

`Store::load` runs once at launch, so the extra pass through `serde_json::Value` costs nothing a frame pays for.

In `pocket/src/board.rs`, `:35-40`, before:

```rust
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub(crate) enum Layout {
    #[default]
    Board,
    List,
}
```

After:

```rust
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub(crate) enum Layout {
    Factory,
    #[default]
    Board,
    List,
}

impl From<BoardLayout> for Layout {
    fn from(saved: BoardLayout) -> Self {
        match saved {
            BoardLayout::Factory => Layout::Factory,
            BoardLayout::Board => Layout::Board,
            BoardLayout::List => Layout::List,
        }
    }
}

impl From<Layout> for BoardLayout {
    fn from(layout: Layout) -> Self {
        match layout {
            Layout::Factory => BoardLayout::Factory,
            Layout::Board => BoardLayout::Board,
            Layout::List => BoardLayout::List,
        }
    }
}

/// The layout the Board opens in: the saved one, or Status when Factory was saved and pocketd does not offer it.
pub(crate) fn shown_layout(saved: BoardLayout, factory: bool) -> Layout {
    match Layout::from(saved) {
        Layout::Factory if !factory => Layout::Board,
        layout => layout,
    }
}
```

Add the import to `board.rs`, after `use std::collections::HashSet;`:

```rust
use store::BoardLayout;
```

`board.rs:187`, before:

```rust
        self.board.layout = if self.store.board_list { Layout::List } else { Layout::Board };
```

After:

```rust
        self.board.layout = shown_layout(self.store.board_layout, self.agents.factory_offered());
```

`display_menu.rs:44`, before:

```rust
                    this.store.board_list = l == Layout::List;
```

After:

```rust
                    this.store.board_layout = l.into();
```

**Step 4: Run** `cargo test -p store && cargo test -p pocket board::tests && cargo build --workspace`

Expected: PASS. `store` reports the four new tests among the passed ones, and `round_trips_through_desktop_json` still passes. `board::tests` reports `2 passed`. The build has no error: `grep -rn "board_list" crates --include='*.rs'` now finds only the function `board_list` in `board/row.rs` and its call in `board/page.rs`, and the string in the `store.rs` tests and in `Store::parse`.

### Task 2.2: `Needs`, `counts` and `Filters.needs`

**Files:**
- Modify: `packages/desktop/crates/pocket/src/factory/station.rs`
- Modify: `packages/desktop/crates/pocket/src/board/logic.rs:273-281` (`Filters`), `:329-337` (`keeper`), tests
- Modify: `packages/desktop/crates/pocket/src/board/views.rs:22-29` (`filters_of`), `:95` (the test helper `working`)

**Context:** B9 and US-04 ask for three chips with counts, `waiting on you`, `ask` and `failed`, and the slots in use. A click on a chip keeps only the matching Tickets, a second click clears it, and the filter applies to every layout. `Filters` (`logic.rs:274`) is the Board's filter state. `keeper` is the one place that decides whether a Ticket stays, and `arrange`, `hidden_by` and `station::arrange` all call it. So the chip is one more field of `Filters`. `Filters::any()` counts only the fields of the Filter menu and stays as it is. Two places write a `Filters` literal with every field: `views.rs:22` and `views.rs:95`. `clear_board_filters` (`filter_bar.rs:253`) sets `Filters::default()`, so it clears the chip too. Plan 1 makes `board::logic::busy` count the Tickets that hold a slot: live with status `starting` or `working` (index, section "Desktop crates").

**Step 1: Write the failing tests.** In `station.rs`, add at the bottom of `mod tests`:

```rust
    #[test]
    fn the_chips_count_what_waits_on_you_what_asks_and_what_failed() {
        let tickets = [
            Ticket { gate: Gate::Plan, ..live(Status::NeedsYou) },
            Ticket { gate: Gate::Ask, ..live(Status::NeedsYou) },
            Ticket { gate: Gate::Pr, ..live(Status::NeedsYou) },
            live(Status::NeedsYou),
            live(Status::Failed),
            live(Status::Working),
            live(Status::Starting),
            Ticket::sample(),
        ];
        let c = counts(&tickets);
        assert_eq!(c, Counts { you: 4, ask: 2, failed: 1, slots: 2 });
        assert_eq!(Needs::ALL.map(|n| c.of(n)), [4, 2, 1]);
        assert_eq!(Needs::ALL.map(Needs::name), ["waiting on you", "ask", "failed"]);
    }

    #[test]
    fn a_second_click_on_a_chip_clears_it_and_another_chip_replaces_it() {
        assert_eq!(Needs::toggled(None, Needs::You), Some(Needs::You));
        assert_eq!(Needs::toggled(Some(Needs::You), Needs::You), None);
        assert_eq!(Needs::toggled(Some(Needs::You), Needs::Failed), Some(Needs::Failed));
    }

    #[test]
    fn a_chip_keeps_only_its_tickets_in_the_columns() {
        let tickets = [Ticket { station: Station::Plan, gate: Gate::Plan, ..live(Status::NeedsYou) }, Ticket { station: Station::Plan, ..live(Status::Working) }, Ticket { station: Station::Checks, ..live(Status::Failed) }];
        let only = |needs| arrange(&tickets, &Filters { needs: Some(needs), ..Filters::default() }, |_| None, |_| None);
        assert_eq!(only(Needs::You), [vec![], vec![], vec![0], vec![], vec![], vec![], vec![]]);
        assert_eq!(only(Needs::Ask), Stations::default());
        assert_eq!(only(Needs::Failed)[Column::Checks as usize], [2]);
    }
```

In `board/logic.rs`, add to the bottom of `mod tests`:

```rust
    #[test]
    fn the_waiting_on_you_chip_filters_the_status_layout_too() {
        let tickets = [ticket("a", State::Live, Status::NeedsYou), ticket("b", State::Live, Status::Working), ticket("c", State::Live, Status::Failed)];
        let you = Filters { needs: Some(Needs::You), ..Filters::default() };
        assert_eq!(hidden_by(&tickets, &you, Tab::All, false), 2);
        assert_eq!(arrange(&tickets, &you, Order::Activity)[Col::NeedsYou as usize], [0]);
        assert!(!you.any(), "the chip is not one of the Filter menu's fields");
        let failed = Filters { needs: Some(Needs::Failed), ..Filters::default() };
        assert_eq!(arrange(&tickets, &failed, Order::Activity)[Col::NeedsYou as usize], [2]);
    }
```

**Step 2: Run** `cargo test -p pocket factory::station`

Expected: FAIL to compile. The errors name `cannot find function \`counts\``, `use of undeclared type \`Needs\`` and `struct \`Filters\` has no field named \`needs\``.

**Step 3: Implement.** In `station.rs`, add `busy` to the import from `crate::board::logic`:

```rust
use crate::board::logic::{FINISHED_SHOWN, Filters, Slot, Tone, activity, busy, column as status_column};
```

Add after `widths`:

```rust
/// What a filter chip of the Factory layout keeps.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Needs {
    You,
    Ask,
    Failed,
}

impl Needs {
    pub(crate) const ALL: [Needs; 3] = [Needs::You, Needs::Ask, Needs::Failed];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Needs::You => "waiting on you",
            Needs::Ask => "ask",
            Needs::Failed => "failed",
        }
    }

    pub(crate) fn keeps(self, t: &Ticket) -> bool {
        match self {
            Needs::You => waits(t),
            Needs::Ask => gate(t, None) == Gate::Ask,
            Needs::Failed => failed(t),
        }
    }

    /// The chip that is on after a click on `clicked`: a second click on the same chip clears it.
    pub(crate) fn toggled(current: Option<Needs>, clicked: Needs) -> Option<Needs> {
        (current != Some(clicked)).then_some(clicked)
    }
}

/// What the chips say, over every ticket whatever the filters hide.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub(crate) struct Counts {
    pub(crate) you: usize,
    pub(crate) ask: usize,
    pub(crate) failed: usize,
    /// The tickets that hold a slot.
    pub(crate) slots: usize,
}

impl Counts {
    pub(crate) fn of(&self, needs: Needs) -> usize {
        match needs {
            Needs::You => self.you,
            Needs::Ask => self.ask,
            Needs::Failed => self.failed,
        }
    }
}

pub(crate) fn counts(tickets: &[Ticket]) -> Counts {
    let n = |needs: Needs| tickets.iter().filter(|t| needs.keeps(t)).count();
    Counts { you: n(Needs::You), ask: n(Needs::Ask), failed: n(Needs::Failed), slots: busy(tickets) }
}
```

In `board/logic.rs`, add the import after `use agents::tickets::{ClosedWhy, State, Status, Ticket};`:

```rust
use crate::factory::station::Needs;
```

The test module's `use super::*;` brings `Needs` into the test from Step 1.

`logic.rs:273-281`, before:

```rust
    pub(crate) label: Choice<String>,
    pub(crate) query: String,
}
```

After:

```rust
    pub(crate) label: Choice<String>,
    pub(crate) query: String,
    /// The Factory layout's chip that is on.
    pub(crate) needs: Option<Needs>,
}
```

`logic.rs:329-337`, in `keeper`, before:

```rust
            && self.label.keeps(|l| t.labels.contains(l))
            && (query.is_empty() || t.title.to_lowercase().contains(&query) || crate::util::basename(&t.worktree).to_lowercase().contains(&query))
```

After:

```rust
            && self.label.keeps(|l| t.labels.contains(l))
            && self.needs.is_none_or(|n| n.keeps(t))
            && (query.is_empty() || t.title.to_lowercase().contains(&query) || crate::util::basename(&t.worktree).to_lowercase().contains(&query))
```

`views.rs:22-29`, in `filters_of`, before:

```rust
        label: Choice { values: view.label.clone(), not: not(FilterCat::Label) },
        query,
    }
```

After:

```rust
        label: Choice { values: view.label.clone(), not: not(FilterCat::Label) },
        query,
        needs: None,
    }
```

`views.rs:95`, in the test helper `working`, before:

```rust
label: one("Bug"), query: String::new() }
```

After:

```rust
label: one("Bug"), query: String::new(), needs: None }
```

A saved view keeps no chip. A view reads as "on" only while no chip is set, because `view_is_on` compares the whole `Filters`.

**Step 4: Run** `cargo test -p pocket factory::station && cargo test -p pocket board::`

Expected: PASS. `factory::station` reports `28 passed`. `board::` passes with `the_waiting_on_you_chip_filters_the_status_layout_too` among the passed tests.

### Task 2.3: `FactoryState`, composed in `Desktop::new` and regrouped with the Board

**Files:**
- Modify: `packages/desktop/crates/pocket/src/factory.rs`
- Modify: `packages/desktop/crates/pocket/src/desktop.rs:18` (import), `:113` (field), `:159`, `:202`, `:250` (`Desktop::new`)
- Modify: `packages/desktop/crates/pocket/src/board.rs:134-152` (`regroup`), `:197-203` (`board_changed`), tests
- Modify: `packages/desktop/crates/pocket/src/board/selection.rs:54`, `:92`
- Modify: `packages/desktop/crates/pocket/src/board/bulk_bar.rs:28`, `:71`
- Modify: `packages/desktop/crates/pocket/src/desktop/alerts.rs:206`
- Modify: `packages/desktop/crates/pocket/src/git_ui/pull_requests.rs:274-277`

**Context:** ADR 0003: a feature's state is its own struct, its `new` returns its own subscriptions, and `Desktop::new` only composes. `BoardState::new` (`board.rs:90`) is the pattern. The Board redraws on every notify, so the grouping must not happen in `render`. `BoardState::regroup` (`board.rs:134`) runs from `Desktop::board_changed` (`board.rs:198`), which runs on a `board` frame, on a filter change and when the Board opens. The Factory layout also places a Ticket by its Team and its PR. A Team arrives as `Event::Team` or `Event::Teams` in `on_agents_from` (`alerts.rs:126`), and a PR in `poll_prs` (`pull_requests.rs:274`). Both must regroup, and only while the Factory layout shows. `Agents::team(lead)` (`agents.rs:497`) and `PullRequests::get(tree)` (`pull_requests.rs:79`) are the two lookups. The last three lines of `regroup` drop the cursor and the picks that left the page. They read `walk_slots`, which knows only the Status and List layouts, so they move to a function that takes the slots. `hidden_by` counts the Tickets the filters hide on the current tab. The Factory layout has no tabs. This task holds one decision, `hidden_scope`. The rest is glue.

**Step 1: Write the failing test.** In `board.rs`, the imports of `mod tests` become:

```rust
    use super::logic::Tab;
    use super::{Layout, hidden_scope, shown_layout};
    use store::BoardLayout;
```

Add to `mod tests`:

```rust
    #[test]
    fn the_factory_layout_counts_what_the_filters_hide_on_every_tab_and_never_a_closed_ticket() {
        assert_eq!(hidden_scope(Layout::Factory, Tab::Todo, true), (Tab::All, false));
        assert_eq!(hidden_scope(Layout::Board, Tab::Todo, true), (Tab::Todo, true));
        assert_eq!(hidden_scope(Layout::List, Tab::Active, false), (Tab::Active, false));
    }
```

**Step 2: Run** `cargo test -p pocket board::tests`

Expected: FAIL to compile, with `unresolved import \`super::hidden_scope\``.

**Step 3: Implement.** Replace `factory.rs` with:

```rust
#[allow(dead_code)]
pub(crate) mod station;

use crate::board::Layout;
use crate::board::logic::Filters;
use crate::desktop::Desktop;
use agents::Team;
use agents::tickets::Ticket;
use git::github::Pr;
use gpui_kit::*;
use station::{COLUMNS, Stations, arrange};

pub(crate) struct FactoryState {
    /// Indices into the snapshot, per column of `COLUMNS`.
    pub(crate) columns: Stations,
    pub(crate) lists: Vec<ListState>,
    /// The rails drawn as full columns.
    pub(crate) open: [bool; COLUMNS.len()],
}

impl FactoryState {
    pub(crate) fn new(_: &mut Window, _: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let state = Self { columns: Default::default(), lists: COLUMNS.iter().map(|_| ListState::new(0, ListAlignment::Top, px(400.))).collect(), open: Default::default() };
        (state, Vec::new())
    }

    pub(crate) fn regroup<'a>(&mut self, tickets: &[Ticket], filters: &Filters, team: impl Fn(&Ticket) -> Option<&'a Team>, pr: impl Fn(&Ticket) -> Option<&'a Pr>) {
        self.columns = arrange(tickets, filters, team, pr);
        // A ticket's height follows its content, so the lists measure afresh whenever the snapshot changes.
        for (list, line) in self.lists.iter().zip(&self.columns) {
            let top = list.logical_scroll_top();
            list.reset(line.len());
            list.scroll_to(top);
        }
    }
}

impl Desktop {
    pub(crate) fn regroup_stations(&mut self) {
        let (agents, prs) = (&self.agents, &self.prs);
        self.factory.regroup(&agents.board.tickets, &self.board.filters, |t| agents.team(&t.agent_id), |t| prs.get(&t.worktree));
    }

    /// A team or a PR changed. Only the Factory layout places a ticket by them.
    pub(crate) fn stations_changed(&mut self) {
        if self.board.layout == Layout::Factory {
            self.board_changed();
        }
    }
}
```

`FactoryState::new` returns no subscription yet. It keeps the signature that ADR 0003 asks for, and plan 3 adds the page's inputs to it.

In `desktop.rs`, add the import after `use crate::explorer::preview::PreviewState;` (`:18`):

```rust
use crate::factory::FactoryState;
```

Add the field after `pub(crate) board: BoardState,` (`:113`):

```rust
    pub(crate) factory: FactoryState,
```

In `Desktop::new`, after `let (board, board_subs) = BoardState::new(window, cx);` (`:159`):

```rust
        let (factory, factory_subs) = FactoryState::new(window, cx);
```

After `_subs.extend(board_subs);` (`:202`):

```rust
        _subs.extend(factory_subs);
```

In the struct literal, after `board,` (`:250`):

```rust
            factory,
```

In `board.rs`, add after `shown_layout` (`Tab`, `Slot` and `Ticket` are already imported):

```rust
/// The tab and the closed tickets that the hidden count runs over. The Factory layout has no tabs and never shows a ticket that closed without a merge.
fn hidden_scope(layout: Layout, tab: Tab, show_closed: bool) -> (Tab, bool) {
    if layout == Layout::Factory { (Tab::All, false) } else { (tab, show_closed) }
}
```

`board.rs:134-152`, `regroup`. Before, its second line:

```rust
        self.hidden = hidden_by(&board.tickets, &self.filters, self.tab, self.show_closed);
```

After:

```rust
        let (tab, show_closed) = hidden_scope(self.layout, self.tab, self.show_closed);
        self.hidden = hidden_by(&board.tickets, &self.filters, tab, show_closed);
```

Before, its last lines:

```rust
        self.rows_list.scroll_to(top);
        let slots = self.walk_slots();
        let ids = |finished: bool| -> HashSet<&str> { slots.iter().filter(|s| finished || !s.col.finished()).filter_map(|s| board.tickets.get(s.index)).map(|t| t.id.as_str()).collect() };
        self.selection.keep(&ids(true), &ids(false));
    }
```

After:

```rust
        self.rows_list.scroll_to(top);
    }

    /// Forgets the cursor and the picks that are no longer among `slots`, the cards on the page.
    fn keep_selection(&mut self, slots: &[Slot], tickets: &[Ticket]) {
        let ids = |finished: bool| -> HashSet<&str> { slots.iter().filter(|s| finished || !s.col.finished()).filter_map(|s| tickets.get(s.index)).map(|t| t.id.as_str()).collect() };
        self.selection.keep(&ids(true), &ids(false));
    }
```

`board.rs:197-203`, before:

```rust
    /// Off the screen nothing reads the columns; opening it regroups.
    pub(crate) fn board_changed(&mut self) {
        if self.screen == Screen::Board {
            self.board.regroup(&self.agents.board);
            self.drop_stale_ticket_page();
        }
    }
```

After:

```rust
    /// Off the screen nothing reads the columns; opening it regroups.
    pub(crate) fn board_changed(&mut self) {
        if self.screen == Screen::Board {
            self.board.regroup(&self.agents.board);
            if self.board.layout == Layout::Factory {
                self.regroup_stations();
            }
            let slots = self.board_slots();
            self.board.keep_selection(&slots, &self.agents.board.tickets);
            self.drop_stale_ticket_page();
        }
    }

    /// The cards on the page in reading order, whatever the layout.
    fn board_slots(&self) -> Vec<Slot> {
        if self.board.layout == Layout::Factory {
            return crate::factory::station::walk(&self.factory.columns, &self.factory.open, &self.agents.board.tickets);
        }
        self.board.walk_slots()
    }
```

The four callers of `walk_slots` outside `board.rs` now ask `board_slots`. `selection.rs:54`, before:

```rust
        let slots = self.board.walk_slots();
```

After:

```rust
        let slots = self.board_slots();
```

`selection.rs:92`, before:

```rust
        let ids = self.board.walk_slots().into_iter().filter(|s| !s.col.finished()).filter_map(|s| tickets.get(s.index)).map(|t| t.id.clone());
```

After:

```rust
        let ids = self.board_slots().into_iter().filter(|s| !s.col.finished()).filter_map(|s| tickets.get(s.index)).map(|t| t.id.clone());
```

`bulk_bar.rs:28`, before:

```rust
        bulk_targets(&self.board.walk_slots(), &self.agents.board.tickets, &self.board.selection.picked, action).into_iter().map(|t| t.id.clone()).collect()
```

After:

```rust
        bulk_targets(&self.board_slots(), &self.agents.board.tickets, &self.board.selection.picked, action).into_iter().map(|t| t.id.clone()).collect()
```

`bulk_bar.rs:71`, before:

```rust
        let slots: Vec<_> = self.board.walk_slots().into_iter().filter(|s| tickets.get(s.index).is_some_and(|t| picked.contains(&t.id))).collect();
```

After:

```rust
        let slots: Vec<_> = self.board_slots().into_iter().filter(|s| tickets.get(s.index).is_some_and(|t| picked.contains(&t.id))).collect();
```

`alerts.rs:206`, before:

```rust
        let board = matches!(ev, Event::Board { .. });
```

After:

```rust
        let board = matches!(ev, Event::Board { .. }) || (self.board.layout == Layout::Factory && matches!(ev, Event::Team(_) | Event::Teams(_)));
```

Add the import to `alerts.rs`, before `use crate::desktop::Desktop;`:

```rust
use crate::board::Layout;
```

`pull_requests.rs:274-277`, before:

```rust
                if d.prs.apply(tree, started, result, Instant::now()) {
                    d.refresh_pr_diffs(cx);
                    cx.notify();
                }
```

After:

```rust
                if d.prs.apply(tree, started, result, Instant::now()) {
                    d.refresh_pr_diffs(cx);
                    d.stations_changed();
                    cx.notify();
                }
```

What a frame costs: nothing new. `regroup_stations` is O(n log n) over the snapshot, with two hash lookups per Ticket. It runs only in the Factory layout, on a `board` frame, a Team change, a PR change, a filter change, a layout change or a rail toggle. The daemon poll, the git refresh, terminal output and hover never reach it.

**Step 4: Run** `cargo test -p pocket board:: && cargo build --workspace`

Expected: PASS, with `board::tests` reporting `3 passed`. The build has no error and no new warning.

**Step 5: Run the PR's checks** (from `packages/desktop`): `cargo build --workspace`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`

Expected: all green. Clippy adds no warning. Open the app's Board (or run `.ui-review/fixture/capture.sh /tmp/lf-2/pr2 board=board` from the repository root): it looks as before, because nothing sets `Layout::Factory` yet.

---

## PR 3: The Factory layout

**Scope:** The fixture's Factory Tickets and the `factory.v1` cap. The icon `status-quarter`, `parts::station_mark` and `parts::station_glyph`. The card in the Factory layout (glyph, note, pills, no answer buttons, no drag). `board/stations.rs` with the columns and the rails, drawn by `board_page`. The segmented control **Factory · Status · List** in the top bar, which replaces the two tiles of the Display menu. The chips `waiting on you`, `ask`, `failed` and the slots text. The capture steps `board-factory`, `board-factory-todo`, `board-factory-you` and `board-many`. The frame timing that US-38 asks for. The ADR 0003 map.
**Depends on:** PR 2. pocketd's plan 1, PR 3, for a pocketd that serves `factory.v1`. The captures need only the fixture.
**Done when:** the checks of Task 3.6 pass, and its captures match what each task describes.

### Task 3.1: Factory Tickets in the fixture, and the "before" captures

**Files:**
- Modify: `.ui-review/fixture/fixture.ts:276` (after `TICKETS`), `:404` (the `hello.ok` caps), `:407` (the `board` frame)

**Context:** `capture.sh` runs the release app against `fixture.ts`. A name that starts with `board` picks the `sessions` scenario, whose agents include `ios` (an Ask for `xcodebuild test`) and `hook` (running). `ticket(id, title, state, status, ago, extra)` (`fixture.ts:261`) builds a Ticket: `number` comes from the id, so an id is a letter and a number. `changedAt` is `ago` minutes back, so a larger `ago` is older. The `hello.ok` frame lists its caps by hand (`fixture.ts:404`) and has no `factory.v1`, so `Agents::factory_offered()` is false in every capture until this task adds it. Edit the fixture first, then capture "before" with today's code. The "before" and "after" screens then differ only by the code of this plan.

**Step 1: Add the Factory Tickets.** After the closing `];` of `TICKETS` (`fixture.ts:276`), add:

```ts
// Several per station, one at each gate, one failed. Plan 2's capture step gives `plan-lead` a proposed Team of three tasks.
const FACTORY_TICKETS = [
  ticket("t13", "Spec the offline send queue", "live", "working", 6, { station: "spec", worktree: "/tmp/wt/offline-queue" }),
  ticket("t14", "Spec the share sheet", "live", "starting", 2, { provider: "codex", station: "spec", worktree: "/tmp/wt/share-sheet" }),
  ticket("t15", "Split the settings store by section", "live", "needsYou", 12, { station: "plan", gate: "plan", agentId: "plan-lead", worktree: "/tmp/wt/settings-split", priority: 2 }),
  ticket("t16", "Plan drag to reorder for tabs", "live", "working", 5, { station: "plan", worktree: "/tmp/wt/tab-reorder" }),
  ticket("t17", "Stream the diff into the review pane", "live", "working", 9, { station: "implement", agentId: "hook", terminalId: "hook", worktree: "/tmp/wt/stream-diff", labels: ["Feature"] }),
  ticket("t18", "Run the iOS UI tests on every push", "live", "needsYou", 14, { provider: "codex", station: "implement", gate: "ask", agentId: "ios", terminalId: "ios", worktree: "/tmp/wt/ios-ui-tests", labels: ["Infra"] }),
  ticket("t19", "Cache the syntax themes", "live", "working", 8, { station: "checks", worktree: "/tmp/wt/syntax-cache" }),
  ticket("t20", "Paste files into the composer", "live", "failed", 20, { station: "checks", fix: 2, why: "cargo test failed in composer.rs", worktree: "/tmp/wt/paste-files", labels: ["Bug"], priority: 1 }),
  ticket("t21", "Batch the git status reads", "live", "working", 7, { provider: "codex", station: "checks", fix: 1, worktree: "/tmp/wt/batch-status" }),
  ticket("t22", "Show the PR checks on the card", "live", "needsYou", 16, { station: "pr", gate: "pr", pr: 203, worktree: "/tmp/wt/pr-checks" }),
  ticket("t23", "Keep the scroll place per tab", "live", "working", 4, { station: "pr", pr: 202, worktree: "/tmp/wt/scroll-place" }),
  ticket("t24", "Fold long tool output", "closed", "idle", 30, { station: "merged", pr: 198, closedWhy: "merged", closedAt: now - 30 * MIN, worktree: "/tmp/wt/fold-output" }),
  ticket("t25", "Name worktrees after the ticket", "closed", "idle", 60, { provider: "codex", station: "merged", pr: 197, closedWhy: "merged", closedAt: now - 60 * MIN, worktree: "/tmp/wt/name-trees" }),
];
```

**Step 2: Offer the Factory and send the Tickets.** `fixture.ts:404`, in the `hello.ok` frame, before:

```ts
caps: ["pair.v1", "scopes.v1", "summary.v2", "host.v1", "automations.v1", "board.v1", "locals.v1", "settings.v1", "access.v1", "team.v1"],
```

After:

```ts
caps: ["pair.v1", "scopes.v1", "summary.v2", "host.v1", "automations.v1", "board.v1", "locals.v1", "settings.v1", "access.v1", "team.v1", "factory.v1"],
```

`fixture.ts:407`, before:

```ts
        ws.send(JSON.stringify({ type: "board", tickets: TICKETS, limit: 4 }));
```

After:

```ts
        ws.send(JSON.stringify({ type: "board", tickets: [...TICKETS, ...FACTORY_TICKETS], limit: 4 }));
```

The limit stays 4. The slots text then reads above the limit (`9/4 slots`), which US-23 allows: a Ticket that resumes after a gate may take the count above the limit.

**Step 3: Capture "before"** (from the repository root, with the code of PR 2 and nothing of PR 3):

```bash
.ui-review/fixture/capture.sh /tmp/lf-2/before board=board board-list=board,board-list board-display=board,board-display
```

Expected: three paths printed, `/tmp/lf-2/before/impl-board.png`, `impl-board-list.png` and `impl-board-display.png`. `impl-board` is the Status layout with the 13 new Tickets in its columns. `impl-board-display` shows the Display menu with the two layout tiles **List** and **Board** at its top.

### Task 3.2: The icon `status-quarter`, `station_mark` and `station_glyph`

**Files:**
- Create: `packages/desktop/crates/theme/assets/icons/status-quarter.svg`
- Modify: `packages/desktop/crates/theme/src/theme.rs:625` (`ICONS`)
- Modify: `packages/desktop/crates/pocket/src/board.rs:14` (`mod parts`)
- Modify: `packages/desktop/crates/pocket/src/board/parts.rs` (new functions, five functions widened, tests)

**Context:** The prototype's `GLYPH` table (`app.js:157`) gives each station a mark. A Ticket at a gate shows the needs-you mark instead, and a failed Ticket the closed mark (`ticketGlyph`). Every mark but one is an icon the app already has. The Plan station's mark, `status-quarter`, is defined in `app.js:157`, not in the prototype's `icons.js`. `theme::icon(name, size, color)` reads `icons/<name>.svg` from `ICONS`, the list that `embed!` compiles into the binary (`theme.rs:622`). An icon missing from `ICONS` draws nothing and raises no error, so a test loads each mark through `theme::Assets`, the `AssetSource` that GPUI reads. The index makes `station_glyph` and the parts that plans 3 and 7 draw `pub(crate)`: plan 3 checks for `station_glyph`, `prop_chip`, `agent_avatar`, `bar_button` and `start_kbd`, and plan 7 for `pub(crate) mod parts`. The index keeps the status colours unchanged: Merged keeps `INDIGO` on its glyph.

**Step 1: Write the failing tests.** In `board/parts.rs`, add at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::station::{COLUMNS, Column};
    use agents::tickets::Gate;

    #[test]
    fn each_station_has_the_prototypes_mark_and_colour() {
        let marks = COLUMNS.map(|c| station_mark(c, Gate::None, false));
        assert_eq!(marks, [("status-todo", TEXT_4), ("status-queued", WORKING), ("status-quarter", WORKING), ("status-half", WORKING), ("status-three-quarters", SUCCESS), ("status-full", GOAL_TEXT), ("status-merged", INDIGO)]);
    }

    #[test]
    fn a_ticket_at_a_gate_shows_the_needs_you_mark_before_a_failure() {
        assert_eq!(station_mark(Column::Plan, Gate::Plan, false), ("status-needs", WAITING_DOT));
        assert_eq!(station_mark(Column::Checks, Gate::Ask, true), ("status-needs", WAITING_DOT));
        assert_eq!(station_mark(Column::Checks, Gate::None, true), ("status-closed", FAILED));
    }

    #[test]
    fn every_station_mark_is_an_embedded_icon() {
        for column in COLUMNS {
            for (gate, failed) in [(Gate::None, false), (Gate::Plan, false), (Gate::None, true)] {
                let (name, _) = station_mark(column, gate, failed);
                assert!(theme::Assets.load(&format!("icons/{name}.svg")).ok().flatten().is_some(), "{name} is not in theme::ICONS");
            }
        }
    }
}
```

**Step 2: Run** `cargo test -p pocket board::parts`

Expected: FAIL to compile, with `cannot find function \`station_mark\``.

**Step 3: Implement.** Create `theme/assets/icons/status-quarter.svg`. It is `status-half.svg` with a quarter in place of the half:

```svg
<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 14 14"><circle cx="7" cy="7" r="6" stroke="#000" stroke-width="1.5" fill="none"/><path d="M7 7V3.5A3.5 3.5 0 0 1 10.5 7Z" fill="#000"/></svg>
```

`theme.rs:625`, in `ICONS`, before:

```rust
"status-merged", "status-needs", "status-queued",
```

After:

```rust
"status-merged", "status-needs", "status-quarter", "status-queued",
```

`board.rs:14`, before:

```rust
mod parts;
```

After:

```rust
pub(crate) mod parts;
```

In `board/parts.rs`, add to the imports:

```rust
use crate::factory::station::Column;
use agents::tickets::Gate;
```

Add after `status_glyph`:

```rust
/// A Factory ticket's mark and colour: a gate first, then a failure, then its station.
pub(crate) fn station_mark(column: Column, gate: Gate, failed: bool) -> (&'static str, Token) {
    match (gate, failed, column) {
        (Gate::Plan | Gate::Pr | Gate::Ask, _, _) => ("status-needs", WAITING_DOT),
        (Gate::None, true, _) => ("status-closed", FAILED),
        (Gate::None, false, Column::Todo) => ("status-todo", TEXT_4),
        (Gate::None, false, Column::Spec) => ("status-queued", WORKING),
        (Gate::None, false, Column::Plan) => ("status-quarter", WORKING),
        (Gate::None, false, Column::Implement) => ("status-half", WORKING),
        (Gate::None, false, Column::Checks) => ("status-three-quarters", SUCCESS),
        (Gate::None, false, Column::Pr) => ("status-full", GOAL_TEXT),
        (Gate::None, false, Column::Merged) => ("status-merged", INDIGO),
    }
}

pub(crate) fn station_glyph(column: Column, gate: Gate, failed: bool) -> impl IntoElement {
    let (name, color) = station_mark(column, gate, failed);
    icon(name, 14., color)
}
```

Widen four parts from `pub(super)` to `pub(crate)`, with no other change: `prop_chip` (`parts.rs:58`), `agent_avatar` (`:68`), `start_kbd` (`:86`), `bar_button` (`:90`). Plans 3 and 7 call them from `pocket::factory` and `pocket::inbox`.

**Step 4: Run** `cargo test -p pocket board::parts && cargo test -p theme`

Expected: PASS. `board::parts` reports `3 passed`. `theme` passes as before.

### Task 3.3: The card in the Factory layout

**Files:**
- Modify: `packages/desktop/crates/pocket/src/factory/station.rs` (`tag`, one test)
- Modify: `packages/desktop/crates/pocket/src/board/card.rs:1-10` (imports), `:42-51` (`ticket_note`), `:94` (`ticket_quick`), `:110-122` (`ticket_pills`), `:126` (`ticket_status`), `:180` (`ticket_card`)

**Context:** B5 keeps the Board's card with its four rows. In the Factory layout three things change on it. The status mark is the station's (`station_glyph`). The note is the Factory's (`station::note`), and a working Ticket keeps today's activity line. The last row adds the sub-task count `2/3` and the branch or the PR number in mono. US-03 and B8 forbid a button that decides anything, so a card in the Factory layout has no **Deny** and **Allow once**. **Start**, **Start again** and **Create PR** stay. A card cannot be dragged, because the Factory layout has no column a drop could land in. A click still opens what `opens` returns today (`board.rs:292`), and plan 3 adds the Factory ticket page. `ticket_card` and its parts read `Slot.col` as the Status column, which `station::walk` and `board/stations.rs` fill in. `Board::layout` is read once per card part. Every lookup a card adds is a hash lookup (`Agents::team`, `PullRequests::get`) or a scan of `Agents.pending`, which holds a few Asks. Today's `ticket_quick` already scans it.

**Step 1: Write the failing test.** In `station.rs`, add at the bottom of `mod tests`:

```rust
    #[test]
    fn a_card_names_its_pr_and_else_its_branch() {
        assert_eq!(tag(&Ticket { pr: 203, ..live(Status::Working) }, Some("feat/x")), Some("#203".to_string()));
        assert_eq!(tag(&live(Status::Working), Some("feat/x")), Some("feat/x".to_string()));
        assert_eq!(tag(&live(Status::Working), None), None);
    }
```

**Step 2: Run** `cargo test -p pocket factory::station`

Expected: FAIL to compile, with `cannot find function \`tag\``.

**Step 3: Implement.** In `station.rs`, add after `progress`:

```rust
/// What a card names the ticket's work by when the desktop holds no PR for it: the PR number pocketd wrote, else the branch.
pub(crate) fn tag(t: &Ticket, branch: Option<&str>) -> Option<String> {
    if t.pr > 0 { Some(format!("#{}", t.pr)) } else { branch.map(str::to_string) }
}
```

In `card.rs`, the imports at the top, before:

```rust
use super::Menu;
use super::logic::{Action, Col, Field, Quick, Slot, Tone, column, note, opens, quick, ticket_key};
use super::parts::{CARD_RADIUS, agent_avatar, label_dot, pill, priority_glyph, prop_chip, quick_button, status_glyph};
use crate::desktop::Desktop;
```

After:

```rust
use super::logic::{Action, Col, Field, Quick, Slot, Tone, column, note, opens, quick, ticket_key};
use super::parts::{CARD_RADIUS, agent_avatar, label_dot, pill, priority_glyph, prop_chip, quick_button, station_glyph, status_glyph};
use super::{Layout, Menu};
use crate::desktop::Desktop;
use crate::factory::station;
```

`card.rs:42-51`, before:

```rust
    pub(super) fn ticket_note(&self, t: &Ticket, slot: Slot) -> Option<(String, Token)> {
        let activity = self.agents.get(&t.agent_id).map(|a| a.activity.as_str());
        let (text, tone) = note(t, slot, activity, self.pending_ask(t).map(|p| p.ask()))?;
        Some((text, match tone {
```

After:

```rust
    pub(super) fn ticket_note(&self, t: &Ticket, slot: Slot) -> Option<(String, Token)> {
        let activity = self.agents.get(&t.agent_id).map(|a| a.activity.as_str());
        let found = if self.board.layout == Layout::Factory {
            let team = self.agents.team(&t.agent_id);
            let ask = station::asker(t, team, &self.agents.pending).map(|p| p.parts().1);
            // A working ticket still says what its agent is at. Queued's place in line belongs to the Status layout's column.
            station::note(t, team, self.prs.get(&t.worktree), ask).or_else(|| note(t, slot, activity, None).filter(|_| slot.col == Col::Working))
        } else {
            note(t, slot, activity, self.pending_ask(t).map(|p| p.ask()))
        };
        let (text, tone) = found?;
        Some((text, match tone {
```

`card.rs:94`, in `ticket_quick`, before:

```rust
        let ask = self.pending_ask(t).filter(|p| p.questions().is_empty() && !self.agents.observe_only());
```

After:

```rust
        // A Factory card decides nothing (B8): its Ask is answered in the session it opens.
        let ask = self.pending_ask(t).filter(|p| p.questions().is_empty() && !self.agents.observe_only() && self.board.layout != Layout::Factory);
```

`card.rs:110-122`, `ticket_pills`, before:

```rust
    pub(super) fn ticket_pills(&self, t: &Ticket, slot: Slot) -> Vec<Div> {
        let pr = self.prs.get(&t.worktree).map(|pr| {
            let (name, color) = pr_glyph(pr);
            let green = pr.checks.failed == 0 && pr.checks.pending == 0 && pr.checks.passed > 0;
            pill().child(icon(name, 12., color)).child(format!("#{}", pr.number)).when(green, |d| d.child(icon("check", 11., SUCCESS)))
        });
        let diff = self.repos.get(&t.worktree).filter(|r| !r.files.is_empty()).map(|r| {
            let (added, removed) = r.totals();
            pill().font_family(MONO).child(ui::meta_diff(added, removed, 11.))
        });
        let closed = (slot.col == Col::Closed).then(|| pill().child(closed_label(t.closed_why)));
        [pr, diff, closed].into_iter().flatten().collect()
    }
```

After:

```rust
    pub(super) fn ticket_pills(&self, t: &Ticket, slot: Slot) -> Vec<Div> {
        let found = self.prs.get(&t.worktree);
        let pr = found.map(|pr| {
            let (name, color) = pr_glyph(pr);
            let green = pr.checks.failed == 0 && pr.checks.pending == 0 && pr.checks.passed > 0;
            pill().child(icon(name, 12., color)).child(format!("#{}", pr.number)).when(green, |d| d.child(icon("check", 11., SUCCESS)))
        });
        let factory = self.board.layout == Layout::Factory;
        let progress = factory.then(|| self.agents.team(&t.agent_id).and_then(station::progress)).flatten().map(|done| pill().font_family(MONO).child(icon("team", 12., TEXT_3)).child(done));
        let tag = (factory && found.is_none()).then(|| station::tag(t, self.board_tree(t).map(|w| w.branch.as_str()))).flatten().map(|tag| pill().max_w(px(150.)).font_family(MONO).child(div().truncate().child(tag)));
        let diff = self.repos.get(&t.worktree).filter(|r| !r.files.is_empty()).map(|r| {
            let (added, removed) = r.totals();
            pill().font_family(MONO).child(ui::meta_diff(added, removed, 11.))
        });
        let closed = (slot.col == Col::Closed).then(|| pill().child(closed_label(t.closed_why)));
        [progress, pr, tag, diff, closed].into_iter().flatten().collect()
    }
```

`card.rs:126`, in `ticket_status`, before:

```rust
        div().id(SharedString::from(format!("ticket-status-{id}"))).size(px(20.)).flex().flex_none().items_center().justify_center().rounded(px(5.)).cursor_pointer().hover(|s| s.bg(FILL_3)).child(status_glyph(slot.col, t.status)).on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
```

After:

```rust
        div().id(SharedString::from(format!("ticket-status-{id}"))).size(px(20.)).flex().flex_none().items_center().justify_center().rounded(px(5.)).cursor_pointer().hover(|s| s.bg(FILL_3)).child(self.ticket_glyph(t, slot)).on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
```

Add after `ticket_status`:

```rust
    /// The Status layout marks a ticket by its column, the Factory layout by its station.
    fn ticket_glyph(&self, t: &Ticket, slot: Slot) -> AnyElement {
        if self.board.layout != Layout::Factory {
            return status_glyph(slot.col, t.status).into_any_element();
        }
        let team = self.agents.team(&t.agent_id);
        station_glyph(station::column(t, team, self.prs.get(&t.worktree)), station::gate(t, team), station::failed(t)).into_any_element()
    }
```

`card.rs:180`, in `ticket_card`, before:

```rust
        let card = self.ticket_shell(card, t, cx).when(col.lifts(), |d| {
```

After:

```rust
        let card = self.ticket_shell(card, t, cx).when(col.lifts() && self.board.layout != Layout::Factory, |d| {
```

**Step 4: Run** `cargo test -p pocket factory::station && cargo build --workspace`

Expected: PASS, `factory::station` reports `29 passed`. The build has no error. The Status and List layouts draw as before, because every change sits behind `Layout::Factory`.

### Task 3.4: `board/stations.rs`: the columns and the rails

**Files:**
- Create: `packages/desktop/crates/pocket/src/board/stations.rs`
- Modify: `packages/desktop/crates/pocket/src/board.rs:16` (`mod` list), `:159-170` (a new `reveal_slot` next to `reveal`)
- Modify: `packages/desktop/crates/pocket/src/factory.rs` (`scroll`, `shown`, `toggle`, `reveal`, one test)
- Modify: `packages/desktop/crates/pocket/src/board/page.rs:8-18`
- Modify: `packages/desktop/crates/pocket/src/board/selection.rs:58`
- Modify: `packages/desktop/crates/pocket/src/desktop.rs:533` (the call of `board_page`)

**Context:** The Status layout draws a column in `board/column.rs:33`: a 38px heading over one GPUI `list` whose rows are built only while they show, with `ticket_card` for each row. The Factory layout does the same for its five middle columns and an open rail. A closed rail is 44px wide and holds the station's mark, its name and its count (B2). GPUI rotates SVGs only, so the name is drawn one upright letter per line. The prototype opens a rail on a click on it, and closes it on a click on its heading. `station::widths` (Task 1.4) gives each column its width from the page's width. The page is as wide as the window less the sidebar: `window.viewport_size()` is in the window's logical pixels and `Desktop::lead_width()` (`sidebar.rs:247`) is the sidebar's width, 56px when collapsed. `board_page` (`page.rs:8`) draws the Board and has no `Window` yet. It is called once, at `desktop.rs:533`, where `main_view` holds one. The keyboard cursor (`step_board`, `selection.rs:50`) scrolls the card it lands on into view through `BoardState::reveal`, which knows only the Status and List layouts.

**Step 1: Write the failing test.** In `factory.rs`, add at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::FactoryState;
    use super::station::Column;

    #[test]
    fn a_rail_opens_on_a_click_and_closes_on_the_next() {
        let mut state = FactoryState { columns: Default::default(), lists: Vec::new(), open: Default::default(), scroll: gpui_kit::ScrollHandle::new() };
        state.toggle(Column::Merged);
        assert_eq!(state.open, [false, false, false, false, false, false, true]);
        state.toggle(Column::Merged);
        assert_eq!(state.open, [false; 7]);
    }

    #[test]
    fn the_layout_shows_the_cards_of_every_column() {
        let state = FactoryState { columns: [vec![0, 1], vec![], vec![2], vec![], vec![], vec![], vec![3]], lists: Vec::new(), open: Default::default(), scroll: gpui_kit::ScrollHandle::new() };
        assert_eq!(state.shown(), 4);
    }
}
```

**Step 2: Run** `cargo test -p pocket factory::tests`

Expected: FAIL to compile, with `struct \`FactoryState\` has no field named \`scroll\``, `no method named \`toggle\`` and `no method named \`shown\``.

**Step 3: Implement.** In `factory.rs`, add `Slot` to the imports and the field `scroll` to `FactoryState`. The struct and `new` become:

```rust
use crate::board::logic::{Filters, Slot};
```

```rust
pub(crate) struct FactoryState {
    /// Indices into the snapshot, per column of `COLUMNS`.
    pub(crate) columns: Stations,
    pub(crate) lists: Vec<ListState>,
    /// The rails drawn as full columns.
    pub(crate) open: [bool; COLUMNS.len()],
    pub(crate) scroll: ScrollHandle,
}

impl FactoryState {
    pub(crate) fn new(_: &mut Window, _: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let state = Self { columns: Default::default(), lists: COLUMNS.iter().map(|_| ListState::new(0, ListAlignment::Top, px(400.))).collect(), open: Default::default(), scroll: ScrollHandle::new() };
        (state, Vec::new())
    }
```

Add after `regroup`, inside `impl FactoryState`:

```rust
    /// How many cards the columns hold.
    pub(crate) fn shown(&self) -> usize {
        self.columns.iter().map(Vec::len).sum()
    }

    pub(crate) fn toggle(&mut self, column: Column) {
        self.open[column as usize] = !self.open[column as usize];
    }

    /// Scrolls the column that holds `slot`, and the row of columns, to show it.
    pub(crate) fn reveal(&self, slot: Slot) {
        if let Some(c) = self.columns.iter().position(|line| line.get(slot.pos) == Some(&slot.index)) {
            self.lists[c].scroll_to_reveal_item(slot.pos);
            self.scroll.scroll_to_item(c);
        }
    }
```

The import from `station` becomes `use station::{COLUMNS, Column, Stations, arrange};`.

In `board.rs`, add the module after `mod selection;` (`:16`):

```rust
mod selection;
mod stations;
mod tabs;
```

Add after `board_slots` in `impl Desktop`:

```rust
    fn reveal_slot(&self, slot: Slot) {
        if self.board.layout == Layout::Factory {
            self.factory.reveal(slot);
        } else {
            self.board.reveal(slot);
        }
    }
```

`selection.rs:58`, before:

```rust
            self.board.reveal(slots[at]);
```

After:

```rust
            self.reveal_slot(slots[at]);
```

Create `board/stations.rs`:

```rust
use super::logic::{Slot, column as status_column};
use super::parts::station_glyph;
use crate::desktop::Desktop;
use crate::factory::station::{COLUMNS, Column, GAP, PAD, RAIL_W, widths};
use agents::tickets::Gate;
use gpui_kit::*;
use theme::*;

const COL_RADIUS: f32 = 10.;
const LETTER_H: f32 = 13.;

impl Desktop {
    /// A column's mark, name and count, and a quiet "You" where you decide (B4).
    pub(super) fn station_heading(&self, column: Column) -> Div {
        let count = self.factory.columns[column as usize].len().to_string();
        let you = column.yours().then(|| div().ml_auto().text_size(px(12.)).font_weight(FontWeight::NORMAL).text_color(TEXT_3).child("You"));
        div()
            .h(px(38.))
            .flex_none()
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.5))
            .font_weight(FontWeight::MEDIUM)
            .child(station_glyph(column, Gate::None, false))
            .child(column.name())
            .child(div().font_weight(FontWeight::NORMAL).text_color(TEXT_3).child(count))
            .children(you)
    }

    /// A closed rail: the mark, the name one letter per line, the count. A click opens it.
    pub(super) fn station_rail(&self, column: Column, cx: &mut Context<Self>) -> Stateful<Div> {
        let letters = column.name().chars().map(|c| div().h(px(LETTER_H)).child(c.to_string()));
        let name = div().flex().flex_col().items_center().line_height(px(LETTER_H)).font_weight(FontWeight::MEDIUM).text_color(TEXT).children(letters);
        let count = self.factory.columns[column as usize].len().to_string();
        div()
            .id(("board-rail", column as usize))
            .w(px(RAIL_W))
            .h_full()
            .pt(px(12.))
            .flex()
            .flex_none()
            .flex_col()
            .items_center()
            .gap(px(8.))
            .rounded(px(COL_RADIUS))
            .bg(FILL_1)
            .cursor_pointer()
            .hover(|s| s.bg(FILL_2))
            .text_size(px(12.5))
            .text_color(TEXT_3)
            .child(station_glyph(column, Gate::None, false))
            .child(name)
            .child(count)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_station_rail(column, cx)))
    }

    fn toggle_station_rail(&mut self, column: Column, cx: &mut Context<Self>) {
        self.factory.toggle(column);
        self.board_changed();
        cx.notify();
    }

    fn station_column(&mut self, column: Column, width: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        let cards = list(self.factory.lists[column as usize].clone(), cx.processor(move |this, ix, _, cx| {
            let index = this.factory.columns[column as usize].get(ix).copied();
            let col = index.and_then(|i| this.agents.board.tickets.get(i)).map(status_column);
            match (index, col) {
                (Some(index), Some(col)) => this.ticket_card(Slot { col, pos: ix, index }, cx),
                _ => div().into_any_element(),
            }
        }))
        .flex_1()
        .w_full();
        let heading = self.station_heading(column);
        let heading = if column.rail() {
            div().id(("board-rail-close", column as usize)).cursor_pointer().child(heading).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_station_rail(column, cx))).into_any_element()
        } else {
            heading.into_any_element()
        };
        div().id(("board-station", column as usize)).w(px(width)).h_full().flex().flex_none().flex_col().rounded(px(COL_RADIUS)).bg(FILL_1).child(heading).child(cards)
    }

    /// The Factory layout: seven columns on a page `available` wide, scrolling sideways when they don't fit (B3).
    pub(super) fn station_columns(&mut self, available: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        let widths = widths(available, &self.factory.open);
        let columns: Vec<AnyElement> = COLUMNS
            .into_iter()
            .map(|c| {
                if c.rail() && !self.factory.open[c as usize] {
                    self.station_rail(c, cx).into_any_element()
                } else {
                    self.station_column(c, widths[c as usize], cx).into_any_element()
                }
            })
            .collect();
        div().id("board-stations").flex_1().min_h_0().overflow_x_scroll().track_scroll(&self.factory.scroll).px(px(PAD)).pb(px(8.)).flex().gap(px(GAP)).children(columns)
    }
}
```

`page.rs:8-18`, before:

```rust
    pub(crate) fn board_page(&mut self, cx: &mut Context<Self>) -> Div {
        if self.board.page.is_some() {
            return self.ticket_page(cx);
        }
        let shown = body(self.board.layout == Layout::List, self.agents.board.tickets.len(), self.board.rows.len(), self.board.hidden);
        let main = match shown {
            Body::Hero => self.board_hero(cx).into_any_element(),
            Body::Columns => {
```

After:

```rust
    pub(crate) fn board_page(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        if self.board.page.is_some() {
            return self.ticket_page(cx);
        }
        let factory = self.board.layout == Layout::Factory;
        let rows = if factory { self.factory.shown() } else { self.board.rows.len() };
        let shown = body(self.board.layout == Layout::List, self.agents.board.tickets.len(), rows, self.board.hidden);
        let main = match shown {
            Body::Hero => self.board_hero(cx).into_any_element(),
            Body::Columns if factory => {
                let available = f32::from(window.viewport_size().width) - self.lead_width();
                self.station_columns(available, cx).into_any_element()
            }
            Body::Columns => {
```

`desktop.rs:533`, before:

```rust
            None if matches!(self.screen, Screen::Board) => self.board_page(cx),
```

After:

```rust
            None if matches!(self.screen, Screen::Board) => self.board_page(window, cx),
```

What a frame costs: the Factory layout builds the rows of the seven lists that show and nothing else. `widths` is a dozen float operations. The headings read `Vec::len`. No Ticket is grouped, sorted or filtered in `render`.

**Step 4: Run** `cargo test -p pocket factory:: && cargo build --workspace`

Expected: PASS, `factory::tests` reports `2 passed` and `factory::station` `29 passed`. The build has no error.

### Task 3.5: The layout switch, the Display menu and the chips

**Files:**
- Modify: `packages/desktop/crates/pocket/src/board.rs` (`Layout::name`, `layouts`, `set_board_layout`, one test)
- Modify: `packages/desktop/crates/pocket/src/board/top_bar.rs:8-15` (`board_bar`)
- Modify: `packages/desktop/crates/pocket/src/board/display_menu.rs:1-89`
- Modify: `packages/desktop/crates/pocket/src/board/tabs.rs:1-47`
- Modify: `packages/desktop/crates/pocket/src/factory.rs` (`counts`, `#[allow(dead_code)]`)

**Context:** The index replaces the Display menu's two layout tiles (`display_menu.rs:23-49`) with a segmented control **Factory · Status · List** in the Board's top bar, with **Factory** only when `factory.v1` was negotiated. `ui::segmented(items, active, small, fill, on, cx)` (`ui.rs:199`) draws one. Its segments take the ids `("segment", i)`, so callers wrap it in a `div` with its own id, as `git_ui/diff.rs:567` does. The top bar is `board_bar` (`top_bar.rs:8`), a 44px `detail_bar` with the title "Board" on the left. B9 puts the chips `waiting on you`, `ask` and `failed`, then the slots, in the second row. In the Status and List layouts that row (`board_tabs`, `tabs.rs:13`) holds the tabs and the saved views, with the Filter and Display buttons at its right. The Factory layout has no tabs (its columns are the stations), so the chips take their place there. `tab_chip` (`tabs.rs:8`) is the row's chip. B13 fixes the order inside a Factory column, so **Ordering** does not apply there, and the Factory layout never shows a Ticket closed without a merge, so **Show closed tickets** does not apply either. The chips read counts that `regroup` computed, so a frame does no counting.

**Step 1: Write the failing test.** In `board.rs`, the imports of `mod tests` become:

```rust
    use super::logic::Tab;
    use super::{Layout, hidden_scope, layouts, shown_layout};
    use store::BoardLayout;
```

Add to `mod tests`:

```rust
    #[test]
    fn the_switch_offers_factory_only_when_pocketd_serves_it() {
        assert_eq!(layouts(true), [Layout::Factory, Layout::Board, Layout::List]);
        assert_eq!(layouts(false), [Layout::Board, Layout::List]);
        assert_eq!(layouts(true).into_iter().map(Layout::name).collect::<Vec<_>>(), ["Factory", "Status", "List"]);
    }
```

**Step 2: Run** `cargo test -p pocket board::tests`

Expected: FAIL to compile, with `unresolved import \`super::layouts\``.

**Step 3: Implement.** In `board.rs`, add after the two `From` impls:

```rust
impl Layout {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Layout::Factory => "Factory",
            Layout::Board => "Status",
            Layout::List => "List",
        }
    }
}

/// The layouts the switch offers.
pub(crate) fn layouts(factory: bool) -> Vec<Layout> {
    [Layout::Factory, Layout::Board, Layout::List].into_iter().filter(|l| factory || *l != Layout::Factory).collect()
}
```

Add to `impl Desktop` in `board.rs`, after `open_board`:

```rust
    fn set_board_layout(&mut self, layout: Layout, cx: &mut Context<Self>) {
        self.board.layout = layout;
        self.store.board_layout = layout.into();
        self.store.save();
        self.board_changed();
        cx.notify();
    }
```

`top_bar.rs:8-15`. Add `use super::layouts;` to the imports. In `board_bar`, before:

```rust
        let title = div().ml(px(2.)).flex_1().min_w_0().flex().items_center().gap(px(7.)).text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(icon("blocks", 14., TEXT_3)).child("Board");
```

After:

```rust
        let segments = layouts(self.agents.factory_offered()).into_iter().map(|l| ui::Segment { icon: None, value: l, label: l.name().into(), badge: None }).collect();
        let switch = div().id("board-layout").ml(px(10.)).child(ui::segmented(segments, self.board.layout, true, false, |this: &mut Self, l, cx| this.set_board_layout(l, cx), cx));
        let title = div().ml(px(2.)).flex_1().min_w_0().flex().items_center().gap(px(7.)).text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(icon("blocks", 14., TEXT_3)).child("Board").child(switch);
```

`display_menu.rs`. Delete the `layouts` binding (`:23-49`, from `let layouts = [(Layout::List, "list", "List")` to `}).collect::<Vec<_>>();`). Keep `use super::Layout;`. The `body` binding, before (`:79-89`):

```rust
        let body = div()
            .p(px(7.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(div().flex().gap(px(6.)).children(layouts))
            .child(section("Ordering").child(div().flex().flex_wrap().gap(px(4.)).children(orders)))
            .child(div().h(px(1.)).bg(HAIRLINE))
            .child(div().flex().flex_col().child(setting_row("Work on at once").child(stepper)).child(setting_row("Show closed tickets").child(closed)))
            .child(div().h(px(1.)).bg(HAIRLINE))
            .child(section("Display properties").child(div().flex().flex_wrap().gap(px(4.)).children(props)));
```

After:

```rust
        // A Factory column keeps the Up next order (B13) and never holds a ticket closed without a merge.
        let factory = self.board.layout == Layout::Factory;
        let body = div()
            .p(px(7.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .when(!factory, |d| d.child(section("Ordering").child(div().flex().flex_wrap().gap(px(4.)).children(orders))).child(div().h(px(1.)).bg(HAIRLINE)))
            .child(div().flex().flex_col().child(setting_row("Work on at once").child(stepper)).when(!factory, |d| d.child(setting_row("Show closed tickets").child(closed))))
            .child(div().h(px(1.)).bg(HAIRLINE))
            .child(section("Display properties").child(div().flex().flex_wrap().gap(px(4.)).children(props)));
```

`tabs.rs`. The imports become:

```rust
use super::Layout;
use super::logic::{Filters, Tab};
use super::views::view_is_on;
use crate::desktop::Desktop;
use crate::factory::station::Needs;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
```

At the top of `board_tabs` (`tabs.rs:13`), before:

```rust
    pub(super) fn board_tabs(&self, cx: &mut Context<Self>) -> Div {
        let view_on = self.store.board_views.iter().position(|v| view_is_on(v, &self.board.filters, self.board.tab));
```

After:

```rust
    pub(super) fn board_tabs(&self, cx: &mut Context<Self>) -> Div {
        if self.board.layout == Layout::Factory {
            let row = div().id("board-tab-row").flex_1().min_w_0().overflow_x_scroll().flex().items_center().gap(px(5.)).children(self.needs_chips(cx)).child(self.slots_text());
            return div().h(px(42.)).flex_none().pl(px(14.)).pr(px(12.)).flex().items_center().gap(px(8.)).child(row).child(self.board_tools(cx));
        }
        let view_on = self.store.board_views.iter().position(|v| view_is_on(v, &self.board.filters, self.board.tab));
```

Add after `board_tabs`, inside `impl Desktop`:

```rust
    /// `waiting on you`, `ask` and `failed`, each with its count. Only the active "waiting on you" chip is tinted (B9).
    fn needs_chips(&self, cx: &mut Context<Self>) -> Vec<Stateful<Div>> {
        let counts = self.factory.counts;
        Needs::ALL
            .into_iter()
            .enumerate()
            .map(|(i, needs)| {
                let on = self.board.filters.needs == Some(needs);
                let tinted = on && needs == Needs::You;
                tab_chip(("board-needs", i), on)
                    .when(tinted, |d| d.bg(WAITING_BG).text_color(WAITING_TEXT))
                    .child(div().font_weight(FontWeight::NORMAL).text_color(if tinted { WAITING_TEXT } else { TEXT_4 }).child(counts.of(needs).to_string()))
                    .child(needs.name())
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.board.filters.needs = Needs::toggled(this.board.filters.needs, needs);
                        this.board_changed();
                        cx.notify();
                    }))
            })
            .collect()
    }

    /// `3/4 slots`: the tickets that hold a slot, and the Board's limit.
    fn slots_text(&self) -> Div {
        let used = div().font_family(MONO).child(format!("{}/{}", self.factory.counts.slots, self.agents.board.limit));
        div().ml(px(6.)).flex().flex_none().items_center().gap(px(4.)).text_size(px(12.5)).text_color(TEXT_3).whitespace_nowrap().child(used).child("slots")
    }
```

In `factory.rs`, remove the `#[allow(dead_code)]` line above `pub(crate) mod station;`. Every function of `station.rs` now has a caller. The import from `station` becomes:

```rust
use station::{COLUMNS, Column, Counts, Stations, arrange, counts};
```

Add the field after `open` in `FactoryState`:

```rust
    /// What the chips say, over every ticket.
    pub(crate) counts: Counts,
```

In `FactoryState::new`, add `counts: Counts::default(),` to the struct literal. In `regroup`, after `self.columns = arrange(tickets, filters, team, pr);`, add:

```rust
        self.counts = counts(tickets);
```

In `factory::tests`, add `counts: Default::default(),` to the two `FactoryState { … }` literals.

**Step 4: Run** `cargo test -p pocket board:: && cargo test -p pocket factory:: && cargo build --workspace && cargo clippy --workspace --all-targets`

Expected: PASS. `board::tests` reports `4 passed`. The build has no error. Clippy reports no warning, so no function of `station.rs` is left without a caller.

### Task 3.6: Capture steps, the "after" captures, the frame timing and the map

**Files:**
- Modify: `packages/desktop/crates/pocket/src/capture.rs:31` (`STEPS` length), `:550` (after `board-filtered`)
- Modify: `docs/adr/0003-desktop-code-layout.md:50` (the `board` row, and a new `factory` row after it)
- Temporary, removed in Step 4: `packages/desktop/crates/pocket/src/factory.rs` (`regroup_stations`), `packages/desktop/crates/pocket/src/capture.rs:1447` (`draw`)

**Context:** Capture mode runs a list of steps per screen, each a `fn(&mut Desktop, &mut Window, &mut Context<Desktop>)` in `STEPS`. `STEPS` is an array, so its length is part of its type (`[(&str, Step); 265]` at `:31`). Other plans add steps too, so take the length rustc names. `show_team` (`capture.rs:1176`) injects a Team through `d.on_agents(agents::Event::Team(team), window, cx)`. `Agents` keeps a Team by its lead whether or not the lead is a listed agent, so a Team for `plan-lead` (the fixture's t15) needs no agent. After each step, capture mode waits and calls `draw` (`capture.rs:1447`), which lays out and paints one frame. US-38 asks that one frame of the Board with 1000 Tickets cost at most 8.3ms of CPU. CLAUDE.md asks to measure in release, with timings tagged `[DEBUG-…]` that are removed afterwards. `capture.sh` builds the release binary. The capture window is 1440 by 900 (`DEFAULT_WINDOW`), with the sidebar 272px wide. The step `sidebar` collapses it to the 56px rail.

**Step 1: Add the steps.** In `capture.rs`, add after the `board-filtered` step (`:546-550`):

```rust
    ("board-factory", |d, window, cx| {
        let tasks = ["Split the store into sections", "Read each section on its own", "Move desktop.json to the sections"]
            .into_iter()
            .enumerate()
            .map(|(i, title)| agents::TeamTask { id: format!("t{}", i + 1), title: title.into(), agent: "claude".into(), state: "planned".into(), ..Default::default() })
            .collect();
        let team = agents::Team { lead: "plan-lead".into(), state: crate::team::PROPOSED.into(), tasks, updated_at: crate::util::now_ms(), ..Default::default() };
        d.on_agents(agents::Event::Team(team), window, cx);
        d.board.layout = crate::board::Layout::Factory;
        d.board_changed();
        cx.notify();
    }),
    ("board-factory-todo", |d, _, cx| {
        d.factory.toggle(crate::factory::station::Column::Todo);
        d.board_changed();
        cx.notify();
    }),
    ("board-factory-you", |d, _, cx| {
        d.board.filters.needs = Some(crate::factory::station::Needs::You);
        d.board_changed();
        cx.notify();
    }),
    ("board-many", |d, _, cx| {
        let more: Vec<_> = (0..40).flat_map(|n| d.agents.board.tickets.iter().map(move |t| (n, t.clone()))).map(|(n, t)| agents::tickets::Ticket { id: format!("{}-{n}", t.id), ..t }).collect();
        d.agents.board.tickets.extend(more);
        d.board_changed();
        cx.notify();
    }),
```

`capture.rs:31`, before:

```rust
const STEPS: [(&str, Step); 265] = [
```

After:

```rust
const STEPS: [(&str, Step); 269] = [
```

If another plan's steps landed first, `cargo build` fails with `expected an array with a size of N, found one with a size of M`. Write the size that the error names as found.

`board-many` copies the fixture's 25 Tickets 40 times, so the snapshot holds 1025 Tickets.

**Step 2: Capture "after"** (from the repository root):

```bash
.ui-review/fixture/capture.sh /tmp/lf-2/after board=board board-list=board,board-list board-display=board,board-display board-factory=board,board-factory board-factory-display=board,board-factory,board-display board-factory-todo=board,board-factory,board-factory-todo board-factory-you=board,board-factory,board-factory-you board-factory-compact=sidebar,board,board-factory
```

Expected: eight paths printed under `/tmp/lf-2/after/`. Open each and check:

- `impl-board.png` and `impl-board-list.png` against the "before" ones from Task 3.1: the same columns, rows and cards. Only the top bar differs: after "Board" it holds the switch **Factory · Status · List**, with **Status** (or **List**) selected.
- `impl-board-display.png`: the Display menu starts with **Ordering**. The two layout tiles are gone.
- `impl-board-factory.png`: **Factory** is selected. The second row holds `4 waiting on you`, `2 ask`, `2 failed` and `9/4 slots`, then the Filter and Display buttons. Seven columns, none cut off and no sideways scroll: the To do rail (its name one letter per line, count 4), Spec (t13, then t14 reading `Starting…`), Plan with "You" (t15 first, with the amber needs-you mark and `Plan ready · 3 tasks`, then t16 with the quarter mark), Implement (t18 `Ask · xcodebuild test`, t7 `Ask · pnpm test --filter app-android`, t8 `Failed · cargo test failed in composer.rs`, then t10, t17, t9, t5, t6), Checks (t20 first, red closed mark and `Failed · cargo test failed in composer.rs`, then t19 and t21), PR review with "You" (t22 `Ready to merge · #203`, then t23 with a mono `#202` pill), and the Merged rail (count 3). No card shows **Deny** or **Allow once**. The To do cards keep **Start**.
- `impl-board-factory-display.png`: the Display menu without **Ordering** and without **Show closed tickets**.
- `impl-board-factory-todo.png`: To do is a 232px column with t2, t1, t4, t3 and its heading. The row scrolls sideways, so the Merged rail sits past the right edge.
- `impl-board-factory-you.png`: the `4 waiting on you` chip is amber. The columns hold only t15, t18, t7 and t22, and the notice reads `20 tickets hidden by filters · Clear filters`.
- `impl-board-factory-compact.png`: the sidebar is the 56px rail, and the five middle columns are 240px wide, still with no sideways scroll.

**Step 3: Time a frame with 1025 Tickets.** Add two temporary timings. In `factory.rs`, `regroup_stations` becomes:

```rust
    pub(crate) fn regroup_stations(&mut self) {
        let started = std::time::Instant::now();
        let (agents, prs) = (&self.agents, &self.prs);
        self.factory.regroup(&agents.board.tickets, &self.board.filters, |t| agents.team(&t.agent_id), |t| prs.get(&t.worktree));
        eprintln!("[DEBUG-factory] regroup {} tickets: {:?}", self.agents.board.tickets.len(), started.elapsed());
    }
```

In `capture.rs`, `draw` becomes:

```rust
fn draw(window: &mut Window, cx: &mut App) {
    window.refresh();
    let started = std::time::Instant::now();
    window.draw(cx).clear(cx);
    eprintln!("[DEBUG-factory] draw: {:?}", started.elapsed());
}
```

Run from the repository root:

```bash
.ui-review/fixture/capture.sh /tmp/lf-2/many board-many=board,board-factory,board-many
```

Expected: `/tmp/lf-2/many/impl-board-many.png` is printed, and stderr shows `[DEBUG-factory] regroup 25 tickets` and then `[DEBUG-factory] regroup 1025 tickets` lines, each followed by `draw` lines. Every `draw` line after the first `regroup 1025 tickets` line is at most 8.3ms (US-38). The screen shows the same seven columns with about forty times as many cards, each column scrolled to its top. Write the regroup time and the largest draw time after `regroup 1025 tickets` into the PR description. If a draw line is above 8.3ms, stop and find what in `render` grows with the snapshot before you go on: CLAUDE.md, "Measure before fixing".

**Step 4: Remove the timings.** Undo both edits of Step 3, then from the repository root run:

```bash
grep -rn "DEBUG-factory" packages/desktop/crates
```

Expected: no output.

**Step 5: Update the map.** `docs/adr/0003-desktop-code-layout.md:50`, in the `board` row, before:

```
`selection` the keyboard cursor and the picked tickets,
```

After:

```
`selection` the keyboard cursor and the picked tickets, `stations` the Factory layout's columns and rails,
```

Add a row after the `board` row:

```
| `factory` | the Factory, Light Factory's pipeline on the Board: `FactoryState` (the station columns, their lists, the open rails, the chip counts); `factory/station` the pure rules (a ticket's column, gate, rank and note, the chips, the column widths) |
```

**Step 6: Run the PR's checks** (from `packages/desktop`): `cargo build --workspace`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`

Expected: all green. Clippy adds no warning.

## Contract changes needed

- **Desktop crates, the `factory/station.rs` bullet.** Replace "`arrange(tickets, …) -> [Vec<usize>; 7]`" with: "`arrange(tickets, filters: &Filters, team: impl Fn(&Ticket) -> Option<&Team>, pr: impl Fn(&Ticket) -> Option<&Pr>) -> Stations` (`Stations = [Vec<usize>; 7]`). Also `on_line(t)`, `gate(t, team) -> Gate`, `waits(t)`, `failed(t)`, `walk(columns, open, tickets) -> Vec<Slot>`, `asker(t, team, pending) -> Option<&Permission>`, `pr_number(t, pr) -> u32`, `progress(team) -> Option<String>`, `tag(t, branch) -> Option<String>`, `widths(available, open) -> [f32; 7]` with `RAIL_W`, `RAIL_OPEN`, `COL_MIN`, `COL_MAX`, `GAP`, `PAD`, and `Needs { You, Ask, Failed }` with `Counts::of(needs)`."
- **Desktop crates, `pocket::board::logic`.** Add: "`Filters` gains `needs: Option<factory::station::Needs>`, the Factory layout's chip. `Filters::keeper` applies it in every layout (plan 2)."
- **Desktop crates, the `pocket::factory` bullet.** Add: "`FactoryState` holds `columns: Stations` (what `station::arrange` returned at the last regroup), `lists`, `open` and `counts`. `Desktop::regroup_stations()` fills it and `Desktop::board_changed()` calls it in the Factory layout." Plan 7's `BoardState.stations` is this `FactoryState.columns`.
- **Desktop crates, the `board/parts.rs` bullet.** Add: "`station_mark(column, gate, failed) -> (&'static str, Token)`, which `station_glyph` draws. `board::parts` is `pub(crate)`, and so are `station_glyph`, `prop_chip`, `agent_avatar`, `bar_button` and `start_kbd` (plan 2)."
- **Desktop crates, the Capture bullet.** Add: "Plan 2's fixture Tickets: t13 and t14 in Spec, t15 (agent `plan-lead`, Plan gate) and t16 in Plan, t17 and t18 (agent `ios`, Ask) in Implement, t19, t20 (failed) and t21 in Checks, t22 (PR gate, `#203`) and t23 in PR review, t24 and t25 in Merged. The `board-factory` step injects a proposed Team of three tasks for `plan-lead`. Plan 2 adds the steps `board-factory-todo`, `board-factory-you` and `board-many` (1025 Tickets)." And in the same bullet: "The icon `status-quarter` comes from the prototype's `app.js:157`."

## Open questions

- **The rail's rotated name.** B2 asks for a rotated name, and GPUI rotates only SVGs. Recommended: keep the upright letters one per line. A design check on the first capture decides whether a rotated name is worth an SVG per station.
- **Where the switch sits.** The index puts it in the Board's top bar. The prototype and US-01 put the tabs "under the breadcrumb", in the second row. Recommended: keep the top bar, as the index says. The second row is full in the Status layout (tabs, saved views, Filter and Display).
- **The default layout once the Factory exists.** Recommended: keep Status as the default until plan 3's last PR, then open a project whose Factory is on in the Factory layout the first time.
- **A mono command in the Ask note.** B8 asks for `Ask · cargo test -p daemon` in mono. This plan draws the note as one text run. Recommended: leave it until plan 4 builds the Ask bar, which draws the command in mono anyway.
- **Slots above the limit.** With plan 1's slot rule, `9/4 slots` can show when Tickets resume after a gate. Recommended: keep the plain count. Plan 1's `wait_hint` already names the real count above the limit.

