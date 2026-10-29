use super::drop::{caret, half, pane_aim, top_index};
use super::strip::slots;
use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;
use workspace::tree::{Edge, PaneId, Rect};

/// What a pane drag carries: the pane held. The marks show where it goes, so the drag has no preview.
pub(super) struct DragPane(pub PaneId);

/// What is held: a pane of the shown tree, or a top tab.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Source {
    Pane(PaneId),
    Top(usize),
}

/// Where a held pane or top tab would drop: at an edge of a pane, or among the top tabs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Drop {
    Pane(PaneId, Edge),
    Top(usize),
}

/// A pane or top tab held over the bar and the panes.
pub(super) struct Held {
    pub(super) from: Source,
    aim: Option<Drop>,
    /// The pointer, in the window.
    at: (f32, f32),
}

impl Desktop {
    /// Lets pane `p` be dragged by `handle`, which keeps the press from the elements under it.
    pub(super) fn pane_handle(&self, handle: Stateful<Div>, p: PaneId, cx: &mut Context<Self>) -> Stateful<Div> {
        let this = cx.weak_entity();
        handle.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).on_drag(DragPane(p), move |_, _, window, cx| {
            let at = window.mouse_position();
            this.update(cx, |this, cx| {
                this.panels.pane_drag = Some(Held { from: Source::Pane(p), aim: None, at: (at.x.into(), at.y.into()) });
                this.freeze_pages(window);
                cx.notify();
            })
            .ok();
            cx.new(|_| EmptyView)
        })
    }

    /// Follows a held pane over the bar and worktree `tree`'s panes, redrawing only when where it would drop changes.
    pub(super) fn aim_pane(&mut self, tree: &str, at: (f32, f32), cx: &mut Context<Self>) {
        let Some(held) = &self.panels.pane_drag else { return };
        if held.at == at {
            return;
        }
        let from = match held.from {
            Source::Pane(p) => Some(p),
            Source::Top(_) => None,
        };
        let Some(w) = self.workspaces.get(tree) else { return };
        let aim = match self.bar_index(tree, at) {
            Some(i) => Some(Drop::Top(i)),
            None => pane_aim(&w.tree.drawn_layout(self.panels.bounds), at, from).map(|(p, e)| Drop::Pane(p, e)),
        };
        let Some(held) = self.panels.pane_drag.as_mut() else { return };
        held.at = at;
        if held.aim != aim {
            held.aim = aim;
            cx.notify();
        }
    }

    /// Moves pane `from` where it was aimed: to an edge of another pane, or into a top tab of its own.
    pub(super) fn drop_pane(&mut self, from: PaneId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Held { aim, .. }) = self.panels.pane_drag.take() else { return };
        let Some(tree) = self.place() else { return };
        let w = self.workspace(&tree);
        match aim {
            Some(Drop::Pane(to, edge)) => w.tree.move_pane(from, to, edge),
            Some(Drop::Top(i)) => w.detach_pane(from, i),
            None => return cx.notify(),
        }
        self.show_focused(&tree, window, cx);
        self.save_soon(cx);
        cx.notify();
    }

    /// Follows top tab `i`, held below the bar, over worktree `tree`'s panes, redrawing only when where it would drop changes.
    pub(super) fn aim_top(&mut self, i: usize, tree: &str, at: (f32, f32), cx: &mut Context<Self>) {
        let Some(w) = self.workspaces.get(tree) else { return };
        let aim = pane_aim(&w.tree.drawn_layout(self.panels.bounds), at, None).map(|(p, e)| Drop::Pane(p, e));
        let held = self.panels.pane_drag.get_or_insert(Held { from: Source::Top(i), aim: None, at });
        held.at = at;
        if held.aim != aim {
            held.aim = aim;
            cx.notify();
        }
    }

    /// Moves the held top tab's panes beside the pane it was aimed at; true if it was aimed at one.
    pub(super) fn drop_top(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(Held { from: Source::Top(i), aim: Some(Drop::Pane(pane, edge)), .. }) = self.panels.pane_drag.take() else { return false };
        let Some(tree) = self.place() else { return false };
        self.workspace(&tree).place_top(i, pane, edge);
        self.show_focused(&tree, window, cx);
        self.save_soon(cx);
        cx.notify();
        true
    }

    /// Lets go of a held pane, top tab or tab without moving it; true if one was held.
    pub(crate) fn cancel_drag(&mut self) -> bool {
        let tab = self.panels.drag.take_if(|(_, d)| !d.released).is_some();
        let top = self.panels.top_drag.take_if(|d| !d.released).is_some();
        self.panels.pane_drag.take().is_some() | tab | top
    }

    /// Which top tab of worktree `tree` a drop at `(x, y)` lands before, if the pointer is on the bar.
    pub(super) fn bar_index(&self, tree: &str, (x, y): (f32, f32)) -> Option<usize> {
        let b = self.panels.bar;
        (b.x <= x && x < b.x + b.w && b.y <= y && y < b.y + b.h).then(|| top_index(&self.top_slots(tree), x))
    }

    fn top_slots(&self, tree: &str) -> Vec<(f32, f32)> {
        self.workspaces.get(tree).map(|w| slots(&self.panels.top_scroll, w.others.len() + 1)).unwrap_or_default()
    }

    /// The caret before top tab `i` of worktree `tree`, within the box whose origin is `o`.
    pub(super) fn top_caret(&self, tree: &str, i: usize, o: Rect) -> Option<Div> {
        let x = caret(&self.top_slots(tree), i)?;
        let b = self.panels.bar;
        Some(div().absolute().left(px(x - 1. - o.x)).top(px(b.y + 7. - o.y)).w(px(2.)).h(px(26.)).rounded(px(1.)).bg(ACCENT))
    }

    /// Where the held pane would drop in worktree `tree`: the half of the pane it would take, edged on its side, or a caret between top tabs.
    /// Drawn within the box that starts at the bar.
    pub(super) fn pane_marks(&self, tree: &str) -> Option<Deferred> {
        let aim = self.panels.pane_drag.as_ref()?.aim?;
        let (o, w) = (self.panels.bar, self.workspaces.get(tree)?);
        let place = |r: Rect| div().absolute().left(px(r.x - o.x)).top(px(r.y - o.y)).w(px(r.w)).h(px(r.h));
        let mark = match aim {
            Drop::Pane(p, edge) => {
                let r = w.tree.drawn_layout(self.panels.bounds).into_iter().find(|&(q, _)| q == p)?.1;
                let line = match edge {
                    Edge::Left => Rect { w: 2., ..r },
                    Edge::Right => Rect { x: r.x + r.w - 2., w: 2., ..r },
                    Edge::Top => Rect { h: 2., ..r },
                    Edge::Bottom => Rect { y: r.y + r.h - 2., h: 2., ..r },
                };
                div().absolute().inset_0().child(place(half(r, edge)).bg(Hsla::from(ACCENT).opacity(0.15))).child(place(line).bg(ACCENT))
            }
            Drop::Top(i) => self.top_caret(tree, i, o)?,
        };
        Some(deferred(div().absolute().inset_0().child(mark)))
    }
}
