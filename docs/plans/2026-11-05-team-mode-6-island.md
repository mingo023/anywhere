# Team Mode 6: Island Team UI Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** Build the Island team UI on the desktop. A plan sheet lets you edit and start the lead's team. A dark capsule over the lead's chat shows a ring per task. An alert takes you to a worker that needs you. A popover lists the tasks, with Open and Stop. Merge shows the lead's verdict, and asks you to confirm when the lead hasn't said the work is ready.

**Architecture:** A new feature module `pocket/src/team.rs` + `pocket/src/team/` owns `TeamUi`: the plain `Island` state, which is tested, plus the allowlist `InputState`. The decisions are plain functions over `agents::Team`: `team/plan.rs` handles the sheet's edits (`Edits` → `Vec<TaskPick>`), and `team/progress.rs` turns task states into marks, ring fills and labels. The views are `team/sheet.rs` (an `Overlay::Team` modal), `team/capsule.rs` (the capsule and the Needs-you alert, hung above the chat composer the same way `chat/agent_map.rs` hangs the agent map), `team/list.rs` (the task popover with the verdict and Merge) and `team/ring.rs` (a canvas ring). `impl Desktop` glue stays thin: it reads `self.agents.team(lead)` and calls `self.outbox.team_*`.

**Toolset (from `packages/desktop`):**
- This plan's tests: `cargo test -p pocket team::` (all of them), `cargo test -p pocket team::plan`, `cargo test -p pocket team::progress`, `cargo test -p pocket team::tests`, `cargo test -p pocket confirm`
- Everything: `cargo build --workspace`, `cargo clippy --workspace --all-targets` (no new warnings), `cargo test --workspace`
- Screens (from the repo root): `.ui-review/fixture/capture.sh <dir> <name>=<step>,<step> …`. A name must start with `chat` so the script picks the chat fixture.

**Read first:**
- `docs/plans/2026-11-05-team-mode.md`: the decisions and the wire/desktop contracts (`Team`, `TeamTask`, states, `TaskPick`, `Outbox::team_*`).
- `CLAUDE.md` and `docs/adr/0003-desktop-code-layout.md`. `pocket` holds views. Each feature is one module with its own state built by its own `new`. `impl Desktop` stays thin. Logic goes on plain types with tests beside it. Render must cost what's visible.
- `.ui-review/prototypes/orchestrator-team.html`: the design, the **Island** variant (`island()`, the `#sheet`/`.isl`/`#ilist` CSS, and `counts`, `summary`, `nowText`).
- `crates/pocket/src/chat/agent_map.rs` (~L240-316): how a panel hangs above the composer (`relative().h_0()` + `absolute().bottom_full()`), and why capture handlers run in paint order.
- `crates/pocket/src/composer/model_menu.rs` + `crates/pocket/src/composer/launch.rs` (`model_choices`): the per-provider model rows this plan reuses.
- `crates/pocket/src/modals.rs` (`may_open`, `open`, `overlay_view`) and `crates/pocket/src/modals/confirm.rs` (`ConfirmText`, `confirm_view`, `confirmed`).
- `crates/pocket/src/capture.rs` (`STEPS`, `reset`, `chat-sub-live`, which applies agent events in-process).

## Contracts this plan relies on (from plan 5)

The code below assumes these exact names. **Task 1.0 checks them.** If plan 5 shipped different names, use plan 5's names everywhere in this plan's code. Do not rename plan 5's API.

```rust
// crates/agents
#[derive(Deserialize, Clone, Debug, Default, PartialEq)] #[serde(default, rename_all = "camelCase")]
pub struct Team { pub lead: String, pub state: String, pub error: String, pub tasks: Vec<TeamTask>, pub ready: bool, pub conflict: Vec<String>, pub repaired: bool, pub updated_at: i64 }
#[derive(Deserialize, Clone, Debug, Default, PartialEq)] #[serde(default, rename_all = "camelCase")]
pub struct TeamTask { pub id: String, pub title: String, pub agent: String, pub model: String, pub files: Vec<String>, pub after: Vec<String>, pub allow: Vec<String>, pub instructions: String, pub state: String, pub worker: String, pub branch: String, pub worktree: String, pub summary: String, pub stat: String }
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct TaskPick { pub id: String, pub agent: String, pub model: String, pub allow: Vec<String> }
pub enum TeamAction { Plan(String), Start, Stop, Merge }  // Plan carries the refused team prompt's request id
pub enum Event { …, Team(Team), Teams(Vec<Team>), TeamFailed { lead: String, action: TeamAction, message: String } }      // Teams replaces every team of the pocketd that sent it
impl Agents { pub fn team(&self, lead: &str) -> Option<&Team> }
// crates/hosts/src/session.rs, routed by the lead id's host tag like `pin`
impl Outboxes { pub fn team_start(&self, lead: &str, tasks: &[TaskPick]); pub fn team_stop(&self, lead: &str, task: &str); pub fn team_merge(&self, lead: &str) }
```

States are strings. Team states: `planning | proposed | running | review | merging | merged | conflict | failed`. Task states: `planned | waiting | running | needsYou | done | failed | stopped | merged`.

## Design → GPUI translation

| Prototype | App |
|---|---|
| `--surface`, `--pop-shadow`, 16px radius sheet | `ui::pop(..)` + `.rounded(px(R_DIALOG))` |
| `--hairline`, `--text-2/3/4`, `--fill-2/3/4` | `HAIRLINE`, `TEXT_2/3/4`, `FILL_2/3/4` |
| `--waiting` / `--success` / `--failed` | `WAITING`, `WAITING_TEXT`, `SUCCESS`, `FAILED`; merged → `MERGED` |
| `.btn.primary` / `.btn.ghost` | `ui::button(.., Variant::Primary / Ghost / Secondary, ..)` |
| `.isl` `rgba(22,22,26,.88)` + `backdrop-filter: blur` | `Token::fixed(0x16161af2)`, no blur (GPUI can't blur what's behind an element, so it's nearly opaque, like the design's `prefers-reduced-transparency` rule) |
| ring colours `#ff9f43` / `#3dd68c` / `#9b9bff` | `0xffb224` (`WAITING_DOT`) / `0x3dd68c` (dark `SUCCESS_TEXT`) / `0xa371f7` (dark `MERGED`), fixed, because the capsule is dark in both appearances |
| ring `stroke-dasharray` progress | `PathBuilder::stroke` + `arc_to` on a `canvas`. Tasks report no progress until they hand back, so a running ring is a quarter arc that turns at 20 fps, and holds still under Reduce motion |
| scrim `rgba(17,17,19,.16)` | `ui::backdrop` alpha `0x29` via `overlay_view` |
| sheet `pop` 240ms, list `translateY+scale`, capsule `scale(.86)` | sheet: `overlay_view`'s fade + 8px rise. List: `ui::menu_in`. Capsule/alert: fade + 6px rise. **Skipped:** scale transforms (GPUI divs have none) and the sheet folding into the capsule along a shared transform origin |
| `⌘↵` Start, `esc` | `cx.intercept_keystrokes` (as `confirm_on_enter`); Esc already closes menus, then the overlay |
| `−/+ N at once` stepper | dropped: `team.Parallel = 4` isn't editable (index decision). The footer says "Up to 4 run at once." |
| alert's Deny / Allow once | kept for a plain permission ask (`outbox.resolve`), plus **Open**, which jumps to the worker |

---

## PR 1: The plan sheet

**Scope:** `team.rs` with `TeamUi`/`Island`, `Edits`, and the `Overlay::Team` sheet. The sheet lets you remove tasks, pick each task's agent and model, edit its allowlist, and press Start (also ⌘↵), which calls `team_start`. The sheet opens by itself when a team turns `proposed` while you view its lead. Start works with a single kept task. A minimal capsule ("Team plan · N agents · Review") reopens it. A refused team request (start, stop, merge, or a team prompt such as `team_busy`) shows as a toast naming the action and pocketd's message. The lead reads Needs you while its plan waits from pocketd itself (plan 3), so this plan overrides no status. Running states come in PR 2.
**Depends on:** plans 1–5 merged.
**Done when:** `cargo test -p pocket team::` is green, the workspace builds, clippy adds no warnings, and captures show the sheet, an edited sheet with the model menu open, and the proposed capsule.

### Task 1.0: Check the plan 5 contracts

**Files:** none changed unless a check fails.

**Step 0: Capture "before"** (from the repo root, before any code in this plan lands):

```bash
.ui-review/fixture/capture.sh /tmp/team-6/before chat-lead=chat
```

Expected: prints `/tmp/team-6/before/impl-chat-lead.png`, the lead's chat with no team.

**Step 1:** From `packages/desktop` run:

```bash
grep -n "pub struct Team\b\|pub struct TeamTask\|pub struct TaskPick\|pub enum TeamAction\|Team(Team)\|Teams(Vec<Team>)\|pub fn team(" crates/agents/src/*.rs
grep -n "pub fn team_start\|pub fn team_stop\|pub fn team_merge" crates/hosts/src/session.rs
```

Expected: at least one hit for each of the seven agents patterns, and three hits in `session.rs`.

**Step 2: Fix only what's missing.**
- `Agents::team` missing, but the teams are stored in a map: add `pub fn team(&self, lead: &str) -> Option<&Team> { self.<field>.get(lead) }` next to `Agents::get` (`agents.rs` ~L465).
- The `Outboxes` wrappers missing: add these to `impl Outboxes` in `crates/hosts/src/session.rs`, after `pin`. The `agents::Outbox` methods come from plan 5.

```rust
    pub fn team_start(&self, lead: &str, tasks: &[agents::TaskPick]) {
        self.by_id(lead, |o, id| o.team_start(id, tasks));
    }

    pub fn team_stop(&self, lead: &str, task: &str) {
        self.by_id(lead, |o, id| o.team_stop(id, task));
    }

    pub fn team_merge(&self, lead: &str) {
        self.by_id(lead, |o, id| o.team_merge(id));
    }
```

- `Team`/`TeamTask` don't derive `Default`/`PartialEq`, or `TaskPick` doesn't derive `Debug`/`PartialEq`: add those derives. The tests below need them.

**Step 3:** `cargo build --workspace` → builds.

### Task 1.1: `team/plan.rs`: the sheet's edits

**Files:**
- Create: `packages/desktop/crates/pocket/src/team/plan.rs`
- Create: `packages/desktop/crates/pocket/src/team.rs` (only `pub(crate) mod plan;` for now; Task 1.2 fills it)
- Modify: `packages/desktop/crates/pocket/src/main.rs` (add `mod team;` after `mod syntax;`, keeping the alphabetical order)

**Context:** The sheet lets you remove a task, change its agent and model, and edit its allowlist (index: "Plan sheet edits"). Titles, files and `after` stay as the lead planned them. **Removal rule (the simpler of the two): a task that another kept task lists in `after` can't be removed.** Its × is dimmed until the waiting task goes. A task with no model shows the model Settings starts its agent with (`store.agents.provider(agent).model`). pocketd launches workers itself, so it never sees Settings, and the sheet must send that model. `branch` mirrors pocketd's `team.Branch`, which uses the lead's id on its own pocketd, so a host tag is stripped. It is the name pocketd tries first: when a task starts, pocketd takes `name-2`, `name-3`, … if that branch or worktree is taken, and stores what it chose in `TeamTask.branch`. A proposed team has no stored branches yet, so the sheet shows the preferred name.

**Step 1: Write the failing tests.** Create `team.rs` containing only:

```rust
//! Team mode on the desktop: the plan sheet a lead's proposal opens, and the island over the lead's chat.

pub(crate) mod plan;
```

