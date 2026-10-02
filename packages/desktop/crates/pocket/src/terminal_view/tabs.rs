use crate::desktop::Desktop;
use crate::desktop::chrome::{id, state};
use crate::status::Status;
use crate::util::basename;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{self, dot};
use workspace::{Doc, Tab};

pub fn tab_label(text: String, panes: usize) -> String {
    if panes > 1 { format!("{text} · {panes} panes") } else { text }
}

impl Desktop {
    fn tab_lead(&self, tab: &Tab) -> Div {
        let row = div().flex().items_center().gap(px(7.));
        let label = |text: String| div().max_w(px(150.)).truncate().child(text);
        let p = match tab {
            Tab::Term(rows) => rows.concat(),
            Tab::Doc(doc) => {
                let path = match doc {
                    Doc::File(p) | Doc::Diff(p) => p,
                };
                let totals = match doc {
                    Doc::Diff(p) => self.repo().and_then(|r| r.files.iter().find(|f| f.path == *p)).map(|f| ui::meta_diff(f.added, f.removed, 11.)),
                    Doc::File(_) => None,
                };
                return row.child(file_icon(path, false, false, 14.)).child(label(basename(path))).children(totals);
            }
        };
        let count = |text: String| tab_label(text, p.len());
        if let Some(a) = self.summary(&p[0]) {
            let mark = ui::indicator(id(format!("tab-mark:{}", a.id)), Status::of(a).map(|s| state(s, 0, 0)));
            return row.child(dot(7., provider_color(&a.provider))).child(label(count(provider_name(&a.provider).into()))).children(mark);
        }
        let s = self.terminals.sessions.get(&p[0]);
        let busy = s.and_then(|s| s.busy());
        let mark = match s {
            Some(s) if s.failed() => icon("x", 12., FAILED).into_any_element(),
            _ if busy.is_some() => dot(6., ACCENT).into_any_element(),
            _ => dot(6., TEXT_5).into_any_element(),
        };
        let text = busy.map_or_else(|| self.pane_label(&p[0]), str::to_string);
        row.child(icon("prompt", 13., TEXT_3)).child(label(count(text))).child(mark)
    }

    pub(crate) fn new_tab_controls(&self, cx: &mut Context<Self>) -> Div {
        let plus = div()
            .id("new-tab")
            .size(px(28.))
            .ml(px(2.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(7.))
            .cursor_pointer()
            .when(self.terminal.tab_menu, |d| d.bg(FILL_3))
            .hover(|s| s.bg(FILL_3))
            .child(icon("plus", 15., TEXT_2))
            // Runs before the open menu's click-outside handler, which would otherwise close it only for this click to reopen it.
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.terminal.tab_menu = !this.terminal.tab_menu;
                this.row_menu = None;
                cx.notify();
            }));
        let menu = self.terminal.tab_menu.then(|| ui::dropdown(29., ui::menu_in("tab-menu-in", self.tab_menu_view(cx))));
        div().relative().flex().flex_none().items_center().child(plus).children(menu)
    }

    pub(crate) fn term_tabs(&mut self, tree: &str, cx: &mut Context<Self>) -> Div {
        let w = self.workspace(tree);
        let active = w.active;
        let tabs = w.tabs.clone();
        let observe = self.agents.observe_only();
        let items: Vec<_> = tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let selected = i == active;
                let closable = match tab {
                    Tab::Term(rows) => rows.iter().flatten().all(|id| self.terminals.may_close(id, observe)),
                    Tab::Doc(_) => true,
                };
                let close = div()
                    .id(("close-tab", i))
                    .size(px(20.))
                    .mr(px(4.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.))
                    .cursor_pointer()
                    .opacity(0.)
                    .group_hover("term-tab", |s| s.opacity(1.))
                    .hover(|s| s.bg(FILL_3))
                    .child(icon("x", 11., TEXT_4))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.close_tab(i, cx);
                    }));
                let tab = div()
                    .id(("tab", i))
                    .h(px(28.))
                    .pl(px(10.))
                    .pr(px(6.))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .child(self.tab_lead(tab))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.select_tab(i, window, cx)));
                div()
                    .group("term-tab")
                    .h(px(28.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .rounded(px(7.))
                    .text_size(px(12.5))
                    .whitespace_nowrap()
                    .when(selected, |d| d.bg(SURFACE).shadow(ui::row_shadow()).font_weight(FontWeight::SEMIBOLD).text_color(TEXT))
                    .when(!selected, |d| d.font_weight(FontWeight::MEDIUM).text_color(TEXT_2).hover(|s| s.bg(FILL_2)))
                    .child(tab)
                    .when(closable, |d| d.child(close))
            })
            .collect();
        let shown = Some((tree.to_string(), active));
        if self.terminal.tab_revealed != shown {
            self.terminal.tab_scroll.scroll_to_item(active);
            self.terminal.tab_revealed = shown;
        }
        // As f32: Pixels orders -0 below 0, so a strip that can't scroll would show its right fade.
        let (offset, max) = (f32::from(self.terminal.tab_scroll.offset().x), f32::from(self.terminal.tab_scroll.max_offset().x));
        let fade = |left: bool| {
            let solid: Hsla = SURFACE_SUNKEN.into();
            let (solid, clear) = (solid, solid.opacity(0.));
            let (from, to) = if left { (solid, clear) } else { (clear, solid) };
            div().absolute().top_0().bottom_0().w(px(24.)).when(left, |d| d.left_0()).when(!left, |d| d.right_0()).bg(linear_gradient(90., linear_color_stop(from, 0.), linear_color_stop(to, 1.)))
        };
        let strip = div()
            .id("tab-strip")
            .track_scroll(&self.terminal.tab_scroll)
            .overflow_x_scroll()
            .flex()
            .min_w_0()
            .items_center()
            .gap(px(2.))
            // The padding keeps the selected tab's shadow inside the clip, else only its corners show; the margin undoes the shift.
            .p(px(3.))
            .m(px(-3.))
            .children(items);
        div()
            .flex()
            .flex_initial()
            .min_w_0()
            .h(px(40.))
            .items_center()
            .gap(px(2.))
            .child(div().relative().flex().min_w_0().child(strip).when(offset < 0., |d| d.child(fade(true))).when(offset > -max, |d| d.child(fade(false))))
            .child(self.new_tab_controls(cx))
    }
}

#[cfg(test)]
mod tests {
    use super::tab_label;

    #[test]
    fn a_split_tab_counts_its_panes() {
        assert_eq!(tab_label("claude".into(), 1), "claude");
        assert_eq!(tab_label("claude".into(), 3), "claude · 3 panes");
    }
}
