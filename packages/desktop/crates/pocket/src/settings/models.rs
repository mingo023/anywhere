use crate::desktop::Desktop;
use crate::modals::form::home;
use gpui_kit::*;
use store::prefs::agents::{CLAUDE_MODELS, efforts};

/// Codex's models, from its CLI once asked; Claude Code lists none, so it has its aliases.
#[derive(Default)]
pub(crate) struct Models {
    codex: Option<Vec<(String, String)>>,
    loading: bool,
    /// Bumped by `forget_models`, so a list asked for before it is dropped.
    asked: u32,
}

/// A dropdown's (value, label) pairs: `none` first as "", then `provider`'s models, keeping a stored one the list lacks.
pub(crate) fn model_options(provider: &str, codex: &[(String, String)], current: &str, none: &str) -> Vec<(String, String)> {
    let listed: Vec<(String, String)> = if provider == "codex" { codex.to_vec() } else { CLAUDE_MODELS.iter().map(|m| (m.to_string(), model_label(m))).collect() };
    let mut out = vec![(String::new(), none.to_string())];
    if !current.is_empty() && !listed.iter().any(|(v, _)| v == current) {
        out.push((current.into(), current.into()));
    }
    out.extend(listed);
    out
}

pub(crate) fn effort_options(provider: &str, none: &str) -> Vec<(String, String)> {
    std::iter::once((String::new(), none.to_string())).chain(efforts(provider).iter().map(|e| (e.to_string(), effort_label(e)))).collect()
}

/// "Haiku" for Claude Code's alias, the slug for anything else, "Agent default" for none.
pub(crate) fn model_label(model: &str) -> String {
    match model {
        "" => "Agent default".into(),
        m if CLAUDE_MODELS.contains(&m) => capitalised(m),
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
    /// Codex's models for the pickers, read once off the UI thread.
    pub(crate) fn load_models(&mut self, cx: &mut Context<Self>) {
        let m = &mut self.settings.models;
        if m.codex.is_some() || m.loading || self.capturing {
            return;
        }
        m.loading = true;
        let asked = m.asked;
        let command = self.store.agents.command("codex");
        cx.spawn(async move |this, cx| {
            let list = cx.background_executor().spawn(async move { daemon::run_login(&[&command, "debug", "models"], &home(), "").ok().map(|out| agents::codex_models(&out)) }).await;
            this.update(cx, |d, cx| {
                let m = &mut d.settings.models;
                if m.asked == asked {
                    // A failed read stays None, so the next page that needs the list asks again.
                    (m.codex, m.loading) = (list, false);
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
        (m.codex, m.loading, m.asked) = (None, false, m.asked.wrapping_add(1));
    }

    pub(crate) fn codex_models(&self) -> &[(String, String)] {
        self.settings.models.codex.as_deref().unwrap_or_default()
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
        assert_eq!(effort_options("claude", "Agent default").iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>(), ["Agent default", "Low", "Medium", "High", "Extra high", "Max"]);
        assert_eq!(effort_options("codex", "Agent default").len(), 1);
    }
}
