use super::page::Page;
use crate::board::parts::{bar_button, start_kbd};
use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use crate::team::plan::can_start;
use crate::team::{PROPOSED, plural};
use agents::factory::Detail;
use agents::tickets::{Gate, Ticket};
use agents::{Permission, Team};
use git::github::Pr;
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::Variant;

/// What the footer bar asks you to decide.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Decision {
    /// The plan waits for approval. `kept` is how many tasks it has.
    Plan { kept: usize },
}

/// What the bar asks of `t`, if anything. Plans 4 to 6 read `pr`, `ask` and `detail`.
pub(crate) fn decision(t: &Ticket, team: Option<&Team>, _pr: Option<&Pr>, _ask: Option<&Permission>, _detail: Option<&Detail>) -> Option<Decision> {
    match t.gate {
        Gate::Plan => team.filter(|team| team.state == PROPOSED).map(|team| Decision::Plan { kept: team.tasks.len() }),
        _ => None,
    }
}

pub(crate) fn summary(kept: usize) -> String {
    format!("{}, {}", plural(kept, "task", "tasks"), plural(kept, "worker", "workers"))
}

pub(crate) fn approve_label(kept: usize) -> String {
    format!("Approve and start {}", plural(kept, "worker", "workers"))
}

pub(crate) fn change_note(typed: &str) -> Option<&str> {
    Some(typed.trim()).filter(|t| !t.is_empty())
}

impl Desktop {
    /// The page's decision, with the count of the tasks you kept at the gate.
    fn page_decision(&self, t: &Ticket) -> Option<Decision> {
        let team = self.agents.team(&t.agent_id);
        decision(t, team, None, None, self.agents.detail(&t.id)).map(|d| match d {
            Decision::Plan { kept } => Decision::Plan { kept: self.team.island.edits.get(&t.agent_id).map_or(kept, |e| e.tasks().len()) },
        })
    }

    /// Points Team mode's plan edits at the page's lead while its ticket waits at the Plan gate.
    pub(crate) fn aim_plan_edits(&mut self) {
        let Some(t) = self.factory.page.as_ref().and_then(|p| self.agents.board.get(&p.ticket)).filter(|t| t.gate == Gate::Plan) else { return };
        let Some(team) = self.agents.team(&t.agent_id).filter(|team| team.state == PROPOSED).cloned() else { return };
        let island = &mut self.team.island;
        if island.sheet.as_deref() != Some(team.lead.as_str()) || !island.edits.contains_key(&team.lead) {
            let settings = &self.store.agents;
            island.show_sheet(&team, |agent| settings.provider(agent).model);
        }
    }

    pub(crate) fn factory_approve(&mut self, cx: &mut Context<Self>) -> bool {
        self.aim_plan_edits();
        let started = self.start_team();
        cx.notify();
        started
    }

    pub(crate) fn factory_changes(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(page) = self.factory.page.as_mut() else { return };
        page.changes = open;
        self.factory.changes.update(cx, |s, cx| {
            s.set_value("", window, cx);
            if open {
                s.focus(window, cx);
            }
        });
        if !open {
            window.focus(&self.root, cx);
        }
        cx.notify();
    }

    pub(crate) fn factory_send_changes(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let typed = self.factory.changes.read(cx).value().to_string();
        let Some(note) = change_note(&typed) else { return false };
        let Some(lead) = self.factory.page.as_ref().and_then(|p| self.agents.board.get(&p.ticket)).map(|t| t.agent_id.clone()) else { return false };
        self.outbox.prompt(&lead, note, true);
        self.factory_changes(false, window, cx);
        true
    }

    /// ⌘↵ on the page: send the change request while it is open, else approve the plan.
    pub(crate) fn factory_key(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(page) = self.factory.page.clone().filter(|_| self.screen == Screen::Board && self.overlay.is_none()) else { return false };
        let Some(t) = self.agents.board.get(&page.ticket).cloned() else { return false };
        match self.page_decision(&t) {
            Some(Decision::Plan { .. }) if page.changes => self.factory_send_changes(window, cx),
            Some(Decision::Plan { .. }) => self.factory_approve(cx),
            None => false,
        }
    }

