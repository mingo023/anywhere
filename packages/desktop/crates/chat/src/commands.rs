use crate::Backend;

/// What the chat does with a slash command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Sent to the agent as a prompt.
    Send,
    /// Run by the desktop itself.
    Local(Local),
    /// Needs the agent's terminal, which the chat can't show.
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Local {
    Model,
    Effort,
    Compact,
    New,
    /// A side question, answered beside the conversation.
    Btw,
    /// Picks an MCP server for the draft to tag.
    Mcp,
}

/// A command an agent offers before its CLI lists any.
pub struct Builtin {
    pub name: &'static str,
    pub hint: &'static str,
    pub description: &'static str,
}

/// A row of the `/` menu.
#[derive(Clone, Debug, PartialEq)]
pub struct Slash {
    pub name: String,
    pub hint: String,
    pub description: String,
    pub kind: Kind,
}

/// The command `text` runs and its argument; none when it is a prompt, as one that starts with a path.
pub fn slash(text: &str) -> Option<(&str, &str)> {
    let rest = text.trim_start().strip_prefix('/')?;
    let (name, arg) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    (!name.is_empty() && !name.contains(['/', '.'])).then(|| (name, arg.trim()))
}

/// Whether `text` goes at once rather than into the queue: a goal can keep the turn it would wait out going for long.
pub fn skips_queue(text: &str) -> bool {
    slash(text) == Some(("goal", "clear"))
}

