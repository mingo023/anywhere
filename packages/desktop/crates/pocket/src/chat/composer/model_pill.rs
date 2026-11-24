use super::{ChatBoxEvent, ComposerState, hang};
use crate::composer::effort_menu::{Effort, choice_menu};
use crate::composer::effort_slider::{self, effort_slider};
use crate::composer::model_menu::{
    Choose, EffortData, Entry, FAVORITES, Listed, Models, RailTab, Star, View, empty_list, focus_search, model_effort_pill, model_list, model_rail,
    panel as model_panel_of,
};
use crate::composer::{self, Held, Menu, launch};
use crate::desktop::Desktop;
use crate::models::effort_label;
use crate::pick_menu::{Line, checked};
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

/// The pill's model name: the one `own`, its provider's models, gives the exact id the agent runs, as the list shows it,
/// else one made from the id. An alias isn't matched, so Claude's pill keeps the version its id names.
fn pill_name(agent: &Summary, own: &[ListedModel]) -> Option<String> {
    let model = agent.model.as_deref()?;
    own.iter().find(|m| m.value == model).map(|m| m.name.clone())
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

pub(crate) struct Pill {
    agent: String,
    provider: String,
    name: String,
    effort: Option<String>,
    picks: bool,
    panel: Option<Panel>,
}

struct Panel {
    viewed: String,
    tabs: Vec<RailTab>,
    models: Models,
    effort: Option<EffortData>,
    settings: Vec<Setting>,
    note: Option<String>,
}

struct Setting {
    id: String,
    name: String,
    choices: Vec<(String, String)>,
    on: String,
}

pub(crate) enum Model {
    Opened(String),
    Viewed(&'static str),
    Picked { agent: String, provider: String, entry: Entry },
    Starred(Entry),
    Effort { agent: String, effort: String },
    Option { agent: String, setting: String, value: String },
}

impl ComposerState {
    /// The agent's model and effort; once pocketd can switch them, the panel that picks them.
    pub(super) fn model_pill(&self, pill: &Pill, cx: &mut Context<Self>) -> Div {
        let chip = model_effort_pill(&pill.provider, pill.name.clone(), pill.effort.clone());
        if !pill.picks {
            return chip;
        }
        let (open, id) = (self.draft.menu == Some(Menu::Model), pill.agent.clone());
        let chip = ui::popover_trigger(
            composer::picker(chip, "chat-model", open),
            open,
            cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                if this.draft.toggle(Menu::Model) {
                    focus_search(&this.search, window, cx);
                    cx.emit(ChatBoxEvent::Model(Model::Opened(id.clone())));
                }
                cx.notify();
            }),
        );
        let menu = pill.panel.as_ref().map(|panel| hang("chat-model-menu", self.model_panel(pill, panel, cx), Self::close_menu, cx));
        div().relative().min_w_0().flex_shrink(1.).child(chip).children(menu)
    }

    fn model_panel(&self, pill: &Pill, panel: &Panel, cx: &mut Context<Self>) -> Div {
        let (rail_scroll, slider, search) = (self.draft.rail.clone(), self.draft.slider.clone(), self.search.clone());
        let view: View<Self> = Rc::new(|_, entry, cx| cx.emit(ChatBoxEvent::Model(Model::Viewed(entry))));
        let rail = model_rail(&panel.viewed, &panel.tabs, &rail_scroll, view, cx);
        let list = match &panel.models {
            Models::Nothing(note) => empty_list(note.clone()),
            Models::Listed(listed) => {
                let (agent, provider) = (pill.agent.clone(), pill.provider.clone());
                let choose: Choose<Self> = Rc::new(move |this, entry, cx| {
                    cx.emit(ChatBoxEvent::Model(Model::Picked { agent: agent.clone(), provider: provider.clone(), entry: entry.clone() }));
                    this.close_menu(cx);
                });
                let star: Star<Self> = Rc::new(|_, entry, cx| cx.emit(ChatBoxEvent::Model(Model::Starred(entry.clone()))));
                model_list("chat-model-row", listed.clone(), panel.viewed == FAVORITES, &self.draft.scroll, (choose, star), cx)
            }
        };
        let effort = panel.effort.as_ref().map(|e| {
            let agent = pill.agent.clone();
            let pick: launch::Pick<Self> = Rc::new(move |_, effort, cx| cx.emit(ChatBoxEvent::Model(Model::Effort { agent: agent.clone(), effort })));
            effort_slider("chat-effort", e.word, (&e.levels, e.on.as_ref()), &slider, pick, cx)
        });
        let hidden = effort.is_none().then(|| effort_slider::hand_back(&slider, search.read(cx).focus_handle(cx))).flatten();
        let menus = panel.settings.iter().enumerate().map(|(i, s)| {
            let (agent, setting) = (pill.agent.clone(), s.id.clone());
            let pick: launch::Pick<Self> =
                Rc::new(move |_, value, cx| cx.emit(ChatBoxEvent::Model(Model::Option { agent: agent.clone(), setting: setting.clone(), value })));
            choice_menu(("chat-setting", i), s.name.clone().into(), (&s.choices, Some(&s.on)), &self.draft.effort, |c, _, f| f(&mut c.draft.effort), pick, cx)
        });
        let menus: Vec<Effort> = menus.collect();
        model_panel_of(rail, &search, list, panel.note.clone(), effort, menus).children(hidden)
    }
}

