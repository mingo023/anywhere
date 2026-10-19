use crate::LaunchPick;
use agents::providers;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The permission modes a new session can ask for; "settings" leaves it to the agent's own settings.
pub const ACCESSES: [&str; 5] = ["settings", "ask", "edits", "auto", "full"];

/// Where "Add to chat" sends a quote when no agent is picked; "ask" never picks one.
pub const CHAT_TO: [&str; 3] = ["focused", "recent", "ask"];

/// A provider's own settings; empty leaves each to the agent.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Clone)]
#[serde(default)]
pub struct Provider {
    pub model: String,
    pub effort: String,
    /// pocketd's, mirrored: it launches the agent. Empty is the provider's name on the login PATH.
    pub command: String,
    /// Shown instead of the provider's own name; empty keeps that.
    pub label: String,
    /// pocketd's, mirrored, as typed; `None` is `Args::default_text`.
    pub prompt_args: Option<String>,
    pub resume_args: Option<String>,
    pub fork_args: Option<String>,
}

/// Where fork args take the source session's id.
pub const SESSION_ID: &str = "{sessionId}";

/// The words pocketd puts around a provider's command; empty leaves them out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Args {
    Prompt,
    Resume,
    Fork,
}

impl Args {
    pub const ALL: [Self; 3] = [Self::Prompt, Self::Resume, Self::Fork];

    /// pocketd's config key under the provider.
    pub fn key(self) -> &'static str {
        match self {
            Self::Prompt => "promptArgs",
            Self::Resume => "resumeArgs",
            Self::Fork => "forkArgs",
        }
    }

    /// pocketd's default when config.json sets none; pocketd's config/args.go holds the same.
    pub fn default_text(self, provider: &str) -> &'static str {
        match self {
            Self::Prompt => "--",
            Self::Resume => providers::known(provider).resume_args,
            Self::Fork => providers::known(provider).fork_args,
        }
    }
}

impl Provider {
    pub fn args_mut(&mut self, which: Args) -> &mut Option<String> {
        match which {
            Args::Prompt => &mut self.prompt_args,
            Args::Resume => &mut self.resume_args,
            Args::Fork => &mut self.fork_args,
        }
    }
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

    /// `provider`'s args as typed, else pocketd's default.
    pub fn args(&self, provider: &str, which: Args) -> String {
        self.provider(provider).args_mut(which).clone().unwrap_or_else(|| which.default_text(provider).into())
    }

    /// The name `provider` shows under, when its label sets one.
    pub fn label(&self, provider: &str) -> Option<String> {
        Some(self.provider(provider).label.trim().to_string()).filter(|l| !l.is_empty())
    }

    /// What runs `provider`: its command, else its name.
    pub fn command(&self, provider: &str) -> String {
        Some(self.provider(provider).command).filter(|c| !c.is_empty()).unwrap_or_else(|| provider.into())
    }

    /// What the project last started, else the default agent; a provider now off falls back too.
    pub fn pick(&self, remembered: LaunchPick) -> LaunchPick {
        let provider = if remembered.provider.is_empty() { self.default_agent() } else { self.usable(&remembered.provider) };
        LaunchPick { account: String::new(), ..remembered.switched(provider) }
    }

    /// A create spec with plan mode where its agent has one (`plan`), and this permission mode and its provider's model and effort, if one of its `efforts`, where it names none.
    pub fn launch(&self, spec: &mut serde_json::Value, plan: bool, efforts: &[String]) {
        if spec.get("access").is_none() {
            spec["access"] = self.access().into();
        }
        spec["plan"] = (self.plan && plan).into();
        let provider = spec["provider"].as_str().unwrap_or_default().to_string();
        let own = self.provider(&provider);
        if spec.get("model").is_none() && !own.model.is_empty() {
            spec["model"] = own.model.into();
        }
        if spec.get("effort").is_none() && efforts.contains(&own.effort) {
            spec["effort"] = own.effort.into();
        }
    }

