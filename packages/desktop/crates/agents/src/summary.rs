use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub terminal_id: String,
    pub title: String,
    pub cwd: String,
    pub provider: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    /// What it may do without asking: ask, edits, auto or full; "settings" leaves that to its own settings.
    pub access: Option<String>,
    pub status: String,
    pub failed: bool,
    pub attached: bool,
    pub restore: String,
    pub compacting: bool,
    pub pinned: bool,
    pub provider_session_id: Option<String>,
    pub tokens_used: u64,
    pub context_window: Option<u64>,
    pub activity: String,
    pub created_at: i64,
    pub updated_at: i64,
    /// The account it runs on; empty is the built-in login.
    pub account: String,
    /// The account it moves to once its turn ends, `builtin` for the built-in login; empty when it stays.
    pub switching: String,
    /// Bumped each time pocketd starts the agent's timeline over, as for a new conversation.
    pub resets: u64,
    /// Run without its TUI, so only the chat drives it.
    pub chat_only: bool,
    /// What `/goal` set it working toward; none when no goal is set.
    pub goal: Option<Goal>,
    /// The model settings it runs on, by the agent's id for each.
    pub options: HashMap<String, String>,
    /// The session it was delegated or forked from; empty for one the user started.
    pub parent: String,
    /// `subagent` or `fork`, when `parent` is set.
    pub relationship: String,
}

/// An agent's `/goal`.
#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Goal {
    pub objective: String,
    pub status: GoalStatus,
    pub tokens_used: u64,
    pub token_budget: u64,
}

/// Codex's goal statuses, and `Failed` from Claude's judge.
#[derive(Deserialize, Default, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum GoalStatus {
    #[default]
    Active,
    Paused,
    /// Can't get further on its own.
    Blocked,
    UsageLimited,
    BudgetLimited,
    Complete,
    Failed,
    /// A status a newer pocketd added.
    #[serde(other)]
    Unknown,
}

impl Summary {
    /// The account to show it on: the one it switches to, else the one it runs on; empty is the built-in login.
    pub fn shown_account(&self) -> &str {
        match self.switching.as_str() {
            "" => &self.account,
            "builtin" => "",
            to => to,
        }
    }

    /// Tokens used and the model's window, once pocketd reports a non-zero window.
    pub fn context(&self) -> Option<(u64, u64)> {
        self.context_window.filter(|&w| w > 0).map(|w| (self.tokens_used, w))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Level {
    Low,
    Warn,
    Danger,
}

/// The whole percent of `window` that `used` fills, capped at 100; an empty window reads as 0.
pub fn percent(used: u64, window: u64) -> u64 {
    used.saturating_mul(100).checked_div(window).unwrap_or(0).min(100)
}

/// Judged on the whole percent against `warn` and `critical`, so 74.99% is still Low at 75.
pub fn level(used: u64, window: u64, warn: u32, critical: u32) -> Level {
    match percent(used, window) {
        p if p >= u64::from(critical) => Level::Danger,
        p if p >= u64::from(warn) => Level::Warn,
        _ => Level::Low,
    }
}

/// "claude-opus-5-5" reads as "Opus 5.5"; a missing model falls back to the provider.
pub fn model_label(a: &Summary) -> String {
    let Some(model) = a.model.as_deref().filter(|m| !m.is_empty()) else {
        let mut p = a.provider.chars();
        return p.next().map(|c| c.to_uppercase().chain(p).collect()).unwrap_or_default();
    };
    model_name(model)
}

/// "claude-haiku-4-5-20251001" reads as "Haiku 4.5", "haiku" as "Haiku", "openai-codex/gpt-5.4" as "GPT 5.4",
/// "opencode/big-pickle" as "Big Pickle" and "us.anthropic.claude-opus-5-5" as "Opus 5.5": the id without who serves
/// it, its date or "claude-", numbers joined by a dot. Claude's "[1m]" for its 1M-context window stays as written.
pub fn model_name(model: &str) -> String {
    let number = |w: &str| !w.is_empty() && w.bytes().all(|b| b.is_ascii_digit());
    let id = model.rsplit('/').next().unwrap_or(model);
    // Hermes writes "provider:model"; Bedrock ends an id in ":0".
    let id = id.rsplit_once(':').filter(|(_, m)| !number(m)).map_or(id, |(_, m)| m);
    let (id, window) = id.find('[').map_or((id, ""), |at| id.split_at(at));
    // Bedrock leads with where and who, as "us.anthropic."; the dot of a version, as "v3.2", has no two letters after it.
    let id = id.rmatch_indices('.').map(|(at, _)| &id[at + 1..]).find(|rest| rest.len() > 1 && rest.bytes().take(2).all(|b| b.is_ascii_alphabetic())).unwrap_or(id);
    let mut name = String::new();
    let mut after_number = false;
    for word in id.strip_prefix("claude-").unwrap_or(id).split('-').filter(|w| !w.is_empty() && !(w.len() >= 8 && number(w))) {
        if !name.is_empty() {
            name.push(if after_number && number(word) { '.' } else { ' ' });
        }
        // A word without a vowel is initials, as "gpt".
        if word.contains(['a', 'e', 'i', 'o', 'u', 'y']) {
            let mut c = word.chars();
            name.extend(c.next().into_iter().flat_map(char::to_uppercase).chain(c));
        } else {
            name.push_str(&word.to_uppercase());
        }
        after_number = number(word);
    }
    name + window
}

/// One model a provider's CLI lists.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListedModel {
    /// What the agent is started or switched with, as "opus".
    pub value: String,
    pub name: String,
    /// The id the agent reports running it as, as "claude-opus-5-5".
    pub resolved: String,
    pub efforts: Vec<String>,
    /// The effort it runs on when none is picked; "" when the CLI doesn't say.
    pub default_effort: String,
    /// Who serves it, as the agent names them: "amazon-bedrock" or "ChatGPT or Codex Subscription". It tells apart two
    /// models of one name; "" for an agent's own.
    pub upstream: String,
    pub settings: Vec<ModelSetting>,
}

/// One of a model's own settings besides its effort, as Cursor's "Fast" or OpenCode's "Agent".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelSetting {
    /// What the agent calls it; a launch's `options` set it by this.
    pub id: String,
    pub name: String,
    /// Each value it takes, and what a menu calls that.
    pub choices: Vec<(String, String)>,
    /// The value it's on when none is picked.
    pub default: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Frame;
    use serde_json::{Value, json};

