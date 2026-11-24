use super::glyph::glyph;
use super::logic::{Tab, created_by_line, next_run_text, run_age_text, session_text, strip, trigger_text, when_label};
use super::parts::{card, crumbs, details, section};
use super::{Automations, AutomationsEvent, Menu};
use crate::desktop::chrome::drag_area;
use crate::desktop::chrome::pill;
use crate::util::{now_ms, tilde};
use agents::automations::{Automation, Run, RunStatus, Schedule};
use chrono::Local;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::sync::Arc;
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

/// The webhook's address in the font for addresses, with a button that copies it.
fn webhook_url(url: &str, cx: &mut Context<Automations>) -> Div {
    let copied = url.to_string();
    div().flex().items_center().gap(px(8.)).child(div().min_w_0().truncate().font_family(MONO).text_size(px(12.5)).child(url.to_string())).child(
        div()
            .id("automation-webhook-copy")
            .size(px(20.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(5.))
            .cursor_pointer()
            .hover(|d| d.bg(FILL_3))
            .child(icon("copy", 12., TEXT_3))
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copied.clone())))),
    )
}

impl Render for Automations {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.page(cx)
    }
}

impl Automations {
    fn selected_automation(&self) -> Option<Automation> {
        self.selected.as_deref().and_then(|id| self.data.items.iter().find(|x| x.id == id)).or(self.data.items.first()).cloned()
    }

    fn shown_run(&self) -> Option<Run> {
        super::logic::shown_run(self.selected_run.as_deref(), self.only.as_deref(), &self.data.runs).cloned()
    }

    pub(crate) fn bar(&mut self, cx: &mut Context<Self>) -> Option<(Div, Div)> {
        if self.form.open {
            return Some(self.editor_bar(cx));
        }
        if !self.data.loaded {
            return None;
        }
        match self.tab {
            Tab::Automations => self.selected_automation().map(|auto| self.automation_bar(&auto, cx)),
            Tab::Runs => self.shown_run().map(|run| self.run_bar(&run, cx)),
        }
    }

    fn page(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.form.open {
            return self.editor_body(cx).into_any_element();
        }
        if !self.data.loaded {
            return div().flex_1().into_any_element();
        }
        match self.tab {
            Tab::Automations => match self.selected_automation() {
                Some(auto) => self.automation_body(auto, cx).into_any_element(),
                None => self.automations_empty(cx).into_any_element(),
            },
            Tab::Runs => match self.shown_run() {
                Some(run) => self.run_body(run, cx).into_any_element(),
                None => self.page_note("No runs yet.").into_any_element(),
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
            .child(
                ui::button("automation-empty-new", Variant::Primary, Some("plus"), "New automation")
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.edit(None, window, cx))),
            )
    }

