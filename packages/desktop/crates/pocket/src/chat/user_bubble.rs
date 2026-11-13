use super::composer::{RADIUS, block_shade, code_shade};
use super::mention::{paint_icons, hidden, tint};
use super::selectable::selectable;
use crate::composer::element_card::element_card;
use crate::composer::elements::heading_parts;
use crate::composer::file_card::file_card;
use crate::composer::rich::highlight;
use crate::composer::{Sent, split_attachments};
use chat::rich::Style;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use theme::*;

const COLLAPSED_LINES: usize = 5;
const LINE_HEIGHT: f32 = 22.;
const TEXT_SIZE: f32 = 14.5;
/// A bubble's widest, of the reading width.
const WIDTH: f32 = 0.78;

/// A mention's `@` and the gap after it, which its icon covers; two narrow spaces, as one leaves the icon touching the name.
const SLOT: &str = "@\u{202F}\u{202F}";
const ICON: f32 = 15.;

/// What a bubble shows of a prompt.
#[derive(Default)]
struct Shown {
    text: String,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
    /// Where each mention shows, and its path.
    mentions: Vec<(Range<usize>, String)>,
    /// Where each link shows, and where it goes.
    links: Vec<(Range<usize>, String)>,
    /// Where each code span shows, and whether it's a fenced block.
    code: Vec<(Range<usize>, bool)>,
}

/// `text` as the composer showed it while typed: its commands tinted, each mention as its icon and name, its markdown
/// styled and its code set apart.
fn shown(text: &str) -> Shown {
    let (marks, code) = chat::marks_outside_code(text);
    let mut out = Shown::default();
    // A mention changes the text's length, so what follows it moves: each mention's place in the prompt, and in the bubble.
    let mut moved: Vec<(Range<usize>, Range<usize>)> = Vec::new();
    let mut copied = 0;
    for m in marks.mentions {
        out.text.push_str(&text[copied..m.start]);
        let path = &text[m.start + 1..m.end];
        let start = out.text.len();
        out.text.push_str(SLOT);
        out.text.push_str(chat::mention_name(path));
        out.highlights.push((start..start + SLOT.len(), hidden()));
        out.highlights.push((start + SLOT.len()..out.text.len(), tint(MENTION)));
        out.mentions.push((start..out.text.len(), path.to_string()));
        moved.push((m.clone(), start..out.text.len()));
        copied = m.end;
    }
    out.text.push_str(&text[copied..]);
    let place = |at: usize| match moved.iter().rev().find(|(m, _)| m.start <= at) {
        Some((m, shown)) if at < m.end => shown.start,
        Some((m, shown)) => shown.end + at - m.end,
        None => at,
    };
    let placed = |r: &Range<usize>| place(r.start)..place(r.end);
    out.highlights.extend(marks.commands.iter().map(|r| (placed(r), tint(COMMAND))));
    for c in &code {
        match c {
            chat::Code::Inline(r) => out.highlights.extend([(place(r.start)..place(r.start) + 1, hidden()), (place(r.end) - 1..place(r.end), hidden())]),
            chat::Code::Block { open, close } => out.highlights.extend([(placed(open), tint(TEXT_4)), (placed(close), tint(TEXT_4))]),
        }
        out.code.push((placed(&c.range()), matches!(c, chat::Code::Block { .. })));
    }
    let rich = chat::rich::spans(text);
    out.links = rich.iter().filter(|(_, style)| *style == Style::Link).map(|(r, _)| (placed(r), chat::rich::link_target(text, r.clone()).to_string())).collect();
    out.highlights = combine_highlights(std::mem::take(&mut out.highlights), rich.iter().map(|(r, style)| (placed(r), highlight(*style)))).collect();
    out
}

