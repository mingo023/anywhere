# Compact Rail (monocode-style) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** ⌘B collapses the projects sidebar into monocode's icon rail: a project switcher popover, Sessions/Explorer/Changes icons that drive a docked column, Search, Inbox and Settings.

**Architecture:** `Layout::Compact` keeps its name but changes shape. The rail (`pocket/src/sidebar/rail.rs`) is rewritten. A new `pocket/src/sidebar/project_picker.rs` owns the switcher popover (`ProjectPicker` state, held in `SidebarState`). In Compact the existing column (`sidebar/column.rs`) sits beside the rail without its tab strip, and `SidebarState::column_hidden` replaces `Desktop::panel`. The floating panel (`sidebar/panel.rs`) and its dimmed backdrop go. Decisions live in plain functions with tests: `sidebar_toggled`, `column_shown`, `rail_pick`, `listed`, `step`, `parent`.

**Toolset** (run everything from `/Users/mingo/Developer/self/coding-pocket/packages/desktop`):
- Tests by name filter: `cargo test -p pocket <filter>`
- Gate before calling the PR done: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
- Clippy baseline: `cargo clippy --workspace --all-targets 2>&1 | grep -c '^warning'` printed **12** before Task 1.1. It must not grow.
- Screens (from the repo root `/Users/mingo/Developer/self/coding-pocket`): `.ui-review/fixture/capture.sh <dir> <name>=<step>,<step>…`
- Do NOT run `cargo fmt`. There is no rustfmt.toml; lines run to ~200 chars by hand.
- Do not commit. Do not stash.
- No comments unless the WHY can't be read from the code (`~/.claude/CLAUDE.md` §4). Docblocks only where they add a fact the signature doesn't.