Create `team/plan.rs` containing only the tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use agents::{TaskPick, Team, TeamTask};

    fn task(id: &str, agent: &str, after: &[&str]) -> TeamTask {
        TeamTask {
            id: id.into(),
            title: format!("Task {id}"),
            agent: agent.into(),
            files: vec![format!("{id}.rs")],
            after: after.iter().map(|a| a.to_string()).collect(),
            allow: vec!["cargo test".into()],
            state: "planned".into(),
            ..Default::default()
        }
    }

    /// Settings starts Claude on opus and leaves Codex on its own default.
    fn plan(tasks: Vec<TeamTask>) -> Edits {
        let team = Team { lead: "lead".into(), state: "proposed".into(), tasks, ..Default::default() };
        Edits::new(&team, |agent| if agent == "claude" { "opus".into() } else { String::new() })
    }

    fn ids(e: &Edits) -> Vec<&str> {
        e.tasks().iter().map(|t| t.id.as_str()).collect()
    }

    fn pick(id: &str, agent: &str, model: &str, allow: &[&str]) -> TaskPick {
        TaskPick { id: id.into(), agent: agent.into(), model: model.into(), allow: allow.iter().map(|a| a.to_string()).collect() }
    }

    #[test]
    fn a_sheet_starts_from_the_plan_with_each_agent_s_default_model() {
        let named = TeamTask { model: "sonnet".into(), ..task("t3", "claude", &[]) };
        let e = plan(vec![task("t1", "claude", &[]), task("t2", "codex", &["t1"]), named]);
        assert_eq!(e.picks(), [pick("t1", "claude", "opus", &["cargo test"]), pick("t2", "codex", "", &["cargo test"]), pick("t3", "claude", "sonnet", &["cargo test"])]);
    }

    #[test]
    fn a_removed_task_is_left_out_of_start() {
        let mut e = plan(vec![task("t1", "claude", &[]), task("t2", "codex", &[]), task("t3", "claude", &[])]);
        e.remove("t2");
        assert_eq!(ids(&e), ["t1", "t3"]);
    }

    #[test]
    fn a_task_another_waits_for_stays_until_that_one_goes() {
        let mut e = plan(vec![task("t1", "claude", &[]), task("t2", "codex", &["t1"])]);
        e.remove("t1");
        assert_eq!((ids(&e), e.removable("t1")), (vec!["t1", "t2"], false));
        e.remove("t2");
        e.remove("t1");
        assert_eq!(ids(&e), Vec::<&str>::new());
    }

    #[test]
    fn picking_a_model_moves_the_task_to_that_agent() {
        let mut e = plan(vec![task("t1", "claude", &[])]);
        e.pick("t1", "codex", "gpt-5.5");
        assert_eq!(e.picks(), [pick("t1", "codex", "gpt-5.5", &["cargo test"])]);
    }

    #[test]
    fn a_prefix_is_allowed_once_trimmed_and_can_be_taken_back() {
        let mut e = plan(vec![task("t1", "claude", &[])]);
        e.allow("t1", "  npm test ");
        e.allow("t1", "npm test");
        e.allow("t1", "   ");
        e.disallow("t1", "cargo test");
        assert_eq!(e.tasks()[0].allow, ["npm test"]);
    }

    #[test]
    fn a_task_names_what_it_waits_for_by_its_place_on_the_sheet() {
        let mut e = plan(vec![task("t1", "claude", &[]), task("t2", "codex", &[]), task("t3", "claude", &["t2"])]);
        e.remove("t1");
        let labels: Vec<Option<String>> = e.tasks().iter().map(|t| e.after_label(t)).collect();
        assert_eq!(labels, [None, Some("after 1".to_string())]);
    }

    #[test]
    fn a_task_s_branch_is_the_name_pocketd_tries_first() {
        assert_eq!([branch("a1b2c3d4e5", "t2"), branch("devbox/a1b2c3d4e5", "t2")], ["team-a1b2c3-t2", "team-a1b2c3-t2"]);
    }

    #[test]
    fn a_sheet_down_to_one_task_can_still_start() {
        assert_eq!([can_start(true, 1), can_start(true, 4), can_start(true, 0), can_start(false, 3)], [true, true, false, false]);
    }
}
```

What each test proves, in order: defaults come from Settings and a model the lead named is kept; Remove drops the task from Start; the removal rule; a model pick also switches the agent; allowlist add/trim/dedupe/remove; `after` is renumbered after a removal; branch names match the one pocketd tries first; Start needs one kept task, not two, and only while the plan waits (pocketd's `team.start` takes 1 to 4).

**Step 2: Run** `cargo test -p pocket team::plan`
Expected: FAIL to compile with `error[E0425]: cannot find function \`branch\``, `cannot find function \`can_start\`` and `cannot find type \`Edits\``.

**Step 3: Implement.** Put this above the tests in `team/plan.rs`:

```rust
//! The plan sheet's edits: what you keep of the lead's plan before Start.

use agents::{TaskPick, Team};

/// A task of the lead's plan as the sheet shows it, with your edits.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Kept {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) files: Vec<String>,
    pub(crate) after: Vec<String>,
    pub(crate) agent: String,
    pub(crate) model: String,
    pub(crate) allow: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Edits {
    tasks: Vec<Kept>,
}

impl Edits {
    /// The plan as the lead proposed it. pocketd starts workers without Settings, so a task with no model takes `default_model` of its agent here.
    pub(crate) fn new(team: &Team, default_model: impl Fn(&str) -> String) -> Self {
        let tasks = team
            .tasks
            .iter()
            .map(|t| Kept {
                id: t.id.clone(),
                title: t.title.clone(),
                files: t.files.clone(),
                after: t.after.clone(),
                agent: t.agent.clone(),
                model: if t.model.is_empty() { default_model(&t.agent) } else { t.model.clone() },
                allow: t.allow.clone(),
            })
            .collect();
        Self { tasks }
    }

    pub(crate) fn tasks(&self) -> &[Kept] {
        &self.tasks
    }

    /// A task another kept task waits for stays; removing it would leave that one waiting on nothing.
    pub(crate) fn removable(&self, id: &str) -> bool {
        !self.tasks.iter().any(|t| t.after.iter().any(|a| a == id))
    }

    pub(crate) fn remove(&mut self, id: &str) {
        if self.removable(id) {
            self.tasks.retain(|t| t.id != id);
        }
    }

    pub(crate) fn pick(&mut self, id: &str, agent: &str, model: &str) {
        if let Some(t) = self.get_mut(id) {
            (t.agent, t.model) = (agent.into(), model.into());
        }
    }

    /// Adds `prefix`, trimmed; an empty or repeated one adds nothing.
    pub(crate) fn allow(&mut self, id: &str, prefix: &str) {
        let prefix = prefix.trim();
        if let Some(t) = self.get_mut(id).filter(|t| !prefix.is_empty() && !t.allow.iter().any(|a| a == prefix)) {
            t.allow.push(prefix.into());
        }
    }

    pub(crate) fn disallow(&mut self, id: &str, prefix: &str) {
        if let Some(t) = self.get_mut(id) {
            t.allow.retain(|a| a != prefix);
        }
    }

    /// What `team.start` takes: the kept tasks, in the plan's order.
    pub(crate) fn picks(&self) -> Vec<TaskPick> {
        self.tasks.iter().map(|t| TaskPick { id: t.id.clone(), agent: t.agent.clone(), model: t.model.clone(), allow: t.allow.clone() }).collect()
    }

    pub(crate) fn after_label(&self, t: &Kept) -> Option<String> {
        let ids: Vec<&str> = self.tasks.iter().map(|t| t.id.as_str()).collect();
        after_label(&t.after, &ids)
    }

    fn get_mut(&mut self, id: &str) -> Option<&mut Kept> {
        self.tasks.iter_mut().find(|t| t.id == id)
    }
}

/// "after 1" or "after 1, 2": the tasks in `after` by their 1-based place among `ids`; none when it waits for none of them.
pub(crate) fn after_label(after: &[String], ids: &[&str]) -> Option<String> {
    let places: Vec<String> = ids.iter().enumerate().filter(|(_, id)| after.iter().any(|a| a == *id)).map(|(i, _)| (i + 1).to_string()).collect();
    (!places.is_empty()).then(|| format!("after {}", places.join(", ")))
}

/// The branch pocketd tries first for a task's worker, `team-<lead6>-<task>`, from the lead's id on its own pocketd; it adds `-2`, `-3`, … when that's taken.
pub(crate) fn branch(lead: &str, task: &str) -> String {
    let (_, lead) = hosts::session::untag(lead);
    format!("team-{}-{task}", lead.get(..6).unwrap_or(lead))
}

/// Start is offered while the plan waits and keeps at least one task; pocketd starts 1 to 4.
pub(crate) fn can_start(proposed: bool, kept: usize) -> bool {
    proposed && kept > 0
}
```

**Step 4: Run** `cargo test -p pocket team::plan`
Expected: PASS, `test result: ok. 8 passed`. (Until Task 1.3 uses them, `cargo build` warns that items are never used. That's expected inside this PR, and the PR's final check has no such warnings.)

### Task 1.2: `TeamUi`, `Island`, and wiring into `Desktop`

**Files:**
- Modify: `packages/desktop/crates/pocket/src/team.rs` (replace the whole file)
- Modify: `packages/desktop/crates/pocket/src/desktop.rs` (field, `Desktop::new`, `menu_open`, `close_menus`)
- Modify: `packages/desktop/crates/pocket/src/desktop/alerts.rs` (`on_agents_from`, ~L126)
- Modify: `packages/desktop/crates/pocket/src/desktop/chrome.rs` (`Overlay::Team`)
- Modify: `packages/desktop/crates/pocket/src/modals.rs` (`may_open`, `open`, `overlay_view`)

**Context:**
- `Island` is plain data, so it's tested. `TeamUi` holds it and the allowlist `Entity<InputState>`, which can't be built in a test. This follows CLAUDE.md's "logic on a plain value type" rule.
- `saw` tracks each lead's last state, so the sheet is offered once per proposal. A new proposal after a re-plan drops the old edits.
- `Event::Teams` (sent on connect) records states without offering anything, so nothing pops up at launch.
- **Needs you while proposed** comes from pocketd: plan 3 marks the lead's agent as awaiting the user while its team is proposed, so its status reads `needsYou` on the sidebar card, in banners, on the Dock badge and on the phone. Nothing here overrides a status; the sheet still opens by itself.
- `Overlay::Team` is `Copy` with no payload. The lead lives in `island.sheet`.

**Step 1: Write the failing tests.** Replace `team.rs` with the module header, the `mod` lines and these tests (the implementation comes in Step 3):

```rust
//! Team mode on the desktop: the plan sheet a lead's proposal opens, and the island over the lead's chat.

pub(crate) mod plan;

#[cfg(test)]
mod tests {
    use super::*;
    use agents::TeamTask;

    fn team(state: &str) -> Team {
        Team { lead: "lead".into(), state: state.into(), tasks: vec![TeamTask { id: "t1".into(), agent: "claude".into(), ..Default::default() }], ..Default::default() }
    }

    #[test]
    fn a_plan_offers_its_sheet_once_when_it_comes_in() {
        let mut i = Island::default();
        assert_eq!([i.saw(&team("planning")), i.saw(&team("proposed")), i.saw(&team("proposed"))], [false, true, false]);
    }

    #[test]
    fn a_new_plan_drops_the_edits_of_the_last() {
        let mut i = Island::default();
        i.saw(&team("proposed"));
        i.show_sheet(&team("proposed"), |_| "opus".into());
        i.sheet_edits().unwrap().remove("t1");
        i.saw(&team("planning"));
        i.saw(&team("proposed"));
        i.show_sheet(&team("proposed"), |_| "opus".into());
        assert_eq!(i.sheet_edits().map(|e| e.picks().len()), Some(1));
    }

    #[test]
    fn start_takes_the_kept_tasks_only_while_the_plan_waits() {
        let mut i = Island::default();
        i.show_sheet(&team("proposed"), |_| "opus".into());
        assert_eq!(i.start(Some("running")), None);
        let kept = vec![TaskPick { id: "t1".into(), agent: "claude".into(), model: "opus".into(), allow: vec![] }];
        assert_eq!(i.start(Some("proposed")), Some(("lead".to_string(), kept)));
        assert_eq!(i.start(Some("proposed")), None, "the edits went with the start");
    }

}
```

**Step 2: Run** `cargo test -p pocket team::tests`
Expected: FAIL to compile with `cannot find type \`Island\``.

**Step 3: Implement.** Final `team.rs` for PR 1. Keep the tests from Step 1 at the bottom:

