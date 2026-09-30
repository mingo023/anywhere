use super::{Note, notes, readable, step};
use crate::desktop::Desktop;
use crate::desktop::chrome::{column, drag_area, empty};
use crate::status::Status;
use crate::util::{ago, now_ms};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{self, Variant, icon_button};

impl Desktop {
    fn on_inbox_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = step(&ev.keystroke.key, self.inbox.selected) else { return };
        self.select_note(i, cx);
        cx.stop_propagation();
    }

    pub fn inbox_list(&mut self, cx: &mut Context<Self>) -> Div {
        let notes = notes(&self.agents);
        let total = notes.len();
        let asks = notes.iter().filter(|n| n.status == Status::NeedsYou).count();
        let now = now_ms();
        let header = drag_area(div())
            .h(px(52.))
            .pl(px(16.))
            .pr(px(10.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(4.))
            .child(div().flex_1().text_size(px(17.)).font_weight(FontWeight::BOLD).child("Inbox"))
            .child(icon_button("inbox-filter", "filter"))
            .child(
                ui::button("mark-read", Variant::Ghost, None, "Mark all read").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        let ids = readable(crate::inbox::notes(&this.agents));
                        if !ids.is_empty() {
                            this.outbox.seen(&ids);
                        }
                        this.select_note(0, cx);
                    })),
            );
        let section = |label: &str, count: usize| {
            div()
                .pt(px(12.))
                .pb(px(4.))
                .px(px(12.))
                .flex()
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(TEXT_3)
                .child(div().flex_1().child(label.to_string()))
                .child(div().font_family(MONO).font_weight(FontWeight::MEDIUM).child(count.to_string()))
        };
        let mut list = div()
            .id("notes")
            .flex_1()
            .overflow_y_scroll()
            .track_focus(&self.inbox.focus)
            .on_key_down(cx.listener(Self::on_inbox_key))
            .p(px(8.))
            .flex()
            .flex_col()
            .gap(px(2.));
        for (i, n) in notes.into_iter().enumerate() {
            if i == 0 && asks > 0 {
                list = list.child(section("NEEDS YOU", asks));
            }
            if i == asks {
                list = list.child(section("DONE", total - asks));
            }
            list = list.child(self.note_row(i, n, now, cx));
        }
        if total == 0 {
            list = list.child(empty("Nothing needs you."));
        }
        column().child(header).child(list)
    }

    fn note_row(&self, i: usize, n: Note, now: i64, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = i == self.inbox.selected;
        let (glyph, color) = match n.status {
            Status::NeedsYou => ("shield", WAITING_TEXT),
            Status::Failed => ("x", FAILED),
            _ => ("check", ACCENT),
        };
        let project = self.project_name(&n.agent);
        let provider = self.agents.get(&n.agent).map(|a| provider_name(&a.provider)).unwrap_or("Shell");
        div()
            .id(("note", i))
            .px(px(12.))
            .py(px(10.))
            .flex()
            .flex_none()
            .gap(px(12.))
            .rounded(px(12.))
            .cursor_pointer()
            .when(selected, |d| d.bg(FILL_3))
            .when(!selected, |d| d.hover(|s| s.bg(FILL_1)))
            .child(
                div()
                    .size(px(26.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.))
                    .bg(SURFACE)
                    .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)])
                    .child(icon(glyph, 13., color)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(12.))
                            .text_color(TEXT_2)
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(project))
                            .child(div().text_color(TEXT_6).child("·"))
                            .child(div().flex_1().truncate().child(provider))
                            .child(div().text_color(TEXT_4).child(ago(n.at, now))),
                    )
                    .child(div().truncate().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(n.title))
                    .child(div().truncate().text_size(px(12.)).text_color(TEXT_2).child(n.subtitle)),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.select_note(i, cx);
                window.focus(&this.inbox.focus, cx);
            }))
    }
}
