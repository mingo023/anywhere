use super::close_menu;
use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay};
use crate::terminals::link;
use agents::Summary;
use gpui_kit::*;
use serde_json::{Value, json};
use store::LaunchPick;
use store::prefs::agents::Agents;
use workspace::Place;

/// A create for a chat on the (provider, model) in `s`'s folder, opening with `prompt`. It takes `s`'s access rather than
/// Settings', so moving to another agent never grants more.
fn switched_spec(s: &Summary, project: &str, (provider, model): (&str, &str), prompt: &str, prefs: &Agents, plan: bool, efforts: &[String]) -> Value {
    let access = s.access.clone().unwrap_or_else(|| "settings".into());
    let pick = LaunchPick { provider: provider.into(), model: model.into(), effort: String::new(), access, account: String::new() };
    let mut spec = pick.spec(project, json!({"worktree": s.cwd}), prompt);
    spec["chat"] = true.into();
    prefs.launch(&mut spec, plan, efforts);
    spec
}

/// What a model from another provider does to a chat.
#[derive(Debug, PartialEq)]
enum Switch {
    Ask,
    Open,
    Replace,
}

/// Asks with a conversation to carry. A chat that has run keeps its place even with none showing: its history may not
/// have loaded, and replacing it would end it unasked.
fn switching(conversation: bool, busy: bool, tokens_used: u64) -> Switch {
    match (conversation, busy || tokens_used > 0) {
        (true, _) => Switch::Ask,
        (false, true) => Switch::Open,
        (false, false) => Switch::Replace,
    }
}

impl Desktop {
    fn chat_conversation(&self, agent: &Summary) -> String {
        self.chats.views.get(&agent.id).map_or_else(String::new, |v| chat::handoff::transcript(&v.session.transcript, &self.agent_name(&agent.provider)))
    }

    /// A chat can't change provider, so a model from another one opens a new chat: asked which kind if there's a
    /// conversation to carry, else in this one's place.
    pub(super) fn switch_chat_agent(&mut self, id: &str, provider: String, model: String, cx: &mut Context<Self>) {
        close_menu(self, cx);
        let Some(agent) = self.agents.get(id) else { return };
        let switch = switching(!self.chat_conversation(agent).is_empty(), self.chat_busy(agent), agent.tokens_used);
        let terminal = agent.terminal_id.clone();
        if switch == Switch::Ask {
            self.confirm = Some(Confirm::SwitchAgent { agent: id.to_string(), provider, model });
            self.overlay = Some(Overlay::Confirm);
            return cx.notify();
        }
        if self.open_switched_chat(id, &provider, &model, false, cx) && switch == Switch::Replace {
            self.close_pane(&terminal, cx);
        }
    }

    /// Opens the new chat beside `id`'s, `fork` telling it the conversation so far. Whether the create was sent.
    pub(crate) fn open_switched_chat(&mut self, id: &str, provider: &str, model: &str, fork: bool, cx: &mut Context<Self>) -> bool {
        if self.terminals.link.stale().is_some() {
            self.error = Some(link::UPDATING.into());
            cx.notify();
            return false;
        }
        let projects = self.projects();
        let Some(s) = self.agents.get(id) else { return false };
        let Some(project) = self.project_of(&s.cwd, &projects) else {
            self.error = Some("Couldn't find this chat's project".into());
            cx.notify();
            return false;
        };
        let prompt = if fork { chat::handoff::prompt(&self.chat_conversation(s), &self.agent_name(&s.provider)) } else { String::new() };
        let (plan, efforts) = (self.agents.provider_info(provider).plan, self.launch_efforts(provider, model));
        let spec = switched_spec(s, project, (provider, model), &prompt, &self.store.agents, plan, &efforts);
        let request = self.send_create(spec);
        let sent = request.is_some();
        if let Some(request) = request {
            self.terminals.tab_chats.insert(request, Place::Pane(None));
        }
        cx.notify();
        sent
    }
}

#[cfg(test)]
mod tests {
    use super::{Agents, Summary, Switch, json, switched_spec, switching};

    fn chat(access: Option<&str>) -> Summary {
        Summary { provider: "claude".into(), cwd: "/w/tree".into(), access: access.map(Into::into), model: Some("opus".into()), effort: Some("max".into()), account: "work".into(), ..Default::default() }
    }

    #[test]
    fn only_a_chat_with_a_conversation_asks() {
        assert_eq!(switching(true, false, 0), Switch::Ask);
        assert_eq!(switching(true, true, 900), Switch::Ask);
    }

    #[test]
    fn only_an_idle_chat_that_never_ran_is_replaced() {
        assert_eq!(switching(false, false, 0), Switch::Replace);
        assert_eq!(switching(false, true, 0), Switch::Open);
        assert_eq!(switching(false, false, 900), Switch::Open, "its history may not have loaded");
    }

    #[test]
    fn the_new_chat_runs_the_picked_model_as_a_chat_in_the_same_folder() {
        let spec = switched_spec(&chat(Some("edits")), "/w", ("codex", "gpt-6-sol"), "Continue", &Agents::default(), false, &[]);
        assert_eq!((&spec["project"], &spec["checkout"], &spec["provider"]), (&json!("/w"), &json!({"worktree": "/w/tree"}), &json!("codex")));
        assert_eq!((&spec["model"], &spec["chat"], &spec["prompt"]), (&json!("gpt-6-sol"), &json!(true), &json!("Continue")));
    }

    #[test]
    fn moving_to_another_agent_never_grants_more_access_than_the_chat_had() {
        let full = Agents { access: "full".into(), ..Agents::default() };
        assert_eq!(switched_spec(&chat(Some("ask")), "/w", ("codex", "m"), "", &full, false, &[])["access"], json!("ask"));
        assert_eq!(switched_spec(&chat(None), "/w", ("codex", "m"), "", &full, false, &[])["access"], json!("settings"));
    }

    #[test]
    fn the_old_chats_effort_and_account_stay_behind() {
        let spec = switched_spec(&chat(None), "/w", ("codex", "m"), "", &Agents::default(), false, &[]);
        assert!(spec.get("effort").is_none());
        assert!(spec.get("account").is_none());
        assert!(spec.get("prompt").is_none(), "a fresh chat opens empty");
    }
}
