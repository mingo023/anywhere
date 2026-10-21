use super::{meta, user_bubble};
use crate::composer::split_images;
use crate::desktop::Desktop;
use chat::Row;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::PathBuf;
use std::time::Duration;
use theme::*;

/// The pill's height while folded to one line.
const HEIGHT: f32 = 34.;
const SIZE: f32 = 13.;
const LINE: f32 = 20.;
/// The arrow and the chevron, inset from the pill's ends as far as from its top.
const KNOB: f32 = 22.;
const INSET: f32 = (HEIGHT - KNOB) / 2.;
/// The padding after the time when no chevron ends the pill.
const PAD_END: f32 = 12.;
const GAP: f32 = 8.;
const MAX_WIDTH: f32 = 560.;
/// A prompt's first images, before its text; the rest are counted as +N.
const THUMB: f32 = 20.;
const THUMBS: usize = 2;
const THUMB_GAP: f32 = 3.;
const TIME_SIZE: f32 = 12.;
/// An opened pill's padding over and under its text.
const OPEN_TOP: f32 = 6.;
const OPEN_BOTTOM: f32 = 8.;
/// Past this an opened prompt scrolls.
const OPEN_MAX_HEIGHT: f32 = 160.;
/// How far a card is pushed before its shadow is gone.
const SETTLE: f32 = 12.;
/// The transcript's side padding, which the card keeps inside.
const SIDE: f32 = 32.;
const MARGIN_TOP: f32 = 10.;
const ENTER: Duration = Duration::from_millis(220);
const DROP: Token = Token::new(0x00000024, 0x00000080);
const DROP_NEAR: Token = Token::new(0x00000014, 0x0000004d);

/// The prompt row whose turn the top of the list is in, once that prompt has scrolled up past the top: none while it
/// shows from its top, above the first prompt, or under a send not yet echoed, which ends the turn before it.
/// `top` is the row at the list's top and `offset` how far into it.
pub(super) fn stuck(rows: &[Row], top: usize, offset: f32) -> Option<usize> {
    let last = top.min(rows.len().checked_sub(1)?);
    let ix = rows[..=last].iter().rposition(|r| matches!(r, Row::Prompt { .. } | Row::Pending(_)))?;
    (matches!(rows[ix], Row::Prompt { .. }) && (ix < top || offset > 0.)).then_some(ix)
}

/// The row at the top of a list scrolled to its end, and how far into it, from each row's height to the end and the
/// height its rows show in. Rows too few to fill it show from the first.
pub(super) fn top_at_end(count: usize, shown: f32, from: impl Fn(usize) -> f32) -> (usize, f32) {
    for ix in (0..count).rev() {
        let height = from(ix);
        if height >= shown {
            return (ix, height - shown);
        }
    }
    (0, 0.)
}

/// The next prompt under the list's top, which pushes the card up as it arrives.
fn next_prompt(rows: &[Row], top: usize) -> Option<usize> {
    rows.iter().enumerate().skip(top + 1).find(|(_, r)| matches!(r, Row::Prompt { .. } | Row::Pending(_))).map(|(ix, _)| ix)
}

/// The card's height for a prompt that wraps to `lines` lines when opened.
fn height(lines: usize, open: bool) -> f32 {
    if !open {
        return HEIGHT;
    }
    // The text sits 1px down, so its first line is level with the arrow.
    OPEN_TOP + (1. + (lines as f32 * LINE).min(OPEN_MAX_HEIGHT)).max(KNOB) + OPEN_BOTTOM
}

/// The width an opened prompt's text wraps at in a card `max` px wide, beside parts this wide, each `GAP` from the next.
fn text_width(max: f32, beside: &[f32]) -> f32 {
    max - 2. * INSET - beside.iter().map(|w| w + GAP).sum::<f32>()
}

/// A prompt folded onto one line, its line breaks read as spaces.
fn one_line(text: &str) -> String {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ")
}

/// What a prompt with no text shows.
fn image_count(n: usize) -> String {
    format!("{n} image{}", if n == 1 { "" } else { "s" })
}