impl Desktop {
    pub(super) fn chat_model(&self, agent: &Summary, view_only: bool, composer: &Entity<ComposerState>, cx: &App) -> Pill {
        let model = agent.model.as_deref().unwrap_or_default();
        let levels = self.efforts_on(&agent.provider, model);
        let default = self.listed_model(&agent.provider, model).map(|m| m.default_effort.as_str());
        let (name, effort) = label(agent, &levels, default);
        let name = pill_name(agent, self.models_of(&agent.provider)).unwrap_or(name);
        let picks = !view_only && self.agents.settings_offered();
        let open = picks && composer.read(cx).draft.menu == Some(Menu::Model);
        let panel = open.then(|| self.chat_panel(agent, &levels, default, composer, cx));
        Pill { agent: agent.id.clone(), provider: agent.provider.clone(), name, effort, picks, panel }
    }

    fn chat_panel(&self, agent: &Summary, levels: &[String], default: Option<&str>, composer: &Entity<ComposerState>, cx: &App) -> Panel {
        let c = composer.read(cx);
        let viewed = c.draft.viewing.unwrap_or(agent.provider.as_str());
        let project = self.chat_project(agent, cx);
        let query = c.search.read(cx).value();
        let listed = c.draft.listed.get((viewed, &*query, &agent.provider, &agent.model, self.lines_from(project.as_deref())), || {
            self.chat_listed(agent, viewed, &query, project.as_deref())
        });
        let models = if listed.lines.is_empty() { Models::Nothing(self.no_models_note(viewed, &query)) } else { Models::Listed(listed) };
        let own = viewed == agent.provider;
        let effort = (own && !levels.is_empty()).then(|| EffortData {
            word: agents::providers::known(&agent.provider).effort_word,
            levels: levels.to_vec(),
            on: running_effort(agent, levels, default).map(String::from),
        });
        let model = agent.model.as_deref().unwrap_or_default();
        let settings = if own { self.listed_model(&agent.provider, model).map_or(&[][..], |m| &m.settings) } else { &[] };
        let settings = settings
            .iter()
            .map(|s| Setting { id: s.id.clone(), name: s.name.clone(), choices: s.choices.clone(), on: running_option(agent, s).clone() })
            .collect();
        Panel { viewed: viewed.into(), tabs: self.rail_tabs(project.as_deref(), &agent.provider), models, effort, settings, note: self.models_note(viewed) }
    }

    /// The project `agent` works in, whose hidden agents its model panel leaves out.
    fn chat_project(&self, agent: &Summary, cx: &App) -> Option<String> {
        self.agent_project(agent, &agent.cwd, &self.projects(cx)).cloned()
    }

    /// The chat's model panel just opened on the agent with this `id`.
    pub(super) fn chat_models_opened(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some((provider, project)) = self.agents.get(id).map(|a| (a.provider.clone(), self.chat_project(a, cx))) else { return };
        self.models_opened(&Held::Chat, &provider, project.as_deref(), cx);
        let Some(agent) = self.agents.get(id) else { return };
        if let Some(on) = self.chat_listed(agent, &provider, "", project.as_deref()).on {
            self.chats.composer.read(cx).draft.scroll.scroll_to_item(on, ScrollStrategy::Center);
        }
    }

    /// The lines `viewed` lists that hold `query`, the model the agent runs among them and checked.
    fn chat_listed(&self, agent: &Summary, viewed: &str, query: &str, project: Option<&str>) -> Listed {
        let own = self.offered_models(&agent.provider);
        let lines = self.model_lines(viewed, if viewed == agent.provider { unlisted_running(agent, own) } else { "" }, query, project);
        Listed { on: running_line(&lines, agent, own), lines }
    }