/// The prompt's text, its mentions' icons and its code's shades painted over it, its mentions' paths on hover and its
/// links opening on a click.
pub(super) fn styled(id: &str, text: &str) -> Div {
    let Shown { text, highlights, mentions, links, code } = shown(text);
    let text = SharedString::from(text);
    let styled = StyledText::new(text.clone()).with_highlights(highlights);
    let layout = styled.layout().clone();
    let slots: Vec<(usize, String)> = mentions.iter().map(|(r, path)| (r.start, path.clone())).collect();
    let marks = canvas(|_, _, _| (), {
        let layout = layout.clone();
        move |within, _, window, cx| {
            let line = layout.line_height();
            let span = |r: &Range<usize>| Some(Bounds::from_corners(layout.position_for_index(r.start)?, layout.position_for_index(r.end)? + point(px(0.), line)));
            for shade in code.iter().filter_map(|(r, block)| if *block { span(r).map(|s| block_shade(s.top(), s.bottom(), within)) } else { span(r).and_then(code_shade) }) {
                window.paint_quad(shade);
            }
            let found: Vec<(Bounds<Pixels>, &str)> = slots.iter().filter_map(|(at, path)| Some((span(&(*at..*at + SLOT.len()))?, path.as_str()))).collect();
            paint_icons(found, ICON, window, cx);
        }
    })
    .absolute()
    .top_0()
    .left_0()
    .size_full();
    let tip = move |ix: usize, window: &mut Window, cx: &mut App| mentions.iter().find(|(r, _)| r.contains(&ix)).map(|(_, path)| Tooltip::new(path.clone()).build(window, cx));
    let (ranges, targets): (Vec<Range<usize>>, Vec<String>) = links.into_iter().unzip();
    let interactive = InteractiveText::new(SharedString::from(format!("{id}-text")), styled).tooltip(tip).on_click(ranges, move |ix, _, cx| cx.open_url(&targets[ix]));
    let shown = selectable(SharedString::from(format!("{id}-select")), text, layout.clone(), interactive);
    div().relative().child(shown).child(marks)
}

/// The lines `text` wraps to in a bubble at most `width * 0.78` wide.
pub(super) fn lines(text: &str, width: f32, window: &Window) -> usize {
    wrapped_lines(text, TEXT_SIZE, width * WIDTH - 32., window)
}

/// The lines a prompt's `text` wraps to at `size` px in `width` px, as a bubble shows it.
pub(super) fn wrapped_lines(text: &str, size: f32, width: f32, window: &Window) -> usize {
    text_lines(&shown(split_attachments(text).text).text, size, width, window)
}

/// The lines plain `text` wraps to at `size` px in `width` px.
pub(super) fn text_lines(text: &str, size: f32, width: f32, window: &Window) -> usize {
    let text = text.to_string();
    let run = window.text_style().to_run(text.len());
    window
        .text_system()
        .shape_text(text.into(), px(size), &[run], Some(px(width)), None)
        .map_or(1, |lines| lines.iter().map(|l| l.wrap_boundaries().len() + 1).sum())
}

pub(super) fn needs_collapse(lines: usize) -> bool {
    lines > COLLAPSED_LINES
}

/// The longest a height eases.
pub(super) const TWEEN_MAX: Duration = Duration::from_millis(850);

/// Growing or shrinking by `delta` px takes longer the further it goes, up to `TWEEN_MAX`.
pub(super) fn tween(delta: f32) -> Duration {
    Duration::from_millis((220. + delta * 0.32) as u64).min(TWEEN_MAX)
}

/// A height easing over `delta` px: a long way eases in too, so it doesn't leap off.
pub(super) fn grow(delta: f32) -> Animation {
    let quint = ease_out_quint();
    Animation::new(tween(delta)).with_easing(move |t| if delta > 500. { ease_in_out(t) } else { quint(t) })
}

/// A collapsible prompt's wrapped lines, and when it was last toggled if that's recent enough to still be moving.
pub(super) struct Fold {
    pub(super) lines: usize,
    pub(super) toggled: Option<Instant>,
}

/// Under a prompt: copy it, and when it was sent.
pub(super) fn actions(id: &str, copied: bool, copy: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static, at: Option<i64>) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(2.))
        .pr(px(4.))
        .text_color(TEXT_4)
        .child(super::meta::action(format!("copy-{id}"), if copied { "check" } else { "copy" }, 28., 15., "Copy message", copy))
        .children(at.map(|at| div().pl(px(6.)).text_size(px(12.)).child(super::meta::format_timestamp(at, &chrono::Local))))
}