**Read first:**
- `CLAUDE.md` (repo root): feature module with its own state struct; thin `impl Desktop`; tests beside the logic, named as sentences; no mocks or render tests.
- `docs/adr/0003-desktop-code-layout.md`: the module map you update in Task 1.4.
- `packages/desktop/crates/pocket/src/sidebar/rail.rs`: the rail being replaced.
- `packages/desktop/crates/pocket/src/sidebar/column.rs`: the column and its tab strip.
- `packages/desktop/crates/pocket/src/modals/new_session/picker.rs`: the popover pattern (`ui::pop`, `on_mouse_down_out`, `ui::dropdown`, `ui::menu_in`).
- `packages/desktop/crates/pocket/src/palette.rs:270-285, 367-391, 440-475`: a search `InputState` with arrow/enter handling in `capture_key_down`.
- `packages/desktop/crates/pocket/src/sidebar/row_menu.rs:55-70`: why menu toggles use `capture_any_mouse_down` + `cx.stop_propagation()` (it stops the open menu's `on_mouse_down_out` from closing it before the toggle reopens it).

**Spec (from monocode, `github.com/hardbeat920/monocode`, `src/app/shell/Sidebar.tsx` `CompactProjectRail` and `src/features/projects/ui/SearchableProjectPicker.tsx`):**
- ⌘B: Sidebars → Compact; Compact or Focus → Sidebars. ⌘. still cycles Sidebars → Compact → Focus.
- Rail top→bottom: expand-sidebar button, current project button, Sessions (`comment`), Explorer (`file`), Changes (`branch`, dot while the worktree has changes), Search (opens the palette), Inbox (count badge while unseen; clicking it while the inbox shows goes back), spacer, Settings, then the existing context-left bars and avatar.
- Project popover: 286px wide, 4px under the button, search field "Search projects…", rows (check on the current project else its mark, name, `~`-shortened parent folder, status indicator), "No projects found" when empty, footer "New project" → `Overlay::AddRepo`. Current project first, the rest in sidebar order. Case-insensitive substring match on name or path. ↑/↓ move the highlight and stop at the ends, Enter picks, Esc or a click outside closes. Each project row is followed by its worktrees (added after review: Compact hides the aside, the only other worktree switcher); picking calls `select_tree`.
- Column in Compact: docked beside the rail, no tab strip. A side icon shows the column on that side; the icon of the side already shown hides it. ⌘\ shows/hides it. While the inbox is open the column shows the inbox list regardless.
- ⌘N / ⌃Tab walk the Sessions column's list in every layout.

**Deliberate limits** (agreed; don't add):
- No slide animation for the column.
- The column's hidden state isn't saved across restarts.
- No per-tab keyboard shortcuts.
- The compose button leaves the rail (the column header's `+` and ⌘N remain).
- The context-left bars and avatar stay at the rail's foot (monocode has neither; dropping them would orphan `Desktop::initials` and the rail's usage view).

---

## PR 1: ⌘B collapses into a monocode-style rail

**Scope:** The whole feature. Ships ⌘B, the new rail, the project switcher and the docked column; removes the floating panel.
**Depends on:** nothing
**Done when:** the gate passes with ≤12 clippy warnings, and the after-screens show the rail, popover and docked column.

### Task 1.1: ⌘B toggles the projects sidebar

**Files:**
- Modify: `packages/desktop/crates/pocket/src/actions.rs:5,31`
- Modify: `packages/desktop/crates/pocket/src/desktop/chrome.rs` (imports line 1; new fn after `empty`; new method after `toggle_rail`; new tests module at the end)
- Modify: `packages/desktop/crates/pocket/src/desktop.rs` (render, next to `.on_action(cx.listener(Self::toggle_rail))`)

**Context:** `Layout` is `store::Layout` (derives `Debug, PartialEq, Copy`), re-exported from `desktop/chrome.rs`. `save_soon` persists the layout. Actions are declared in the `actions!` list and bound in `bindings()`.

**Step 0: Capture the before screens** (from the repo root, before any code change):

Run: `.ui-review/fixture/capture.sh /tmp/compact-before compact=focus compact-panel=focus,rail sidebars=session`
Expected: three PNG paths printed.

**Step 1: Write the failing test** — append to `desktop/chrome.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::{Layout, sidebar_toggled};

    #[test]
    fn cmd_b_swaps_the_projects_sidebar_for_the_rail_and_brings_it_back() {
        assert_eq!([Layout::Sidebars, Layout::Compact, Layout::Focus].map(sidebar_toggled), [Layout::Compact, Layout::Sidebars, Layout::Sidebars]);
    }
}
```

Proves Sidebars and Compact swap, and Focus comes back to Sidebars.

**Step 2: Run it to verify it fails**

Run: `cargo test -p pocket cmd_b_swaps`
Expected: compile error, `cannot find function sidebar_toggled`.

**Step 3: Implement**

`actions.rs`: add `ToggleSidebar` to the `actions!` list right after `ToggleRail`, and add the binding right above `cmd-\\`:

```rust
        KeyBinding::new("cmd-b", ToggleSidebar, None),
```

`desktop/chrome.rs`: change line 1 to

```rust
use crate::actions::{ToggleFocus, ToggleRail, ToggleSidebar};
```

Add after `pub fn empty(...)`:

```rust
pub fn sidebar_toggled(layout: Layout) -> Layout {
    match layout {
        Layout::Sidebars => Layout::Compact,
        Layout::Compact | Layout::Focus => Layout::Sidebars,
    }
}
```

Add inside `impl Desktop`, after `toggle_rail`:

```rust
    pub(crate) fn toggle_sidebar(&mut self, _: &ToggleSidebar, _: &mut Window, cx: &mut Context<Self>) {
        self.layout = sidebar_toggled(self.layout);
        self.save_soon(cx);
        cx.notify();
    }
```

`desktop.rs` render: after `.on_action(cx.listener(Self::toggle_rail))` add

```rust
            .on_action(cx.listener(Self::toggle_sidebar))
```

**Step 4: Run it to verify it passes**

Run: `cargo test -p pocket cmd_b_swaps`
Expected: PASS.

### Task 1.2: The project picker

**Files:**
- Create: `packages/desktop/crates/pocket/src/sidebar/project_picker.rs`
- Modify: `packages/desktop/crates/pocket/src/sidebar.rs` (mod list; `SidebarState`)
- Modify: `packages/desktop/crates/pocket/src/desktop.rs:359-369` (`menu_open`, `close_menus`)

**Context:** The picker is sidebar state, so it lives in `SidebarState` (built by `SidebarState::new`, which returns its subscriptions), not on `Desktop`. Adding it to `menu_open`/`close_menus` gives Esc-to-close for free (the root's `capture_key_down` calls `close_menus` on escape) and hides the native browser view while the popover is open (`browser.rs` `sync_browser` checks `menu_open`). The popover is rendered by the rail in Task 1.3; until then this code is unreferenced (dead-code warnings are expected only until Task 1.3).

**Step 1: Write the failing tests** — create `sidebar/project_picker.rs` with only the tests module first:

```rust
#[cfg(test)]
mod tests {
    use super::{listed, parent, step};
    use crate::util::basename;

    fn strings(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_current_project_leads_and_the_rest_keep_the_sidebar_order() {
        let got = listed(strings(&["/w/api", "/w/app", "/w/docs"]), Some("/w/docs"), "", |p| basename(p));
        assert_eq!(got, strings(&["/w/docs", "/w/api", "/w/app"]));
    }

    #[test]
    fn the_search_matches_a_name_or_a_path_ignoring_case() {
        let projects = strings(&["/work/api", "/self/app"]);
        let named = |p: &str| if p == "/self/app" { "Pocket".to_string() } else { basename(p) };
        assert_eq!(listed(projects.clone(), None, " API ", named), strings(&["/work/api"]));
        assert_eq!(listed(projects.clone(), None, "pock", named), strings(&["/self/app"]));
        assert_eq!(listed(projects.clone(), None, "self/", named), strings(&["/self/app"]));
        assert_eq!(listed(projects, None, "zzz", named), Vec::<String>::new());
    }

    #[test]
    fn arrows_move_the_highlight_and_stop_at_the_ends() {
        let got = [step("up", 0, 3), step("down", 0, 3), step("down", 2, 3), step("up", 2, 3), step("down", 0, 0), step("x", 1, 3)];
        assert_eq!(got, [Some(0), Some(1), Some(2), Some(1), Some(0), None]);
    }

    #[test]
    fn a_row_names_the_folder_its_project_sits_in() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(parent(&format!("{home}/work/app")), "~/work");
        assert_eq!(parent("/srv/app"), "/srv");
    }
}
```

And in `sidebar.rs` add `pub(crate) mod project_picker;` after `mod panel;`.

Proves: ordering (current first, rest kept), filtering by display name or path, clamped arrows, and the parent-folder label.

**Step 2: Run them to verify they fail**

Run: `cargo test -p pocket -- project_picker`
Expected: compile errors, `unresolved imports super::listed, super::parent, super::step`.

**Step 3: Implement** — put this above the tests module in `sidebar/project_picker.rs`:

```rust
use crate::desktop::Desktop;
use crate::desktop::chrome::{Overlay, id};
use crate::util::tilde;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::Path;
use theme::*;

pub struct ProjectPicker {
    pub(crate) open: bool,
    pub(crate) search: Entity<InputState>,
    /// The highlighted row of the filtered list.
    ix: usize,
}

impl ProjectPicker {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search projects…"));
        let subs = vec![cx.subscribe(&search, |this, _, ev: &InputEvent, cx| {
            if let InputEvent::Change = ev {
                this.sidebar.picker.ix = 0;
            }
            cx.notify()
        })];
        (Self { open: false, search, ix: 0 }, subs)
    }
}

/// `current` first, then the rest in sidebar order, kept while `query` is in their name or path.
pub(crate) fn listed(projects: Vec<String>, current: Option<&str>, query: &str, name: impl Fn(&str) -> String) -> Vec<String> {
    let query = query.trim().to_lowercase();
    let (mut first, rest): (Vec<String>, Vec<String>) = projects.into_iter().partition(|p| Some(p.as_str()) == current);
    first.extend(rest);
    first.into_iter().filter(|p| format!("{}\n{p}", name(p)).to_lowercase().contains(&query)).collect()
}

fn step(key: &str, ix: usize, total: usize) -> Option<usize> {
    match key {
        "up" => Some(ix.saturating_sub(1)),
        "down" => Some((ix + 1).min(total.saturating_sub(1))),
        _ => None,
    }
}

fn parent(path: &str) -> String {
    tilde(&Path::new(path).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default())
}

impl Desktop {
    fn picked_projects(&self, cx: &App) -> Vec<String> {
        listed(self.projects(), self.project.as_deref(), &self.sidebar.picker.search.read(cx).value(), |p| self.repo_name(p))
    }

    pub(crate) fn toggle_project_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let open = !self.sidebar.picker.open;
        self.close_menus();
        self.sidebar.picker.open = open;
        if open {
            self.sidebar.picker.ix = 0;
            self.sidebar.picker.search.update(cx, |s, cx| {
                s.set_value("", window, cx);
                s.focus(window, cx);
            });
        }
        cx.notify();
    }

    fn pick_project(&mut self, p: String, cx: &mut Context<Self>) {
        self.sidebar.picker.open = false;
        self.select_project(p, cx);
    }

    fn on_picker_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let projects = self.picked_projects(cx);
        let ix = self.sidebar.picker.ix.min(projects.len().saturating_sub(1));
        let key = ev.keystroke.key.as_str();
        if key == "enter" {
            let Some(p) = projects.into_iter().nth(ix) else { return };
            self.pick_project(p, cx);
        } else {
            let Some(ix) = step(key, ix, projects.len()) else { return };
            self.sidebar.picker.ix = ix;
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(crate) fn project_picker(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let projects = self.picked_projects(cx);
        let ix = self.sidebar.picker.ix.min(projects.len().saturating_sub(1));
        let rows = projects.iter().enumerate().map(|(i, p)| {
            let lead = if self.project.as_ref() == Some(p) {
                icon("check", 14., TEXT).into_any_element()
            } else {
                ui::repo_mark(&self.repo_name(p), false, None).size(px(18.)).text_size(px(10.)).into_any_element()
            };
            let target = p.clone();
            div()
                .id(("projects-row", i))
                .h(px(36.))
                .px(px(10.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(10.))
                .rounded(px(8.))
                .cursor_pointer()
                .text_size(px(13.))
                .when(i == ix, |d| d.bg(FILL_3).text_color(TEXT))
                .when(i != ix, |d| d.text_color(TEXT_2))
                .child(div().w(px(18.)).flex().flex_none().justify_center().child(lead))
                .child(div().flex_1().min_w_0().truncate().font_weight(FontWeight::MEDIUM).child(self.repo_name(p)))
                .child(div().max_w(px(112.)).flex_none().truncate().font_family(MONO).text_size(px(11.)).text_color(TEXT_4).child(parent(p)))
                .children(ui::indicator(id(format!("projects-state:{p}")), self.project_state(p)))
                .on_mouse_move(cx.listener(move |this, _: &MouseMoveEvent, _, cx| {
                    if this.sidebar.picker.ix != i {
                        this.sidebar.picker.ix = i;
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.pick_project(target.clone(), cx)))
        });
        let search = div()
            .h(px(44.))
            .px(px(14.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(10.))
            .border_b(px(0.5))
            .border_color(SEPARATOR)
            .child(icon("search", 15., TEXT_3))
            .child(div().flex_1().text_size(px(13.)).child(Input::new(&self.sidebar.picker.search).appearance(false).p_0().text_size(px(13.))));
        let empty = div().h(px(36.)).px(px(10.)).flex().items_center().text_size(px(13.)).text_color(TEXT_3).child("No projects found");
        let list = div().id("projects-list").max_h(px(290.)).overflow_y_scroll().p(px(5.)).flex().flex_col().when(projects.is_empty(), |d| d.child(empty)).children(rows);
        let new = ui::menu_row("projects-new", "plus", "New project", None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
            this.sidebar.picker.open = false;
            this.open(Overlay::AddRepo, window, cx);
        }));
        ui::pop(div().id("projects-menu"))
            .w(px(286.))
            .overflow_hidden()
            .flex()
            .flex_col()
            .occlude()
            .capture_key_down(cx.listener(Self::on_picker_key))
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.sidebar.picker.open = false;
                cx.notify();
            }))
            .child(search)
            .child(list)
            .child(div().p(px(5.)).border_t(px(0.5)).border_color(SEPARATOR).child(new))
    }
}
```

`sidebar.rs` — import and hold the picker:

```rust
use crate::sidebar::project_picker::ProjectPicker;
```

```rust
pub struct SidebarState {
    pub(crate) search: Entity<InputState>,
    /// Where a right-click opened the row menu; `None` drops it under its `···` button.
    pub(crate) menu_at: Option<Point<Pixels>>,
    pub(crate) picker: ProjectPicker,
}

impl SidebarState {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions…"));
        let (picker, picker_subs) = ProjectPicker::new(window, cx);
        let mut subs = vec![cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify())];
        subs.extend(picker_subs);
        (Self { search, menu_at: None, picker }, subs)
    }
}
```

`desktop.rs` `menu_open` / `close_menus`:

```rust
    pub(crate) fn menu_open(&self) -> bool {
        self.panels.menu.is_some() || self.panels.actions.is_some() || self.row_menu.is_some() || self.changes.commit_menu || self.changes.menu || self.sidebar.picker.open
    }

    /// Returns whether a menu was open.
    pub(crate) fn close_menus(&mut self) -> bool {
        let open = self.menu_open();
        (self.panels.menu, self.panels.actions, self.row_menu, self.changes.commit_menu, self.changes.menu) = (None, None, None, false, false);
        self.sidebar.menu_at = None;
        self.sidebar.picker.open = false;
        open
    }
```

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p pocket -- project_picker`
Expected: 4 passed. (`cargo build` may warn that `toggle_project_picker` / `project_picker` are unused; Task 1.3 uses them.)

### Task 1.3: The new rail and the docked column

**Files:**
- Modify (rewrite): `packages/desktop/crates/pocket/src/sidebar/rail.rs`
- Modify: `packages/desktop/crates/pocket/src/sidebar.rs` (imports; `mod panel;` removed; `SidebarState.column_hidden`; `column_shown` + test)
- Delete: `packages/desktop/crates/pocket/src/sidebar/panel.rs`
- Modify: `packages/desktop/crates/pocket/src/sidebar/column.rs` (tab strip hidden in Compact)
- Modify: `packages/desktop/crates/pocket/src/desktop/chrome.rs` (`Side` derives `Debug`; `toggle_rail`; `toggle_focus`)
- Modify: `packages/desktop/crates/pocket/src/desktop.rs` (drop `panel` field and init; render)
- Modify: `packages/desktop/crates/pocket/src/browser.rs:146`
- Modify: `packages/desktop/crates/pocket/src/capture.rs:166`
- Modify: `packages/desktop/crates/pocket/src/desktop/jump.rs` (`walkable` loses its Compact branch)

**Context:** `Desktop::panel` drove the floating panel; it goes, and `SidebarState::column_hidden` (default `false`) says whether the user hid Compact's column. `Screen::Inbox` makes `column_view` render the inbox list, so the column must show while the inbox is open even if hidden. `live` (rail.rs) only served the old rail and ⌘N's Compact branch; both go, so ⌘N walks the Sessions column's list in every layout. The old `a_status_change_keeps_the_session_order` test goes with `live`.

**Step 1: Write the failing tests**

`desktop/chrome.rs`: add `Debug` to `Side`'s derive:

```rust
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Side {
```

Replace the whole `#[cfg(test)] mod tests` in `sidebar/rail.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::rail_pick;
    use crate::desktop::chrome::Side;

    #[test]
    fn a_side_icon_shows_its_side_and_the_shown_sides_icon_hides_the_column() {
        let got = [rail_pick(Side::Sessions, true, Side::Explorer), rail_pick(Side::Explorer, true, Side::Explorer), rail_pick(Side::Explorer, false, Side::Explorer)];
        assert_eq!(got, [(Side::Explorer, false), (Side::Explorer, true), (Side::Explorer, false)]);
    }
}
```

In `sidebar.rs` tests, change the `use super::{...}` line to `use super::{ProjectRow, column_shown, in_tree, setting_up};`, add `use crate::desktop::chrome::{Layout, Screen};`, and add:

```rust
    #[test]
    fn compact_shows_the_column_unless_hidden_and_always_for_the_inbox() {
        let got = [
            column_shown(Layout::Sidebars, Screen::Sessions, true),
            column_shown(Layout::Compact, Screen::Sessions, false),
            column_shown(Layout::Compact, Screen::Sessions, true),
            column_shown(Layout::Compact, Screen::Inbox, true),
            column_shown(Layout::Focus, Screen::Sessions, false),
        ];
        assert_eq!(got, [true, true, false, true, false]);
    }
```

In `desktop/jump.rs` tests, replace `ids` and `cmd_n_counts_the_filtered_list_and_the_rail_in_compact` with:

```rust
    fn ids(query: &str) -> Vec<String> {
        let cards = vec![card("a", "idle", "/app"), card("b", "working", "/app-wt"), card("c", "done", "/app"), card("d", "idle", "/app")];
        walkable(cards, Some("/app"), |cwd| Some(cwd.to_string()), query).into_iter().map(|c| c.id).collect()
    }

    #[test]
    fn cmd_n_counts_the_filtered_list() {
        assert_eq!(ids(""), vec!["a", "c", "d"]);
        assert_eq!(ids("fix c"), vec!["c"]);
    }
```

and drop `use crate::desktop::chrome::Layout;` from that tests module.

**Step 2: Run them to verify they fail**

Run: `cargo test -p pocket -- rail_pick column_shown cmd_n_counts`
Expected: compile errors (`rail_pick`, `column_shown` not found; `walkable` takes 6 arguments).

**Step 3: Implement**

`sidebar/rail.rs` — replace everything above the tests module with:

```rust
use crate::desktop::Desktop;
use crate::desktop::chrome::{Overlay, Screen, Side, drag_area};
use crate::sidebar::column::{changes_badge, changes_dot};
use crate::terminal_view::context;
use agents::Level;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

/// `clicked`'s icon, with the column `shown` on `side`: the side to show, and whether to hide the column.
pub(crate) fn rail_pick(side: Side, shown: bool, clicked: Side) -> (Side, bool) {
    (clicked, shown && side == clicked)
}

fn rail_button(id: impl Into<ElementId>, name: &str, active: bool) -> Stateful<Div> {
    div()
        .id(id)
        .relative()
        .size(px(32.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(8.))
        .cursor_pointer()
        .when(active, |d| d.bg(FILL_3))
        .when(!active, |d| d.hover(|s| s.bg(FILL_2)))
        .child(icon(name, 17., if active { TEXT } else { TEXT_2 }))
}

impl Desktop {
    fn column_on_side(&self) -> bool {
        self.screen != Screen::Inbox && !self.sidebar.column_hidden
    }

    fn pick_side(&mut self, side: Side, cx: &mut Context<Self>) {
        (self.side, self.sidebar.column_hidden) = rail_pick(self.side, self.column_on_side(), side);
        self.screen = Screen::Sessions;
        self.refresh_graph(cx);
        cx.notify();
    }

    fn toggle_inbox(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.screen == Screen::Inbox {
            self.screen = Screen::Sessions;
            cx.notify();
        } else {
            self.open_inbox(window, cx);
        }
    }

    /// The compact layout's rail: sidebar toggle, project switcher, the column's sides, search, inbox and settings.
    pub(crate) fn nav(&self, cx: &mut Context<Self>) -> Div {
        let expand = rail_button("nav-expand", "sidebar-expand", false)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_sidebar(&crate::actions::ToggleSidebar, window, cx)));
        let open = self.sidebar.picker.open;
        let name = self.project.as_deref().map(|p| self.repo_name(p)).unwrap_or_default();
        let button = div()
            .id("nav-project")
            .size(px(32.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(8.))
            .cursor_pointer()
            .when(open, |d| d.bg(FILL_3))
            .when(!open, |d| d.hover(|s| s.bg(FILL_2)))
            .child(ui::repo_mark(&name, true, None).size(px(24.)).text_size(px(12.)))
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                this.toggle_project_picker(window, cx);
            }));
        let project = div().relative().child(button).when(open, |d| d.child(ui::dropdown(36., ui::menu_in("projects-menu-in", self.project_picker(cx)))));
        let shown = self.column_on_side();
        let dirty = changes_badge(self.repo()).is_some();
        let sides = [(Side::Sessions, "comment"), (Side::Explorer, "file"), (Side::Changes, "branch")].into_iter().enumerate().map(|(i, (side, name))| {
            rail_button(("nav-side", i), name, shown && self.side == side)
                .when(side == Side::Changes && dirty, |d| d.child(changes_dot()))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.pick_side(side, cx)))
        });
        let search = rail_button("nav-search", "search", self.overlay == Some(Overlay::Palette))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::Palette, window, cx)));
        let unseen = crate::inbox::count(&self.agents);
        let inbox = rail_button("nav-inbox", "inbox", self.screen == Screen::Inbox)
            .when(unseen > 0, |d| d.child(ui::count_badge(unseen).top(px(-3.)).right(px(-3.))))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_inbox(window, cx)));
        let settings = rail_button("nav-settings", "settings", false)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.project_settings(&crate::actions::ProjectSettings, window, cx)));
        let bars = self.usage().into_iter().map(|(p, left, level)| {
            let fill = if level == Level::Low { provider_color(p) } else { context::glyph(level) };
            div().w(px(24.)).h(px(3.)).flex().rounded(px(2.)).bg(SEPARATOR_STRONG).child(div().w(relative(left as f32 / 100.)).rounded(px(2.)).bg(fill))
        });
        let me = if self.initials.is_empty() { "ME".to_string() } else { self.initials.clone() };
        drag_area(ui::side(div()))
            .w(px(56.))
            .flex_none()
            .h_full()
            .pt(px(37.))
            .pb(px(12.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(6.))
            .child(expand)
            .child(project)
            .children(sides)
            .child(search)
            .child(inbox)
            .child(
                div()
                    .mt_auto()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(8.))
                    .child(settings)
                    .child(div().py(px(4.)).flex().flex_col().items_center().gap(px(3.)).children(bars))
                    .child(ui::avatar(&me, 30.).text_size(px(10.5))),
            )
    }
}
```

`sidebar.rs`:
- Remove `mod panel;` and delete `sidebar/panel.rs`.
- Change the chrome import to `use crate::desktop::chrome::{Column, Layout, Overlay, RowMenu, Screen, drag_area, id, state};`
- Add the field to `SidebarState` (after `picker`) and its initializer (`column_hidden: false`):

```rust
    /// Whether the user hid Compact's column.
    pub(crate) column_hidden: bool,
