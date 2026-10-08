use super::Menu;
use crate::desktop::Desktop;
use crate::desktop::chrome::drag_area;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{self, Variant};

pub fn card() -> Div {
    div().rounded(px(12.)).bg(SURFACE).shadow(ui::row_shadow())
}

/// A small heading over `body`.
pub fn section(label: &str, body: impl IntoElement) -> Div {
    div().flex().flex_col().gap(px(8.)).child(div().pl(px(4.)).text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_3).child(label.to_string())).child(body)
}

/// Pill buttons of the detail bars: `Primary` is the inverse one.
pub fn pill(id: impl Into<ElementId>, v: Variant, icon_name: Option<&str>, label: &str) -> Stateful<Div> {
    ui::button(id, v, icon_name, label.to_string()).h(px(28.)).px(px(12.)).rounded(px(14.)).font_weight(FontWeight::MEDIUM)
}

/// Equal-width choices in a tray; `roomy` is the larger one the trigger card uses.
pub fn segmented(id: &'static str, labels: &[&'static str], selected: usize, roomy: bool, on: fn(&mut Desktop, usize, &mut Context<Desktop>), cx: &mut Context<Desktop>) -> Div {
    let (pad, h, gutter, inner, outer, size) = if roomy { (3., 26., 12., 8., 11., 12.5) } else { (2., 24., 9., 7., 9., 12.) };
    div().flex().flex_none().p(px(pad)).gap(px(2.)).rounded(px(outer)).bg(FILL_3).children(labels.iter().enumerate().map(|(i, label)| {
        div()
            .id((id, i))
            .h(px(h))
            .px(px(gutter))
            .flex()
            .items_center()
            .rounded(px(inner))
            .cursor_pointer()
            .whitespace_nowrap()
            .text_size(px(size))
            .when(i == selected, |d| d.bg(SURFACE).shadow(ui::row_shadow()).text_color(TEXT).font_weight(FontWeight::SEMIBOLD))
            .when(i != selected, |d| d.text_color(TEXT_2).font_weight(FontWeight::MEDIUM))
            .child(*label)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| on(this, i, cx)))
    }))
}

/// "parent › title".
pub fn crumbs(parent: &str, title: impl IntoElement) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(6.))
        .child(div().flex_none().text_color(TEXT_3).child(parent.to_string()))
        .child(icon("chevron-right", 12., TEXT_4))
        .child(div().min_w_0().truncate().font_weight(FontWeight::SEMIBOLD).child(title))
}

/// A card's key and value rows, split by hairlines.
pub fn details(rows: Vec<(&str, AnyElement)>) -> Div {
    card().py(px(4.)).flex().flex_col().children(rows.into_iter().enumerate().map(|(i, (key, value))| {
        div()
            .px(px(16.))
            .py(px(9.))
            .flex()
            .gap(px(12.))
            .text_size(px(13.))
            .line_height(px(18.))
            .when(i > 0, |d| d.border_t(px(0.5)).border_color(HAIRLINE))
            .child(div().w(px(140.)).flex_none().text_color(TEXT_3).child(key.to_string()))
            .child(div().flex_1().min_w_0().child(value))
    }))
}

impl Desktop {
    /// A floating menu of `width` that a click outside closes.
    pub(super) fn popup(&self, id: &'static str, width: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        ui::pop(div().id(id)).w(px(width)).p(px(5.)).flex().flex_col().occlude().on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
            this.close_menus();
            cx.notify();
        }))
    }

    /// `button` as the toggle of `menu`; the press is caught so the menu's own outside-click doesn't close it first.
    pub(super) fn menu_toggle(&self, button: Stateful<Div>, menu: Menu, cx: &mut Context<Self>) -> Stateful<Div> {
        button.capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            cx.stop_propagation();
            this.toggle_menu(menu, cx);
        }))
    }

    pub(super) fn detail_bar(&self, crumb: Div, right: impl IntoElement, cx: &mut Context<Self>) -> Div {
        let (pad, toggle) = self.bar_start(14., cx);
        drag_area(div())
            .h(px(42.))
            .flex_none()
            .pl(px(pad))
            .pr(px(12.))
            .flex()
            .items_center()
            .gap(px(8.))
            .border_b(px(0.5))
            .border_color(SEPARATOR)
            .text_size(px(13.))
            .children(toggle)
            .child(crumb)
            .child(div().flex().flex_none().items_center().gap(px(8.)).child(right))
    }

    /// The scrolling column every detail page is: centred, 760 wide, `gap` between blocks.
    pub(super) fn detail_body(&self, id: &'static str, gap: f32, body: impl IntoIterator<Item = AnyElement>) -> Stateful<Div> {
        div()
            .id(id)
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(div().max_w(px(760.)).mx_auto().pt(px(28.)).px(px(32.)).pb(px(48.)).flex().flex_col().gap(px(gap)).children(body))
    }
}
