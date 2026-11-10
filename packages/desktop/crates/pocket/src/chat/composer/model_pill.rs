use super::{close_menu, hang};
use crate::composer::{self, Menu, launch};
use crate::composer::effort_menu::{Effort, effort_menu};
use crate::composer::model_menu::{FAVORITES, focus_search, model_effort_pill, model_row, panel};
use crate::desktop::Desktop;
use crate::settings::models::{effort_label, listed_models};
use agents::{ListedModel, Summary};
use gpui_kit::*;
use std::rc::Rc;

/// The model's short name and its effort, else `default`, the model's own, labelled as the new session's pill labels them.
/// Once the model's `levels` are known, an effort outside them isn't one it runs on.
pub(super) fn label(agent: &Summary, levels: &[String], default: Option<&str>) -> (String, Option<String>) {
    let effort = agent.effort.as_deref().into_iter().chain(default).find(|e| !e.is_empty() && (levels.is_empty() || levels.iter().any(|l| l == e)));
    (agents::model_label(agent), effort.map(effort_label))
}

/// Whether the agent runs `model`; Claude Code reports an alias's full id, as "claude-opus-5-5" for "opus". Until the CLI
/// says what a fallback alias stands for, the alias in the id is the best guess.
fn runs(agent: &Summary, model: &ListedModel) -> bool {
    let unresolved = model.resolved == model.value && agents::providers::known(&agent.provider).models.fallback.contains(&model.value.as_str());
    agent.model.as_deref().is_some_and(|m| m == model.value || m == model.resolved || unresolved && m.split('-').any(|w| w == model.value))
}

