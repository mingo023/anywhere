use super::pane;
use git::{Kind, Line};
use gpui_kit::component::input::{Editor, EditorState, RopeExt};
use gpui_kit::*;
use std::rc::Rc;
use theme::*;

const BAR: Pixels = px(3.);
/// The editor leaves 10px between its gutter and the code; the bar sits in it.
const BAR_INSET: Pixels = px(7.);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mark {
    Added,
    Modified,
    /// Lines were removed after this one, or before the first when it's 0.
    Deleted,
}

/// What git changed, by new-side line number in order: modified where a line replaced another, added where it was only added.
pub fn gutter(lines: &[Line]) -> Vec<(usize, Mark)> {
    let (mut marks, mut last, mut i) = (Vec::new(), 0, 0);
    while i < lines.len() {
        let dels = lines[i..].iter().take_while(|l| l.kind == Kind::Del).count();
        let adds = lines[i + dels..].iter().take_while(|l| l.kind == Kind::Add).count();
        if dels + adds == 0 {
            last = lines[i].new.unwrap_or(last);
            i += 1;
            continue;
        }
        if adds == 0 {
            marks.push((last, Mark::Deleted));
        }
        for (k, n) in lines[i + dels..i + dels + adds].iter().filter_map(|l| l.new).enumerate() {
            marks.push((n, if k < dels { Mark::Modified } else { Mark::Added }));
            last = n;
        }
        i += dels + adds;
    }
    marks
}

/// The marks of the lines on screen, painted over the editor's gutter from the layout it just painted, so they follow scrolling and folds.
fn hunks(code: Entity<EditorState>, marks: Rc<[(usize, Mark)]>) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            let state = code.read(cx);
            let (Some(rows), Some(text)) = (state.visible_row_range(), state.text_bounds()) else { return };
            let rope = state.text();
            let row = |row: usize| {
                let at = rope.line_start_offset(row);
                state.range_to_bounds(&(at..at))
            };
            // The editor gives a row it didn't lay out, folded away or scrolled off, the bounds of the next one it did.
            let shown = |r: usize| row(r).filter(|b| r + 1 >= rope.lines_len() || row(r + 1) != Some(*b));
            let start = marks.partition_point(|&(line, _)| line < rows.start);
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                for &(line, mark) in marks[start..].iter().take_while(|&&(line, _)| line <= rows.end) {
                    let at = match mark {
                        Mark::Deleted => line.checked_sub(1).and_then(shown).map(|b| (b, b.bottom())).or_else(|| shown(line).map(|b| (b, b.top()))),
                        Mark::Added | Mark::Modified => shown(line - 1).map(|b| (b, b.top())),
                    };
                    let Some((b, y)) = at else { continue };
                    let x = state.input_bounds().origin.x + (b.origin.x - text.origin.x) - BAR_INSET;
                    match mark {
                        Mark::Deleted => {
                            let mut wedge = PathBuilder::fill();
                            wedge.add_polygon(&[point(x, y - px(4.)), point(x + px(4.), y), point(x, y + px(4.))], true);
                            if let Ok(p) = wedge.build() {
                                window.paint_path(p, Hsla::from(FAILED));
                            }
                        }
                        Mark::Added | Mark::Modified => {
                            let color = if mark == Mark::Added { SUCCESS } else { MODIFIED };
                            window.paint_quad(fill(Bounds::new(point(x, y), size(BAR, b.size.height)), Hsla::from(color)));
                        }
                    }
                }
            });
        },
    )
    .absolute()
    .inset_0()
}

/// The editor a text file shows and is edited in.
pub fn code_pane(code: &Entity<EditorState>, marks: Rc<[(usize, Mark)]>) -> Div {
    pane()
        .relative()
        .bg(SURFACE_SUNKEN)
        .child(Editor::new(code).bordered(false).size_full().font_family(MONO).text_size(px(13.)).line_height(px(22.)))
        .child(hunks(code.clone(), marks))
}

#[cfg(test)]
mod tests {
    use super::{Mark, gutter};
    use git::{Kind, Line};

    fn l(kind: Kind, old: Option<usize>, new: Option<usize>) -> Line {
        Line { kind, old, new, text: String::new() }
    }

    #[test]
    fn gutter_tells_modified_from_added() {
        let lines = vec![
            l(Kind::Hunk, None, None),
            l(Kind::Context, Some(1), Some(1)),
            l(Kind::Del, Some(2), None),
            l(Kind::Add, None, Some(2)),
            l(Kind::Add, None, Some(3)),
        ];
        assert_eq!(gutter(&lines), [(2, Mark::Modified), (3, Mark::Added)]);
    }

    #[test]
    fn gutter_marks_removed_lines_after_the_line_before_them() {
        let lines = vec![
            l(Kind::Hunk, None, None),
            l(Kind::Del, Some(1), None),
            l(Kind::Context, Some(2), Some(1)),
            l(Kind::Context, Some(3), Some(2)),
            l(Kind::Del, Some(4), None),
            l(Kind::Del, Some(5), None),
            l(Kind::Context, Some(6), Some(3)),
        ];
        assert_eq!(gutter(&lines), [(0, Mark::Deleted), (2, Mark::Deleted)]);
    }

    #[test]
    fn lines_replaced_by_fewer_show_as_modified_only() {
        let lines = vec![l(Kind::Hunk, None, None), l(Kind::Del, Some(1), None), l(Kind::Del, Some(2), None), l(Kind::Add, None, Some(1))];
        assert_eq!(gutter(&lines), [(1, Mark::Modified)]);
    }
}
