use super::page::Page;
use crate::board::logic::ended;
use crate::board::parts::{bar_button, start_kbd};
use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use crate::team::plan::can_start;
use crate::team::{PROPOSED, plural};
use agents::factory::Detail;
use agents::tickets::{Gate, Station, Status, Ticket};
use agents::Decision as Answer;
use agents::{Permission, Team};
use git::github::Pr;
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::Variant;

/// What the footer bar asks you to decide.
#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Decision {
    /// The plan waits for approval. `kept` is how many tasks it has.
    Plan { kept: usize },
    /// A session of the Ticket waits for your answer before it runs `command`. `worker` names that session.
    Ask { command: String, worker: String },
    /// Every worker handed back and the lead stopped without a verdict.
    Idle,
    /// The Ticket failed at Checks. `used` of `cap` auto-fix attempts are spent.
    Failed { used: u8, cap: u8 },
    /// The lead of a Spec or a Plan failed and is gone: Start again makes a new one.
    Retry { why: String },
    /// The PR waits for your review. `gate` says what holds its merge back.
    Pr { gate: git::github::Gate, number: u32 },
}

/// What the bar asks of `t`, if anything. Plans 5 and 6 read `pr` and `detail`.
pub(crate) fn decision(t: &Ticket, team: Option<&Team>, pr: Option<&Pr>, ask: Option<&Permission>, _detail: Option<&Detail>) -> Option<Decision> {
    match t.gate {
        Gate::Plan => team.filter(|team| team.state == PROPOSED).map(|team| Decision::Plan { kept: team.tasks.len() }),
        // A question has its own dialog, in the session.
        Gate::Ask => ask.filter(|p| p.questions().is_empty()).map(|p| Decision::Ask { command: p.parts().1, worker: asker(p, team) }),
        Gate::Pr => Some(Decision::Pr { gate: pr.map_or(git::github::Gate::Checking, Pr::gate), number: t.pr }),
        _ if t.station == Station::Implement && t.status == Status::Idle => Some(Decision::Idle),
        // The project's cap is not known here; page_decision fills it in.
        _ if t.station == Station::Checks && t.status == Status::Failed => Some(Decision::Failed { used: t.fix, cap: t.fix }),
        _ if matches!(t.station, Station::Spec | Station::Plan) && t.status == Status::Failed && ended(t) => Some(Decision::Retry { why: t.why.clone() }),
        _ => None,
    }
}

/// Who asks: a worker by its task, else the lead.
fn asker(p: &Permission, team: Option<&Team>) -> String {
    team.and_then(|tm| tm.tasks.iter().find(|x| !x.worker.is_empty() && x.worker == p.agent_id)).map_or_else(|| "The lead".into(), |x| format!("Worker {}", x.id))
}

/// The first open ask of `lead` or of a worker of its team, in the order they came.
pub(crate) fn ticket_ask<'a>(pending: &'a [Permission], lead: &str, team: Option<&Team>) -> Option<&'a Permission> {
    pending.iter().find(|p| p.agent_id == lead || team.is_some_and(|tm| tm.tasks.iter().any(|x| !x.worker.is_empty() && x.worker == p.agent_id)))
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

pub(crate) fn retry_title(station: Station) -> &'static str {
    match station {
        Station::Plan => "The plan failed",
        _ => "The spec failed",
    }
}

pub(crate) fn hint_text(typed: &str) -> Option<&str> {
    Some(typed.trim()).filter(|t| !t.is_empty())
}

pub(crate) fn ask_label(worker: &str) -> String {
    format!("{worker} wants to run")
}

impl Desktop {
    /// The open ask the page answers, unless this connection may only watch.
    fn page_ask(&self, t: &Ticket) -> Option<&Permission> {
        ticket_ask(&self.agents.pending, &t.agent_id, self.agents.team(&t.agent_id)).filter(|_| !self.agents.observe_only())
    }

