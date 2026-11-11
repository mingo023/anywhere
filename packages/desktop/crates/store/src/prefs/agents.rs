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
            Self::Prompt => providers::known(provider).prompt_args,
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
    /// New sessions start as a chat rather than in the agent's TUI.
    pub start_as_chat: bool,
    /// By provider.
    pub providers: BTreeMap<String, Provider>,
    /// Models starred in the model panel, as (provider, model), oldest first.
    pub favorites: Vec<(String, String)>,
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
            start_as_chat: false,
            providers: BTreeMap::new(),
            favorites: Vec::new(),
        }
    }
}

impl Agents {
    /// The providers left on; never none, since a session needs one.
    pub fn enabled(&self) -> Vec<&'static str> {
        let on: Vec<_> = LaunchPick::PROVIDERS.into_iter().filter(|p| !self.disabled.contains(*p)).collect();
        if on.is_empty() { LaunchPick::PROVIDERS.to_vec() } else { on }
    }

    /// The providers the pickers offer: the ones left on whose CLI pocketd found, which `live` holds. Before pocketd
    /// answers, and from one too old to list an agent, only Claude and Codex count as found. With none of those left
    /// on, every one found; none only when no agent is installed.
    pub fn offered(&self, live: &agents::Agents) -> Vec<&'static str> {
        let found: Vec<_> = LaunchPick::PROVIDERS.into_iter().filter(|p| live.provider_info(p).available).collect();
        let on: Vec<_> = found.iter().copied().filter(|p| !self.disabled.contains(*p)).collect();
        if on.is_empty() { found } else { on }
    }

    pub fn enable(&mut self, provider: &str) {
        self.disabled.remove(provider);
    }

    /// Turns a provider off, unless it's the last one offered: a session needs an agent that is installed.
    pub fn disable(&mut self, provider: &str, live: &agents::Agents) {
        if self.offered(live) != [provider] {
            self.disabled.insert(provider.into());
        }
    }

    /// The default agent if it's offered, else the first one that is; with no agent installed, the default as saved.
    pub fn default_agent(&self, live: &agents::Agents) -> &'static str {
        let (offered, saved) = (self.offered(live), LaunchPick::known_provider(&self.default));
        if offered.contains(&saved) { saved } else { offered.first().copied().unwrap_or(saved) }
    }

    /// `provider` if it's offered, else the default agent.
    pub fn usable(&self, provider: &str, live: &agents::Agents) -> &'static str {
        self.offered(live).into_iter().find(|p| *p == provider).unwrap_or_else(|| self.default_agent(live))
    }

    /// Whether a session of `provider` started with a prompt is a chat: when Settings say so, and always for an agent
    /// whose status only a chat reports.
    pub fn starts_as_chat(&self, provider: &str) -> bool {
        self.start_as_chat || providers::get(provider).is_some_and(|b| b.chat_only_status)
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

    /// What runs `provider`: the command Settings names, else the agent's own, else its name.
    pub fn command(&self, provider: &str) -> String {
        Some(self.provider(provider).command).filter(|c| !c.is_empty()).unwrap_or_else(|| providers::get(provider).map_or(provider, |b| b.command).into())
    }

    /// What lists `provider`'s models: its command, or the one its catalog runs instead, and the catalog's args.
    pub fn catalog_argv(&self, provider: &str) -> Vec<String> {
        let models = &providers::known(provider).models;
        let command = if models.command.is_empty() { self.command(provider) } else { models.command.into() };
        std::iter::once(command).chain(models.args.iter().map(|a| a.to_string())).collect()
    }

    /// What the project last started, else the default agent; a provider no longer offered falls back too.
    pub fn pick(&self, remembered: LaunchPick, live: &agents::Agents) -> LaunchPick {
        let provider = if remembered.provider.is_empty() { self.default_agent(live) } else { self.usable(&remembered.provider, live) };
        LaunchPick { account: String::new(), ..remembered.switched(provider) }
    }

    /// A create spec with what `live` says of its agent: plan mode where it has one, full access where it never asks,
    /// and a chat where only one reports its status. Else this permission mode and chat or TUI where the spec names
    /// none, and its provider's model and effort, if one of its `efforts`, where it names none.
    pub fn launch(&self, spec: &mut serde_json::Value, live: &agents::Agents, efforts: &[String]) {
        let provider = spec["provider"].as_str().unwrap_or_default().to_string();
        let agent = live.provider_info(&provider);
        if agent.never_asks() {
            spec["access"] = "full".into();
        } else if spec.get("access").is_none() {
            spec["access"] = self.access().into();
        }
        let chat_only = providers::get(&provider).is_some_and(|b| b.chat_only_status);
        if chat_only || (self.start_as_chat && spec.get("chat").is_none()) {
            spec["chat"] = true.into();
        }
        spec["plan"] = (self.plan && agent.plan).into();
        let own = self.provider(&provider);
        if spec.get("model").is_none() && !own.model.is_empty() {
            spec["model"] = own.model.into();
        }
        if spec.get("effort").is_none() && efforts.contains(&own.effort) {
            spec["effort"] = own.effort.into();
        }
    }

    pub fn is_favorite(&self, provider: &str, model: &str) -> bool {
        self.favorites.iter().any(|(p, m)| p == provider && m == model)
    }

    /// Stars `provider`'s `model`, or takes its star off.
    pub fn toggle_favorite(&mut self, provider: &str, model: &str) {
        if self.is_favorite(provider, model) {
            self.favorites.retain(|(p, m)| p != provider || m != model);
        } else {
            self.favorites.push((provider.into(), model.into()));
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
    fn every_agent_is_on_until_turned_off_and_the_last_one_installed_cannot_be() {
        let mut a = Agents::default();
        assert_eq!(a.enabled(), ["claude", "codex", "cursor", "grok", "opencode", "pi", "omp", "fx", "hermes", "antigravity"]);
        let live = pocketd(&[("claude", true), ("codex", false), ("pi", true), ("hermes", true)]);
        for p in LaunchPick::PROVIDERS {
            a.disable(p, &live);
        }
        assert_eq!((a.enabled(), a.offered(&live)), (vec!["hermes"], vec!["hermes"]));
        a.enable("claude");
        a.disable("hermes", &live);
        assert_eq!(a.offered(&live), ["claude"]);
    }

    #[test]
    fn a_user_with_claude_and_codex_cannot_turn_both_off_and_is_never_offered_one_that_is_not_installed() {
        let (mut a, live) = (Agents::default(), agents::Agents::default());
        a.disable("claude", &live);
        a.disable("codex", &live);
        assert_eq!((a.offered(&live), a.default_agent(&live), a.usable("cursor", &live)), (vec!["codex"], "codex", "codex"));
        let both_off = Agents { disabled: ["claude".to_string(), "codex".to_string()].into(), ..Agents::default() };
        assert_eq!((both_off.offered(&live), both_off.default_agent(&live)), (vec!["claude", "codex"], "claude"), "a file that has both off");
    }

    fn pocketd(installed: &[(&str, bool)]) -> agents::Agents {
        let providers = installed.iter().map(|(id, available)| providers::ProviderInfo { id: id.to_string(), available: *available, ..Default::default() }).collect();
        agents::Agents { providers, ..Default::default() }
    }

    #[test]
    fn only_agents_left_on_and_installed_are_offered() {
        let mut a = Agents::default();
        assert_eq!(a.offered(&agents::Agents::default()), ["claude", "codex"], "before pocketd answers");
        assert_eq!(a.offered(&pocketd(&[("claude", true), ("codex", false)])), ["claude"], "a pocketd older than the other agents");
        let live = pocketd(&[("claude", true), ("codex", false), ("cursor", false), ("pi", true), ("hermes", true)]);
        assert_eq!(a.offered(&live), ["claude", "pi", "hermes"]);
        a.disable("pi", &live);
        assert_eq!(a.offered(&live), ["claude", "hermes"]);
    }

    #[test]
    fn with_nothing_installed_no_agent_is_offered_and_the_default_stays_the_one_saved() {
        let a = Agents { default: "codex".into(), ..Agents::default() };
        let live = pocketd(&[("claude", false), ("codex", false)]);
        assert_eq!((a.offered(&live), a.default_agent(&live), a.usable("pi", &live)), (vec![], "codex", "codex"));
    }

    #[test]
    fn a_default_that_is_off_or_not_installed_falls_back_to_one_that_is_offered() {
        let (mut a, live) = (Agents::default(), agents::Agents::default());
        a.disable("claude", &live);
        assert_eq!((a.default_agent(&live), a.usable("claude", &live), a.usable("codex", &live)), ("codex", "codex", "codex"));
        let cursor = Agents { default: "cursor".into(), ..Agents::default() };
        assert_eq!((cursor.default_agent(&live), cursor.default_agent(&pocketd(&[("cursor", true)]))), ("claude", "cursor"));
    }

    #[test]
    fn an_unknown_permission_mode_leaves_it_to_the_agent() {
        let a = Agents { access: "yolo".into(), ..Agents::default() };
        assert_eq!(a.access(), "settings");
    }

    #[test]
    fn a_project_without_a_pick_starts_the_default_agent() {
        let a = Agents { default: "codex".into(), ..Agents::default() };
        let live = agents::Agents::default();
        assert_eq!(a.pick(LaunchPick::default(), &live).provider, "codex");
        let opus = LaunchPick { provider: "claude".into(), model: "opus".into(), ..LaunchPick::default() };
        assert_eq!(a.pick(opus.clone(), &live), opus);
    }

    #[test]
    fn what_a_project_last_started_with_falls_back_when_its_agent_is_off() {
        let last = LaunchPick { provider: "codex".into(), model: "gpt-5".into(), access: "auto".into(), ..LaunchPick::default() };
        let off = Agents { disabled: ["codex".to_string()].into(), ..Agents::default() };
        assert_eq!(off.pick(last, &agents::Agents::default()), LaunchPick { provider: "claude".into(), access: "auto".into(), ..LaunchPick::default() });
    }

    #[test]
    fn what_a_project_last_started_with_falls_back_when_its_agent_is_not_installed() {
        let last = LaunchPick { provider: "pi".into(), model: "deepseek/deepseek-v4-pro".into(), ..LaunchPick::default() };
        let a = Agents::default();
        assert_eq!(a.pick(last.clone(), &pocketd(&[("claude", true), ("pi", false)])), LaunchPick { provider: "claude".into(), ..LaunchPick::default() });
        assert_eq!(a.pick(last.clone(), &pocketd(&[("claude", true), ("pi", true)])), last);
    }

    #[test]
    fn a_new_agent_starts_on_the_default_account_whatever_the_project_last_started_on() {
        let last = LaunchPick { provider: "codex".into(), account: "work".into(), ..LaunchPick::default() };
        assert_eq!(Agents::default().pick(last, &agents::Agents::default()).account, "");
    }

    #[test]
    fn a_launch_takes_its_provider_s_model_and_effort_unless_it_names_its_own() {
        let mut a = Agents::default();
        *a.provider_mut("claude") = Provider { model: "opus".into(), effort: "high".into(), ..Provider::default() };
        *a.provider_mut("codex") = Provider { model: "gpt-5".into(), effort: "high".into(), ..Provider::default() };
        let (mut claude, mut named, mut codex) = (json!({"provider": "claude"}), json!({"provider": "claude", "model": "haiku"}), json!({"provider": "codex"}));
        let live = agents::Agents::default();
        a.launch(&mut claude, &live, &claude_efforts());
        a.launch(&mut named, &live, &claude_efforts());
        a.launch(&mut codex, &live, &[]);
        assert_eq!((&claude["model"], &claude["effort"]), (&json!("opus"), &json!("high")));
        assert_eq!((&named["model"], &named["effort"]), (&json!("haiku"), &json!("high")));
        assert_eq!((&codex["model"], codex.get("effort")), (&json!("gpt-5"), None));
    }

    #[test]
    fn a_provider_runs_the_command_settings_names_or_else_its_own() {
        let mut a = Agents::default();
        a.provider_mut("codex").command = "/opt/codex-dev".into();
        assert_eq!((a.command("claude"), a.command("codex")), ("claude".to_string(), "/opt/codex-dev".to_string()));
        assert_eq!(["cursor", "antigravity", "gemini"].map(|p| a.command(p)), ["cursor-agent", "agy", "gemini"]);
    }

    #[test]
    fn models_are_listed_by_the_agent_s_command_unless_its_catalog_runs_another() {
        let mut a = Agents::default();
        a.provider_mut("pi").command = "/opt/pi".into();
        a.provider_mut("antigravity").command = "/opt/agy".into();
        assert_eq!(a.catalog_argv("pi"), ["/opt/pi", "--mode", "rpc", "--no-session"]);
        assert_eq!(a.catalog_argv("cursor"), ["cursor-agent", "--list-models"]);
        assert_eq!(a.catalog_argv("antigravity")[..2], ["sh", "-c"]);
    }

    #[test]
    fn args_are_pocketd_s_default_until_typed_and_empty_turns_them_off() {
        let mut a = Agents::default();
        assert_eq!((a.args("claude", Args::Resume), a.args("codex", Args::Resume)), ("--resume".to_string(), "resume".to_string()));
        assert_eq!(a.args("codex", Args::Fork), "fork {sessionId}");
        assert_eq!(["claude", "opencode", "hermes", "cursor"].map(|p| a.args(p, Args::Prompt)), ["--", "--prompt", "-q", ""]);
        assert_eq!((a.args("pi", Args::Resume), a.args("pi", Args::Fork), a.args("hermes", Args::Resume), a.args("hermes", Args::Fork)), ("--session".to_string(), "--fork {sessionId}".to_string(), "chat --resume".to_string(), String::new()));
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
        a.launch(&mut claude, &agents::Agents::default(), &claude_efforts());
        a.launch(&mut codex, &agents::Agents::default(), &[]);
        assert_eq!((&claude["access"], &claude["plan"], &codex["access"], &codex["plan"]), (&"auto".into(), &true.into(), &"auto".into(), &false.into()));
    }

    #[test]
    fn a_launch_takes_the_app_s_permission_mode_unless_it_names_its_own() {
        let a = Agents { access: "auto".into(), ..Agents::default() };
        let mut named = json!({"provider": "claude", "access": "ask"});
        a.launch(&mut named, &agents::Agents::default(), &claude_efforts());
        assert_eq!(named["access"], "ask");
    }

    #[test]
    fn an_agent_that_never_asks_launches_with_full_access_whatever_was_picked() {
        let (a, live) = (Agents { access: "ask".into(), ..Agents::default() }, agents::Agents::default());
        for provider in ["pi", "omp"] {
            let (mut carried, mut unnamed) = (json!({"provider": provider, "access": "edits"}), json!({"provider": provider}));
            a.launch(&mut carried, &live, &[]);
            a.launch(&mut unnamed, &live, &[]);
            assert_eq!((&carried["access"], &unnamed["access"]), (&json!("full"), &json!("full")), "{provider}");
        }
        let said = |never_asks| agents::Agents { providers: vec![providers::ProviderInfo { id: "cursor".into(), never_asks, ..Default::default() }], ..Default::default() };
        let (mut unasked, mut asked) = (json!({"provider": "cursor", "access": "edits"}), json!({"provider": "cursor", "access": "edits"}));
        a.launch(&mut unasked, &said(Some(true)), &[]);
        a.launch(&mut asked, &said(None), &[]);
        assert_eq!((&unasked["access"], &asked["access"]), (&json!("full"), &json!("edits")), "pocketd's word on an agent wins");
    }

    #[test]
    fn a_launch_starts_as_a_chat_only_when_set_unless_it_names_its_own() {
        let (off, on) = (Agents::default(), Agents { start_as_chat: true, ..Agents::default() });
        let (mut tui, mut chat, mut named) = (json!({"provider": "claude"}), json!({"provider": "claude"}), json!({"provider": "claude", "chat": true}));
        let live = agents::Agents::default();
        off.launch(&mut tui, &live, &claude_efforts());
        on.launch(&mut chat, &live, &claude_efforts());
        off.launch(&mut named, &live, &claude_efforts());
        assert_eq!((tui.get("chat"), &chat["chat"], &named["chat"]), (None, &true.into(), &true.into()));
    }

    #[test]
    fn an_agent_whose_status_only_a_chat_reports_starts_as_a_chat_whatever_settings_say() {
        let a = Agents::default();
        let chats = LaunchPick::PROVIDERS.map(|provider| {
            let mut spec = json!({"provider": provider, "prompt": "fix the build"});
            a.launch(&mut spec, &agents::Agents::default(), &[]);
            (spec["chat"] == true, a.starts_as_chat(provider))
        });
        assert_eq!(chats, [(false, false), (false, false), (true, true), (true, true), (true, true), (true, true), (true, true), (true, true), (true, true), (true, true)]);
        let mut tui = json!({"provider": "hermes", "chat": false});
        a.launch(&mut tui, &agents::Agents::default(), &[]);
        assert_eq!(tui["chat"], true);
        assert!(Agents { start_as_chat: true, ..Agents::default() }.starts_as_chat("claude"));
    }

    #[test]
    fn a_starred_model_comes_off_when_starred_again_and_others_keep_their_place() {
        let mut agents = Agents::default();
        agents.toggle_favorite("claude", "opus");
        agents.toggle_favorite("codex", "opus");
        agents.toggle_favorite("claude", "haiku");
        assert!(agents.is_favorite("codex", "opus"));
        agents.toggle_favorite("claude", "opus");
        assert!(!agents.is_favorite("claude", "opus"));
        assert_eq!(agents.favorites, [("codex".to_string(), "opus".to_string()), ("claude".to_string(), "haiku".to_string())]);
    }

    #[test]
    fn favorites_survive_a_save_and_a_file_from_before_them_has_none() {
        let mut agents = Agents::default();
        agents.toggle_favorite("claude", "opus");
        let saved: Agents = serde_json::from_str(&serde_json::to_string(&agents).unwrap()).unwrap();
        assert!(saved.is_favorite("claude", "opus"));
        agents.toggle_favorite("opencode", "opencode/big-pickle");
        assert_eq!(agents.favorites[1], ("opencode".to_string(), "opencode/big-pickle".to_string()));
        let old: Agents = serde_json::from_str("{}").unwrap();
        assert!(old.favorites.is_empty());
    }

    #[test]
    fn a_file_without_agent_settings_keeps_todays_behaviour() {
        let a: Agents = serde_json::from_str("{}").unwrap();
        assert_eq!((a.access(), a.plan, a.new_worktree, a.tabs_open_agent, a.send_now), ("settings", false, false, false, false));
        assert_eq!((a.chat_to(), a.close_on_exit, a.start_as_chat, a.provider("claude")), ("focused", false, false, Provider::default()));
    }

    #[test]
    fn an_unknown_add_to_chat_target_sends_to_the_focused_session() {
        let a = Agents { chat_to: "nowhere".into(), ..Agents::default() };
        assert_eq!(a.chat_to(), "focused");
    }
}
