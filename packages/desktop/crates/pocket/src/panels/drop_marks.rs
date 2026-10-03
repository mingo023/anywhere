use super::drop::{self, Aim};
use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;
use workspace::Tab;
use workspace::tree::{Target, Tree};

impl Desktop {
    /// The caret or lit pane where the held tab would drop, and the tab itself under the pointer.
    pub(super) fn drop_marks(&self, tree: &str) -> Option<Deferred> {
        let ((from, d), (x, y)) = (self.panels.drag.as_ref()?, self.panels.at?);
        if !d.away {
            return None;
        }
        let t = &self.workspaces[tree].tree;
        let o = self.panels.bounds;
        let mark = match self.panels.aim {
            Some(Aim::To(target)) => self.drop_mark(t, target, y),
            _ => None,
        };
        let tab = t.pane(*from)?.tabs.get(d.from)?;
        let ghost = div()
            .absolute()
            .left(px(x - d.grab - o.x))
            .top(px(y - 14. - o.y))
            .h(px(28.))
            .px(px(10.))
            .flex()
            .items_center()
            .rounded(px(7.))
            .bg(SURFACE)
            .shadow(ui::row_shadow())
            .text_size(px(12.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(TEXT)
            .whitespace_nowrap()
            .child(self.tab_lead(tab, false, TEXT));
        Some(deferred(div().absolute().inset_0().children(mark).child(ghost)))
    }

    /// The caret on `target`'s strip, or the part of its pane lit, with the pointer at height `y`.
    fn drop_mark(&self, t: &Tree<Tab>, target: Target, y: f32) -> Option<Div> {
        let o = self.panels.bounds;
        let layout = t.drawn_layout(o);
        let rect_of = |p| layout.iter().find(|&&(q, _)| q == p).map(|&(_, r)| r);
        Some(match target {
            Target::Into { pane, index } if rect_of(pane).is_some_and(|r| y - r.y < drop::STRIP_H) => {
                let r = rect_of(pane)?;
                let caret = drop::caret(self.panels.slots.get(&pane).map_or(&[][..], Vec::as_slice), index).unwrap_or(r.x + 12.).min(r.x + r.w - 12.).max(r.x + 12.);
                div().absolute().left(px(caret - 1. - o.x)).top(px(r.y + 11. - o.y)).w(px(2.)).h(px(20.)).rounded(px(1.)).bg(ACCENT)
            }
            Target::Into { pane, .. } | Target::Split { pane, .. } => {
                let edge = if let Target::Split { edge, .. } = target { Some(edge) } else { None };
                let h = drop::highlight(rect_of(pane)?, edge);
                div().absolute().left(px(h.x - o.x)).top(px(h.y - o.y)).w(px(h.w)).h(px(h.h)).rounded(px(8.)).border_1().border_color(ACCENT).bg(Hsla::from(ACCENT).opacity(0.12))
            }
        })
    }
}
