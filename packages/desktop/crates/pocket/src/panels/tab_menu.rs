use super::frame::{AgentRow, NewTab};
use super::{Panels, PanelsEvent};
use crate::desktop::Desktop;
use agents::Summary;
use gpui_kit::*;
use theme::*;
use workspace::Place;

/// The last model seen for `provider`, as a short label.
pub fn model_hint(list: &[Summary], provider: &str) -> Option<String> {
    let latest = list.iter().filter(|a| a.provider == provider && a.model.is_some()).max_by_key(|a| a.updated_at)?;
    Some(agents::model_label(latest))
}

impl Desktop {
    pub fn model_hint(&self, provider: &str) -> Option<String> {
        model_hint(&self.agents.list, provider)
    }
}

impl Panels {
    pub(super) fn tab_menu_view(&self, place: Place, rows: Option<&NewTab>, cx: &mut Context<Self>) -> Stateful<Div> {
        let (branch, agents) = rows.map_or((String::new(), &[][..]), |r| (r.branch.clone(), r.agents.as_slice()));
        let item = |id: ElementId, lead: AnyElement, label: String, hint: Option<String>, keys: Option<&str>| {
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
                .hover(|s| s.bg(FILL_2))
                .child(div().w(px(16.)).flex().flex_none().justify_center().child(lead))
                .child(div().flex_1().flex().whitespace_nowrap().child(label).children(hint.map(|h| div().ml(px(7.)).text_color(TEXT_3).child(h))))
                .children(keys.map(|k| div().text_size(px(11.5)).text_color(TEXT_4).child(k.to_string())))
        };
        let agent = |row: &AgentRow, cx: &mut Context<Self>| {
            let provider = row.provider;
            item(format!("tab-menu-{provider}").into(), provider_icon(provider, 14., TEXT).into_any_element(), row.name.clone(), row.hint.clone(), None)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.menu = None;
                    cx.emit(PanelsEvent::NewAgent(provider, place));
                }))
        };
        ui::pop(div().id("tab-menu"))
            .w(px(264.))
            .p(px(6.))
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
                    .text_color(TEXT_3)
                    .child("New tab in ")
                    .child(div().font_family(MONO).font_weight(FontWeight::MEDIUM).child(branch)),
            )
            .child(
                item(
                    "tab-menu-shell".into(),
                    icon("prompt", 14., TEXT_2).into_any_element(),
                    "New shell".into(),
                    None,
                    crate::settings::hint(&crate::actions::NewTab, cx).as_deref(),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.menu = None;
                    cx.emit(PanelsEvent::NewShell(place));
                })),
            )
            .child(
                item(
                    "tab-menu-browser".into(),
                    icon("globe", 14., TEXT_2).into_any_element(),
                    "New browser".into(),
                    None,
                    crate::settings::hint(&crate::actions::NewBrowser, cx).as_deref(),
                )
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(PanelsEvent::OpenBrowser(place)))),
            )
            .child(item("tab-menu-chat".into(), icon("comment", 14., TEXT_2).into_any_element(), "New chat".into(), None, None).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| {
                    this.menu = None;
                    cx.emit(PanelsEvent::NewChat(place));
                },
            )))
            .child(div().h(px(0.5)).my(px(4.)).mx(px(6.)).bg(SEPARATOR))
            .children(agents.iter().map(|row| agent(row, cx)))
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.menu = None;
                cx.notify();
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::model_hint;
    use agents::Summary;

    fn agent(provider: &str, model: Option<&str>, updated_at: i64) -> Summary {
        Summary { provider: provider.into(), model: model.map(String::from), updated_at, ..Default::default() }
    }

    #[test]
    fn the_model_hint_is_the_last_model_the_provider_used() {
        let list = [agent("claude", Some("claude-opus-4-1"), 2), agent("claude", Some("claude-sonnet-4-5"), 1), agent("codex", Some("gpt-5"), 3)];
        assert_eq!(model_hint(&list, "claude").as_deref(), Some("Opus 4.1"));
    }

    #[test]
    fn agents_that_have_not_reported_a_model_give_no_hint() {
        let list = [agent("claude", Some("claude-opus-4-1"), 1), agent("claude", None, 2), agent("codex", None, 3)];
        assert_eq!(model_hint(&list, "claude").as_deref(), Some("Opus 4.1"));
        assert_eq!(model_hint(&list, "codex"), None);
    }
}
