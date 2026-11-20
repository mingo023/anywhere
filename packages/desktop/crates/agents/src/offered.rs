use serde::Deserialize;

/// A slash command the agent's CLI offers, from `agent.commands`.
#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Command {
    pub name: String,
    pub description: String,
    pub argument_hint: String,
}

/// An MCP server the agent's session has, from `agent.mcp`.
#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct McpServer {
    pub name: String,
    pub status: McpStatus,
    /// Where it is configured, as `user` or `project`; empty when the agent doesn't say.
    pub scope: String,
}

#[derive(Deserialize, Default, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum McpStatus {
    Connected,
    NeedsAuth,
    Failed,
    Disabled,
    /// Still connecting, or a status a newer pocketd added.
    #[default]
    #[serde(other)]
    Pending,
}

/// What `agent.mcp` may do to a server before it lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpAction {
    /// Starts a sign-in the user finishes in a browser.
    Auth,
    Reconnect,
    Enable,
}

impl McpAction {
    pub(super) fn wire(self) -> &'static str {
        match self {
            McpAction::Auth => "auth",
            McpAction::Reconnect => "reconnect",
            McpAction::Enable => "enable",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_mcp_status_a_newer_pocketd_added_reads_as_pending() {
        let s: McpServer = serde_json::from_str(r#"{"name":"x","status":"warming"}"#).unwrap();
        assert_eq!(s.status, McpStatus::Pending);
        let s: McpServer = serde_json::from_str(r#"{"name":"x","status":"pending"}"#).unwrap();
        assert_eq!(s.status, McpStatus::Pending);
    }
}
