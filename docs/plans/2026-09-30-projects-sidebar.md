# Projects Sidebar Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** The sidebar manages projects (reorder, collapse, remove, keep) and lists each project's worktrees by name only. The New worktree popup gets a Name field that falls back to a free name. A worktree's setup runs in its terminal.

**Architecture:** The store gains project order, collapse state and remove/keep operations. `git` lists worktrees oldest first and can remove one. `ui` gets one shared sidebar row shape: a chevron, a mark, the name, then a trail. The trail shows a status mark that swaps for `+`/`⋯` buttons on hover. The pocket view builds rows per project; `⋯` menus and a Confirm sheet run the destructive actions. The new-worktree form checks the Name against branches and folders, and the setup runs in the new terminal before the agent via `daemon::setup_op`.

**Toolset** (paths relative to the repo root `/Users/mingo/Developer/self/anywhere`):
- desktop, one crate: `cd packages/desktop && cargo test -p <crate> <test_name>` (crates: `pocket`, `store`, `git`, `daemon`, `ui`, `workspace`, `agents`)
- desktop, build UI only: `cd packages/desktop && cargo build -p ui -p storybook`
- desktop, full (every PR boundary): `cd packages/desktop && cargo test -p pocket -p workspace -p store -p agents -p ui -p daemon -p git && cargo build -p pocket -p storybook`. No new warnings allowed.
- Storybook (visual check of `ui` rows): `cd packages/desktop && cargo run -p storybook`
- Run the app: terminal 1 `cd packages/pocketd && go run ./cmd/pocketd serve`, terminal 2 `cd packages/desktop && cargo run --release -p pocket`
- The shell is fish: quote globs and separators (`echo '----'`).

**Read first:**
- `CONTEXT.md`: **Project** and **Worktree** entries. They are the spec for this plan; use these words.
- `packages/desktop/crates/pocket/src/forms.rs:132-146` and `:491-512`, the chip + `picker_menu` pattern. A button toggles on `capture_any_mouse_down` + `cx.stop_propagation()`; the menu closes on `on_mouse_down_out`. The `⋯` menus copy it.
- `packages/desktop/crates/ui/src/ui.rs:32-40` (`pop`, `dropdown`) and `:646-659` (`modal`). Menus and sheets are built from these.
- `packages/desktop/crates/pocket/src/view.rs:145-165` (`resizable`): the one existing `on_drag` use, with its empty drag preview.

