use super::glyph::glyph;
use super::logic::{last_label, trigger_line};
use crate::desktop::Desktop;
use crate::util::now_ms;
use agents::automations::RunStatus;
use chrono::Local;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

impl Desktop {
    /// Automation `i` as a row of the list: 80 px tall, 2 of gap.
    pub(super) fn automation_row(&self, i: usize, cx: &mut Context<Self>) -> AnyElement {
        let a = &self.agents.automations;
        let Some(auto) = a.items.get(i) else { return Empty.into_any_element() };
        let now = now_ms();
        let drafting = self.automations.form.open && self.automations.form.v.id.is_none();
        let selected = !drafting && self.automations.selected.as_deref() == Some(auto.id.as_str());
        let last = a.last_run(&auto.id);
        let status = if !auto.enabled {
            ui::tag("Paused").text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_3).bg(FILL_3).rounded(px(5.)).px(px(6.)).py(px(1.)).into_any_element()
        } else if let Some(r) = last {
            let color = match r.status {
                RunStatus::Waiting => WAITING_TEXT,
                RunStatus::Running | RunStatus::Pending => ACCENT,
                RunStatus::Failed => FAILED,
                _ => TEXT_3,
            };
            div()
                .flex()
                .items_center()
                .gap(px(5.))
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(color)
                .child(glyph(r.status, 11., ("automation-last", i)))
                .child(last_label(r, now, &Local))
                .into_any_element()
        } else {
            div().text_size(px(12.)).font_weight(FontWeight::MEDIUM).text_color(TEXT_3).child("Never run").into_any_element()
        };
        let (id, enabled) = (auto.id.clone(), auto.enabled);
        let toggle = div()
            .id(("automation-toggle", i))
            .absolute()
            .top(px(9.))
            .right(px(10.))
            .cursor_pointer()
            .child(ui::toggle(enabled))
            .capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.toggle_automation(&id, !enabled, cx);
            }));
        let pick = auto.id.clone();
        let content = div()
            .flex()
            .flex_col()
            .gap(px(3.))
            .when(!auto.enabled, |d| d.opacity(0.55))
            .child(
                div()
                    .pr(px(40.))
                    .h(px(16.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.))
                    .text_color(TEXT_3)
                    .child(provider_icon(&auto.provider, 12., TEXT_3))
                    .child(div().truncate().child(format!("{} · {}", provider_name(&auto.provider), self.repo_name(&auto.folder)))),
            )
            .child(div().h(px(20.)).truncate().text_size(px(14.5)).font_weight(FontWeight::SEMIBOLD).child(auto.name.clone()))
            .child(
                div()
                    .h(px(17.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.5))
                    .text_color(TEXT_3)
                    .child(icon("clock", 13., TEXT_4))
                    .child(div().flex_1().min_w_0().truncate().child(trigger_line(auto, now, &Local)))
                    .child(div().flex_none().child(status)),
            );
        div()
            .w_full()
            .h(px(82.))
            .pb(px(2.))
            .child(
                div()
                    .id(("automation-row", i))
                    .relative()
                    .size_full()
                    .p(px(10.))
                    .rounded(px(10.))
                    .cursor_pointer()
                    .when(selected, |d| d.bg(FILL_3))
                    .when(!selected, |d| d.hover(|s| s.bg(FILL_2)))
                    .child(content)
                    .child(toggle)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_automation(pick.clone(), cx))),
            )
            .into_any_element()
    }
}