    fn summary(provider: &str, model: Option<&str>) -> Summary {
        Summary { provider: provider.into(), model: model.map(Into::into), ..Default::default() }
    }

    #[test]
    fn labels_models_like_the_picker() {
        assert_eq!(model_label(&summary("claude", Some("claude-opus-5"))), "Opus 5");
        assert_eq!(model_label(&summary("claude", Some("claude-opus-5-5"))), "Opus 5.5");
        assert_eq!(model_label(&summary("claude", Some("claude-haiku-4-5-20251001"))), "Haiku 4.5");
        assert_eq!(model_label(&summary("codex", None)), "Codex");
    }

    #[test]
    fn a_model_reads_without_who_serves_it_its_date_or_its_dashes() {
        let names = ["openai-codex/gpt-5.4", "opencode/big-pickle", "anthropic/claude-sonnet-4-5", "openai-codex:gpt-6-sol", "zai/glm-5.2-fast", "composer-2.5", "gemini-3.8-flash-high", "claude-3-5-sonnet-20241022", "haiku", ""];
        assert_eq!(names.map(model_name), ["GPT 5.4", "Big Pickle", "Sonnet 4.5", "GPT 6 Sol", "GLM 5.2 Fast", "Composer 2.5", "Gemini 3.8 Flash High", "3.5 Sonnet", "Haiku", ""]);
        assert_eq!(model_label(&summary("pi", Some("deepseek/deepseek-v4-pro"))), "Deepseek V4 Pro");
    }

    #[test]
    fn claude_s_1m_context_mark_stays_as_written_after_the_name() {
        let names = ["claude-opus-4-7[1m]", "sonnet[1m]", "claude-haiku-4-5-20251001[1m]", "[1m]"];
        assert_eq!(names.map(model_name), ["Opus 4.7[1m]", "Sonnet[1m]", "Haiku 4.5[1m]", "[1m]"]);
    }

    #[test]
    fn a_bedrock_id_reads_without_where_and_who_serve_it() {
        let names = [
            "amazon-bedrock/us.anthropic.claude-opus-5-5",
            "us.anthropic.claude-sonnet-4-5-20250929-v1:0",
            "amazon-bedrock/anthropic.claude-haiku-4-5-20251001-v1:0",
            "amazon-bedrock/amazon.nova-lite-v1:0",
            "amazon-bedrock/us-gov.openai.gpt-oss-120b-1:0",
            "amazon-bedrock/global.openai.gpt-6.1-sol",
            "amazon-bedrock/moonshotai.kimi-k2.5",
            "amazon-bedrock/qwen.qwen3-32b-v1:0",
        ];
        assert_eq!(names.map(model_name), ["Opus 5.5", "Sonnet 4.5 V1:0", "Haiku 4.5 V1:0", "Nova Lite V1:0", "GPT Oss 120B 1:0", "GPT 6.1 Sol", "Kimi K2.5", "Qwen3 32B V1:0"]);
    }

    #[test]
    fn a_version_s_dot_is_no_vendor_prefix() {
        let names = ["gpt-5.4", "qwen3.5-coder", "MiniMax-M2.5", "amazon-bedrock/deepseek.v3.2", "opencode/mimo-v2.6-flash-free", "a.b"];
        assert_eq!(names.map(model_name), ["GPT 5.4", "Qwen3.5 Coder", "MiniMax M2.5", "Deepseek.v3.2", "Mimo V2.6 Flash Free", "A.b"]);
    }

