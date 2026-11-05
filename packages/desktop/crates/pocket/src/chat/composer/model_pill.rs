use super::{chip_menu, close_menu};
use crate::composer::{self, Menu, launch};
use crate::composer::model_menu::{check, row};
use crate::desktop::Desktop;
use crate::settings::models::{effort_label, listed_models};
use agents::Summary;
use gpui_kit::*;
use std::rc::Rc;
use theme::*;

/// The model's short name and its effort, labelled as the new session's pills label them.
pub(super) fn label(agent: &Summary) -> (String, Option<String>) {
    (agents::model_label(agent), agent.effort.as_deref().filter(|e| !e.is_empty()).map(effort_label))
}

/// Whether the agent runs `value`; Claude Code reports an alias's full id, as "claude-opus-5-5" for "opus".
fn runs(agent: &Summary, value: &str) -> bool {
    agent.model.as_deref().is_some_and(|m| m == value || agents::providers::get(&agent.provider).is_some_and(|b| b.aliases().is_some()) && m.split('-').any(|w| w == value))
}

/// The agent's model, as read where it can't be switched.
fn model_pill(agent: &Summary) -> Div {
    composer::pill()
        .max_w(px(248.))
        .min_w_0()
        .flex_shrink(1.)
        .child(provider_icon(&agent.provider, 16., TEXT_2))
        .child(div().truncate().font_weight(FontWeight::MEDIUM).text_color(TEXT).child(label(agent).0))
}

/// Opens `menu`, or closes it when open.
fn toggle(this: &mut Desktop, menu: Menu, cx: &mut Context<Desktop>) {
    this.chats.composer.draft.toggle(menu);
    cx.notify();
}

impl Desktop {
    /// The model pill, a picker of the agent's models once pocketd can switch them.
    pub(super) fn chat_model_pill(&self, agent: &Summary, view_only: bool, cx: &mut Context<Self>) -> Div {
        if view_only || !self.agents.settings_offered() {
            return model_pill(agent);
        }
        let open = self.chats.composer.draft.menu == Some(Menu::Model);
        let chip = composer::picker(model_pill(agent), "chat-model", open).capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
            cx.stop_propagation();
            this.load_models(cx);
            toggle(this, Menu::Model, cx);
        }));
        div().relative().min_w_0().flex_shrink(1.).child(chip).children(open.then(|| self.chat_model_menu(agent, cx)))
    }

    /// The models the agent's provider lists, the one it runs checked; picking one switches it.
    fn chat_model_menu(&self, agent: &Summary, cx: &mut Context<Self>) -> Div {
        let rows: Vec<_> = listed_models(&agent.provider, self.catalog_models())
            .into_iter()
            .enumerate()
            .map(|(i, (value, name))| {
                let (on, id) = (runs(agent, &value), agent.id.clone());
                row(("chat-model-row", i), on).child(div().flex_1().min_w_0().truncate().font_weight(FontWeight::MEDIUM).child(name)).child(check(on)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.outbox.settings(&id, &value, "");
                    close_menu(this, cx);
                }))
            })
            .collect();
        chip_menu("chat-model-menu", "Model", 250., rows, cx)
    }

    /// The agent's effort: a picker of its model's levels once pocketd can switch them, else what it reports.
    pub(super) fn chat_effort_pill(&self, agent: &Summary, view_only: bool, cx: &mut Context<Self>) -> Option<Div> {
        let effort = label(agent).1;
        let levels = self.efforts_on(&agent.provider, agent.model.as_deref().unwrap_or_default());
        if view_only || !self.agents.settings_offered() || levels.is_empty() {
            return effort.map(|e| composer::pill().child(icon("bulb", 14., TEXT_2)).child(div().whitespace_nowrap().font_weight(FontWeight::MEDIUM).text_color(TEXT).child(e)));
        }
        let open = self.chats.composer.draft.menu == Some(Menu::Effort);
        let chip = launch::chip("chat-effort", open, "bulb", effort.unwrap_or_else(|| "Default".into()), TEXT, |this, cx| toggle(this, Menu::Effort, cx), cx);
        let menu = open.then(|| {
            let id = agent.id.clone();
            let pick: launch::Pick = Rc::new(move |this, effort, cx| {
                this.outbox.settings(&id, "", &effort);
                close_menu(this, cx);
            });
            let rows = launch::effort_rows("chat-effort-row", levels, agent.effort.as_deref().unwrap_or_default(), pick, cx);
            chip_menu("chat-effort-menu", "Effort", 260., rows, cx)
        });
        Some(div().relative().child(chip).children(menu))
    }
}

#[cfg(test)]
mod tests {
    use super::{Summary, label, runs};

    #[test]
    fn the_model_pill_reads_model_and_effort() {
        let agent = |model: &str, effort: Option<&str>| Summary { provider: "claude".into(), model: Some(model.into()), effort: effort.map(Into::into), ..Default::default() };
        assert_eq!(label(&agent("claude-opus-4-5", Some("xhigh"))), ("Opus 4.5".into(), Some("Extra high".into())));
        assert_eq!(label(&agent("claude-opus-4-5", Some("medium"))).1.as_deref(), Some("Medium"));
        assert_eq!(label(&agent("claude-opus-4-5", None)).1, None);
    }

    #[test]
    fn a_claude_alias_is_the_model_its_full_id_names() {
        let agent = |provider: &str, model: &str| Summary { provider: provider.into(), model: Some(model.into()), ..Default::default() };
        assert!(runs(&agent("claude", "claude-opus-5-5"), "opus"));
        assert!(runs(&agent("claude", "opus"), "opus"));
        assert!(!runs(&agent("claude", "claude-opus-5-5"), "sonnet"));
        assert!(runs(&agent("codex", "gpt-6-sol"), "gpt-6-sol"));
        assert!(!runs(&agent("codex", "gpt-6-sol"), "sol"));
    }
}
