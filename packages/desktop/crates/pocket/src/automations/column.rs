use super::logic::{Entry, Tab};
use crate::desktop::Desktop;
use crate::desktop::chrome::{HEADER, Layout, column, drag_area, empty, past_lights};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{self, Variant, icon_button_sized};

impl Desktop {
    pub(crate) fn automations_column(&mut self, cx: &mut Context<Self>) -> Div {
        let tab = self.automations.tab;
        let draft = (self.automations.form.open && self.automations.form.v.id.is_none()).then(|| self.draft_row(cx));
        let body = match tab {
            Tab::Automations => self.automations_list(cx).into_any_element(),
            Tab::Runs => self.runs_list(cx).into_any_element(),
        };
        column()
            .child(drag_area(self.automations_header(cx)).h(px(HEADER)).flex_none().border_b(px(0.5)).border_color(SEPARATOR))
            .child(self.automations_tabs(cx))
            .children((tab == Tab::Automations).then(|| self.needs_banner(cx)).flatten())
            .children(draft)
            .child(body)
    }

    fn automations_header(&self, cx: &mut Context<Self>) -> Div {
        let add = icon_button_sized("automation-add", "plus", 28., TEXT_2).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.edit_automation(None, window, cx)));
        div()
            .pl(px(if self.layout == Layout::Compact { past_lights(self.store.appearance.zoom_factor(), 18.) } else { 18. }))
            .pr(px(12.))
            .flex()
            .items_center()
            .gap(px(4.))
            .child(div().flex_1().text_size(px(16.)).font_weight(FontWeight::BOLD).child("Automations"))
            .child(add)
    }

    fn automations_tabs(&self, cx: &mut Context<Self>) -> Div {
        let waiting = self.agents.automations.waiting().count();
        let tabs = [(Tab::Automations, "Automations"), (Tab::Runs, "Runs")].into_iter().enumerate().map(|(i, (tab, label))| {
            let selected = self.automations.tab == tab;
            let badge = (tab == Tab::Runs && waiting > 0).then(|| {
                div().min_w(px(16.)).h(px(16.)).px(px(4.)).flex().items_center().justify_center().rounded(px(8.)).bg(WAITING_DOT).text_size(px(10.)).font_weight(FontWeight::BOLD).text_color(WHITE).child(waiting.to_string())
            });
            div()
                .id(("automations-tab", i))
                .flex_1()
                .h(px(30.))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(6.))
                .rounded(px(8.))
                .cursor_pointer()
                .text_size(px(13.5))
                .when(selected, |d| d.bg(FILL_4).text_color(TEXT).font_weight(FontWeight::MEDIUM))
                .when(!selected, |d| d.text_color(TEXT_2).font_weight(FontWeight(450.)).hover(|s| s.bg(FILL_2)))
                .child(label)
                .children(badge)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_tab(tab, cx)))
        });
        div().h(px(46.)).p(px(8.)).flex().flex_none().gap(px(4.)).border_b(px(0.5)).border_color(SEPARATOR).children(tabs)
    }

    /// The newest run waiting on the user.
    fn needs_banner(&self, cx: &mut Context<Self>) -> Option<Div> {
        let a = &self.agents.automations;
        let run = a.waiting().next().filter(|_| !self.automations.form.open)?;
        let name = a.items.iter().find(|x| x.id == run.automation_id).map_or("An automation", |x| x.name.as_str());
        let line = self.run_ask(run, "Needs your answer".into());
        let run = run.clone();
        let answer = ui::button("needs-answer", Variant::Primary, None, "Answer").h(px(26.)).px(px(12.)).rounded(px(13.)).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            this.answer_run(&run, window, cx);
        }));
        Some(
            div()
                .mt(px(2.))
                .mx(px(8.))
                .mb(px(2.))
                .py(px(10.))
                .pr(px(10.))
                .pl(px(12.))
                .flex()
                .flex_none()
                .items_start()
                .gap(px(10.))
                .rounded(px(10.))
                .bg(WAITING_BG)
                .child(div().pt(px(5.)).child(ui::dot(8., WAITING).shadow(vec![ui::ring(WAITING_BG, 3.)])))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child(format!("{name} needs you")))
                        .child(div().truncate().text_size(px(12.)).text_color(WAITING_TEXT).child(line)),
                )
                .child(answer),
        )
    }

    /// The editor's automation, listed at the top while it is open.
    fn draft_row(&self, cx: &mut Context<Self>) -> Div {
        let v = self.automations.form.values(cx);
        let title = if !v.title.trim().is_empty() { v.title.trim().to_string() } else { "New automation".into() };
        let when = v.label().unwrap_or_else(|| "No trigger yet".into());
        div().mx(px(8.)).my(px(2.)).p(px(10.)).flex().flex_col().flex_none().gap(px(3.)).rounded(px(10.)).bg(FILL_3).child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(ACCENT).child(if v.id.is_some() { "Editing" } else { "Draft" }))
                .child(div().truncate().text_size(px(14.5)).font_weight(FontWeight::SEMIBOLD).child(title))
                .child(div().flex().items_center().gap(px(6.)).text_size(px(12.5)).text_color(TEXT_3).child(icon("clock", 13., TEXT_4)).child(div().truncate().child(when))),
        )
    }

    fn automations_list(&mut self, cx: &mut Context<Self>) -> Div {
        let a = &self.agents.automations;
        let body = div().flex_1().min_h_0();
        if !a.loaded {
            return body;
        }
        if a.items.is_empty() {
            return body.child(empty("No automations yet."));
        }
        let list = list(
            self.automations.list.clone(),
            cx.processor(|this, ix, _, cx| match this.automations.entries.get(ix).copied() {
                Some(Entry::Head { active, count, open }) => this.group_head(active, count, open, cx),
                Some(Entry::Item(i)) => this.automation_row(i, cx),
                None => Empty.into_any_element(),
            }),
        )
        .size_full()
        .px(px(8.))
        .pb(px(12.));
        body.relative().child(list).vertical_scrollbar(&self.automations.list)
    }

    /// "Active 3" over its group; a press folds or unfolds it.
    fn group_head(&self, active: bool, count: usize, open: bool, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w_full()
            .pt(px(6.))
            .child(
                div()
                    .id(("automations-group", active as usize))
                    .h(px(28.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(7.))
                    .cursor_pointer()
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(TEXT_3)
                    .hover(|s| s.bg(FILL_2))
                    .child(icon(if open { "chevron-down" } else { "chevron-right" }, 12., TEXT_4))
                    .child(if active { "Active" } else { "Paused" })
                    .child(div().font_weight(FontWeight::MEDIUM).text_color(TEXT_4).child(count.to_string()))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.fold_group(active, cx))),
            )
            .into_any_element()
    }
}
