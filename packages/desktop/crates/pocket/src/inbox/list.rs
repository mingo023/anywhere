use super::{FILTERS, Note, asks, heading, readable, step};
use crate::desktop::Desktop;
use crate::desktop::chrome::{Layout, column, drag_area, empty, past_lights, state};
use crate::util::{ago, now_ms};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use store::prefs::sidebar::{InboxFilter, InboxSort};
use ui::{self, Variant};

impl Desktop {
    fn on_inbox_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = step(&ev.keystroke.key, self.inbox.selected) else { return };
        self.select_note(i, cx);
        cx.stop_propagation();
    }

    pub fn inbox_list(&mut self, cx: &mut Context<Self>) -> Div {
        let notes = self.shown_notes();
        let total = notes.len();
        let urgent = self.store.sidebar.inbox_sort == InboxSort::Urgent;
        let headings: Vec<_> = (0..total).map(|i| heading(&notes, i).filter(|_| urgent)).collect();
        let now = now_ms();
        let header = drag_area(div())
            .h(px(52.))
            .pl(px(if self.layout == Layout::Compact { past_lights(self.store.appearance.zoom_factor(), 16.) } else { 16. }))
            .pr(px(10.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(4.))
            .child(div().flex_1().text_size(px(17.)).font_weight(FontWeight::BOLD).child("Inbox"))
            .child(self.inbox_filter(cx))
            .child(
                ui::button("mark-seen", Variant::Ghost, None, "Mark all seen").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
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
        for (i, (n, head)) in notes.into_iter().zip(headings).enumerate() {
            if let Some((label, count)) = head {
                list = list.child(section(&label, count));
            }
            list = list.child(self.note_row(i, n, now, cx));
        }
        if total == 0 {
            list = list.child(empty("Nothing needs you."));
        }
        column().child(header).child(list)
    }

    fn inbox_filter(&mut self, cx: &mut Context<Self>) -> Div {
        let (chosen, open) = (self.inbox.filter, self.inbox.filter_open);
        let button = ui::icon_button_sized("inbox-filter", "filter", 28., if chosen == InboxFilter::All { TEXT_2 } else { ACCENT }).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.inbox.filter_open = !open;
            cx.notify();
        }));
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
                    (this.inbox.filter, this.inbox.filter_open) = (filter, false);
                    this.select_note(0, cx);
                }))
        });
        let menu = ui::menu_in("inbox-filter-in", ui::pop(div()).min_w(px(140.)).p(px(4.)).flex().flex_col().children(rows));
        div().relative().child(button).when(open, |d| d.child(ui::dropdown_right(32., menu)))
    }

    fn note_row(&self, i: usize, n: Note, now: i64, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = i == self.inbox.selected;
        let mark = ui::indicator(("note-mark", i), Some(state(n.status, 0, 0)));
        let project = self.project_name(&n.agent);
        let provider = self.agents.get(&n.agent).map(|a| provider_name(&a.provider)).unwrap_or("Shell");
        let (agent, markable) = (n.agent.clone(), !asks(&n));
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
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(project))
                            .child(div().text_color(TEXT_6).child("·"))
                            .child(div().flex_1().truncate().child(provider))
                            .child(div().text_color(TEXT_4).child(ago(n.at, now))),
                    )
                    .child(div().truncate().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(n.title))
                    .child(div().truncate().text_size(px(12.)).text_color(TEXT_2).child(n.subtitle)),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                if markable {
                    this.outbox.seen(std::slice::from_ref(&agent));
                }
                this.focus_agent(&agent, window, cx);
            }))
    }
}
