//! Team mode on the desktop: the plan sheet a lead's proposal opens, and the island over the lead's chat.

pub(crate) mod capsule;
mod list;
pub(crate) mod plan;
pub(crate) mod progress;
mod ring;
mod sheet;

use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay};
use agents::{Decision, TaskPick, Team, TeamAction};
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
    /// The lead whose task list is open over its capsule.
    pub(crate) list: Option<String>,
}

impl Island {
    /// The model menu, an allowlist being added to, or a task list is open.
    pub(crate) fn menu_open(&self) -> bool {
        self.menu.is_some() || self.adding.is_some() || self.list.is_some()
    }

    pub(crate) fn close_menus(&mut self) {
        (self.menu, self.adding, self.list) = (None, None, None);
    }

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
    /// The open model menu's rows, and where it's scrolled to.
    menu_rows: crate::util::Kept<Vec<sheet::MenuRow>>,
    menu_scroll: UniformListScrollHandle,
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
            if k.key == "enter" && k.modifiers.secondary(cx) && desktop.update(cx, |d, cx| d.team_start(window, cx)).unwrap_or(false) {
                cx.stop_propagation();
            }
        });
        (Self { island: Island::default(), allow, menu_rows: Default::default(), menu_scroll: Default::default() }, vec![typed, start])
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
        self.team.menu_scroll = Default::default();
        self.settings.models.retry();
        for agent in sheet::workers(&self.offered(), &self.agents) {
            self.load_models_of(agent, cx);
        }
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

    /// The branch the lead's checkout is on: workers start from it and merges land in it.
    pub(crate) fn lead_branch(&self, lead: &str) -> String {
        self.agents.get(lead).and_then(|a| self.worktree_of(&a.cwd)).map(|w| w.branch.clone()).filter(|b| !b.is_empty()).unwrap_or_else(|| "the lead's branch".into())
    }
}

#[cfg(test)]
mod tests {
    use super::{Island, failed};
    use agents::{TaskPick, Team, TeamAction, TeamTask};

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
}
