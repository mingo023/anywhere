use crate::desktop::Desktop;
use crate::desktop::chrome::{SEAM, id};
use gpui_kit::*;
use theme::*;
use workspace::tree::{Axis, Divider, UNIT};

impl Desktop {
    /// The seam before cell `j` of the split at `path`: drag it to resize, double-click it to even the split out.
    pub(super) fn divider(&self, path: &[usize], j: usize, axis: Axis, cx: &mut Context<Self>) -> Option<Deferred> {
        // The deferred handle paints above overlays, so it would steal their clicks.
        if self.overlay.is_some() {
            return None;
        }
        let line = div().absolute().group_hover("seam", |s| s.bg(SEPARATOR_STRONG));
        let handle = div().id(id(format!("divider:{path:?}:{j}"))).group("seam").absolute().occlude();
        let (line, handle) = match axis {
            Axis::Row => (line.top_0().bottom_0().left(px(SEAM / 2.)).w(px(1.)), handle.top_0().bottom_0().left(px(-SEAM / 2.)).w(px(SEAM)).cursor_col_resize()),
            Axis::Column => (line.left_0().right_0().top(px(SEAM / 2.)).h(px(1.)), handle.left_0().right_0().top(px(-SEAM / 2.)).h(px(SEAM)).cursor_row_resize()),
        };
        let at = path.to_vec();
        let handle = handle
            .child(line)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    if e.click_count == 2 {
                        this.equalize_split(&at, cx);
                    }
                }),
            )
            .on_drag(Divider { path: path.to_vec(), i: j - 1 }, |_, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| EmptyView)
            });
        Some(deferred(handle))
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
