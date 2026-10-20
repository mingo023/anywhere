use super::{meta, transcript, user_bubble};
use crate::composer::split_images;
use crate::desktop::Desktop;
use chat::Row;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

/// Lines a stuck prompt shows until it is opened, so a long one never takes over the chat.
const FOLDED_LINES: usize = 2;
const SIZE: f32 = 14.;
const LINE: f32 = 20.;
const PAD_X: f32 = 12.;
const PAD_Y: f32 = 10.;
const RADIUS: f32 = 12.;
/// The row under a cut-off prompt that holds Show more, padded as the card's top is.
const MORE_ROW: f32 = LINE + PAD_Y;
/// Past this an opened prompt scrolls.
const OPEN_MAX_HEIGHT: f32 = 160.;
/// The time the prompt was sent, at the end of its first line.
const TIME_SIZE: f32 = 12.;
const TIME_GAP: f32 = 8.;
/// The card's padding and border, on both sides.
const INSET: f32 = 2. * (PAD_X + 1.);
/// The transcript's side padding, which the card lines up with.
const SIDE: f32 = 32.;

/// The prompt row whose turn the top of the list is in, once that prompt has scrolled up past the top: none while it
/// shows from its top, or above the first prompt. `top` is the row at the list's top and `offset` how far into it.
pub(super) fn stuck(rows: &[Row], top: usize, offset: f32) -> Option<usize> {
    let last = top.min(rows.len().checked_sub(1)?);
    let ix = rows[..=last].iter().rposition(|r| matches!(r, Row::Prompt { .. }))?;
    (ix < top || offset > 0.).then_some(ix)
}

/// The row at the top of a list scrolled to its end, and how far into it, from each row's height to the end and the
/// height its rows show in. Rows too few to fill it show from the first.
pub(super) fn top_at_end(count: usize, shown: f32, from: impl Fn(usize) -> Option<f32>) -> Option<(usize, f32)> {
    for ix in (0..count).rev() {
        let height = from(ix)?;
        if height >= shown {
            return Some((ix, height - shown));
        }
    }
    Some((0, 0.))
}

/// The next prompt under the list's top, which pushes the card up as it arrives.
fn next_prompt(rows: &[Row], top: usize) -> Option<usize> {
    rows.iter().enumerate().skip(top + 1).find(|(_, r)| matches!(r, Row::Prompt { .. } | Row::Pending(_))).map(|(ix, _)| ix)
}

/// The card's height for a prompt that wraps to `lines` lines. It hangs from the top, so only its bottom has a border.
fn height(lines: usize, open: bool) -> f32 {
    let text = if open { (lines as f32 * LINE).min(OPEN_MAX_HEIGHT) } else { lines.min(FOLDED_LINES) as f32 * LINE };
    1. + PAD_Y + text + if lines > FOLDED_LINES { MORE_ROW } else { PAD_Y }
}

/// How far the card moves up so the next prompt, `next` px under the list's top, pushes it out rather than slides
/// under it; `bottom` is where the card ends.
fn pushed(bottom: f32, next: Option<f32>) -> f32 {
    next.map_or(0., |next| (next - bottom).min(0.))
}