```

```rust
        (Self { search, menu_at: None, picker, column_hidden: false }, subs)
```

- Add after `in_tree`:

```rust
/// The Sessions, Explorer and Changes column; the inbox lists in it too.
pub(crate) fn column_shown(layout: Layout, screen: Screen, hidden: bool) -> bool {
    match layout {
        Layout::Sidebars => true,
        Layout::Compact => screen == Screen::Inbox || !hidden,
        Layout::Focus => false,
    }
}
```

`sidebar/column.rs`: change the chrome import to `use crate::desktop::chrome::{Column, Layout, Overlay, Screen, Side, column, drag_area};` and in `column_view` replace `.child(tabs)` with:

```rust
            .when(self.layout != Layout::Compact, |d| d.child(tabs))
```

`desktop/chrome.rs`:

```rust
    pub(crate) fn toggle_rail(&mut self, _: &ToggleRail, _: &mut Window, cx: &mut Context<Self>) {
        if self.layout == Layout::Compact {
            self.sidebar.column_hidden = !self.sidebar.column_hidden;
            cx.notify();
        }
    }

    pub(crate) fn toggle_focus(&mut self, _: &ToggleFocus, _: &mut Window, cx: &mut Context<Self>) {
        self.layout = match self.layout {
            Layout::Sidebars => Layout::Compact,
            Layout::Compact => Layout::Focus,
            Layout::Focus => Layout::Sidebars,
        };
        self.save_soon(cx);
        cx.notify();
    }
