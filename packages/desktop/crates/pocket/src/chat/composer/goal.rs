use super::{chip_menu, close_menu};
use crate::chat::shimmer::shimmer;
use crate::composer::model_menu::row;
use crate::composer::{self, Menu};
use crate::desktop::Desktop;
use crate::terminal_view::context::thousands;
use agents::{Goal, GoalStatus, Summary};
use gpui_kit::*;
use theme::*;

/// The tokens the goal used, against its budget when it has one.
fn spent(goal: &Goal) -> Option<String> {
    match (goal.tokens_used, goal.token_budget) {
        (0, 0) => None,
        (used, 0) => Some(format!("{} tokens", thousands(used))),
        (used, budget) => Some(format!("{} / {} tokens", thousands(used), thousands(budget))),
    }
}

/// The pill's words for a goal in `status`; none once it's met, stuck or otherwise over.
fn pill_label(status: GoalStatus) -> Option<&'static str> {
    match status {
        GoalStatus::Active => Some("Goal active"),
        GoalStatus::Paused => Some("Goal paused"),
        _ => None,
    }
}

impl Desktop {
    /// The agent's goal as a pill that opens it; none once `pill_label` has no words for it.
    pub(super) fn chat_goal_pill(&self, agent: &Summary, view_only: bool, cx: &mut Context<Self>) -> Option<Div> {
        let open = self.chats.composer.draft.menu == Some(Menu::Goal);
        let shown = agent.goal.as_ref().filter(|_| !view_only).and_then(|g| Some((g, pill_label(g.status)?)));
        let Some((goal, label)) = shown else {
            // Left open as the goal ends, the menu would pop up by itself over the next goal.
            if open {
                let desktop = cx.entity();
                cx.defer(move |cx| desktop.update(cx, close_menu));
            }
            return None;
        };
        let active = goal.status == GoalStatus::Active;
        let (fill, hover, mark, text) = if active { (GOAL_BG, GOAL_BG_STRONG, GOAL_TEXT, GOAL_TEXT) } else { (FILL_2, FILL_3, TEXT_3, TEXT_2) };
        let words = div().whitespace_nowrap().font_weight(FontWeight::MEDIUM).text_color(text);
        // At work for as long as it's active, like the agent's other live labels.
        let words = if active && !cx.reduce_motion() { words.child(shimmer(SharedString::from(format!("chat-goal-shimmer:{}", agent.id)), label, text)) } else { words.child(label) };
        let chip = composer::pill()
            .id("chat-goal")
            .cursor_pointer()
            .bg(if open { hover } else { fill })
            .hover(move |s| s.bg(hover))
            .child(icon("sparkle", 13., mark))
            .child(words)
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.chats.composer.draft.toggle(Menu::Goal);
                cx.notify();
            }));
        let menu = open.then(|| self.chat_goal_menu(agent, goal, cx));
        // The model pill's own padding and the bar's gap already sit right of the rule, so all its room goes left.
        let divider = div().w(px(1.)).h(px(16.)).ml(px(11.)).bg(SEPARATOR);
        Some(div().flex_none().flex().items_center().child(div().relative().child(chip).children(menu)).child(divider))
    }

    /// The goal's objective and tokens, then Pause or Resume where the agent can hold it, and Clear.
    fn chat_goal_menu(&self, agent: &Summary, goal: &Goal, cx: &mut Context<Self>) -> Div {
        // Sent at once rather than queued behind the turn, which a goal can keep going for long.
        let send = |text: &'static str| {
            let agent = agent.id.clone();
            cx.listener(move |this, _: &ClickEvent, _, cx| {
                _ = this.outbox.prompt(&agent, text, false);
                close_menu(this, cx);
            })
        };
        let objective = div().px(px(10.)).pb(px(6.)).text_size(px(13.)).line_height(px(19.)).text_color(TEXT).child(goal.objective.clone());
        let tokens = spent(goal).map(|s| div().px(px(10.)).pb(px(6.)).text_size(px(12.)).text_color(TEXT_3).child(s));
        let pause_or_resume = chat::for_provider(&agent.provider).pauses_goal().then_some(goal.status).and_then(|status| match status {
            GoalStatus::Active => Some(row("chat-goal-pause", false).child(icon("stop", 12., TEXT_2)).child("Pause the goal").on_click(send("/goal pause"))),
            GoalStatus::Paused => Some(row("chat-goal-resume", false).child(icon("play", 12., TEXT_2)).child("Resume the goal").on_click(send("/goal resume"))),
            _ => None,
        });
        let clear = row("chat-goal-clear", false).child(icon("x", 13., TEXT_2)).child("Clear the goal").on_click(send("/goal clear"));
        let rule = div().h(px(1.)).mx(px(4.)).my(px(3.)).bg(SEPARATOR);
        let rows = [Some(objective.into_any_element()), tokens.map(IntoElement::into_any_element), Some(rule.into_any_element()), pause_or_resume.map(IntoElement::into_any_element), Some(clear.into_any_element())];
        chip_menu("chat-goal-menu", "Goal", 340., rows.into_iter().flatten(), cx)
    }
}

#[cfg(test)]
mod tests {
    use super::{Goal, GoalStatus, pill_label, spent};

    #[test]
    fn a_goal_keeps_its_pill_only_while_active_or_paused() {
        assert_eq!(pill_label(GoalStatus::Active), Some("Goal active"));
        assert_eq!(pill_label(GoalStatus::Paused), Some("Goal paused"));
        assert_eq!(pill_label(GoalStatus::Complete), None);
        assert_eq!(pill_label(GoalStatus::Blocked), None);
    }

    #[test]
    fn a_goal_shows_its_tokens_against_a_budget_only_when_it_has_one() {
        let goal = |used, budget| Goal { tokens_used: used, token_budget: budget, ..Default::default() };
        assert_eq!(spent(&goal(16510, 50000)).as_deref(), Some("16,510 / 50,000 tokens"));
        assert_eq!(spent(&goal(16510, 0)).as_deref(), Some("16,510 tokens"));
        assert_eq!(spent(&goal(0, 0)), None);
    }
}
