use agents::providers;
use crate::desktop::Desktop;
use crate::modals::form::home;
use gpui_kit::*;

/// The catalog provider's models, from its CLI once asked; an alias provider lists none.
#[derive(Default)]
pub(crate) struct Models {
    catalog: Option<Vec<(String, String)>>,
    /// Each Codex model's efforts, as (slug, efforts).
    efforts: Vec<(String, Vec<String>)>,
    loading: bool,
    /// Bumped by `forget_models`, so a list asked for before it is dropped.
    asked: u32,
}

/// A dropdown's (value, label) pairs: `none` first as "", then `provider`'s models, keeping a stored one the list lacks.
pub(crate) fn model_options(provider: &str, catalog: &[(String, String)], current: &str, none: &str) -> Vec<(String, String)> {
    let listed = listed_models(provider, catalog);
    let mut out = vec![(String::new(), none.to_string())];
    if !current.is_empty() && !listed.iter().any(|(v, _)| v == current) {
        out.push((current.into(), current.into()));
    }
    out.extend(listed);
    out
}

/// `provider`'s models as (value, label): Codex's from its CLI, Claude Code's aliases.
pub(crate) fn listed_models(provider: &str, catalog: &[(String, String)]) -> Vec<(String, String)> {
    match providers::known(provider).models {
        providers::Models::Catalog => catalog.to_vec(),
        providers::Models::Aliases(aliases) => aliases.iter().map(|m| (m.to_string(), model_label(m))).collect(),
    }
}

pub(crate) fn effort_options(efforts: &[String], none: &str) -> Vec<(String, String)> {
    std::iter::once((String::new(), none.to_string())).chain(efforts.iter().map(|e| (e.clone(), effort_label(e)))).collect()
}

/// "Haiku" for Claude Code's alias, the slug for anything else, "Agent default" for none.
pub(crate) fn model_label(model: &str) -> String {
    match model {
        "" => "Agent default".into(),
        m if providers::ALL.iter().any(|b| b.aliases().is_some_and(|a| a.contains(&m))) => capitalised(m),
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
    /// The catalog provider's models for the pickers, read once off the UI thread.
    pub(crate) fn load_models(&mut self, cx: &mut Context<Self>) {
        let Some(catalog) = providers::ALL.iter().find(|b| b.models == providers::Models::Catalog) else { return };
        let command = self.store.agents.command(catalog.id);
        let m = &mut self.settings.models;
        if m.catalog.is_some() || m.loading || self.capturing {
            return;
        }
        m.loading = true;
        let asked = m.asked;
        cx.spawn(async move |this, cx| {
            let list = cx.background_executor().spawn(async move { daemon::run_login(&[&command, "debug", "models"], &home(), "").ok() }).await;
            this.update(cx, |d, cx| {
                let m = &mut d.settings.models;
                if m.asked == asked {
                    m.efforts = list.as_deref().map(agents::codex_efforts).unwrap_or_default();
                    // A failed read stays None, so the next page that needs the list asks again.
                    (m.catalog, m.loading) = (list.as_deref().map(agents::codex_models), false);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Asks Codex again, as after its command changes.
    pub(crate) fn forget_models(&mut self) {
        let m = &mut self.settings.models;
        (m.catalog, m.loading, m.asked) = (None, false, m.asked.wrapping_add(1));
    }

    pub(crate) fn catalog_models(&self) -> &[(String, String)] {
        self.settings.models.catalog.as_deref().unwrap_or_default()
    }

    /// The efforts `provider` offers on `model`: Claude Code's on any, Codex's per model.
    pub(crate) fn efforts_on(&self, provider: &str, model: &str) -> Vec<String> {
        if providers::known(provider).models != providers::Models::Catalog {
            return self.agents.provider_info(provider).efforts.clone();
        }
        self.settings.models.efforts.iter().find(|(slug, _)| slug == model).map(|(_, e)| e.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::{effort_options, model_label, model_options};

    #[test]
    fn claude_offers_its_aliases_and_codex_what_its_cli_listed() {
        let codex = vec![("gpt-6-sol".to_string(), "GPT-6-Sol".to_string())];
        let values = |o: Vec<(String, String)>| o.into_iter().map(|(v, _)| v).collect::<Vec<_>>();
        assert_eq!(values(model_options("claude", &codex, "", "Agent default")), ["", "haiku", "sonnet", "opus"]);
        assert_eq!(model_options("codex", &codex, "", "App default"), [(String::new(), "App default".to_string()), ("gpt-6-sol".to_string(), "GPT-6-Sol".to_string())]);
    }

    #[test]
    fn a_stored_model_the_list_lacks_stays_choosable() {
        assert_eq!(model_options("codex", &[], "gpt-5", "Agent default")[1], ("gpt-5".to_string(), "gpt-5".to_string()));
        assert_eq!((model_label(""), model_label("opus"), model_label("gpt-5")), ("Agent default".to_string(), "Opus".to_string(), "gpt-5".to_string()));
    }

    #[test]
    fn only_claude_code_offers_efforts() {
        assert_eq!(effort_options(&agents::providers::builtin("claude").efforts, "Agent default").iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>(), ["Agent default", "Low", "Medium", "High", "Extra high", "Max"]);
        assert_eq!(effort_options(&[], "Agent default").len(), 1);
    }
}