/// How far the card moves up so the next prompt, `next` px under the list's top, pushes it out rather than slides
/// under it; `bottom` is where the card ends.
fn pushed(bottom: f32, next: Option<f32>) -> f32 {
    next.map_or(0., |next| (next - bottom).min(0.))
}

/// How much of its shadow a card keeps when pushed `shift` px up: it settles onto the next bubble over the first
/// `SETTLE` px, so the shadow doesn't darken that bubble.
fn lift(shift: f32) -> f32 {
    (1. + shift / SETTLE).clamp(0., 1.)
}

/// Paints `child` only below `top`, where a stuck prompt's card ends, so the margins around the card show the chat's
/// background rather than transcript text cut off by the card. Only the paint is cut: the list under it still takes
/// the wheel there.
pub(super) struct Below {
    top: Pixels,
    child: AnyElement,
}

pub(super) fn below(top: f32, child: impl IntoElement) -> Below {
    Below { top: px(top), child: child.into_any_element() }
}

impl IntoElement for Below {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Below {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) {
        self.child.prepaint(window, cx);
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), _: &mut (), window: &mut Window, cx: &mut App) {
        let shown = Bounds::from_corners(point(bounds.left(), (bounds.top() + self.top).min(bounds.bottom())), bounds.bottom_right());
        window.with_content_mask(Some(ContentMask { bounds: shown }), |window| self.child.paint(window, cx));
    }
}

/// One of a prompt's images, as small as a letter.
fn thumb(path: &str) -> Div {
    let image = img(PathBuf::from(path)).size(px(THUMB)).rounded(px(5.)).object_fit(ObjectFit::Cover);
    div().size(px(THUMB)).flex_none().rounded(px(5.)).shadow(vec![ui::ring(hairline(0.16), 0.5)]).child(image)
}

