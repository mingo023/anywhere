# Activity Page Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** Add an Activity screen to the desktop app: every session across all projects in five columns (Idle, Working, Needs you, Done, In review), with a preview panel, Open session, Allow/Deny on cards, search, and project/agent filters.

**Architecture:** A new `pocket` feature module `activity` (state struct `ActivityState`, composed by `Desktop`) with its pure rules in `activity/logic.rs`. Each frame the page is on screen, `impl Desktop` gathers one plain `Item` per session (status, open PR, branch, note) and hands them to `logic::arrange`, which places each in a column. A column is a virtualized `list` like the Board's. Status comes from `crate::status::Status::of`; "In review" means an idle session whose worktree has an open or draft PR in `Desktop.prs`.

**Toolset** (run from `packages/desktop`):
- One module's tests: `cargo test -p pocket activity::logic`
- Palette tests: `cargo test -p pocket palette`
- Shortcut table tests: `cargo test -p pocket settings::keybindings`
- Build: `cargo build --workspace`
- Lint (no new warnings): `cargo clippy --workspace --all-targets`
- Full suite: `cargo test --workspace`
- Screens: `.ui-review/fixture/capture.sh <dir> <name>=<step>,<step> …` from the repo root (builds release with `--features capture`, runs a fake pocketd; names not starting with `team`, `chat`, `agents` or `restore-` use the `sessions` scenario)

**Read first:**
- `CLAUDE.md` (repo root): where code goes, test style, the commands to run before committing, and the performance rules (render must cost what's visible; virtualize lists).
- `docs/adr/0003-desktop-code-layout.md`: one module per feature with its own state struct; `impl Desktop` stays thin.
- `CONTEXT.md` sections **Session**, **Status** (Needs you, Working, Done, Failed, Idle, Seen) and **Board**: the words this page uses. The Board's "In review" means something else (any idle ticket); this page's means "has an open PR".
- `packages/desktop/crates/pocket/src/board.rs`, `board/column.rs`, `board/card.rs`, `board/logic.rs`: the template. Activity copies the column/list/card shape with far fewer features.
- `packages/desktop/crates/pocket/src/inbox.rs` + `inbox/detail.rs`: how notes are built from `agents::Summary` and how "Open session" calls `focus_agent`.
- `.ui-review/prototypes/sessions-manager.html` (variant 1, "Board"): the agreed look.

**Decisions this plan rests on** (settled with the user):
- Layout is the Board-style columns (prototype variant 1). Name: **Activity** (Sessions is taken by the sidebar's Scratch section; Agents by Settings).
- Columns, in order: Idle, Working, Needs you (includes Failed), Done, In review.
- In review = an Idle (or not attached) session whose worktree branch has an open or draft PR. Main checkouts and Locals are never polled for PRs, so their sessions never land there.
- Clicking a card opens a side panel with a summary (status, pending ask with Allow/Deny, last message, PR) and an Open session button. It never renders the live terminal (a narrow pane would resize the agent's PTY).
- Opening the panel marks a Done or Failed session seen, like Inbox does; Needs you is never cleared that way.
- v1 includes Allow/Deny on cards, search, and project/agent filters. Not in v1: Create PR, a back button from the session to Activity, Merged/Deleted history, iOS.

---

## PR 1: Activity screen with status columns

**Scope:** New `Screen::Activity`, the `OpenActivity` action, the sidebar row, rail button and palette entry, and the five columns of read-only cards. Clicking a card opens its session directly (PR 2 turns that into the preview panel). No search or filters yet.
**Depends on:** nothing
**Done when:** `cargo test --workspace` is green, clippy has no new warnings, and the `activity` capture shows the five columns.

### Task 1.1: Column rules

**Files:**
- Create: `packages/desktop/crates/pocket/src/activity/logic.rs`

**Context:** `crate::status::Status` (in `packages/desktop/crates/pocket/src/status.rs:8`) is `NeedsYou, Failed, Done, Working, Idle`, derived `Ord` in that urgency order. `Status::of(&Summary)` returns `None` when the agent isn't attached (it reports only terminal activity); such a session belongs with Idle. This file is pure: no GPUI, no `Desktop`. Tests sit at the bottom, named as sentences, like `packages/desktop/crates/pocket/src/board/logic.rs:760`. The module is declared in Task 1.2; until then this file is not compiled, so write it whole now and run its tests at the end of Task 1.2.

**Step 1: Write the failing tests** (append to the new file after the code skeleton in Step 3; they are listed first because they define the behaviour)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, status: Option<Status>, pr_open: bool, at: i64) -> Item {
        Item { id: id.into(), status, pr_open, at, ..Item::default() }
    }

    fn ids(items: &[Item], columns: &Columns, col: Col) -> Vec<String> {
        columns[col as usize].iter().map(|&i| items[i].id.clone()).collect()
    }

    // Each status lands in its own column.
    #[test]
    fn a_session_sits_in_the_column_its_status_says() {
        assert_eq!(column(Some(Status::NeedsYou), false), Col::NeedsYou);
        assert_eq!(column(Some(Status::Working), false), Col::Working);
        assert_eq!(column(Some(Status::Done), false), Col::Done);
        assert_eq!(column(Some(Status::Idle), false), Col::Idle);
    }

    // Failed asks for a look like Needs you does.
    #[test]
    fn a_failed_turn_needs_you() {
        assert_eq!(column(Some(Status::Failed), false), Col::NeedsYou);
    }

    // An open PR only matters once the agent has nothing for you.
    #[test]
    fn an_idle_session_with_an_open_pr_is_in_review() {
        assert_eq!(column(Some(Status::Idle), true), Col::InReview);
        assert_eq!(column(None, true), Col::InReview);
        assert_eq!(column(None, false), Col::Idle);
    }

    #[test]
    fn a_pr_never_hides_work_or_a_question() {
        assert_eq!(column(Some(Status::Working), true), Col::Working);
        assert_eq!(column(Some(Status::NeedsYou), true), Col::NeedsYou);
        assert_eq!(column(Some(Status::Done), true), Col::Done);
    }

    // Up next order inside the columns that want you; most recent first elsewhere.
    #[test]
    fn what_wants_you_comes_oldest_first_and_the_rest_newest_first() {
        let items = vec![
            item("done-new", Some(Status::Done), false, 30),
            item("done-old", Some(Status::Done), false, 10),
            item("idle-old", Some(Status::Idle), false, 10),
            item("idle-new", Some(Status::Idle), false, 30),
            item("work-new", Some(Status::Working), false, 30),
            item("work-old", Some(Status::Working), false, 10),
        ];
        let columns = arrange(&items);
        assert_eq!(ids(&items, &columns, Col::Done), ["done-old", "done-new"]);
        assert_eq!(ids(&items, &columns, Col::Working), ["work-old", "work-new"]);
        assert_eq!(ids(&items, &columns, Col::Idle), ["idle-new", "idle-old"]);
    }

    #[test]
    fn needs_you_lists_questions_before_failed_turns() {
        let items = vec![item("failed", Some(Status::Failed), false, 1), item("asks", Some(Status::NeedsYou), false, 9)];
        assert_eq!(ids(&items, &arrange(&items), Col::NeedsYou), ["asks", "failed"]);
    }
}
```

**Step 2:** No separate red run here: the module isn't declared until Task 1.2. Task 1.2 Step 4 runs these tests.

**Step 3: Write the implementation** (top of `activity/logic.rs`, above the tests)

```rust
use crate::status::Status;
use std::cmp::Reverse;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Col {
    Idle,
    Working,
    NeedsYou,
    Done,
    InReview,
}

pub(crate) const COLS: [Col; 5] = [Col::Idle, Col::Working, Col::NeedsYou, Col::Done, Col::InReview];

impl Col {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Col::Idle => "Idle",
            Col::Working => "Working",
            Col::NeedsYou => "Needs you",
            Col::Done => "Done",
            Col::InReview => "In review",
        }
    }

    /// What it says while it holds nothing.
    pub(crate) fn hint(self) -> &'static str {
        match self {
            Col::Idle => "Sessions at the prompt with nothing unseen.",
            Col::Working => "Agents running a turn show up here.",
            Col::NeedsYou => "Questions, permissions and failed turns land here.",
            Col::Done => "Finished turns you haven't seen yet.",
            Col::InReview => "Idle sessions whose branch has an open PR.",
        }
    }
}