/// Under the last prompt the agent hasn't taken: when it will, and `action` to have it take them sooner.
pub(super) fn waiting(who: &str, count: usize, action: Option<AnyElement>) -> Div {
    let what = if count > 1 { format!("{who} reads these {count} after its current step") } else { format!("{who} reads this after its current step") };
    note(what).when_some(action, |d, action| d.child("·").child(action))
}

/// Under a command sent mid-turn, which the agent runs only once the turn ends.
pub(super) fn after_turn(who: &str) -> Div {
    note(format!("{who} runs this when its current turn ends"))
}

fn note(what: String) -> Div {
    div()
        .h(px(28.))
        .flex()
        .items_center()
        .gap(px(7.))
        .pr(px(4.))
        .text_size(px(12.))
        .text_color(TEXT_3)
        .child(div().size(px(6.)).flex_none().rounded_full().bg(TEXT_3))
        .child(what)
}

pub(super) fn send_now(id: &str) -> Stateful<Div> {
    div()
        .id(SharedString::from(format!("send-now-{id}")))
        .px(px(4.))
        .mx(px(-4.))
        .py(px(2.))
        .rounded(px(5.))
        .text_color(TEXT)
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(|d| d.bg(FILL_3))
        .active(|d| d.bg(FILL_4))
        .tooltip(|window, cx| Tooltip::new("Have it read them now, not after this step").build(window, cx))
        .child("Send now")
}

