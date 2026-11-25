use super::Activity;
use super::logic::COLS;
use crate::desktop::Desktop;
use gpui_kit::*;
use std::time::Instant;
use theme::*;

impl Render for Activity {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let columns: Vec<_> = COLS.into_iter().map(|col| self.activity_column(col, cx)).collect();
        // A click on the columns closes the drawer; cards and their buttons stop it, and the bar above keeps it.
        let body = div().id("activity-columns").flex_1().min_h_0().overflow_x_scroll().px(px(20.)).py(px(14.)).flex().gap(px(10.)).children(columns).on_click(
            cx.listener(|this, _: &ClickEvent, _, cx| {
                this.previewed = None;
                cx.notify();
            }),
        );
        let flights = self.activity_flights(cx);
        div().relative().flex_1().min_w_0().min_h_0().flex().flex_col().child(body).children(self.activity_panel(cx)).children(flights)
    }
}

impl Activity {
    /// Copies of the cards changing place, above the columns' clipping, each where its move has it now.
    fn activity_flights(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let now = Instant::now();
        let flights = {
            let mut motion = self.motion.borrow_mut();
            motion.reduce = cx.reduce_motion();
            motion.frame(now);
            motion.flights(now)
        };
        flights
            .into_iter()
            .filter_map(|(id, at, width)| {
                let card = self
                    .activity_card_body(self.grouped.item(&id)?, self.side.extras.get(&id)?, cx)
                    .w(width)
                    .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5), ui::shadow(rgba(0x00000066), 8., 24.)]);
                // Zero-sized, so `anchored` never pushes a copy that runs past the window's edge back inside it.
                let frames = div().size_0().on_children_prepainted(|_, window, _| window.request_animation_frame()).child(card.absolute().top_0().left_0());
                Some(deferred(anchored().position(at).child(frames)).with_priority(1).into_any_element())
            })
            .collect()
    }
}

impl Desktop {
    pub(crate) fn activity_page(&mut self, cx: &mut Context<Self>) -> Div {
        let side = self.activity_side(cx);
        self.activity.update(cx, |a, _| a.set(side));
        let filters = self.activity.update(cx, |a, cx| a.filters(cx));
        let title = div()
            .ml(px(2.))
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(7.))
            .text_size(px(13.))
            .font_weight(FontWeight::MEDIUM)
            .child(icon("activity", 14., TEXT_3))
            .child("Activity");
        let bar = self.detail_bar(title, filters, cx).h(px(44.));
        div().relative().flex_1().min_w_0().min_h_0().flex().flex_col().bg(CANVAS).child(bar).child(self.activity.clone())
    }
}
