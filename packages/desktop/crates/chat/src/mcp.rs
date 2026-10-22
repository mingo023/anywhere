use agents::{McpAction, McpServer, McpStatus};
use std::collections::BTreeMap;

/// What a picked server is written as in the draft, before its name.
pub const TAG: &str = "@mcp/";

/// Whether the agent can use `s` now: it is connected, or still connecting.
pub fn usable(s: &McpServer) -> bool {
    matches!(s.status, McpStatus::Connected | McpStatus::Pending)
}

/// What picking `s` does when the agent can't use it now: starts its sign-in, afresh when one started already, as its
/// browser tab may be gone; reconnects a failed one; enables a disabled one where `provider` can.
pub fn action(provider: &str, s: &McpServer) -> Option<McpAction> {
    match s.status {
        McpStatus::NeedsAuth => Some(McpAction::Auth),
        McpStatus::Failed => Some(McpAction::Reconnect),
        McpStatus::Disabled if provider == "claude" => Some(McpAction::Enable),
        _ => None,
    }
}

/// The `servers` whose name or scope holds `query`, each by name: the usable first, then those waiting on a sign-in,
/// the failed and the disabled. So what a pick can take, a tag or an action, leads.
pub fn picker(servers: &[McpServer], query: &str) -> Vec<McpServer> {
    let query = query.to_lowercase();
    let rank = |s: &McpServer| match s.status {
        _ if usable(s) => 0,
        McpStatus::NeedsAuth => 1,
        McpStatus::Failed => 2,
        _ => 3,
    };
    let mut rows: Vec<McpServer> = servers.iter().filter(|s| format!("{} {}", s.name, s.scope).to_lowercase().contains(&query)).cloned().collect();
    rows.sort_by_cached_key(|s| (rank(s), s.name.to_lowercase()));
    rows
}

/// The `servers` `text` tags, in the order it does. A tag starts a word and ends at a space or punctuation; where two
/// names start alike, the longer one that fits is the tag.
pub fn tagged<'a>(text: &str, servers: &'a [McpServer]) -> Vec<&'a McpServer> {
    let mut at: BTreeMap<usize, &McpServer> = BTreeMap::new();
    for s in servers {
        let tag = format!("{TAG}{}", s.name);
        for (i, _) in text.match_indices(&tag) {
            let starts = text[..i].chars().next_back().is_none_or(|c| c.is_whitespace() || "([{".contains(c));
            let ends = text[i + tag.len()..].chars().next().is_none_or(|c| c.is_whitespace() || ".,;:!?)]}".contains(c));
            if starts && ends && at.get(&i).is_none_or(|t| t.name.len() < s.name.len()) {
                at.insert(i, s);
            }
        }
    }
    let mut out: Vec<&McpServer> = Vec::new();
    for s in at.into_values() {
        if !out.iter().any(|t| t.name == s.name) {
            out.push(s);
        }
    }
    out
}

/// `text` as the agent gets it: led by a line asking it to use the servers it tags, as `provider` has them.
pub fn with_context(text: &str, servers: &[McpServer], provider: &str) -> String {
    let tagged = tagged(text, servers);
    if tagged.is_empty() {
        return text.to_string();
    }
    let names: Vec<String> = tagged.iter().map(|s| format!("{:?} ({provider})", s.name)).collect();
    let plural = if tagged.len() == 1 { "" } else { "s" };
    format!("MCP context: Use the configured server{plural} {} when relevant to this request.\n\n{text}", names.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(name: &str, status: McpStatus) -> McpServer {
        McpServer { name: name.into(), status, scope: "user".into() }
    }

    fn names(rows: &[McpServer]) -> Vec<&str> {
        rows.iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn the_picker_lists_usable_servers_first_then_those_waiting_on_a_sign_in() {
        let all = [
            server("zed", McpStatus::Failed),
            server("linear", McpStatus::NeedsAuth),
            server("fff", McpStatus::Connected),
            server("Docs", McpStatus::Pending),
            server("old", McpStatus::Disabled),
        ];
        assert_eq!(names(&picker(&all, "")), ["Docs", "fff", "linear", "zed", "old"]);
        assert_eq!(names(&picker(&all, "LIN")), ["linear"]);
        assert_eq!(picker(&all, "user").len(), 5, "the scope matches too");
    }

    #[test]
    fn only_a_connected_or_connecting_server_is_usable() {
        assert!(usable(&server("a", McpStatus::Connected)));
        assert!(usable(&server("a", McpStatus::Pending)));
        assert!(!usable(&server("a", McpStatus::NeedsAuth)));
        assert!(!usable(&server("a", McpStatus::Failed)));
        assert!(!usable(&server("a", McpStatus::Disabled)));
    }

    #[test]
    fn a_server_the_agent_cant_use_is_signed_in_to_reconnected_or_enabled() {
        let auth = server("a", McpStatus::NeedsAuth);
        assert_eq!(action("codex", &auth), Some(McpAction::Auth));
        assert_eq!(action("claude", &server("a", McpStatus::Failed)), Some(McpAction::Reconnect));
        assert_eq!(action("claude", &server("a", McpStatus::Disabled)), Some(McpAction::Enable));
        assert_eq!(action("codex", &server("a", McpStatus::Disabled)), None, "codex can't enable one from the chat");
        assert_eq!(action("claude", &server("a", McpStatus::Connected)), None);
    }

    #[test]
    fn a_tag_is_a_whole_word_naming_a_known_server() {
        let all = [server("fff", McpStatus::Connected), server("docs", McpStatus::Connected)];
        assert_eq!(names(&tagged("use @mcp/docs, then @mcp/fff", &all).into_iter().cloned().collect::<Vec<_>>()), ["docs", "fff"]);
        assert!(tagged("use @mcp/fffx", &all).is_empty(), "a longer word names another server");
        assert!(tagged("mail@mcp/fff", &all).is_empty(), "a tag starts a word");
        assert!(tagged("@mcp/other", &all).is_empty());
        assert_eq!(tagged("(@mcp/fff) and @mcp/fff", &all).len(), 1, "a server tagged twice counts once");
    }

    #[test]
    fn of_two_names_that_start_alike_the_longer_that_fits_is_the_tag() {
        let all = [server("claude.ai Atlassian", McpStatus::Connected), server("claude.ai Atlassian MCP", McpStatus::Connected)];
        let only = |text: &str| tagged(text, &all).iter().map(|s| s.name.clone()).collect::<Vec<_>>();
        assert_eq!(only("ask @mcp/claude.ai Atlassian MCP for it"), ["claude.ai Atlassian MCP"]);
        assert_eq!(only("ask @mcp/claude.ai Atlassian for it"), ["claude.ai Atlassian"]);
    }

    #[test]
    fn a_prompt_that_tags_servers_asks_the_agent_to_use_them() {
        let all = [server("fff", McpStatus::Connected), server("docs", McpStatus::Connected)];
        assert_eq!(
            with_context("find it with @mcp/fff", &all, "codex"),
            "MCP context: Use the configured server \"fff\" (codex) when relevant to this request.\n\nfind it with @mcp/fff"
        );
        assert!(with_context("@mcp/docs and @mcp/fff", &all, "claude").starts_with("MCP context: Use the configured servers \"docs\" (claude), \"fff\" (claude) when"));
        assert_eq!(with_context("no tags here", &all, "codex"), "no tags here");
    }
}
