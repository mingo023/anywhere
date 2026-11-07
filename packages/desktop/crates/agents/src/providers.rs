use std::sync::LazyLock;

use serde::Deserialize;

/// What pocketd says one provider takes, from `agent.providers`.
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: String,
    pub efforts: Vec<String>,
    pub plan: bool,
    /// `None` from a pocketd older than the field.
    pub chat: Option<ChatCaps>,
}

/// What a provider's chat driver does, as pocketd says.
#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct ChatCaps {
    pub interrupt: bool,
    pub compact: bool,
    pub send_while_working: bool,
    pub sub_agents: bool,
    pub send_now: bool,
}

/// How a provider's CLI lists its models.
pub struct Catalog {
    /// Run after the provider's command.
    pub args: &'static [&'static str],
    pub input: &'static str,
    pub parse: fn(&str) -> Vec<crate::ListedModel>,
    /// Offered, as they are, until the CLI answers or when it can't.
    pub fallback: &'static [&'static str],
}

/// How the AI commit message runs the agent once.
pub struct Oneshot {
    pub args: &'static [&'static str],
    pub model_flag: &'static str,
    pub model: &'static str,
}

/// What this build knows of a provider without pocketd.
pub struct Builtin {
    pub id: &'static str,
    pub efforts: &'static [&'static str],
    /// What the agent calls its effort.
    pub effort_word: &'static str,
    pub plan: bool,
    pub models: Catalog,
    pub resume_args: &'static str,
    pub fork_args: &'static str,
    pub status_via: &'static str,
    pub unattached: &'static str,
    pub pastes_code: bool,
    pub oneshot: Oneshot,
}

pub const ALL: [Builtin; 2] = [
    Builtin {
        id: "claude",
        efforts: &["low", "medium", "high", "xhigh", "max"],
        effort_word: "Effort",
        plan: true,
        // Asks without a prompt, so it costs no tokens; no hooks, so the user's don't take it for a session.
        models: Catalog {
            args: &["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--no-session-persistence", "--settings", r#"{"disableAllHooks":true}"#],
            input: r#"{"type":"control_request","request_id":"models","request":{"subtype":"initialize"}}"#,
            parse: crate::claude_catalog,
            fallback: &["haiku", "sonnet", "opus"],
        },
        resume_args: "--resume",
        fork_args: "--resume {sessionId} --fork-session",
        status_via: "uses Claude Code hooks",
        unattached: "Claude skips hooks in folders it doesn't trust. Trust this folder in Claude to see status.",
        pastes_code: true,
        oneshot: Oneshot { args: &["-p"], model_flag: "--model", model: "haiku" },
    },
    Builtin {
        id: "codex",
        efforts: &[],
        effort_word: "Reasoning",
        plan: false,
        models: Catalog { args: &["debug", "models"], input: "", parse: crate::codex_catalog, fallback: &[] },
        resume_args: "resume",
        fork_args: "fork {sessionId}",
        status_via: "reads Codex session events",
        unattached: "This codex runs without the app-server, so Anywhere can't see its status.",
        pastes_code: false,
        oneshot: Oneshot { args: &["exec", "-s", "read-only", "--skip-git-repo-check", "--color", "never"], model_flag: "-m", model: "" },
    },
];

pub const COUNT: usize = ALL.len();

pub const IDS: [&str; COUNT] = {
    let mut ids = [""; COUNT];
    let mut i = 0;
    while i < COUNT {
        ids[i] = ALL[i].id;
        i += 1;
    }
    ids
};

pub fn get(id: &str) -> Option<&'static Builtin> {
    ALL.iter().find(|b| b.id == id)
}

/// The entry for `id`, else Claude's: a pick naming no known agent starts Claude.
pub fn known(id: &str) -> &'static Builtin {
    get(id).unwrap_or(&ALL[0])
}

static BUILTIN: LazyLock<Vec<ProviderInfo>> = LazyLock::new(|| {
    ALL.iter().map(|b| ProviderInfo { id: b.id.into(), efforts: b.efforts.iter().map(|e| e.to_string()).collect(), plan: b.plan, chat: None }).collect()
});
static OTHER: LazyLock<ProviderInfo> = LazyLock::new(ProviderInfo::default);

/// What this build knows of `id` before or without pocketd's reply.
pub fn builtin(id: &str) -> &'static ProviderInfo {
    BUILTIN.iter().find(|p| p.id == id).unwrap_or(&OTHER)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_claude_plans_and_takes_efforts_before_pocketd_says() {
        let claude = builtin("claude");
        assert_eq!((&claude.efforts[..], claude.plan, claude.chat), (&["low", "medium", "high", "xhigh", "max"].map(String::from)[..], true, None));
        for id in ["codex", "gemini"] {
            assert_eq!((builtin(id).efforts.len(), builtin(id).plan, builtin(id).chat), (0, false, None));
        }
    }

    #[test]
    fn an_older_pocketd_s_providers_have_no_chat() {
        let info: ProviderInfo = serde_json::from_str(r#"{"id":"claude","available":true,"efforts":["low"],"plan":true}"#).unwrap();
        assert_eq!((info.efforts, info.plan, info.chat), (vec!["low".to_string()], true, None));
    }

    #[test]
    fn claude_comes_first_so_an_unknown_pick_starts_it() {
        assert_eq!(known("gemini").id, "claude");
        assert_eq!(IDS, ["claude", "codex"]);
    }

    #[test]
    fn default_args_match_pocketd() {
        let args = |id| (known(id).resume_args, known(id).fork_args);
        assert_eq!(args("claude"), ("--resume", "--resume {sessionId} --fork-session"));
        assert_eq!(args("codex"), ("resume", "fork {sessionId}"));
    }
}
