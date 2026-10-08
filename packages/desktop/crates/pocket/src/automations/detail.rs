use super::glyph::glyph;
use super::logic::{Tab, run_age_text, stamp, strip, trigger_text, when_label};
use super::parts::{card, crumbs, details, pill, section};
use super::Menu;
use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay, drag_area};
use crate::util::{initials, now_ms, tilde};
use agents::automations::{Automation, Run, RunStatus};
use chrono::Local;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{self, Variant};

fn strip_color(status: RunStatus) -> Token {
    match status {
        RunStatus::Succeeded => SUCCESS,
        RunStatus::Failed => FAILED,
        RunStatus::Waiting => WAITING,
        RunStatus::Running | RunStatus::Pending => ACCENT,
        RunStatus::Cancelled => TEXT_5,
        RunStatus::Skipped | RunStatus::Unknown => TEXT_6,
    }
}

/// What a recent run's line says about its outcome, and the colour it says it in.
fn outcome(status: RunStatus) -> (&'static str, Token) {
    match status {
        RunStatus::Waiting => ("Needs you", WAITING_TEXT),
        RunStatus::Running | RunStatus::Pending => ("Working", ACCENT),
        RunStatus::Succeeded => ("Done", TEXT_3),
        RunStatus::Failed => ("Failed", FAILED),
        RunStatus::Skipped => ("Skipped", TEXT_3),
        RunStatus::Cancelled => ("Stopped", TEXT_3),
        RunStatus::Unknown => ("", TEXT_3),
    }
}

impl Desktop {
    /// The right-hand page of the Automations screen.
    pub(crate) fn automations_page(&mut self, cx: &mut Context<Self>) -> Div {
        if self.automations.form.open {
            return self.automation_editor_page(cx);
        }
        let a = &self.agents.automations;
        if !a.loaded {
            return div().flex_1();
        }
        match self.automations.tab {
            Tab::Automations => {
                let auto = self.automations.selected.as_deref().and_then(|id| a.items.iter().find(|x| x.id == id)).or(a.items.first()).cloned();
                match auto {
                    Some(auto) => self.automation_detail(auto, cx),
                    None => self.automations_empty(cx),
                }
            }
            Tab::Runs => match super::logic::shown_run(self.automations.selected_run.as_deref(), self.automations.only.as_deref(), &a.runs).cloned() {
                Some(run) => self.run_detail(run, cx),
                None => self.page_note("No runs yet."),
            },
        }
    }

    fn page_note(&self, text: &str) -> Div {
        drag_area(div()).flex_1().flex().items_center().justify_center().text_size(px(14.)).text_color(TEXT_3).child(text.to_string())
    }

