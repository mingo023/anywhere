use crate::desktop::Desktop;
use crate::star::glyph;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

/// The card's fixed height, so it can grow in from nothing: text that would wrap truncates instead.
const CARD: f32 = 94.;
const GAP: f32 = 8.;
const RISE: f32 = 10.;

impl Desktop {
    /// "Enjoying Anywhere?" with a Star button, once, from a few days after the first launch.
    pub(super) fn star_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.star.visible() {
            return None;
        }
        let (at, thanked, reduce_motion) = (self.star.at(), self.star.thanked, cx.reduce_motion());
        let dismiss = div()
            .id("aside-star-dismiss")
            .size(px(20.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(5.))
            .hover(|s| s.bg(FILL_3))
            .active(|s| s.opacity(0.6))
            .child(icon("x", 12., TEXT_3))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close_star(cx)));
        let copy = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(div().h(px(18.)).truncate().text_size(px(12.5)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child("Enjoying Anywhere?"))
            .child(div().h(px(16.)).truncate().text_size(px(12.)).text_color(TEXT_2).child("A star helps others find it."));
        let label = if thanked {
            div().child("Thank you!").with_animation("aside-star-thanks", Animation::new(std::time::Duration::from_millis(180)), |d, t| d.opacity(t)).into_any_element()
        } else {
            div().flex().items_center().gap(px(6.)).child(icon("star-fill", 12., ui::Variant::Primary.fg())).child("Star on GitHub").into_any_element()
        };
        let star = ui::button("aside-star", ui::Variant::Primary, None, label)
            .mt(px(10.))
            .w_full()
            .h(px(28.))
            .rounded(px(14.))
            .justify_center()
            .text_size(px(12.5))
            .when(thanked, |b| b.cursor_default())
            .when(!thanked, |b| b.on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.star_on_github(cx))));
        let card = div()
            .id("aside-star-card")
            .relative()
            .top(px(if reduce_motion { 0. } else { RISE * (1. - at) }))
            .opacity(if reduce_motion { at } else { (at * 1.4 - 0.2).clamp(0., 1.) })
            .mt(px(GAP))
            .mx(px(2.))
            .h(px(CARD))
            .py(px(10.))
            .pl(px(12.))
            .pr(px(10.))
            .flex()
            .flex_col()
            .rounded(px(12.))
            .bg(SIDE_NOTICE)
            .shadow(vec![ui::ring(FILL_3, 0.5)])
            .child(div().flex().items_start().gap(px(8.)).child(div().mt(px(1.)).child(glyph("aside-star-pop", thanked, 16., reduce_motion))).child(copy).child(dismiss))
            .child(star);
        // Clipped only while it grows or shrinks, so the settled card keeps its ring.
        Some(div().flex_none().when(at < 1., |d| d.h(px((GAP + CARD + 1.) * at)).overflow_hidden()).child(card).into_any_element())
    }
}
