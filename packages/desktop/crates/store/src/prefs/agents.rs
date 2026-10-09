use crate::LaunchPick;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The permission modes a new session can ask for; "settings" leaves it to the agent's own settings.
pub const ACCESSES: [&str; 5] = ["settings", "ask", "edits", "auto", "full"];

/// Where "Add to chat" sends a quote when no agent is picked; "ask" never picks one.
pub const CHAT_TO: [&str; 3] = ["focused", "recent", "ask"];

/// Claude Code's model aliases; its CLI lists no models.
pub const CLAUDE_MODELS: [&str; 3] = ["haiku", "sonnet", "opus"];

/// The efforts pocketd passes to Claude Code.
pub const CLAUDE_EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// The efforts `provider` takes; pocketd refuses one for Codex.
pub fn efforts(provider: &str) -> &'static [&'static str] {
    if provider == PLANS { &CLAUDE_EFFORTS } else { &[] }
}

/// A provider's own settings; empty leaves each to the agent.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Clone)]
#[serde(default)]
pub struct Provider {
    pub model: String,
    pub effort: String,
    /// pocketd's, mirrored: it launches the agent. Empty is the provider's name on the login PATH.
    pub command: String,
}

/// Providers, launch defaults and sessions.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(default)]
pub struct Agents {
    /// Providers turned off; they leave every picker.
    pub disabled: BTreeSet<String>,
    /// Starts sessions in a project that hasn't picked one yet.
    pub default: String,
    pub access: String,
    pub plan: bool,
    pub new_worktree: bool,
    /// ⌘T and splits open the default agent instead of a shell.
    pub tabs_open_agent: bool,
    pub starters: Vec<String>,
    /// "Add to chat" presses Return after the quote.
    pub send_now: bool,
    /// One of `CHAT_TO`.
    pub chat_to: String,
    /// A tab closes when its agent exits, instead of returning to the shell.
    pub close_on_exit: bool,
    /// By provider.
    pub providers: BTreeMap<String, Provider>,
}

impl Default for Agents {
    fn default() -> Self {
        Self {
            disabled: BTreeSet::new(),
            default: "claude".into(),
            access: "settings".into(),
            plan: false,
            new_worktree: false,
            tabs_open_agent: false,
            starters: ["Review my uncommitted changes", "Fix the failing tests", "Summarise what this branch changes", "Write the commit message"].map(String::from).to_vec(),
            send_now: false,
            chat_to: "focused".into(),
            close_on_exit: false,
            providers: BTreeMap::new(),
        }
    }
}

