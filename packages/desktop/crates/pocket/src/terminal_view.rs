pub(crate) mod pane;
pub(crate) mod surface;
pub(crate) mod tab_menu;
pub(crate) mod tabs;

use crate::actions::NewTab;
use crate::desktop::Desktop;
use crate::desktop::chrome::{Overlay, drag_area};
use gpui_kit::*;
use std::ops::Range;
use theme::*;
use workspace::{Doc, Tab};

pub struct TerminalViewState {
    pub(crate) focus: FocusHandle,
    pub(crate) focused: Option<String>,
    pub(crate) marked: Option<usize>,
    pub(crate) tab_scroll: ScrollHandle,
    /// The worktree and tab last scrolled into view, so a tab is revealed once when it becomes active rather than every frame.
    pub(crate) tab_revealed: Option<(String, usize)>,
    pub(crate) tab_menu: bool,
}

impl TerminalViewState {
    pub fn new(cx: &mut Context<Desktop>) -> Self {
        Self { focus: cx.focus_handle(), focused: None, marked: None, tab_scroll: ScrollHandle::new(), tab_revealed: None, tab_menu: false }
    }
}

impl Desktop {
    pub fn focus_pane(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.terminal.focused = Some(id);
        window.focus(&self.terminal.focus, cx);
        cx.notify();
    }

    pub fn new_tab(&mut self, _: &NewTab, _: &mut Window, cx: &mut Context<Self>) {
        self.terminal.tab_menu = false;
        self.new_shell(None, cx);
    }

    pub(crate) fn on_term_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.terminal.focused.clone() else { return };
        let Some(s) = self.terminals.sessions.get(&id) else { return };
        let app_cursor = s.term.as_ref().is_some_and(|t| t.app_cursor());
        if let Some(bytes) = keys::key_bytes(&ev.keystroke, app_cursor) {
            self.daemon.input(&id, &bytes);
            cx.stop_propagation();
        }
    }

    pub(crate) fn session_page(&mut self, tree: &str, cx: &mut Context<Self>) -> Div {
        let (added, removed) = self.repo().map(|r| r.totals()).unwrap_or_default();
        let diff = (added + removed > 0).then(|| {
            div()
                .id("bar-diff")
                .flex_none()
                .mr(px(4.))
                .cursor_pointer()
                .child(ui::meta_diff(added, removed, 12.))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open_changes(None, cx)))
        });
        let error = self.error.clone().map(|e| div().min_w_0().truncate().mr(px(6.)).text_size(px(12.5)).text_color(rgba(FAILED)).child(e));
        let status = div().ml_auto().pl(px(8.)).min_w_0().flex().items_center().children(error);
        let right = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .child(ui::icon_group([
                ui::group_button("split-right", "split-right").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_shell(Some(false), cx))),
                ui::group_button("split-down", "split-down").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_shell(Some(true), cx))),
            ]))
            .child(ui::icon_group([
                ui::group_button("session-more", "more").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::More, window, cx))),
            ]));
        let (pad, toggle) = self.bar_start(cx);
        let bar = drag_area(div())
            .h(px(42.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.))
            .pl(px(pad))
            .pr(px(10.))
            .children(toggle)
            .child(self.term_tabs(tree, cx))
            .child(status)
            .children(diff)
            .child(right);
        let body = match self.workspace(tree).active().cloned() {
            Some(Tab::Term(rows)) => self.panes(rows, cx),
            Some(Tab::Doc(Doc::File(p))) if self.preview.file.as_ref() == Some(&p) => self.file_view(cx),
            Some(Tab::Doc(Doc::Diff(p))) if self.diff.file.as_ref() == Some(&p) => self.diff_view(cx),
            Some(Tab::Doc(_)) | None => div().flex_1(),
        };
        div().flex_1().min_h_0().flex().flex_col().bg(rgba(SURFACE_SUNKEN)).child(bar).child(body)
    }
}

/// Typed text arrives here rather than as key-downs so IMEs (Telex, dead keys, CJK) can compose it.
impl EntityInputHandler for Desktop {
    fn text_for_range(&mut self, _: Range<usize>, _: &mut Option<Range<usize>>, _: &mut Window, _: &mut Context<Self>) -> Option<String> {
        None
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        Some(UTF16Selection { range: 0..0, reversed: false })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.terminal.marked.map(|len| 0..len)
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.terminal.marked = None;
    }

    fn replace_text_in_range(&mut self, _: Option<Range<usize>>, text: &str, _: &mut Window, _: &mut Context<Self>) {
        self.terminal.marked = None;
        if let Some(id) = &self.terminal.focused {
            self.daemon.input(id, text.as_bytes());
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.terminal.marked = (!text.is_empty()).then(|| text.encode_utf16().count());
    }

    fn bounds_for_range(&mut self, _: Range<usize>, _: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        None
    }

    fn character_index_for_point(&mut self, _: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        None
    }
}
