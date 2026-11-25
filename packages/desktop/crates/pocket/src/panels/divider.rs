use super::{Panels, PanelsEvent};
use crate::desktop::chrome::{SEAM, id, seam_handle};
use gpui_kit::*;
use workspace::tree::{Axis, Divider, UNIT};

impl Panels {
    /// The seam before cell `j` of the split at `path`: drag it to resize, double-click it to even the split out.
    pub(super) fn divider(&self, path: &[usize], j: usize, axis: Axis, seams: bool, cx: &mut Context<Self>) -> Option<Deferred> {
        // The deferred handle paints above overlays, so it would steal their clicks.
        if !seams {
            return None;
        }
        let at = path.to_vec();
        let handle = seam_handle(id(format!("divider:{path:?}:{j}")), axis, move |this: &mut Self, cx| this.equalize_split(&at, cx), cx);
        let handle = match axis {
            Axis::Row => handle.left(px(-SEAM / 2.)),
            Axis::Column => handle.top(px(-SEAM / 2.)),
        };
        Some(deferred(handle.on_drag(Divider { path: path.to_vec(), i: j - 1 }, |_, _, _, cx| {
            cx.stop_propagation();
            cx.new(|_| EmptyView)
        })))
    }

    pub(super) fn resize_split(&mut self, d: &Divider, at: f32, len: f32, cx: &mut Context<Self>) {
        let moved = self.edit(cx, |w| {
            let t = &mut w.tree;
            let before = t.layout(UNIT);
            t.resize(d, at, len);
            t.layout(UNIT) != before
        });
        // A drag move comes every frame, moved or not; notifying on each would redraw forever.
        if moved == Some(true) {
            cx.emit(PanelsEvent::Changed);
        }
    }

    fn equalize_split(&mut self, path: &[usize], cx: &mut Context<Self>) {
        self.edit(cx, |w| w.tree.equalize(path));
        cx.emit(PanelsEvent::Changed);
    }
}
