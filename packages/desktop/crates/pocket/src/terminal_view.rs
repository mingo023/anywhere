pub(crate) mod surface;
pub(crate) mod tabs;

use crate::actions::NewTab;
use crate::desktop::Desktop;
use crate::desktop::chrome::{Overlay, drag_area, id};
use crate::status;
use crate::terminal_view::surface::Metrics;
use crate::util::basename;
use daemon::Info;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::ops::Range;
use theme::*;
use ui::{self, icon_button_sized};
use workspace::{Doc, Tab};

/// "/bin/zsh -l" reads as "zsh": the login flag the desktop adds says nothing about the pane.
/// A login shell shows as its name only: the script it may run to start an agent is ours, not the user's.
pub fn command_line(info: &Info) -> String {
    if info.args.first().is_some_and(|a| a == "-l") {
        return basename(&info.cmd);
    }
    std::iter::once(basename(&info.cmd)).chain(info.args.iter().cloned()).collect::<Vec<_>>().join(" ")
}

impl Desktop {
    pub fn focus_pane(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.focused = Some(id);
        window.focus(&self.term_focus, cx);
        cx.notify();
    }

    pub fn new_tab(&mut self, _: &NewTab, _: &mut Window, cx: &mut Context<Self>) {
        self.tab_menu = false;
        self.new_shell(None, cx);
    }

    pub(crate) fn on_term_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.focused.clone() else { return };
        let Some(s) = self.sessions.get(&id) else { return };
        let app_cursor = s.term.as_ref().is_some_and(|t| t.app_cursor());
        if let Some(bytes) = keys::key_bytes(&ev.keystroke, app_cursor) {
            self.daemon.input(&id, &bytes);
            cx.stop_propagation();
        }
    }

    pub fn pane_label(&self, id: &str) -> String {
        match (self.summary(id), self.sessions.get(id)) {
            (Some(a), _) => a.provider.clone(),
            (None, Some(s)) => s.busy().map_or_else(|| command_line(&s.info), str::to_string),
            (None, None) => "session".into(),
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
            Some(Tab::Doc(Doc::File(p))) if self.file.as_ref() == Some(&p) => self.file_view(cx),
            Some(Tab::Doc(Doc::Diff(p))) if self.diff_file.as_ref() == Some(&p) => self.diff_view(cx),
            Some(Tab::Doc(_)) | None => div().flex_1(),
        };
        div().flex_1().min_h_0().flex().flex_col().bg(rgba(SURFACE_SUNKEN)).child(bar).child(body)
    }

    fn panes(&mut self, rows: Vec<Vec<String>>, cx: &mut Context<Self>) -> Div {
        let split = rows.len() > 1 || rows[0].len() > 1;
        let mut n = 0;
        let mut out = Vec::new();
        for (r, row) in rows.into_iter().enumerate() {
            let m = if r == 0 { &surface::MAIN } else { &surface::SMALL };
            let panes: Vec<Div> = row
                .into_iter()
                .map(|id| {
                    n += 1;
                    self.pane(&id, split.then_some(n), m, cx)
                })
                .collect();
            out.push(div().flex().gap(px(0.5)).min_h_0().when(r == 0, |d| d.flex_1()).when(r > 0, |d| d.h(px(250.)).flex_none()).children(panes));
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(0.5))
            .bg(rgba(SEPARATOR))
            .border_t(px(0.5))
            .border_color(rgba(SEPARATOR))
            .key_context(keys::CONTEXT)
            .track_focus(&self.term_focus)
            .on_key_down(cx.listener(Self::on_term_key))
            .children(out)
    }

    pub fn pane(&mut self, id: &str, n: Option<usize>, m: &'static Metrics, cx: &mut Context<Self>) -> Div {
        let focused = self.focused.as_deref() == Some(id);
        let exit = self.sessions.get(id).and_then(|s| s.exit);
        let known = self.sessions.get(id).is_some();
        let title = match self.summary(id) {
            Some(a) => format!("{} — {}", a.provider, basename(&a.cwd)),
            None => self.pane_label(id),
        };
        let banner = self.summary(id).and_then(status::banner).map(|text| {
            div().flex_none().px(px(16.)).py(px(6.)).border_b(px(0.5)).border_color(rgba(SEPARATOR)).bg(rgba(FILL_2)).text_size(px(12.)).text_color(rgba(TEXT_2)).child(text)
        });
        let body = match self.sessions.get_mut(id).and_then(|s| s.term.as_mut()) {
            Some(t) => {
                let (f, cells) = t.frame();
                surface::screen(&f, cells, m)
            }
            None => div().text_color(rgba(TEXT_3)).child(if known { "Connecting…" } else { "This session is not running." }),
        };
        let close_id = id.to_string();
        let header = n.map(|n| {
            div()
                .h(px(30.))
                .flex_none()
                .pl(px(16.))
                .pr(px(6.))
                .flex()
                .items_center()
                .border_b(px(0.5))
                .border_color(rgba(SEPARATOR))
                .text_color(rgba(if focused { TEXT } else { TEXT_3 }))
                .font_family(MONO)
                .text_size(px(11.5))
                .child(div().flex_1().truncate().child(format!("{n} · {title}")))
                .child(icon_button_sized(self::id(format!("close-{close_id}")), "x", 22., TEXT_3).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.close_pane(&close_id, cx);
                })))
        });
        let focus_id = id.to_string();
        let screen = div()
            .relative()
            .size_full()
            .overflow_hidden()
            .child(surface::surface(cx.entity(), id.to_string(), m, focused.then(|| self.term_focus.clone())))
            .child(body);
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgba(SURFACE_SUNKEN))
            .when(focused && n.is_some(), |d| d.shadow(vec![BoxShadow { inset: true, ..ui::ring(SEPARATOR_STRONG, 0.5) }]))
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _: &MouseDownEvent, window, cx| this.focus_pane(focus_id.clone(), window, cx)))
            .children(header)
            .children(banner)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .pt(px(10.))
                    .px(px(20.))
                    .pb(px(14.))
                    .font_family(MONO)
                    .text_size(px(m.size))
                    .line_height(px(m.line))
                    .text_color(rgba(TEXT))
                    .child(screen),
            )
            .children(exit.map(|c| {
                div().px(px(24.)).pb(px(12.)).text_size(px(12.)).text_color(rgba(if c == 0 { TEXT_3 } else { FAILED })).child(format!("Process exited with code {c}"))
            }))
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
        self.marked.map(|len| 0..len)
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
    }

    fn replace_text_in_range(&mut self, _: Option<Range<usize>>, text: &str, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
        if let Some(id) = &self.focused {
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
        self.marked = (!text.is_empty()).then(|| text.encode_utf16().count());
    }

    fn bounds_for_range(&mut self, _: Range<usize>, _: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        None
    }

    fn character_index_for_point(&mut self, _: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{Info, command_line};

    #[test]
    fn login_shells_show_as_their_name() {
        let info = Info { cmd: "/bin/zsh".into(), args: vec!["-l".into()], ..Default::default() };
        assert_eq!(command_line(&info), "zsh");
        let info = Info { cmd: "/opt/homebrew/bin/fish".into(), args: ["-l", "-c", "$argv; exec fish -l", "claude"].map(String::from).to_vec(), ..Default::default() };
        assert_eq!(command_line(&info), "fish");
        let info = Info { cmd: "pnpm".into(), args: vec!["dev".into(), "--port".into(), "8081".into()], ..Default::default() };
        assert_eq!(command_line(&info), "pnpm dev --port 8081");
    }
}