    /// The page's decision, with the count of the tasks you kept at the gate.
    fn page_decision(&self, t: &Ticket) -> Option<Decision> {
        let team = self.agents.team(&t.agent_id);
        decision(t, team, self.prs.get(&t.worktree), self.page_ask(t), self.agents.detail(&t.id)).map(|d| match d {
            Decision::Plan { kept } => Decision::Plan { kept: self.team.island.edits.get(&t.agent_id).map_or(kept, |e| e.tasks().len()) },
            Decision::Failed { used, .. } => Decision::Failed { used, cap: self.agents.factory_config(&t.folder).map_or(used, |c| c.fix) },
            other => other,
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
        let Some((lead, gate)) = self.factory.page.as_ref().and_then(|p| self.agents.board.get(&p.ticket)).map(|t| (t.agent_id.clone(), t.gate)) else { return false };
        self.outbox.prompt(&lead, note, super::pr::replans(gate));
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
            Some(Decision::Ask { .. }) => {
                let Some(request) = self.page_ask(&t).map(|p| p.request_id.clone()) else { return false };
                self.team_answer(&request, Answer::Allow, cx);
                true
            }
            Some(Decision::Failed { .. }) => self.factory_send_hint(window, cx),
            Some(Decision::Pr { .. }) if page.changes => self.factory_send_changes(window, cx),
            Some(Decision::Pr { .. }) => self.factory_merge(&t, cx),
            _ => false,
        }
    }

    pub(super) fn factory_bar(&self, t: &Ticket, page: &Page, cx: &mut Context<Self>) -> Option<Div> {
        let content = match self.page_decision(t)? {
            Decision::Plan { kept } => self.plan_bar(kept, page, cx),
            Decision::Ask { command, worker } => self.ask_bar(t, &command, &worker, cx)?,
            Decision::Idle => self.idle_bar(t, cx),
            Decision::Failed { cap, .. } => self.failed_bar(t, cap, cx),
            Decision::Retry { why } => self.retry_bar(t, &why, cx),
            Decision::Pr { gate, number } => self.pr_bar(t, gate, number, page, cx),
        };
        Some(ui::pop(div()).absolute().left(px(12.)).right(px(12.)).bottom(px(10.)).min_h(px(42.)).pl(px(14.)).pr(px(8.)).py(px(7.)).rounded(px(11.)).flex().items_center().text_size(px(12.5)).text_color(TEXT_2).child(content))
    }

    fn plan_bar(&self, kept: usize, page: &Page, cx: &mut Context<Self>) -> Div {
        let what = div().flex_1().min_w_0().flex().items_center().gap(px(7.)).child(div().size(px(6.)).rounded(px(3.)).bg(WAITING_DOT)).child(div().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child("Plan approval")).child(div().text_color(TEXT_3).child(summary(kept)));
        if page.changes {
            self.changes_bar(cx)
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
        }
    }

    /// The change request's field with Cancel and Send to the lead.
    pub(super) fn changes_bar(&self, cx: &mut Context<Self>) -> Div {
        let field = div().flex_1().min_w_0().h(px(28.)).px(px(9.)).flex().items_center().rounded(px(7.)).bg(FILL_2).child(Input::new(&self.factory.changes).appearance(false).p_0().text_size(px(12.5)));
        div()
            .flex_1()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(field)
            .child(bar_button("factory-changes-cancel", Variant::Ghost, None, "Cancel").h(px(28.)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.factory_changes(false, window, cx))))
            .child(bar_button("factory-changes-send", Variant::Primary, None, "Send to the lead").h(px(28.)).child(start_kbd()).on_click(cx.listener(|this, _: &ClickEvent, window, cx| _ = this.factory_send_changes(window, cx))))
    }