```

`desktop.rs`:
- Delete the `pub(crate) panel: bool,` field and the `panel: false,` initializer.
- In `render`, replace

```rust
        let column = (self.layout == Layout::Sidebars).then(|| self.column_view(cx));
        let panel = (self.layout == Layout::Compact && self.panel).then(|| self.panel_view(cx));
```

with

```rust
        let column = crate::sidebar::column_shown(self.layout, self.screen, self.sidebar.column_hidden).then(|| self.column_view(cx));
```

and delete the `.children(panel)` line.

`browser.rs:146`: delete ` || (self.layout == Layout::Compact && self.panel)`. If `Layout` becomes unused in `browser.rs`, drop it from the `use crate::desktop::chrome::{...}` line.

`capture.rs` `reset`:

```rust
    (d.layout, d.widths, d.sidebar.column_hidden, d.panels.menu) = (Layout::Sidebars, [None; 2], false, None);
```

`desktop/jump.rs`:
- Imports: remove `use crate::sidebar::rail::live;`; change `use crate::desktop::chrome::{Layout, Side};` to `use crate::desktop::chrome::Side;`.
- Replace `walkable`:

```rust
/// The sessions ⌘n and ⌃Tab walk, in the order the Sessions column shows them.
pub(crate) fn walkable(cards: Vec<Card>, tree: Option<&str>, tree_of: impl Fn(&str) -> Option<String>, query: &str) -> Vec<Card> {
    matching(in_tree(cards, tree, tree_of), query)
}
```

- In `visible_sessions`, call it as `walkable(self.cards(project), tree.as_deref(), |cwd| self.tree_of(cwd), &query)`.

**Step 4: Run the tests and the build**

Run: `cargo test -p pocket -- rail_pick column_shown cmd_n_counts project_picker cmd_b_swaps`
Expected: all PASS.
Run: `cargo build --workspace 2>&1 | grep -E '^(warning|error)' | sort | uniq -c`
Expected: no errors, and no unused-code warnings from the files this PR touched.

### Task 1.4: Capture steps, module map and screens

**Files:**
- Modify: `packages/desktop/crates/pocket/src/capture.rs` (`STEPS`)
- Modify: `docs/adr/0003-desktop-code-layout.md:35`

**Context:** Capture steps let `capture.sh` reach a state by name. Two new ones: `sidebar` (⌘B) and `projects` (open the switcher). `STEPS` is a fixed-size array; its length goes from 26 to 28.

**Step 1: Implement**

`capture.rs`: change `use crate::actions::{ToggleFocus, ToggleRail};` to `use crate::actions::{ToggleFocus, ToggleRail, ToggleSidebar};`, change `const STEPS: [(&str, Step); 26]` to `28`, and add after the `("focus", …)` entry:

```rust
    ("sidebar", |d, window, cx| d.toggle_sidebar(&ToggleSidebar, window, cx)),
    ("projects", |d, window, cx| d.toggle_project_picker(window, cx)),
```

`docs/adr/0003-desktop-code-layout.md`: change the `sidebar` row to

```markdown
| `sidebar` | projects aside, compact rail, `sidebar/project_picker` the rail's project switcher, sessions column |
```

**Step 2: Gate**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets 2>&1 | grep -c '^warning' && cargo test --workspace`
Expected: build OK, warning count ≤ 12, all tests pass.

**Step 3: After screens** (from the repo root):

Run: `.ui-review/fixture/capture.sh /tmp/compact-after compact=sidebar compact-hidden=sidebar,rail compact-explorer=sidebar,explore compact-projects=sidebar,projects sidebars=session`
Expected: five PNG paths printed. Compare with `/tmp/compact-before`:
- `compact`: 56px rail (expand, project mark, three side icons, search, inbox, settings, bars, avatar) with the Sessions column docked beside it and no tab strip.
- `compact-hidden`: rail only, page widened.
- `compact-explorer`: Explorer icon highlighted, file tree in the column.
- `compact-projects`: the popover under the project mark: search field, current project checked, parent paths, "New project".
- `sidebars`: unchanged from before.