```rust
//! Team mode on the desktop: the plan sheet a lead's proposal opens, and the island over the lead's chat.

pub(crate) mod capsule;
pub(crate) mod plan;
mod sheet;

use crate::desktop::Desktop;
use crate::desktop::chrome::Overlay;
use agents::{TaskPick, Team};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use plan::Edits;
use std::collections::HashMap;

/// A team whose plan waits on Start.
pub(crate) const PROPOSED: &str = "proposed";

/// "1 agent", "2 agents".
pub(crate) fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// What the team UI shows; plain data, so it's tested without a window.
#[derive(Default)]
pub(crate) struct Island {
    /// The lead whose plan sheet was last opened; the sheet shows while `Overlay::Team` does.
    pub(crate) sheet: Option<String>,
    /// The task whose model menu is open on the sheet.
    pub(crate) menu: Option<String>,
    /// The task whose allowlist `TeamUi::allow` adds to.
    pub(crate) adding: Option<String>,
    /// Each lead's plan as edited, until it starts or a new plan replaces it.
    pub(crate) edits: HashMap<String, Edits>,
    /// Each lead's team state as last seen.
    seen: HashMap<String, String>,
}

impl Island {
    /// Records `team`'s state; true when it just turned proposed. A new proposal drops the edits of the last.
    pub(crate) fn saw(&mut self, team: &Team) -> bool {
        let was = self.seen.insert(team.lead.clone(), team.state.clone());
        let turned = team.state == PROPOSED && was.as_deref() != Some(PROPOSED);
        if turned {
            self.edits.remove(&team.lead);
        }
        turned
    }

    /// Shows `team`'s sheet, with the edits made before it was put away.
    pub(crate) fn show_sheet(&mut self, team: &Team, default_model: impl Fn(&str) -> String) {
        self.edits.entry(team.lead.clone()).or_insert_with(|| Edits::new(team, default_model));
        (self.sheet, self.menu, self.adding) = (Some(team.lead.clone()), None, None);
    }

    pub(crate) fn sheet_edits(&mut self) -> Option<&mut Edits> {
        let lead = self.sheet.as_ref()?;
        self.edits.get_mut(lead)
    }

    /// The sheet's lead and the tasks it keeps, while its team's `state` is still proposed; the edits go with it.
    pub(crate) fn start(&mut self, state: Option<&str>) -> Option<(String, Vec<TaskPick>)> {
        let lead = self.sheet.clone().filter(|_| state == Some(PROPOSED))?;
        let picks = self.edits.get(&lead).map(Edits::picks).filter(|p| !p.is_empty())?;
        self.edits.remove(&lead);
        Some((lead, picks))
    }
}

pub(crate) struct TeamUi {
    pub(crate) island: Island,
    /// Where a new allowlist prefix is typed.
    pub(crate) allow: Entity<InputState>,
}

impl TeamUi {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let allow = cx.new(|cx| InputState::new(window, cx).placeholder("npm test"));
        let typed = cx.subscribe_in(&allow, window, |this, _, ev: &InputEvent, _, cx| {
            if let InputEvent::PressEnter { .. } | InputEvent::Blur = ev {
                this.team_commit_allow(cx);
            }
        });
        let desktop = cx.entity().downgrade();
        // Ahead of the keymap, as `confirm_on_enter`: the chat input behind the sheet keeps focus, and its ⌘⏎ would queue its draft.
        let start = cx.intercept_keystrokes(move |ev, window, cx| {
            let k = &ev.keystroke;
            if k.key == "enter" && k.modifiers.secondary() && desktop.update(cx, |d, cx| d.team_start(window, cx)).unwrap_or(false) {
                cx.stop_propagation();
            }
        });
        (Self { island: Island::default(), allow }, vec![typed, start])
    }
}

impl Desktop {
    /// A team update from pocketd: its sheet opens when its plan just came in and you're on its lead.
    pub(crate) fn team_updated(&mut self, team: &Team, window: &mut Window, cx: &mut Context<Self>) {
        if self.team.island.saw(team) && self.session() == Some(team.lead.as_str()) {
            self.open_team_sheet(team, window, cx);
        }
    }

    pub(crate) fn open_team_sheet(&mut self, team: &Team, window: &mut Window, cx: &mut Context<Self>) {
        let settings = &self.store.agents;
        self.team.island.show_sheet(team, |agent| settings.provider(agent).model);
        self.open(Overlay::Team, window, cx);
    }

    /// The capsule's Review: the lead's sheet again, edits and all.
    pub(crate) fn team_review(&mut self, lead: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(team) = self.agents.team(lead).cloned() {
            self.open_team_sheet(&team, window, cx);
        }
    }

    /// Starts the sheet's team with the tasks kept; false when there's no sheet to start.
    pub(crate) fn team_start(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.overlay != Some(Overlay::Team) {
            return false;
        }
        let state = self.team.island.sheet.as_deref().and_then(|l| self.agents.team(l)).map(|t| t.state.clone());
        let Some((lead, picks)) = self.team.island.start(state.as_deref()) else { return false };
        self.outbox.team_start(&lead, &picks);
        self.close_overlay(window, cx);
        true
    }

    pub(crate) fn team_remove(&mut self, task: &str, cx: &mut Context<Self>) {
        if let Some(e) = self.team.island.sheet_edits() {
            e.remove(task);
        }
        cx.notify();
    }

    pub(crate) fn team_toggle_menu(&mut self, task: &str, cx: &mut Context<Self>) {
        let menu = &mut self.team.island.menu;
        *menu = (menu.as_deref() != Some(task)).then(|| task.to_string());
        self.load_models(cx);
        cx.notify();
    }

    pub(crate) fn team_pick(&mut self, task: &str, agent: &str, model: &str, cx: &mut Context<Self>) {
        if let Some(e) = self.team.island.sheet_edits() {
            e.pick(task, agent, model);
        }
        self.team.island.menu = None;
        cx.notify();
    }

    pub(crate) fn team_begin_allow(&mut self, task: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.team.island.adding = Some(task.to_string());
        self.team.allow.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    /// Enter, or leaving the input, adds what was typed; Esc has already cleared `adding` through `close_menus`.
    fn team_commit_allow(&mut self, cx: &mut Context<Self>) {
        let Some(task) = self.team.island.adding.take() else { return };
        let typed = self.team.allow.read(cx).value().to_string();
        if let Some(e) = self.team.island.sheet_edits() {
            e.allow(&task, &typed);
        }
        cx.notify();
    }

    pub(crate) fn team_disallow(&mut self, task: &str, prefix: &str, cx: &mut Context<Self>) {
        if let Some(e) = self.team.island.sheet_edits() {
            e.disallow(task, prefix);
        }
        cx.notify();
    }

    /// The branch the lead's checkout is on: workers start from it and merges land in it.
    pub(crate) fn lead_branch(&self, lead: &str) -> String {
        self.agents.get(lead).and_then(|a| self.worktree_of(&a.cwd)).map(|w| w.branch.clone()).filter(|b| !b.is_empty()).unwrap_or_else(|| "the lead's branch".into())
    }
}
```

Wiring:

1. `desktop/chrome.rs`: add a variant at the end of `enum Overlay`:

```rust
    /// A lead's team plan, to edit and start.
    Team,
```

2. `modals.rs`:
   - in `may_open`, add the arm `Overlay::Team => new_session::may_start(agents),`
   - in `open`, change `Overlay::More | Overlay::Confirm | Overlay::PhoneAccess => {}` to `Overlay::More | Overlay::Confirm | Overlay::PhoneAccess | Overlay::Team => {}`
   - in `overlay_view`'s match, add `Overlay::Team => (self.team_sheet(cx), 0x29, Some((240, 8.))),`

3. `desktop.rs`:
   - `use crate::team::TeamUi;`
   - add the field `pub(crate) team: TeamUi,` after `pub(crate) chats: ChatState,`
   - in `Desktop::new`, after `let (chats, chats_subs) = ChatState::new(window, cx);`, add `let (team, team_subs) = TeamUi::new(window, cx);`; after `_subs.extend(chats_subs);` add `_subs.extend(team_subs);`; and add `team,` after `chats,` in the struct literal
   - at the end of the boolean chain in `menu_open`, add `|| self.team.island.menu.is_some() || self.team.island.adding.is_some()`
   - in `close_menus`, before `open`, add `(self.team.island.menu, self.team.island.adding) = (None, None);`

4. `desktop/alerts.rs`, at the very top of `on_agents_from`, before the `PairCode` check. It doesn't return, so plan 5's own handling of the event still runs:

```rust
        if let Event::Team(team) = &ev {
            self.team_updated(team, window, cx);
        }
        if let Event::Teams(teams) = &ev {
            teams.iter().for_each(|t| _ = self.team.island.saw(t));
        }
```

`team.rs` declares `capsule` and `sheet`. Create empty files `team/capsule.rs` and `team/sheet.rs` now; Tasks 1.3 and 1.4 fill them. Until Task 1.3 adds `team_sheet`, the `overlay_view` arm doesn't compile. Do Task 1.3 before running a build.

**Step 4: Run** `cargo test -p pocket team::tests` after Task 1.3 is in.
Expected: PASS, `3 passed`.

### Task 1.3: `team/sheet.rs`: the plan sheet