/// A prompt floating over the top of the chat as a pill: an arrow back to it, its first images, the prompt folded to
/// one line and the time it was sent. A chevron opens a prompt too long for the line.
#[allow(clippy::too_many_arguments)]
fn card(
    id: &str,
    text: &str,
    images: &[&str],
    time: Option<String>,
    long: bool,
    open: bool,
    max: f32,
    top: f32,
    cover: f32,
    enter: bool,
    on_jump: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_more: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let knob = || div().size(px(KNOB)).flex_none().rounded_full().flex().items_center().justify_center();
    let arrow = knob().bg(ink(0.07)).group_hover("sticky", |s| s.bg(ink(0.12))).child(icon("arrow-up", 12., TEXT_2).group_hover("sticky", |s| s.text_color(TEXT)));
    // A prompt of only images counts them in its label instead.
    let rest = if text.is_empty() { 0 } else { images.len().saturating_sub(THUMBS) };
    let thumbs = (!images.is_empty()).then(|| {
        let rest = (rest > 0).then(|| div().px(px(2.)).text_size(px(11.)).text_color(TEXT_3).child(format!("+{rest}")));
        div().flex().flex_none().items_center().gap(px(THUMB_GAP)).children(images.iter().take(THUMBS).map(|path| thumb(path))).children(rest)
    });
    let body = if text.is_empty() {
        div().flex_none().text_color(TEXT_2).child(image_count(images.len())).into_any_element()
    } else if open {
        let text = div().id(SharedString::from(format!("sticky-text-{id}"))).max_h(px(OPEN_MAX_HEIGHT)).overflow_y_scroll().child(user_bubble::styled(&format!("sticky-{id}"), text));
        div().flex_1().min_w_0().pt(px(1.)).text_color(TEXT_BODY).child(text).into_any_element()
    } else {
        div().min_w_0().truncate().child(user_bubble::styled(&format!("sticky-{id}"), &one_line(text))).into_any_element()
    };
    let time = time.map(|time| div().flex().flex_none().gap(px(GAP)).text_size(px(TIME_SIZE)).line_height(px(KNOB)).child(div().text_color(TEXT_5).child("·")).child(div().text_color(TEXT_3).child(time)));
    let chevron = long.then(|| {
        knob()
            .id(SharedString::from(format!("sticky-more-{id}")))
            .group("sticky-more")
            .hover(|d| d.bg(FILL_3))
            .on_click(move |e, window, cx| {
                cx.stop_propagation();
                on_more(e, window, cx);
            })
            .child(
                icon("chevron-down", 12., TEXT_3)
                    .when(open, |i| i.with_transformation(Transformation::rotate(radians(std::f32::consts::PI))))
                    .group_hover("sticky-more", |s| s.text_color(TEXT)),
            )
    });
    let lift = lift(top);
    let card = div()
        .id(SharedString::from(format!("sticky-{id}")))
        .group("sticky")
        // An opened prompt scrolls itself; a folded one lets the wheel through to the transcript.
        .when(open, |d| d.occlude())
        .when(!open, |d| d.block_mouse_except_scroll())
        .flex()
        .gap(px(GAP))
        .min_w_0()
        .max_w(px(max))
        .when(open, |d| d.w_full().items_start().pt(px(OPEN_TOP)).pb(px(OPEN_BOTTOM)).rounded(px(16.)))
        .when(!open, |d| d.h(px(HEIGHT)).items_center().rounded(px(HEIGHT / 2.)))
        .pl(px(INSET))
        .pr(px(if long { INSET } else { PAD_END }))
        // Opaque, as GPUI has no backdrop blur; the transcript isn't painted around it.
        .bg(POPOVER)
        .shadow(vec![
            ui::ring(hairline(0.11), 0.5),
            BoxShadow { inset: true, ..ui::shadow(HIGHLIGHT, 0.5, 0.) },
            ui::shadow(Hsla::from(DROP).opacity(lift), 10., 28.),
            ui::shadow(Hsla::from(DROP_NEAR).opacity(lift), 2., 6.),
        ])
        .text_size(px(SIZE))
        .line_height(px(LINE))
        .text_color(TEXT)
        .cursor_pointer()
        .on_click(on_jump)
        .child(arrow)
        .children(thumbs)
        .child(body)
        .children(time)
        .children(chevron);
    let card = if enter {
        let rise = Animation::new(ENTER).with_easing(ease_out_quint());
        card.with_animation(SharedString::from(format!("sticky-in-{id}")), rise, |d, t| d.opacity(t).mt(px(-6. * (1. - t)))).into_any_element()
    } else {
        card.into_any_element()
    };
    let card = div().absolute().top(px(MARGIN_TOP + top)).left_0().right_0().px(px(SIDE)).flex().justify_center().child(card);
    // The transcript isn't painted under `cover`, so its rows there take no clicks or hover either; the wheel still
    // scrolls them.
    let cover = div().absolute().top_0().left_0().right_0().h(px(cover)).block_mouse_except_scroll();
    // Clipped at the transcript's top, so a pushed card and the shadow slide out under the toolbar, not over it.
    div().absolute().inset_0().overflow_hidden().child(cover).child(card)
}