/// A prompt, right-aligned in a wash rounded and ringed as the composer it was typed in, its files and images under
/// it; one over five lines clips to five with "Show more".
/// `toggle` is set when the prompt collapses; `expanded` says which way it reads. Just after a toggle the height eases between the two.
pub(super) fn bubble(id: &str, prompt: String, width: f32, pending: bool, expanded: bool, toggle: Option<(Fold, impl Fn(&ClickEvent, &mut Window, &mut App) + 'static)>) -> Div {
    let Sent { text, files, images, elements } = split_attachments(&prompt);
    let text = text.to_string();
    let attached = (!images.is_empty() || !files.is_empty() || !elements.is_empty()).then(|| {
        let tile = |path: &str| div().size(px(56.)).flex_none().rounded(px(8.)).border_1().border_color(hairline(0.10)).child(img(PathBuf::from(path)).size(px(54.)).rounded(px(7.)).object_fit(ObjectFit::Cover));
        let card = |(i, path): (usize, &str)| file_card(SharedString::from(format!("{id}-file-{i}")), Path::new(path));
        let element = |(i, heading): (usize, &str)| {
            let (number, host, title) = heading_parts(heading);
            element_card(SharedString::from(format!("{id}-element-{i}")), number, host, title, None)
        };
        div()
            .flex()
            .flex_wrap()
            .gap(px(8.))
            .when(!text.is_empty(), |d| d.mt(px(8.)))
            .mb(px(3.))
            .children(images.into_iter().map(tile))
            .children(files.into_iter().enumerate().map(card))
            .children(elements.into_iter().enumerate().map(element))
    });
    let collapsed = toggle.is_some() && !expanded;
    let clipped = LINE_HEIGHT * (COLLAPSED_LINES + 1) as f32;
    let full = toggle.as_ref().map_or(0., |(fold, _)| fold.lines as f32 * LINE_HEIGHT);
    let delta = full - clipped;
    let moving = toggle.as_ref().and_then(|(fold, _)| fold.toggled).is_some_and(|at| at.elapsed() < tween(delta));
    let body = if moving {
        let (from, to) = if expanded { (clipped, full) } else { (full, clipped) };
        div()
            .overflow_hidden()
            .child(styled(id, &text))
            .with_animation(SharedString::from(format!("{id}-{expanded}")), grow(delta), move |d, t| d.h(px(from + (to - from) * t)))
            .into_any_element()
    } else if collapsed {
        div().flex().flex_col().child(div().h(px(LINE_HEIGHT * COLLAPSED_LINES as f32)).overflow_hidden().child(styled(id, &text))).child(div().h(px(LINE_HEIGHT)).child("...")).into_any_element()
    } else {
        div().whitespace_normal().child(styled(id, &text)).into_any_element()
    };
    let expander = toggle.map(|(_, on_click)| {
        div().mt(px(8.)).flex().items_start().child(
            div()
                .id(SharedString::from(format!("{id}-expander")))
                .group("user-toggle")
                .flex()
                .items_center()
                .gap(px(5.))
                .text_color(TEXT_2)
                .cursor_pointer()
                .hover(|d| d.text_color(TEXT))
                .on_click(on_click)
                .child(if expanded { "Show less" } else { "Show more" })
                .child(
                    icon("chevron-down", 12., TEXT_2)
                        .when(expanded, |i| i.with_transformation(Transformation::rotate(radians(std::f32::consts::PI))))
                        .group_hover("user-toggle", |s| s.text_color(TEXT)),
                ),
        )
    });
    let bubble = div()
        .min_w_0()
        .max_w(px(width * WIDTH))
        .bg(BUBBLE)
        .border_1()
        .border_color(HAIRLINE)
        .rounded(px(RADIUS))
        .px(px(16.))
        .py(px(9.))
        .text_size(px(TEXT_SIZE))
        .line_height(px(LINE_HEIGHT))
        .text_color(TEXT)
        .when(pending, |d| d.opacity(0.65))
        .child(body)
        .children(attached)
        .children(expander);
    div().w_full().flex().justify_end().child(bubble)
}

#[cfg(test)]
mod tests {
    use super::{needs_collapse, shown, tween};
    use std::time::Duration;

    #[test]
    fn a_prompt_over_five_wrapped_lines_collapses() {
        assert!(!needs_collapse(5));
        assert!(needs_collapse(6));
    }

    #[test]
    fn a_bubble_shows_each_mention_as_its_name_and_keeps_its_path() {
        let s = shown("/review then @src/parser.rs and @docs/");
        assert_eq!(s.text, "/review then @\u{202F}\u{202F}parser.rs and @\u{202F}\u{202F}docs/");
        let mentions: Vec<(&str, &str)> = s.mentions.iter().map(|(r, path)| (&s.text[r.clone()], path.as_str())).collect();
        assert_eq!(mentions, [("@\u{202F}\u{202F}parser.rs", "src/parser.rs"), ("@\u{202F}\u{202F}docs/", "docs/")]);
    }

    #[test]
    fn a_bubble_tints_each_skill_where_it_shows_after_a_mention() {
        let s = shown("/prototype @src/parser.rs with /apple-design");
        let tinted: Vec<&str> = s.highlights.iter().filter(|(_, h)| h.color.is_some()).map(|(r, _)| &s.text[r.clone()]).collect();
        assert_eq!(tinted, ["/prototype", "parser.rs", "/apple-design"]);
    }

    #[test]
    fn a_bubble_styles_markdown_and_links_where_they_show_after_a_mention() {
        let s = shown("@src/parser.rs **fix** it, see https://a.dev/x and [docs](https://b.dev)");
        let links: Vec<(&str, &str)> = s.links.iter().map(|(r, to)| (&s.text[r.clone()], to.as_str())).collect();
        assert_eq!(links, [("https://a.dev/x", "https://a.dev/x"), ("docs", "https://b.dev")]);
        let bold: Vec<&str> = s.highlights.iter().filter(|(_, h)| h.font_weight.is_some()).map(|(r, _)| &s.text[r.clone()]).collect();
        assert_eq!(bold, ["fix"]);
    }

    #[test]
    fn a_bubble_sets_code_apart_and_leaves_what_it_holds_as_typed() {
        let s = shown("run `/review @a.rs` then\n```\n- **b**\n```");
        let code: Vec<(&str, bool)> = s.code.iter().map(|(r, block)| (&s.text[r.clone()], *block)).collect();
        assert_eq!(code, [("`/review @a.rs`", false), ("```\n- **b**\n```", true)]);
        assert!(s.mentions.is_empty() && s.links.is_empty());
        assert!(s.highlights.iter().all(|(_, h)| h.font_weight.is_none()));
    }

    #[test]
    fn a_bubble_eases_longer_the_further_it_moves_up_to_850ms() {
        assert_eq!(tween(0.), Duration::from_millis(220));
        assert_eq!(tween(500.), Duration::from_millis(380));
        assert_eq!(tween(5000.), Duration::from_millis(850));
    }
}

