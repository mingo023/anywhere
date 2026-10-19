use super::tool_detail::{Lines, MAX_LINES};

/// Columns a thought wraps at, counted in chars so its height is known without layout.
const WRAP: usize = 96;

/// A thought's lines, word-wrapped. Past the cap a live thought keeps its tail, the fresh thinking, and a settled one its head.
pub(crate) fn lines(text: &str, live: bool) -> Lines {
    let mut lines: Vec<String> = text.trim_end().lines().flat_map(wrap).collect();
    let more = lines.len().saturating_sub(MAX_LINES);
    if live && more > 0 {
        lines.drain(..more);
        // The cut can land on a paragraph break.
        while lines.first().is_some_and(|l| l.trim().is_empty()) {
            lines.remove(0);
        }
        return Lines { lines, more };
    }
    Lines::head(lines)
}

/// `line` word-wrapped at `WRAP` columns; a word longer than that is cut.
fn wrap(line: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    for word in line.split_whitespace() {
        for piece in super::tool_detail::wrap(word, WRAP) {
            let last = out.last_mut().expect("starts with a line");
            let len = last.chars().count();
            if len == 0 {
                last.push_str(&piece);
            } else if len + 1 + piece.chars().count() <= WRAP {
                last.push(' ');
                last.push_str(&piece);
            } else {
                out.push(piece);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::lines;

    #[test]
    fn live_thinking_keeps_its_tail_and_settled_thinking_its_head() {
        let text: String = (1..=30).map(|i| format!("step {i}\n")).collect();
        let live = lines(&text, true);
        assert_eq!((live.lines.first().unwrap().as_str(), live.lines.last().unwrap().as_str(), live.more), ("step 7", "step 30", 6));
        let settled = lines(&text, false);
        assert_eq!((settled.lines.first().unwrap().as_str(), settled.lines.last().unwrap().as_str(), settled.more), ("step 1", "step 24", 6));
    }

    #[test]
    fn a_long_thought_wraps_between_words_at_96_columns() {
        let text = "word ".repeat(40);
        let l = lines(&text, false).lines;
        assert!(l.len() == 3 && l.iter().all(|l| l.chars().count() <= 96 && !l.starts_with(' ')));
    }
}