**Files:**
- Create: `packages/desktop/crates/pocket/src/team/sheet.rs`
- Modify: `packages/desktop/crates/pocket/src/composer/model_menu.rs:24` (`fn model_name` → `pub(crate) fn model_name`, so the sheet reuses the composer's model names)

**Context:**
- The design is the prototype's `#sheet`: a 580px pop. The header has an h2 at 17px and a 13px paragraph. Each row is a 20px number badge, the title (13.5 semibold), a sub line (branch, "N files in scope", "after N"), the agent chip and ×. The footer has a note, **Not now** and **Start N agents ⌘↵**.
- Each row also gets a third line, **Runs without asking**: mono pills with ×, plus **Allow**, which turns into an inline input. The prototype has no allowlist editing; this line adds it.
- The model menu lists Claude Code's and Codex's models, using the same rows as the composer (`launch::model_choices`). Its first row "Default · X" stands for X, the model Settings starts that agent with. The chip follows the composer's pattern: it toggles in `capture_any_mouse_down` with `stop_propagation`, and the menu closes on `on_mouse_down_out`.
- No unit test here: this is render code. Its decisions are tested in Tasks 1.1–1.2.

**Step 1: Implement** `team/sheet.rs`:

```rust
//! The plan sheet: the lead's tasks as you'll start them.

use super::plan::{Kept, branch, can_start};
use super::{PROPOSED, plural};
use crate::composer::launch::model_choices;
use crate::composer::model_menu::model_name;
use crate::composer::{menu_title, picker, pill};
use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use gpui_kit::component::input::Input;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::Variant;

const WIDTH: f32 = 580.;
/// The agents a task can run as.
const AGENTS: [&str; 2] = ["claude", "codex"];

impl Desktop {
    /// Each task with its agent and model, what it runs without asking, and ×; then Not now and Start.
    pub(crate) fn team_sheet(&self, cx: &mut Context<Self>) -> Div {
        let Some(lead) = self.team.island.sheet.clone() else { return div() };
        let Some(edits) = self.team.island.edits.get(&lead) else { return div() };
        let proposed = self.agents.team(&lead).is_some_and(|t| t.state == PROPOSED);
        let n = edits.tasks().len();
        let head = div()
            .pt(px(20.))
            .px(px(22.))
            .pb(px(14.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(div().text_size(px(17.)).line_height(px(22.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(format!("Start a team of {}?", plural(n, "agent", "agents"))))
            .child(div().text_size(px(13.)).line_height(px(19.)).text_color(TEXT_2).child(format!(
                "Each one works in its own worktree from {} and hands its result back to this chat. Anything they ask comes to you, unless it's on their allowlist.",
                self.lead_branch(&lead)
            )));
        let rows: Vec<AnyElement> = if n == 0 {
            vec![div().px(px(22.)).py(px(14.)).text_size(px(12.5)).text_color(TEXT_3).child("No tasks left. Tell the lead what to plan instead.").into_any_element()]
        } else {
            edits.tasks().iter().enumerate().map(|(i, t)| self.sheet_row(&lead, i, t, edits.after_label(t), edits.removable(&t.id), cx).into_any_element()).collect()
        };
        let startable = can_start(proposed, n);
        let start = ui::button("team-start", Variant::Primary, None, format!("Start {}", plural(n, "agent", "agents")))
            .child(ui::button_kbd("⌘↵"))
            .when(startable, |b| b.on_click(cx.listener(|this, _: &ClickEvent, window, cx| _ = this.team_start(window, cx))))
            .when(!startable, |b| b.opacity(0.4));
        let later = ui::button("team-later", Variant::Ghost, None, "Not now").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let note = if proposed { "Up to 4 run at once." } else { "This plan has already started." };
        let foot = div().pt(px(14.)).pb(px(16.)).pl(px(22.)).pr(px(16.)).flex().items_center().gap(px(8.)).child(div().flex_1().text_size(px(12.5)).text_color(TEXT_3).child(note)).child(later).child(start);
        div().absolute().inset_0().flex().items_center().justify_center().child(
            ui::pop(div().id("team-sheet"))
                .w(px(WIDTH))
                .rounded(px(R_DIALOG))
                .occlude()
                .flex()
                .flex_col()
                .child(head)
                .child(div().border_t(px(0.5)).border_b(px(0.5)).border_color(HAIRLINE).flex().flex_col().children(rows))
                .child(foot),
        )
    }

    fn sheet_row(&self, lead: &str, i: usize, t: &Kept, after: Option<String>, removable: bool, cx: &mut Context<Self>) -> Div {
        let num = div()
            .size(px(20.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .bg(FILL_3)
            .text_size(px(11.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(TEXT_2)
            .child((i + 1).to_string());
        let sub = div()
            .mt(px(3.))
            .flex()
            .items_center()
            .gap(px(8.))
            .overflow_hidden()
            .whitespace_nowrap()
            .text_size(px(12.))
            .text_color(TEXT_3)
            .child(div().flex().items_center().gap(px(4.)).font_family(MONO).text_size(px(11.5)).child(icon("branch", 11., TEXT_3)).child(branch(lead, &t.id)))
            .child(format!("{} in scope", plural(t.files.len(), "file", "files")))
            .children(after);
        let main = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(div().truncate().text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(t.title.clone()))
            .child(sub)
            .child(self.allow_row(t, cx));
        let task = t.id.clone();
        let remove = ui::icon_button(id(format!("team-remove:{}", t.id)), "x")
            .when(removable, |b| b.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.team_remove(&task, cx))))
            .when(!removable, |b| b.opacity(0.3).tooltip(|window, cx| Tooltip::new("Another task waits for this one").build(window, cx)));
        div()
            .pl(px(22.))
            .pr(px(12.))
            .py(px(10.))
            .flex()
            .items_start()
            .gap(px(12.))
            .when(i > 0, |d| d.border_t(px(0.5)).border_color(HAIRLINE))
            .child(num)
            .child(main)
            .child(self.agent_chip(t, cx))
            .child(remove)
    }

    /// The task's agent and model; its menu picks both.
    fn agent_chip(&self, t: &Kept, cx: &mut Context<Self>) -> Div {
        let open = self.team.island.menu.as_deref() == Some(t.id.as_str());
        let label = div().whitespace_nowrap().font_weight(FontWeight::MEDIUM).text_color(TEXT).child(model_name(&t.model, self.catalog_models()));
        let task = t.id.clone();
        let chip = picker(pill().child(provider_icon(&t.agent, 14., TEXT_2)).child(label), id(format!("team-agent:{}", t.id)), open).capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            cx.stop_propagation();
            this.team_toggle_menu(&task, cx);
        }));
        let menu = open.then(|| ui::dropdown_right(34., ui::menu_in("team-model-menu-in", self.model_menu(t, cx))));
        div().relative().flex_none().child(chip).children(menu)
    }

    /// Each agent's models, as the composer lists them; "Default" is the one Settings starts that agent with.
    fn model_menu(&self, t: &Kept, cx: &mut Context<Self>) -> Stateful<Div> {
        let enabled = self.store.agents.enabled();
        let mut rows: Vec<AnyElement> = Vec::new();
        for agent in AGENTS.into_iter().filter(|a| enabled.contains(a)) {
            let settings = self.store.agents.provider(agent).model;
            let picked = if t.agent == agent { t.model.as_str() } else { "" };
            rows.push(menu_title(self.agent_name(agent)).into_any_element());
            for (i, (value, label)) in model_choices(agent, self.catalog_models(), picked, &settings).into_iter().enumerate() {
                let chosen = t.agent == agent && if value.is_empty() { t.model.is_empty() } else { t.model == value };
                let model = if value.is_empty() { settings.clone() } else { value };
                let task = t.id.clone();
                let row = ui::menu_pick(id(format!("team-model:{}:{agent}:{i}", t.id)), provider_icon(agent, 14., TEXT_2), &label, chosen)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.team_pick(&task, agent, &model, cx)));
                rows.push(row.into_any_element());
            }
        }
        let task = t.id.clone();
        ui::pop(div().id("team-model-menu"))
            .w(px(260.))
            .max_h(px(360.))
            .overflow_y_scroll()
            .p(px(5.))
            .flex()
            .flex_col()
            .gap(px(2.))
            // Menus hang over the sheet; without this a click on one also lands on what it covers.
            .occlude()
            .children(rows)
            .on_mouse_down_out(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                if this.team.island.menu.as_deref() == Some(task.as_str()) {
                    this.team.island.menu = None;
                    cx.notify();
                }
            }))
    }

    /// What the task runs without asking: each prefix with its ×, then Allow, or the input while adding.
    fn allow_row(&self, t: &Kept, cx: &mut Context<Self>) -> Div {
        let pills: Vec<AnyElement> = t
            .allow
            .iter()
            .enumerate()
            .map(|(i, prefix)| {
                let (task, p) = (t.id.clone(), prefix.clone());
                let x = div()
                    .id(id(format!("team-disallow:{}:{i}", t.id)))
                    .size(px(16.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .cursor_pointer()
                    .hover(|s| s.bg(FILL_4))
                    .child(icon("x", 9., TEXT_3))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.team_disallow(&task, &p, cx)));
                div().h(px(20.)).pl(px(6.)).pr(px(2.)).flex().flex_none().items_center().gap(px(2.)).rounded(px(5.)).bg(FILL_2).font_family(MONO).text_size(px(11.5)).text_color(TEXT_2).child(prefix.clone()).child(x).into_any_element()
            })
            .collect();
        let add = if self.team.island.adding.as_deref() == Some(t.id.as_str()) {
            div().w(px(160.)).h(px(20.)).px(px(6.)).flex().items_center().rounded(px(5.)).bg(SURFACE).shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)]).font_family(MONO).child(Input::new(&self.team.allow).appearance(false).p_0().text_size(px(11.5))).into_any_element()
        } else {
            let task = t.id.clone();
            div()
                .id(id(format!("team-allow:{}", t.id)))
                .h(px(20.))
                .px(px(6.))
                .flex()
                .items_center()
                .gap(px(3.))
                .rounded(px(5.))
                .cursor_pointer()
                .text_size(px(11.5))
                .text_color(TEXT_3)
                .hover(|s| s.bg(FILL_2).text_color(TEXT))
                .child(icon("plus", 10., TEXT_3))
                .child("Allow")
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.team_begin_allow(&task, window, cx)))
                .into_any_element()
        };
        div().mt(px(6.)).flex().flex_wrap().items_center().gap(px(4.)).text_size(px(12.)).text_color(TEXT_3).child(div().mr(px(2.)).child("Runs without asking")).children(pills).child(add)
    }
}
```

**Step 2: Run** `cargo build --workspace`
Expected: builds (at most dead-code warnings for `capsule`, which Task 1.4 fills).

**Step 3: Run** `cargo test -p pocket team::`
Expected: PASS, `11 passed`.

### Task 1.4: The proposed capsule over the lead's chat

**Files:**
- Modify: `packages/desktop/crates/pocket/src/team/capsule.rs` (fill the empty file)
- Modify: `packages/desktop/crates/pocket/src/chat/composer.rs` (`chat_composer`, ~L613-615)

**Context:**
- The prototype's `.isl-c`: a 38px dark pill with the team icon, "Team plan · N agents" and a **Review** button. It sits 12px above the composer, centred over the chat.
- It hangs from a zero-height div that leads the composer column, the same way the agent map does. It isn't `deferred`. It paints after the transcript because the composer follows the transcript, and below any overlay because overlays are drawn later at the window root. `occlude()` keeps presses from reaching the transcript under it.
- PR 2 replaces this file with the full capsule.

**Step 1: Implement** `team/capsule.rs`:

```rust
//! The island over a lead's chat: a dark capsule, here only its plan's Review.

use super::{PROPOSED, plural};
use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use agents::Summary;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Duration;
use theme::*;

/// Dark in either appearance, as the design's island is. GPUI can't blur what's behind it, so it's nearly opaque.
pub(super) const FILL: Token = Token::fixed(0x16161af2);
pub(super) const INK: Token = Token::fixed(0xffffffeb);
pub(super) const DIM: Token = Token::fixed(0xffffff8c);
const CTA_FILL: Token = Token::fixed(0xffffff24);
const CTA_HOVER: Token = Token::fixed(0xffffff38);
pub(super) const ON_LIGHT: Token = Token::fixed(0x111113ff);
pub(super) const LIGHT_HOVER: Token = Token::fixed(0xe9e9ecff);

pub(super) fn island_shadow() -> Vec<BoxShadow> {
    vec![ui::ring(Token::fixed(0xffffff14), 0.5), ui::shadow(rgba(0x11111347), 12., 32.), ui::shadow(rgba(0x11111329), 2., 8.)]
}

/// The capsule's dark pill.
pub(super) fn pill() -> Div {
    div().h(px(38.)).pl(px(12.)).pr(px(6.)).flex().flex_none().items_center().gap(px(10.)).rounded(px(19.)).bg(FILL).shadow(island_shadow()).text_size(px(12.5)).text_color(INK).whitespace_nowrap().occlude()
}

/// The capsule's button, white when it's what to do next.
pub(super) fn cta(key: String, label: &'static str, light: bool) -> Stateful<Div> {
    div()
        .id(id(key))
        .h(px(26.))
        .px(px(11.))
        .flex()
        .items_center()
        .rounded(px(13.))
        .cursor_pointer()
        .font_weight(FontWeight::SEMIBOLD)
        .when(light, |d| d.bg(WHITE).text_color(ON_LIGHT).hover(|s| s.bg(LIGHT_HOVER)))
        .when(!light, |d| d.bg(CTA_FILL).text_color(WHITE).hover(|s| s.bg(CTA_HOVER)))
        .child(label)
}

/// Hangs `stack`, its first element lowest, 12px above the top of the composer column it leads, centred over the chat.
pub(super) fn hang(stack: Vec<AnyElement>) -> AnyElement {
    let shown = div().absolute().bottom_full().left_0().right_0().pb(px(12.)).flex().flex_col_reverse().items_center().gap(px(10.)).children(stack);
    div().relative().w_full().h_0().child(shown).into_any_element()
}

/// Fades `e` in as it rises 6px into place.
pub(super) fn rise<E: Styled + IntoElement + 'static>(key: String, e: E, reduce_motion: bool) -> AnyElement {
    if reduce_motion {
        return e.into_any_element();
    }
    e.with_animation(id(key), Animation::new(Duration::from_millis(220)).with_easing(ease_out_quint()), |e, t| e.opacity(t).mb(px(-6. * (1. - t)))).into_any_element()
}

impl Desktop {
    /// Over the lead's chat while its plan waits: "Team plan · N agents", and Review opens the sheet.
    pub(crate) fn team_island(&self, agent: &Summary, cx: &mut Context<Self>) -> Option<AnyElement> {
        let team = self.agents.team(&agent.id).filter(|t| t.state == PROPOSED)?;
        let (lead, review) = (agent.id.clone(), agent.id.clone());
        let label = div().flex().gap(px(4.)).child(div().font_weight(FontWeight::MEDIUM).child("Team plan")).child(div().text_color(DIM).child(format!("· {}", plural(team.tasks.len(), "agent", "agents"))));
        let body = div()
            .id(id(format!("team-capsule:{lead}")))
            .flex()
            .items_center()
            .gap(px(10.))
            .cursor_pointer()
            .child(icon("agent-map", 14., INK))
            .child(label)
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.team_review(&lead, window, cx)));
        let button = cta(format!("team-review:{review}"), "Review", false).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.team_review(&review, window, cx)));
        Some(hang(vec![rise(format!("team-capsule-in:{}", agent.id), pill().child(body).child(button), cx.reduce_motion())]))
    }
}
```

In `chat/composer.rs` `chat_composer`, compute the island just before `let column = …` and make it the column's first child:

```rust
        let island = self.team_island(agent, cx);
        let column = div().w_full().max_w(px(MAX_WIDTH)).flex().flex_col().children(island).children(map).children(goal).children(btw).children(background).children(tray).child(body).child(footer).children(handoff);
```

**Step 2: Run** `cargo build --workspace && cargo clippy --workspace --all-targets`
Expected: builds, and clippy adds no warnings in `team/`.

### Task 1.5: Refused team requests show as a toast

**Files:**
- Modify: `packages/desktop/crates/pocket/src/team.rs` (new `failed`, the `agents` import, a test)
- Modify: `packages/desktop/crates/pocket/src/desktop/alerts.rs` (plan 5's `Event::TeamFailed` block in `on_agents_from`)

**Context:** Plan 5 turns every refused team request into `Event::TeamFailed { lead, action, message }`: `team.start`, `team.stop`, `team.merge`, or a team prompt (`TeamAction::Plan(request)`, refused e.g. `team_busy`), with pocketd's message (`OUTDATED` for an older pocketd). Its handler in `alerts.rs` only fails a refused team prompt's send. `Desktop.error` is the toast the other refusals use (`ConfigFailed`, `AutomationError`, a refused stop); setting it shows the message. The words are a plain function, so they're tested.

**Step 1: Write the failing test.** In `team.rs`'s `mod tests`, add:

```rust
    #[test]
    fn a_refused_team_request_says_what_was_asked_and_why() {
        let toasts = [
            failed(&TeamAction::Plan("prompt-1".into()), "This session's team is still running or waiting to merge"),
            failed(&TeamAction::Start, "Task t3 starts from t1's work; keep t1 or remove t3 too"),
            failed(&TeamAction::Stop, "Task t1 isn't running"),
            failed(&TeamAction::Merge, "This checkout has uncommitted changes; commit or stash them first"),
        ];
        assert_eq!(
            toasts,
            [
                "Couldn't ask for a team plan: This session's team is still running or waiting to merge",
                "Couldn't start the team: Task t3 starts from t1's work; keep t1 or remove t3 too",
                "Couldn't stop the task: Task t1 isn't running",
                "Couldn't merge the team: This checkout has uncommitted changes; commit or stash them first",
            ]
        );
    }
```

What it proves: each toast names the action that was refused, then pocketd's reason as it sent it.

**Step 2: Run** `cargo test -p pocket team::tests`
Expected: FAIL to compile with `cannot find function \`failed\`` and `use of undeclared type \`TeamAction\``.

**Step 3: Implement.** In `team.rs`, the import becomes `use agents::{TaskPick, Team, TeamAction};`, and after `plural` add:

```rust
/// The toast for a team request pocketd refused: what was asked, then pocketd's reason.
pub(crate) fn failed(action: &TeamAction, message: &str) -> String {
    let asked = match action {
        TeamAction::Plan(_) => "ask for a team plan",
        TeamAction::Start => "start the team",
        TeamAction::Stop => "stop the task",
        TeamAction::Merge => "merge the team",
    };
    format!("Couldn't {asked}: {message}")
}
```

In `desktop/alerts.rs`, plan 5's block becomes:

```rust
        if let Event::TeamFailed { lead, action, message } = &ev {
            // A refused team prompt fails its send, as a refused prompt does.
            if let (TeamAction::Plan(request), Some(v)) = (action, self.chats.views.get_mut(lead)) {
                v.session.outgoing.fail(request);
            }
            self.error = Some(crate::team::failed(action, message));
            return cx.notify();
        }
```

**Step 4: Run** `cargo test -p pocket team:: && cargo build --workspace`
Expected: PASS, `12 passed` (`team::tests` has 4), and the workspace builds.

### Task 1.6: Captures for the sheet and capsule

**Files:**
- Modify: `packages/desktop/crates/pocket/src/capture.rs` (`STEPS`, `reset`, a `show_team` helper)

**Context:** The fixture (`.ui-review/fixture/fixture.ts`) is a fake pocketd that knows nothing of teams. As `chat-sub-live` does, these steps apply agent events in-process through `Desktop::on_agents`, so `fixture.ts` doesn't change. The lead is the chat fixture's agent (`parser`), focused by the `chat` step. `reset` must forget teams between screens.

**Step 1:** The "before" screen was captured in Task 1.0, Step 0.

**Step 2: Implement.** In `capture.rs`, add this helper next to `fn session`:

```rust
/// The plan of the team captures, proposed by the shown chat's agent, each task in `states`' state; `edit` adjusts the rest.
fn show_team(d: &mut Desktop, state: &str, states: [&str; 4], edit: impl FnOnce(&mut agents::Team), window: &mut Window, cx: &mut Context<Desktop>) {
    let Some(lead) = d.session().map(str::to_string) else { return };
    let plan: [(&str, &str, &str, &[&str], &[&str], &str); 4] = [
        ("t1", "Keep sends in a pocketd queue", "claude", &["internal/daemon/queue.go", "internal/daemon/stream.go"], &[], "go test ./internal/daemon"),
        ("t2", "Deliver queued sends when a turn ends", "codex", &["internal/daemon/claude.go", "internal/daemon/codex_chat.go"], &["t1"], "go test ./internal/daemon"),
        ("t3", "Show pocketd's queue in the composer", "claude", &["crates/chat/src/outgoing.rs", "crates/pocket/src/chat/composer/queue.rs"], &[], "cargo test -p chat"),
        ("t4", "e2e: a queued send survives a restart", "codex", &["e2e/queue_test.go"], &[], "go test ./e2e -run TestQueue"),
    ];
    let strings = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let tasks = plan
        .into_iter()
        .zip(states)
        .map(|((id, title, agent, files, after, allow), state)| agents::TeamTask {
            id: id.into(),
            title: title.into(),
            agent: agent.into(),
            files: strings(files),
            after: strings(after),
            allow: vec![allow.into()],
            state: state.into(),
            worker: if state == "planned" { String::new() } else { format!("w-{id}") },
            stat: if matches!(state, "done" | "merged") { " 2 files changed, 64 insertions(+), 9 deletions(-)".into() } else { String::new() },
            ..Default::default()
        })
        .collect();
    let mut team = agents::Team { lead, state: state.into(), tasks, updated_at: crate::util::now_ms(), ..Default::default() };
    edit(&mut team);
    d.on_agents(agents::Event::Team(team), window, cx);
}
```

Add these entries to `STEPS` after `("chat-agent-map", …)`, and raise the array length in `const STEPS: [(&str, Step); N]` by 3. If plans 1–5 added steps, `rustc` names the right size in `expected an array with a size of …`.

```rust
    ("team-proposed", |d, window, cx| show_team(d, "proposed", ["planned"; 4], |_| {}, window, cx)),
    ("team-edited", |d, _, cx| {
        if let Some(e) = d.team.island.sheet_edits() {
            e.remove("t4");
            e.pick("t1", "codex", "");
            e.allow("t3", "cargo clippy -p chat");
        }
        d.team_toggle_menu("t2", cx);
    }),
    ("team-later", |d, window, cx| d.close_overlay(window, cx)),
```

At the end of `fn reset`, add:

```rust
    d.team.island = crate::team::Island::default();
    d.agents.apply(agents::Event::Teams(Vec::new()));
```

**Step 3: Capture "after"** (from the repo root):

```bash
.ui-review/fixture/capture.sh /tmp/team-6/pr1 chat-lead=chat chat-team-sheet=chat,team-proposed chat-team-edited=chat,team-proposed,team-edited chat-team-capsule=chat,team-proposed,team-later
```

Expected: four PNG paths printed under `/tmp/team-6/pr1/`. Open them and compare `impl-chat-lead.png` with the "before" one (identical: no team, no capsule). `chat-team-sheet`: the sheet centred over a dim backdrop with 4 rows; t1 has "after" absent, t2 "after 1", each with a "Runs without asking" line. `chat-team-edited`: 3 rows, t1 on Codex "Agent default", t3 allows two prefixes, t2's model menu open. `chat-team-capsule`: no sheet, and a dark "Team plan · 4 agents  Review" capsule above the composer. (The lead's Needs you comes from pocketd, which the fixture doesn't run, so its sidebar card doesn't show it here.)

**Step 4: Run the PR's checks** (from `packages/desktop`): `cargo build --workspace`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`
Expected: all green; clippy adds no warnings.

---

## PR 2: The running team: rings, the Needs-you alert, the task list

**Scope:** `team/progress.rs` (marks, ring fills, counts, labels, the capsule's text, the stat summary) with tests, `team/ring.rs`, `team/list.rs`, and the full capsule. While a team runs, the capsule shows a ring per task and "N working · M of K done". It opens a list where a row opens its worker's chat and **Stop** stops it. When a worker needs you, an expanded alert takes the capsule's place: **Open** jumps to the worker, and **Deny / Allow once** answer a plain permission ask. Review/merge states come in PR 3; until then the capsule shows nothing for them.
**Depends on:** PR 1.
**Done when:** `cargo test -p pocket team::` green; captures show the running capsule, the alert, and the list.

### Task 2.1: `team/progress.rs`: what each task and the capsule say

**Files:**
- Create: `packages/desktop/crates/pocket/src/team/progress.rs`
- Modify: `packages/desktop/crates/pocket/src/team.rs` (add `pub(crate) mod progress;`)

**Context:** The texts are the prototype's `counts`, `summary` and `nowText`, cut to what `TeamTask.state` holds. pocketd doesn't report times or steps, so there's no clock and no last step. `stat` is `git diff --stat` against the task's base, and its last line sums it up.

**Step 1: Write the failing tests** (the whole file for now):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, state: &str, after: &[&str]) -> TeamTask {
        TeamTask { id: id.into(), state: state.into(), after: after.iter().map(|a| a.to_string()).collect(), ..Default::default() }
    }

    fn team(state: &str, tasks: Vec<TeamTask>) -> Team {
        Team { lead: "lead".into(), state: state.into(), tasks, updated_at: 1_000, ..Default::default() }
    }

    #[test]
    fn each_task_state_has_its_mark_and_ring() {
        let states = ["planned", "waiting", "running", "needsYou", "done", "failed", "stopped", "merged"];
        let shown = states.map(|s| (mark(s), fill(mark(s))));
        assert_eq!(
            shown,
            [(Mark::Waiting, 0.), (Mark::Waiting, 0.), (Mark::Running, 0.25), (Mark::NeedsYou, 1.), (Mark::Done, 1.), (Mark::Failed, 1.), (Mark::Stopped, 1.), (Mark::Merged, 1.)]
        );
    }

    #[test]
    fn the_list_header_counts_what_each_task_is_doing() {
        let tasks = vec![task("t1", "done", &[]), task("t2", "waiting", &["t1"]), task("t3", "needsYou", &[]), task("t4", "running", &[])];
        assert_eq!(summary(&counts(&tasks), 4), "1 needs you · 1 working · 1 waiting · 1 of 4 done");
    }

    #[test]
    fn a_waiting_task_says_which_task_it_starts_after_until_that_one_hands_back() {
        let waiting = |first: &str| vec![task("t1", first, &[]), task("t2", "waiting", &["t1"])];
        let (running, done) = (waiting("running"), waiting("done"));
        assert_eq!([now_text(&running[1], &running), now_text(&done[1], &done)], ["Starts after 1", "Waits for a free slot"]);
    }

    #[test]
    fn a_proposed_team_s_capsule_offers_its_plan() {
        let t = team("proposed", vec![task("t1", "planned", &[]), task("t2", "planned", &[])]);
        assert_eq!(capsule(&t), Some(Capsule { label: "Team plan".into(), detail: "· 2 agents".into(), rings: false, click: Click::Sheet, cta: Some(Cta::Review), warn: false }));
    }

    #[test]
    fn a_running_team_s_capsule_counts_its_workers() {
        let t = team("running", vec![task("t1", "done", &[]), task("t2", "running", &[]), task("t3", "failed", &[])]);
        assert_eq!(capsule(&t), Some(Capsule { label: "1 working".into(), detail: "· 1 of 3 done · 1 failed".into(), rings: true, click: Click::List, cta: None, warn: true }));
    }

    #[test]
    fn a_team_still_planning_shows_no_capsule() {
        assert_eq!(capsule(&team("planning", Vec::new())), None);
    }

    #[test]
    fn the_alert_is_for_the_first_task_that_needs_you() {
        let t = team("running", vec![task("t1", "running", &[]), task("t2", "needsYou", &[]), task("t3", "needsYou", &[])]);
        assert_eq!(needing(&t).map(|t| t.id.as_str()), Some("t2"));
    }

    #[test]
    fn a_stat_sums_up_on_its_last_line() {
        let stats = [" a.rs | 3 ++-\n 2 files changed, 64 insertions(+), 9 deletions(-)", " 1 file changed, 1 insertion(+)", ""];
        assert_eq!(stats.map(stat_counts), [Some((2, 64, 9)), Some((1, 1, 0)), None]);
    }
}
```

**Step 2: Run** `cargo test -p pocket team::progress`
Expected: FAIL to compile (`cannot find type \`Mark\``, …).

