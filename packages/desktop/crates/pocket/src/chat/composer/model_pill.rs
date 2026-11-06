use super::{close_menu, hang};
use crate::composer::{self, Menu, launch};
use crate::composer::model_menu::{OpenBeside, beside_list, check, effort_rows, model_effort_pill, model_value, pill_menu, row, rows_offset};
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

/// Opens `list` beside the model pill's menu.
fn open_beside(this: &mut Desktop, list: Menu, cx: &mut Context<Desktop>) {
    if this.chats.composer.draft.menu != Some(list) {
        this.chats.composer.draft.menu = Some(list);
        cx.notify();
    }
}

impl Desktop {
    /// The agent's model and effort; once pocketd can switch them, a menu of the model's efforts and the agent's models.
    pub(super) fn chat_model_pill(&self, agent: &Summary, view_only: bool, cx: &mut Context<Self>) -> Div {
        let model = agent.model.as_deref().unwrap_or_default();
        let levels = self.efforts_on(&agent.provider, model);
        let default = self.listed_model(&agent.provider, model).map(|m| m.default_effort.as_str());
        let (name, effort) = label(agent, &levels, default);
        let pill = model_effort_pill(&agent.provider, name.clone(), effort.clone());
        if view_only || !self.agents.settings_offered() {
            return pill;
        }
        let menu = self.chats.composer.draft.menu;
        let open = menu.is_some_and(Menu::of_model_pill);
        let chip = composer::picker(pill, "chat-model", open).capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
            cx.stop_propagation();
            this.load_models(cx);
            this.chats.composer.draft.toggle(Menu::ModelAndEffort);
            cx.notify();
        }));
        let menu = open.then(|| {
            let word = agents::providers::known(&agent.provider).effort_word;
            let effort_row = (!levels.is_empty()).then(|| (word, effort.unwrap_or_else(|| "Default".into())));
            let open_list: OpenBeside = Rc::new(|this, list, _, cx| open_beside(this, list, cx));
            let rows = pill_menu(["chat-effort", "chat-model-row"], effort_row, model_value(&agent.provider, name), menu, open_list, cx);
            // The menu opens upward, so a list lines up with its row's bottom: the effort row has the Model row under it.
            let beside = match menu {
                Some(Menu::Model) => Some(div().child(self.chat_model_menu(agent, cx))),
                Some(Menu::Effort) if !levels.is_empty() => Some(div().mb(rows_offset(1)).child(self.chat_effort_menu(agent, &levels, default, cx))),
                _ => None,
            };
            hang("chat-model-menu", div().flex().items_end().gap(px(4.)).child(rows).children(beside), cx)
        });
        div().relative().min_w_0().flex_shrink(1.).child(chip).children(menu)
    }

    /// The models the agent's provider lists, the one it runs checked; picking one switches it.
    fn chat_model_menu(&self, agent: &Summary, cx: &mut Context<Self>) -> Stateful<Div> {
        let rows: Vec<_> = listed_models(&agent.provider, self.models_of(&agent.provider))
            .into_iter()
            .enumerate()
            .map(|(i, model)| {
                let (on, id, value) = (runs(agent, &model), agent.id.clone(), model.value);
                row(("chat-model-row", i), on).child(div().flex_1().min_w_0().truncate().font_weight(FontWeight::MEDIUM).child(model.name)).child(check(on)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.outbox.settings(&id, &value, "");
                    close_menu(this, cx);
                }))
            })
            .collect();
        beside_list("chat-models", 250., rows)
    }

    /// The model's efforts, the one it runs on, else `default`, checked; picking one switches it.
    fn chat_effort_menu(&self, agent: &Summary, levels: &[String], default: Option<&str>, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = agent.id.clone();
        let pick: launch::Pick = Rc::new(move |this, effort, cx| {
            this.outbox.settings(&id, "", &effort);
            close_menu(this, cx);
        });
        let on = agent.effort.as_deref().into_iter().chain(default).find(|e| levels.iter().any(|l| l == e)).map(String::from);
        beside_list("chat-efforts", 200., effort_rows("chat-effort-row", levels, on.as_ref(), pick, cx))
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
