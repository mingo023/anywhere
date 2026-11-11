use super::{close_menu, hang};
use crate::composer::{self, Menu, launch};
use crate::composer::effort_menu::{Effort, choice_menu, effort_menu};
use crate::composer::model_menu::{Choose, Entry, FAVORITES, Listed, focus_search, model_effort_pill, model_list, panel};
use crate::pick_menu::{Line, checked};
use crate::desktop::Desktop;
use crate::settings::models::effort_label;
use agents::{ListedModel, ModelSetting, Summary};
use gpui_kit::*;
use std::rc::Rc;

/// The effort `agent` runs on, else `default`, the model's own. Once the model's `levels` are known, an effort outside
/// them isn't one it runs on.
fn running_effort<'a>(agent: &'a Summary, levels: &[String], default: Option<&'a str>) -> Option<&'a str> {
    agent.effort.as_deref().into_iter().chain(default).find(|e| !e.is_empty() && (levels.is_empty() || levels.iter().any(|l| l == e)))
}

/// The value `agent` runs `setting` on: the one it was started on or switched to, else the model's default.
fn running_option<'a>(agent: &'a Summary, setting: &'a ModelSetting) -> &'a String {
    agent.options.get(&setting.id).unwrap_or(&setting.default)
}

/// The model's short name and its `running_effort`, labelled as the new session's pill labels them.
pub(super) fn label(agent: &Summary, levels: &[String], default: Option<&str>) -> (String, Option<String>) {
    (crate::status::model_or_agent(agent), running_effort(agent, levels, default).map(effort_label))
}

/// Whether the agent runs `model`; Claude Code reports an alias's full id, as "claude-opus-5-5" for "opus". Until the CLI
/// says what a fallback alias stands for, the alias in the id is the best guess.
fn runs(agent: &Summary, model: &ListedModel) -> bool {
    let unresolved = model.resolved == model.value && agents::providers::known(&agent.provider).models.fallback.contains(&model.value.as_str());
    agent.model.as_deref().is_some_and(|m| m == model.value || m == model.resolved || unresolved && m.split('-').any(|w| w == model.value))
}

/// The model `agent` runs when none of `own`, its provider's models, is it: the panel lists it, as a launch's does a
/// stored model its list lacks.
fn unlisted_running<'a>(agent: &'a Summary, own: &[ListedModel]) -> &'a str {
    agent.model.as_deref().filter(|_| !own.iter().any(|m| runs(agent, m))).unwrap_or_default()
}

