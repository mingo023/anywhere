use super::markdown::Styles;
use crate::add_to_chat::{Body, Quote};
use crate::util::basename;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

const MAX_W: f32 = 480.;
const RAIL_ROOM: f32 = 28.;
const RAIL_W: f32 = 17.;
const SHOWN_LINES: usize = 2;
/// The space between the cards and the bubble, which the rail spans.
pub(super) const GAP: f32 = 6.;

/// The grammar and text a card's code is coloured from: its shown lines, joined. None for prose and diffs, which keep their own colours.
pub(super) fn colour_source(quote: &Quote) -> Option<(&'static str, String)> {
    match &quote.body {
        Body::Code { text, .. } => Some((crate::syntax::language_for(&quote.path), shown_lines(text).join("\n"))),
        Body::Diff { .. } | Body::Prose { .. } | Body::Chat { .. } => None,
    }
}

/// `colours` are byte ranges into `colour_source`'s text. Card `nth` past the first joins the rail the one above it starts.
pub(super) fn quote_card(
    id: &str,
    nth: usize,
    quote: &Quote,
    width: f32,
    colours: Option<Styles>,
    open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let (place, text) = match &quote.body {
        Body::Code { lines, text, .. } => (format!("lines {}", span(*lines)), text),
        Body::Diff { lines, text, .. } => (format!("uncommitted · line {}", span(*lines)), text),
        Body::Prose { section, text } => (section.as_ref().map(|s| format!("› {s}")).unwrap_or_default(), text),
        Body::Chat { text } => (String::new(), text),
    };
    let name = match &quote.body {
        Body::Chat { .. } => quote.label(),
        _ => basename(&quote.path),
    };
    let head = div()
        .flex()
        .items_center()
        .gap(px(6.))
        .mb(px(3.))
        .child(icon(quote.icon(), 13., TEXT_3))
        .child(div().flex_none().font_weight(FontWeight::MEDIUM).text_color(TEXT).child(name))
        .child(div().min_w_0().truncate().font_family(code_font()).text_size(px(12.)).text_color(TEXT_3).child(place));
    let body = match &quote.body {
        Body::Prose { .. } | Body::Chat { .. } => div().line_height(px(18.)).line_clamp(SHOWN_LINES).child(text.clone()).into_any_element(),
        Body::Diff { .. } | Body::Code { .. } => {
            let diff = matches!(quote.body, Body::Diff { .. });
            div()
                .flex()
                .flex_col()
                .font_family(code_font())
                .line_height(px(19.))
                .children(shown_lines(text).into_iter().zip(line_starts(text)).enumerate().map(move |(i, (line, start))| {
                    let ink = match line.chars().next() {
                        Some('+') if diff => DIFF_ADD_TEXT,
                        Some('-') if diff => DIFF_DEL_TEXT,
                        _ => TEXT_2,
                    };
                    let runs = colours.as_deref().map(|c| in_line(c, start, line.len())).unwrap_or_default();
                    div().whitespace_nowrap().truncate().text_color(ink).when(i > 0, |d| d.opacity(0.45)).child(StyledText::new(line).with_highlights(runs))
                }))
                .into_any_element()
        }
    };
    let rail = div().absolute().bottom(px(-GAP)).right(px(-RAIL_W)).w(px(RAIL_W)).border_r_1().border_color(RAIL);
    let rail = match nth {
        0 => rail.top(relative(0.5)).border_t_1().rounded_tr(px(10.)),
        _ => rail.top(px(-1.)).child(div().absolute().top(relative(0.5)).left_0().right_0().border_t_1().border_color(RAIL)),
    };
    let path = quote.path.clone();
    let file = !path.is_empty();
    div()
        .id(SharedString::from(format!("{id}-quote-{nth}")))
        .relative()
        .min_w_0()
        .max_w(px(MAX_W.min(width * 0.7)))
        .mr(px(RAIL_ROOM))
        .px(px(12.))
        .pt(px(8.))
        .pb(px(9.))
        .rounded(px(12.))
        .border_1()
        .border_color(SEPARATOR)
        .text_size(px(12.5))
        .text_color(TEXT_2)
        .when(file, |d| d.cursor_pointer().hover(|d| d.bg(FILL_2)).tooltip(move |window, cx| Tooltip::new(path.clone()).build(window, cx)).on_click(open))
        .child(head)
        .child(body)
        .child(rail)
}

fn span((lo, hi): (usize, usize)) -> String {
    if lo == hi { lo.to_string() } else { format!("{lo}–{hi}") }
}

/// Where each shown line starts in their `\n`-joined text.
fn line_starts(text: &str) -> Vec<usize> {
    shown_lines(text).iter().scan(0, |at, l| Some(std::mem::replace(at, *at + l.len() + 1))).collect()
}

/// The runs of `styles` that fall in the line at `start..start + len`, measured from the line's start.
fn in_line(styles: &[(std::ops::Range<usize>, HighlightStyle)], start: usize, len: usize) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
    let end = start + len;
    styles.iter().filter(|(r, _)| r.start < end && r.end > start).map(|(r, s)| (r.start.max(start) - start..r.end.min(end) - start, *s)).collect()
}

fn shown_lines(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).take(SHOWN_LINES).collect();
    let indent = lines.iter().map(|l| l.len() - l.trim_start_matches([' ', '\t']).len()).min().unwrap_or(0);
    lines.iter().map(|l| l[indent..].to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::{in_line, line_starts, shown_lines};
    use gpui_kit::HighlightStyle;

    #[test]
    fn a_card_shows_the_first_two_lines_with_text_without_their_shared_indent() {
        assert_eq!(shown_lines("\n    pub fn a() {\n        b();\n    }"), ["pub fn a() {", "    b();"]);
    }

    #[test]
    fn lines_indented_with_different_whitespace_keep_it() {
        assert_eq!(shown_lines(" x\n\u{3000}y"), [" x", "\u{3000}y"]);
    }

    #[test]
    fn colours_for_the_joined_lines_split_back_onto_each_line() {
        let text = "  let a = 1;\n  b(a);";
        assert_eq!(line_starts(text), [0, 11]);
        let style = HighlightStyle::default();
        let runs = in_line(&[(0..3, style), (9..14, style)], 11, 5);
        assert_eq!(runs.iter().map(|(r, _)| (r.start, r.end)).collect::<Vec<_>>(), [(0, 3)]);
    }

    #[test]
    fn a_diff_keeps_its_signs_when_dedented() {
        assert_eq!(shown_lines("-    let a = 1;\n+    let a = 2;"), ["-    let a = 1;", "+    let a = 2;"]);
    }
}
