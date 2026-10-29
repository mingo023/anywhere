//! The zoom and window buttons over a diagram's top-right corner while the pointer is over it, as markdown-preview's.

use gpui_kit::*;
use std::rc::Rc;
use theme::*;

/// What a button asks of the diagram.
#[derive(Clone, Copy)]
pub(super) enum Act {
    Zoom(f32),
    Fit,
    Fullscreen,
}

const INSET: f32 = 8.;

/// The zoom out, `level` and zoom in pill, then fit and, when `fullscreen`, the button opening the diagram's window.
pub(super) fn hud(place: u64, level: String, fullscreen: bool, on: impl Fn(Act, &mut Window, &mut App) + 'static) -> Div {
    let on = Rc::new(on);
    let id = |name: &str| SharedString::from(format!("mermaid-{name}-{place}"));
    let press = |act: Act, e: Stateful<Div>| {
        let on = on.clone();
        e.on_click(move |_, window, cx| on(act, window, cx))
    };
    let button = |glyph: &str, act: Act| press(act, ui::group_button(id(glyph), glyph));
    let level = ui::control(div().id(id("level")))
        .h(px(30.))
        .min_w(px(44.))
        .px(px(4.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(15.))
        .cursor_pointer()
        .hover(|s| s.bg(FILL_3))
        .active(|s| s.bg(FILL_4))
        .text_size(px(12.))
        .text_color(TEXT)
        .child(level);
    let zoom = [button("minus", Act::Zoom(1. / super::viewport::STEP)), press(Act::Fit, level), button("plus", Act::Zoom(super::viewport::STEP))];
    let actions = [Some(button("fit", Act::Fit)), fullscreen.then(|| button("fullscreen", Act::Fullscreen))];
    div()
        .absolute()
        .top(px(INSET))
        .right(px(INSET))
        .flex()
        .gap(px(8.))
        // Not a drag or double-click starting on the diagram beneath.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(ui::icon_group(zoom))
        .child(ui::icon_group(actions.into_iter().flatten()))
}