    fn automation_menu(&self, id: &str, cx: &mut Context<Self>) -> Div {
        let open = self.menu == Some(Menu::More);
        let button = self.menu_toggle(ui::icon_button("automation-more", "more"), Menu::More, open, cx);
        let target = id.to_string();
        div().relative().child(button).when(open, |d| {
            let menu = ui::menu_in(
                "automation-more-in",
                self.popup("automation-more-menu", 180., cx).child(
                    ui::danger_row("automation-delete", "trash", "Delete")
                        .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(AutomationsEvent::AskDelete(target.clone())))),
                ),
            );
            d.child(ui::dropdown(32., menu))
        })
    }

    fn automation_bar(&mut self, auto: &Automation, cx: &mut Context<Self>) -> (Div, Div) {
        let (id, enabled) = (auto.id.clone(), auto.enabled);
        let toggle = div()
            .id("automation-enabled")
            .flex()
            .items_center()
            .gap(px(8.))
            .cursor_pointer()
            .text_color(TEXT_2)
            .child(ui::toggle("switch", enabled))
            .child(if enabled { "Active" } else { "Paused" })
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_automation(&id, !enabled, cx)));
        let (edit_id, run_id) = (auto.id.clone(), auto.id.clone());
        let right = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(toggle)
            .child(
                pill("automation-edit", Variant::Secondary, Some("pencil"), "Edit")
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.edit(Some(&edit_id), window, cx))),
            )
            .child(
                pill("automation-run", Variant::Primary, Some("play"), "Run now")
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.run_automation(&run_id, cx))),
            )
            .child(self.automation_menu(&auto.id, cx));
        (crumbs("Automations", auto.name.clone()), right)
    }

    fn automation_body(&mut self, auto: Automation, cx: &mut Context<Self>) -> Stateful<Div> {
        let now = now_ms();
        let a = Arc::clone(&self.data);
        let project = self.repo_name(&auto.folder);
        let created = created_by_line(&auto.created_by, self.side.titles.get(&auto.created_by).map(String::as_str));
        let runs: Vec<Run> = a.runs_of(&auto.id).cloned().collect();

        let title = div().flex().items_center().gap(px(14.)).child(ui::repo_tile(&self.repo_mark(&auto.folder), 40., false, None)).child(
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
                        .child(format!("{} · {project}", self.agent_name(&auto.provider))),
                )
                .children(created.map(|line| div().truncate().text_size(px(12.5)).text_color(TEXT_3).child(line))),
        );

        let waiting = a.waiting().find(|r| r.automation_id == auto.id).cloned().map(|run| self.waiting_card(&run, &auto.name, cx));
        let next = next_run_text(&auto, now, &Local);
        let mut rows = vec![
            ("Trigger", when_label(&auto.schedule).into_any_element()),
            ("Folder", div().truncate().child(tilde(&auto.folder)).into_any_element()),
            ("Agent", self.agent_name(&auto.provider).into_any_element()),
            ("Next run", next.into_any_element()),
        ];
        if !auto.bound.is_empty() {
            rows.push((
                "Posts into",
                div().truncate().child(session_text(self.side.titles.get(&auto.bound).map(String::as_str)).to_string()).into_any_element(),
            ));
        }
        if !auto.webhook_url.is_empty() {
            rows.push(("Webhook", webhook_url(&auto.webhook_url, cx).into_any_element()));
        }
        if auto.schedule != Schedule::Webhook {
            rows.push(("If missed", "Skipped if more than 12 hours late".into_any_element()));
        }
        let facts = details(rows);
        let recent = self.recent_runs(&auto, &runs, now, cx);
        let body = [title.into_any_element()]
            .into_iter()
            .chain(waiting.map(IntoElement::into_any_element))
            .chain([section("Details", facts).into_any_element(), recent.into_any_element()]);
        self.detail_body("automation-detail", 18., body)
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
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.answer_run(&run, cx);
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
            .child(div().ml_auto().child(ui::link("automation-all-runs", "All runs").text_size(px(12.)).text_color(ACCENT_LINK).px(px(4.)).on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.tab = Tab::Runs;
                    this.set_only(Some(id.clone()), cx);
                }),
            )));
        let body = if runs.is_empty() {
            let when = match &auto.schedule {
                Schedule::Webhook => "when its webhook is called".to_string(),
                s => {
                    let label = when_label(s);
                    let mut chars = label.chars();
                    chars.next().into_iter().flat_map(char::to_lowercase).chain(chars).collect()
                }
            };
            card().py(px(18.)).px(px(16.)).text_size(px(13.)).text_color(TEXT_3).child(format!("No runs yet. It runs {when}, or press Run now."))
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
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open_run_session(&agent, cx)))
        });
        let first = div()
            .flex()
            .items_baseline()
            .gap(px(8.))
            .child(div().flex_none().text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).child(trigger_text(r, &Local)))
            .child(div().min_w_0().truncate().text_size(px(12.)).font_weight(FontWeight::MEDIUM).text_color(tone).child(if r.why.is_empty() {
                label.to_string()
            } else {
                format!("{label} · {}", r.why)
            }));
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
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap(px(2.))
                    .flex_none()
                    .child(div().text_size(px(12.)).text_color(TEXT_3).child(run_age_text(r, now, &Local)))
                    .children(open),
            )
    }
}