impl Agents {
    /// The providers left on; never none, since a session needs one.
    pub fn enabled(&self) -> Vec<&'static str> {
        let on: Vec<_> = LaunchPick::PROVIDERS.into_iter().filter(|p| !self.disabled.contains(*p)).collect();
        if on.is_empty() { LaunchPick::PROVIDERS.to_vec() } else { on }
    }

    /// Turns a provider on or off, refusing to turn off the last one on.
    pub fn set_enabled(&mut self, provider: &str, on: bool) {
        if on {
            self.disabled.remove(provider);
        } else if self.enabled().len() > 1 {
            self.disabled.insert(provider.into());
        }
    }

    /// The default agent if it's on, else the first one that is.
    pub fn default_agent(&self) -> &'static str {
        let on = self.enabled();
        on.iter().copied().find(|p| *p == self.default).unwrap_or(on[0])
    }

    /// `provider` if it's on, else the default agent.
    pub fn usable(&self, provider: &str) -> &'static str {
        self.enabled().into_iter().find(|p| *p == provider).unwrap_or_else(|| self.default_agent())
    }

    /// The permission mode to send; an unknown one leaves it to the agent.
    pub fn access(&self) -> &'static str {
        ACCESSES.into_iter().find(|a| *a == self.access).unwrap_or("settings")
    }

    pub fn provider(&self, provider: &str) -> Provider {
        self.providers.get(provider).cloned().unwrap_or_default()
    }

    pub fn provider_mut(&mut self, provider: &str) -> &mut Provider {
        self.providers.entry(provider.into()).or_default()
    }

    /// What runs `provider`: its command, else its name.
    pub fn command(&self, provider: &str) -> String {
        Some(self.provider(provider).command).filter(|c| !c.is_empty()).unwrap_or_else(|| provider.into())
    }

    /// The project's own agent where it set one, else what it last started, else the default agent; a provider now off falls back too.
    pub fn pick(&self, project: &LaunchPick, remembered: LaunchPick) -> LaunchPick {
        if !project.provider.is_empty() {
            return project.switched(self.usable(&project.provider));
        }
        let provider = if remembered.provider.is_empty() { self.default_agent() } else { self.usable(&remembered.provider) };
        remembered.switched(provider)
    }

    /// A create spec with this permission mode, plan mode where its agent has one, and its provider's model and effort where it names none.
    pub fn launch(&self, spec: &mut serde_json::Value) {
        spec["access"] = self.access().into();
        spec["plan"] = (self.plan && spec["provider"] == PLANS).into();
        let provider = spec["provider"].as_str().unwrap_or_default().to_string();
        let own = self.provider(&provider);
        if spec.get("model").is_none() && !own.model.is_empty() {
            spec["model"] = own.model.into();
        }
        if spec.get("effort").is_none() && efforts(&provider).contains(&own.effort.as_str()) {
            spec["effort"] = own.effort.into();
        }
    }

    /// The `CHAT_TO` in effect; an unknown one is "focused".
    pub fn chat_to(&self) -> &'static str {
        CHAT_TO.into_iter().find(|t| *t == self.chat_to).unwrap_or("focused")
    }

    /// Moves starter prompt `from` to where `to` is, shifting the ones between.
    pub fn move_starter(&mut self, from: usize, to: usize) {
        if from < self.starters.len() && to < self.starters.len() {
            let s = self.starters.remove(from);
            self.starters.insert(to, s);
        }
    }

    /// Adds a starter prompt at the end; refuses a blank one or one already there.
    pub fn add_starter(&mut self, text: &str) -> bool {
        let text = text.trim();
        let fresh = !text.is_empty() && !self.starters.iter().any(|s| s == text);
        if fresh {
            self.starters.push(text.into());
        }
        fresh
    }
}