    #[test]
    fn an_agent_summary_without_activity_reads_it_as_empty() {
        let s: Summary = serde_json::from_value(json!({ "id": "a", "status": "working" })).unwrap();
        assert_eq!(s.activity, "");
    }

    #[test]
    fn an_agent_summary_carries_its_activity() {
        let s: Summary = serde_json::from_value(json!({ "id": "a", "activity": "Running tests" })).unwrap();
        assert_eq!(s.activity, "Running tests");
    }

    #[test]
    fn decodes_a_v3_summary() {
        let f: Frame = serde_json::from_str(include_str!("../../../../pocketd/internal/proto/testdata/golden/server/agent_update_failed.json")).unwrap();
        let a = f.agent.unwrap();
        assert_eq!((a.terminal_id.as_str(), a.status.as_str(), a.failed, a.attached, a.compacting), ("t1", "done", true, true, false));
    }

    #[test]
    fn a_summary_names_the_agents_access() {
        let f: Frame = serde_json::from_str(include_str!("../../../../pocketd/internal/proto/testdata/golden/server/agent_update_model.json")).unwrap();
        assert_eq!(f.agent.unwrap().access.as_deref(), Some("edits"));
    }

    #[test]
    fn a_summary_shows_the_account_it_switches_to() {
        let f: Frame = serde_json::from_str(include_str!("../../../../pocketd/internal/proto/testdata/golden/server/agent_update_switching.json")).unwrap();
        let a = f.agent.unwrap();
        assert_eq!((a.account.as_str(), a.shown_account()), ("3f9a0c1b2d4e", ""));
        let staying = Summary { account: "work".into(), ..Default::default() };
        assert_eq!(staying.shown_account(), "work");
        let moving = Summary { switching: "home".into(), ..staying };
        assert_eq!(moving.shown_account(), "home");
    }

    #[test]
    fn summary_reads_the_provider_session_id() {
        let a: Summary = serde_json::from_str(r#"{"id":"a","providerSessionId":"s-1"}"#).unwrap();
        assert_eq!(a.provider_session_id.as_deref(), Some("s-1"));
        let fresh: Summary = serde_json::from_str(r#"{"id":"a"}"#).unwrap();
        assert_eq!(fresh.provider_session_id, None);
    }

    #[test]
    fn level_is_low_at_74_warn_at_75_and_89_danger_at_90() {
        let at = |used| level(used, 200_000, 75, 90);
        assert_eq!([at(148_000), at(149_998), at(150_000), at(179_998), at(180_000), at(400_000)], [Level::Low, Level::Low, Level::Warn, Level::Warn, Level::Danger, Level::Danger]);
    }

    #[test]
    fn level_follows_the_thresholds_it_is_given() {
        let at = |used| level(used, 100, 50, 60);
        assert_eq!([at(49), at(50), at(59), at(60)], [Level::Low, Level::Warn, Level::Warn, Level::Danger]);
    }

    #[test]
    fn an_empty_window_is_zero_percent_full() {
        assert_eq!((percent(184_000, 0), level(184_000, 0, 75, 90)), (0, Level::Low));
    }

    #[test]
    fn context_is_unknown_without_a_window() {
        let a = |window| Summary { tokens_used: 184_000, context_window: window, ..Default::default() };
        assert_eq!(a(None).context(), None);
        assert_eq!(a(Some(0)).context(), None);
        assert_eq!(a(Some(200_000)).context(), Some((184_000, 200_000)));
    }

    #[test]
    fn summary_reads_tokens_used_and_context_window() {
        let a: Summary = serde_json::from_str(r#"{"id":"a","tokensUsed":184000,"contextWindow":200000}"#).unwrap();
        assert_eq!((a.tokens_used, a.context_window), (184_000, Some(200_000)));
    }

    #[test]
    fn a_summary_without_restore_decodes() {
        let f: Frame = serde_json::from_str(include_str!("../../../../pocketd/internal/proto/testdata/golden/server/agent_update_restore_cleared.json")).unwrap();
        assert_eq!(f.agent.unwrap().restore, "");
        let a: Summary = serde_json::from_value(json!({"id": "a1", "restore": "interrupted"})).unwrap();
        assert_eq!(a.restore, "interrupted");
    }

    #[test]
    fn the_golden_goal_rides_on_its_agent() {
        let f: Frame = serde_json::from_value(golden!("server/agent_update_goal.json")).unwrap();
        let goal = Goal { objective: "tests pass".into(), status: GoalStatus::Active, tokens_used: 16510, token_budget: 50000 };
        assert_eq!(f.agent.unwrap().goal, Some(goal));
    }

    #[test]
    fn the_golden_options_ride_on_their_agent() {
        let f: Frame = serde_json::from_value(golden!("server/agent_update_options.json")).unwrap();
        assert_eq!(f.agent.unwrap().options, HashMap::from([("fast".to_string(), "true".to_string())]));
    }
}
