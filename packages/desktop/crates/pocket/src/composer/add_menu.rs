use super::model_menu::{GAP, PAD};
use super::Draft;
use crate::desktop::Desktop;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::*;
use theme::*;

/// Which way the menu opens: down on the new session page, up from the chat's box at the window's foot.
#[derive(Clone, Copy)]
pub(crate) enum Opens {
    Down,
    Up,
}

/// The "+" button and, while `open`, its menu: Attach images, and Team, checked while set.
/// Where Team isn't offered (`team` is `None`) the button attaches images itself; a one-row menu would only cost a click.
/// `toggle` opens or closes the menu; `draft` reaches the box's draft.
pub(crate) fn add_button(
    id: impl Into<ElementId>,
    open: bool,
    team: Option<bool>,
    opens: Opens,
    draft: impl Fn(&mut Desktop) -> &mut Draft + Copy + 'static,
    toggle: impl Fn(&mut Desktop, &mut Window, &mut Context<Desktop>) + 'static,
    cx: &mut Context<Desktop>,
) -> Div {
    let button = div()
        .id(id)
        .size(px(32.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(10.))
        .cursor_pointer()
        .when(open, |d| d.bg(FILL_3))
        .hover(|s| s.bg(FILL_3))
        .active(|s| s.bg(FILL_4))
        .child(icon("plus", 17., TEXT_2))
        .when(team.is_none(), |d| d.tooltip(|window, cx| Tooltip::new("Attach images").build(window, cx)))
        .capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            if team.is_some() {
                toggle(this, window, cx);
            } else {
                attach_images(this, draft, window, cx);
            }
        }));
    div().relative().flex_none().child(button).children(open.then(|| menu(team, opens, draft, cx)))
}

fn attach_images(d: &mut Desktop, draft: impl Fn(&mut Desktop) -> &mut Draft + Copy + 'static, window: &mut Window, cx: &mut Context<Desktop>) {
    draft(d).menu = None;
    d.pick_path(false, window, cx, move |d, paths, _, _| {
        draft(d).drop_images(&paths);
    });
    cx.notify();
}

fn menu(team: Option<bool>, opens: Opens, draft: impl Fn(&mut Desktop) -> &mut Draft + Copy + 'static, cx: &mut Context<Desktop>) -> Div {
    let attach = ui::menu_row("composer-add-images", "file-plus", "Attach images", None).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| attach_images(this, draft, window, cx)));
    let team = team.map(|on| {
        ui::menu_pick("composer-add-team", icon("team", 14., TEXT_2), "Team", on).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            draft(this).toggle_team();
            cx.notify();
        }))
    });
    let menu = ui::pop(div().id("composer-add-menu"))
        .w(px(200.))
        .p(px(PAD))
        .flex()
        .flex_col()
        .gap(px(GAP))
        // Menus hang over what's under them; without this a click on one also lands there.
        .occlude()
        .child(attach)
        .children(team)
        .on_mouse_down_out(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            draft(this).menu = None;
            cx.notify();
        }));
    let menu = ui::menu_in("composer-add-menu-in", menu);
    match opens {
        Opens::Down => ui::dropdown_right(36., menu),
        Opens::Up => div().absolute().top(px(-6.)).right_0().child(deferred(anchored().anchor(Anchor::BottomRight).snap_to_window_with_margin(px(8.)).child(menu)).with_priority(1)),
    }
}