**Step 3: Implement** (above the tests):

```rust
//! What the capsule, its alert and its list say of a team: plain data from `Team`, drawn elsewhere.

use super::plan::after_label;
use super::{PROPOSED, plural};
use agents::{Team, TeamTask};

/// How a task's state shows, as a ring on the capsule and a glyph in the list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Mark {
    Waiting,
    Running,
    NeedsYou,
    Done,
    Failed,
    Stopped,
    Merged,
}

pub(crate) fn mark(state: &str) -> Mark {
    match state {
        "running" => Mark::Running,
        "needsYou" => Mark::NeedsYou,
        "done" => Mark::Done,
        "failed" => Mark::Failed,
        "stopped" => Mark::Stopped,
        "merged" => Mark::Merged,
        _ => Mark::Waiting,
    }
}

/// How much of a task's ring is drawn. A task reports no progress until it hands back, so a running one is a quarter arc that turns.
pub(crate) fn fill(m: Mark) -> f32 {
    match m {
        Mark::Waiting => 0.,
        Mark::Running => 0.25,
        _ => 1.,
    }
}

#[derive(Debug, Default, PartialEq)]
pub(crate) struct Counts {
    pub(crate) working: usize,
    pub(crate) needs: usize,
    pub(crate) waiting: usize,
    pub(crate) done: usize,
    pub(crate) failed: usize,
    pub(crate) stopped: usize,
}

pub(crate) fn counts(tasks: &[TeamTask]) -> Counts {
    let mut c = Counts::default();
    for t in tasks {
        match mark(&t.state) {
            Mark::Running => c.working += 1,
            Mark::NeedsYou => c.needs += 1,
            Mark::Waiting => c.waiting += 1,
            Mark::Done | Mark::Merged => c.done += 1,
            Mark::Failed => c.failed += 1,
            Mark::Stopped => c.stopped += 1,
        }
    }
    c
}

/// "1 needs you · 2 working · 1 of 4 done": the list's header.
pub(crate) fn summary(c: &Counts, n: usize) -> String {
    let parts = [(c.needs, "needs you"), (c.working, "working"), (c.waiting, "waiting"), (c.stopped, "stopped"), (c.failed, "failed")];
    parts.iter().filter(|(k, _)| *k > 0).map(|(k, w)| format!("{k} {w}")).chain([format!("{} of {n} done", c.done)]).collect::<Vec<_>>().join(" · ")
}

/// What a task is doing, for its row in the list.
pub(crate) fn now_text(t: &TeamTask, tasks: &[TeamTask]) -> String {
    match t.state.as_str() {
        "planned" | "waiting" => {
            let unfinished: Vec<String> = t.after.iter().filter(|a| tasks.iter().any(|x| &x.id == *a && mark(&x.state) != Mark::Done)).cloned().collect();
            let ids: Vec<&str> = tasks.iter().map(|t| t.id.as_str()).collect();
            after_label(&unfinished, &ids).map_or_else(|| "Waits for a free slot".into(), |a| format!("Starts {a}"))
        }
        "running" => "Working".into(),
        "needsYou" => "Needs you".into(),
        "done" => "Handed back".into(),
        "failed" => "Failed".into(),
        "stopped" => "Stopped".into(),
        "merged" => "Merged".into(),
        other => other.into(),
    }
}

/// The first task waiting on you; the alert is about it.
pub(crate) fn needing(team: &Team) -> Option<&TeamTask> {
    team.tasks.iter().find(|t| t.state == "needsYou")
}

/// What a press on the capsule opens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Click {
    Sheet,
    List,
}

/// The capsule's button.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Cta {
    Review,
}

#[derive(Debug, PartialEq)]
pub(crate) struct Capsule {
    pub(crate) label: String,
    pub(crate) detail: String,
    /// A ring per task leads the label.
    pub(crate) rings: bool,
    pub(crate) click: Click,
    pub(crate) cta: Option<Cta>,
    /// Amber: something failed or isn't ready.
    pub(crate) warn: bool,
}

/// The capsule over the lead's chat; none while the lead is still planning.
pub(crate) fn capsule(team: &Team) -> Option<Capsule> {
    let (c, n) = (counts(&team.tasks), team.tasks.len());
    let at = |label: String, detail: String| Capsule { label, detail, rings: true, click: Click::List, cta: None, warn: false };
    Some(match team.state.as_str() {
        PROPOSED => Capsule { rings: false, click: Click::Sheet, cta: Some(Cta::Review), ..at("Team plan".into(), format!("· {}", plural(n, "agent", "agents"))) },
        "running" => {
            let label = if c.working > 0 {
                format!("{} working", c.working)
            } else if c.waiting > 0 {
                format!("{} waiting", c.waiting)
            } else {
                "None working".into()
            };
            let failed = if c.failed > 0 { format!(" · {} failed", c.failed) } else { String::new() };
            Capsule { warn: c.failed > 0, ..at(label, format!("· {} of {n} done{failed}", c.done)) }
        }
        _ => return None,
    })
}

/// The (files, added, removed) a `git diff --stat` sums up on its last line; none without that line.
pub(crate) fn stat_counts(stat: &str) -> Option<(usize, usize, usize)> {
    let last = stat.lines().rev().find(|l| l.contains("changed"))?;
    let mut out = (0, 0, 0);
    for part in last.split(',') {
        let mut words = part.split_whitespace();
        let (Some(n), Some(word)) = (words.next().and_then(|n| n.parse().ok()), words.next()) else { continue };
        match word {
            w if w.starts_with("file") => out.0 = n,
            w if w.starts_with("insertion") => out.1 = n,
            w if w.starts_with("deletion") => out.2 = n,
            _ => {}
        }
    }
    Some(out)
}
```

