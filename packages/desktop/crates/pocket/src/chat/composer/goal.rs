use super::queue::{control, surface};
use crate::desktop::Desktop;
use crate::terminal_view::context::thousands;
use agents::{Goal, GoalStatus, Summary};
use gpui_kit::*;
use theme::*;

fn status_label(status: GoalStatus) -> &'static str {
    match status {
        GoalStatus::Active => "Active",
        GoalStatus::Paused => "Paused",
        GoalStatus::Blocked => "Stuck",
        GoalStatus::UsageLimited => "Usage limit hit",
        GoalStatus::BudgetLimited => "Over budget",
        GoalStatus::Complete => "Met",
        GoalStatus::Failed => "Gave up",
        GoalStatus::Unknown => "",
    }
}

/// The tokens the goal used, against its budget when it has one.
fn spent(goal: &Goal) -> Option<String> {
    match (goal.tokens_used, goal.token_budget) {
        (0, 0) => None,
        (used, 0) => Some(format!("{} tokens", thousands(used))),
        (used, budget) => Some(format!("{} / {} tokens", thousands(used), thousands(budget))),
    }
}

impl Desktop {
    /// The agent's goal over the box, with Pause where the agent can hold it and Clear; none without a goal.
    pub(super) fn goal_strip(&self, agent: &Summary, cx: &mut Context<Self>) -> Option<Div> {
        let goal = agent.goal.as_ref()?;
        // Sent at once rather than queued behind the turn, which a goal can keep going for long.
        let send = |text: &'static str| {
            let agent = agent.id.clone();
            cx.listener(move |this, _: &ClickEvent, _, _| _ = this.outbox.prompt(&agent, text))
        };
        let pause = (chat::for_provider(&agent.provider).pauses_goal() && goal.status == GoalStatus::Active)
            .then(|| control(format!("chat-goal-pause:{}", agent.id), "Pause the goal").child(icon("stop", 12., TEXT_3)).on_click(send("/goal pause")));
        let clear = control(format!("chat-goal-clear:{}", agent.id), "Clear the goal").child(icon("x", 13., TEXT_3)).on_click(send("/goal clear"));
        let row = div()
            .min_h(px(26.))
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.))
            .text_color(TEXT_3)
            .child(icon("sparkle", 13., TEXT_2))
            .child(div().flex_1().min_w_0().truncate().text_size(px(13.)).text_color(TEXT_BODY).child(goal.objective.clone()))
            .children(Some(status_label(goal.status)).filter(|s| !s.is_empty()).map(|s| div().flex_none().text_color(TEXT_2).child(s)))
            .children(spent(goal).map(|s| div().flex_none().child(s)))
            .children(pause)
            .child(clear);
        Some(surface().py(px(6.)).child(row))
    }
}

#[cfg(test)]
mod tests {
    use super::{Goal, GoalStatus, spent, status_label};

    #[test]
    fn a_stuck_codex_goal_reads_as_stuck() {
        assert_eq!(status_label(GoalStatus::Blocked), "Stuck");
        assert_eq!(status_label(GoalStatus::Failed), "Gave up");
    }

    #[test]
    fn a_goal_shows_its_tokens_against_a_budget_only_when_it_has_one() {
        let goal = |used, budget| Goal { tokens_used: used, token_budget: budget, ..Default::default() };
        assert_eq!(spent(&goal(16510, 50000)).as_deref(), Some("16,510 / 50,000 tokens"));
        assert_eq!(spent(&goal(16510, 0)).as_deref(), Some("16,510 tokens"));
        assert_eq!(spent(&goal(0, 0)), None);
    }
}
