use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;
use ui::{self, dot};

impl Desktop {
    /// The last model seen for `provider`, as a short label.
    pub fn model_hint(&self, provider: &str) -> Option<String> {
        let latest = self.agents.list.iter().filter(|a| a.provider == provider && a.model.is_some()).max_by_key(|a| a.updated_at)?;
        Some(agents::model_label(latest))
    }

    pub(crate) fn tab_menu_view(&self, cx: &mut Context<Self>) -> Stateful<Div> {
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