/// The commands whose name holds `query`, those it starts first: `b`'s own, then what its CLI `listed`, without what
/// the chat can't run.
pub fn menu<B: Backend + ?Sized>(b: &B, listed: &[agents::Command], query: &str) -> Vec<Slash> {
    let query = query.to_lowercase();
    let own = b.builtins().iter().map(|c| (c.name, c.hint, c.description));
    let theirs = listed.iter().map(|c| (c.name.as_str(), c.argument_hint.as_str(), c.description.as_str()));
    let mut rows: Vec<Slash> = Vec::new();
    for (name, hint, description) in own.chain(theirs) {
        if !name.to_lowercase().contains(&query) || rows.iter().any(|r| r.name == name) {
            continue;
        }
        let kind = b.command(name, "");
        if kind != Kind::Unsupported {
            rows.push(Slash { name: name.into(), hint: hint.into(), description: description.into(), kind });
        }
    }
    rows.sort_by_key(|r| !r.name.to_lowercase().starts_with(&query));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::for_provider;

    fn listed(names: &[&str]) -> Vec<agents::Command> {
        names.iter().map(|n| agents::Command { name: (*n).into(), ..Default::default() }).collect()
    }

    fn names(rows: &[Slash]) -> Vec<&str> {
        rows.iter().map(|r| r.name.as_str()).collect()
    }

    #[test]
    fn a_slash_command_is_its_name_and_argument() {
        assert_eq!(slash("  /model haiku \n"), Some(("model", "haiku")));
        assert_eq!(slash("/compact"), Some(("compact", "")));
        assert_eq!(slash("/goal tests pass\nand lint"), Some(("goal", "tests pass\nand lint")));
    }

    #[test]
    fn a_prompt_that_starts_with_a_path_is_no_command() {
        assert_eq!(slash("/usage/report.md is empty"), None);
        assert_eq!(slash("/etc.conf"), None);
        assert_eq!(slash("/"), None);
        assert_eq!(slash("why /model?"), None);
    }

    #[test]
    fn only_clearing_the_goal_skips_the_queue() {
        assert!(skips_queue("/goal clear"));
        assert!(skips_queue("  /goal  clear "));
        assert!(!skips_queue("/goal clear the tests"));
        assert!(!skips_queue("/goal pass the tests"));
        assert!(!skips_queue("goal clear"));
    }

    #[test]
    fn claude_runs_pickers_and_compact_itself_and_leaves_panels_out() {
        let claude = for_provider("claude");
        assert_eq!(claude.command("model", ""), Kind::Local(Local::Model));
        assert_eq!(claude.command("model", "haiku"), Kind::Local(Local::Model));
        assert_eq!(claude.command("compact", ""), Kind::Local(Local::Compact));
        assert_eq!(claude.command("btw", "why?"), Kind::Local(Local::Btw));
        assert_eq!(claude.command("mcp", ""), Kind::Local(Local::Mcp));
        assert_eq!(claude.command("compact", "keep the plan"), Kind::Send, "an instruction only Claude can take");
        assert_eq!(claude.command("clear", ""), Kind::Unsupported);
        assert_eq!(claude.command("cost", ""), Kind::Unsupported);
        assert_eq!(claude.command("resume", ""), Kind::Unsupported);
        assert_eq!(claude.command("add-dir", ""), Kind::Unsupported);
        assert_eq!(claude.command("add-dir", "../other"), Kind::Send);
        assert_eq!(claude.command("goal", "tests pass"), Kind::Send);
    }

    #[test]
    fn codex_sends_goal_and_skills_and_leaves_its_tui_commands_out() {
        let codex = for_provider("codex");
        assert_eq!(codex.command("goal", "pause"), Kind::Send);
        assert_eq!(codex.command("my-skill", ""), Kind::Send);
        assert_eq!(codex.command("new", ""), Kind::Local(Local::New));
        assert_eq!(codex.command("clear", ""), Kind::Unsupported);
        assert_eq!(codex.command("effort", "high"), Kind::Local(Local::Effort));
        assert_eq!(codex.command("review", ""), Kind::Unsupported);
        assert_eq!(codex.command("mcp", ""), Kind::Local(Local::Mcp));
    }

    #[test]
    fn an_acp_agent_runs_the_pickers_itself_and_sends_the_commands_it_lists() {
        for provider in ["cursor", "grok", "opencode", "fx", "hermes", "antigravity"] {
            let acp = for_provider(provider);
            assert_eq!((acp.command("model", ""), acp.command("effort", "high"), acp.command("new", "")), (Kind::Local(Local::Model), Kind::Local(Local::Effort), Kind::Local(Local::New)), "{provider}");
            assert_eq!((acp.command("compress", ""), acp.command("mcp", "")), (Kind::Send, Kind::Send), "{provider}");
        }
        let rows = menu(for_provider("hermes"), &listed(&["help", "model", "compress"]), "");
        assert_eq!(names(&rows), ["model", "new", "help", "compress"]);
    }

    #[test]
    fn opencode_and_hermes_leave_out_the_commands_only_their_terminals_run() {
        let kinds = |provider: &str, commands: [&str; 5]| commands.map(|c| for_provider(provider).command(c, ""));
        let terminal = [Kind::Unsupported; 5];
        assert_eq!(kinds("opencode", ["themes", "sessions", "clear", "undo", "export"]), terminal);
        assert_eq!(kinds("hermes", ["skin", "clear", "quit", "snap", "toolsets"]), terminal);
        assert_eq!(kinds("opencode", ["init", "review", "skin", "snap", "toolsets"]), [Kind::Send; 5], "OpenCode's ACP server lists these, and Hermes's list is not its own");
        assert_eq!(kinds("hermes", ["tools", "compress", "help", "sessions", "undo"]), [Kind::Send; 5]);
        for provider in ["cursor", "grok", "fx", "antigravity"] {
            assert_eq!(kinds(provider, ["themes", "sessions", "clear", "skin", "quit"]), [Kind::Send; 5], "{provider}");
        }
        let rows = menu(for_provider("opencode"), &listed(&["init", "sessions", "review"]), "");
        assert_eq!(names(&rows), ["model", "compact", "new", "init", "review"]);
    }

    #[test]
    fn grok_and_opencode_compact_in_the_chat_and_the_other_acp_agents_send_it() {
        for provider in ["grok", "opencode"] {
            assert_eq!(for_provider(provider).command("compact", ""), Kind::Local(Local::Compact), "{provider}");
        }
        for provider in ["cursor", "fx", "hermes", "antigravity"] {
            assert_eq!(for_provider(provider).command("compact", ""), Kind::Send, "{provider}");
        }
        let rows = menu(for_provider("grok"), &listed(&["compact", "context"]), "");
        assert_eq!(names(&rows), ["model", "compact", "new", "context"]);
    }

    #[test]
    fn pi_and_omp_run_thinking_and_compact_themselves() {
        for provider in ["pi", "omp"] {
            let pi = for_provider(provider);
            assert_eq!((pi.command("thinking", "high"), pi.command("effort", "")), (Kind::Local(Local::Effort), Kind::Local(Local::Effort)), "{provider}");
            assert_eq!((pi.command("compact", "keep the plan"), pi.command("model", "")), (Kind::Local(Local::Compact), Kind::Local(Local::Model)), "{provider}");
            assert_eq!(pi.command("skill:review", ""), Kind::Send, "{provider}");
        }
    }

    #[test]
    fn pi_leaves_out_the_commands_only_its_tui_runs_and_omp_sends_what_it_lists() {
        let tui = ["tree", "login", "settings", "llama", "quit"];
        assert_eq!(tui.map(|c| for_provider("pi").command(c, "")), [Kind::Unsupported; 5]);
        assert_eq!(tui.map(|c| for_provider("omp").command(c, "")), [Kind::Send; 5]);
        let rows = menu(for_provider("pi"), &listed(&["skill:review", "settings", "compact"]), "");
        assert_eq!(names(&rows), ["model", "thinking", "compact", "new", "skill:review"]);
        let rows = menu(for_provider("omp"), &listed(&["tree", "settings", "compact"]), "");
        assert_eq!(names(&rows), ["model", "thinking", "compact", "new", "tree", "settings"]);
    }

    #[test]
    fn an_unknown_provider_sends_every_command() {
        assert_eq!(for_provider("gemini").command("cost", ""), Kind::Send);
    }

    #[test]
    fn the_menu_puts_the_agents_own_first_then_its_list_without_panels_or_twins() {
        let rows = menu(for_provider("claude"), &listed(&["goal", "cost", "clear", "review"]), "");
        assert_eq!(names(&rows), ["model", "effort", "compact", "btw", "goal", "mcp", "review"]);
        assert_eq!(rows[0].kind, Kind::Local(Local::Model));
    }

    #[test]
    fn the_menu_keeps_names_that_hold_the_query_those_it_starts_first() {
        let rows = menu(for_provider("codex"), &listed(&["release-notes", "Rebase"]), "re");
        assert_eq!(names(&rows), ["release-notes", "Rebase"]);
        let rows = menu(for_provider("codex"), &listed(&["write-note"]), "e");
        assert_eq!(names(&rows), ["effort", "model", "new", "write-note"]);
    }
}
