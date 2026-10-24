use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;
use theme::*;

/// A detail line's text size and height.
pub(crate) const TEXT_SIZE: f32 = 12.;
pub(crate) const LINE: f32 = 18.;
/// Lines a detail block shows before counting the rest.
pub(crate) const MAX_LINES: usize = 24;
/// Chars kept of one output line: lines clip, but each is still shaped whole.
const LINE_MAX_CHARS: usize = 8 * 1024;

/// A detail block's lines, and how many more were cut.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Lines {
    pub(crate) lines: Vec<String>,
    pub(crate) more: usize,
}

impl Lines {
    /// Trims trailing blank lines and keeps the first `MAX_LINES`.
    pub(crate) fn head(mut lines: Vec<String>) -> Self {
        while lines.last().is_some_and(|l| l.trim().is_empty()) {
            lines.pop();
        }
        let more = lines.len().saturating_sub(MAX_LINES);
        lines.truncate(MAX_LINES);
        Self { lines, more }
    }

    /// Lines it takes to show, the one counting the rest included.
    pub(crate) fn rows(&self) -> usize {
        self.lines.len() + usize::from(self.more > 0)
    }

    /// What it shows as one text, ending on the count of the rest.
    pub(crate) fn text(&self) -> String {
        self.lines.join("\n") + &if self.more > 0 { format!("\n… {} more lines", self.more) } else { String::new() }
    }
}

/// A tool's output, verbatim.
pub(crate) fn output(text: &str) -> Lines {
    Lines::head(
        text.lines()
            .map(|l| match l.char_indices().nth(LINE_MAX_CHARS) {
                Some((cut, _)) => format!("{}…", &l[..cut]),
                None => l.to_string(),
            })
            .collect(),
    )
}

/// `line` cut into chunks of `cols` chars.
pub(crate) fn wrap(line: &str, cols: usize) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= cols {
        return vec![line.to_string()];
    }
    chars.chunks(cols).map(|c| c.iter().collect()).collect()
}

/// Chips the user opened or closed, by key; the rest keep their default.
#[derive(Default)]
pub(crate) struct Details(HashMap<String, bool>);

impl Details {
    pub(crate) fn open(&self, key: &str, default: bool) -> bool {
        self.0.get(key).copied().unwrap_or(default)
    }

    /// Flips the chip and returns whether it's now open.
    pub(crate) fn toggle(&mut self, key: &str, default: bool) -> bool {
        let open = !self.open(key, default);
        self.0.insert(key.to_string(), open);
        open
    }
}

/// Lines as faint 18px rows that clip instead of wrapping, in the code font when `mono`, then the count of the rest.
pub(crate) fn body(l: &Lines, mono: bool) -> Div {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        .py(px(6.))
        .text_size(px(TEXT_SIZE))
        .text_color(TEXT_3)
        .when(mono, |d| d.font_family(code_font()))
        .children(l.lines.iter().map(|line| div().h(px(LINE)).w_full().min_w_0().overflow_hidden().whitespace_nowrap().child(line.clone())))
        .when(l.more > 0, |d| d.child(div().h(px(LINE)).font_family(ui_font()).child(format!("… {} more lines", l.more))))
}

/// Lines as preformatted code: a failed call's error, kept whole in wrapped lines under 240px.
pub(crate) fn pre(l: &Lines, colour: Token, opacity: f32) -> Div {
    let text = l.text();
    div()
        .w_full()
        .min_w_0()
        .py(px(4.))
        .max_h(px(240.))
        .overflow_hidden()
        .font_family(code_font())
        .text_size(px(12.))
        .line_height(px(20.))
        .text_color(colour)
        .opacity(opacity)
        .child(text)
}

#[cfg(test)]
mod tests {
    use super::{Details, MAX_LINES, output};

    #[test]
    fn output_over_24_lines_keeps_the_head_and_counts_the_rest() {
        let text: String = (1..=30).map(|i| format!("line {i}\n")).collect::<String>() + "\n\n";
        let out = output(&text);
        assert_eq!(out.lines.len(), MAX_LINES);
        assert_eq!(out.lines[0], "line 1");
        assert_eq!(out.lines[23], "line 24");
        assert_eq!(out.more, 6);
        assert_eq!(output("ok\n").more, 0);
    }

    #[test]
    fn a_users_choice_overrides_a_chips_default() {
        let mut d = Details::default();
        assert!(d.open("thought", true) && !d.open("read", false));
        assert!(!d.toggle("thought", true));
        assert!(!d.open("thought", true));
    }
}