    fn automations_empty(&self, cx: &mut Context<Self>) -> Div {
        drag_area(div())
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(14.))
            .text_size(px(14.))
            .text_color(TEXT_3)
            .child(icon("bolt", 22., TEXT_4))
            .child("No automations yet.")
            .child(ui::button("automation-empty-new", Variant::Primary, Some("plus"), "New automation").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.edit_automation(None, window, cx))))
    }

    pub(super) fn ask_delete_automation(&mut self, id: String, cx: &mut Context<Self>) {
        self.close_menus();
        self.confirm = Some(Confirm::DeleteAutomation(id));
        self.overlay = Some(Overlay::Confirm);
        cx.notify();
    }

    fn automation_menu(&self, id: &str, cx: &mut Context<Self>) -> Div {
        let open = self.automations.menu == Some(Menu::More);
        let button = self.menu_toggle(ui::icon_button("automation-more", "more"), Menu::More, cx);
        let target = id.to_string();
        div().relative().child(button).when(open, |d| {
            let menu = ui::menu_in(
                "automation-more-in",
                self.popup("automation-more-menu", 180., cx)
                    .child(ui::danger_row("automation-delete", "trash", "Delete").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.ask_delete_automation(target.clone(), cx)))),
            );
            d.child(ui::dropdown(32., menu))
        })
    }

    fn automation_detail(&mut self, auto: Automation, cx: &mut Context<Self>) -> Div {
        let now = now_ms();
        let a = &self.agents.automations;
        let project = self.repo_name(&auto.folder);
        let runs: Vec<Run> = a.runs_of(&auto.id).cloned().collect();
        let (id, enabled) = (auto.id.clone(), auto.enabled);
        let toggle = div()
            .id("automation-enabled")
            .flex()
            .items_center()
            .gap(px(8.))
            .cursor_pointer()
            .text_color(TEXT_2)
            .child(ui::toggle(enabled))
            .child(if enabled { "Active" } else { "Paused" })
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_automation(&id, !enabled, cx)));
        let (edit_id, run_id) = (auto.id.clone(), auto.id.clone());
        let right = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(toggle)
            .child(pill("automation-edit", Variant::Secondary, Some("pencil"), "Edit").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.edit_automation(Some(&edit_id), window, cx))))
            .child(pill("automation-run", Variant::Primary, Some("play"), "Run now").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.run_automation(&run_id);
                cx.notify();
            })))
            .child(self.automation_menu(&auto.id, cx));
        let bar = self.detail_bar(crumbs("Automations", auto.name.clone()), right, cx);

        let title = div()
            .flex()
            .items_center()
            .gap(px(14.))
            .child(ui::repo_tile(&initials(&project), 40., false, None))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(div().truncate().text_size(px(22.)).line_height(px(28.)).font_weight(FontWeight::BOLD).child(auto.name.clone()))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(7.))
                            .text_size(px(13.))
                            .text_color(TEXT_3)
                            .child(provider_icon(&auto.provider, 14., TEXT_2))
                            .child(format!("{} · {project}", provider_name(&auto.provider))),
                    ),
            );

        let waiting = a.waiting().find(|r| r.automation_id == auto.id).cloned().map(|run| self.waiting_card(&run, &auto.name, cx));
        let next = if enabled && auto.next_run_at > 0 { stamp(auto.next_run_at, now, &Local) } else { "Paused".into() };
        let facts = details(vec![
            ("Trigger", when_label(&auto.schedule).into_any_element()),
            ("Folder", div().truncate().child(tilde(&auto.folder)).into_any_element()),
            ("Agent", provider_name(&auto.provider).into_any_element()),
            ("Next run", next.into_any_element()),
            ("If missed", "Skipped if more than 12 hours late".into_any_element()),
        ]);
        let recent = self.recent_runs(&auto, &runs, now, cx);
        let body = [title.into_any_element()]
            .into_iter()
            .chain(waiting.map(IntoElement::into_any_element))
        .chain([section("Details", facts).into_any_element(), recent.into_any_element()]);
        div().flex_1().min_w_0().flex().flex_col().child(bar).child(self.detail_body("automation-detail", 18., body))
    }

    fn waiting_card(&self, run: &Run, name: &str, cx: &mut Context<Self>) -> Div {
        let line = self.run_ask(run, format!("{name} paused until you answer"));
        let run = run.clone();
        let answer = div()
            .id("waiting-answer")
            .h(px(28.))
            .px(px(12.))
            .flex()
            .flex_none()
            .items_center()
            .rounded(px(14.))
            .cursor_pointer()
            .bg(WAITING_DOT)
            .text_color(ON_WAITING)
            .text_size(px(13.))
            .font_weight(FontWeight::SEMIBOLD)
            .child("Answer")
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.answer_run(&run, window, cx);
            }));
        card()
            .p(px(12.))
            .flex()
            .items_center()
            .gap(px(12.))
            .shadow(vec![ui::ring(WAITING_BG, 1.), ui::shadow(gpui_kit::rgba(0x1111130f), 1., 2.)])
            .child(div().size(px(34.)).flex_none().flex().items_center().justify_center().rounded(px(9.)).bg(WAITING_BG).child(icon("bolt", 17., WAITING)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(div().text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).child("Needs you"))
                    .child(div().truncate().text_size(px(12.5)).text_color(TEXT_3).child(line)),
            )
            .child(answer)
    }

    fn recent_runs(&self, auto: &Automation, runs: &[Run], now: i64, cx: &mut Context<Self>) -> Div {
        let bars = strip(runs.iter()).into_iter().map(|s| div().w(px(6.)).h(px(12.)).rounded(px(2.)).bg(strip_color(s)));
        let id = auto.id.clone();
        let head = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .pl(px(4.))
            .text_size(px(12.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(TEXT_3)
            .child("Recent runs")
            .child(div().flex().gap(px(3.)).children(bars))
            .child(div().ml_auto().child(ui::link("automation-all-runs", "All runs").text_size(px(12.)).text_color(ACCENT_LINK).px(px(4.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.automations.tab = Tab::Runs;
                this.set_only(Some(id.clone()), cx);
            }))));
        let body = if runs.is_empty() {
            let label = when_label(&auto.schedule);
            let mut chars = label.chars();
            let lower: String = chars.next().into_iter().flat_map(char::to_lowercase).chain(chars).collect();
            card().py(px(18.)).px(px(16.)).text_size(px(13.)).text_color(TEXT_3).child(format!("No runs yet. It runs {lower}, or press Run now."))
        } else {
            card().overflow_hidden().children(runs.iter().take(5).enumerate().map(|(i, r)| self.recent_row(i, r, now, cx)))
        };
        div().flex().flex_col().gap(px(8.)).child(head).child(body)
    }

    fn recent_row(&self, i: usize, r: &Run, now: i64, cx: &mut Context<Self>) -> Div {
        let (label, tone) = outcome(r.status);
        let agent = r.agent_id.clone();
        let open = (!agent.is_empty()).then(|| {
            div()
                .id(("recent-open", i))
                .flex()
                .items_center()
                .gap(px(4.))
                .cursor_pointer()
                .text_size(px(12.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(ACCENT_LINK)
                .underline()
                .child("Open")
                .child(icon("external", 12., ACCENT_LINK))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_run_session(&agent, window, cx)))
        });
        let first = div()
            .flex()
            .items_baseline()
            .gap(px(8.))
            .child(div().flex_none().text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).child(trigger_text(r, &Local)))
            .child(div().min_w_0().truncate().text_size(px(12.)).font_weight(FontWeight::MEDIUM).text_color(tone).child(if r.why.is_empty() { label.to_string() } else { format!("{label} · {}", r.why) }));
        div()
            .py(px(11.))
            .pr(px(14.))
            .pl(px(16.))
            .flex()
            .items_start()
            .gap(px(12.))
            .when(i > 0, |d| d.border_t(px(0.5)).border_color(HAIRLINE))
            .child(div().w(px(16.)).flex_none().pt(px(3.)).child(glyph(r.status, 14., ("recent-glyph", i))))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(first)
                    .children((!r.summary.is_empty()).then(|| div().line_clamp(2).text_size(px(12.5)).text_color(TEXT_2).child(r.summary.clone()))),
            )
            .child(div().flex().flex_col().items_end().gap(px(2.)).flex_none().child(div().text_size(px(12.)).text_color(TEXT_3).child(run_age_text(r, now, &Local))).children(open))
    }
}
