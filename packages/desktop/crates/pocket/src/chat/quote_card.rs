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

pub(super) fn quote_card(id: &str, quote: &Quote, width: f32, open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Stateful<Div> {
    let (place, text) = match &quote.body {
        Body::Code { lines, text, .. } => (format!("lines {}", span(*lines)), text),
        Body::Diff { lines, text, .. } => (format!("uncommitted · line {}", span(*lines)), text),
        Body::Prose { section, text } => (section.as_ref().map(|s| format!("› {s}")).unwrap_or_default(), text),
    };
    let head = div()
        .flex()
        .items_center()
        .gap(px(6.))
        .mb(px(3.))
        .child(icon(quote.icon(), 13., TEXT_3))
        .child(div().flex_none().font_weight(FontWeight::MEDIUM).text_color(TEXT).child(basename(&quote.path)))
        .child(div().min_w_0().truncate().font_family(code_font()).text_size(px(12.)).text_color(TEXT_3).child(place));
    let body = match &quote.body {
        Body::Prose { .. } => div().line_height(px(18.)).line_clamp(SHOWN_LINES).child(text.clone()).into_any_element(),
        Body::Diff { .. } | Body::Code { .. } => {
            let diff = matches!(quote.body, Body::Diff { .. });
            div()
                .flex()
                .flex_col()
                .font_family(code_font())
                .line_height(px(19.))
                .children(shown_lines(text).into_iter().enumerate().map(move |(i, line)| {
                    let ink = match line.chars().next() {
                        Some('+') if diff => DIFF_ADD_TEXT,
                        Some('-') if diff => DIFF_DEL_TEXT,
                        _ => TEXT_2,
                    };
                    div().whitespace_nowrap().truncate().text_color(ink).when(i > 0, |d| d.opacity(0.45)).child(line)
                }))
                .into_any_element()
        }
    };
    let rail =
        div().absolute().top(relative(0.5)).bottom(px(-6.)).right(px(-RAIL_W)).w(px(RAIL_W)).border_t_1().border_r_1().border_color(RAIL).rounded_tr(px(10.));
    let path = quote.path.clone();
    div()
        .id(SharedString::from(format!("{id}-quote")))
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
        .cursor_pointer()
        .hover(|d| d.bg(FILL_2))
        .tooltip(move |window, cx| Tooltip::new(path.clone()).build(window, cx))
        .on_click(open)
        .child(head)
        .child(body)
        .child(rail)
}

fn span((lo, hi): (usize, usize)) -> String {
    if lo == hi { lo.to_string() } else { format!("{lo}–{hi}") }
}

fn shown_lines(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).take(SHOWN_LINES).collect();
    let indent = lines.iter().map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0);
    lines.iter().map(|l| l[indent..].to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::shown_lines;

    #[test]
    fn a_card_shows_the_first_two_lines_with_text_without_their_shared_indent() {
        assert_eq!(shown_lines("\n    pub fn a() {\n        b();\n    }"), ["pub fn a() {", "    b();"]);
    }

    #[test]
    fn a_diff_keeps_its_signs_when_dedented() {
        assert_eq!(shown_lines("-    let a = 1;\n+    let a = 2;"), ["-    let a = 1;", "+    let a = 2;"]);
    }
}