/// One session as the page shows it, gathered from the agent list, its terminal and its worktree.
#[derive(Clone, PartialEq, Debug, Default)]
pub(crate) struct Item {
    pub id: String,
    /// `None` while the agent isn't attached: it reports only terminal activity.
    pub status: Option<Status>,
    /// Its worktree's branch has an open or draft PR.
    pub pr_open: bool,
    pub at: i64,
    pub title: String,
    /// The project's path, and the name the sidebar shows for it.
    pub project: String,
    pub project_name: String,
    pub branch: String,
    /// Its worktree's path, which PRs and diffs are kept under; empty outside a worktree.
    pub tree: String,
    pub provider: String,
    /// The pending ask, the error, what it's doing or what it last said, by status.
    pub note: String,
}

/// Status decides; an open PR files a session with nothing for you under In review, where it waits on other people.
pub(crate) fn column(status: Option<Status>, pr_open: bool) -> Col {
    match status {
        Some(Status::NeedsYou | Status::Failed) => Col::NeedsYou,
        Some(Status::Working) => Col::Working,
        Some(Status::Done) => Col::Done,
        Some(Status::Idle) | None if pr_open => Col::InReview,
        Some(Status::Idle) | None => Col::Idle,
    }
}

pub(crate) type Columns = [Vec<usize>; COLS.len()];

/// Each column's items as indexes into `items`: what wants you oldest first, as Up next orders it; the rest newest first.
pub(crate) fn arrange(items: &[Item]) -> Columns {
    let mut out: Columns = Default::default();
    for (i, item) in items.iter().enumerate() {
        out[column(item.status, item.pr_open) as usize].push(i);
    }
    for (col, line) in COLS.iter().zip(out.iter_mut()) {
        match col {
            Col::NeedsYou => line.sort_by_key(|&i| (items[i].status, items[i].at)),
            Col::Working | Col::Done => line.sort_by_key(|&i| items[i].at),
            Col::Idle | Col::InReview => line.sort_by_key(|&i| Reverse(items[i].at)),
        }
    }
    out
}
```

**Step 4:** Tests run in Task 1.2.

### Task 1.2: Screen, action and state

**Files:**
- Create: `packages/desktop/crates/pocket/src/activity.rs`
- Modify: `packages/desktop/crates/pocket/src/main.rs:4-41` (module list)
- Modify: `packages/desktop/crates/pocket/src/desktop/chrome.rs:15-22` (`Screen`)
- Modify: `packages/desktop/crates/pocket/src/actions.rs:8` (`actions!`)
- Modify: `packages/desktop/crates/pocket/src/settings/keybindings.rs:63-64` (labels)
- Modify: `packages/desktop/crates/pocket/src/desktop.rs` (field ~line 113, constructor ~line 250, escape chain ~line 654, `on_action` list ~line 676)
- Modify: `packages/desktop/crates/pocket/src/inbox.rs:25` (make `first_line` reusable)

**Context:** Screens are entered by an action handler that sets `self.screen` (see `open_board` at `packages/desktop/crates/pocket/src/board.rs:178`). `Desktop` composes one state struct per feature (`inbox`, `automations`, `board` fields at `desktop.rs:111-113`). `focus_agent(id, window, cx)` at `desktop.rs:276` selects the session's worktree, shows its tab, focuses the pane and sets `Screen::Sessions`. Unlike the Board, Activity needs no pocketd capability, so it is always offered. A session's project comes from `project_of(cwd, &projects)` and its worktree from `worktree_of(cwd)` (`desktop/project.rs:48,52`); `git::Worktree` has `path`, `branch`, `main`. PRs live in `self.prs` keyed by worktree path (`git_ui/pull_requests.rs:79`, `get(tree)`); `git::github::PrState::Open` covers drafts too (`draft` is a separate flag). Inbox already turns an agent into a one-line note (`inbox.rs:34-50`); reuse its `first_line`.

**Step 1: Write the failing test** — the shortcut table must label every action, and the palette must offer the screen. Add to the tests in `packages/desktop/crates/pocket/src/settings/keybindings.rs` (inside `mod tests`):

```rust
    // Unbound actions still list in Keyboard settings so they can be given a key.
    #[test]
    fn go_to_activity_has_a_label() {
        assert_eq!(label("desktop::OpenActivity"), Some(("Go to Activity", Topic::Navigation)));
    }
```

**Step 2: Run it to verify it fails**

Run: `cargo test -p pocket settings::keybindings::tests::go_to_activity_has_a_label`
Expected: FAIL — `assertion left == right failed` (left: `None`)

**Step 3: Write the implementation**

`main.rs`: add `mod activity;` above `mod accounts;`:

```rust
mod activity;
mod accounts;
```

`desktop/chrome.rs:15-22`:

```rust
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Screen {
    Sessions,
    Inbox,
    Activity,
    Automations,
    Board,
    Settings,
}
```

`actions.rs:8`: add `OpenActivity` right before `OpenAutomations` in the `actions!` list:

```rust
actions!(desktop, [OpenPalette, GoToFile, OpenSession, StartSession, NextNeedsYou, GoToUpNext, NextSession, PrevSession, ToggleRail, ToggleSidebar, ToggleFocus, NewWorktree, ProjectSettings, OpenSettings, FocusSearch, OpenActivity, OpenAutomations, OpenBoard, CheckForUpdates, NewTab, CopySelection, SelectAll, Paste, CloseTab, Save, Quit, NewBrowser, FocusAddress, Reload, Back, Forward, SplitRight, SplitDown, PrevTab, NextTab, ZoomPane, EqualizePanes, AddToChat, PrevPrompt, NextPrompt, ZoomIn, ZoomOut, ResetZoom, CopyWorktreePath, FormatBold, FormatItalic, FormatCode, FormatLink]);
```

`settings/keybindings.rs`, in `label()` next to `OpenAutomations`:

```rust
        "OpenActivity" => ("Go to Activity", Navigation),
        "OpenAutomations" => ("Go to Automations", Navigation),
```

`inbox.rs:25`: change `fn first_line` to `pub(crate) fn first_line` (body unchanged).

Create `activity.rs`:

```rust
mod card;
mod column;
pub(crate) mod logic;
mod page;

use crate::actions::OpenActivity;
use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use crate::inbox::first_line;
use crate::status::{self, Status};
use agents::Summary;
use git::github::PrState;
use gpui_kit::*;
use logic::{COLS, Columns, Item, arrange};

pub struct ActivityState {
    items: Vec<Item>,
    columns: Columns,
    lists: Vec<ListState>,
}

impl ActivityState {
    pub fn new() -> Self {
        Self { items: Vec::new(), columns: Default::default(), lists: COLS.iter().map(|_| ListState::new(0, ListAlignment::Top, px(400.))).collect() }
    }

    /// Takes the sessions as they are now; the lists measure afresh only when what they show changed.
    fn regroup(&mut self, items: Vec<Item>) {
        let columns = arrange(&items);
        if items == self.items && columns == self.columns {
            return;
        }
        // A card's height follows its note, so the lists measure again, keeping where each was scrolled to.
        for (list, line) in self.lists.iter().zip(&columns) {
            let top = list.logical_scroll_top();
            list.reset(line.len());
            list.scroll_to(top);
        }
        (self.items, self.columns) = (items, columns);
    }

