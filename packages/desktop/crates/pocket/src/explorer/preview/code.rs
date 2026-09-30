use super::pane;
use git::{Kind, Line};
use gpui_kit::component::input::{Editor, EditorState, TextDecoration};
use gpui_kit::*;
use std::collections::HashMap;
use theme::*;

/// New-side line numbers that git marks as changed: true where a line replaced another, false where it was only added.
pub fn gutter(lines: &[Line]) -> HashMap<usize, bool> {
    git::split(lines)
        .into_iter()
        .filter_map(|pair| match pair {
            (Some(l), Some(r)) if l != r => Some((lines[r].new?, true)),
            (None, Some(r)) if lines[r].kind == Kind::Add => Some((lines[r].new?, false)),
            _ => None,
        })
        .collect()
}

/// Background tints for the lines git marks as changed: amber where a line was modified, green where it was only added.
pub fn decorations(text: &str, marks: &HashMap<usize, bool>) -> Vec<TextDecoration> {
    text.lines()
        .enumerate()
        .filter_map(|(i, line)| {
            let modified = *marks.get(&(i + 1))?;
            let start = line.as_ptr() as usize - text.as_ptr() as usize;
            let bg = HighlightStyle { background_color: Some(rgba(if modified { WAITING_BG } else { RUNNING_BG }).into()), ..Default::default() };
            Some(TextDecoration::new(start..start + line.len(), bg))
        })
        .collect()
}

/// The read-only editor a text file shows in.
pub fn code_pane(code: &Entity<EditorState>) -> Div {
    pane()
        .bg(rgba(SURFACE_SUNKEN))
        .child(Editor::new(code).readonly(true).bordered(false).size_full().font_family(MONO).text_size(px(13.)).line_height(px(22.)))
}

#[cfg(test)]
mod tests {
    use super::{decorations, gutter};
    use git::{Kind, Line};
    use gpui_kit::rgba;
    use std::collections::HashMap;
    use theme::{RUNNING_BG, WAITING_BG};

    #[test]
    fn tints_changed_lines() {
        let d = decorations("a\nbb\nccc\n", &HashMap::from([(2, true), (3, false)]));
        assert_eq!(d.iter().map(|d| d.range.clone()).collect::<Vec<_>>(), vec![2..4, 5..8]);
        assert_eq!(d[0].style.background_color, Some(rgba(WAITING_BG).into()));
        assert_eq!(d[1].style.background_color, Some(rgba(RUNNING_BG).into()));
    }

    #[test]
    fn gutter_tells_modified_from_added() {
        let l = |kind, old, new| Line { kind, old, new, text: String::new() };
        let lines = vec![
            l(Kind::Hunk, None, None),
            l(Kind::Context, Some(1), Some(1)),
            l(Kind::Del, Some(2), None),
            l(Kind::Add, None, Some(2)),
            l(Kind::Add, None, Some(3)),
        ];
        let marks = gutter(&lines);
        assert_eq!(marks.get(&2), Some(&true));
        assert_eq!(marks.get(&3), Some(&false));
        assert_eq!(marks.get(&1), None);
    }
}