/// The one agent with a plan mode; pocketd refuses plan for Codex.
const PLANS: &str = "claude";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_blank_or_repeated_starter_prompt_is_not_added() {
        let mut a = Agents::default();
        assert!(a.add_starter("  Explain this repo "));
        assert!(!a.add_starter("Explain this repo"));
        assert!(!a.add_starter("   "));
        assert_eq!(a.starters.last().map(String::as_str), Some("Explain this repo"));
    }

    #[test]
    fn the_last_provider_on_cannot_be_turned_off() {
        let mut a = Agents::default();
        a.set_enabled("claude", false);
        a.set_enabled("codex", false);
        assert_eq!(a.enabled(), ["codex"]);
    }

    #[test]
    fn a_turned_off_default_falls_back_to_one_that_is_on() {
        let mut a = Agents::default();
        a.set_enabled("claude", false);
        assert_eq!((a.default_agent(), a.usable("claude"), a.usable("codex")), ("codex", "codex", "codex"));
    }

    #[test]
    fn an_unknown_permission_mode_leaves_it_to_the_agent() {
        let a = Agents { access: "yolo".into(), ..Agents::default() };
        assert_eq!(a.access(), "settings");
    }

    #[test]
    fn a_project_without_a_pick_starts_the_default_agent() {
        let a = Agents { default: "codex".into(), ..Agents::default() };
        let none = LaunchPick::default();
        assert_eq!(a.pick(&none, LaunchPick::default()).provider, "codex");
        let opus = LaunchPick { provider: "claude".into(), model: "opus".into(), effort: String::new() };
        assert_eq!(a.pick(&none, opus.clone()), opus);
    }

    #[test]
    fn a_project_s_own_agent_wins_over_what_it_last_started() {
        let a = Agents::default();
        let own = LaunchPick { provider: "codex".into(), model: "gpt-5".into(), effort: String::new() };
        let last = LaunchPick { provider: "claude".into(), model: "opus".into(), effort: "high".into() };
        assert_eq!(a.pick(&own, last.clone()), own);
        let off = Agents { disabled: ["codex".to_string()].into(), ..Agents::default() };
        assert_eq!(off.pick(&own, last), LaunchPick { provider: "claude".into(), ..LaunchPick::default() });
    }

    #[test]
    fn a_launch_takes_its_provider_s_model_and_effort_unless_it_names_its_own() {
        let mut a = Agents::default();
        *a.provider_mut("claude") = Provider { model: "opus".into(), effort: "high".into(), command: String::new() };
        *a.provider_mut("codex") = Provider { model: "gpt-5".into(), effort: "high".into(), command: String::new() };
        let (mut claude, mut named, mut codex) = (json!({"provider": "claude"}), json!({"provider": "claude", "model": "haiku"}), json!({"provider": "codex"}));
        [&mut claude, &mut named, &mut codex].into_iter().for_each(|s| a.launch(s));
        assert_eq!((&claude["model"], &claude["effort"]), (&json!("opus"), &json!("high")));
        assert_eq!((&named["model"], &named["effort"]), (&json!("haiku"), &json!("high")));
        assert_eq!((&codex["model"], codex.get("effort")), (&json!("gpt-5"), None));
    }

    #[test]
    fn a_provider_runs_its_command_or_else_its_name() {
        let mut a = Agents::default();
        a.provider_mut("codex").command = "/opt/codex-dev".into();
        assert_eq!((a.command("claude"), a.command("codex")), ("claude".to_string(), "/opt/codex-dev".to_string()));
    }

    #[test]
    fn plan_mode_goes_only_to_the_agent_that_has_one() {
        let a = Agents { access: "auto".into(), plan: true, ..Agents::default() };
        let (mut claude, mut codex) = (serde_json::json!({"provider": "claude"}), serde_json::json!({"provider": "codex"}));
        a.launch(&mut claude);
        a.launch(&mut codex);
        assert_eq!((&claude["access"], &claude["plan"], &codex["access"], &codex["plan"]), (&"auto".into(), &true.into(), &"auto".into(), &false.into()));
    }

    #[test]
    fn a_file_without_agent_settings_keeps_todays_behaviour() {
        let a: Agents = serde_json::from_str("{}").unwrap();
        assert_eq!((a.access(), a.plan, a.new_worktree, a.tabs_open_agent, a.send_now, a.starters.len()), ("settings", false, false, false, false, 4));
        assert_eq!((a.chat_to(), a.close_on_exit, a.provider("claude")), ("focused", false, Provider::default()));
    }

    #[test]
    fn a_dragged_starter_prompt_lands_where_it_was_dropped() {
        let mut a = Agents { starters: ["a", "b", "c", "d"].map(String::from).to_vec(), ..Agents::default() };
        a.move_starter(0, 2);
        assert_eq!(a.starters, ["b", "c", "a", "d"]);
        a.move_starter(3, 0);
        assert_eq!(a.starters, ["d", "b", "c", "a"]);
        a.move_starter(1, 9);
        assert_eq!(a.starters, ["d", "b", "c", "a"]);
    }

    #[test]
    fn an_unknown_add_to_chat_target_sends_to_the_focused_session() {
        let a = Agents { chat_to: "nowhere".into(), ..Agents::default() };
        assert_eq!(a.chat_to(), "focused");
    }
}