/// A prompt hung from the top of the chat: cut to two lines, the second fading out above Show more, with the time it
/// was sent at the end of its first line.
#[allow(clippy::too_many_arguments)]
fn card(
    id: &str,
    text: &str,
    time: Option<(String, f32)>,
    long: bool,
    open: bool,
    width: f32,
    top: f32,
    on_jump: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_more: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    // The prompt's own bubble, which is opaque, so the transcript scrolls under it unseen.
    let bg = Hsla::from(BUBBLE);
    // Quiet, as a prompt bubble's own Show more.
    let more = div()
        .id(SharedString::from(format!("sticky-more-{id}")))
        .group("sticky-more")
        .absolute()
        .right(px(PAD_X))
        .bottom(px(PAD_Y))
        .h(px(LINE))
        .flex()
        .items_center()
        .gap(px(5.))
        .text_color(TEXT_2)
        .cursor_pointer()
        .hover(|d| d.text_color(TEXT))
        .on_click(move |e, window, cx| {
            cx.stop_propagation();
            on_more(e, window, cx);
        })
        .child(if open { "Show less" } else { "Show more" })
        .child(
            icon("chevron-down", 12., TEXT_2)
                .when(open, |i| i.with_transformation(Transformation::rotate(radians(std::f32::consts::PI))))
                .group_hover("sticky-more", |s| s.text_color(TEXT)),
        );
    let room = time.as_ref().map_or(0., |(_, width)| *width);
    let time = time.map(|(time, _)| div().absolute().top(px(PAD_Y)).right(px(PAD_X)).text_size(px(TIME_SIZE)).line_height(px(LINE)).text_color(TEXT_2).child(time));
    let card = div()
        .id(SharedString::from(format!("sticky-{id}")))
        // An opened prompt scrolls itself; a folded one lets the wheel through to the transcript.
        .when(open, |d| d.occlude())
        .when(!open, |d| d.block_mouse_except_scroll())
        .relative()
        .w_full()
        .px(px(PAD_X))
        .pt(px(PAD_Y))
        .pb(px(if long { MORE_ROW } else { PAD_Y }))
        // Flush with the top, so no transcript shows over it.
        .rounded_b(px(RADIUS))
        .border_x_1()
        .border_b_1()
        .border_color(hairline(0.2))
        .bg(bg)
        .shadow(vec![ui::shadow(rgba(0x0000002e), 4., 14.), ui::shadow(rgba(0x0000001f), 1., 3.)])
        .text_size(px(SIZE))
        .line_height(px(LINE))
        .text_color(TEXT)
        .cursor_pointer()
        .on_click(on_jump)
        // The text keeps clear of the time on every line, not only the first, as `wrapped_lines` counts it. A row of the
        // two would wrap the text at the row's width.
        .child(
            div()
                .id(SharedString::from(format!("sticky-text-{id}")))
                .pr(px(room))
                .when(open, |d| d.max_h(px(OPEN_MAX_HEIGHT)).overflow_y_scroll())
                // A height, not a line clamp: a clamp cuts each paragraph on its own.
                .when(!open, |d| d.max_h(px(FOLDED_LINES as f32 * LINE)).overflow_hidden())
                .child(user_bubble::styled(&format!("sticky-{id}"), text)),
        )
        .children(time)
        .when(long && !open, |d| {
            d.child(div().absolute().left_0().right_0().top(px(PAD_Y + LINE)).h(px(LINE)).bg(linear_gradient(
                180.,
                linear_color_stop(bg.opacity(0.), 0.),
                linear_color_stop(bg, 1.),
            )))
        })
        .when(long, |d| d.child(more));
    let card = div().absolute().top(px(top)).left_0().right_0().px(px(SIDE)).flex().justify_center().child(div().w_full().max_w(px(width)).child(card));
    // Clipped at the transcript's top, so a pushed card and the shadow slide out under the toolbar, not over it.
    div().absolute().inset_0().overflow_hidden().child(card)
}

impl Desktop {
    /// The prompt whose turn the top of `agent`'s chat is in, floated over the top once it scrolls past, as VS Code's
    /// Claude panel pins it. A click scrolls back to the prompt; the next prompt pushes it out as it arrives.
    pub(super) fn chat_sticky(&mut self, agent: &str, window: &mut Window, cx: &mut Context<Self>) -> Option<Div> {
        let view = self.chats.views.get(agent)?;
        let s = &view.session;
        let (top, offset) = view.top_row()?;
        let ix = stuck(&s.rows, top, offset)?;
        let Row::Prompt { turn } = s.rows[ix] else { return None };
        let prompt = s.transcript.turns.get(turn)?.prompt.as_ref()?;
        let text = split_images(&prompt.text).0.trim();
        if text.is_empty() {
            return None;
        }
        let id = s.ids.get(ix)?.clone();
        let viewport = view.list.viewport_bounds();
        let width = transcript::content_width(transcript::WIDTH).min(f32::from(viewport.size.width) - 2. * SIDE);
        let time = (prompt.at > 0).then(|| meta::format_timestamp(prompt.at, &chrono::Local));
        let time = time.map(|time| {
            let run = window.text_style().to_run(time.len());
            let width = TIME_GAP + f32::from(window.text_system().shape_line(time.clone().into(), px(TIME_SIZE), &[run], None).width);
            (time, width)
        });
        let time_width = time.as_ref().map_or(0., |(_, width)| *width);
        let lines = user_bubble::wrapped_lines(text, SIZE, width - INSET - time_width, window);
        let open = view.sticky_open.as_deref() == Some(id.as_str());
        // The next prompt's bubble, under the gap its row keeps over it.
        let next = next_prompt(&s.rows, top).and_then(|n| Some(view.row_top(n)? + chat::top_gap(n.checked_sub(1).and_then(|i| s.rows.get(i)), &s.rows[n])));
        let bottom = height(lines, open);
        let shift = pushed(bottom, next);
        if shift <= -bottom {
            return None;
        }
        let on_jump = {
            let agent = agent.to_string();
            cx.listener(move |this, _: &ClickEvent, _, cx| this.chat_jump_to_prompt(&agent, ix, cx))
        };
        let on_more = {
            let (agent, row) = (agent.to_string(), id.clone());
            cx.listener(move |this, _: &ClickEvent, _, cx| this.chat_toggle_sticky(&agent, &row, cx))
        };
        Some(card(&id, text, time, lines > FOLDED_LINES, open, width, shift, on_jump, on_more))
    }

