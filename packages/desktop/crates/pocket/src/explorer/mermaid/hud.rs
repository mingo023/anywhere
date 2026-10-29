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

/// From the card's top and right edges, as markdown-preview's.
const INSET: f32 = 6.5;
const BUTTON: f32 = 24.;
/// markdown-preview's glass pill, opaque enough to read over the diagram.
const PILL: Token = Token::new(0xf5f5f5e6, 0x1c1c1ce6);

/// The zoom out, `level` and zoom in pill, then fit and, when `fullscreen`, the button opening the diagram's window.
pub(super) fn hud(place: u64, level: String, fullscreen: bool, on: impl Fn(Act, &mut Window, &mut App) + 'static) -> Div {
    let on = Rc::new(on);
    let button = |name: &str, width: f32, act: Act| {
        let on = on.clone();
        ui::control(div().id(SharedString::from(format!("mermaid-{name}-{place}"))))
            .w(px(width))
            .h(px(BUTTON))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(BUTTON / 2.))
            .cursor_pointer()
            .hover(|s| s.bg(Hsla::from(TEXT).opacity(0.1)))
            .active(|s| s.bg(Hsla::from(TEXT).opacity(0.16)))
            .on_click(move |_, window, cx| on(act, window, cx))
    };
    let glyph = |name: &str, act: Act| button(name, BUTTON, act).child(icon(name, 13., TEXT));
    let pill = |buttons: Vec<Stateful<Div>>| div().h(px(32.)).p(px(4.)).flex().items_center().gap(px(2.)).rounded(px(16.)).bg(PILL).shadow(vec![ui::ring(SEPARATOR, 0.5), ui::shadow(rgba(0x0000001f), 2., 8.)]).children(buttons);
    let level = button("level", 44., Act::Fit).text_size(px(11.)).text_color(TEXT).child(level);
    let zoom = vec![glyph("minus", Act::Zoom(1. / super::viewport::STEP)), level, glyph("plus", Act::Zoom(super::viewport::STEP))];
    let actions = [Some(glyph("fit", Act::Fit)), fullscreen.then(|| glyph("fullscreen", Act::Fullscreen))].into_iter().flatten().collect();
    div()
        .absolute()
        .top(px(INSET))
        .right(px(INSET))
        .flex()
        .gap(px(8.))
        // Not a drag or double-click starting on the diagram beneath.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(pill(zoom))
        .child(pill(actions))
}