impl Desktop {
    /// The agent's model and effort; once pocketd can switch them, the panel that picks them.
    pub(super) fn chat_model_pill(&self, agent: &Summary, view_only: bool, cx: &mut Context<Self>) -> Div {
        let model = agent.model.as_deref().unwrap_or_default();
        let levels = self.efforts_on(&agent.provider, model);
        let default = self.listed_model(&agent.provider, model).map(|m| m.default_effort.as_str());
        let (name, effort) = label(agent, &levels, default);
        let pill = model_effort_pill(&agent.provider, name, effort);
        if view_only || !self.agents.settings_offered() {
            return pill;
        }
        let open = self.chats.composer.draft.menu == Some(Menu::Model);
        let chip = composer::picker(pill, "chat-model", open).capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            this.load_models(cx);
            if this.chats.composer.draft.toggle(Menu::Model) {
                let search = this.chats.composer.search.clone();
                focus_search(&search, window, cx);
            }
            cx.notify();
        }));
        let menu = open.then(|| {
            let viewed = self.chats.composer.draft.viewing.unwrap_or(agent.provider.as_str());
            let rail = self.model_rail(viewed, |d: &mut Desktop| &mut d.chats.composer.draft, cx);
            let effort = (viewed == agent.provider && !levels.is_empty()).then(|| self.chat_effort_menu(agent, &levels, default, cx));
            hang("chat-model-menu", panel(rail, viewed, &self.chats.composer.search, self.chat_models(agent, viewed, cx), effort), cx)
        });
        div().relative().min_w_0().flex_shrink(1.).child(chip).children(menu)
    }

    /// What `viewed` lists that holds the search, the model the agent runs checked. Picking one of its provider's switches
    /// it; one from another provider has to start a new chat.
    fn chat_models(&self, agent: &Summary, viewed: &str, cx: &mut Context<Self>) -> Vec<Stateful<Div>> {
        let query = self.chats.composer.search.read(cx).value();
        let entries = self.model_entries(viewed, |d, p| listed_models(p, d.models_of(p)).into_iter().map(|m| (m.value, m.name)).collect(), &query);
        let own = listed_models(&agent.provider, self.models_of(&agent.provider));
        entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let on = entry.provider == agent.provider && own.iter().any(|m| m.value == entry.value && runs(agent, m));
                let (id, same, provider, value) = (agent.id.clone(), entry.provider == agent.provider, entry.provider.clone(), entry.value.clone());
                model_row("chat-model-row", i, entry, on, viewed == FAVORITES, self.store.agents.is_favorite(&entry.provider, &entry.value), cx).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if same {
                        this.outbox.settings(&id, &value, "");
                        close_menu(this, cx);
                    } else {
                        this.switch_chat_agent(&id, provider.clone(), value.clone(), cx);
                    }
                }))
            })
            .collect()
    }

    /// The model's efforts, naming the one it runs on, else `default`; picking one switches it.
    fn chat_effort_menu(&self, agent: &Summary, levels: &[String], default: Option<&str>, cx: &mut Context<Self>) -> Effort {
        let id = agent.id.clone();
        let pick: launch::Pick = Rc::new(move |this, effort, _| this.outbox.settings(&id, "", &effort));
        let on = agent.effort.as_deref().into_iter().chain(default).find(|e| levels.iter().any(|l| l == e)).map(String::from);
        effort_menu("chat-effort", agents::providers::known(&agent.provider).effort_word, (levels, on.as_ref()), &self.chats.composer.draft.effort, |d: &mut Desktop| &mut d.chats.composer.draft, pick, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::{ListedModel, Summary, label, runs};

    #[test]
    fn the_model_pill_reads_model_and_effort_else_the_models_default() {
        let agent = |model: &str, effort: Option<&str>| Summary { provider: "claude".into(), model: Some(model.into()), effort: effort.map(Into::into), ..Default::default() };
        let levels = ["low".to_string(), "medium".to_string()];
        assert_eq!(label(&agent("claude-opus-4-5", Some("xhigh")), &[], None), ("Opus 4.5".into(), Some("Extra high".into())));
        assert_eq!(label(&agent("claude-opus-4-5", Some("medium")), &levels, Some("low")).1.as_deref(), Some("Medium"));
        assert_eq!(label(&agent("claude-opus-4-5", None), &levels, Some("low")).1.as_deref(), Some("Low"));
        assert_eq!(label(&agent("claude-opus-4-5", None), &levels, Some("")).1, None);
    }

    #[test]
    fn an_effort_the_model_lacks_falls_back_to_the_models_own() {
        let agent = Summary { provider: "claude".into(), model: Some("claude-opus-4-6".into()), effort: Some("xhigh".into()), ..Default::default() };
        let levels = ["low".to_string(), "high".to_string()];
        assert_eq!(label(&agent, &levels, Some("high")).1.as_deref(), Some("High"));
        assert_eq!(label(&agent, &levels, None).1, None);
    }

    #[test]
    fn a_listed_model_is_the_one_its_value_or_resolved_id_names() {
        let agent = |provider: &str, model: &str| Summary { provider: provider.into(), model: Some(model.into()), ..Default::default() };
        let listed = |value: &str, resolved: &str| ListedModel { value: value.into(), name: String::new(), resolved: resolved.into(), ..ListedModel::default() };
        assert!(runs(&agent("claude", "claude-opus-5-5"), &listed("opus", "claude-opus-5-5")));
        assert!(runs(&agent("claude", "opus"), &listed("opus", "claude-opus-5-5")));
        assert!(!runs(&agent("claude", "claude-opus-4-6"), &listed("opus", "claude-opus-5-5")));
        assert!(runs(&agent("claude", "claude-opus-5-5"), &listed("opus", "opus")), "a fallback alias, before the CLI resolves it");
        assert!(!runs(&agent("claude", "claude-opus-5-5"), &listed("claude-opus-5", "claude-opus-5")));
        assert!(runs(&agent("codex", "gpt-6-sol"), &listed("gpt-6-sol", "gpt-6-sol")));
        assert!(!runs(&agent("codex", "gpt-6-sol"), &listed("sol", "sol")));
    }
}