    fn chat_toggle_sticky(&mut self, agent: &str, row: &str, cx: &mut Context<Self>) {
        let Some(view) = self.chats.views.get_mut(agent) else { return };
        view.sticky_open = if view.sticky_open.as_deref() == Some(row) { None } else { Some(row.to_string()) };
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{FOLDED_LINES, LINE, height, next_prompt, pushed, stuck, top_at_end};
    use chat::Row;

    fn rows() -> Vec<Row> {
        vec![Row::Prompt { turn: 0 }, Row::Fold { turn: 0 }, Row::End { turn: 0 }, Row::Prompt { turn: 1 }, Row::Fold { turn: 1 }, Row::Live { turn: 1 }]
    }

    #[test]
    fn a_prompt_sticks_once_its_turn_scrolls_past_its_top() {
        assert_eq!(stuck(&rows(), 0, 0.), None, "it shows whole at the top");
        assert_eq!(stuck(&rows(), 0, 12.), Some(0));
        assert_eq!(stuck(&rows(), 2, 0.), Some(0));
        assert_eq!(stuck(&rows(), 3, 0.), None, "the next prompt shows from its top");
        assert_eq!(stuck(&rows(), 5, 40.), Some(3));
    }

    #[test]
    fn nothing_sticks_above_the_first_prompt() {
        let rows = [Row::Working, Row::Prompt { turn: 0 }];
        assert_eq!(stuck(&rows, 0, 30.), None);
        assert_eq!(stuck(&[], 0, 30.), None);
        assert_eq!(stuck(&rows, 9, 0.), Some(1), "a top past the end reads as the last row");
    }

    #[test]
    fn at_its_end_the_top_row_is_the_first_that_reaches_the_top() {
        let heights = [100., 300., 50.];
        let from = |ix: usize| Some(heights[ix..].iter().sum::<f32>());
        assert_eq!(top_at_end(3, 200., from), Some((1, 150.)));
        assert_eq!(top_at_end(3, 350., from), Some((1, 0.)));
        assert_eq!(top_at_end(3, 900., from), Some((0, 0.)), "a short chat shows from its first row");
        assert_eq!(top_at_end(3, 200., |ix| (ix > 1).then_some(50.)), None, "unmeasured rows say nothing");
    }

    #[test]
    fn the_next_prompt_or_a_pending_send_pushes_the_card() {
        assert_eq!(next_prompt(&rows(), 1), Some(3));
        assert_eq!(next_prompt(&rows(), 3), None);
        assert_eq!(next_prompt(&[Row::Prompt { turn: 0 }, Row::Live { turn: 0 }, Row::Pending(0)], 0), Some(2));
    }

    #[test]
    fn the_card_moves_up_only_once_the_next_prompt_reaches_it() {
        assert_eq!(pushed(80., None), 0.);
        assert_eq!(pushed(80., Some(200.)), 0.);
        assert_eq!(pushed(80., Some(50.)), -30.);
    }

    #[test]
    fn a_folded_card_holds_two_lines_and_an_opened_one_grows_to_160px() {
        assert_eq!(height(1, false), 1. + 20. + LINE);
        assert_eq!(height(FOLDED_LINES + 3, false), height(FOLDED_LINES + 1, false));
        assert_eq!(height(50, true), 1. + 10. + 160. + 30.);
    }
}