**Assumptions** (settled; don't re-open):
- The working tree's uncommitted changes (staged desktop edits, the CONTEXT.md glossary) are the baseline. Don't revert them. Line numbers below refer to that state.
- The project row stands for the main worktree. Only non-main worktrees get rows, in creation order.
- Projects start expanded; collapse is per project and persisted in `desktop.json`.
- Collapsed: the project row shows the roll-up of all its worktrees. Expanded: only its main worktree's.
- Non-git folders and auto-added projects get no chevron, `+`, or worktree rows. Worktrees are only read for kept projects.
- Auto-added projects sit at the end. They have a dashed mark and a dim name and can't be dragged. `⋯` offers Keep in Pocket / Remove.
- Right-click on a row opens its `⋯` menu. The menu drops below `⋯` and may overlap the next column.
- The drag preview is empty; the accent drop line is the only feedback.
- Remove project closes its terminals, keeps its settings in `repos`, and asks first only when terminals exist. Delete worktree closes its terminals, removes the folder and keeps the branch. It asks first when terminals exist or files are uncommitted.
- "Closes N terminals" counts every pane in the project/worktree, exited ones included.
- Deleting the selected worktree falls back to the project's main worktree. Removing the selected project selects the first remaining one.
- Worktree name = folder name = branch name. There is no `task/` prefix. With Name empty, it is the prompt's slug, or a random `adjective-noun` if the prompt is empty. Auto names are made free with `-2`, `-3`…; a typed name that is taken shows an inline error and disables Create.
- The worktrees folder stays `RepoConfig.worktrees`, else `~/.worktrees/<repo name>`.
- Setup is one command line run as `setup || exit` before the agent. `a && b` chains stop the agent on failure; `a; b` only guards `b`.
- "Setting up…" ends when pocketd reports an agent in that terminal, or the terminal exits or closes. If the agent binary is missing, the spinner stays until the terminal closes.
- Agent badges leave the sidebar; the session list already shows each agent.
- The footer settings gear stays. `.ui-review/fixture` is untouched.
- CLAUDE.md: no comments unless the WHY is not readable from the code. Keep doc comments this plan gives; add no others.

---

## PR 1: Store and git groundwork

**Scope:** `Store` learns project order, collapse, add/remove. `git::worktrees` lists oldest first, and `git::remove_worktree` comes back. Inert except for `add_repo` using `Store::add`. Nothing on screen changes except worktree order in the old rows.
**Depends on:** nothing
**Done when:** the desktop full line is green; `store` and `git` have the new tests.

### Task 1.1: Store: order, collapse, add, remove

**Files:**
- Modify: `packages/desktop/crates/store/src/store.rs` (whole file, 59 lines)
- Modify: `packages/desktop/crates/pocket/src/forms.rs:681-690` (`add_repo`)
- Test: `packages/desktop/crates/store/src/store.rs` (`mod tests`)

**Context:** `Store` is `desktop.json`: `projects` is the sidebar order, and `repos` holds per-project settings keyed by path. Methods here don't save; callers call `save()`, as they do today. Removing a project must keep its `repos` entry so adding it back restores its settings.

**Step 1: Write the failing tests**

Add to `mod tests`, and add `s.collapsed.insert("/w".into());` to `round_trips_through_desktop_json` right after `s.projects.push("/w".into());`:

```rust
    #[test]
    fn a_dragged_project_takes_the_place_of_the_one_it_lands_on() {
        let mut s = Store { projects: ["a", "b", "c", "d"].map(String::from).to_vec(), ..Default::default() };
        s.move_project("a", "c");
        assert_eq!(s.projects, ["b", "c", "a", "d"]);
        s.move_project("d", "b");
        assert_eq!(s.projects, ["d", "b", "c", "a"]);
        s.move_project("x", "b");
        assert_eq!(s.projects, ["d", "b", "c", "a"]);
    }

    #[test]
    fn removing_a_project_keeps_its_settings() {
        let mut s = Store::default();
        s.add("/w");
        s.add("/w");
        s.repos.insert("/w".into(), RepoConfig { name: "w".into(), ..Default::default() });
        s.toggle("/w");
        assert_eq!((s.projects.len(), s.collapsed.contains("/w")), (1, true));
        s.remove("/w");
        assert!(s.projects.is_empty() && s.collapsed.is_empty());
        assert!(s.repos.contains_key("/w"));
        s.toggle("/w");
        s.toggle("/w");
        assert!(!s.collapsed.contains("/w"));
    }
```

These prove: a drop moves a project to the target's slot in both directions, and unknown paths are ignored. `add` is idempotent. `remove` forgets order and collapse but keeps settings. `toggle` flips.

**Step 2: Run the tests to verify they fail**

Run: `cd packages/desktop && cargo test -p store`
Expected: FAIL to compile: `no method named 'move_project' found for struct 'Store'` (plus the same for `add`, `toggle`, `remove` and `no field 'collapsed'`).

**Step 3: Write the implementation**

In `store.rs` change the import and the struct, and add the methods to `impl Store`:

```rust
use std::collections::{BTreeMap, BTreeSet};
```

```rust
pub struct Store {
    pub projects: Vec<String>,
    /// Keyed by the repository's path in `projects`.
    pub repos: BTreeMap<String, RepoConfig>,
    /// Project paths whose worktrees are hidden on the sidebar.
    pub collapsed: BTreeSet<String>,
    #[serde(skip)]
    path: PathBuf,
}
```

```rust
    pub fn add(&mut self, path: &str) {
        if !self.projects.iter().any(|p| p == path) {
            self.projects.push(path.to_string());
        }
    }

    /// Forgets `path`'s place on the sidebar but keeps its settings, so adding it back restores them.
    pub fn remove(&mut self, path: &str) {
        self.projects.retain(|p| p != path);
        self.collapsed.remove(path);
    }

    pub fn toggle(&mut self, path: &str) {
        if !self.collapsed.remove(path) {
            self.collapsed.insert(path.to_string());
        }
    }

    /// Puts project `from` where `to` is, shifting the ones between.
    pub fn move_project(&mut self, from: &str, to: &str) {
        let (Some(i), Some(j)) = (self.projects.iter().position(|p| p == from), self.projects.iter().position(|p| p == to)) else { return };
        let p = self.projects.remove(i);
        self.projects.insert(j, p);
    }
```

In `forms.rs` `add_repo`, replace

```rust
        if !self.store.projects.contains(&path) {
            self.store.projects.push(path.clone());
        }
```

with

```rust
        self.store.add(&path);
```

**Step 4: Run the tests to verify they pass**

Run: `cd packages/desktop && cargo test -p store && cargo build -p pocket`
Expected: PASS (3 tests); pocket builds with no warnings.

### Task 1.2: `git::worktrees` lists the main worktree, then oldest first

**Files:**
- Modify: `packages/desktop/crates/git/src/git.rs:140-143` (`worktrees`)
- Test: `packages/desktop/crates/git/src/git.rs` (`mod tests`, line 362)

**Context:** `git worktree list --porcelain` prints the main worktree first, then the rest sorted by path, not by age. The sidebar wants creation order (a new worktree lands at the bottom), so sort by the folder's creation time. macOS `/tmp` resolves to `/private/...` in git's output, so tests compare folder names, not full paths.

**Step 1: Write the failing test**

Add to `mod tests` in `git.rs` (a helper plus a test):

```rust
    fn scratch_repo(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pocket-git-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let run = |args: &[&str]| assert!(Command::new("git").arg("-C").arg(&repo).args(args).output().unwrap().status.success());
        run(&["init", "-q"]);
        std::fs::write(repo.join("a"), "a\n").unwrap();
        run(&["add", "."]);
        run(&["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "commit", "-qm", "init"]);
        dir
    }

    #[test]
    fn lists_worktrees_main_first_then_oldest_first() {
        let dir = scratch_repo("order");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        for name in ["zeta", "alpha", "mid"] {
            add_worktree(r, dir.join(name).to_str().unwrap(), name, "HEAD").unwrap();
        }
        let names: Vec<String> = worktrees(r).iter().map(|w| w.path.rsplit('/').next().unwrap().to_string()).collect();
        assert_eq!(names, ["repo", "zeta", "alpha", "mid"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
```

It proves that worktrees created as zeta, alpha, mid come back in that order, after main.

**Step 2: Run the test to verify it fails**

Run: `cd packages/desktop && cargo test -p git lists_worktrees_main_first_then_oldest_first`
Expected: FAIL: `left: ["repo", "alpha", "mid", "zeta"]`, `right: ["repo", "zeta", "alpha", "mid"]`.

**Step 3: Write the implementation**

Replace `worktrees` in `git.rs`:

```rust
/// The repository's checkouts: the main one, then the rest oldest first.
pub fn worktrees(cwd: &str) -> Vec<Worktree> {
    let mut out = parse_worktrees(&git(cwd, &["worktree", "list", "--porcelain"]).unwrap_or_default());
    out.sort_by_cached_key(|w| (!w.main, std::fs::metadata(&w.path).and_then(|m| m.created()).ok()));
    out
}
```

**Step 4: Run the test to verify it passes**

Run: `cd packages/desktop && cargo test -p git`
Expected: PASS (all git tests).

### Task 1.3: `git::remove_worktree`

**Files:**
- Modify: `packages/desktop/crates/git/src/git.rs` (add after `add_worktree`, line ~172)
- Test: `packages/desktop/crates/git/src/git.rs` (`mod tests`)

**Context:** Deleting a worktree removes its folder even with uncommitted or untracked files; the sidebar asks the user first (PR 3). The branch stays. `scratch_repo` is from Task 1.2.

**Step 1: Write the failing test**

```rust
    #[test]
    fn removing_a_worktree_deletes_its_folder_but_keeps_its_branch() {
        let dir = scratch_repo("remove");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        let tree = dir.join("fix");
        add_worktree(r, tree.to_str().unwrap(), "fix", "HEAD").unwrap();
        std::fs::write(tree.join("draft"), "unsaved\n").unwrap();
        remove_worktree(r, tree.to_str().unwrap()).unwrap();
        assert!(!tree.exists());
        assert!(branches(r).contains(&"fix".to_string()));
        assert_eq!(worktrees(r).len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
```

It proves that a worktree with an untracked file is removed, and that its branch survives.

**Step 2: Run the test to verify it fails**

Run: `cd packages/desktop && cargo test -p git removing_a_worktree`
Expected: FAIL to compile: `cannot find function 'remove_worktree' in this scope`.

**Step 3: Write the implementation**

After `add_worktree`:

```rust
/// Deletes the worktree's folder, uncommitted changes included; its branch stays.
pub fn remove_worktree(repo: &str, path: &str) -> Result<(), String> {
    let out = Command::new("git").arg("-C").arg(repo).args(["worktree", "remove", "--force", path]).output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}
```

**Step 4: Run the tests to verify they pass**

Run: the desktop full line from Toolset.
Expected: PASS, no new warnings.

---

## PR 2: Name-only worktree rows under collapsible projects

**Scope:** New sidebar rows. Each project row has a chevron (git projects only) and stands for the main worktree. Non-main worktrees are listed by folder name with one status mark each. A project with no worktrees shows a "New worktree" row. Collapse persists. Auto-added projects get a dashed mark. Merged tracking, the agent-badge rows and the two-line worktree rows go. No menus yet (PR 3).
**Depends on:** PR 1
**Done when:** the desktop full line is green and the manual check in Task 2.2 passes.

### Task 2.1: ui: sidebar rows, chevron, status indicator

**Files:**
- Modify: `packages/desktop/crates/ui/src/ui.rs:246-249` (delete `mini_status`), `:291-296` (`repo_mark`), `:331-355` (`repo_row`), `:741-790` (`worktree_row`)
- Modify: `packages/desktop/crates/storybook/src/main.rs:137-144` ("Project row" story), `:187-194` ("Worktrees" story)

**Context:** `ui` is the shared component crate; storybook shows each component. This task changes `repo_row`/`worktree_row` signatures, so **`pocket` won't compile until Task 2.2**. Verify this task with the UI-only build. Every sidebar row shares one shape: 32px tall, rounded 9, white with `row_shadow()` when selected, `FILL_2` on hover. Each row is a gpui `group` named `ROW_GROUP` so PR 3 can reveal buttons on hover. Worktree names start at 54px, lined up with the project's name: 4 padding + 14 chevron + 7 gap + 22 mark + 7 gap.

**Step 1: Write the implementation**

In `ui.rs`, delete `mini_status` (lines 246-249 with its doc comment).

Replace `repo_mark` (keep its position):

```rust
/// One letter for a repository, taken from its last dash-separated word: "app-ios" is I.
fn mark_letter(name: &str) -> String {
    let word = name.rsplit('-').find(|w| !w.is_empty()).unwrap_or(name);
    word.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default()
}

pub fn repo_mark(name: &str, selected: bool, state: Option<State>) -> Div {
    repo_tile(&mark_letter(name), 22., selected, state)
}
```

Replace `repo_row`:

```rust
const ROW_GROUP: &str = "sidebar-row";

fn sidebar_row(id: impl Into<ElementId>, selected: bool) -> Stateful<Div> {
    div()
        .id(id)
        .group(ROW_GROUP)
        .h(px(32.))
        .pr(px(6.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(7.))
        .rounded(px(9.))
        .cursor_pointer()
        .when(selected, |d| d.bg(rgba(0xffffffe6)).shadow(row_shadow()))
        .when(!selected, |d| d.hover(|s| s.bg(rgba(FILL_2))))
}

/// `kept` is false for a project Pocket shows only while it has terminals: its mark is dashed and its name dim.
pub fn repo_row(id: impl Into<ElementId>, lead: impl IntoElement, name: &str, selected: bool, kept: bool) -> Stateful<Div> {
    let mark = if kept {
        repo_mark(name, false, None)
    } else {
        div()
            .size(px(22.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .border(px(1.))
            .border_dashed()
            .border_color(rgba(TEXT_5))
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgba(TEXT_3))
            .child(mark_letter(name))
    };
    sidebar_row(id, selected)
        .pl(px(4.))
        .text_size(px(14.5))
        .font_weight(FontWeight::SEMIBOLD)
        .child(lead)
        .child(mark)
        .child(div().flex_1().min_w_0().truncate().when(!kept, |d| d.text_color(rgba(TEXT_3))).child(name.to_string()))
}

/// Opens or closes a project's worktrees.
pub fn chevron(id: impl Into<ElementId>, open: bool) -> Stateful<Div> {
    div()
        .id(id)
        .w(px(14.))
        .h(px(20.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .hover(|s| s.bg(rgba(FILL_3)))
        .child(icon(if open { "chevron-down" } else { "chevron-right" }, 12., TEXT_4))
}

/// A row's status mark: a spinner while working, a dot when it asks for a look.
pub fn indicator(id: impl Into<ElementId>, state: Option<State>) -> Option<AnyElement> {
    match state? {
        State::Working => Some(spinner(id, 11., RUNNING_TEXT).into_any_element()),
        s => alert_color(s).map(|c| dot(7., c).into_any_element()),
    }
}
```

Replace `worktree_row`:

```rust
/// Indented so its name lines up with its project's: 4 + chevron 14 + 7 + mark 22 + 7.
pub fn worktree_row(id: impl Into<ElementId>, name: String, selected: bool) -> Stateful<Div> {
    sidebar_row(id, selected)
        .pl(px(54.))
        .text_size(px(13.5))
        .font_weight(if selected { FontWeight::SEMIBOLD } else { FontWeight(450.) })
        .text_color(rgba(if selected { TEXT } else { TEXT_BODY }))
        .child(div().flex_1().min_w_0().truncate().child(name))
}
```

In `storybook/src/main.rs`, replace the "Project row" story's `list()` with:

```rust
                list()
                    .child(ui::repo_row("repo-android", ui::chevron("chev-android", true), "app-android", true, true))
                    .child(ui::repo_row("repo-ios", ui::chevron("chev-ios", false), "app-ios", false, true).children(ui::indicator("spin-ios", Some(State::NeedsYou))))
                    .child(ui::repo_row("repo-scratch", div().w(px(14.)).flex_none(), "scratch", false, false).children(ui::indicator("spin-scratch", Some(State::Working)))),
```

and the "Worktrees" story's `list()` with:

```rust
                list()
                    .child(ui::worktree_row("w1", "fix-login".into(), true))
                    .child(ui::worktree_row("w2", "brave-otter".into(), false).children(ui::indicator("w2-spin", Some(State::NeedsYou))))
                    .child(ui::worktree_row("w3", "add-dark-mode".into(), false).children(ui::indicator("w3-spin", Some(State::Working)))),
```

**Step 2: Build**

Run: `cd packages/desktop && cargo build -p ui -p storybook`
Expected: builds with no warnings. (`cargo build -p pocket` fails until Task 2.2. That is expected.)

**Step 3: Look at it**

Run: `cd packages/desktop && cargo run -p storybook`
Expected:
- Project row: app-android is white with a chevron-down; app-ios has an orange dot at the right end; scratch has a dashed mark, a grey name and a spinner.
- Worktrees: names start under the project names; fix-login is white and bold.

### Task 2.2: Sidebar: project rows with their worktrees

**Files:**
- Modify: `packages/desktop/crates/pocket/src/main.rs:473-485` (add `select_tree` after `select_project`)
- Modify: `packages/desktop/crates/pocket/src/view.rs:176-331` (`aside` doc and loop, `tree_cards`, replace `worktree_rows`)

**Context:**
- `projects()` is the kept projects (`store.projects`) followed by auto-added folders that have terminals.
- `worktrees[p]` is filled only for kept projects; the main worktree comes first, then the rest oldest first (PR 1).
- `cwd()` is the worktree on screen: `self.worktree`, else the project's main (`tree_of(project)`).
- `tree_of(cwd)` maps any folder to its worktree path.
- `cards(p)` is every session of project `p`.
- `status::roll_up` picks the most urgent status; `state()` turns it into a `ui::State`.
- `new_worktree` opens the New worktree popup for `self.project`, so select the project first.

**Step 1: Write the implementation**

In `main.rs`, after `select_project`:

```rust
    /// Shows worktree `tree` of project `p`; `None` is the project's main worktree.
    pub fn select_tree(&mut self, p: String, tree: Option<String>, cx: &mut Context<Self>) {
        self.select_project(p, cx);
        self.worktree = tree;
        self.session = None;
        cx.notify();
    }
```

In `view.rs`, add a free function next to `state`:

```rust
fn roll(cards: &[Card]) -> Option<State> {
    status::roll_up(cards.iter().map(|c| c.status)).map(|(s, _)| state(s, 0, 0))
}
```

Change `aside`'s doc comment to `/// The expanded sidebar: projects with their worktrees.` and replace its `let mut repos = Vec::new(); for … { … }` block (lines 206-219) with:

```rust
        let mut repos = Vec::new();
        for (i, p) in self.projects().into_iter().enumerate() {
            repos.extend(self.project_rows(i, &p, cx));
        }
```

Replace `tree_cards` (with its doc comment) and `worktree_rows` (lines 258-331) with:

```rust
    fn tree_cards(&self, project: &str, tree: &str) -> Vec<Card> {
        self.cards(project).into_iter().filter(|c| self.tree_of(&c.cwd).as_deref() == Some(tree)).collect()
    }

    /// A project's row, then its worktrees' rows while it is open.
    fn project_rows(&self, i: usize, p: &str, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let current = self.cwd().filter(|_| self.screen == Screen::Sessions && self.project.as_deref() == Some(p));
        let kept = self.store.projects.iter().any(|k| k == p);
        let main = self.tree_of(p).unwrap_or_else(|| p.to_string());
        let git = self.worktrees.get(p).is_some_and(|w| !w.is_empty());
        let open = git && !self.store.collapsed.contains(p);
        let cards = if open { self.tree_cards(p, &main) } else { self.cards(p) };
        let lead = if git {
            let target = p.to_string();
            ui::chevron(("aside-chevron", i), open)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.store.toggle(&target);
                    this.store.save();
                    cx.notify();
                }))
                .into_any_element()
        } else {
            div().w(px(14.)).flex_none().into_any_element()
        };
        let target = p.to_string();
        let mut out = vec![
            ui::repo_row(("aside-repo", i), lead, &self.repo_name(p), current.as_ref() == Some(&main), kept)
                .children(ui::indicator(("aside-spin", i), roll(&cards)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_tree(target.clone(), None, cx)))
                .into_any_element(),
        ];
        if !open {
            return out;
        }
        let trees: Vec<String> = self.worktrees[p].iter().filter(|w| !w.main).map(|w| w.path.clone()).collect();
        for tree in &trees {
            let (target, path) = (p.to_string(), tree.clone());
            out.push(
                ui::worktree_row(id(format!("aside-tree:{tree}")), basename(tree), current.as_ref() == Some(tree))
                    .children(ui::indicator(id(format!("aside-tree-spin:{tree}")), roll(&self.tree_cards(p, tree))))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_tree(target.clone(), Some(path.clone()), cx)))
                    .into_any_element(),
            );
        }
        if trees.is_empty() {
            out.push(self.new_worktree_row(p, cx));
        }
        out
    }

    fn new_worktree_row(&self, p: &str, cx: &mut Context<Self>) -> AnyElement {
        let target = p.to_string();
        div()
            .id(id(format!("aside-new-worktree:{p}")))
            .h(px(32.))
            .pl(px(25.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(7.))
            .rounded(px(9.))
            .cursor_pointer()
            .text_size(px(13.))
            .text_color(rgba(TEXT_4))
            .hover(|s| s.bg(rgba(FILL_2)))
            .child(div().w(px(22.)).flex().flex_none().justify_center().child(icon("plus", 12., TEXT_4)))
            .child("New worktree")
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.select_project(target.clone(), cx);
                this.new_worktree(&crate::NewWorktree, window, cx);
            }))
            .into_any_element()
    }
```

**Step 2: Build and test**

Run: `cd packages/desktop && cargo build -p pocket && cargo test -p pocket`
Expected: builds with no warnings; tests PASS.

**Step 3: Manual check** (run the app per Toolset)
- Every git project shows a chevron and is open. Its worktrees show by folder name in creation order, with no "main" row.
- Clicking the project row selects main (row turns white). Clicking a worktree selects it.
- Clicking the chevron collapses the project without selecting it. Restart the app: it stays collapsed.
- A collapsed project's dot/spinner reflects all its worktrees' sessions.
- A git project with no worktrees shows "+ New worktree", which opens the New worktree popup for that project.
- A terminal opened outside every project shows its folder at the end with a dashed mark.

### Task 2.3: Drop Merged tracking

**Files:**
- Modify: `packages/desktop/crates/git/src/git.rs:150-153` (delete `merged`)
- Modify: `packages/desktop/crates/pocket/src/main.rs:140` (field), `:248` (init), `:592-647` (`refresh_git`)

**Context:** Worktree rows no longer show Merged, so nothing reads `Desktop.merged`. Keep `ui::State::Merged`, which the status pill still offers.

**Step 1: Write the implementation**
- `git.rs`: delete `merged` and its doc comment.
- `main.rs`: delete the `merged: HashSet<String>,` field and the `merged: HashSet::new(),` initializer.
- `main.rs` `refresh_git`:
  - Delete `let current = self.project.clone();`.
  - Delete the `let merged: HashSet<String> = current.map(…).unwrap_or_default();` statement.
  - Change the task's result to `(repos, diff, initials, tree, worktrees, file)`, and the await line to `let (repos, diff, initials, tree, worktrees, file) = task.await;`.
  - Change the apply lines to:

```rust
                let mut changed = repos != d.repos || tree != d.tree || initials != d.initials || worktrees != d.worktrees;
                (d.repos, d.tree, d.initials, d.worktrees) = (repos, tree, initials, worktrees);
```

**Step 2: Run the full suite**

Run: the desktop full line from Toolset.
Expected: PASS, no warnings.

---

## PR 3: Manage rows: `⋯` menus, hover `+`, confirm, drag to reorder

**Scope:**
- Hovering a row swaps its status mark for its buttons: `+` (new worktree, git projects) and `⋯`. Right-click opens `⋯` too.
- Project menu: Settings… / Remove from Pocket, or Keep in Pocket / Remove for auto-added ones.
- Worktree menu: Delete worktree….
- A Confirm sheet lists consequences before destroying anything.
- Kept projects reorder by drag.

**Depends on:** PR 2
**Done when:** the desktop full line is green and the manual checks in Tasks 3.3 and 3.4 pass.

### Task 3.1: ui: row trail, danger menu rows, drop line, Danger button, trash icon

**Files:**
- Create: `packages/desktop/crates/theme/assets/icons/trash.svg`
- Modify: `packages/desktop/crates/theme/src/theme.rs:149` (`embed!` list)
- Modify: `packages/desktop/crates/ui/src/ui.rs:50-96` (`Variant`, `button`), `:681-697` (`menu_row`), plus new functions after `indicator`
- Modify: `packages/desktop/crates/storybook/src/main.rs` (Button, Worktrees, Palette and menus stories)

**Context:**
- Icons are embedded from an explicit list. An SVG not in `embed!` renders blank.
- `group_hover(ROW_GROUP, …)` restyles a child while its row (`sidebar_row`, Task 2.1) is hovered. Opacity-0 buttons stay clickable, but clicking needs hovering, which shows them.
- The drop line is an inset accent shadow: 2px on the row's bottom edge when the dragged row moves down, on its top edge when it moves up.
- Menus put the destructive item last, behind a divider.

**Step 1: Write the implementation**

`trash.svg`:

```svg
<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M4 7h16M10 11v6M14 11v6M5 7l1 12a2 2 0 002 2h8a2 2 0 002-2l1-12M9 7V4a1 1 0 011-1h4a1 1 0 011 1v3"/></svg>
```

`theme.rs` line 149: insert `"trash",` between `"terminal",` and `"unfold",`.

`ui.rs`, add `Danger` to `Variant` (after `Accent`). Change the `fg` arm to `Variant::Primary | Variant::Accent | Variant::Danger => WHITE,`. Add this arm to `button`'s match:

```rust
        Variant::Danger => d.bg(rgba(FAILED)).font_weight(FontWeight::SEMIBOLD),
```

Replace `menu_row` with:

```rust
fn menu_item(id: impl Into<ElementId>, icon_name: &str, label: &str, tint: u32, hover: u32) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(32.))
        .px(px(10.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(10.))
        .rounded(px(8.))
        .cursor_pointer()
        .text_size(px(13.5))
        .hover(move |s| s.bg(rgba(hover)))
        .child(icon(icon_name, 14., tint))
        .child(div().flex_1().child(label.to_string()))
}

pub fn menu_row(id: impl Into<ElementId>, icon_name: &str, label: &str, keys: Option<&str>) -> Stateful<Div> {
    menu_item(id, icon_name, label, TEXT_2, FILL_2).children(keys.map(kbd))
}

/// A menu row that destroys something; menus keep it last, behind a divider.
pub fn danger_row(id: impl Into<ElementId>, icon_name: &str, label: &str) -> Stateful<Div> {
    menu_item(id, icon_name, label, FAILED, FAILED_BG).text_color(rgba(FAILED))
}

pub fn menu_divider() -> Div {
    div().h(px(0.5)).mx(px(8.)).my(px(4.)).flex_none().bg(rgba(SEPARATOR))
}
```

After `indicator`:

```rust
/// A sidebar row's right end: its status mark, swapped for its buttons while the row is hovered or its menu is open.
pub fn row_trail(mark: Option<AnyElement>, buttons: Vec<AnyElement>, open: bool) -> Div {
    let width = buttons.len() as f32 * 24. - 2.;
    div()
        .relative()
        .h_full()
        .min_w(px(width))
        .flex()
        .flex_none()
        .items_center()
        .justify_end()
        .children(mark.map(|m| div().flex().items_center().map(|d| if open { d.opacity(0.) } else { d.group_hover(ROW_GROUP, |s| s.opacity(0.)) }).child(m)))
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .right_0()
                .flex()
                .items_center()
                .gap(px(2.))
                .when(!open, |d| d.opacity(0.).group_hover(ROW_GROUP, |s| s.opacity(1.)))
                .children(buttons),
        )
}

/// The accent line a dragged row lands on: under the row when moving down, over it when moving up.
pub fn drop_line(below: bool) -> Vec<BoxShadow> {
    vec![BoxShadow { inset: true, ..shadow(ACCENT, if below { -2. } else { 2. }, 0.) }]
}
```

Storybook:
- Button story: add `.child(ui::button("delete", Variant::Danger, None, "Delete"))` after the Ghost button.
- Worktrees story: replace w2's `.children(ui::indicator("w2-spin", Some(State::NeedsYou)))` with `.child(ui::row_trail(ui::indicator("w2-spin", Some(State::NeedsYou)), vec![ui::icon_button_sized("w2-more", "more", 22., TEXT_3).rounded(px(6.)).into_any_element()], false))`.
- Palette and menus story: append `.child(ui::menu_divider()).child(ui::danger_row("m3", "trash", "Delete worktree…"))`.

**Step 2: Build**

Run: `cd packages/desktop && cargo build -p ui -p storybook -p pocket`
Expected: builds with no warnings.

**Step 3: Look at it**

Run: `cd packages/desktop && cargo run -p storybook`
Expected:
- A red "Delete" button.
- Hovering brave-otter hides its orange dot and shows `⋯`.
- The menus story ends with a hairline and a red "Delete worktree…" row with a trash icon; its hover is pale red.

### Task 3.2: Confirm sheet; remove project, keep project, delete worktree

**Files:**
- Modify: `packages/desktop/crates/pocket/src/main.rs:61-67` (`Overlay`), fields (after `worktree`, line ~141), init (after `worktree: None,`), methods after `close_tab` (line ~566)
- Modify: `packages/desktop/crates/pocket/src/overlay.rs:1-8` (imports), `:259-266` (`overlay_view`), plus `confirm_view` and `confirmed`
- Modify: `packages/desktop/crates/pocket/src/forms.rs:152` (`footer` becomes `pub`)
- Modify: `packages/desktop/crates/pocket/src/view.rs:128` (`open` match)

**Context:**
- `close_pane(id)` kills a terminal and drops it everywhere.
- `project_of`/`tree_of` place a terminal by its launch folder.
- `git::read(dir).files` lists changed and untracked files.
- `ui::modal` is the sheet shape; `forms::footer` is the Cancel/Submit row used by Project settings.
- The Escape handler already closes any overlay except the palette.
- The three `ask`/`keep` methods get callers in Task 3.3; until then the build warns that they are never used. That is expected.

**Step 1: Write the implementation**

`main.rs`, add `Confirm` to `Overlay`:

```rust
pub enum Overlay {
    Palette,
    NewSession,
    AddRepo,
    More,
    Confirm,
}
```

and after `Overlay`:

```rust
/// What the confirm sheet asks before doing.
#[derive(Clone)]
pub enum Confirm {
    RemoveProject(String),
    DeleteWorktree { project: String, tree: String, branch: String, dirty: usize },
}
```

Field after `worktree: Option<String>,`: `confirm: Option<Confirm>,`. Init after `worktree: None,`: `confirm: None,`.

Methods after `close_tab`:

```rust
    fn project_terminals(&self, p: &str) -> Vec<String> {
        let projects = self.projects();
        self.sessions.items.iter().filter(|s| self.project_of(&s.info.cwd, &projects).is_some_and(|o| o == p)).map(|s| s.info.id.clone()).collect()
    }

    fn tree_terminals(&self, tree: &str) -> Vec<String> {
        self.sessions.items.iter().filter(|s| self.tree_of(&s.info.cwd).as_deref() == Some(tree)).map(|s| s.info.id.clone()).collect()
    }

    fn keep_project(&mut self, p: &str, cx: &mut Context<Self>) {
        self.store.add(p);
        self.store.save();
        self.refresh_git(cx);
        cx.notify();
    }

    /// Closes the project's terminals and takes it off the sidebar; its folder is untouched.
    fn remove_project(&mut self, p: &str, cx: &mut Context<Self>) {
        for id in self.project_terminals(p) {
            self.close_pane(&id, cx);
        }
        self.store.remove(p);
        self.store.save();
        self.worktrees.remove(p);
        if self.project.as_deref() == Some(p) {
            (self.project, self.worktree, self.session) = (None, None, None);
            if let Some(next) = self.projects().into_iter().next() {
                self.select_tree(next, None, cx);
            }
        }
        cx.notify();
    }

    fn ask_remove_project(&mut self, p: String, cx: &mut Context<Self>) {
        if self.project_terminals(&p).is_empty() {
            self.remove_project(&p, cx);
        } else {
            self.confirm = Some(Confirm::RemoveProject(p));
            self.overlay = Some(Overlay::Confirm);
        }
        cx.notify();
    }

    /// Asks first when deleting would close terminals or lose uncommitted changes.
    fn ask_delete_worktree(&mut self, project: String, tree: String, cx: &mut Context<Self>) {
        let branch = self.worktrees.get(&project).into_iter().flatten().find(|w| w.path == tree).map(|w| w.branch.clone()).unwrap_or_default();
        let dir = tree.clone();
        let task = cx.background_executor().spawn(async move { git::read(&dir).map_or(0, |r| r.files.len()) });
        cx.spawn(async move |this, cx| {
            let dirty = task.await;
            this.update(cx, |d, cx| {
                if dirty == 0 && d.tree_terminals(&tree).is_empty() {
                    d.delete_worktree(project, tree, cx);
                } else {
                    d.confirm = Some(Confirm::DeleteWorktree { project, tree, branch, dirty });
                    d.overlay = Some(Overlay::Confirm);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Closes the worktree's terminals and deletes its folder; its branch stays.
    fn delete_worktree(&mut self, project: String, tree: String, cx: &mut Context<Self>) {
        for id in self.tree_terminals(&tree) {
            self.close_pane(&id, cx);
        }
        self.workspaces.remove(&tree);
        let dir = tree.clone();
        let task = cx.background_executor().spawn(async move { git::remove_worktree(&project, &dir) });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |d, cx| {
                match res {
                    Err(e) => d.error = Some(e),
                    Ok(()) if d.worktree.as_ref() == Some(&tree) => (d.worktree, d.session) = (None, None),
                    Ok(()) => {}
                }
                d.refresh_git(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
```

`forms.rs` line 152: `fn footer(` becomes `pub fn footer(`.

`view.rs` `open`: `Overlay::More => {}` becomes `Overlay::More | Overlay::Confirm => {}`.

`overlay.rs` imports become:

```rust
use crate::view::{basename, tilde};
use crate::{Card, Confirm, Desktop, Overlay, Screen, Side, Status};
use gpui_kit::component::input::Input;
use gpui_kit::*;
use std::cmp::Reverse;
use std::path::Path;
use theme::*;
use ui::{self, Variant, dot, menu_row};
```

Add a free function after `hint`:

```rust
fn closes(n: usize) -> Option<String> {
    match n {
        0 => None,
        1 => Some("Closes 1 terminal".into()),
        n => Some(format!("Closes {n} terminals")),
    }
}
```

In `overlay_view` add the arm `Overlay::Confirm => (self.confirm_view(cx), 0x2e),`. Add to `impl Desktop`, before `overlay_view`:

```rust
    fn confirm_view(&mut self, cx: &mut Context<Self>) -> Div {
        let (title, action, facts, dirty): (String, &str, Vec<String>, usize) = match &self.confirm {
            Some(Confirm::RemoveProject(p)) => (
                format!("Remove {}?", self.repo_name(p)),
                "Remove",
                closes(self.project_terminals(p).len()).into_iter().chain(["The repository stays on disk".to_string()]).collect(),
                0,
            ),
            Some(Confirm::DeleteWorktree { tree, branch, dirty, .. }) => (
                format!("Delete {}?", basename(tree)),
                "Delete",
                closes(self.tree_terminals(tree).len()).into_iter().chain([format!("Deletes the folder {}", tilde(tree)), format!("Keeps the branch {branch}")]).collect(),
                *dirty,
            ),
            None => return div(),
        };
        let bullet = |text: String| div().flex().gap(px(8.)).text_size(px(13.5)).text_color(rgba(TEXT_2)).child("•").child(text);
        let mut body = vec![div().flex().flex_col().gap(px(6.)).children(facts.into_iter().map(bullet)).into_any_element()];
        if dirty > 0 {
            let files = if dirty == 1 { "file" } else { "files" };
            body.push(
                div().px(px(12.)).py(px(10.)).rounded(px(10.)).bg(rgba(FAILED_BG)).text_size(px(13.)).text_color(rgba(FAILED)).child(format!("{dirty} uncommitted {files} will be lost.")).into_any_element(),
            );
        }
        let cancel = ui::large(ui::button("confirm-cancel", Variant::Ghost, None, "Cancel").text_color(rgba(TEXT)))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let submit = ui::large(ui::button("confirm-go", Variant::Danger, None, action)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.confirmed(window, cx)));
        body.push(crate::forms::footer("", cancel, submit).into_any_element());
        let close = ui::icon_button("confirm-close", "x").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        ui::modal(&title, 440., 160., close, body)
    }

    fn confirmed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.confirm.take() {
            Some(Confirm::RemoveProject(p)) => self.remove_project(&p, cx),
            Some(Confirm::DeleteWorktree { project, tree, .. }) => self.delete_worktree(project, tree, cx),
            None => {}
        }
        self.close_overlay(window, cx);
    }
```

**Step 2: Build and test**

Run: `cd packages/desktop && cargo build -p pocket && cargo test -p pocket`
Expected: builds; the only warnings are `keep_project`, `ask_remove_project` and `ask_delete_worktree` never used (fixed in Task 3.3); tests PASS.

### Task 3.3: Row menus, hover `+`, right-click

**Files:**
- Modify: `packages/desktop/crates/pocket/src/main.rs` (add `RowMenu` after `Confirm`, field `row_menu` after `confirm`, init `row_menu: None,`)
- Modify: `packages/desktop/crates/pocket/src/view.rs:7` (imports), `project_rows` (from Task 2.2), new `row_menu_button`/`row_menu_items`, and the Escape handler (~line 1019)

**Context:** Copy the forms.rs chip pattern.
- The `⋯` button toggles `row_menu` on `capture_any_mouse_down` with `cx.stop_propagation()`. That stops the row's click, the drag, and the open menu's `on_mouse_down_out`.
- The menu is `ui::dropdown` inside a `relative()` wrapper and closes on `on_mouse_down_out`.
- Nested buttons (`+`, chevron) use `on_click` + `cx.stop_propagation()` so the row isn't selected.
- Only one menu is open at a time (`Option<RowMenu>`).
- Escape closes it.

**Step 1: Write the implementation**

`main.rs`:

```rust
/// The sidebar row whose `⋯` menu is open.
#[derive(Clone, PartialEq)]
pub enum RowMenu {
    Project(String),
    Tree { project: String, tree: String },
}
```

Field `row_menu: Option<RowMenu>,` and init `row_menu: None,`.

`view.rs` line 7: `use crate::{Card, Column, Desktop, Layout, Overlay, RowMenu, Screen, Side, Status};`

Replace `project_rows` with:

```rust
    /// A project's row, then its worktrees' rows while it is open.
    fn project_rows(&self, i: usize, p: &str, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let current = self.cwd().filter(|_| self.screen == Screen::Sessions && self.project.as_deref() == Some(p));
        let kept = self.store.projects.iter().any(|k| k == p);
        let main = self.tree_of(p).unwrap_or_else(|| p.to_string());
        let git = self.worktrees.get(p).is_some_and(|w| !w.is_empty());
        let open = git && !self.store.collapsed.contains(p);
        let cards = if open { self.tree_cards(p, &main) } else { self.cards(p) };
        let lead = if git {
            let target = p.to_string();
            ui::chevron(("aside-chevron", i), open)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.store.toggle(&target);
                    this.store.save();
                    cx.notify();
                }))
                .into_any_element()
        } else {
            div().w(px(14.)).flex_none().into_any_element()
        };
        let menu = RowMenu::Project(p.to_string());
        let mut buttons = Vec::new();
        if git {
            let target = p.to_string();
            buttons.push(
                icon_button_sized(id(format!("aside-plus:{p}")), "plus", 22., TEXT_3)
                    .rounded(px(6.))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.select_project(target.clone(), cx);
                        this.new_worktree(&crate::NewWorktree, window, cx);
                    }))
                    .into_any_element(),
            );
        }
        buttons.push(self.row_menu_button(p, menu.clone(), cx));
        let trail = ui::row_trail(ui::indicator(("aside-spin", i), roll(&cards)), buttons, self.row_menu.as_ref() == Some(&menu));
        let target = p.to_string();
        let row = ui::repo_row(("aside-repo", i), lead, &self.repo_name(p), current.as_ref() == Some(&main), kept)
            .child(trail)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_tree(target.clone(), None, cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                    this.row_menu = Some(menu.clone());
                    cx.notify();
                }),
            );
        let mut out = vec![row.into_any_element()];
        if !open {
            return out;
        }
        let trees: Vec<String> = self.worktrees[p].iter().filter(|w| !w.main).map(|w| w.path.clone()).collect();
        for tree in &trees {
            let menu = RowMenu::Tree { project: p.to_string(), tree: tree.clone() };
            let mark = ui::indicator(id(format!("aside-tree-spin:{tree}")), roll(&self.tree_cards(p, tree)));
            let trail = ui::row_trail(mark, vec![self.row_menu_button(tree, menu.clone(), cx)], self.row_menu.as_ref() == Some(&menu));
            let (target, path) = (p.to_string(), tree.clone());
            out.push(
                ui::worktree_row(id(format!("aside-tree:{tree}")), basename(tree), current.as_ref() == Some(tree))
                    .child(trail)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_tree(target.clone(), Some(path.clone()), cx)))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                            this.row_menu = Some(menu.clone());
                            cx.notify();
                        }),
                    )
                    .into_any_element(),
            );
        }
        if trees.is_empty() {
            out.push(self.new_worktree_row(p, cx));
        }
        out
    }

    fn row_menu_button(&self, key: &str, menu: RowMenu, cx: &mut Context<Self>) -> AnyElement {
        let open = self.row_menu.as_ref() == Some(&menu);
        let toggle = menu.clone();
        let button = icon_button_sized(id(format!("aside-more:{key}")), "more", 22., TEXT_3).rounded(px(6.)).capture_any_mouse_down(cx.listener(
            move |this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.row_menu = (this.row_menu.as_ref() != Some(&toggle)).then(|| toggle.clone());
                cx.notify();
            },
        ));
        div()
            .relative()
            .child(button)
            .when(open, |d| {
                d.child(ui::dropdown(
                    26.,
                    ui::pop(div().id("aside-menu").w(px(210.)).p(px(6.)).rounded(px(14.)).flex().flex_col())
                        .occlude()
                        .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                            this.row_menu = None;
                            cx.notify();
                        }))
                        .children(self.row_menu_items(&menu, cx)),
                ))
            })
            .into_any_element()
    }

    fn row_menu_items(&self, menu: &RowMenu, cx: &mut Context<Self>) -> Vec<AnyElement> {
        match menu.clone() {
            RowMenu::Project(p) if self.store.projects.contains(&p) => {
                let target = p.clone();
                vec![
                    ui::menu_row("aside-menu-settings", "settings", "Settings…", None)
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.row_menu = None;
                            this.select_project(target.clone(), cx);
                            this.project_settings(&crate::ProjectSettings, window, cx);
                        }))
                        .into_any_element(),
                    ui::menu_divider().into_any_element(),
                    ui::danger_row("aside-menu-remove", "x", "Remove from Pocket")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.row_menu = None;
                            this.ask_remove_project(p.clone(), cx);
                        }))
                        .into_any_element(),
                ]
            }
            RowMenu::Project(p) => {
                let target = p.clone();
                vec![
                    ui::menu_row("aside-menu-keep", "check", "Keep in Pocket", None)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.row_menu = None;
                            this.keep_project(&target, cx);
                        }))
                        .into_any_element(),
                    ui::menu_divider().into_any_element(),
                    ui::danger_row("aside-menu-remove", "x", "Remove")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.row_menu = None;
                            this.ask_remove_project(p.clone(), cx);
                        }))
                        .into_any_element(),
                ]
            }
            RowMenu::Tree { project, tree } => vec![
                ui::danger_row("aside-menu-delete", "trash", "Delete worktree…")
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.row_menu = None;
                        this.ask_delete_worktree(project.clone(), tree.clone(), cx);
                    }))
                    .into_any_element(),
            ],
        }
    }
```

In the Escape handler (`view.rs` ~1019) change the condition to:

```rust
                if this.close_picker() || std::mem::take(&mut this.tab_menu) || this.row_menu.take().is_some() {
```

**Step 2: Build and test**

Run: `cd packages/desktop && cargo build -p pocket && cargo test -p pocket`
Expected: builds with no warnings; tests PASS.

**Step 3: Manual check** (run the app)
- Hovering a git project row hides its mark and shows `+` and `⋯`; a worktree row shows `⋯`.
- `+` opens New worktree for that project.
- `⋯` or right-click opens the menu. Clicking outside, pressing Escape, or clicking `⋯` again closes it.
- Settings… opens that project's settings.
- Remove from Pocket on a project with no terminals removes it at once. With terminals, a sheet lists "Closes N terminals" and "The repository stays on disk". Adding the project back restores its settings.
- Delete worktree… on a clean worktree without terminals deletes it at once. The folder is gone and `git branch` still lists the branch.
- A dirty worktree asks first, with the red "N uncommitted files will be lost." box.
- Deleting the selected worktree lands on the project's main.
- An auto-added folder's menu shows Keep in Pocket (it becomes a normal project) and Remove (asks, closes its terminals, disappears).

### Task 3.4: Drag to reorder projects

**Files:**
- Modify: `packages/desktop/crates/pocket/src/view.rs` (add `DragProject` near `roll`; the `let row = …` statement in `project_rows`)

**Context:** gpui drags carry a value. `on_drag` starts one on mouse-move after a press, `drag_over::<T>` restyles a hovered target, and `on_drop::<T>` receives the value. Only kept projects drag or accept drops. `i` is the row's position in `projects()`, where kept projects come first, so `d.ix < i` means moving down.

**Step 1: Write the implementation**

Near `roll`:

```rust
#[derive(Clone)]
struct DragProject {
    path: String,
    ix: usize,
}
```

In `project_rows`, add these calls to the `let row = …` chain after `.on_mouse_down(…)`:

```rust
            .when(kept, |row| {
                let (from, to) = (DragProject { path: p.to_string(), ix: i }, p.to_string());
                row.on_drag(from, |_, _, _, cx| cx.new(|_| EmptyView))
                    .drag_over::<DragProject>(move |s, d, _, _| if d.ix == i { s } else { s.shadow(ui::drop_line(d.ix < i)) })
                    .on_drop(cx.listener(move |this, d: &DragProject, _, cx| {
                        this.store.move_project(&d.path, &to);
                        this.store.save();
                        cx.notify();
                    }))
            });
```

**Step 2: Run the full suite**

Run: the desktop full line from Toolset.
Expected: PASS, no warnings.

**Step 3: Manual check**
- Dragging a project over another shows a blue line under it (moving down) or over it (moving up). Dropping moves the project there.
- The order survives a restart.
- Auto-added rows neither drag nor accept drops.
- A plain click still selects.

---

## PR 4: New worktree: Name field, free names, setup in the terminal

**Scope:**
- The New worktree popup gets a Name field (folder = branch = name).
- Empty Name falls back to the prompt's slug or a random `adjective-noun`, made free.
- A typed name that is taken or invalid shows an inline error and disables Create.
- The setup command runs in the new worktree's terminal before the agent. The row shows "Setting up…" until the agent appears.

**Depends on:** PR 3
**Done when:** the desktop full line is green and the manual checks in Tasks 4.3 and 4.4 pass.

### Task 4.1: daemon: `setup_op`

**Files:**
- Modify: `packages/desktop/crates/daemon/src/daemon.rs:102-114` (`agent_op`, `agent_args`), add `setup_op`
- Test: `packages/desktop/crates/daemon/src/daemon.rs:207-224` (existing tests) plus a new test

**Context:** `agent_args` builds `login-shell -l -c '<script>' argv…`; the script runs the agent, then `exec`s a fresh login shell. A setup prefixes the script with `setup || exit; `. If it fails, the shell exits with its status, so the terminal stays open showing the error and the agent never starts. The existing real-shell test swaps `exec` for `echo` to see the hand-back line.

**Step 1: Write the failing test**

Update both existing tests to pass an empty setup: `agent_args("/opt/homebrew/bin/fish", "", &argv)`, `agent_args("/bin/zsh", "", &argv)`, `agent_args(shell, "", &argv)`. Add:

```rust
    #[test]
    fn setup_runs_first_and_a_failed_one_stops_the_agent() {
        for shell in ["/bin/zsh", "/bin/bash", "/bin/sh", "/opt/homebrew/bin/fish"].into_iter().filter(|s| Path::new(s).exists()) {
            let argv = ["printf", "[%s]", "agent"].map(String::from);
            let run = |setup: &str| {
                let mut args = agent_args(shell, setup, &argv);
                args[2] = args[2].replace("exec", "echo");
                let home = std::env::temp_dir().join("pocket-desktop-no-home");
                std::process::Command::new(shell).args(&args).env_clear().env("HOME", &home).env("PATH", "/usr/bin:/bin").output().unwrap()
            };
            let ok = run("printf ready");
            assert_eq!(String::from_utf8_lossy(&ok.stdout), format!("ready[agent]{shell} -l\n"), "{shell}");
            let failed = run("false");
            assert!(!failed.status.success(), "{shell}");
            assert_eq!(String::from_utf8_lossy(&failed.stdout), "", "{shell}");
        }
    }
```

It proves, in every installed shell, that the setup runs before the agent. A failing setup skips the agent and the hand-back and exits non-zero.

**Step 2: Run the test to verify it fails**

Run: `cd packages/desktop && cargo test -p daemon`
Expected: FAIL to compile: `error[E0061]: this function takes 2 arguments but 3 arguments were supplied`.

**Step 3: Write the implementation**

```rust
/// Runs an agent inside the login shell, which takes over once the agent exits, so the user lands at their prompt.
pub fn agent_op(argv: &[String], cwd: &str) -> Value {
    spawn_op(login_shell(), agent_args(login_shell(), "", argv), cwd)
}

/// Runs `setup` in the terminal first, where the user can watch it; the agent starts only if it succeeds.
pub fn setup_op(setup: &str, argv: &[String], cwd: &str) -> Value {
    spawn_op(login_shell(), agent_args(login_shell(), setup, argv), cwd)
}

/// The agent's argv goes to the shell as arguments rather than inside the script, so no shell's quoting rules can garble a prompt.
fn agent_args(shell: &str, setup: &str, argv: &[String]) -> Vec<String> {
    let first = if setup.is_empty() { String::new() } else { format!("{setup} || exit; ") };
    let back = format!("exec '{shell}' -l");
    let mut args = vec!["-l".to_string(), "-c".to_string()];
    if Path::new(shell).file_name().is_some_and(|n| n == "fish") {
        args.push(format!("{first}$argv; {back}"));
    } else {
        // `sh -c` binds the first argument after the script to $0.
        args.extend([format!("{first}\"$@\"; {back}"), shell.to_string()]);
    }
    args.extend(argv.iter().cloned());
    args
}
```

**Step 4: Run the tests to verify they pass**

Run: `cd packages/desktop && cargo test -p daemon && cargo build -p pocket`
Expected: PASS; pocket builds with no warnings.

### Task 4.2: Worktree naming rules

**Files:**
- Modify: `packages/desktop/crates/pocket/src/forms.rs:1-9` (imports), `:67-70` (`slug`), add naming functions after `slug`
- Test: `packages/desktop/crates/pocket/src/forms.rs` (`mod tests`, ~line 889)

**Context:**
- One name is the folder (`<worktrees dir>/<name>`) and the branch, so it can't contain `/`.
- `taken` (filled in Task 4.3) is every local branch plus every folder in the worktrees dir.
- An auto name must be free: the slug gets `-2`, `-3`… when taken.
- The random name walks 64 `adjective-noun` pairs from a seed, so it stays stable while the popup is open.
- Until Task 4.3 calls these functions, the build warns that `auto_name`, `name_problem`, `free_name`, `unique`, `ADJECTIVES` and `NOUNS` are never used. That is expected.

**Step 1: Write the failing tests**

Replace the test module's `use` and the slug test, and add two tests:

```rust
    use super::{auto_name, name_problem, repo_from_url, slug};
    use std::collections::HashSet;

    #[test]
    fn slug_names_a_worktree_after_the_prompt() {
        assert_eq!(slug("The RestoreView snapshot fails on CI"), "the-restoreview-snapshot-fails");
        assert_eq!(slug("fix: flaky!"), "fix-flaky");
        assert_eq!(slug("  …  "), "");
    }

    #[test]
    fn an_empty_name_falls_back_to_a_free_one() {
        let taken: HashSet<String> = ["fix-flaky", "brave-otter", "brave-heron"].map(String::from).into();
        assert_eq!(auto_name("fix: flaky!", 0, &taken), "fix-flaky-2");
        assert_eq!(auto_name("Add dark mode", 0, &taken), "add-dark-mode");
        assert_eq!(auto_name("", 0, &taken), "brave-maple");
        assert_eq!(auto_name("", 63, &HashSet::new()), "swift-lynx");
        assert_eq!(auto_name("", 64, &HashSet::new()), "brave-otter");
    }

    #[test]
    fn a_name_must_be_free_and_valid_for_git() {
        let taken: HashSet<String> = ["main".to_string()].into();
        assert_eq!(name_problem("fix-login_2.0", &taken), None);
        assert_eq!(name_problem("main", &taken), Some("A worktree or branch with this name already exists"));
        for bad in ["", "fix login", "fix/login", "-x", ".x", "x.", "a..b", "x.lock", "tên"] {
            assert_eq!(name_problem(bad, &taken), Some("Use letters, digits, - _ or ."), "{bad}");
        }
    }
```

These prove:
- The slug has no `task/` prefix.
- A taken slug gets `-2`; a free one is kept.
- An empty prompt yields the first free random pair from the seed, and the seed wraps at 64.
- Taken names and names git or the folder can't hold are refused with the right message.

**Step 2: Run the tests to verify they fail**

Run: `cd packages/desktop && cargo test -p pocket forms`
Expected: FAIL to compile: `unresolved imports 'super::auto_name', 'super::name_problem'`.

**Step 3: Write the implementation**

Add `use std::collections::HashSet;` to the imports. Replace `slug` and add after it:

```rust
fn slug(prompt: &str) -> String {
    prompt.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).take(4).map(str::to_lowercase).collect::<Vec<_>>().join("-")
}

const ADJECTIVES: [&str; 8] = ["brave", "calm", "eager", "fuzzy", "keen", "lucky", "quiet", "swift"];
const NOUNS: [&str; 8] = ["otter", "heron", "maple", "comet", "falcon", "cedar", "koala", "lynx"];

fn free_name(seed: usize, taken: &HashSet<String>) -> String {
    (0..64).map(|i| (seed + i) % 64).map(|n| format!("{}-{}", ADJECTIVES[n / 8], NOUNS[n % 8])).find(|n| !taken.contains(n)).unwrap_or_else(|| unique("worktree", taken))
}

/// `base`, or `base-2`, `base-3`… when taken.
fn unique(base: &str, taken: &HashSet<String>) -> String {
    std::iter::once(base.to_string()).chain((2..).map(|n| format!("{base}-{n}"))).find(|n| !taken.contains(n)).unwrap()
}

/// The name a worktree gets when the user leaves Name empty: the prompt's slug, else a random one.
fn auto_name(prompt: &str, seed: usize, taken: &HashSet<String>) -> String {
    match slug(prompt) {
        s if s.is_empty() => free_name(seed, taken),
        s => unique(&s, taken),
    }
}

/// Why `name` can't name a new worktree and its branch, if it can't.
fn name_problem(name: &str, taken: &HashSet<String>) -> Option<&'static str> {
    let valid = !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        && !name.starts_with(['-', '.'])
        && !name.ends_with('.')
        && !name.ends_with(".lock")
        && !name.contains("..");
    if taken.contains(name) {
        Some("A worktree or branch with this name already exists")
    } else if !valid {
        Some("Use letters, digits, - _ or .")
    } else {
        None
    }
}
```

**Step 4: Run the tests to verify they pass**

Run: `cd packages/desktop && cargo test -p pocket forms`
Expected: PASS (4 tests); only the never-used warnings listed in Context.

### Task 4.3: The Name field in New worktree

**Files:**
- Modify: `packages/desktop/crates/pocket/src/forms.rs`:
  - `:18-30` (`NewForm`), `:157-186` (`NewForm::new`)
  - `:253-308` (`reset_new_form`, `pick_repo`), `:310-312` (`new_branch` → `new_name`)
  - `:326-333` (`session_ready`), `:355-362` (`start_session` name/path)
  - `:471-567` (`new_session_view`)

**Context:**
- The form's `branch` input becomes `name`.
- `pick_repo` loads branches in the background. It now also loads `taken` (all local branches + folder names in the worktrees dir) and needs `window` to refresh the Name placeholder once `taken` arrives, so it switches to `cx.spawn_in(window, …)` + `update_in`, as `pick_path` does.
- `seed` is set once per popup open so the random placeholder doesn't change while typing.
- The footer summary's inline branch input and its width hack go. The summary now reads `New branch <name> from <base>`.

**Step 1: Write the implementation**

`NewForm`: replace `branch: Entity<InputState>,` with

```rust
    name: Entity<InputState>,
    /// Branches and worktree folders a new worktree's name must not reuse.
    taken: HashSet<String>,
    seed: usize,
```

`NewForm::new`: rename `let branch = …` to `let name = cx.new(|cx| InputState::new(window, cx));`. The `cx.subscribe(&branch, …)` becomes `cx.subscribe(&name, |_, _, _: &InputEvent, cx| cx.notify()),`. In `Self { … }`, `branch,` becomes `name, taken: HashSet::new(), seed: 0,`. The prompt's `InputEvent::Change` arm becomes:

```rust
                InputEvent::Change => {
                    let f = &this.new_form;
                    let name = auto_name(&prompt.read(cx).value(), f.seed, &f.taken);
                    this.new_form.name.update(cx, |b, cx| b.set_placeholder(name, window, cx));
                    cx.notify();
                }
```

Replace `reset_new_form` and `pick_repo`:

```rust
    pub fn reset_new_form(&mut self, prompt: Option<String>, worktree: bool, window: &mut Window, cx: &mut Context<Self>) {
        let text = prompt.unwrap_or_default();
        let f = &mut self.new_form;
        f.seed = crate::view::now_ms() as usize;
        f.taken.clear();
        let placeholder = auto_name(&text, f.seed, &f.taken);
        f.prompt.update(cx, |s, cx| {
            s.set_value(text, window, cx);
            s.focus(window, cx);
        });
        f.name.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.set_placeholder(placeholder, window, cx);
        });
        f.worktree = worktree;
        f.perm = Perm::Ask;
        f.picker = None;
        if let Some(repo) = self.project.clone() {
            self.pick_repo(repo, window, cx);
        }
    }

    fn pick_repo(&mut self, repo: String, window: &mut Window, cx: &mut Context<Self>) {
        let cfg = self.store.repos.get(&repo).cloned().unwrap_or_default();
        let folders = self.worktrees_dir(&repo);
        let f = &mut self.new_form;
        f.repo = Some(repo.clone());
        f.branches.clear();
        f.base = 0;
        f.copy_env = !cfg.copy.is_empty();
        f.run_setup = !cfg.setup.is_empty();
        let dir = repo.clone();
        let task = cx.background_executor().spawn(async move {
            let current = git::read(&dir).map(|r| r.branch).unwrap_or_default();
            let all = git::branches(&dir);
            let branches: Vec<(String, Option<i64>)> = all.iter().take(20).map(|b| {
                let at = git::committed_at(&dir, b);
                (b.clone(), at)
            }).collect();
            let entries = std::fs::read_dir(&folders).into_iter().flatten().flatten();
            let taken: HashSet<String> = all.into_iter().chain(entries.filter_map(|e| e.file_name().into_string().ok())).collect();
            (current, branches, taken)
        });
        cx.spawn_in(window, async move |this, cx| {
            let (current, mut branches, taken) = task.await;
            this.update_in(cx, |d, window, cx| {
                let f = &mut d.new_form;
                if f.repo.as_ref() != Some(&repo) {
                    return;
                }
                let default = default_base(branches.iter().map(|(b, _)| b.as_str()), &cfg.base, &current);
                if !branches.is_empty() {
                    branches[..=default].rotate_right(1);
                }
                f.base = 0;
                f.branches = branches;
                f.taken = taken;
                let name = auto_name(&f.prompt.read(cx).value(), f.seed, &f.taken);
                f.name.update(cx, |s, cx| s.set_placeholder(name, window, cx));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
```

Replace `new_branch` with:

```rust
    fn new_name(&self, cx: &App) -> String {
        let f = &self.new_form;
        typed_or(&f.name, || auto_name(&f.prompt.read(cx).value(), f.seed, &f.taken), cx)
    }
```

`session_ready`, the worktree line becomes:

```rust
            f.repo.is_some() && !f.branches.is_empty() && name_problem(&self.new_name(cx), &f.taken).is_none()
```

`start_session`, the worktree branch's first lines become (and `git::add_worktree(&repo, &path, &branch, &base)?` becomes `git::add_worktree(&repo, &path, &name, &base)?`):

```rust
                let name = self.new_name(cx);
                let path = format!("{}/{name}", self.worktrees_dir(&repo));
```

`new_session_view`:
- Before `let composer = …`, add:

```rust
        let problem = if f.worktree { name_problem(&self.new_name(cx), &f.taken) } else { None };
        let name_field = f.worktree.then(|| {
            ui::field_box()
                .child(icon("worktree", 14., TEXT_3))
                .child(div().flex_1().min_w_0().font_family(MONO).child(Input::new(&f.name).appearance(false).p_0().text_size(px(13.))))
                .children(problem.map(|p| div().flex_none().text_size(px(12.)).text_color(rgba(FAILED)).child(p)))
        });
```

- Replace the worktree arm of `summary` (the `let width = …` line, the comment and the `Input` element included) with:

```rust
            vec![
                div().child("New branch").into_any_element(),
                mono(self.new_name(cx)).into_any_element(),
                div().child("from").into_any_element(),
                mono(base).into_any_element(),
            ]
```

- In the final `ui::pop(…)` chain, `.child(header).child(composer)` becomes `.child(header).children(name_field).child(composer)`.

**Step 2: Build and test**

Run: `cd packages/desktop && cargo build -p pocket && cargo test -p pocket`
Expected: builds with no warnings; tests PASS.

**Step 3: Manual check** (run the app, press ⌘⇧N on a git project)
- The Name field shows a grey random `adjective-noun`.
- Typing "Fix login bug" in the prompt changes it to `fix-login-bug`, and the footer reads `New branch fix-login-bug from <base>`.
- Typing `main` in Name shows the red "already exists" message and greys out the send button. `fix login` shows "Use letters, digits, - _ or .".
- Starting creates `~/.worktrees/<repo>/<name>` on branch `<name>`, and the row appears at the bottom of the project.

### Task 4.4: Setup runs in the worktree's terminal

**Files:**
- Modify: `packages/desktop/crates/ui/src/ui.rs` (add `setting_up` after `indicator`)
- Modify: `packages/desktop/crates/pocket/src/main.rs:79-82` (`Intent`), fields/init, `on_agents` (`:271-285`), `on_msg` (`:358-376`), `close_pane` (`:536-547`)
- Modify: `packages/desktop/crates/pocket/src/forms.rs` (`start_session` worktree branch)
- Modify: `packages/desktop/crates/pocket/src/view.rs` (`project_rows` worktree loop)

**Context:**
- Today the setup runs hidden in the background before the terminal opens; a failure only shows as an error string.
- Now the new terminal runs `daemon::setup_op(setup, argv, path)` (Task 4.1), so the user watches it.
- `setups` maps terminal → worktree while the setup may still run. An entry ends when pocketd lists an agent in that terminal (`agents.list[].terminal_id`), or when the terminal exits or closes.
- A failed setup exits non-zero, so `close_clean_exits` keeps the pane and its output.

**Step 1: Write the implementation**

`ui.rs` after `indicator`:

```rust
pub fn setting_up(id: impl Into<ElementId>) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(5.))
        .text_size(px(12.))
        .font_weight(FontWeight::NORMAL)
        .text_color(rgba(TEXT_4))
        .child(spinner(id, 11., TEXT_4))
        .child("Setting up…")
}
```

`main.rs`:
- `Intent` gains `Setup(String),`.
- Field after `worktree`:

```rust
    /// Terminal → worktree while the terminal runs the worktree's setup before its agent.
    setups: HashMap<String, String>,
```

  with init `setups: HashMap::new(),`.
- In `on_agents`, right after `self.agents.apply(ev);`:

```rust
        let agents = &self.agents;
        self.setups.retain(|term, _| !agents.list.iter().any(|a| &a.terminal_id == term));
```

- In `on_msg` `"spawned"`, add the arm:

```rust
                    Some(Intent::Setup(tree)) => {
                        self.setups.insert(m.id.clone(), tree.clone());
                        self.adopt(m.id.clone(), tree, None, window, cx)
                    }
```

- In `on_msg` `"exit"`, add `self.setups.remove(&m.id);` before `self.close_clean_exits(&m.id, cx);`.
- In `close_pane`, add `self.setups.remove(id);` after `self.sized.remove(id);`.

`forms.rs` `start_session`, worktree branch: replace the background task and the `cx.spawn` after it with:

```rust
                let task = cx.background_executor().spawn(async move {
                    git::add_worktree(&repo, &path, &name, &base)?;
                    for rel in copy {
                        let to = Path::new(&path).join(&rel);
                        if let Some(dir) = to.parent() {
                            std::fs::create_dir_all(dir).ok();
                        }
                        std::fs::copy(Path::new(&repo).join(&rel), to).ok();
                    }
                    Ok::<_, String>(path)
                });
                cx.spawn(async move |this, cx| {
                    let res = task.await;
                    this.update(cx, |d, cx| {
                        match res {
                            Ok(path) if setup.is_empty() => d.send_spawn(daemon::agent_op(&argv, &path), Intent::Tab(path), cx),
                            Ok(path) => d.send_spawn(daemon::setup_op(&setup, &argv, &path), Intent::Setup(path), cx),
                            Err(e) => d.error = Some(e),
                        }
                        d.refresh_git(cx);
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
```

`view.rs` `project_rows`, in the worktree loop, replace the `let mark = …` line with:

```rust
            let mark = if self.setups.values().any(|t| t == tree) {
                Some(ui::setting_up(id(format!("aside-setup:{tree}"))).into_any_element())
            } else {
                ui::indicator(id(format!("aside-tree-spin:{tree}")), roll(&self.tree_cards(p, tree)))
            };
```

**Step 2: Run the full suite**

Run: the desktop full line from Toolset.
Expected: PASS, no warnings.

**Step 3: Manual check**
- Set a project's Setup (Project settings) to `sleep 3 && echo done`, then create a worktree with "run setup" on.
  - The new row shows "Setting up…" with a spinner.
  - The terminal shows the command, then `done`, then the agent starts, and the row switches to the agent's status.
- Set Setup to `false` and create another worktree.
  - The terminal stays open with a non-zero exit, and no agent starts.
  - "Setting up…" disappears.
- With "run setup" off, the agent starts at once.