**Step 4: Run** `cargo test -p pocket team::progress`
Expected: PASS, `8 passed`.

### Task 2.2: `team/ring.rs`: a task's ring

**Files:**
- Create: `packages/desktop/crates/pocket/src/team/ring.rs`
- Modify: `packages/desktop/crates/pocket/src/team.rs` (add `mod ring;`)

**Context:**
- The prototype's `.ring`: 16px, radius 6, stroke 2, drawn on the dark capsule. The colours are fixed, since the capsule is dark in both appearances.
- An SVG arc between two points is ambiguous at a half turn or more, so arcs are drawn in pieces of at most half a turn.
- A waiting ring's track is dashed (`PathBuilder::dash_array`, as `stroke-dasharray: 2 2.4`).
- The running ring's animation is capped at 20 fps, like `theme::gradient_spinner`, because every frame redraws the whole window. It only runs while a task runs and the lead's chat is on screen.
- No unit test: this is paint code. Its inputs (`mark`, `fill`) are tested in 2.1.

**Step 1: Implement:**

```rust
//! A task's ring on the capsule.

use super::progress::{Mark, fill};
use gpui_kit::*;
use std::f32::consts::TAU;
use std::time::Duration;
use theme::*;

const SIZE: f32 = 16.;
const RADIUS: f32 = 6.;
const STROKE: f32 = 2.;

/// (track, arc) on the dark capsule.
fn colors(m: Mark) -> (Hsla, Hsla) {
    let white = Hsla::from(WHITE);
    let tint = |c: u32| {
        let h = Hsla::from(Token::fixed(c));
        (h.opacity(0.35), h)
    };
    match m {
        Mark::NeedsYou => tint(0xffb224ff),
        Mark::Done => tint(0x3dd68cff),
        Mark::Merged => tint(0xa371f7ff),
        Mark::Failed => tint(0xff9592ff),
        Mark::Stopped => (white.opacity(0.18), white.opacity(0.35)),
        Mark::Waiting | Mark::Running => (white.opacity(0.18), white),
    }
}

/// The point `turn` of the way round, clockwise from the top.
fn on_circle(c: Point<Pixels>, turn: f32) -> Point<Pixels> {
    let a = turn * TAU;
    point(c.x + px(RADIUS * a.sin()), c.y - px(RADIUS * a.cos()))
}

/// Adds the arc from `from` to `to` turns in pieces of at most half a turn: an arc between the same two points is ambiguous past that.
fn arc(path: &mut PathBuilder, c: Point<Pixels>, from: f32, to: f32) {
    path.move_to(on_circle(c, from));
    let mut at = from;
    while to - at > 1e-4 {
        let next = (at + 0.5).min(to);
        path.arc_to(point(px(RADIUS), px(RADIUS)), px(0.), false, true, on_circle(c, next));
        at = next;
    }
}

fn paint(m: Mark, phase: f32, bounds: Bounds<Pixels>, window: &mut Window) {
    let c = bounds.center();
    let (track, value) = colors(m);
    let mut ring = PathBuilder::stroke(px(STROKE));
    if m == Mark::Waiting {
        ring = ring.dash_array(&[px(2.), px(2.4)]);
    }
    arc(&mut ring, c, 0., 1.);
    if let Ok(p) = ring.build() {
        window.paint_path(p, track);
    }
    if fill(m) > 0. {
        let mut v = PathBuilder::stroke(px(STROKE));
        arc(&mut v, c, phase, phase + fill(m));
        if let Ok(p) = v.build() {
            window.paint_path(p, value);
        }
    }
}

/// A task's ring. A running one turns at 20fps, sharing frames with the sidebar's spinners, and holds still under Reduce motion.
pub(super) fn ring(key: ElementId, m: Mark, reduce_motion: bool) -> AnyElement {
    let face = move |phase: f32| canvas(|_, _, _| (), move |bounds, _, window, _| paint(m, phase, bounds, window)).size(px(SIZE));
    if m != Mark::Running || reduce_motion {
        return div().size(px(SIZE)).flex_none().child(face(0.)).into_any_element();
    }
    div().size(px(SIZE)).flex_none().with_animation(key, Animation::new(Duration::from_secs(1)).repeat_synced().with_max_fps(20.), move |d, t| d.child(face(t))).into_any_element()
}
```

**Step 2: Run** `cargo build --workspace`
Expected: builds (`ring` unused until Task 2.3).

### Task 2.3: The full capsule and the Needs-you alert

**Files:**
- Modify: `packages/desktop/crates/pocket/src/team/capsule.rs` (replace the whole file)
- Modify: `packages/desktop/crates/pocket/src/team.rs` (glue: `team_toggle_list`, `team_open_worker`, `team_stop`, `team_answer`; `Island.list`)
- Modify: `packages/desktop/crates/pocket/src/desktop.rs` (`menu_open`/`close_menus` include `island.list`)

**Context:**
- **Alert:** the prototype's `.isl-x`, 400px wide with radius 22. It has rings, amber **Needs you** and "· task title", then "<Agent> wants to run", the command in a mono well, and the buttons.
- The ask comes from `self.agents.pending`, matched by the task's `worker`. `Permission::parts()` gives ("Wants to run", "cargo test -p chat").
- **Open** calls `focus_agent(worker)`. **Deny/Allow once** call `outbox.resolve` the same way the chat's permission card does. They're only offered for a permission with no questions, and not when observe-only.
- The capsule and its CTA act on **press** (`capture_any_mouse_down` + `stop_propagation`), not click. The list's `on_mouse_down_out` runs in paint order. The capsule is the stack's first child, so it paints before the list (`flex_col_reverse` shows it lowest). Its press stops the list's press-outside, which would otherwise shut the list the capsule just toggled. The agent map relies on the same rule.

**Step 1: Implement.** Replace `team/capsule.rs` with the PR 1 file with these changes. Keep the consts, `island_shadow`, `pill`, `cta`, `hang` and `rise` as they were. Change the imports and replace `impl Desktop`. Add a `WARN` const and `dark_button`:

```rust
//! The island over a lead's chat: the capsule with a ring per task, the alert when a worker needs you, and the list it opens.

use super::progress::{self, Click, Cta};
use super::ring::ring;
use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use agents::{Decision, Summary, TeamTask};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Duration;
use theme::*;

// … FILL, INK, DIM, CTA_FILL, CTA_HOVER, ON_LIGHT, LIGHT_HOVER, island_shadow, pill, cta, hang, rise unchanged …

/// Amber on the dark capsule.
const WARN: Token = WAITING_DOT;

/// The alert's buttons.
fn dark_button(key: String, label: &'static str, light: bool) -> Stateful<Div> {
    div()
        .id(id(key))
        .h(px(26.))
        .px(px(10.))
        .flex()
        .items_center()
        .rounded(px(7.))
        .cursor_pointer()
        .text_size(px(12.5))
        .font_weight(FontWeight::MEDIUM)
        .when(light, |d| d.bg(WHITE).text_color(ON_LIGHT).hover(|s| s.bg(LIGHT_HOVER)))
        .when(!light, |d| d.text_color(Token::fixed(0xffffffbf)).hover(|s| s.bg(Token::fixed(0xffffff1a)).text_color(WHITE)))
        .child(label)
}

impl Desktop {
    /// Over the lead's chat: the alert for a worker that needs you, else the capsule and the list it opens.
    pub(crate) fn team_island(&self, agent: &Summary, cx: &mut Context<Self>) -> Option<AnyElement> {
        let team = self.agents.team(&agent.id)?;
        let lead = agent.id.clone();
        let reduce = cx.reduce_motion();
        let rings = |key: &str| div().flex().gap(px(4.)).children(team.tasks.iter().map(|t| ring(id(format!("team-ring:{key}:{lead}:{}", t.id)), progress::mark(&t.state), reduce)));
        if let Some(task) = progress::needing(team) {
            return Some(hang(vec![rise(format!("team-alert-in:{lead}:{}", task.id), self.needs_alert(task, rings("alert"), cx), reduce)]));
        }
        let c = progress::capsule(team)?;
        let label = div()
            .flex()
            .gap(px(4.))
            .child(div().font_weight(FontWeight::MEDIUM).text_color(if c.warn { WARN } else { INK }).child(c.label))
            .when(!c.detail.is_empty(), |d| d.child(div().text_color(DIM).child(c.detail)));
        let (click, at) = (c.click, lead.clone());
        let body = div()
            .id(id(format!("team-capsule:{lead}")))
            .flex()
            .items_center()
            .gap(px(10.))
            .cursor_pointer()
            .children(c.rings.then(|| rings("capsule")))
            .children((!c.rings).then(|| icon("agent-map", 14., INK)))
            .child(label)
            // A press, not a click: it runs before the list's press-outside, which would shut the list this toggles.
            .capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                match click {
                    Click::Sheet => this.team_review(&at, window, cx),
                    Click::List => this.team_toggle_list(&at, cx),
                }
            }));
        let action = c.cta.map(|cta| self.capsule_cta(&lead, cta, cx));
        let capsule = pill().when(action.is_none(), |d| d.pr(px(14.))).child(body).children(action);
        let list = (self.team.island.list.as_deref() == Some(lead.as_str())).then(|| ui::menu_in(id(format!("team-list-in:{lead}")), self.team_list(team, cx)).into_any_element());
        Some(hang(std::iter::once(rise(format!("team-capsule-in:{lead}"), capsule, reduce)).chain(list).collect()))
    }

    fn capsule_cta(&self, lead: &str, c: Cta, cx: &mut Context<Self>) -> Stateful<Div> {
        let lead = lead.to_string();
        let (label, light) = match c {
            Cta::Review => ("Review", false),
        };
        cta(format!("team-cta:{lead}"), label, light).capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            match c {
                Cta::Review => this.team_review(&lead, window, cx),
            }
        }))
    }

    /// The capsule grown into what the worker asks; Open goes to its chat.
    fn needs_alert(&self, task: &TeamTask, rings: Div, cx: &mut Context<Self>) -> Stateful<Div> {
        let ask = self.agents.pending.iter().find(|p| p.agent_id == task.worker);
        let who = self.agent_name(&task.agent);
        let (title, command) = match ask {
            Some(p) => {
                let (verb, what) = p.parts();
                (format!("{who} {}", verb.to_lowercase()), Some(what))
            }
            None => (format!("{who} is waiting on you"), None),
        };
        let worker = task.worker.clone();
        let open = dark_button(format!("team-open:{}", task.id), "Open", false).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.team_open_worker(&worker, window, cx)));
        let answers = ask.filter(|p| p.questions().is_empty() && !self.agents.observe_only()).map(|p| {
            let (deny, allow) = (p.request_id.clone(), p.request_id.clone());
            [
                dark_button(format!("team-deny:{deny}"), "Deny", false).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.team_answer(&deny, Decision::Deny, cx))),
                dark_button(format!("team-allow:{allow}"), "Allow once", true).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.team_answer(&allow, Decision::Allow, cx))),
            ]
        });
        let head = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.5))
            .text_color(DIM)
            .child(rings)
            .child(div().flex_none().font_weight(FontWeight::SEMIBOLD).text_color(WARN).child("Needs you"))
            .child(div().min_w_0().truncate().child(format!("· {}", task.title)));
        let well = command.map(|c| div().mt(px(8.)).px(px(10.)).py(px(7.)).rounded(px(8.)).bg(Token::fixed(0xffffff17)).font_family(MONO).text_size(px(12.5)).child(c));
        div()
            .id(id(format!("team-alert:{}", task.id)))
            .w(px(400.))
            .pt(px(14.))
            .pb(px(12.))
            .pl(px(16.))
            .pr(px(14.))
            .rounded(px(22.))
            .bg(FILL)
            .shadow(island_shadow())
            .occlude()
            .text_color(INK)
            .child(head)
            .child(div().mt(px(8.)).text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).child(title))
            .children(well)
            .child(div().mt(px(12.)).flex().items_center().gap(px(6.)).child(open).child(div().flex_1()).children(answers.into_iter().flatten()))
    }
}
```

In `team.rs`:
- Add the field `pub(crate) list: Option<String>,` to `Island`, with the doc `/// The lead whose task list is open over its capsule.`
- Add `pub(crate) mod progress;`, `mod ring;` and `mod list;` (`list.rs` comes in 2.4; create it empty now).
- Add `use agents::Decision;` and these glue methods to `impl Desktop`:

```rust
    pub(crate) fn team_toggle_list(&mut self, lead: &str, cx: &mut Context<Self>) {
        let list = &mut self.team.island.list;
        *list = (list.as_deref() != Some(lead)).then(|| lead.to_string());
        cx.notify();
    }

    pub(crate) fn team_open_worker(&mut self, worker: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.team.island.list = None;
        self.focus_agent(worker, window, cx);
        cx.notify();
    }

    pub(crate) fn team_stop(&mut self, lead: &str, task: &str, cx: &mut Context<Self>) {
        self.outbox.team_stop(lead, task);
        cx.notify();
    }

    pub(crate) fn team_answer(&mut self, request: &str, decision: Decision, cx: &mut Context<Self>) {
        self.outbox.resolve(request, decision);
        cx.notify();
    }
```