    pub(super) fn factory_bar(&self, t: &Ticket, page: &Page, cx: &mut Context<Self>) -> Option<Div> {
        let Decision::Plan { kept } = self.page_decision(t)?;
        let what = div().flex_1().min_w_0().flex().items_center().gap(px(7.)).child(div().size(px(6.)).rounded(px(3.)).bg(WAITING_DOT)).child(div().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child("Plan approval")).child(div().text_color(TEXT_3).child(summary(kept)));
        let content = if page.changes {
            let field = div().flex_1().min_w_0().h(px(28.)).px(px(9.)).flex().items_center().rounded(px(7.)).bg(FILL_2).child(Input::new(&self.factory.changes).appearance(false).p_0().text_size(px(12.5)));
            div()
                .flex_1()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(field)
                .child(bar_button("factory-changes-cancel", Variant::Ghost, None, "Cancel").h(px(28.)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.factory_changes(false, window, cx))))
                .child(bar_button("factory-changes-send", Variant::Primary, None, "Send to the lead").h(px(28.)).child(start_kbd()).on_click(cx.listener(|this, _: &ClickEvent, window, cx| _ = this.factory_send_changes(window, cx))))
        } else {
            let can = can_start(true, kept);
            div()
                .flex_1()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(what)
                .child(bar_button("factory-changes", Variant::Secondary, None, "Request changes").h(px(28.)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.factory_changes(true, window, cx))))
                .child(
                    bar_button("factory-approve", Variant::Primary, None, approve_label(kept))
                        .h(px(28.))
                        .child(start_kbd())
                        .when(!can, |b| b.opacity(0.5))
                        .when(can, |b| b.on_click(cx.listener(|this, _: &ClickEvent, _, cx| _ = this.factory_approve(cx)))),
                )
        };
        Some(ui::pop(div()).absolute().left(px(12.)).right(px(12.)).bottom(px(10.)).min_h(px(42.)).pl(px(14.)).pr(px(8.)).py(px(7.)).rounded(px(11.)).flex().items_center().text_size(px(12.5)).text_color(TEXT_2).child(content))
    }
}

#[cfg(test)]
mod tests {
    // A glob would bring in gpui's `test` attribute, which shadows the std one.
    use super::{Decision, approve_label, change_note, decision, summary};
    use agents::tickets::{Gate, State, Station, Status, Ticket};
    use agents::{Team, TeamTask};

    fn gated() -> Ticket {
        Ticket { state: State::Live, station: Station::Plan, gate: Gate::Plan, status: Status::NeedsYou, agent_id: "a1".into(), ..Ticket::sample() }
    }

    fn team(state: &str, tasks: usize) -> Team {
        Team { lead: "a1".into(), state: state.into(), tasks: (0..tasks).map(|i| TeamTask { id: format!("t{i}"), ..TeamTask::default() }).collect(), ..Team::default() }
    }

    #[test]
    fn a_ticket_at_the_plan_gate_asks_you_to_approve_its_plan() {
        assert_eq!(decision(&gated(), Some(&team("proposed", 3)), None, None, None), Some(Decision::Plan { kept: 3 }));
    }

    #[test]
    fn there_is_nothing_to_decide_before_the_gate_or_without_a_proposed_plan() {
        assert_eq!(decision(&gated(), None, None, None, None), None);
        assert_eq!(decision(&gated(), Some(&team("planning", 3)), None, None, None), None);
        let working = Ticket { gate: Gate::None, status: Status::Working, ..gated() };
        assert_eq!(decision(&working, Some(&team("proposed", 3)), None, None, None), None);
    }

    #[test]
    fn the_bar_counts_one_worker_per_kept_task() {
        assert_eq!(summary(2), "2 tasks, 2 workers");
        assert_eq!(summary(1), "1 task, 1 worker");
        assert_eq!(approve_label(3), "Approve and start 3 workers");
        assert_eq!(approve_label(1), "Approve and start 1 worker");
    }

    #[test]
    fn a_change_request_needs_words() {
        assert_eq!(change_note("  Make t2 smaller \n"), Some("Make t2 smaller"));
        assert_eq!(change_note(" \n"), None);
    }
}
