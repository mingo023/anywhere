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
}

/// How a provider's models are offered.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Models {
    /// Offered as they are; the agent reports the full id, as "claude-opus-5-5" for "opus".
    Aliases(&'static [&'static str]),
    /// What `<command> debug models` lists (`codex_models`), each with its own efforts.
    Catalog,
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
    pub plan: bool,
    pub models: Models,
    pub resume_args: &'static str,
    pub fork_args: &'static str,
    pub status_via: &'static str,
    pub unattached: &'static str,
    pub pastes_code: bool,
    pub terminal_prompts: bool,
    pub oneshot: Oneshot,
}

pub const ALL: [Builtin; 2] = [
    Builtin {
        id: "claude",
        efforts: &["low", "medium", "high", "xhigh", "max"],
        plan: true,
        models: Models::Aliases(&["haiku", "sonnet", "opus"]),
        resume_args: "--resume",
        fork_args: "--resume {sessionId} --fork-session",
        status_via: "uses Claude Code hooks",
        unattached: "Claude skips hooks in folders it doesn't trust. Trust this folder in Claude to see status.",
        pastes_code: true,
        terminal_prompts: true,
        oneshot: Oneshot { args: &["-p"], model_flag: "--model", model: "haiku" },
    },
    Builtin {
        id: "codex",
        efforts: &[],
        plan: false,
        models: Models::Catalog,
        resume_args: "resume",
        fork_args: "fork {sessionId}",
        status_via: "reads Codex session events",
        unattached: "This codex runs without the app-server, so Pocket can't see its status.",
        pastes_code: false,
        terminal_prompts: false,
        oneshot: Oneshot { args: &["exec", "-s", "read-only", "--skip-git-repo-check", "--color", "never"], model_flag: "-m", model: "" },
    },
];

impl Builtin {
    /// The model aliases it's offered with, if it takes aliases.
    pub fn aliases(&self) -> Option<&'static [&'static str]> {
        match self.models {
            Models::Aliases(a) => Some(a),
            Models::Catalog => None,
        }
    }
}

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