    /// An Ask: who wants to run what, the words of a deny when the request takes them, Deny and Allow once.
    fn ask_bar(&self, t: &Ticket, command: &str, worker: &str, cx: &mut Context<Self>) -> Option<Div> {
        let p = self.page_ask(t)?;
        let (deny, allow) = (p.request_id.clone(), p.request_id.clone());
        let what = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(7.))
            .child(div().flex_none().size(px(6.)).rounded(px(3.)).bg(WAITING_DOT))
            .child(div().flex_none().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(ask_label(worker)))
            .child(div().min_w_0().truncate().font_family(MONO).text_color(TEXT_2).child(command.to_string()));
        let field = p.feedback.then(|| div().w(px(200.)).h(px(28.)).px(px(9.)).flex().items_center().rounded(px(7.)).bg(FILL_2).child(Input::new(&self.factory.deny).appearance(false).p_0().text_size(px(12.5))));
        let buttons = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(bar_button("factory-deny", Variant::Ghost, None, "Deny").h(px(28.)).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.factory_deny(&deny, window, cx))))
            .child(bar_button("factory-allow", Variant::Primary, None, "Allow once").h(px(28.)).child(start_kbd()).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.team_answer(&allow, Answer::Allow, cx))));
        Some(div().flex_1().flex().items_center().gap(px(10.)).child(what).children(field).child(buttons))
    }

    /// Every worker handed back and the lead gave no verdict: you may move the Ticket on (I8).
    fn idle_bar(&self, t: &Ticket, cx: &mut Context<Self>) -> Div {
        let ticket = t.id.clone();
        let what = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(div().flex().items_center().gap(px(7.)).child(div().size(px(6.)).rounded(px(3.)).bg(WAITING_DOT)).child(div().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child("Every worker handed back")))
            .child(div().pl(px(13.)).text_size(px(11.5)).text_color(TEXT_3).child("The lead stopped without saying the work is ready."));
        let button = bar_button("factory-advance", Variant::Primary, None, "Move to Checks anyway").h(px(28.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.factory_advance(&ticket, cx)));
        div().flex_1().flex().items_center().gap(px(10.)).child(what).child(button)
    }

    /// Failed at Checks: what failed and the attempts used, a hint for the lead, Close ticket and Send back with a hint.
    fn failed_bar(&self, t: &Ticket, cap: u8, cx: &mut Context<Self>) -> Div {
        let what = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(7.))
            .child(div().size(px(6.)).rounded(px(3.)).bg(FAILED))
            .child(div().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child("Checks failed"))
            .child(div().text_color(FAILED_TEXT).child(super::checks::attempt_line(t, cap)));
        let field = div().flex_1().min_w(px(120.)).h(px(28.)).px(px(9.)).flex().items_center().rounded(px(7.)).bg(FILL_2).child(Input::new(&self.factory.hint).appearance(false).p_0().text_size(px(12.5)));
        let can = hint_text(self.factory.hint.read(cx).value().as_ref()).is_some();
        let ticket = t.id.clone();
        let buttons = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .child(bar_button("factory-close", Variant::Ghost, None, "Close ticket").h(px(28.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.factory_close(&ticket, cx))))
            .child(
                bar_button("factory-hint", Variant::Primary, None, "Send back with a hint")
                    .h(px(28.))
                    .child(start_kbd())
                    .when(!can, |b| b.opacity(0.5))
                    .when(can, |b| b.on_click(cx.listener(|this, _: &ClickEvent, window, cx| _ = this.factory_send_hint(window, cx)))),
            );
        div().flex_1().flex().items_center().gap(px(10.)).child(what).child(field).child(buttons)
    }

    /// A Spec or a Plan failed and its lead is gone: say why, then Close ticket or Start again.
    fn retry_bar(&self, t: &Ticket, why: &str, cx: &mut Context<Self>) -> Div {
        let (close, start) = (t.id.clone(), t.id.clone());
        let what = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(div().flex().items_center().gap(px(7.)).child(div().size(px(6.)).rounded(px(3.)).bg(FAILED)).child(div().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(retry_title(t.station))))
            .children((!why.is_empty()).then(|| div().pl(px(13.)).min_w_0().truncate().text_size(px(11.5)).text_color(TEXT_3).child(why.to_string())));
        let buttons = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .child(bar_button("factory-close", Variant::Ghost, None, "Close ticket").h(px(28.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.factory_close(&close, cx))))
            .child(bar_button("factory-start-again", Variant::Primary, None, "Start again").h(px(28.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.factory_start_again(&start, cx))));
        div().flex_1().flex().items_center().gap(px(10.)).child(what).child(buttons)
    }

    /// Sends the typed hint to the page's lead; pocketd grants one more attempt.
    pub(crate) fn factory_send_hint(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let typed = self.factory.hint.read(cx).value().to_string();
        let Some(text) = hint_text(&typed) else { return false };
        let Some(ticket) = self.factory.page.as_ref().map(|p| p.ticket.clone()) else { return false };
        self.outbox.local.factory_hint(&ticket, text);
        self.factory.hint.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
        true
    }

    fn factory_close(&mut self, ticket: &str, cx: &mut Context<Self>) {
        self.outbox.local.board_close(ticket);
        cx.notify();
    }

    fn factory_start_again(&mut self, ticket: &str, cx: &mut Context<Self>) {
        self.outbox.local.board_start(ticket);
        cx.notify();
    }

    /// Denies `request`, with the typed words when there are any.
    pub(crate) fn factory_deny(&mut self, request: &str, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.factory.deny.read(cx).value().to_string();
        match change_note(&typed) {
            Some(words) => self.outbox.reply(request, words),
            None => self.outbox.resolve(request, Answer::Deny),
        }
        self.factory.deny.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }

    pub(crate) fn factory_advance(&mut self, ticket: &str, cx: &mut Context<Self>) {
        self.outbox.local.factory_advance(ticket);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    // A glob would bring in gpui's `test` attribute, which shadows the std one.
    use super::{Decision, approve_label, ask_label, change_note, decision, hint_text, retry_title, summary, ticket_ask};
    use agents::tickets::{Gate, State, Station, Status, Ticket};
    use agents::{Detail as ToolDetail, Permission, Team, TeamTask};
    use git::github::{Gate as PrGate, Pr};

    fn gated() -> Ticket {
        Ticket { state: State::Live, station: Station::Plan, gate: Gate::Plan, status: Status::NeedsYou, agent_id: "a1".into(), ..Ticket::sample() }
    }

    fn team(state: &str, tasks: usize) -> Team {
        Team { lead: "a1".into(), state: state.into(), tasks: (0..tasks).map(|i| TeamTask { id: format!("t{i}"), ..TeamTask::default() }).collect(), ..Team::default() }
    }

    fn shell(agent: &str, command: &str) -> Permission {
        Permission { request_id: "r1".into(), agent_id: agent.into(), tool_name: "Bash".into(), detail: ToolDetail { kind: "shell".into(), command: command.into(), ..Default::default() }, feedback: true }
    }

    fn implementing(status: Status, gate: Gate) -> Ticket {
        Ticket { state: State::Live, station: Station::Implement, gate, status, agent_id: "a1".into(), ..Ticket::sample() }
    }

    fn running() -> Team {
        let mut tm = team("running", 2);
        tm.tasks[0].worker = "w0".into();
        tm.tasks[1].worker = "w1".into();
        tm
    }

    #[test]
    fn a_ticket_that_waits_on_a_workers_ask_shows_the_command_and_the_worker() {
        let t = implementing(Status::NeedsYou, Gate::Ask);
        let ask = shell("w1", "gh pr merge 7");
        assert_eq!(decision(&t, Some(&running()), None, Some(&ask), None), Some(Decision::Ask { command: "gh pr merge 7".into(), worker: "Worker t1".into() }));
        let leads = shell("a1", "make build");
        assert_eq!(decision(&t, Some(&running()), None, Some(&leads), None), Some(Decision::Ask { command: "make build".into(), worker: "The lead".into() }));
    }

    #[test]
    fn a_question_or_a_lifted_gate_asks_nothing_on_the_bar() {
        let t = implementing(Status::NeedsYou, Gate::Ask);
        let question = Permission { tool_name: "AskUserQuestion".into(), detail: ToolDetail { input: serde_json::json!({"questions": [{"question": "Which?"}]}), ..Default::default() }, ..shell("w1", "") };
        assert_eq!(decision(&t, Some(&running()), None, Some(&question), None), None);
        assert_eq!(decision(&t, Some(&running()), None, None, None), None);
        assert_eq!(decision(&implementing(Status::Working, Gate::None), Some(&running()), None, Some(&shell("w1", "ls")), None), None);
    }

    #[test]
    fn an_idle_ticket_at_implement_offers_the_move_to_checks() {
        assert_eq!(decision(&implementing(Status::Idle, Gate::None), Some(&team("review", 2)), None, None, None), Some(Decision::Idle));
        assert_eq!(decision(&implementing(Status::Working, Gate::None), Some(&team("review", 2)), None, None, None), None);
    }

    #[test]
    fn the_tickets_ask_is_the_first_of_its_lead_or_its_workers() {
        let pending = vec![shell("stranger", "ls"), Permission { request_id: "r2".into(), ..shell("w1", "make build") }, Permission { request_id: "r3".into(), ..shell("a1", "make") }];
        assert_eq!(ticket_ask(&pending, "a1", Some(&running())).map(|p| p.request_id.as_str()), Some("r2"));
        assert_eq!(ticket_ask(&pending, "a1", None).map(|p| p.request_id.as_str()), Some("r3"));
        assert!(ticket_ask(&pending[..1], "a1", Some(&running())).is_none());
    }

    #[test]
    fn the_ask_bar_names_who_wants_to_run_it() {
        assert_eq!(ask_label("Worker t1"), "Worker t1 wants to run");
        assert_eq!(ask_label("The lead"), "The lead wants to run");
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
    fn a_ticket_that_failed_at_checks_asks_for_a_hint() {
        let t = Ticket { state: State::Live, station: Station::Checks, status: Status::Failed, fix: 2, agent_id: "a1".into(), ..Ticket::sample() };
        assert_eq!(decision(&t, None, None, None, None), Some(Decision::Failed { used: 2, cap: 2 }));
        assert_eq!(decision(&Ticket { status: Status::Working, ..t.clone() }, None, None, None, None), None);
    }

    #[test]
    fn a_ticket_at_the_pr_gate_offers_the_merge_with_what_holds_it_back() {
        let t = Ticket { state: State::Live, station: Station::Pr, gate: Gate::Pr, status: Status::NeedsYou, pr: 203, ..Ticket::sample() };
        let draft = Pr { number: 203, draft: true, ..Pr::default() };
        assert_eq!(decision(&t, None, Some(&draft), None, None), Some(Decision::Pr { gate: PrGate::Draft, number: 203 }));
        assert_eq!(decision(&t, None, None, None, None), Some(Decision::Pr { gate: PrGate::Checking, number: 203 }));
        let working = Ticket { gate: Gate::None, status: Status::Working, ..t };
        assert_eq!(decision(&working, None, Some(&draft), None, None), None);
    }

    #[test]
    fn a_hint_needs_words() {
        assert_eq!(hint_text("  Update the snapshot \n"), Some("Update the snapshot"));
        assert_eq!(hint_text(" \n"), None);
    }

    #[test]
    fn a_spec_or_a_plan_that_failed_with_its_lead_gone_offers_start_again() {
        let failed = |station, agent: &str| Ticket { state: State::Live, station, status: Status::Failed, why: "The lead stopped".into(), agent_id: agent.into(), ..Ticket::sample() };
        assert_eq!(decision(&failed(Station::Spec, ""), None, None, None, None), Some(Decision::Retry { why: "The lead stopped".into() }));
        assert_eq!(decision(&failed(Station::Plan, ""), None, None, None, None), Some(Decision::Retry { why: "The lead stopped".into() }));
    }

    #[test]
    fn start_again_waits_until_the_failed_lead_is_gone() {
        let alive = Ticket { state: State::Live, station: Station::Plan, status: Status::Failed, agent_id: "a1".into(), ..Ticket::sample() };
        assert_eq!(decision(&alive, None, None, None, None), None);
    }

    #[test]
    fn the_retry_bar_names_what_failed() {
        assert_eq!(retry_title(Station::Spec), "The spec failed");
        assert_eq!(retry_title(Station::Plan), "The plan failed");
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
