use chat::Action;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Duration;
use theme::*;

const SIZE: f32 = 34.;

fn stop_square(color: impl Into<Hsla>) -> Div {
    div().size(px(10.)).rounded(px(2.5)).bg(color.into())
}

fn button(id: ElementId, tip: &'static str) -> Stateful<Div> {
    div().id(id).h(px(SIZE)).min_w(px(SIZE)).flex_none().flex().items_center().justify_center().gap(px(6.)).rounded_full().tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
}

/// The box's Send / Steer / Queue / Stop: a working agent's empty box stops it, typing into it steers, and queues once it's stopping. `blocked` when the agent can't take it now.
pub(crate) fn send_button(id: ElementId, action: Action, empty: bool, blocked: bool) -> Stateful<Div> {
    let b = match action {
        Action::Stop => {
            let ring = icon("send-ring", SIZE, ON_TEXT).absolute().top_0().left_0().with_animation("send-ring", Animation::new(Duration::from_secs(1)).repeat().with_max_fps(30.), |s, t| {
                s.with_transformation(Transformation::rotate(percentage(t)))
            });
            button(id, "Stop · esc").relative().bg(TEXT).child(ring).child(stop_square(ON_TEXT))
        }
        Action::Stopping => button(id, "Stopping…").bg(TEXT).child(stop_square(ON_TEXT)),
        Action::Steer => button(id, "Send now · ⏎").bg(TEXT).child(icon("arrow-up", 16., ON_TEXT)),
        Action::Queue => button(id, "Queue · ⏎")
            .pl(px(14.))
            .pr(px(12.))
            .bg(TEXT)
            .text_size(px(12.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(ON_TEXT)
            .child("Queue")
            .child(icon("list", 16., ON_TEXT)),
        Action::Send if empty => button(id, "Send · ⏎").bg(FILL_4).child(icon("arrow-up", 16., TEXT_4)),
        Action::Send => button(id, "Send · ⏎").bg(TEXT).child(icon("arrow-up", 16., ON_TEXT)),
    };
    b.when(blocked, |b| b.opacity(0.35)).when(!blocked && !(action == Action::Send && empty), |b| b.cursor_pointer().hover(|s| s.opacity(0.85)).active(|s| s.opacity(0.7)))
}

/// Stop beside Steer, for an agent working while you type.
pub(super) fn ghost_stop(id: ElementId) -> Stateful<Div> {
    button(id, "Stop · esc").bg(FILL_3).cursor_pointer().hover(|s| s.bg(FILL_4)).child(stop_square(TEXT))
}