    fn shown(&self, col: logic::Col, ix: usize) -> Option<&Item> {
        self.columns[col as usize].get(ix).and_then(|&i| self.items.get(i))
    }
}

impl Desktop {
    pub(crate) fn open_activity(&mut self, _: &OpenActivity, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        if self.overlay.is_some() {
            self.close_overlay(window, cx);
        }
        self.screen = Screen::Activity;
        window.focus(&self.root, cx);
        cx.notify();
    }

    pub(crate) fn leave_activity(&mut self, cx: &mut Context<Self>) {
        self.screen = Screen::Sessions;
        cx.notify();
    }

    pub(crate) fn escape_activity(&mut self, cx: &mut Context<Self>) {
        self.leave_activity(cx);
    }

    /// Every session with a terminal, as the page shows it.
    fn activity_items(&self) -> Vec<Item> {
        let projects = self.projects();
        self.agents
            .list
            .iter()
            .filter_map(|a| {
                let cwd = &self.terminals.sessions.get(&a.terminal_id)?.info.cwd;
                let project = self.project_of(cwd, &projects).cloned().unwrap_or_else(|| cwd.clone());
                let tree = self.worktree_of(cwd);
                let status = Status::of(a);
                let pr_open = tree.and_then(|w| self.prs.get(&w.path)).is_some_and(|pr| pr.state == PrState::Open);
                Some(Item {
                    id: a.id.clone(),
                    status,
                    pr_open,
                    at: a.updated_at,
                    title: status::title(a),
                    project_name: self.repo_name(&project),
                    project,
                    branch: tree.map(|w| w.branch.clone()).unwrap_or_default(),
                    tree: tree.map(|w| w.path.clone()).unwrap_or_default(),
                    provider: a.provider.clone(),
                    note: self.activity_note(a, status),
                })
            })
            .collect()
    }

    fn activity_note(&self, a: &Summary, status: Option<Status>) -> String {
        match status {
            Some(Status::NeedsYou) => self.agents.pending.iter().find(|p| p.agent_id == a.id).map(|p| p.ask()).unwrap_or_default(),
            Some(Status::Failed) => first_line(self.agents.last_result(&a.id).map_or("", |r| r.error.as_str())),
            Some(Status::Working) => a.activity.clone(),
            Some(Status::Done) => first_line(self.agents.last_text(&a.id).unwrap_or_default()),
            Some(Status::Idle) | None => String::new(),
        }
    }
}
```

Create placeholder child modules so the crate builds; Task 1.3 fills them. `activity/page.rs`:

```rust
use crate::desktop::Desktop;
use gpui_kit::*;

impl Desktop {
    pub(crate) fn activity_page(&mut self, _cx: &mut Context<Self>) -> Div {
        let items = self.activity_items();
        self.activity.regroup(items);
        div().flex_1()
    }
}
```

`activity/column.rs` and `activity/card.rs`: each a single line `use crate::desktop::Desktop;` is not allowed (unused import warning). Create them empty for now (zero bytes).

`desktop.rs`:
- imports: add `use crate::activity::ActivityState;` next to `use crate::automations::AutomationsState;`
- struct field after `pub(crate) inbox: InboxState,`: `pub(crate) activity: ActivityState,`
- constructor, after `inbox: InboxState::new(cx),`: `activity: ActivityState::new(),`
- `main_view` (~line 531), add after the Inbox arm:

```rust
            None if matches!(self.screen, Screen::Activity) => self.activity_page(cx),
