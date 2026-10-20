use gpui_kit::base::{TextSelectionHandle, TextSelectionRegistration, TextSelectionRun};
use gpui_kit::*;
use theme::SELECTION;

/// Text that joins the window's text selection: a drag, a double or triple click, and copy.
/// `child` lays out `text` as `layout`; wrapping it rather than the text keeps what `child` adds, such as tooltips.
pub(super) struct Selectable {
    id: ElementId,
    text: SharedString,
    layout: TextLayout,
    child: AnyElement,
}

pub(super) fn selectable(id: impl Into<ElementId>, text: SharedString, layout: TextLayout, child: impl IntoElement) -> Selectable {
    Selectable { id: id.into(), text, layout, child: child.into_any_element() }
}

impl IntoElement for Selectable {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Selectable {
    type RequestLayoutState = TextSelectionHandle;
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, id: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, TextSelectionHandle) {
        let handle = window.with_element_state(id.expect("Selectable has an id"), |handle: Option<TextSelectionHandle>, window| {
            let handle = handle.unwrap_or_else(|| {
                let handle = TextSelectionHandle::new(self.text.clone(), cx);
                handle.refresh_window_on_change(window, cx).detach();
                handle
            });
            (handle.clone(), handle)
        });
        (self.child.request_layout(window, cx), handle)
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, handle: &mut TextSelectionHandle, window: &mut Window, cx: &mut App) -> Hitbox {
        self.child.prepaint(window, cx);
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        handle.register(TextSelectionRegistration::new(hitbox.clone(), bounds).with_text_bounds(vec![bounds]), window, cx);
        hitbox
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, handle: &mut TextSelectionHandle, hitbox: &mut Hitbox, window: &mut Window, cx: &mut App) {
        window.set_cursor_style(CursorStyle::IBeam, hitbox);
        let projection = handle.update_runs(&[TextSelectionRun::new(self.text.clone(), self.layout.clone(), bounds)], cx);
        let line = self.layout.line_height();
        for range in projection.ranges().iter().flatten() {
            let (Some(start), Some(end)) = (self.layout.position_for_index(range.start), self.layout.position_for_index(range.end)) else { continue };
            for band in bands(start, end, bounds, line) {
                window.paint_quad(fill(band, Hsla::from(SELECTION)));
            }
        }
        self.child.paint(window, cx);
    }
}

/// The boxes a selection from `start` to `end` covers: its first line from `start`, the full lines between, and its last line up to `end`.
fn bands(start: Point<Pixels>, end: Point<Pixels>, bounds: Bounds<Pixels>, line: Pixels) -> Vec<Bounds<Pixels>> {
    if start.y == end.y {
        return vec![Bounds::from_corners(start, point(end.x, end.y + line))];
    }
    let mut out = vec![Bounds::from_corners(start, point(bounds.right(), start.y + line))];
    if end.y > start.y + line {
        out.push(Bounds::from_corners(point(bounds.left(), start.y + line), point(bounds.right(), end.y)));
    }
    out.push(Bounds::from_corners(point(bounds.left(), end.y), point(end.x, end.y + line)));
    out
}

#[cfg(test)]
mod tests {
    use super::bands;
    use gpui_kit::{Bounds, point, px, size};

    #[test]
    fn a_selection_over_three_lines_covers_its_ends_and_the_whole_line_between() {
        let bounds = Bounds::new(point(px(0.), px(0.)), size(px(100.), px(60.)));
        let out = bands(point(px(40.), px(0.)), point(px(20.), px(40.)), bounds, px(20.));
        assert_eq!(
            out,
            vec![
                Bounds::from_corners(point(px(40.), px(0.)), point(px(100.), px(20.))),
                Bounds::from_corners(point(px(0.), px(20.)), point(px(100.), px(40.))),
                Bounds::from_corners(point(px(0.), px(40.)), point(px(20.), px(60.))),
            ]
        );
    }
}