    pub(super) fn on_model(&mut self, ev: &Model, cx: &mut Context<Self>) {
        match ev {
            Model::Opened(id) => self.chat_models_opened(id, cx),
            Model::Viewed(entry) => self.view_models(&Held::Chat, entry, cx),
            Model::Picked { agent, provider, entry } if *provider == entry.provider => {
                self.outbox.settings(agent, &entry.value, "");
                self.model_used(&entry.provider, &entry.value, cx);
            }
            Model::Picked { agent, entry, .. } => self.switch_chat_agent(agent, entry.provider.clone(), entry.value.clone(), cx),
            Model::Starred(entry) => self.star_model(&entry.provider, &entry.value, cx),
            Model::Effort { agent, effort } => self.outbox.settings(agent, "", effort),
            Model::Option { agent, setting, value } => self.outbox.option(agent, setting, value),
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{Entry, Line, ListedModel, ModelSetting, Summary, label, pill_name, running_effort, running_line, running_option, runs, unlisted_running};

    #[test]
    fn the_line_checked_is_the_model_the_chat_runs() {
        let listed = |value: &str, resolved: &str| ListedModel { value: value.into(), resolved: resolved.into(), ..ListedModel::default() };
        let line = |provider: &str, value: &str| {
            Line::Pick(Entry { provider: provider.into(), value: value.into(), label: String::new(), upstream: String::new(), missing: false, starred: false })
        };
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
        let line = |value: &str| {
            Line::Pick(Entry { provider: "pi".into(), value: value.into(), label: String::new(), upstream: String::new(), missing: false, starred: false })
        };
        assert_eq!(running_line(&[line("claude-sonnet-4-6"), line("")], &agent, &[listed("")]), Some(0));
    }

    #[test]
    fn the_model_pill_reads_model_and_effort_else_the_models_default() {
        let agent = |model: &str, effort: Option<&str>| Summary {
            provider: "claude".into(),
            model: Some(model.into()),
            effort: effort.map(Into::into),
            ..Default::default()
        };
        let levels = ["low".to_string(), "medium".to_string()];
        assert_eq!(label(&agent("claude-opus-4-5", Some("xhigh")), &[], None), ("Opus 4.5".into(), Some("Extra high".into())));
        assert_eq!(label(&agent("claude-opus-4-5", Some("medium")), &levels, Some("low")).1.as_deref(), Some("Medium"));
        assert_eq!(label(&agent("claude-opus-4-5", None), &levels, Some("low")).1.as_deref(), Some("Low"));
        assert_eq!(label(&agent("claude-opus-4-5", None), &levels, Some("")).1, None);
    }

    #[test]
    fn the_pill_names_the_model_as_its_list_does_but_keeps_an_aliased_models_version() {
        let listed =
            |value: &str, name: &str| ListedModel { value: value.into(), name: name.into(), resolved: "claude-opus-5-5".into(), ..ListedModel::default() };
        let agent = |model: Option<&str>| Summary { provider: "cursor".into(), model: model.map(Into::into), ..Default::default() };
        assert_eq!(pill_name(&agent(Some("gpt-5.4")), &[listed("gpt-5.4", "GPT-5.4")]).as_deref(), Some("GPT-5.4"));
        assert_eq!(pill_name(&agent(Some("claude-opus-5-5")), &[listed("opus", "Opus")]), None);
        assert_eq!(pill_name(&agent(None), &[listed("gpt-5.4", "GPT-5.4")]), None);
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
        let fast = ModelSetting {
            id: "fast".into(),
            name: "Fast".into(),
            choices: vec![("false".into(), "Off".into()), ("true".into(), "On".into())],
            default: "false".into(),
        };
        let agent = |options: &[(&str, &str)]| Summary {
            provider: "omp".into(),
            options: options.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            ..Default::default()
        };
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
        let listed =
            |value: &str, resolved: &str| ListedModel { value: value.into(), name: String::new(), resolved: resolved.into(), ..ListedModel::default() };
        assert!(runs(&agent("claude", "claude-opus-5-5"), &listed("opus", "claude-opus-5-5")));
        assert!(runs(&agent("claude", "opus"), &listed("opus", "claude-opus-5-5")));
        assert!(!runs(&agent("claude", "claude-opus-4-6"), &listed("opus", "claude-opus-5-5")));
        assert!(runs(&agent("claude", "claude-opus-5-5"), &listed("opus", "opus")), "a fallback alias, before the CLI resolves it");
        assert!(!runs(&agent("claude", "claude-opus-5-5"), &listed("claude-opus-5", "claude-opus-5")));
        assert!(runs(&agent("codex", "gpt-6-sol"), &listed("gpt-6-sol", "gpt-6-sol")));
        assert!(!runs(&agent("codex", "gpt-6-sol"), &listed("sol", "sol")));
    }
}
