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
