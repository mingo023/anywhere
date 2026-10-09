use crate::desktop::Desktop;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::rc::Rc;
use theme::*;

/// `trigger` opening a menu of `options` while `settings.menu` is `id`; `pick` gets the chosen value.
pub(crate) fn dropdown(
    id: &'static str,
    trigger: Stateful<Div>,
    options: Vec<(String, String)>,
    chosen: &str,
    d: &Desktop,
    pick: impl Fn(&mut Desktop, String, &mut Context<Desktop>) + 'static,
    cx: &mut Context<Desktop>,
) -> Div {
    let open = d.settings.menu == Some(id);
    let trigger = trigger.cursor_pointer().on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
        this.settings.menu = (!open).then_some(id);
        cx.notify();
    }));
    let pick = Rc::new(pick);
    let rows = options.into_iter().enumerate().map(|(i, (value, label))| {
        let pick = pick.clone();
        let on = value == chosen;
        div()
            .id((id, i))
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
            .child(div().w(px(14.)).flex_none().when(on, |d| d.child(icon("check", 13., TEXT))))
            .child(label)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.settings.menu = None;
                pick(this, value.clone(), cx);
                this.save_soon(cx);
                cx.notify();
            }))
    });
    let menu = ui::menu_in(SharedString::from(format!("{id}-menu-in")), ui::pop(div()).min_w(px(180.)).max_h(px(320.)).p(px(4.)).flex().flex_col().children(rows));
    div().relative().child(trigger).when(open, |d| d.child(ui::dropdown_right(30., menu)))
}
