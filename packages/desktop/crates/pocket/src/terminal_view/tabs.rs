use crate::desktop::Desktop;
use crate::status::Status;
use crate::util::basename;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{self, dot};
use workspace::{Doc, Tab};

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
        let count = |text: String| if p.len() > 1 { format!("{text} · {} panes", p.len()) } else { text };
        if let Some(a) = self.summary(&p[0]) {
            let mark = match Status::of(a) {
                Some(Status::NeedsYou) => Some(dot(6., WAITING).into_any_element()),
                Some(Status::Failed) => Some(icon("x", 12., FAILED).into_any_element()),
                Some(Status::Done) => Some(dot(6., ACCENT).into_any_element()),
                Some(Status::Working) => Some(dot(6., RUNNING).into_any_element()),
                _ => None,
            };
            return row.child(dot(7., provider_color(&a.provider))).child(label(count(provider_name(&a.provider).into()))).children(mark);
        }
        let s = self.terminals.sessions.get(&p[0]);
        let busy = s.and_then(|s| s.busy());
        let mark = match s {
            Some(s) if s.failed() => icon("x", 12., FAILED).into_any_element(),
            _ if busy.is_some() => dot(6., RUNNING).into_any_element(),
            _ => dot(6., TEXT_5).into_any_element(),
        };
        let text = busy.map_or_else(|| self.pane_label(&p[0]), str::to_string);
        row.child(icon("prompt", 13., TEXT_3)).child(label(count(text))).child(mark)
    }

    pub(crate) fn term_tabs(&mut self, tree: &str, cx: &mut Context<Self>) -> Div {
        let w = self.workspace(tree);
        let active = w.active;
        let tabs = w.tabs.clone();
        let items: Vec<_> = tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let selected = i == active;
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
                    .hover(|s| s.bg(rgba(FILL_3)))
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
                    .when(selected, |d| d.bg(rgba(WHITE)).shadow(ui::row_shadow()).font_weight(FontWeight::SEMIBOLD).text_color(rgba(TEXT)))
                    .when(!selected, |d| d.font_weight(FontWeight::MEDIUM).text_color(rgba(TEXT_2)).hover(|s| s.bg(rgba(FILL_2))))
                    .child(tab)
                    .child(close)
            })
            .collect();
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
            .hover(|s| s.bg(rgba(FILL_3)))
            .child(icon("plus", 15., TEXT_2))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_shell(None, cx)));
        let chevron = div()
            .id("tab-menu-toggle")
            .w(px(20.))
            .h(px(28.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .cursor_pointer()
            .when(self.terminal.tab_menu, |d| d.bg(rgba(FILL_3)))
            .hover(|s| s.bg(rgba(FILL_3)))
            .child(icon("chevron-down", 12., TEXT_3))
            // Runs before the open menu's click-outside handler, which would otherwise close it only for this click to reopen it.
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.terminal.tab_menu = !this.terminal.tab_menu;
                this.row_menu = None;
                cx.notify();
            }));
        let menu = self.terminal.tab_menu.then(|| ui::dropdown(29., self.tab_menu_view(cx)));
        let shown = Some((tree.to_string(), active));
        if self.terminal.tab_revealed != shown {
            self.terminal.tab_scroll.scroll_to_item(active);
            self.terminal.tab_revealed = shown;
        }
        let (offset, max) = (self.terminal.tab_scroll.offset().x, self.terminal.tab_scroll.max_offset().x);
        let fade = |left: bool| {
            let (solid, clear) = (rgba(SURFACE_SUNKEN), rgba(SURFACE_SUNKEN & 0xffffff00));
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
            .child(div().relative().flex().min_w_0().child(strip).when(offset < px(0.), |d| d.child(fade(true))).when(offset > -max, |d| d.child(fade(false))))
            .child(div().relative().flex().flex_none().items_center().gap(px(2.)).child(plus).child(chevron).children(menu))
    }

    /// The last model seen for `provider`, as a short label.
    pub fn model_hint(&self, provider: &str) -> Option<String> {
        let latest = self.agents.list.iter().filter(|a| a.provider == provider && a.model.is_some()).max_by_key(|a| a.updated_at)?;
        Some(agents::model_label(latest))
    }

    fn tab_menu_view(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let branch = self.repo().map(|r| r.branch.clone()).unwrap_or_default();
        let item = |id: &'static str, lead: AnyElement, label: String, hint: Option<String>, keys: Option<&str>| {
            div()
                .id(id)
                .h(px(32.))
                .px(px(8.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(9.))
                .rounded(px(6.))
                .cursor_pointer()
                .text_size(px(13.))
                .hover(|s| s.bg(rgba(FILL_2)))
                .child(div().w(px(16.)).flex().flex_none().justify_center().child(lead))
                .child(div().flex_1().flex().whitespace_nowrap().child(label).children(hint.map(|h| div().ml(px(7.)).text_color(rgba(TEXT_3)).child(h))))
                .children(keys.map(|k| div().text_size(px(11.5)).text_color(rgba(TEXT_4)).child(k.to_string())))
        };
        let agent = |id: &'static str, provider: &'static str, cx: &mut Context<Self>| {
            item(id, dot(8., provider_color(provider)).into_any_element(), provider_name(provider).into(), self.model_hint(provider), None)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.new_agent_tab(provider, cx)))
        };
        ui::pop(div().id("tab-menu"))
            .w(px(264.))
            .p(px(6.))
            .rounded(px(10.))
            .flex()
            .flex_col()
            .gap(px(1.))
            .child(
                div()
                    .pt(px(4.))
                    .px(px(8.))
                    .pb(px(6.))
                    .flex()
                    .text_size(px(11.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgba(TEXT_3))
                    .child("New tab in ")
                    .child(div().font_family(MONO).font_weight(FontWeight::MEDIUM).child(branch)),
            )
            .child(
                item("tab-menu-shell", icon("prompt", 14., TEXT_2).into_any_element(), "New shell".into(), None, Some("⌘T"))
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.new_tab(&crate::actions::NewTab, window, cx))),
            )
            .child(div().h(px(0.5)).my(px(4.)).mx(px(6.)).bg(rgba(SEPARATOR)))
            .child(agent("tab-menu-claude", "claude", cx))
            .child(agent("tab-menu-codex", "codex", cx))
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.terminal.tab_menu = false;
                cx.notify();
            }))
    }
}
