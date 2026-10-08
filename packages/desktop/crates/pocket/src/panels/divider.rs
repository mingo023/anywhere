use crate::desktop::Desktop;
use crate::desktop::chrome::{SEAM, id};
use gpui_kit::*;
use workspace::tree::{Axis, Divider, UNIT};

impl Desktop {
    /// The seam before cell `j` of the split at `path`: drag it to resize, double-click it to even the split out.
    pub(super) fn divider(&self, path: &[usize], j: usize, axis: Axis, cx: &mut Context<Self>) -> Option<Deferred> {
        let at = path.to_vec();
        let handle = self.seam(id(format!("divider:{path:?}:{j}")), axis, move |this, cx| this.equalize_split(&at, cx), cx)?;
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
        let Some(tree) = self.cwd() else { return };
        let t = &mut self.workspace(&tree).tree;
        let before = t.layout(UNIT);
        t.resize(d, at, len);
        // A drag move comes every frame, moved or not; notifying on each would redraw forever.
        if t.layout(UNIT) != before {
            self.save_soon(cx);
            cx.notify();
        }
    }

    fn equalize_split(&mut self, path: &[usize], cx: &mut Context<Self>) {
        let Some(tree) = self.cwd() else { return };
        self.workspace(&tree).tree.equalize(path);
        self.save_soon(cx);
        cx.notify();
    }
}
