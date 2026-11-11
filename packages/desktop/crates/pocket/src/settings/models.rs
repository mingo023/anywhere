use super::dropdown::{Choice, Row};
use crate::desktop::Desktop;
use crate::modals::form::home;
use agents::ListedModel;
use agents::providers;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::*;
use std::sync::LazyLock;
use std::time::Duration;

/// What one provider's CLI listed, once asked.
#[derive(Default)]
struct Listing {
    models: Option<Vec<ListedModel>>,
    loading: bool,
    /// Why the last read listed nothing. It isn't asked again until `Models::retry`: a page that needs the list asks on
    /// every pass, and each ask starts the CLI.
    failed: Option<String>,
    /// Bumped by `forget`, so a list asked for before it is dropped.
    asked: u32,
}

/// Each provider's models, in `providers::ALL`'s order.
pub(crate) struct Models {
    lists: [Listing; providers::COUNT],
    /// What each falls back on, named as a picker lists them.
    fallback: [Vec<ListedModel>; providers::COUNT],
    /// Counts the changes to any list, for what `Kept` builds from them.
    pub(crate) revision: u64,
}

impl Default for Models {
    fn default() -> Self {
        let fallback = |i: usize| providers::ALL[i].models.fallback.iter().map(|m| ListedModel { value: m.to_string(), name: model_label(m), resolved: m.to_string(), ..ListedModel::default() }).collect();
        Self { lists: Default::default(), fallback: std::array::from_fn(fallback), revision: 0 }
    }
}

/// The one row of an agent whose models aren't known: it starts on the model the agent picks itself.
static AGENT_DEFAULT: LazyLock<[ListedModel; 1]> = LazyLock::new(|| [ListedModel { name: model_label(""), ..ListedModel::default() }]);

fn index(provider: &str) -> Option<usize> {
    providers::IDS.iter().position(|p| *p == provider)
}

impl Models {
    /// What `provider`'s CLI listed, none before it answers.
    pub(crate) fn listed(&self, provider: &str) -> &[ListedModel] {
        index(provider).and_then(|i| self.lists[i].models.as_deref()).unwrap_or_default()
    }

    /// What a picker offers of `provider`: what its CLI listed, else the models it falls back on. With neither, an
    /// `installed` agent offers its own default once it's no longer being asked, so it can still be picked.
    pub(crate) fn offered(&self, provider: &str, installed: bool) -> &[ListedModel] {
        let Some(i) = index(provider) else { return &[] };
        match &self.lists[i].models {
            Some(listed) => listed,
            None if !self.fallback[i].is_empty() => &self.fallback[i],
            None if self.lists[i].loading || !installed => &[],
            None => &*AGENT_DEFAULT,
        }
    }

    pub(crate) fn loading(&self, provider: &str) -> bool {
        index(provider).is_some_and(|i| self.lists[i].loading)
    }

    /// Why `provider`'s last read listed nothing.
    pub(crate) fn failure(&self, provider: &str) -> Option<&str> {
        index(provider).and_then(|i| self.lists[i].failed.as_deref())
    }

    /// Marks `provider` as being asked and returns what `answer` takes back; `None` when there is nothing to ask: it
    /// has listed, is being asked, or failed since the last `retry`.
    pub(crate) fn ask(&mut self, provider: &str) -> Option<u32> {
        let l = &mut self.lists[index(provider)?];
        if l.models.is_some() || l.loading || l.failed.is_some() {
            return None;
        }
        (l.loading, self.revision) = (true, self.revision + 1);
        Some(l.asked)
    }

    /// Takes what `provider`'s CLI said, unless `forget` came after the `ask`. No models is a failure too: it lists
    /// nothing to keep, and may list some once the agent is signed in.
    pub(crate) fn answer(&mut self, provider: &str, asked: u32, read: Result<Vec<ListedModel>, String>) -> bool {
        let Some(l) = index(provider).map(|i| &mut self.lists[i]).filter(|l| l.asked == asked) else { return false };
        l.loading = false;
        match read {
            Ok(models) if models.is_empty() => l.failed = Some("it listed none".into()),
            Ok(models) => l.models = Some(models),
            Err(why) => l.failed = Some(why),
        }
        self.revision += 1;
        true
    }

    /// Lets each agent whose last read failed be asked again, as when a menu that lists models opens.
    pub(crate) fn retry(&mut self) {
        for l in &mut self.lists {
            l.failed = None;
        }
    }