/// The line of the model `agent` runs, among `own`, its provider's models, or listed as it is.
fn running_line(lines: &[Line<Entry>], agent: &Summary, own: &[ListedModel]) -> Option<usize> {
    let running: Vec<&str> = own.iter().filter(|m| runs(agent, m)).map(|m| m.value.as_str()).chain(agent.model.as_deref()).collect();
    checked(lines, |e| e.provider == agent.provider && running.contains(&e.value.as_str()))
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
        let (open, id) = (self.chats.composer.draft.menu == Some(Menu::Model), agent.id.clone());
        let chip = composer::picker(pill, "chat-model", open).capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            if this.chats.composer.draft.toggle(Menu::Model) {
                let search = this.chats.composer.search.clone();
                focus_search(&search, window, cx);
                this.chat_models_opened(&id, cx);
            }
            cx.notify();
        }));
        let menu = open.then(|| {
            let viewed = self.chats.composer.draft.viewing.unwrap_or(agent.provider.as_str());
            let project = self.chat_project(agent);
            let rail = self.model_rail(viewed, &agent.provider, project.as_deref(), &self.chats.composer.draft.rail, |d: &mut Desktop| &mut d.chats.composer.draft, cx);
            let own = viewed == agent.provider;
            let effort = (own && !levels.is_empty()).then(|| self.chat_effort_menu(agent, &levels, default, cx));
            let settings = if own { self.chat_setting_menus(agent, cx) } else { Vec::new() };
            let rows = effort.into_iter().chain(settings).collect();
            hang("chat-model-menu", panel(rail, &self.chats.composer.search, self.chat_models(agent, viewed, project.as_deref(), cx), self.models_note(viewed), rows), cx)
        });
        div().relative().min_w_0().flex_shrink(1.).child(chip).children(menu)
    }

    /// The project `agent` works in, whose hidden agents its model panel leaves out.
    fn chat_project(&self, agent: &Summary) -> Option<String> {
        self.project_of(&agent.cwd, &self.projects()).cloned()
    }

    /// The chat's model panel just opened on the agent with this `id`.
    pub(super) fn chat_models_opened(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some((provider, project)) = self.agents.get(id).map(|a| (a.provider.clone(), self.chat_project(a))) else { return };
        self.models_opened(|d| &mut d.chats.composer.draft, &provider, project.as_deref(), cx);
    }

    /// What `viewed` lists that holds the search, the model the agent runs checked. Picking one of its provider's switches
    /// it; one from another provider has to start a new chat.
    fn chat_models(&self, agent: &Summary, viewed: &str, project: Option<&str>, cx: &mut Context<Self>) -> Div {
        let draft = &self.chats.composer.draft;
        let query = self.chats.composer.search.read(cx).value();
        let listed = draft.listed.get((viewed, &*query, &agent.provider, &agent.model, self.lines_from(project)), || {
            let own = self.offered_models(&agent.provider);
            let lines = self.model_lines(viewed, if viewed == agent.provider { unlisted_running(agent, own) } else { "" }, &query, project);
            Listed { on: running_line(&lines, agent, own), lines }
        });
        if listed.lines.is_empty() {
            return self.no_models(viewed, &query);
        }
        let (id, provider) = (agent.id.clone(), agent.provider.clone());
        let choose: Choose = Rc::new(move |this, entry, cx| {
            if entry.provider == provider {
                this.outbox.settings(&id, &entry.value, "");
                this.model_used(&entry.provider, &entry.value, cx);
                close_menu(this, cx);
            } else {
                this.switch_chat_agent(&id, entry.provider.clone(), entry.value.clone(), cx);
            }
        });
        model_list("chat-model-row", listed, viewed == FAVORITES, &draft.scroll, choose, cx)
    }

    /// The model's efforts, naming the one it runs on, else `default`; picking one switches it.
    fn chat_effort_menu(&self, agent: &Summary, levels: &[String], default: Option<&str>, cx: &mut Context<Self>) -> Effort {
        let id = agent.id.clone();
        let pick: launch::Pick = Rc::new(move |this, effort, _| this.outbox.settings(&id, "", &effort));
        let on = running_effort(agent, levels, default).map(String::from);
        effort_menu("chat-effort", agents::providers::known(&agent.provider).effort_word, (levels, on.as_ref()), &self.chats.composer.draft.effort, |d: &mut Desktop| &mut d.chats.composer.draft, pick, cx)
    }

    /// The running model's own settings, each naming the value it's on, else the model's default; picking one switches it.
    fn chat_setting_menus(&self, agent: &Summary, cx: &mut Context<Self>) -> Vec<Effort> {
        let model = agent.model.as_deref().unwrap_or_default();
        let settings = self.listed_model(&agent.provider, model).map_or(&[][..], |m| &m.settings);
        let menus = settings.iter().enumerate().map(|(i, s)| {
            let (id, setting) = (agent.id.clone(), s.id.clone());
            let pick: launch::Pick = Rc::new(move |this, value, _| this.outbox.option(&id, &setting, &value));
            choice_menu(("chat-setting", i), s.name.clone().into(), (&s.choices, Some(running_option(agent, s))), &self.chats.composer.draft.effort, |d: &mut Desktop| &mut d.chats.composer.draft, pick, cx)
        });
        menus.collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{Entry, Line, ListedModel, ModelSetting, Summary, label, running_effort, running_line, running_option, runs, unlisted_running};

    #[test]
    fn the_line_checked_is_the_model_the_chat_runs() {
        let listed = |value: &str, resolved: &str| ListedModel { value: value.into(), resolved: resolved.into(), ..ListedModel::default() };
        let line = |provider: &str, value: &str| Line::Pick(Entry { provider: provider.into(), value: value.into(), label: String::new(), upstream: String::new(), missing: false });
        let agent = Summary { provider: "claude".into(), model: Some("claude-opus-5-5".into()), ..Default::default() };
        let own = [listed("sonnet", "claude-sonnet-5"), listed("opus", "claude-opus-5-5")];
        let lines = [Line::Heading("ANTHROPIC".into()), line("codex", "opus"), line("claude", "sonnet"), line("claude", "opus")];
        assert_eq!(running_line(&lines, &agent, &own), Some(3), "its own agent's row, by the id the alias stands for");
        assert_eq!(running_line(&lines[..3], &agent, &own), None);
        assert_eq!(running_line(&lines, &Summary { model: None, ..agent }, &own), None);
    }

    #[test]
    fn a_running_model_its_list_lacks_is_listed_and_checked() {
        let agent = Summary { provider: "pi".into(), model: Some("claude-sonnet-4-6".into()), ..Default::default() };
        let listed = |value: &str| ListedModel { value: value.into(), resolved: value.into(), ..ListedModel::default() };
        assert_eq!(unlisted_running(&agent, &[listed("")]), "claude-sonnet-4-6");
        assert_eq!(unlisted_running(&agent, &[listed("claude-sonnet-4-6")]), "", "a listed one needs no extra line");
        assert_eq!(unlisted_running(&Summary { model: None, ..agent.clone() }, &[]), "");
        let line = |value: &str| Line::Pick(Entry { provider: "pi".into(), value: value.into(), label: String::new(), upstream: String::new(), missing: false });
        assert_eq!(running_line(&[line("claude-sonnet-4-6"), line("")], &agent, &[listed("")]), Some(0));
    }

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
    fn the_effort_running_is_the_agents_else_the_models_own_and_any_while_the_levels_are_unknown() {
        let agent = |effort: Option<&str>| Summary { provider: "claude".into(), effort: effort.map(Into::into), ..Default::default() };
        let levels = ["low".to_string(), "high".to_string()];
        assert_eq!(running_effort(&agent(Some("high")), &levels, Some("low")), Some("high"));
        assert_eq!(running_effort(&agent(Some("xhigh")), &levels, Some("low")), Some("low"));
        assert_eq!(running_effort(&agent(Some("xhigh")), &[], None), Some("xhigh"));
        assert_eq!(running_effort(&agent(Some("")), &[], Some("")), None);
    }

    #[test]
    fn a_setting_runs_on_what_the_agent_was_started_on_or_switched_to_else_the_models_default() {
        let fast = ModelSetting { id: "fast".into(), name: "Fast".into(), choices: vec![("false".into(), "Off".into()), ("true".into(), "On".into())], default: "false".into() };
        let agent = |options: &[(&str, &str)]| Summary { provider: "omp".into(), options: options.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), ..Default::default() };
        assert_eq!(running_option(&agent(&[("fast", "true")]), &fast), "true");
        assert_eq!(running_option(&agent(&[("mode", "review")]), &fast), "false");
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
