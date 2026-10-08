use super::glyph::status_pill;
use super::logic::{duration_text, run_age_text, run_reason, stamp, when_label};
use super::parts::{card, crumbs, details, pill, section};
use crate::desktop::Desktop;
use crate::util::{now_ms, tilde};
use agents::Decision;
use agents::automations::{Run, RunStatus};
use chrono::Local;
use gpui_kit::*;
use theme::*;
use ui::{self, Variant};

impl Desktop {
    pub(super) fn run_detail(&mut self, run: Run, cx: &mut Context<Self>) -> Div {
        let now = now_ms();
        let auto = self.agents.automations.items.iter().find(|a| a.id == run.automation_id).cloned();
        let reason = run_reason(&run, &Local);
        let agent = run.agent_id.clone();
        let open = (!agent.is_empty()).then(|| {
            pill("run-open-session", Variant::Secondary, None, "Open session").child(icon("external", 12., TEXT)).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_run_session(&agent, window, cx)))
        });
        let bar = self.detail_bar(crumbs("Runs", reason.clone()), div().children(open), cx);

        let provider = auto.as_ref().map_or("claude", |a| a.provider.as_str());
        let project = auto.as_ref().map(|a| self.repo_name(&a.folder));
        let who = match &auto {
            Some(a) => {
                let id = a.id.clone();
                ui::link("run-automation", a.name.clone()).text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(ACCENT_LINK).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_automation(id.clone(), cx))).into_any_element()
            }
            None => "a deleted automation".into_any_element(),
        };
        let header = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(div().flex().items_center().gap(px(8.)).child(status_pill(run.status, "run-pill")).child(div().text_size(px(12.5)).text_color(TEXT_3).child(run_age_text(&run, now, &Local))))
            .child(div().text_size(px(22.)).line_height(px(28.)).font_weight(FontWeight::BOLD).child(reason))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .text_size(px(13.))
                    .text_color(TEXT_2)
                    .child(provider_icon(provider, 14., TEXT_2))
                    .child(format!("{} ran", provider_name(provider)))
                    .child(who)
                    .children(project.map(|p| format!("in {p}"))),
            );

        let approval = (run.status == RunStatus::Waiting).then(|| self.approval_card(&run, provider, cx));
        let failure = (run.status == RunStatus::Failed).then(|| {
            let title = if run.why.is_empty() { "Run failed".to_string() } else { format!("Run failed · {}", run.why) };
            div().p(px(16.)).py(px(14.)).rounded(px(12.)).bg(FAILED_BG).text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).text_color(FAILED).child(title)
        });
        let summary = (!run.summary.is_empty()).then(|| {
            section("Summary", card().py(px(14.)).px(px(16.)).text_size(px(14.)).line_height(px(22.4)).child(run.summary.clone()))
        });
        let duration = match run.status {
            RunStatus::Running | RunStatus::Pending | RunStatus::Waiting => "In progress".to_string(),
            _ => Some(duration_text(run.started_at, run.finished_at)).filter(|d| !d.is_empty()).unwrap_or_else(|| "—".into()),
        };
        let mut rows = vec![
            ("Trigger", auto.as_ref().map_or_else(|| "—".to_string(), |a| when_label(&a.schedule)).into_any_element()),
            ("Started", stamp(run.started_at, now, &Local).into_any_element()),
            ("Duration", duration.into_any_element()),
        ];
        if let Some(a) = &auto {
            rows.push(("Folder", div().truncate().child(tilde(&a.folder)).into_any_element()));
        }
        let body = [header.into_any_element()]
            .into_iter()
            .chain(approval.map(IntoElement::into_any_element))
            .chain(failure.map(IntoElement::into_any_element))
            .chain(summary.map(IntoElement::into_any_element))
            .chain([section("Details", details(rows)).into_any_element()]);
        div().flex_1().min_w_0().flex().flex_col().child(bar).child(self.detail_body("run-detail", 18., body))
    }

    /// Asks to allow what a waiting run's agent wants to do; answering resolves the request itself.
    fn approval_card(&self, run: &Run, provider: &str, cx: &mut Context<Self>) -> Div {
        let pending = self.agents.pending.iter().find(|p| !run.agent_id.is_empty() && p.agent_id == run.agent_id).cloned();
        let agent = run.agent_id.clone();
        let reply = (!agent.is_empty()).then(|| {
            div()
                .id("approval-reply")
                .ml_auto()
                .flex()
                .items_center()
                .gap(px(4.))
                .cursor_pointer()
                .text_size(px(12.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(TEXT_2)
                .child("Reply in session")
                .child(icon("external", 11., TEXT_2))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_run_session(&agent, window, cx)))
        });
        let command = pending.as_ref().map(|p| {
            let text = if p.detail.kind == "shell" { format!("$ {}", p.detail.command) } else { p.ask() };
            div().py(px(10.)).px(px(12.)).rounded(px(8.)).bg(FILL_2).font_family(MONO).text_size(px(13.)).text_color(Token::new(0x0f766eff, 0x5eead4ff)).child(text)
        });
        let answers = pending.map(|p| {
            let (allow, deny) = (p.request_id.clone(), p.request_id);
            [
                pill("approval-allow", Variant::Primary, None, "Allow once")
                    .h(px(30.))
                    .px(px(14.))
                    .rounded(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.outbox.resolve(&allow, Decision::Allow);
                        cx.notify();
                    })),
                pill("approval-deny", Variant::Ghost, None, "Deny")
                    .h(px(30.))
                    .rounded(px(15.))
                    .text_color(FAILED)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.outbox.resolve(&deny, Decision::Deny);
                        cx.notify();
                    })),
            ]
        });
        card()
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .shadow(vec![ui::ring(WAITING_BG, 1.), ui::shadow(gpui_kit::rgba(0x1111130f), 1., 2.)])
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(9.))
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(ui::dot(8., WAITING).shadow(vec![ui::ring(WAITING_BG, 3.)]))
                    .child(if command.is_some() { "Allow command?" } else { "Needs your answer" })
                    .child(div().text_size(px(12.5)).font_weight(FontWeight::NORMAL).text_color(TEXT_3).child(format!("{} paused until you answer", provider_name(provider)))),
            )
            .children(command)
            .child(div().h(px(30.)).flex().items_center().gap(px(8.)).children(answers.into_iter().flatten()).children(reply))
    }
}