    /// Asks `provider` again, as after its command changes.
    pub(crate) fn forget(&mut self, provider: &str) {
        if let Some(i) = index(provider) {
            self.lists[i] = Listing { asked: self.lists[i].asked.wrapping_add(1), ..Listing::default() };
            self.revision += 1;
        }
    }

    /// Stands in for `provider`'s CLI, which a capture never runs: its list, or with none that it's still being asked.
    pub(crate) fn set(&mut self, provider: &str, models: Option<Vec<ListedModel>>) {
        if let Some(i) = index(provider) {
            self.lists[i] = Listing { loading: models.is_none(), models, ..Listing::default() };
            self.revision += 1;
        }
    }
}

/// What a menu offers of an agent, each as what picks it, its name and who serves it: `offered`, after a stored
/// `current` the list lacks.
pub(crate) fn model_options<'a>(offered: &'a [ListedModel], current: &'a str) -> impl Iterator<Item = (&'a str, &'a str, &'a str)> {
    let stored = (!current.is_empty() && !offered.iter().any(|m| m.value == current)).then_some((current, current, ""));
    stored.into_iter().chain(offered.iter().map(|m| (m.value.as_str(), m.name.as_str(), m.upstream.as_str())))
}

/// `items` under who serves each, in the order the first of each comes, for a `provider` whose models come from other
/// makers and whose list says which; any other's in one group with no name. One that names nobody sits under the agent.
pub(crate) fn by_upstream<T>(provider: &str, items: Vec<T>, upstream: impl Fn(&T) -> &str) -> Vec<(Option<String>, Vec<T>)> {
    if !providers::get(provider).is_some_and(|b| b.by_upstream) || items.iter().all(|i| upstream(i).is_empty()) {
        return vec![(None, items)];
    }
    let mut groups: Vec<(String, Vec<T>)> = Vec::new();
    for item in items {
        let who = Some(upstream(&item)).filter(|w| !w.is_empty()).unwrap_or(provider).to_string();
        match groups.iter_mut().find(|(g, _)| *g == who) {
            Some((_, group)) => group.push(item),
            None => groups.push((who, vec![item])),
        }
    }
    groups.into_iter().map(|(who, group)| (Some(who.replace('-', " ").to_uppercase()), group)).collect()
}

/// `provider`'s models as a Settings menu lists them.
pub(crate) fn model_rows(provider: &str, offered: &[ListedModel], current: &str) -> Vec<Row> {
    let groups = by_upstream(provider, model_options(offered, current).collect(), |(_, _, upstream)| upstream);
    groups.into_iter().flat_map(|(who, models)| who.map(Row::Heading).into_iter().chain(models.into_iter().map(|(value, label, _)| Row::Pick(Choice { value: value.into(), label: label.into() })))).collect()
}

pub(crate) fn effort_options(efforts: &[String], none: &str) -> Vec<(String, String)> {
    std::iter::once((String::new(), none.to_string())).chain(efforts.iter().map(|e| (e.clone(), effort_label(e)))).collect()
}

/// "Haiku" or "GLM 5.2 Fast" for a model an agent falls back on, the slug for anything else, "Agent default" for none.
pub(crate) fn model_label(model: &str) -> String {
    match model {
        "" => "Agent default".into(),
        m if providers::ALL.iter().any(|b| b.models.fallback.contains(&m)) => agents::model_name(m),
        m => m.into(),
    }
}

pub(crate) fn effort_label(effort: &str) -> String {
    match effort {
        "" => "Agent default".into(),
        "xhigh" | "extra-high" => "Extra high".into(),
        e => capitalised(e),
    }
}

/// "12 models available", as Settings says of an installed agent.
pub(crate) fn count_line(models: usize) -> String {
    format!("{models} {} available", if models == 1 { "model" } else { "models" })
}

/// Why an agent pocketd didn't find can't be picked, and how to get it: the command that installs it, then what is
/// left to do in plain words.
pub(crate) fn not_found(provider: &str) -> String {
    let install = providers::get(provider).map(|b| &b.install);
    let command = install.map(|i| i.command).filter(|c| !c.is_empty()).map(|c| format!("`{c}`"));
    let then = install.map(|i| i.then).filter(|t| !t.is_empty()).map(String::from);
    let how: Vec<String> = command.into_iter().chain(then).collect();
    let how = if how.is_empty() { String::new() } else { format!(" ({})", how.join(", then ")) };
    format!("{} not found{how}. Install it, or restart Anywhere if it is already installed.", theme::provider_name(provider))
}

