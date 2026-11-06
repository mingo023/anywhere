use agents::ListedModel;
use agents::providers;
use crate::desktop::Desktop;
use crate::modals::form::home;
use gpui_kit::*;

/// What one provider's CLI listed, once asked.
#[derive(Default)]
struct Listing {
    models: Option<Vec<ListedModel>>,
    loading: bool,
    /// Bumped by `forget_models`, so a list asked for before it is dropped.
    asked: u32,
}

/// Each provider's models, in `providers::ALL`'s order.
#[derive(Default)]
pub(crate) struct Models([Listing; providers::COUNT]);

/// A dropdown's (value, label) pairs: `provider`'s models, keeping a stored one the list lacks.
pub(crate) fn model_options(provider: &str, listed: &[ListedModel], current: &str) -> Vec<(String, String)> {
    let offered = listed_models(provider, listed);
    let mut out = Vec::new();
    if !current.is_empty() && !offered.iter().any(|m| m.value == current) {
        out.push((current.into(), current.into()));
    }
    out.extend(offered.into_iter().map(|m| (m.value, m.name)));
    out
}

/// `provider`'s models: what its CLI listed, else the ones it falls back on.
pub(crate) fn listed_models(provider: &str, listed: &[ListedModel]) -> Vec<ListedModel> {
    if !listed.is_empty() {
        return listed.to_vec();
    }
    let fallback = providers::known(provider).models.fallback;
    fallback.iter().map(|m| ListedModel { value: m.to_string(), name: model_label(m), resolved: m.to_string(), ..ListedModel::default() }).collect()
}

pub(crate) fn effort_options(efforts: &[String], none: &str) -> Vec<(String, String)> {
    std::iter::once((String::new(), none.to_string())).chain(efforts.iter().map(|e| (e.clone(), effort_label(e)))).collect()
}

/// "Haiku" for a fallback alias, the slug for anything else, "Agent default" for none.
pub(crate) fn model_label(model: &str) -> String {
    match model {
        "" => "Agent default".into(),
        m if providers::ALL.iter().any(|b| b.models.fallback.contains(&m)) => capitalised(m),
        m => m.into(),
    }
}

pub(crate) fn effort_label(effort: &str) -> String {
    match effort {
        "" => "Agent default".into(),
        "xhigh" => "Extra high".into(),
        e => capitalised(e),
    }
}

fn capitalised(word: &str) -> String {
    let mut c = word.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

impl Desktop {
    /// Each provider's models for the pickers, read once off the UI thread.
    pub(crate) fn load_models(&mut self, cx: &mut Context<Self>) {
        if self.capturing {
            return;
        }
        let enabled = self.store.agents.enabled();
        for (i, builtin) in providers::ALL.iter().enumerate() {
            let l = &mut self.settings.models.0[i];
            if l.models.is_some() || l.loading || !enabled.contains(&builtin.id) {
                continue;
            }
            l.loading = true;
            let (asked, catalog) = (l.asked, &builtin.models);
            let mut argv = vec![self.store.agents.command(builtin.id)];
            argv.extend(catalog.args.iter().map(|a| a.to_string()));
            cx.spawn(async move |this, cx| {
                let list = cx.background_executor().spawn(async move { daemon::run_login(&argv.iter().map(String::as_str).collect::<Vec<_>>(), &home(), catalog.input).ok() }).await;
                this.update(cx, |d, cx| {
                    let l = &mut d.settings.models.0[i];
                    if l.asked == asked {
                        // A failed read stays None, so the next page that needs the list asks again.
                        (l.models, l.loading) = (list.as_deref().map(catalog.parse), false);
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
        }
    }

    /// Asks `provider` again, as after its command changes.
    pub(crate) fn forget_models(&mut self, provider: &str) {
        if let Some(i) = providers::IDS.iter().position(|p| *p == provider) {
            let l = &mut self.settings.models.0[i];
            (l.models, l.loading, l.asked) = (None, false, l.asked.wrapping_add(1));
        }
    }

    /// What `provider`'s CLI listed, none before it answers.
    pub(crate) fn models_of(&self, provider: &str) -> &[ListedModel] {
        let i = providers::IDS.iter().position(|p| *p == provider);
        i.and_then(|i| self.settings.models.0[i].models.as_deref()).unwrap_or_default()
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
    use super::{ListedModel, effort_options, model_label, model_options};

    fn listed(value: &str, name: &str) -> ListedModel {
        ListedModel { value: value.into(), name: name.into(), resolved: value.into(), ..ListedModel::default() }
    }

    #[test]
    fn each_agent_offers_what_its_cli_listed() {
        let fable = [listed("fable", "Fable 5.1")];
        assert_eq!(model_options("claude", &fable, ""), [("fable".to_string(), "Fable 5.1".to_string())]);
        let codex = [listed("gpt-6-sol", "GPT-6-Sol")];
        assert_eq!(model_options("codex", &codex, ""), [("gpt-6-sol".to_string(), "GPT-6-Sol".to_string())]);
    }

    #[test]
    fn claude_offers_its_aliases_until_its_cli_lists_models() {
        let values: Vec<_> = model_options("claude", &[], "").into_iter().map(|(v, _)| v).collect();
        assert_eq!(values, ["haiku", "sonnet", "opus"]);
        assert!(model_options("codex", &[], "").is_empty());
    }

    #[test]
    fn a_stored_model_the_list_lacks_stays_choosable() {
        assert_eq!(model_options("codex", &[], "gpt-5"), [("gpt-5".to_string(), "gpt-5".to_string())]);
        assert_eq!((model_label(""), model_label("opus"), model_label("gpt-5")), ("Agent default".to_string(), "Opus".to_string(), "gpt-5".to_string()));
    }

    #[test]
    fn only_claude_code_offers_efforts() {
        assert_eq!(effort_options(&agents::providers::builtin("claude").efforts, "Agent default").iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>(), ["Agent default", "Low", "Medium", "High", "Extra high", "Max"]);
        assert_eq!(effort_options(&[], "Agent default").len(), 1);
    }
}
