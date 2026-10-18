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

static CLAUDE: LazyLock<ProviderInfo> = LazyLock::new(|| ProviderInfo {
    id: "claude".into(),
    efforts: ["low", "medium", "high", "xhigh", "max"].map(String::from).to_vec(),
    plan: true,
    chat: None,
});
static OTHER: LazyLock<ProviderInfo> = LazyLock::new(ProviderInfo::default);

/// What this build knows of `id` before or without pocketd's reply: only that Claude plans and takes efforts.
pub fn builtin(id: &str) -> &'static ProviderInfo {
    if id == "claude" { &CLAUDE } else { &OTHER }
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
}
