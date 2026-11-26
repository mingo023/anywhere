use super::card::{Btn, btn};
use super::{Card, Inbox, InboxEvent, Tab, mark_all_seen};
use crate::desktop::chrome::id;
use crate::util::{ago, now_ms};
use gpui_kit::*;
use theme::*;
use ui::Segment;

const WIDTH: f32 = 440.;
/// What the popover leaves of the window's height: the title bar above it and a margin below.
const CLEARANCE: f32 = 72.;

pub(crate) fn hung(top: f32, left: f32, inbox: Entity<Inbox>) -> Div {
    let menu = ui::menu_in("inbox-in", div().child(inbox));
    div().absolute().top(px(top)).left(px(left)).child(deferred(anchored().snap_to_window_with_margin(px(8.)).child(menu)).with_priority(1))
}

fn empty(text: &'static str) -> Div {
    div()
        .py(px(46.))
        .px(px(16.))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(10.))
        .text_size(px(13.))
        .text_color(TEXT_3)
        .child(div().size(px(36.)).rounded(px(18.)).flex().items_center().justify_center().bg(SUCCESS_BG).child(icon("check", 18., SUCCESS_TEXT)))
        .child(text)
}

impl Render for Inbox {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (needs, done): (Vec<&Card>, Vec<&Card>) = self.cards.iter().partition(|c| Tab::of(c.status) == Tab::NeedsYou);
        let segment = |value, label: &'static str, n: usize| Segment { icon: None, value, label: label.into(), badge: Some(n.to_string()) };
        let tabs = ui::segmented(
            vec![segment(Tab::NeedsYou, "Needs you", needs.len()), segment(Tab::Updates, "Updates", done.len())],
            self.tab,
            true,
            false,
            |this: &mut Self, tab, cx| this.show_tab(tab, cx),
            cx,
        );
        let header = div()
            .h(px(48.))
            .flex_none()
            .pl(px(16.))
            .pr(px(12.))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(div().flex_1().text_size(px(15.)).font_weight(FontWeight::BOLD).child("Inbox"))
            .child(tabs);
        let now = now_ms();
        let list = div().id("inbox-list").flex_1().min_h_0().overflow_y_scroll().px(px(10.)).pt(px(4.)).pb(px(10.)).flex().flex_col();
        let (list, foot) = match self.tab {
            Tab::NeedsYou => {
                let list = if needs.is_empty() {
                    let to_updates = (!done.is_empty()).then(|| {
                        let label = if done.len() == 1 { "See 1 update".to_string() } else { format!("See {} updates", done.len()) };
                        btn("inbox-to-updates", Btn::Secondary, label).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.show_tab(Tab::Updates, cx)))
                    });
                    list.child(empty("Nothing needs you.").children(to_updates))
                } else {
                    list.gap(px(8.)).children(needs.iter().map(|c| self.card_view(c, now, cx)))
                };
                let note = if self.observe_only { "This desktop only observes these agents" } else { "Agents wait until you answer" };
                (list, Some(div().child(note)))
            }
            Tab::Updates if done.is_empty() => (list.child(empty("No updates.")), None),
            Tab::Updates => {
                let ids = mark_all_seen(&self.cards);
                let all = btn("inbox-all-seen", Btn::Ghost, "Mark all seen")
                    .ml(px(-8.))
                    .text_size(px(11.5))
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(InboxEvent::Seen(ids.clone()))));
                (list.gap(px(1.)).children(done.iter().map(|c| self.update_row(c, now, cx))), Some(div().child(all)))
            }
        };
        let foot = foot.map(|d| {
            d.h(px(34.)).flex_none().px(px(14.)).flex().items_center().text_size(px(11.5)).text_color(TEXT_3).border_t(px(0.5)).border_color(SEPARATOR)
        });
        ui::pop(div().id("inbox"))
            .w(px(WIDTH))
            .max_h(window.viewport_size().height - px(CLEARANCE))
            .overflow_hidden()
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| this.shut(cx)))
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                if e.keystroke.key == "escape" {
                    if !this.cancel_reply(cx) {
                        this.shut(cx);
                    }
                    cx.stop_propagation();
                }
            }))
            .child(header)
            .child(list)
            .children(foot)
    }
}

impl Inbox {
    fn update_row(&self, c: &Card, now: i64, cx: &mut Context<Self>) -> Stateful<Div> {
        let (open, seen) = (c.agent.clone(), c.agent.clone());
        let group = SharedString::from(format!("inbox-update:{}", c.agent));
        let action = |name: ElementId, glyph: &str| {
            div()
                .id(name)
                .size(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.))
                .cursor_pointer()
                .bg(SURFACE)
                .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)])
                .child(icon(glyph, 14., TEXT_2))
        };
        let acts = div()
            .absolute()
            .top(px(6.))
            .right(px(6.))
            .flex()
            .gap(px(4.))
            .invisible()
            .group_hover(group.clone(), |s| s.visible())
            .child(action(id(format!("inbox-seen:{seen}")), "check").on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                cx.emit(InboxEvent::Seen(vec![seen.clone()]))
            })))
            .child(action(id(format!("inbox-update-open:{open}")), "external"));
        let open = c.agent.clone();
        div()
            .id(id(format!("inbox-update:{}", c.agent)))
            .group(group)
            .relative()
            .h(px(40.))
            .px(px(10.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(10.))
            .rounded(px(9.))
            .cursor_pointer()
            .hover(|s| s.bg(FILL_1))
            .child(div().w(px(16.)).flex().justify_center().child(icon("check", 13., SUCCESS)))
            .child(div().flex_1().min_w_0().truncate().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(c.title.clone()))
            .child(div().flex_none().text_size(px(12.)).text_color(TEXT_3).child(c.project.clone()))
            .child(div().w(px(28.)).flex_none().text_right().text_size(px(12.)).text_color(TEXT_4).child(ago(c.at, now)))
            .child(acts)
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(InboxEvent::Open(open.clone()))))
    }
}