In `desktop.rs`: in `menu_open` add `|| self.team.island.list.is_some()`. In `close_menus` extend the PR 1 line to `(self.team.island.menu, self.team.island.adding, self.team.island.list) = (None, None, None);`.

**Step 2:** Build after Task 2.4. `team_list` comes from there.

### Task 2.4: `team/list.rs`: the task popover

**Files:**
- Create: `packages/desktop/crates/pocket/src/team/list.rs` (fill the empty file)

**Context:**
- The prototype's `#ilist`: 540px wide. The header has the team icon, **Team** and the counts summary, plus ×. Each 40px row has a glyph, the title (max 220px), what it's doing (mono), the agent and model, and its +/− once handed back.
- This version also adds **Stop** on live rows and opens the worker's chat on a row press.
- Glyphs use `ui::glyph(ui::tone(State))` like the sidebar does, because this is a light surface. Merged uses the `merge` icon in `MERGED`, and stopped uses `stop`.

**Step 1: Implement:**

```rust
//! The team's tasks, over its capsule: what each is doing; a press opens its worker's chat, Stop stops it.

use super::progress::{self, Mark};
use crate::composer::model_menu::model_name;
use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use agents::{Team, TeamTask};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{State, Variant};

fn mark_glyph(key: ElementId, m: Mark) -> AnyElement {
    let state = match m {
        Mark::Running => State::Working,
        Mark::NeedsYou => State::NeedsYou,
        Mark::Done => State::Done(0, 0),
        Mark::Failed => State::Failed,
        Mark::Merged => return icon("merge", 13., MERGED).into_any_element(),
        Mark::Stopped => return icon("stop", 12., TEXT_4).into_any_element(),
        Mark::Waiting => return div().size(px(11.)).flex_none().rounded_full().border_1().border_color(TEXT_5).into_any_element(),
    };
    ui::tone(state).map_or_else(|| div().into_any_element(), |t| ui::glyph(key, t))
}

impl Desktop {
    pub(super) fn team_list(&self, team: &Team, cx: &mut Context<Self>) -> Stateful<Div> {
        let lead = team.lead.clone();
        let close = ui::icon_button(id(format!("team-list-close:{lead}")), "x").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.team.island.list = None;
            cx.notify();
        }));
        let head = div()
            .h(px(42.))
            .pl(px(16.))
            .pr(px(10.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .border_b(px(0.5))
            .border_color(HAIRLINE)
            .text_size(px(13.))
            .child(icon("agent-map", 14., TEXT_2))
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Team"))
            .child(div().min_w_0().truncate().text_size(px(12.5)).text_color(TEXT_3).child(progress::summary(&progress::counts(&team.tasks), team.tasks.len())))
            .child(div().flex_1())
            .child(close);
        let rows: Vec<AnyElement> = team.tasks.iter().enumerate().map(|(i, t)| self.list_row(team, i, t, cx).into_any_element()).collect();
        ui::pop(div().id(id(format!("team-list:{lead}"))))
            .w(px(540.))
            .flex()
            .flex_col()
            .overflow_hidden()
            .occlude()
            .child(head)
            .children(rows)
            .on_mouse_down_out(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                if this.team.island.list.as_deref() == Some(lead.as_str()) {
                    this.team.island.list = None;
                    cx.notify();
                }
            }))
    }

    fn list_row(&self, team: &Team, i: usize, t: &TeamTask, cx: &mut Context<Self>) -> Stateful<Div> {
        let m = progress::mark(&t.state);
        let live = matches!(m, Mark::Waiting | Mark::Running | Mark::NeedsYou);
        let stat = progress::stat_counts(&t.stat).filter(|_| matches!(m, Mark::Done | Mark::Merged));
        let stop = live.then(|| {
            let (lead, task) = (team.lead.clone(), t.id.clone());
            ui::button(id(format!("team-stop:{}", t.id)), Variant::Ghost, None, "Stop").h(px(24.)).px(px(8.)).text_size(px(12.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.team_stop(&lead, &task, cx);
            }))
        });
        let worker = t.worker.clone();
        let who = div().flex_none().flex().items_center().gap(px(5.)).text_size(px(12.)).text_color(TEXT_3).child(provider_icon(&t.agent, 12., TEXT_3)).child(model_name(&t.model, self.catalog_models()));
        div()
            .id(id(format!("team-row:{}", t.id)))
            .h(px(40.))
            .px(px(16.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(10.))
            .text_size(px(13.))
            .when(i > 0, |d| d.border_t(px(0.5)).border_color(HAIRLINE))
            .when(!worker.is_empty(), |d| d.cursor_pointer().hover(|s| s.bg(FILL_2)).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.team_open_worker(&worker, window, cx))))
            .child(div().w(px(14.)).flex().flex_none().justify_center().child(mark_glyph(id(format!("team-mark:{}", t.id)), m)))
            .child(div().flex_none().max_w(px(220.)).truncate().font_weight(FontWeight::MEDIUM).child(t.title.clone()))
            .child(div().flex_1().min_w_0().truncate().font_family(MONO).text_size(px(12.)).text_color(TEXT_3).child(progress::now_text(t, &team.tasks)))
            .child(who)
            .child(div().w(px(64.)).flex().flex_none().justify_end().children(stat.map(|(_, added, removed)| ui::diffstat(added, removed))))
            .children(stop)
    }
}
```

**Step 2: Run** `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test -p pocket team::`
Expected: builds, no new clippy warnings, `20 passed`.

### Task 2.5: Captures for the running team

**Files:**
- Modify: `packages/desktop/crates/pocket/src/capture.rs` (3 steps; `STEPS` length +3)

**Step 1: Implement.** Add after the PR 1 team steps:

```rust
    ("team-running", |d, window, cx| show_team(d, "running", ["done", "running", "running", "waiting"], |_| {}, window, cx)),
    ("team-needs", |d, window, cx| {
        show_team(d, "running", ["done", "running", "needsYou", "waiting"], |_| {}, window, cx);
        let detail = agents::Detail { kind: "shell".into(), command: "cargo test -p chat".into(), ..Default::default() };
        let ask = agents::Permission { request_id: "ask-w-t3".into(), agent_id: "w-t3".into(), tool_name: "Bash".into(), detail, feedback: false };
        d.on_agents(agents::Event::Asked(ask), window, cx);
    }),
    ("team-list", |d, _, cx| {
        if let Some(lead) = d.session().map(str::to_string) {
            d.team_toggle_list(&lead, cx);
        }
    }),
```

**Step 2: Capture** (from the repo root):

```bash
.ui-review/fixture/capture.sh /tmp/team-6/pr2 chat-team-running=chat,team-running chat-team-needs=chat,team-needs chat-team-list=chat,team-running,team-list
```

Expected: three PNG paths printed under `/tmp/team-6/pr2/`.
- `chat-team-running`: four rings (green full, two white quarter arcs, one dashed) with "2 working · 1 of 4 done".
- `chat-team-needs`: the 400px alert with "Needs you · Show pocketd's queue in the composer", "Claude Code wants to run", `cargo test -p chat` in a mono well, and Open / Deny / Allow once.
- `chat-team-list`: the 540px list above the capsule, header "2 working · 1 waiting · 1 of 4 done". The rows show Handed back +64 −9, Working, Working, and "Waits for a free slot". Live rows have Stop.

**Step 3: Run the PR's checks:** `cargo build --workspace`, `cargo clippy --workspace --all-targets`, `cargo test --workspace` → green.

---

## PR 3: Review and merge

**Scope:**
- The capsule says what the review, merging, merged, conflict and failed states mean. In review its **Merge** button is white when the lead said "Ready to merge".
- The list ends with the lead's verdict: a warning when the team isn't ready, the conflict files, and Merge with the diffstat totals.
- Merge calls `team_merge` when the team is ready. Otherwise it asks first (`Confirm::MergeTeam`).
- After a merge the capsule leaves 4s past the team's `updatedAt`. No timer is needed: the 1s daemon poll redraws.

**Depends on:** PR 2.
**Done when:** tests green; captures show review (not ready and ready), the merge confirm, and a conflict.

### Task 3.1: `progress`: review states, verdict, totals

**Files:**
- Modify: `packages/desktop/crates/pocket/src/team/progress.rs`
- Modify: `packages/desktop/crates/pocket/src/team/capsule.rs` (the one `progress::capsule(team)` call)

**Step 1: Write the failing tests.** Add to `progress.rs` tests:

```rust
    #[test]
    fn a_reviewed_team_s_capsule_says_whether_the_lead_passed_it() {
        let done = || vec![task("t1", "done", &[]), task("t2", "done", &[])];
        let ready = Team { ready: true, ..team("review", done()) };
        let merge = |ready| Some(Cta::Merge { ready });
        let shown = [capsule(&ready, false, "main", 0), capsule(&team("review", done()), true, "main", 0), capsule(&team("review", done()), false, "main", 0)];
        assert_eq!(
            shown,
            [
                Some(Capsule { label: "Ready to merge".into(), detail: "· 2 handed back".into(), rings: true, click: Click::List, cta: merge(true), warn: false }),
                Some(Capsule { label: "The lead is reviewing…".into(), detail: String::new(), rings: true, click: Click::List, cta: merge(false), warn: false }),
                Some(Capsule { label: "2 handed back".into(), detail: "· not marked ready".into(), rings: true, click: Click::List, cta: merge(false), warn: true }),
            ]
        );
    }

    #[test]
    fn a_merge_s_capsule_stays_a_few_seconds_and_a_conflict_s_until_it_s_dealt_with() {
        let merged = team("merged", vec![task("t1", "merged", &[])]);
        let conflict = Team { conflict: vec!["a.rs".into(), "b.rs".into()], ..team("conflict", vec![task("t1", "merged", &[]), task("t2", "done", &[])]) };
        let label = |c: Option<Capsule>| c.map(|c| (c.label, c.detail, c.warn));
        assert_eq!(
            [label(capsule(&merged, false, "main", 2_000)), label(capsule(&merged, false, "main", 9_000)), label(capsule(&conflict, false, "main", 9_000))],
            [Some(("Merged into main".to_string(), "· 1 branch".to_string(), false)), None, Some(("Merge stopped".to_string(), "· conflict in 2 files".to_string(), true))]
        );
    }

    #[test]
    fn the_list_ends_with_the_lead_s_verdict() {
        let done = || vec![task("t1", "done", &[])];
        let conflict = Team { conflict: vec!["a.rs".into()], ..team("conflict", done()) };
        let shown = [verdict(&Team { ready: true, ..team("review", done()) }, false, "main"), verdict(&team("review", done()), false, "main"), verdict(&conflict, false, "main"), verdict(&team("running", done()), false, "main")];
        let v = |text: &str, warn, files: &[&str], merge| Some(Verdict { text: text.into(), warn, files: files.iter().map(|f| f.to_string()).collect(), merge });
        assert_eq!(
            shown,
            [
                v("The lead says it's ready to merge.", false, &[], Some(true)),
                v("The lead hasn't said it's ready to merge. Read its review before you merge.", true, &[], Some(false)),
                v("The merge conflicted in these files and was undone; nothing after it was merged. The lead was told.", true, &["a.rs"], None),
                None,
            ]
        );
    }

    #[test]
    fn totals_sum_the_handed_back_stats() {
        let stat = |s: &str| TeamTask { stat: s.into(), ..task("t", "done", &[]) };
        assert_eq!(totals(&[stat(" 2 files changed, 64 insertions(+), 9 deletions(-)"), stat(" 1 file changed, 3 insertions(+)"), task("t3", "running", &[])]), Some((67, 9)));
        assert_eq!(totals(&[task("t1", "running", &[])]), None);
    }
```

Change the two PR 2 capsule tests and the planning test to call `capsule(&t, false, "main", 0)`.

**Step 2: Run** `cargo test -p pocket team::progress` → FAIL to compile (wrong `capsule` arity, `no variant \`Merge\``, `cannot find function \`verdict\``).

**Step 3: Implement** in `progress.rs`:

- Add the `Merge` variant: `pub(crate) enum Cta { Review, Merge { ready: bool } }`.
- Add `/// How long a merged team's capsule stays, in ms. const MERGED_FOR: i64 = 4_000;`
- Replace `capsule` with:

```rust
/// The capsule over the lead's chat; none while the lead plans, nor once a merge is a few seconds old. `into` is the lead's branch.
pub(crate) fn capsule(team: &Team, lead_working: bool, into: &str, now: i64) -> Option<Capsule> {
    let (c, n) = (counts(&team.tasks), team.tasks.len());
    let at = |label: String, detail: String| Capsule { label, detail, rings: true, click: Click::List, cta: None, warn: false };
    Some(match team.state.as_str() {
        PROPOSED => Capsule { rings: false, click: Click::Sheet, cta: Some(Cta::Review), ..at("Team plan".into(), format!("· {}", plural(n, "agent", "agents"))) },
        "running" => {
            let label = if c.working > 0 {
                format!("{} working", c.working)
            } else if c.waiting > 0 {
                format!("{} waiting", c.waiting)
            } else {
                "None working".into()
            };
            let failed = if c.failed > 0 { format!(" · {} failed", c.failed) } else { String::new() };
            Capsule { warn: c.failed > 0, ..at(label, format!("· {} of {n} done{failed}", c.done)) }
        }
        "review" => {
            let merge = Some(Cta::Merge { ready: team.ready });
            if team.ready {
                Capsule { cta: merge, ..at("Ready to merge".into(), format!("· {n} handed back")) }
            } else if lead_working {
                Capsule { cta: merge, ..at("The lead is reviewing…".into(), String::new()) }
            } else {
                Capsule { cta: merge, warn: true, ..at(format!("{n} handed back"), "· not marked ready".into()) }
            }
        }
        "merging" => at(format!("Merging into {into}…"), String::new()),
        "merged" if now - team.updated_at < MERGED_FOR => at(format!("Merged into {into}"), format!("· {}", plural(n, "branch", "branches"))),
        "conflict" => Capsule { warn: true, ..at("Merge stopped".into(), format!("· conflict in {}", plural(team.conflict.len(), "file", "files"))) },
        "failed" => Capsule { warn: true, ..at("Team failed".into(), String::new()) },
        _ => return None,
    })
}

/// The list's last line once every task has handed back.
#[derive(Debug, PartialEq)]
pub(crate) struct Verdict {
    pub(crate) text: String,
    pub(crate) warn: bool,
    /// Where a merge conflicted.
    pub(crate) files: Vec<String>,
    /// Merge is offered; true when the lead said it's ready.
    pub(crate) merge: Option<bool>,
}

/// The lead's verdict and Merge, the merge underway or done, or why it stopped; none before every task hands back.
pub(crate) fn verdict(team: &Team, lead_working: bool, into: &str) -> Option<Verdict> {
    let v = |text: String, warn: bool| Verdict { text, warn, files: Vec::new(), merge: None };
    Some(match team.state.as_str() {
        "review" if team.ready => Verdict { merge: Some(true), ..v("The lead says it's ready to merge.".into(), false) },
        "review" if lead_working => Verdict { merge: Some(false), ..v("The lead is reviewing the handbacks…".into(), false) },
        "review" => Verdict { merge: Some(false), ..v("The lead hasn't said it's ready to merge. Read its review before you merge.".into(), true) },
        "merging" => v(format!("Merging into {into}…"), false),
        "merged" => v(format!("Merged into {into}. Worktrees removed, branches kept."), false),
        "conflict" => Verdict { files: team.conflict.clone(), ..v("The merge conflicted in these files and was undone; nothing after it was merged. The lead was told.".into(), true) },
        "failed" => v(if team.error.is_empty() { "The team stopped.".into() } else { team.error.clone() }, true),
        _ => return None,
    })
}

/// The lines the handed-back tasks added and removed, from their stats.
pub(crate) fn totals(tasks: &[TeamTask]) -> Option<(usize, usize)> {
    let stats: Vec<(usize, usize, usize)> = tasks.iter().filter_map(|t| stat_counts(&t.stat)).collect();
    (!stats.is_empty()).then(|| stats.iter().fold((0, 0), |(a, r), s| (a + s.1, r + s.2)))
}
```

In `capsule.rs` `team_island`, replace `let c = progress::capsule(team)?;` with:

```rust
        let (working, into) = (agent.status == "working", self.lead_branch(&lead));
        let c = progress::capsule(team, working, &into, crate::util::now_ms())?;
```

**Step 4: Run** `cargo test -p pocket team::progress` → PASS, `12 passed`. It won't build until Task 3.3 handles `Cta::Merge` in `capsule_cta`, so do 3.2 and 3.3 before running it.

### Task 3.2: Merge, and the confirm when the lead hasn't passed it

**Files:**
- Modify: `packages/desktop/crates/pocket/src/desktop/chrome.rs` (`Confirm::MergeTeam(String)`)
- Modify: `packages/desktop/crates/pocket/src/modals/confirm.rs` (`ConfirmText::merge_team`, `confirm_view`, `confirmed`, a test)
- Modify: `packages/desktop/crates/pocket/src/team.rs` (`team_merge`)

**Context:** Index decision: Merge works either way but warns when the team isn't ready, because the spike's merge was clean and tests passed while the work was wrong. The confirm uses the existing sheet (`ui::alert`), not danger red: merging is undoable by git.

**Step 1: Write the failing test** in `confirm.rs` tests:

```rust
    #[test]
    fn merging_a_team_the_lead_hasn_t_passed_says_so() {
        let facts = vec!["The lead hasn't said \"Ready to merge\"".to_string(), "Merges 4 branches into main".to_string()];
        assert_eq!(ConfirmText::merge_team(4, "main"), ConfirmText { title: "Merge without the lead's go-ahead?".into(), action: "Merge anyway", facts, dirty: 0, lost: 0, danger: false });
    }
```

**Step 2: Run** `cargo test -p pocket confirm` → FAIL (`no function or associated item named \`merge_team\``).

**Step 3: Implement.**
- `chrome.rs`, `enum Confirm`: add `MergeTeam(String),` (the lead's id).
- `confirm.rs`, in `impl ConfirmText`, after `sign_out_account`:

```rust
    fn merge_team(branches: usize, into: &str) -> Self {
        let facts = vec!["The lead hasn't said \"Ready to merge\"".into(), format!("Merges {} into {into}", crate::team::plural(branches, "branch", "branches"))];
        Self { title: "Merge without the lead's go-ahead?".into(), action: "Merge anyway", facts, dirty: 0, lost: 0, danger: false }
    }
```

- `confirm_view`'s match, before `None => return div(),`:
  `Some(Confirm::MergeTeam(lead)) => ConfirmText::merge_team(self.agents.team(lead).map_or(0, |t| t.tasks.len()), &self.lead_branch(lead)),`
- `confirmed`'s match, before `None => {}`: `Some(Confirm::MergeTeam(lead)) => self.outbox.team_merge(&lead),`
- `team.rs` `impl Desktop`, with `use crate::desktop::chrome::Confirm;` added to the imports:

```rust
    /// Merges the lead's team; when the lead hasn't said it's ready, only once you confirm.
    pub(crate) fn team_merge(&mut self, lead: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ready) = self.agents.team(lead).map(|t| t.ready) else { return };
        self.team.island.list = None;
        if ready {
            self.outbox.team_merge(lead);
            return cx.notify();
        }
        self.confirm = Some(Confirm::MergeTeam(lead.to_string()));
        self.open(Overlay::Confirm, window, cx);
    }
```

**Step 4:** Run after 3.3: `cargo test -p pocket confirm` → PASS.

### Task 3.3: Merge in the capsule and the verdict in the list

**Files:**
- Modify: `packages/desktop/crates/pocket/src/team/capsule.rs` (`capsule_cta`, the `team_list` call)
- Modify: `packages/desktop/crates/pocket/src/team/list.rs` (`team_list` takes the verdict inputs; `list_verdict`)

**Step 1: Implement.**

`capsule.rs` `capsule_cta`, with both matches covering Merge:

```rust
        let (label, light) = match c {
            Cta::Review => ("Review", false),
            Cta::Merge { ready } => ("Merge", ready),
        };
        cta(format!("team-cta:{lead}"), label, light).capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            match c {
                Cta::Review => this.team_review(&lead, window, cx),
                Cta::Merge { .. } => this.team_merge(&lead, window, cx),
            }
        }))
```

and in `team_island` change the list line to `self.team_list(team, working, &into, cx)`.

`list.rs`: add `use super::progress::Verdict;` and `use crate::team::plural;`. Change the signature to `pub(super) fn team_list(&self, team: &Team, lead_working: bool, into: &str, cx: &mut Context<Self>) -> Stateful<Div>`. Then, after `.children(rows)`, add:

```rust
            .children(progress::verdict(team, lead_working, into).map(|v| self.list_verdict(team, v, cx)))
```

and add this method to `impl Desktop` in `list.rs`:

```rust
    /// The lead's verdict, the files a merge conflicted in, the lines changed, and Merge.
    fn list_verdict(&self, team: &Team, v: Verdict, cx: &mut Context<Self>) -> Div {
        let merge = v.merge.map(|ready| {
            let lead = team.lead.clone();
            let variant = if ready { Variant::Primary } else { Variant::Secondary };
            ui::button(id(format!("team-merge:{lead}")), variant, Some("merge"), format!("Merge {}", plural(team.tasks.len(), "branch", "branches")))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.team_merge(&lead, window, cx)))
        });
        let text = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(12.5))
            .text_color(if v.warn { WAITING_TEXT } else { TEXT_BODY })
            .when(v.warn, |d| d.child(icon("warning", 13., WAITING)))
            .child(div().min_w_0().child(v.text));
        let files = (!v.files.is_empty()).then(|| div().mt(px(6.)).flex().flex_col().gap(px(2.)).font_family(MONO).text_size(px(12.)).text_color(TEXT_BODY).children(v.files.into_iter().map(|f| div().truncate().child(f))));
        let totals = v.merge.and_then(|_| progress::totals(&team.tasks)).map(|(added, removed)| ui::diffstat(added, removed));
        div()
            .px(px(16.))
            .py(px(12.))
            .border_t(px(0.5))
            .border_color(HAIRLINE)
            .flex()
            .items_center()
            .gap(px(10.))
            .child(div().flex_1().min_w_0().flex().flex_col().child(text).children(files))
            .children(totals)
            .children(merge)
    }
```

**Step 2: Run** `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test -p pocket team:: && cargo test -p pocket confirm`
Expected: builds, no new warnings, all pass (`24 passed` for `team::`).

### Task 3.4: Captures for review and merge, and the final checks

**Files:**
- Modify: `packages/desktop/crates/pocket/src/capture.rs` (4 steps; `STEPS` length +4)

**Step 1: Implement:**

```rust
    ("team-review", |d, window, cx| show_team(d, "review", ["done"; 4], |_| {}, window, cx)),
    ("team-ready", |d, window, cx| show_team(d, "review", ["done"; 4], |t| t.ready = true, window, cx)),
    ("team-merge", |d, window, cx| {
        if let Some(lead) = d.session().map(str::to_string) {
            d.team_merge(&lead, window, cx);
        }
    }),
    ("team-conflict", |d, window, cx| {
        show_team(d, "conflict", ["merged", "done", "done", "done"], |t| t.conflict = vec!["internal/daemon/stream.go".into(), "crates/chat/src/outgoing.rs".into()], window, cx)
    }),
```

**Step 2: Capture** (from the repo root):

```bash
.ui-review/fixture/capture.sh /tmp/team-6/pr3 chat-team-review=chat,team-review,team-list chat-team-ready=chat,team-ready,team-list chat-team-merge=chat,team-review,team-merge chat-team-conflict=chat,team-conflict,team-list
```

Expected: four PNG paths under `/tmp/team-6/pr3/`.
- `chat-team-review`: amber "4 handed back · not marked ready" with a grey Merge, and the list ending in the amber warning, +256 −36 and a Secondary "Merge 4 branches".
- `chat-team-ready`: "Ready to merge · 4 handed back" with a white Merge, and the list's primary Merge.
- `chat-team-merge`: the "Merge without the lead's go-ahead?" alert with Merge anyway / Cancel.
- `chat-team-conflict`: amber "Merge stopped · conflict in 2 files", and the list showing both files in mono.

**Step 3: Final checks** (from `packages/desktop`):

```bash
cargo build --workspace
cargo clippy --workspace --all-targets
cargo test --workspace
```

Expected: all green; clippy reports nothing in `crates/pocket/src/team*`, `capture.rs`, `confirm.rs`, `modals.rs`, `desktop*.rs`. Compare `/tmp/team-6/before/impl-chat-lead.png` with `/tmp/team-6/pr1/impl-chat-lead.png`: unchanged.

---

## Contract changes needed

All folded into the index (`docs/plans/2026-11-05-team-mode.md`): `Agents::team(&self, lead)`, the derives, `Event::Teams` replacing a pocketd's teams, host tagging of `lead`, `worker` and `worktree` and `Outboxes` routing by the lead, `updatedAt` in Unix ms, Start enabled with one kept task (`plan::can_start`), refused team requests as a toast (`team::failed` over plan 5's `Event::TeamFailed`), and Needs you while proposed coming from pocketd (plan 3), with no desktop override.

## Open questions

- Should `review` with `ready` also make the lead read Needs you, as the prototype's `leadState` does? pocketd only does `proposed` (plan 3's `Teams.awaits`); adding review is one more state there, not a desktop override.
- **Stop has no Resume.** There's no `team.resume` verb, so a stopped task stays stopped. The prototype offers Resume in the worker view.
- **The rings show no real progress.** pocketd reports no per-task progress, so a running ring spins a quarter arc. Should `TeamTask` carry steps or a start time so the ring can fill, like the prototype's `t/len`?
- **Allowlist prefixes aren't validated.** A prefix containing `;`, `|`, `&&` etc. can never match, because pocketd only auto-allows single commands. Should the sheet refuse such prefixes, or say why they won't match?
- **"Scroll to bottom" overlaps the capsule.** The chat's pill (`chat/jump.rs`, 6px above the composer) sits where the capsule does. When both show, the capsule covers it. One fix is to lift the jump pill while `self.agents.team(lead)` has a capsule.
- **The capsule only shows over the lead's chat.** A worker's chat has plan 1's breadcrumb but no capsule. Is that enough?

Resolved by the UX decisions: Start works with one kept task (`can_start`, Task 1.1); refused team requests show as a toast (Task 1.5); the lead's Needs you while proposed comes from pocketd, so the desktop-only sidebar override (`lead_status`) is gone and the sheet still opens by itself.
