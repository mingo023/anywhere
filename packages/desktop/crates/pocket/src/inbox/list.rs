use super::{FILTERS, Inbox, InboxEvent, Row, asks, step};
use crate::desktop::chrome::{column, drag_area, empty, state};
use crate::util::{ago, now_ms};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::prefs::sidebar::InboxFilter;
use theme::*;
use ui::{self, Variant};

impl Render for Inbox {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let total = self.rows.len();
        let now = now_ms();
        let header = drag_area(div())
            .h(px(52.))
            .pl(px(self.lead))
            .pr(px(10.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(4.))
            .child(div().flex_1().text_size(px(17.)).font_weight(FontWeight::BOLD).child("Inbox"))
            .child(self.inbox_filter(cx))
            .child(
                ui::button("mark-seen", Variant::Ghost, None, "Mark all seen")
                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(InboxEvent::MarkAllSeen))),
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
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .p(px(8.))
            .flex()
            .flex_col()
            .gap(px(2.));
        for (i, row) in self.rows.iter().enumerate() {
            if let Some((label, count)) = &row.heading {
                list = list.child(section(label, *count));
            }
            list = list.child(self.note_row(i, row, now, cx));
        }
        if total == 0 {
            list = list.child(empty("Nothing needs you."));
        }
        column().w(px(self.width)).child(header).child(list)
    }
}

impl Inbox {
    fn on_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = step(&ev.keystroke.key, self.selected) else { return };
        cx.emit(InboxEvent::Select(i));
        cx.stop_propagation();
    }

    fn inbox_filter(&mut self, cx: &mut Context<Self>) -> Div {
        let (chosen, open) = (self.filter, self.filter_open);
        let button = ui::icon_button_sized("inbox-filter", "filter", 28., if chosen == InboxFilter::All { TEXT_2 } else { ACCENT }).on_click(cx.listener(
            move |this, _: &ClickEvent, _, cx| {
                this.filter_open = !open;
                cx.notify();
            },
        ));
        let rows = FILTERS.into_iter().map(|(filter, label)| {
            div()
                .id(label)
                .h(px(26.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(5.))
                .cursor_pointer()
                .hover(|d| d.bg(FILL_2))
                .text_size(px(13.))
                .text_color(TEXT)
                .child(div().w(px(14.)).flex_none().when(filter == chosen, |d| d.child(icon("check", 13., TEXT))))
                .child(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    (this.filter, this.filter_open) = (filter, false);
                    cx.emit(InboxEvent::Select(0));
                }))
        });
        let menu = ui::menu_in("inbox-filter-in", ui::pop(div()).min_w(px(140.)).p(px(4.)).flex().flex_col().children(rows));
        div().relative().child(button).when(open, |d| d.child(ui::dropdown_right(32., menu)))
    }

    fn note_row(&self, i: usize, row: &Row, now: i64, cx: &mut Context<Self>) -> Stateful<Div> {
        let Row { note: n, project, provider, .. } = row;
        let selected = i == self.selected;
        let mark = ui::indicator(("note-mark", i), Some(state(n.status, 0, 0)));
        let (agent, mark_seen) = (n.agent.clone(), !asks(n));
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
                    .children(mark),
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
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(project.clone()))
                            .child(div().text_color(TEXT_6).child("·"))
                            .child(div().flex_1().truncate().child(provider.clone()))
                            .child(div().text_color(TEXT_4).child(ago(n.at, now))),
                    )
                    .child(div().truncate().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(n.title.clone()))
                    .child(div().truncate().text_size(px(12.)).text_color(TEXT_2).child(n.subtitle.clone())),
            )
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(InboxEvent::Open { agent: agent.clone(), mark_seen })))
    }
}