    /// The `CHAT_TO` in effect; an unknown one is "focused".
    pub fn chat_to(&self) -> &'static str {
        CHAT_TO.into_iter().find(|t| *t == self.chat_to).unwrap_or("focused")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn claude_efforts() -> Vec<String> {
        ["low", "medium", "high", "xhigh", "max"].map(String::from).to_vec()
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
        assert_eq!(a.pick(LaunchPick::default()).provider, "codex");
        let opus = LaunchPick { provider: "claude".into(), model: "opus".into(), ..LaunchPick::default() };
        assert_eq!(a.pick(opus.clone()), opus);
    }

    #[test]
    fn what_a_project_last_started_with_falls_back_when_its_agent_is_off() {
        let last = LaunchPick { provider: "codex".into(), model: "gpt-5".into(), access: "auto".into(), ..LaunchPick::default() };
        let off = Agents { disabled: ["codex".to_string()].into(), ..Agents::default() };
        assert_eq!(off.pick(last), LaunchPick { provider: "claude".into(), access: "auto".into(), ..LaunchPick::default() });
    }

    #[test]
    fn a_new_agent_starts_on_the_default_account_whatever_the_project_last_started_on() {
        let last = LaunchPick { provider: "codex".into(), account: "work".into(), ..LaunchPick::default() };
        assert_eq!(Agents::default().pick(last).account, "");
    }

    #[test]
    fn a_launch_takes_its_provider_s_model_and_effort_unless_it_names_its_own() {
        let mut a = Agents::default();
        *a.provider_mut("claude") = Provider { model: "opus".into(), effort: "high".into(), ..Provider::default() };
        *a.provider_mut("codex") = Provider { model: "gpt-5".into(), effort: "high".into(), ..Provider::default() };
        let (mut claude, mut named, mut codex) = (json!({"provider": "claude"}), json!({"provider": "claude", "model": "haiku"}), json!({"provider": "codex"}));
        a.launch(&mut claude, true, &claude_efforts());
        a.launch(&mut named, true, &claude_efforts());
        a.launch(&mut codex, false, &[]);
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
    fn args_are_pocketd_s_default_until_typed_and_empty_turns_them_off() {
        let mut a = Agents::default();
        assert_eq!((a.args("claude", Args::Resume), a.args("codex", Args::Resume)), ("--resume".to_string(), "resume".to_string()));
        assert_eq!(a.args("codex", Args::Fork), "fork {sessionId}");
        a.provider_mut("claude").resume_args = Some(String::new());
        assert_eq!(a.args("claude", Args::Resume), "");
    }

    #[test]
    fn a_blank_label_keeps_the_provider_s_own_name() {
        let mut a = Agents::default();
        a.provider_mut("claude").label = "  ".into();
        assert_eq!(a.label("claude"), None);
        a.provider_mut("claude").label = " Claude ".into();
        assert_eq!(a.label("claude").as_deref(), Some("Claude"));
    }

    #[test]
    fn plan_mode_goes_only_to_the_agent_that_has_one() {
        let a = Agents { access: "auto".into(), plan: true, ..Agents::default() };
        let (mut claude, mut codex) = (serde_json::json!({"provider": "claude"}), serde_json::json!({"provider": "codex"}));
        a.launch(&mut claude, true, &claude_efforts());
        a.launch(&mut codex, false, &[]);
        assert_eq!((&claude["access"], &claude["plan"], &codex["access"], &codex["plan"]), (&"auto".into(), &true.into(), &"auto".into(), &false.into()));
    }

    #[test]
    fn a_launch_takes_the_app_s_permission_mode_unless_it_names_its_own() {
        let a = Agents { access: "auto".into(), ..Agents::default() };
        let mut named = json!({"provider": "claude", "access": "ask"});
        a.launch(&mut named, true, &claude_efforts());
        assert_eq!(named["access"], "ask");
    }

    #[test]
    fn a_file_without_agent_settings_keeps_todays_behaviour() {
        let a: Agents = serde_json::from_str("{}").unwrap();
        assert_eq!((a.access(), a.plan, a.new_worktree, a.tabs_open_agent, a.send_now), ("settings", false, false, false, false));
        assert_eq!((a.chat_to(), a.close_on_exit, a.provider("claude")), ("focused", false, Provider::default()));
    }

    #[test]
    fn an_unknown_add_to_chat_target_sends_to_the_focused_session() {
        let a = Agents { chat_to: "nowhere".into(), ..Agents::default() };
        assert_eq!(a.chat_to(), "focused");
    }
}