impl Desktop {
    /// The prompt whose turn the top of `agent`'s chat is in, floated over the top once it scrolls past. A click scrolls
    /// back to the prompt; the next prompt pushes it out as it arrives. Also how far down its card ends, which the
    /// transcript paints only below.
    pub(super) fn chat_sticky(&mut self, agent: &str, window: &mut Window, cx: &mut Context<Self>) -> Option<(Div, f32)> {
        let view = self.chats.views.get(agent)?;
        let s = &view.session;
        let (top, offset) = view.top_row();
        let ix = stuck(&s.rows, top, offset)?;
        let Row::Prompt { turn } = s.rows[ix] else { return None };
        let prompt = s.transcript.turns.get(turn)?.prompt.as_ref()?;
        let (text, images) = split_images(&prompt.text);
        let text = text.trim();
        let id = s.ids.get(ix)?.clone();
        let viewport = view.list.viewport_bounds();
        let max = MAX_WIDTH.min(f32::from(viewport.size.width) - 2. * SIDE);
        let time = (prompt.at > 0).then(|| meta::format_timestamp(prompt.at, &chrono::Local));
        let measure = |s: &str, size: f32| {
            let run = window.text_style().to_run(s.len());
            f32::from(window.text_system().shape_line(s.to_string().into(), px(size), &[run], None).width)
        };
        // Opened, the text wraps beside the same parts the folded line has.
        let lines = if text.is_empty() {
            0
        } else {
            let shown = images.len().min(THUMBS);
            let rest = images.len() - shown;
            let thumbs = (shown > 0).then(|| shown as f32 * (THUMB + THUMB_GAP) - THUMB_GAP + if rest > 0 { THUMB_GAP + 4. + measure(&format!("+{rest}"), 11.) } else { 0. });
            let time = time.as_ref().map(|t| measure("·", TIME_SIZE) + GAP + measure(t, TIME_SIZE));
            let beside: Vec<f32> = [Some(KNOB), thumbs, time, Some(KNOB)].into_iter().flatten().collect();
            user_bubble::wrapped_lines(text, SIZE, text_width(max, &beside), window)
        };
        let long = lines > 1;
        let open = long && view.sticky_open.as_deref() == Some(id.as_str());
        // The next prompt's bubble, under the gap its row keeps over it.
        let next = next_prompt(&s.rows, top).and_then(|n| Some(view.row_top(n)? + chat::top_gap(n.checked_sub(1).and_then(|i| s.rows.get(i)), &s.rows[n])));
        let bottom = MARGIN_TOP + height(lines, open);
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
        let cover = bottom + shift;
        Some((card(&id, text, &images, time, long, open, max, shift, cover, !cx.reduce_motion(), on_jump, on_more), cover))
    }

    fn chat_toggle_sticky(&mut self, agent: &str, row: &str, cx: &mut Context<Self>) {
        let Some(view) = self.chats.views.get_mut(agent) else { return };
        view.sticky_open = if view.sticky_open.as_deref() == Some(row) { None } else { Some(row.to_string()) };
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{HEIGHT, KNOB, OPEN_BOTTOM, OPEN_MAX_HEIGHT, OPEN_TOP, height, image_count, lift, next_prompt, one_line, pushed, stuck, top_at_end};
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
    fn a_send_held_at_the_top_before_its_echo_sticks_nothing() {
        let rows = [Row::Prompt { turn: 0 }, Row::Fold { turn: 0 }, Row::End { turn: 0 }, Row::Pending(0), Row::Working];
        assert_eq!(stuck(&rows, 3, 0.), None);
        assert_eq!(stuck(&rows, 4, 10.), None, "the previous prompt's turn ended above the send");
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
        let from = |ix: usize| heights[ix..].iter().sum::<f32>();
        assert_eq!(top_at_end(3, 200., from), (1, 150.));
        assert_eq!(top_at_end(3, 350., from), (1, 0.));
        assert_eq!(top_at_end(3, 900., from), (0, 0.), "a short chat shows from its first row");
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
    fn a_pushed_card_loses_its_shadow_over_its_first_12px() {
        assert_eq!(lift(0.), 1.);
        assert_eq!(lift(-6.), 0.5);
        assert_eq!(lift(-30.), 0.);
    }

    #[test]
    fn a_folded_card_is_one_line_tall_and_an_opened_one_grows_to_160px() {
        assert_eq!(height(9, false), HEIGHT);
        assert_eq!(height(1, true), OPEN_TOP + KNOB + OPEN_BOTTOM, "one line sits level with the arrow");
        assert_eq!(height(50, true), OPEN_TOP + 1. + OPEN_MAX_HEIGHT + OPEN_BOTTOM);
    }

    #[test]
    fn a_folded_prompt_reads_its_line_breaks_as_spaces() {
        assert_eq!(one_line("Fix these:\n1) filter\n\n  2) sort "), "Fix these: 1) filter 2) sort");
    }

    #[test]
    fn a_prompt_of_only_images_counts_them() {
        assert_eq!(image_count(1), "1 image");
        assert_eq!(image_count(4), "4 images");
    }
}