/// What a menu says of `agent` when its CLI listed no models, with the first line of `why`.
pub(crate) fn couldnt_list(agent: &str, why: &str) -> String {
    let said = format!("Couldn't list {agent}'s models");
    match why.lines().map(str::trim).find(|l| !l.is_empty()) {
        Some(why) => format!("{said}: {why}"),
        None => said,
    }
}

/// The agents a picker lists, in the table's order: the ones `offered` and, so what is picked stays in view, `current`.
pub(crate) fn listed_agents(offered: &[&'static str], current: &[&str]) -> Vec<&'static str> {
    providers::IDS.into_iter().filter(|p| offered.contains(p) || current.contains(p)).collect()
}

/// How long a server gets to list its models before it's stopped.
const ANSWER_WITHIN: Duration = Duration::from_secs(45);

/// What `argv` printed, run where a session would be with `input` on stdin; a server is stopped at its `answer`.
fn list_models(argv: &[String], input: &str, answer: Option<fn(&str) -> bool>) -> Result<String, String> {
    let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
    match answer {
        Some(answer) => daemon::run_login_until(&argv, &home(), input, answer, ANSWER_WITHIN),
        None => daemon::run_login(&argv, &home(), input),
    }
}

fn capitalised(word: &str) -> String {
    let mut c = word.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

impl Desktop {
    /// The agents the pickers offer: left on, and installed where pocketd runs.
    pub(crate) fn offered(&self) -> Vec<&'static str> {
        self.store.agents.offered(&self.agents)
    }

    /// The agents `project`'s model panel offers: the ones on offer but for those it hides.
    pub(crate) fn offered_in(&self, project: Option<&str>) -> Vec<&'static str> {
        self.store.agents.shown(&self.agents, self.store.hidden(project))
    }

    /// Whether pocketd found `provider`'s CLI.
    pub(crate) fn installed(&self, provider: &str) -> bool {
        self.agents.provider_info(provider).available
    }

    /// `row` for picking `provider`; one pocketd didn't find is dimmed and says how to install it.
    pub(crate) fn agent_row(&self, provider: &str, row: Stateful<Div>) -> Stateful<Div> {
        if self.installed(provider) {
            return row;
        }
        let why = not_found(provider);
        row.opacity(0.4).tooltip(move |window, cx| Tooltip::new(why.clone()).build(window, cx))
    }

    /// Every offered agent's models for the pickers, each read once off the UI thread.
    pub(crate) fn load_models(&mut self, cx: &mut Context<Self>) {
        for provider in self.offered() {
            self.load_models_of(provider, cx);
        }
    }

    /// `provider`'s models, read and parsed once off the UI thread; an agent that isn't offered has no CLI to ask.
    pub(crate) fn load_models_of(&mut self, provider: &str, cx: &mut Context<Self>) {
        let Some(provider) = self.offered().into_iter().find(|p| *p == provider).filter(|_| !self.capturing) else { return };
        let Some(asked) = self.settings.models.ask(provider) else { return };
        let catalog = &providers::known(provider).models;
        let (argv, input) = (self.store.agents.catalog_argv(provider), catalog.input_in(&home()));
        cx.spawn(async move |this, cx| {
            let read = cx.background_executor().spawn(async move { list_models(&argv, &input, catalog.answer).and_then(|printed| catalog.read(&printed)) }).await;
            this.update(cx, |d, cx| {
                if d.settings.models.answer(provider, asked, read) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// What `provider`'s CLI listed, none before it answers.
    pub(crate) fn models_of(&self, provider: &str) -> &[ListedModel] {
        self.settings.models.listed(provider)
    }

    /// What a picker offers of `provider`: `Models::offered`.
    pub(crate) fn offered_models(&self, provider: &str) -> &[ListedModel] {
        self.settings.models.offered(provider, self.installed(provider))
    }

    /// The listed model `model` names, by the value it's picked with or the id the agent reports.
    pub(crate) fn listed_model(&self, provider: &str, model: &str) -> Option<&ListedModel> {
        self.models_of(provider).iter().find(|m| m.value == model || m.resolved == model)
    }

    /// The efforts `provider` offers on `model`: the model's own once listed, else what pocketd says the provider takes.
    pub(crate) fn efforts_on(&self, provider: &str, model: &str) -> Vec<String> {
        self.listed_model(provider, model).map_or_else(|| self.agents.provider_info(provider).efforts.clone(), |m| m.efforts.clone())
    }

    /// The efforts a launch of `provider` on `model` offers; with no model picked, it starts on Settings'.
    pub(crate) fn launch_efforts(&self, provider: &str, model: &str) -> Vec<String> {
        let settings = self.store.agents.provider(provider).model;
        self.efforts_on(provider, if model.is_empty() { &settings } else { model })
    }
}

#[cfg(test)]
mod tests {
    use super::{Choice, ListedModel, Models, Row, by_upstream, couldnt_list, count_line, effort_label, effort_options, listed_agents, model_label, model_options, model_rows, not_found};

    #[test]
    fn a_picker_lists_each_agent_on_offer_and_the_one_picked() {
        assert_eq!(listed_agents(&["claude", "codex"], &["claude"]), ["claude", "codex"]);
        assert_eq!(listed_agents(&["claude", "hermes"], &["pi"]), ["claude", "pi", "hermes"], "a pick whose agent is no longer installed stays listed");
        assert_eq!(listed_agents(&["claude"], &["shell"]), ["claude"]);
    }

    fn listed(value: &str, name: &str) -> ListedModel {
        ListedModel { value: value.into(), name: name.into(), resolved: value.into(), ..ListedModel::default() }
    }

    fn served(upstream: &str, value: &str, name: &str) -> ListedModel {
        ListedModel { upstream: upstream.into(), ..listed(value, name) }
    }

    fn options<'a>(offered: &'a [ListedModel], current: &'a str) -> Vec<(&'a str, &'a str)> {
        model_options(offered, current).map(|(value, name, _)| (value, name)).collect()
    }

    #[test]
    fn each_agent_offers_what_its_cli_listed() {
        let mut models = Models::default();
        for (provider, value, name) in [("claude", "fable", "Fable 5.1"), ("codex", "gpt-6-sol", "GPT-6-Sol")] {
            let asked = models.ask(provider).unwrap();
            assert!(models.answer(provider, asked, Ok(vec![listed(value, name)])));
            assert_eq!(options(models.offered(provider, true), ""), [(value, name)]);
            assert_eq!(models.listed(provider).len(), 1);
        }
    }

    #[test]
    fn claude_offers_its_aliases_until_its_cli_lists_models() {
        let models = Models::default();
        assert_eq!(options(models.offered("claude", true), ""), [("haiku", "Haiku"), ("sonnet", "Sonnet"), ("opus", "Opus")]);
        assert!(models.listed("claude").is_empty(), "the aliases aren't what its CLI listed");
    }

    #[test]
    fn an_agent_with_no_models_to_fall_back_on_offers_its_own_default_once_it_is_not_being_asked() {
        let mut models = Models::default();
        let asked = models.ask("hermes").unwrap();
        assert!(models.loading("hermes") && models.offered("hermes", true).is_empty());
        models.answer("hermes", asked, Err("Authentication required".into()));
        assert_eq!(options(models.offered("hermes", true), ""), [("", "Agent default")]);
        assert_eq!((models.loading("hermes"), models.failure("hermes")), (false, Some("Authentication required")));
        assert!(models.offered("hermes", false).is_empty(), "one that isn't installed has no default to start on");
        assert_eq!(options(models.offered("codex", true), ""), [("", "Agent default")], "before it's asked, as in a capture");
    }

    #[test]
    fn a_failed_read_is_not_asked_again_until_a_menu_reopens() {
        let mut models = Models::default();
        let asked = models.ask("pi").unwrap();
        assert_eq!(models.ask("pi"), None, "while it's being asked");
        models.answer("pi", asked, Ok(Vec::new()));
        assert_eq!(models.failure("pi"), Some("it listed none"));
        assert_eq!(models.ask("pi"), None);
        models.retry();
        let again = models.ask("pi").unwrap();
        models.answer("pi", again, Ok(vec![listed("zai/glm-5", "GLM 5")]));
        models.retry();
        assert_eq!(models.ask("pi"), None, "once it has listed");
    }

    #[test]
    fn a_list_asked_for_before_the_command_changed_is_dropped() {
        let mut models = Models::default();
        let asked = models.ask("codex").unwrap();
        models.forget("codex");
        assert!(!models.answer("codex", asked, Ok(vec![listed("gpt-5", "GPT 5")])));
        assert!(models.listed("codex").is_empty() && models.ask("codex").is_some());
    }

    #[test]
    fn every_change_to_a_list_moves_the_revision() {
        let mut models = Models::default();
        let mut seen = vec![models.revision];
        let asked = models.ask("codex").unwrap();
        seen.push(models.revision);
        models.answer("codex", asked, Ok(vec![listed("gpt-5", "GPT 5")]));
        seen.push(models.revision);
        models.forget("codex");
        seen.push(models.revision);
        models.set("codex", None);
        seen.push(models.revision);
        assert!(seen.windows(2).all(|w| w[0] < w[1]), "{seen:?}");
    }

    #[test]
    fn a_stored_model_the_list_lacks_stays_choosable() {
        assert_eq!(options(&[], "gpt-5"), [("gpt-5", "gpt-5")]);
        assert_eq!(options(&[listed("gpt-5", "GPT 5")], "gpt-5"), [("gpt-5", "GPT 5")]);
        assert_eq!((model_label(""), model_label("opus"), model_label("gpt-5")), ("Agent default".to_string(), "Opus".to_string(), "gpt-5".to_string()));
    }

    #[test]
    fn a_model_an_agent_falls_back_on_reads_as_its_name() {
        assert_eq!(["haiku", "gpt-5.4", "zai/glm-5.2-fast", "claude-sonnet-4-6"].map(model_label), ["Haiku", "GPT 5.4", "GLM 5.2 Fast", "Sonnet 4.6"]);
        assert_eq!(options(Models::default().offered("fx", true), ""), [("zai/glm-5.2-fast", "GLM 5.2 Fast")]);
    }

    #[test]
    fn the_agents_that_run_other_makers_models_list_them_under_who_serves_each() {
        let models = [served("anthropic", "anthropic/claude-opus-4-5", "Claude Opus 4.5"), served("amazon-bedrock", "amazon-bedrock/claude-opus-4-5", "Claude Opus 4.5"), served("anthropic", "anthropic/claude-haiku-4-5", "Claude Haiku 4.5")];
        let shown = |provider: &str, current: &str| {
            let rows = model_rows(provider, &models, current);
            rows.into_iter().map(|r| match r { Row::Heading(who) => format!("# {who}"), Row::Pick(Choice { label, .. }) => label }).collect::<Vec<_>>()
        };
        for provider in ["opencode", "pi", "omp", "hermes"] {
            assert_eq!(shown(provider, ""), ["# ANTHROPIC", "Claude Opus 4.5", "Claude Haiku 4.5", "# AMAZON BEDROCK", "Claude Opus 4.5"], "{provider}");
        }
        assert_eq!(shown("cursor", ""), ["Claude Opus 4.5", "Claude Opus 4.5", "Claude Haiku 4.5"]);
        assert_eq!(shown("pi", "local")[..2], ["# PI", "local"], "a stored model that names nobody sits under the agent");
        let own = by_upstream("hermes", vec!["", ""], |upstream| upstream);
        assert_eq!(own, [(None, vec!["", ""])], "a list that names nobody has no groups");
    }

    #[test]
    fn an_effort_reads_the_same_under_either_spelling() {
        assert_eq!(["xhigh", "extra-high", "off", ""].map(effort_label), ["Extra high", "Extra high", "Off", "Agent default"]);
    }

    #[test]
    fn an_agent_that_is_not_installed_says_how_to_install_it() {
        assert_eq!(not_found("pi"), "Pi not found (`npm i -g @earendil-works/pi-coding-agent`). Install it, or restart Anywhere if it is already installed.");
        assert_eq!(not_found("codex"), "Codex not found. Install it, or restart Anywhere if it is already installed.");
    }

    #[test]
    fn what_is_left_to_do_after_the_install_command_reads_as_plain_words() {
        assert_eq!(
            not_found("antigravity"),
            "Antigravity not found (`curl -fsSL https://antigravity.google/cli/install.sh | bash`, then run agy once to sign in). Install it, or restart Anywhere if it is already installed."
        );
        assert_eq!(
            not_found("hermes"),
            "Hermes Agent not found (`curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash`, then run hermes model). Install it, or restart Anywhere if it is already installed."
        );
    }

    #[test]
    fn a_read_that_listed_nothing_says_so_with_its_first_line() {
        assert_eq!(couldnt_list("Hermes Agent", "\n Authentication required \nrun hermes model"), "Couldn't list Hermes Agent's models: Authentication required");
        assert_eq!(couldnt_list("omp", ""), "Couldn't list omp's models");
    }

    #[test]
    fn an_installed_agent_counts_its_models() {
        assert_eq!((count_line(1), count_line(0), count_line(181)), ("1 model available".to_string(), "0 models available".to_string(), "181 models available".to_string()));
    }

    #[test]
    fn only_claude_code_offers_efforts() {
        assert_eq!(effort_options(&agents::providers::builtin("claude").efforts, "Agent default").iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>(), ["Agent default", "Low", "Medium", "High", "Extra high", "Max"]);
        assert_eq!(effort_options(&[], "Agent default").len(), 1);
    }
}