```

- escape chain in `render` (~line 652), add before the `Screen::Automations` branch:

```rust
                } else if this.overlay.is_none() && this.screen == Screen::Activity {
                    this.escape_activity(cx);
```

- `on_action` list, before `.on_action(cx.listener(Self::open_automations))`:

```rust
            .on_action(cx.listener(Self::open_activity))
```

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p pocket settings::keybindings && cargo test -p pocket activity::logic`
Expected: PASS (keybindings tests including `go_to_activity_has_a_label`; 6 tests in `activity::logic`)

Clippy waits for Task 1.3: until the columns draw, `ActivityState::shown` and the list states are unused and warn. If clippy then flags `new` without `Default` on `ActivityState`, add `impl Default for ActivityState { fn default() -> Self { Self::new() } }` right below the struct.

### Task 1.3: Columns and cards

**Files:**
- Modify: `packages/desktop/crates/pocket/src/activity/page.rs`
- Modify: `packages/desktop/crates/pocket/src/activity/column.rs`
- Modify: `packages/desktop/crates/pocket/src/activity/card.rs`
- Modify: `packages/desktop/crates/pocket/src/board.rs:14` (`mod parts;` → `pub(crate) mod parts;`)
- Modify: `packages/desktop/crates/pocket/src/board/parts.rs` (`CARD_RADIUS`, `pill`, `agent_avatar`, `quick_button` from `pub(super)` to `pub(crate)`)

**Context:** Copy the Board's shapes: a column is a `list` over a `ListState` (variable-height rows, only visible ones built) inside a rounded `FILL_1` box (`board/column.rs:438-460`); a card is a `SURFACE` box with `ui::row_shadow()` (`board/card.rs:635-649`). Reuse `pill`, `agent_avatar`, `CARD_RADIUS` and (in PR 2) `quick_button` from `board/parts.rs` instead of copying them. `crate::git_ui::pull_requests::glyph(pr)` gives the PR icon and colour; `self.repos.get(tree)` holds a worktree's changed files (`r.totals()` gives added/removed). `crate::util::{ago, now_ms}` format "12m". Icons are SVGs under `packages/desktop/crates/theme/assets/icons/`, loaded by name with `icon(name, size, color)`. Clicking a card opens its session for now; PR 2 replaces that with the preview panel. There are no render tests in this repo; verify with the capture in Task 1.5.

**Step 1:** No unit test (view code). The rules it shows are tested in Task 1.1.

**Step 2:** n/a

**Step 3: Write the implementation**

`board.rs:14`: `pub(crate) mod parts;`

`board/parts.rs`: change these four to `pub(crate)` (bodies unchanged):

```rust
pub(crate) const CARD_RADIUS: f32 = 9.;
pub(crate) fn pill() -> Div {
pub(crate) fn agent_avatar(id: impl Into<ElementId>, provider: &str, name: String) -> Stateful<Div> {
pub(crate) fn quick_button(id: impl Into<ElementId>, lead: Option<&'static str>, label: &'static str) -> Stateful<Div> {
```

`activity/page.rs`:

```rust
use super::logic::COLS;
use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(crate) fn activity_page(&mut self, cx: &mut Context<Self>) -> Div {
        let items = self.activity_items();
        self.activity.regroup(items);
        let columns: Vec<_> = COLS.into_iter().map(|col| self.activity_column(col, cx)).collect();
        let body = div().id("activity-columns").flex_1().min_h_0().overflow_x_scroll().px(px(12.)).pb(px(8.)).flex().gap(px(8.)).children(columns);
        div().relative().flex_1().min_w_0().min_h_0().flex().flex_col().bg(CANVAS).child(self.activity_bar(cx)).child(body)
    }

    fn activity_bar(&self, cx: &mut Context<Self>) -> Div {
        let title = div().ml(px(2.)).flex_1().min_w_0().flex().items_center().gap(px(7.)).text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(icon("activity", 14., TEXT_3)).child("Activity");
        self.detail_bar(title, div(), cx).h(px(44.))
    }
}
```

`activity/column.rs`:

```rust
use super::card::col_glyph;
use super::logic::Col;
use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(super) fn activity_column(&mut self, col: Col, cx: &mut Context<Self>) -> Stateful<Div> {
        let count = self.activity.columns[col as usize].len();
        let head = div().h(px(38.)).flex_none().pl(px(12.)).pr(px(8.)).flex().items_center().gap(px(8.)).text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child(col_glyph(col)).child(col.name()).child(div().font_weight(FontWeight::NORMAL).text_color(TEXT_3).child(count.to_string()));
        let empty = (count == 0).then(|| div().mx(px(8.)).min_h(px(72.)).flex().flex_none().items_center().justify_center().px(px(18.)).py(px(12.)).rounded(px(9.)).border_1().border_dashed().border_color(SEPARATOR).text_center().text_size(px(12.)).line_height(px(18.)).text_color(TEXT_4).child(col.hint()));
        let cards = list(self.activity.lists[col as usize].clone(), cx.processor(move |this, ix, _, cx| this.activity_card(col, ix, cx))).flex_1().w_full();
        div().id(("activity-col", col as usize)).flex_1().min_w(px(240.)).max_w(px(340.)).h_full().flex().flex_col().rounded(px(10.)).bg(FILL_1).child(head).children(empty).child(cards)
    }
}
```

`activity/card.rs`:

```rust
use super::logic::{Col, Item};
use crate::board::parts::{CARD_RADIUS, agent_avatar, pill};
use crate::desktop::Desktop;
use crate::git_ui::pull_requests::glyph as pr_glyph;
use crate::status::Status;
use crate::util::{ago, now_ms};
use gpui_kit::*;
use theme::*;

/// A column's mark, and its cards' too.
pub(super) fn col_glyph(col: Col) -> Svg {
    let (name, color) = match col {
        Col::Idle => ("status-todo", TEXT_4),
        Col::Working => ("status-half", WORKING),
        Col::NeedsYou => ("status-needs", WAITING_DOT),
        Col::Done => ("status-three-quarters", SUCCESS),
        Col::InReview => ("pull-request", GOAL_TEXT),
    };
    icon(name, 14., color)
}

/// A card's mark: its column's, but a failed turn shows as failed.
fn card_glyph(col: Col, item: &Item) -> Svg {
    if item.status == Some(Status::Failed) { icon("status-closed", 14., FAILED) } else { col_glyph(col) }
}

impl Desktop {
    pub(super) fn activity_card(&self, col: Col, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(item) = self.activity.shown(col, ix) else { return div().into_any_element() };
        let id = item.id.clone();
        let top = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .min_h(px(18.))
            .text_size(px(11.5))
            .text_color(TEXT_3)
            .child(div().min_w_0().truncate().child(item.project_name.clone()))
            .child(div().flex_1())
            .child(agent_avatar(SharedString::from(format!("activity-agent-{id}")), &item.provider, self.agent_name(&item.provider)))
            .child(div().flex_none().child(ago(item.at, now_ms())));
        let title = div().min_w_0().flex_1().text_size(px(13.)).line_height(px(18.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).line_clamp(2).child(item.title.clone());
        let head = div().flex().items_start().gap(px(7.)).child(div().flex_none().h(px(18.)).flex().items_center().child(card_glyph(col, item))).child(title);
        let branch = (!item.branch.is_empty()).then(|| div().flex().items_center().gap(px(5.)).text_size(px(11.)).font_family(MONO).text_color(TEXT_3).child(icon("branch", 11., TEXT_4)).child(div().min_w_0().truncate().child(item.branch.clone())));
        let tone = match (col, item.status) {
            (_, Some(Status::Failed)) => FAILED_TEXT,
            (Col::NeedsYou, _) => WAITING_TEXT,
            _ => TEXT_3,
        };
        let note = (!item.note.is_empty()).then(|| div().text_size(px(12.)).line_height(px(16.)).text_color(tone).line_clamp(2).child(item.note.clone()));
        let meta = div().flex().flex_wrap().items_center().gap(px(6.)).children(self.activity_pills(item));
        let body = div().flex().flex_col().gap(px(6.)).child(top).child(head).children(branch).children(note).child(meta);
        let card = div()
            .id(SharedString::from(format!("activity-{id}")))
            .px(px(10.))
            .py(px(9.))
            .rounded(px(CARD_RADIUS))
            .bg(SURFACE)
            .shadow(ui::row_shadow())
            .cursor_pointer()
            .hover(|s| s.shadow(vec![ui::ring(SEPARATOR_STRONG, 1.)]))
            .child(body)
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.focus_agent(&id, window, cx)));
        div().w_full().px(px(8.)).pb(px(6.)).child(card).into_any_element()
    }

    /// Its PR and its uncommitted diff, when it has them.
    fn activity_pills(&self, item: &Item) -> Vec<Div> {
        let pr = self.prs.get(&item.tree).map(|pr| {
            let (name, color) = pr_glyph(pr);
            pill().child(icon(name, 12., color)).child(format!("#{}", pr.number))
        });
        let diff = self.repos.get(&item.tree).filter(|r| !r.files.is_empty()).map(|r| {
            let (added, removed) = r.totals();
            pill().font_family(MONO).child(ui::meta_diff(added, removed, 11.))
        });
        [pr, diff].into_iter().flatten().collect()
    }
}
```

Create `packages/desktop/crates/theme/assets/icons/activity.svg` (Lucide "activity", same licence header as its neighbours):

```svg
<!-- @license lucide-static v1.55.0 - ISC -->
<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2"/></svg>
```

**Step 4: Build**

Run: `cargo build -p pocket && cargo clippy -p pocket --all-targets`
Expected: builds; no new warnings.

### Task 1.4: Ways in — sidebar, rail, palette

**Files:**
- Modify: `packages/desktop/crates/pocket/src/sidebar.rs:283-314`
- Modify: `packages/desktop/crates/pocket/src/sidebar/rail.rs:28-104`
- Modify: `packages/desktop/crates/pocket/src/palette.rs` (`Pick` ~line 21, entries ~line 262, dispatch ~line 383, tests ~lines 703-735)

**Context:** The sidebar's Board and Automations rows are `ui::nav_row` built in `sidebar.rs:283-291` and added to the aside at 313-314. The compact layout's rail has the same buttons (`sidebar/rail.rs`). The palette lists "Go to …" entries in `action_entries` and dispatches them in `palette.rs:384-385`; its tests pin the exact list, so they change with it. Activity is always offered (no capability check).

**Step 1: Update the palette tests first** (they now expect Activity). In `palette.rs` tests:

In `actions_match_on_their_titles`, insert after `(Pick::Browser, "Open browser"),`:

```rust
            (Pick::Activity, "Go to Activity"),
```

In `pair_phone_is_offered_only_to_the_owner`, replace the three `assert_eq!` lines with:

```rust
        assert_eq!(picks(&owner), vec![Pick::New, Pick::Split, Pick::Next, Pick::UpNext, Pick::Browser, Pick::Activity, Pick::Automations, Pick::PairPhone, Pick::HostFolder]);
        assert_eq!(picks(&["observe"]), vec![Pick::Next, Pick::UpNext, Pick::Browser, Pick::Activity, Pick::Automations, Pick::HostFolder]);
        assert_eq!(picks(&[]), vec![Pick::New, Pick::Split, Pick::Next, Pick::UpNext, Pick::Browser, Pick::Activity, Pick::Automations, Pick::HostFolder]);
```

**Step 2: Run to verify they fail**

Run: `cargo test -p pocket palette`
Expected: FAIL to compile — `no variant or associated item named Activity found for enum Pick`

**Step 3: Write the implementation**

`palette.rs`, `enum Pick`: add `Activity,` before `Automations,`.

`action_entries`, insert after the `Pick::Browser` entry:

```rust
        Entry { pick: Pick::Activity, lead: Lead::Icon("activity"), title: "Go to Activity".into(), detail: String::new(), keys: None },
```

Dispatch, before `Pick::Automations => …`:

```rust
            Pick::Activity => self.open_activity(&crate::actions::OpenActivity, window, cx),
```

`sidebar.rs`, after the `board` row (~line 291):

```rust
        let activity = ui::nav_row("aside-activity", "activity", "Activity", self.screen == Screen::Activity)
            .mb(px(2.))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_activity(&crate::actions::OpenActivity, window, cx)));
```

and in the aside chain, right after `.child(search)`:

```rust
            .child(activity)
```

`sidebar/rail.rs`, add next to `toggle_board`:

```rust
    fn toggle_activity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.screen == Screen::Activity {
            self.leave_activity(cx);
        } else {
            self.open_activity(&crate::actions::OpenActivity, window, cx);
        }
    }
```

in `nav`, after the `inbox` button:

```rust
        let activity = rail_button("nav-activity", "activity", self.screen == Screen::Activity).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_activity(window, cx)));
```

and in the column chain right after `.child(inbox)`:

```rust
            .child(activity)
```

**Step 4: Run to verify they pass**

Run: `cargo test -p pocket palette`
Expected: PASS

### Task 1.5: Capture step, docs, and the full check

**Files:**
- Modify: `packages/desktop/crates/pocket/src/capture.rs:36` (`STEPS` length) and ~line 511 (new step)
- Modify: `CONTEXT.md` (new **Activity** section before `### Board`, line ~104)
- Modify: `docs/adr/0003-desktop-code-layout.md` (the `pocket` module map, next to the `board` row)

**Context:** `STEPS` is a fixed-size array; adding a step means bumping its length (267 → 268). `reset` (capture.rs ~1416) sets `Screen::Sessions` before each screen, so no reset change is needed. The `prs` step gives the current project's non-main worktrees PRs, which fills In review.

**Step 1:** Add the step after `("inbox", …)`:

```rust
    ("activity", |d, window, cx| d.open_activity(&crate::actions::OpenActivity, window, cx)),
```

and change `const STEPS: [(&str, Step); 267]` to `268`.

**Step 2:** `CONTEXT.md`, insert before `### Board`:

```markdown
### Activity

**Activity**:
One screen of every session across projects, in columns by where it stands: Idle, Working, Needs you, Done and In review. A session sits in the column its Status says, Failed with Needs you. An Idle session whose worktree's branch has an open or draft PR is In review instead: it waits on other people. That differs from the Board's In review, which holds every idle ticket. Opening a card previews the session and marks a Done or Failed one seen; Open session goes to its pane.
_Avoid_: Sessions page, Agents, dashboard
```

`docs/adr/0003-desktop-code-layout.md`, add a row to the `pocket` module table right above the `board` row:

```markdown
| `activity` | the Activity screen: `activity/logic` the pure rules (which column a session is in, their order, the filters), `page`, `column`, `card`, `panel` the preview beside the columns, `filter_bar` |
```

**Step 3: Run the full check** (from `packages/desktop`)

Run: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: build OK, no new clippy warnings, all tests pass.

**Step 4: Capture** (from the repo root)

Run: `.ui-review/fixture/capture.sh /tmp/activity-pr1 activity=activity,prs`
Expected: `/tmp/activity-pr1/impl-activity.png` shows five columns; the fixture's needsYou and failed agents under Needs you, working ones under Working, idle ones under Idle, and idle agents on PR'd worktrees under In review. Compare against `.ui-review/prototypes/sessions-manager.html?v=1`.

---

## PR 2: Preview panel, Open session, Allow/Deny

**Scope:** Clicking a card opens a side panel (summary, pending ask, last message, PR) instead of jumping away; opening it marks a Done or Failed session seen. Open session (button, ↵ or ⌘↵) goes to the pane. Permission asks get Allow/Deny on the card and in the panel. Esc closes the panel, then leaves Activity.
**Depends on:** PR 1
**Done when:** suite green; the `activity-panel` capture shows the panel over the columns.

### Task 2.1: Which sessions opening marks seen

**Files:**
- Modify: `packages/desktop/crates/pocket/src/activity/logic.rs`

**Context:** pocketd owns "seen": `self.outbox.seen(&[id])` (`hosts/src/session.rs:221`) clears an unseen Done, so pocketd reports it Idle. Inbox marks a row seen on click unless it Needs you (`inbox/list.rs`). Activity does the same on opening the panel.

**Step 1: Write the failing test** (add to `mod tests` in `activity/logic.rs`)

```rust
    // Previewing clears what was waiting to be seen, never a question.
    #[test]
    fn opening_marks_only_a_done_or_failed_turn_seen() {
        assert!(marks_seen(Some(Status::Done)));
        assert!(marks_seen(Some(Status::Failed)));
        assert!(!marks_seen(Some(Status::NeedsYou)));
        assert!(!marks_seen(Some(Status::Working)));
        assert!(!marks_seen(Some(Status::Idle)));
        assert!(!marks_seen(None));
    }
```

**Step 2: Run to verify it fails**

Run: `cargo test -p pocket activity::logic`
Expected: FAIL to compile — `cannot find function marks_seen`

**Step 3: Write the implementation** (in `activity/logic.rs`, after `arrange`)

```rust
/// Opening a session's preview marks it seen when an unseen turn is all it was waiting on.
pub(crate) fn marks_seen(status: Option<Status>) -> bool {
    matches!(status, Some(Status::Done | Status::Failed))
}
```

**Step 4: Run to verify it passes**

Run: `cargo test -p pocket activity::logic`
Expected: PASS (7 tests)

### Task 2.2: Panel state, open, close, Open session

**Files:**
- Modify: `packages/desktop/crates/pocket/src/activity.rs`
- Modify: `packages/desktop/crates/pocket/src/activity/card.rs` (card click)
- Modify: `packages/desktop/crates/pocket/src/desktop.rs` (escape call ~line 654, `on_key_down` ~line 662)

**Context:** The Board handles its keys in `board_key` (`board.rs:256`), called from `on_key_down` in `desktop.rs` render; a key it handles returns `true` and stops propagation. Plain ↵ and ⌘↵ open the previewed session only while the window's root has focus (not while typing in a field). `focus_agent` switches to `Screen::Sessions` itself.

**Step 1:** No unit test: the rule is `marks_seen` (Task 2.1); the rest is wiring checked by capture in Task 2.4.

**Step 3: Write the implementation**

`activity.rs` — add the field and its upkeep:

```rust
pub struct ActivityState {
    /// The session the preview panel shows.
    pub(crate) open: Option<String>,
    items: Vec<Item>,
    columns: Columns,
    lists: Vec<ListState>,
}
```

in `new`: `open: None,`

at the end of `regroup` (after the assignment), drop a preview whose session is gone:

```rust
        if self.open.as_ref().is_some_and(|id| !self.items.iter().any(|i| &i.id == id)) {
            self.open = None;
        }
```

add to `impl ActivityState`:

```rust
    fn item(&self, id: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }
```

`activity.rs` — imports: add `use logic::marks_seen;` and `use agents::Decision;`. Replace `leave_activity` and `escape_activity`, and add the new methods to `impl Desktop`:

```rust
    pub(crate) fn leave_activity(&mut self, cx: &mut Context<Self>) {
        self.activity.open = None;
        self.screen = Screen::Sessions;
        cx.notify();
    }

    /// Esc closes the preview first, then leaves the page.
    pub(crate) fn escape_activity(&mut self, cx: &mut Context<Self>) {
        if self.activity.open.take().is_some() {
            cx.notify();
        } else {
            self.leave_activity(cx);
        }
    }

    fn preview_session(&mut self, id: &str, cx: &mut Context<Self>) {
        if marks_seen(self.agents.get(id).and_then(Status::of)) {
            self.outbox.seen(&[id.to_string()]);
        }
        self.activity.open = Some(id.to_string());
        cx.notify();
    }

    fn open_previewed(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.activity.open = None;
        self.focus_agent(id, window, cx);
    }

    /// Answers the ask the button was drawn for, never one that has replaced it since.
    fn answer_activity(&mut self, request: &str, decision: Decision, cx: &mut Context<Self>) {
        if self.agents.pending.iter().any(|p| p.request_id == request) {
            self.outbox.resolve(request, decision);
        }
        cx.notify();
    }

    /// ↵ or ⌘↵ opens the previewed session, unless a field has the keyboard.
    pub(crate) fn activity_key(&mut self, key: &Keystroke, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let plain_or_command = !key.modifiers.modified() || key.modifiers == Modifiers::command();
        if self.screen != Screen::Activity || self.overlay.is_some() || !self.root.is_focused(window) || key.key != "enter" || !plain_or_command {
            return false;
        }
        let Some(id) = self.activity.open.clone() else { return false };
        self.open_previewed(&id, window, cx);
        true
    }
```

`activity/card.rs` — the card now previews, and the previewed one wears a ring. Add `use gpui_kit::prelude::FluentBuilder as _;` to the imports, and replace the `let card = …;` statement with:

```rust
        let previewed = self.activity.open.as_deref() == Some(item.id.as_str());
        let card = div()
            .id(SharedString::from(format!("activity-{id}")))
            .px(px(10.))
            .py(px(9.))
            .rounded(px(CARD_RADIUS))
            .bg(SURFACE)
            .shadow(ui::row_shadow())
            .cursor_pointer()
            .hover(|s| s.shadow(vec![ui::ring(SEPARATOR_STRONG, 1.)]))
            .when(previewed, |d| d.shadow(vec![ui::ring(SEPARATOR_STRONG, 1.5)]))
            .child(body)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.preview_session(&id, cx)));
```

`desktop.rs` render:
- the escape branch from PR 1 already calls `this.escape_activity(cx)`; no change.
- `on_key_down`:

```rust
                if this.board_key(&ev.keystroke, window, cx) || this.activity_key(&ev.keystroke, window, cx) || this.type_into_chat(&ev.keystroke, window, cx) {
```

**Step 4: Build**

Run: `cargo build -p pocket && cargo clippy -p pocket --all-targets`
Expected: builds; no new warnings (the panel itself comes in Task 2.3, so `item` may warn as unused until then — if so, do Task 2.3 before running clippy).

### Task 2.3: The panel view and Allow/Deny

**Files:**
- Create: `packages/desktop/crates/pocket/src/activity/panel.rs`
- Modify: `packages/desktop/crates/pocket/src/activity.rs` (`mod panel;`)
- Modify: `packages/desktop/crates/pocket/src/activity/page.rs` (draw the panel)
- Modify: `packages/desktop/crates/pocket/src/activity/card.rs` (Allow/Deny on the card)

**Context:** Pending asks are `self.agents.pending` (`agents::Permission { request_id, agent_id, … }`, `ask()` gives "Run cargo test -p git"-style text, `questions()` is non-empty for multi-choice questions, which need the session's dialog). Read-only connections can't answer (`self.agents.observe_only()`). The Board's quick buttons are the model (`board/card.rs:550-568`). PR details: `crate::git_ui::pull_requests::{detail, tone}` give "Draft · 2 passed"-style text and its colour. The panel floats over the columns' right side without a scrim (the page stays usable), and closes on a click outside it.

**Step 3: Write the implementation**

`activity/panel.rs`:

```rust
use super::card::col_glyph;
use super::logic::column;
use crate::board::parts::quick_button;
use crate::desktop::Desktop;
use crate::git_ui::pull_requests::{detail, glyph as pr_glyph, tone};
use crate::status::Status;
use crate::util::{ago, now_ms};
use agents::Decision;
use gpui_kit::*;
use theme::*;
use ui::Variant;

impl Desktop {
    /// Deny and Allow once for the session's permission ask; `None` for a question, which needs its dialog.
    pub(super) fn activity_answers(&self, id: &str, cx: &mut Context<Self>) -> Option<Div> {
        let ask = self.agents.pending.iter().find(|p| p.agent_id == id).filter(|p| p.questions().is_empty() && !self.agents.observe_only())?;
        let (allow, deny) = (ask.request_id.clone(), ask.request_id.clone());
        let deny = quick_button(SharedString::from(format!("activity-deny-{id}")), None, "Deny").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            this.answer_activity(&deny, Decision::Deny, cx);
        }));
        let allow = quick_button(SharedString::from(format!("activity-allow-{id}")), None, "Allow once").bg(PRIMARY).text_color(ON_PRIMARY).hover(|s| s.bg(PRIMARY).opacity(0.9)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            this.answer_activity(&allow, Decision::Allow, cx);
        }));
        Some(div().flex().flex_none().items_center().gap(px(4.)).child(deny).child(allow))
    }

    fn activity_ask(&self, id: &str, cx: &mut Context<Self>) -> Option<Div> {
        let p = self.agents.pending.iter().find(|p| p.agent_id == id)?;
        let answer = self.activity_answers(id, cx).map(IntoElement::into_any_element).unwrap_or_else(|| div().text_size(px(12.)).text_color(TEXT_3).child("Answer it in the session.").into_any_element());
        Some(div().p(px(10.)).rounded(px(9.)).bg(WAITING_BG).flex().flex_col().gap(px(8.)).child(div().text_size(px(13.)).line_height(px(19.)).child(p.ask())).child(answer))
    }

    pub(super) fn activity_panel(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let id = self.activity.open.clone()?;
        let item = self.activity.item(&id)?;
        let col = column(item.status, item.pr_open);
        let label = if item.status == Some(Status::Failed) { "Failed" } else { col.name() };
        let chip = div().h(px(22.)).px(px(8.)).flex().flex_none().items_center().gap(px(5.)).rounded(px(11.)).bg(FILL_2).text_size(px(11.5)).font_weight(FontWeight::MEDIUM).text_color(TEXT_2).child(col_glyph(col)).child(label);
        let go = id.clone();
        let open = ui::button("activity-open-session", Variant::Primary, None, "Open session").child(ui::kbd("↵")).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_previewed(&go, window, cx)));
        let close = ui::icon_button_sized("activity-panel-close", "x", 24., TEXT_3).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.activity.open = None;
            cx.notify();
        }));
        let head = div().flex().items_center().gap(px(8.)).child(chip).child(div().flex_1()).child(open).child(close);
        let title = div().text_size(px(15.)).line_height(px(20.)).font_weight(FontWeight::SEMIBOLD).child(item.title.clone());
        let meta = div().flex().flex_wrap().items_center().gap(px(8.)).text_size(px(12.)).text_color(TEXT_3).child(item.project_name.clone()).child(self.agent_name(&item.provider)).child(ago(item.at, now_ms())).children((!item.branch.is_empty()).then(|| div().font_family(MONO).child(item.branch.clone())));
        let ask = self.activity_ask(&id, cx);
        let note = (ask.is_none() && !item.note.is_empty()).then(|| div().text_size(px(13.)).line_height(px(19.)).text_color(TEXT_2).child(item.note.clone()));
        let pr = self.prs.get(&item.tree).map(|pr| {
            let (name, color) = pr_glyph(pr);
            let line = div().flex().items_center().gap(px(6.)).text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child(icon(name, 13., color)).child(div().min_w_0().truncate().child(format!("#{} {}", pr.number, pr.title)));
            div().p(px(10.)).rounded(px(9.)).border_1().border_color(SEPARATOR).flex().flex_col().gap(px(4.)).child(line).child(div().text_size(px(12.)).text_color(tone(pr)).child(detail(pr)))
        });
        let panel = ui::pop(div().id("activity-panel"))
            .absolute()
            .top(px(52.))
            .right(px(12.))
            .bottom(px(12.))
            .w(px(400.))
            .p(px(14.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.activity.open = None;
                cx.notify();
            }))
            .child(head)
            .child(title)
            .child(meta)
            .children(ask)
            .children(note)
            .children(pr);
        Some(panel)
    }
}
```

`activity.rs`: add `mod panel;` to the module list.

`activity/page.rs` — draw the panel last, over the columns:

```rust
        div().relative().flex_1().min_w_0().min_h_0().flex().flex_col().bg(CANVAS).child(self.activity_bar(cx)).child(body).children(self.activity_panel(cx))
```

`activity/card.rs` — Allow/Deny on the card, after the pills:

```rust
        let meta = div().flex().flex_wrap().items_center().gap(px(6.)).children(self.activity_pills(item)).children(self.activity_answers(&item.id, cx).map(|a| a.ml_auto()));
```

**Step 4: Build and lint**

Run: `cargo build -p pocket && cargo clippy -p pocket --all-targets`
Expected: builds; no new warnings.

### Task 2.4: Capture the panel and run the full check

**Files:**
- Modify: `packages/desktop/crates/pocket/src/capture.rs` (`STEPS` 268 → 269, new step after `activity`)

**Step 1:** Add:

```rust
    ("activity-panel", |d, _, cx| {
        d.activity.open = d.agents.list.iter().find(|a| a.status == "needsYou").map(|a| a.id.clone());
        cx.notify();
    }),
```

**Step 2: Full check** (from `packages/desktop`)

Run: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: all green, no new warnings.

**Step 3: Capture** (repo root)

Run: `.ui-review/fixture/capture.sh /tmp/activity-pr2 activity=activity,prs activity-panel=activity,prs,activity-panel`
Expected: `impl-activity-panel.png` shows the panel on the right with the needsYou session's ask and Allow once/Deny; its card in Needs you has the same buttons and a ring.

---

## PR 3: Search and project/agent filters

**Scope:** A search field and Project and Agent filter chips in the page's bar. Filters narrow every column; counts follow. Nothing is saved across launches.
**Depends on:** PR 1 (PR 2 not required, but if merged first, keep its changes)
**Done when:** suite green; filtering in the app narrows the columns.

### Task 3.1: Filter rules

**Files:**
- Modify: `packages/desktop/crates/pocket/src/activity/logic.rs`

**Context:** `arrange` gains a `&Filters` argument; every caller and test changes with it. An empty set keeps everything. The query matches each of its words against title, project name and branch, case-insensitively.

**Step 1: Write the failing tests** (add to `mod tests`; also change the two existing `arrange(&items)` calls to `arrange(&items, &Filters::default())`)

```rust
    fn named(id: &str, project: &str, provider: &str, branch: &str) -> Item {
        Item { id: id.into(), title: format!("Fix {id}"), project: format!("/repos/{project}"), project_name: project.into(), provider: provider.into(), branch: branch.into(), ..Item::default() }
    }

    #[test]
    fn no_filter_keeps_every_session() {
        assert!(Filters::default().keeps(&named("a", "app", "claude", "main")));
    }

    #[test]
    fn a_project_filter_keeps_only_its_projects() {
        let f = Filters { projects: ["/repos/app".to_string()].into(), ..Filters::default() };
        assert!(f.keeps(&named("a", "app", "claude", "")));
        assert!(!f.keeps(&named("b", "web", "claude", "")));
    }

    #[test]
    fn an_agent_filter_keeps_only_its_agents() {
        let f = Filters { providers: ["codex".to_string()].into(), ..Filters::default() };
        assert!(f.keeps(&named("a", "app", "codex", "")));
        assert!(!f.keeps(&named("b", "app", "claude", "")));
    }

    // Every word must match somewhere: title, project or branch.
    #[test]
    fn search_matches_every_word_in_title_project_or_branch() {
        let item = named("parser", "app", "claude", "fix/lexer");
        let q = |s: &str| Filters { query: s.into(), ..Filters::default() };
        assert!(q("PARSER app").keeps(&item));
        assert!(q("lexer").keeps(&item));
        assert!(!q("parser web").keeps(&item));
    }

    #[test]
    fn filtered_out_sessions_leave_their_column() {
        let items = vec![named("a", "app", "claude", ""), named("b", "web", "claude", "")];
        let f = Filters { projects: ["/repos/web".to_string()].into(), ..Filters::default() };
        assert_eq!(ids(&items, &arrange(&items, &f), Col::Idle), ["b"]);
    }

    // A filter menu lists each value once, with how many sessions have it.
    #[test]
    fn a_menu_counts_each_value_once_in_first_seen_order() {
        let items = vec![named("a", "web", "claude", ""), named("b", "app", "codex", ""), named("c", "web", "codex", "")];
        assert_eq!(values(&items, |i| &i.provider), [("claude".to_string(), 1), ("codex".to_string(), 2)]);
        assert_eq!(values(&items, |i| &i.project), [("/repos/web".to_string(), 2), ("/repos/app".to_string(), 1)]);
    }
```

**Step 2: Run to verify they fail**

Run: `cargo test -p pocket activity::logic`
Expected: FAIL to compile — `cannot find struct Filters`, `cannot find function values`

**Step 3: Write the implementation** (in `activity/logic.rs`; add `use std::collections::BTreeSet;`)

```rust
#[derive(Clone, Default, PartialEq, Debug)]
pub(crate) struct Filters {
    pub query: String,
    /// Project paths; empty keeps every project.
    pub projects: BTreeSet<String>,
    /// Agent providers; empty keeps every agent.
    pub providers: BTreeSet<String>,
}

impl Filters {
    pub(crate) fn keeps(&self, item: &Item) -> bool {
        let text = format!("{} {} {}", item.title, item.project_name, item.branch).to_lowercase();
        (self.projects.is_empty() || self.projects.contains(&item.project))
            && (self.providers.is_empty() || self.providers.contains(&item.provider))
            && self.query.split_whitespace().all(|w| text.contains(&w.to_lowercase()))
    }
}

/// The values a filter menu offers, each with how many sessions have it, in the order they first appear.
pub(crate) fn values(items: &[Item], value: fn(&Item) -> &String) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    for item in items {
        match out.iter_mut().find(|(v, _)| v == value(item)) {
            Some((_, n)) => *n += 1,
            None => out.push((value(item).clone(), 1)),
        }
    }
    out
}
```

and change `arrange`:

```rust
/// Each column's kept items as indexes into `items`: what wants you oldest first, as Up next orders it; the rest newest first.
pub(crate) fn arrange(items: &[Item], filters: &Filters) -> Columns {
    let mut out: Columns = Default::default();
    for (i, item) in items.iter().enumerate().filter(|(_, item)| filters.keeps(item)) {
        out[column(item.status, item.pr_open) as usize].push(i);
    }
    // … the sorting loop stays as it is
    out
}
```

In `activity.rs`, `regroup` calls `arrange(&items, &self.filters)` (field added in Task 3.2).

**Step 4: Run to verify they pass**

Run: `cargo test -p pocket activity::logic`
Expected: PASS (13 tests)

### Task 3.2: Search field and filter chips

**Files:**
- Create: `packages/desktop/crates/pocket/src/activity/filter_bar.rs`
- Modify: `packages/desktop/crates/pocket/src/activity.rs` (state, `new`, `regroup`, `escape_activity`, `mod filter_bar;`)
- Modify: `packages/desktop/crates/pocket/src/activity/page.rs` (`activity_bar`)
- Modify: `packages/desktop/crates/pocket/src/desktop.rs` (`ActivityState::new` call and subs ~lines 158-202, `menu_open` and `close_menus` ~lines 465-490)
- Modify: `packages/desktop/crates/pocket/src/board.rs:7` (`mod filter_bar;` → `pub(crate) mod filter_bar;`) and `board/filter_bar.rs` (`value_row` → `pub(crate)`)

**Context:** The Board's search is an `InputState` entity subscribed in its state's `new` (`board.rs:90-100`), drawn with `Input::new(&state).appearance(false).p_0()` (`board/top_bar.rs`). Its filter menu rows are `value_row` (`board/filter_bar.rs:33`): a tick, a mark, a name and a count — reuse it. Popups use `self.popup(id, width, cx)` (`desktop/chrome.rs:228`, closes menus on a click outside) inside `ui::dropdown_right(top, ui::menu_in(id, …))`. Every popup must be closable by `close_menus` and counted by `menu_open` (`desktop.rs:465-490`), or Esc and outside clicks won't close it. `self.repo_name(path)` names a project; `self.agent_name(provider)` names an agent and `theme::provider_icon(provider, size, color)` draws its mark.

**Step 1:** No unit test (view wiring); rules are tested in Task 3.1.

**Step 3: Write the implementation**

`board.rs`: `pub(crate) mod filter_bar;` — `board/filter_bar.rs:33`: `pub(crate) fn value_row(…)` (body unchanged).

`activity.rs` — state and constructor:

```rust
use gpui_kit::component::input::{InputEvent, InputState};
use logic::{COLS, Columns, Filters, Item, arrange, marks_seen};

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Menu {
    Project,
    Agent,
}

pub struct ActivityState {
    pub(crate) open: Option<String>,
    pub(crate) filters: Filters,
    pub(crate) menu: Option<Menu>,
    items: Vec<Item>,
    columns: Columns,
    lists: Vec<ListState>,
    search: Entity<InputState>,
}

impl ActivityState {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions"));
        let subs = vec![cx.subscribe(&search, |this: &mut Desktop, s, _: &InputEvent, cx| {
            this.activity.filters.query = s.read(cx).value().to_string();
            cx.notify();
        })];
        let state = Self { open: None, filters: Filters::default(), menu: None, items: Vec::new(), columns: Default::default(), lists: COLS.iter().map(|_| ListState::new(0, ListAlignment::Top, px(400.))).collect(), search };
        (state, subs)
    }
```

(remove the `Default` impl if Task 1.2 added one; `regroup` now calls `arrange(&items, &self.filters)`.)

`escape_activity` — a focused search field lets go of the keyboard first:

```rust
    pub(crate) fn escape_activity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.activity.search.focus_handle(cx).is_focused(window) {
            window.focus(&self.root, cx);
            cx.notify();
        } else if self.activity.open.take().is_some() {
            cx.notify();
        } else {
            self.leave_activity(cx);
        }
    }
```

and in `desktop.rs` the escape branch becomes `this.escape_activity(window, cx);`.

`desktop.rs` — constructor: next to the board, `let (activity, activity_subs) = ActivityState::new(window, cx);`, `_subs.extend(activity_subs);`, and the field init `activity,` (replacing `activity: ActivityState::new(),`). In `menu_open()` append `|| self.activity.menu.is_some()`; in `close_menus()` add `self.activity.menu = None;` next to `self.board.menu = None;`.

`activity/filter_bar.rs`:

```rust
use super::Menu;
use super::logic::values;
use crate::board::filter_bar::value_row;
use crate::desktop::Desktop;
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(super) fn activity_filters(&self, cx: &mut Context<Self>) -> Div {
        let search = div().w(px(220.)).h(px(26.)).px(px(8.)).flex().items_center().gap(px(6.)).rounded(px(7.)).bg(FILL_1).child(icon("search", 12., TEXT_3)).child(div().flex_1().min_w_0().text_size(px(12.5)).child(Input::new(&self.activity.search).appearance(false).p_0()));
        let project = self.filter_chip("activity-project", "Project", self.activity.filters.projects.len(), Menu::Project, cx);
        let agent = self.filter_chip("activity-agent", "Agent", self.activity.filters.providers.len(), Menu::Agent, cx);
        div().flex().items_center().gap(px(6.)).child(search).child(project).child(agent)
    }

    fn filter_chip(&self, id: &'static str, label: &'static str, picked: usize, menu: Menu, cx: &mut Context<Self>) -> Div {
        let open = self.activity.menu == Some(menu);
        let text = if picked == 0 { label.to_string() } else { format!("{label} · {picked}") };
        let chip = div()
            .id(id)
            .h(px(26.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(5.))
            .rounded(px(13.))
            .border_1()
            .border_color(SEPARATOR)
            .cursor_pointer()
            .hover(|s| s.bg(FILL_2))
            .when(open || picked > 0, |d| d.bg(FILL_3))
            .text_size(px(12.5))
            .text_color(TEXT_2)
            .child(text)
            .child(icon("chevron-down", 11., TEXT_3))
            .capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                let was = this.activity.menu;
                this.close_menus();
                this.activity.menu = (was != Some(menu)).then_some(menu);
                cx.notify();
            }));
        div().relative().child(chip).when(open, |d| d.child(self.filter_menu(menu, cx)))
    }

    fn filter_menu(&self, menu: Menu, cx: &mut Context<Self>) -> Div {
        let field: fn(&super::logic::Item) -> &String = match menu {
            Menu::Project => |i| &i.project,
            Menu::Agent => |i| &i.provider,
        };
        let rows = values(&self.activity.items, field).into_iter().enumerate().map(|(i, (value, count))| {
            let on = match menu {
                Menu::Project => self.activity.filters.projects.contains(&value),
                Menu::Agent => self.activity.filters.providers.contains(&value),
            };
            let (lead, name) = match menu {
                Menu::Project => (icon("folder", 13., TEXT_3).into_any_element(), self.repo_name(&value)),
                Menu::Agent => (theme::provider_icon(&value, 12., TEXT_2).into_any_element(), self.agent_name(&value)),
            };
            value_row(("activity-filter", i), on, lead, &name, count).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                let set = match menu {
                    Menu::Project => &mut this.activity.filters.projects,
                    Menu::Agent => &mut this.activity.filters.providers,
                };
                if !set.remove(&value) {
                    set.insert(value.clone());
                }
                cx.notify();
            }))
        });
        ui::dropdown_right(30., ui::menu_in("activity-filter-in", self.popup("activity-filter-menu", 260., cx).children(rows)))
    }
}
```

`activity.rs`: add `mod filter_bar;`.

`activity/page.rs` — the bar shows the filters on its right:

```rust
    fn activity_bar(&self, cx: &mut Context<Self>) -> Div {
        let title = div().ml(px(2.)).flex_1().min_w_0().flex().items_center().gap(px(7.)).text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(icon("activity", 14., TEXT_3)).child("Activity");
        let filters = self.activity_filters(cx);
        self.detail_bar(title, filters, cx).h(px(44.))
    }
```

Note: the menus list values from `self.activity.items`, which holds every session (filters are applied in `arrange`, not when gathering), so a value stays in its menu while it's filtered out.

**Step 4: Full check** (from `packages/desktop`)

Run: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: all green, no new warnings.

### Task 3.3: Capture with a filter

**Files:**
- Modify: `packages/desktop/crates/pocket/src/capture.rs` (`STEPS` 269 → 270)

**Step 1:** Add after `activity-panel`:

```rust
    ("activity-filtered", |d, _, cx| {
        if let Some(p) = d.project.clone() {
            d.activity.filters.projects.insert(p);
        }
        d.activity.menu = Some(crate::activity::Menu::Project);
        cx.notify();
    }),
```

**Step 2: Capture** (repo root)

Run: `.ui-review/fixture/capture.sh /tmp/activity-pr3 activity=activity,prs activity-filtered=activity,prs,activity-filtered`
Expected: `impl-activity-filtered.png` shows the Project menu open with one project ticked, the chip reading "Project · 1", and only that project's sessions in the columns.

**Step 3:** Run `cargo test --workspace` once more (from `packages/desktop`). Expected: PASS.
