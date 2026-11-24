use crate::automations::Automations;
use crate::desktop::Desktop;
use crate::desktop::chrome::{Column, SEAM, Screen, seam_handle};
use crate::inbox::Inbox;
use git::Repo;
use gpui_kit::*;
use theme::*;
use workspace::tree::Axis;

/// Whether the worktree has any changed file.
pub(crate) fn has_changes(repo: Option<&Repo>) -> bool {
    repo.is_some_and(|r| !r.files.is_empty())
}

/// Marks a sidebar toggle while the hidden Changes list has something in it.
pub(crate) fn changes_dot() -> Div {
    ui::dot(6., ACCENT).absolute().top(px(5.)).right(px(5.)).shadow(vec![ui::ring(RAIL_CUTOUT, 1.5)])
}

/// How many files the Changes list holds, at the bottom right of its 32px rail icon.
pub(crate) fn changes_badge(n: usize) -> Div {
    ui::count_badge(n).top(px(19.)).right(px(-4.)).bg(PRIMARY).text_color(ON_PRIMARY).shadow(vec![ui::ring(RAIL_CUTOUT, 2.)])
}

/// What the column left of the page lists.
#[derive(PartialEq)]
pub(crate) enum ColumnBody {
    Inbox(Entity<Inbox>),
    Automations(Entity<Automations>),
}

/// What `Desktop` hands the column each frame.
#[derive(PartialEq)]
pub(crate) struct ColumnFrame {
    pub(crate) width: f32,
    pub(crate) seam: bool,
    pub(crate) body: ColumnBody,
}

/// What the column asks `Desktop` for.
pub(crate) enum ColumnEvent {
    /// Its right edge dragged this far from its left.
    Drag(f32),
    Reset,
}

impl EventEmitter<ColumnEvent> for ColumnView {}

/// The inbox or Automations list left of the page, with its right edge to drag.
#[derive(Default)]
pub(crate) struct ColumnView {
    frame: Option<ColumnFrame>,
}

impl ColumnView {
    pub(crate) fn set(&mut self, frame: ColumnFrame, cx: &mut Context<Self>) {
        if self.frame.as_ref() != Some(&frame) {
            self.frame = Some(frame);
            cx.notify();
        }
    }
}

impl Render for ColumnView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(f) = self.frame.as_ref() else { return div() };
        let list = match &f.body {
            ColumnBody::Automations(a) => a.update(cx, |a, cx| a.column(cx)).w(px(f.width)),
            ColumnBody::Inbox(i) => div().h_full().flex_none().child(i.clone()),
        };
        if !f.seam {
            return list;
        }
        let col = Column::Sessions;
        let handle = seam_handle(("resize-column", col as usize), Axis::Row, |_: &mut Self, cx| cx.emit(ColumnEvent::Reset), cx).right(px(-SEAM / 2.)).on_drag(
            col,
            |_, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| EmptyView)
            },
        );
        // Deferred so the handle straddles the edge: `column()` clips its children.
        list.relative().child(deferred(handle)).on_drag_move(cx.listener(move |_, e: &DragMoveEvent<Column>, _, cx| {
            if *e.drag(cx) == col {
                cx.emit(ColumnEvent::Drag(f32::from(e.event.position.x - e.bounds.left())));
            }
        }))
    }
}

impl Desktop {
    /// The inbox or Automations list left of the page.
    pub(crate) fn column_view(&mut self, cx: &mut Context<Self>) -> Entity<ColumnView> {
        let width = self.width(Column::Sessions, 348.);
        let body = if self.screen == Screen::Automations {
            let side = self.automations_side(cx);
            self.automations.update(cx, |a, _| a.set(side));
            ColumnBody::Automations(self.automations.clone())
        } else {
            ColumnBody::Inbox(self.inbox_list(width, cx))
        };
        let frame = ColumnFrame { width, seam: self.overlay.is_none(), body };
        let view = self.sidebar.read(cx).column.clone();
        view.update(cx, |v, cx| v.set(frame, cx));
        view
    }

    pub(crate) fn on_column_event(&mut self, _: &Entity<ColumnView>, ev: &ColumnEvent, _: &mut Window, cx: &mut Context<Self>) {
        match ev {
            ColumnEvent::Drag(width) => self.drag_edge(Column::Sessions, *width, cx),
            ColumnEvent::Reset => self.reset_width(Column::Sessions, cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::has_changes;
    use git::{FileStat, Repo};

    #[test]
    fn a_clean_worktree_has_no_changes() {
        assert!(!has_changes(Some(&Repo::default())));
        assert!(!has_changes(None));
    }

    #[test]
    fn binary_changes_alone_still_mark_the_worktree() {
        let repo = Repo {
            files: vec![FileStat { path: "a.rs".into(), added: 0, removed: 0, staged: false, unstaged: true, status: 'M', old_path: None }],
            ..Default::default()
        };
        assert!(has_changes(Some(&repo)));
    }
}
