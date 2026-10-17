use crate::composer::split_images;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use theme::*;

const COLLAPSED_LINES: usize = 5;
const LINE_HEIGHT: f32 = 22.;
const TEXT_SIZE: f32 = 14.5;
/// A bubble's widest, of the reading width.
const WIDTH: f32 = 0.78;

/// Byte ranges of the `@path` mentions in `text`.
pub(super) fn mentions(text: &str) -> Vec<std::ops::Range<usize>> {
    let part = |c: char| c.is_alphanumeric() || matches!(c, '_' | '.' | '/' | '-');
    let mut out = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c != '@' {
            continue;
        }
        let mut end = i + 1;
        while let Some(&(j, c)) = chars.peek().filter(|(_, c)| part(*c)) {
            end = j + c.len_utf8();
            chars.next();
        }
        if end > i + 1 {
            out.push(i..end);
        }
    }
    out
}

/// The prompt's text with its mentions picked out in the code font.
fn styled(text: String) -> StyledText {
    let ranges = mentions(&text);
    let highlights = ranges.iter().map(|r| (r.clone(), HighlightStyle { color: Some(MENTION.into()), ..Default::default() })).collect::<Vec<_>>();
    let fonts = ranges.into_iter().map(|r| (r, code_font())).collect::<Vec<_>>();
    StyledText::new(text).with_highlights(highlights).with_font_family_overrides(fonts)
}

/// The lines `text` wraps to in a bubble at most `width * 0.78` wide.
pub(super) fn lines(text: &str, width: f32, window: &Window) -> usize {
    let text = split_images(text).0;
    let run = window.text_style().to_run(text.len());
    window
        .text_system()
        .shape_text(text.to_string().into(), px(TEXT_SIZE), &[run], Some(px(width * WIDTH - 32.)), None)
        .map_or(1, |lines| lines.iter().map(|l| l.wrap_boundaries().len() + 1).sum())
}

pub(super) fn needs_collapse(lines: usize) -> bool {
    lines > COLLAPSED_LINES
}

/// Growing or shrinking by `delta` px takes longer the further it goes, up to 850ms.
fn tween(delta: f32) -> Duration {
    Duration::from_millis((220. + delta * 0.32).min(850.) as u64)
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

/// A prompt, right-aligned in a wash; one over five lines clips to five with "Show more".
/// `toggle` is set when the prompt collapses; `expanded` says which way it reads. Just after a toggle the height eases between the two.
pub(super) fn bubble(id: &str, prompt: String, width: f32, pending: bool, expanded: bool, toggle: Option<(Fold, impl Fn(&ClickEvent, &mut Window, &mut App) + 'static)>) -> Div {
    let (text, images) = split_images(&prompt);
    let text = text.to_string();
    let thumbs = (!images.is_empty()).then(|| {
        let tile = |path: &str| div().size(px(56.)).flex_none().rounded(px(8.)).border_1().border_color(hairline(0.10)).child(img(PathBuf::from(path)).size(px(54.)).rounded(px(7.)).object_fit(ObjectFit::Cover));
        div().flex().flex_wrap().gap(px(8.)).when(!text.is_empty(), |d| d.mt(px(8.))).children(images.into_iter().map(tile))
    });
    let collapsed = toggle.is_some() && !expanded;
    let clipped = LINE_HEIGHT * (COLLAPSED_LINES + 1) as f32;
    let full = toggle.as_ref().map_or(0., |(fold, _)| fold.lines as f32 * LINE_HEIGHT);
    let delta = full - clipped;
    let moving = toggle.as_ref().and_then(|(fold, _)| fold.toggled).is_some_and(|at| at.elapsed() < tween(delta));
    let body = if moving {
        let (from, to) = if expanded { (clipped, full) } else { (full, clipped) };
        let quint = ease_out_quint();
        let easing = move |t| if delta > 500. { ease_in_out(t) } else { quint(t) };
        div()
            .overflow_hidden()
            .child(styled(text))
            .with_animation(SharedString::from(format!("{id}-{expanded}")), Animation::new(tween(delta)).with_easing(easing), move |d, t| d.h(px(from + (to - from) * t)))
            .into_any_element()
    } else if collapsed {
        div().flex().flex_col().child(div().h(px(LINE_HEIGHT * COLLAPSED_LINES as f32)).overflow_hidden().child(styled(text))).child(div().h(px(LINE_HEIGHT)).child("...")).into_any_element()
    } else {
        div().whitespace_normal().child(styled(text)).into_any_element()
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
        .rounded(px(18.))
        .px(px(16.))
        .py(px(9.))
        .text_size(px(TEXT_SIZE))
        .line_height(px(LINE_HEIGHT))
        .text_color(TEXT)
        .when(pending, |d| d.opacity(0.65))
        .child(body)
        .children(thumbs)
        .children(expander);
    div().w_full().flex().justify_end().child(bubble)
}

#[cfg(test)]
mod tests {
    use super::{mentions, needs_collapse, tween};
    use std::time::Duration;

    #[test]
    fn a_prompt_over_five_wrapped_lines_collapses() {
        assert!(!needs_collapse(5));
        assert!(needs_collapse(6));
    }

    #[test]
    fn mentions_are_at_signs_followed_by_a_path() {
        let text = "fix @src/chat.rs and @a_b-c.md, mail me@ x";
        let found: Vec<&str> = mentions(text).into_iter().map(|r| &text[r]).collect();
        assert_eq!(found, ["@src/chat.rs", "@a_b-c.md"]);
    }

    #[test]
    fn a_bubble_eases_longer_the_further_it_moves_up_to_850ms() {
        assert_eq!(tween(0.), Duration::from_millis(220));
        assert_eq!(tween(500.), Duration::from_millis(380));
        assert_eq!(tween(5000.), Duration::from_millis(850));
    }
}
