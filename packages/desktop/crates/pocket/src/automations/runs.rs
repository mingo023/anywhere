use super::glyph::glyph;
use super::logic::{Row, Section, run_reason, run_when, shown_run, stamp};
use crate::desktop::Desktop;
use crate::desktop::chrome::empty;
use crate::util::now_ms;
use chrono::Local;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

fn head(label: String, count: Option<usize>, tone: Token) -> AnyElement {
    div()
        .w_full()
        .pt(px(14.))
        .px(px(10.))
        .pb(px(6.))
        .flex()
        .gap(px(6.))
        .text_size(px(11.5))
        .font_weight(FontWeight::BOLD)
        .text_color(tone)
        .child(label.to_uppercase())
        .children(count.map(|n| div().font_weight(FontWeight::MEDIUM).text_color(TEXT_4).child(n.to_string())))
        .into_any_element()
}

impl Desktop {
    pub(super) fn runs_list(&mut self, cx: &mut Context<Self>) -> Div {
        let body = div().flex_1().min_h_0().flex().flex_col();
        if !self.agents.automations.loaded {
            return body;
        }
        let only = self.automations.only.as_deref().and_then(|id| self.agents.automations.items.iter().find(|a| a.id == id)).map(|a| a.name.clone());
        let bar = only.map(|name| {
            div()
                .h(px(32.))
                .px(px(12.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(6.))
                .text_size(px(12.5))
                .text_color(TEXT_3)
                .child(div().min_w_0().truncate().child(format!("Runs of {name}")))
                .child("·")
                .child(ui::link("runs-show-all", "Show all").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.set_only(None, cx))))
        });
        if self.automations.rows.is_empty() {
            return body.children(bar).child(empty("No runs yet."));
        }
        let rows = list(self.automations.runs_list.clone(), cx.processor(|this, ix, _, cx| this.run_row(ix, cx))).size_full();
        body.children(bar).child(div().relative().flex_1().min_h_0().child(rows).vertical_scrollbar(&self.automations.runs_list))
    }

    fn run_row(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let a = &self.agents.automations;
        let now = now_ms();
        match self.automations.rows.get(ix).copied() {
            Some(Row::SoonHead) => head("Up next".into(), None, TEXT_3),
            Some(Row::Soon(i)) => {
                let Some(auto) = a.items.get(i) else { return Empty.into_any_element() };
                let id = auto.id.clone();
                div()
                    .px(px(8.))
                    .child(
                        div()
                            .id(("soon", ix))
                            .h(px(26.))
                            .px(px(10.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .rounded(px(8.))
                            .cursor_pointer()
                            .text_size(px(13.))
                            .hover(|s| s.bg(FILL_2))
                            .child(icon("clock", 13., TEXT_4))
                            .child(div().flex_1().min_w_0().truncate().child(auto.name.clone()))
                            .child(div().flex_none().text_size(px(12.)).text_color(TEXT_3).child(stamp(auto.next_run_at, now, &Local)))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_automation(id.clone(), cx))),
                    )
                    .into_any_element()
            }
            Some(Row::Head(section, n)) => head(section.label().into(), Some(n), if section == Section::NeedsYou { WAITING_TEXT } else { TEXT_3 }),
            Some(Row::Run(i)) => {
                let Some(r) = a.runs.get(i) else { return Empty.into_any_element() };
                let selected = shown_run(self.automations.selected_run.as_deref(), self.automations.only.as_deref(), &a.runs).is_some_and(|s| s.id == r.id);
                let name = a.items.iter().find(|x| x.id == r.automation_id).map_or("Deleted automation".to_string(), |x| x.name.clone());
                let id = r.id.clone();
                div()
                    .px(px(8.))
                    .pb(px(2.))
                    .child(
                        div()
                            .id(("run", ix))
                            .w_full()
                            .py(px(9.))
                            .px(px(10.))
                            .flex()
                            .items_start()
                            .gap(px(10.))
                            .rounded(px(10.))
                            .cursor_pointer()
                            .when(selected, |d| d.bg(FILL_3))
                            .when(!selected, |d| d.hover(|s| s.bg(FILL_2)))
                            .child(div().w(px(16.)).flex_none().pt(px(3.)).child(glyph(r.status, 14., ("run-glyph", ix))))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(div().truncate().text_size(px(13.5)).line_height(px(19.)).font_weight(FontWeight::SEMIBOLD).child(name))
                                    .child(div().truncate().text_size(px(12.)).line_height(px(16.)).text_color(TEXT_3).child(run_reason(r, &Local))),
                            )
                            .child(div().flex_none().text_size(px(11.5)).line_height(px(19.)).text_color(TEXT_3).child(run_when(r, now, &Local)))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_run(id.clone(), cx))),
                    )
                    .into_any_element()
            }
            None => Empty.into_any_element(),
        }
    }
}
