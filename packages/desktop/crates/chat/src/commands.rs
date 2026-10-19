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
    }

    #[test]
    fn an_unknown_provider_sends_every_command() {
        assert_eq!(for_provider("gemini").command("cost", ""), Kind::Send);
    }

    #[test]
    fn the_menu_puts_the_agents_own_first_then_its_list_without_panels_or_twins() {
        let rows = menu(for_provider("claude"), &listed(&["goal", "cost", "clear", "review"]), "");
        assert_eq!(names(&rows), ["model", "effort", "compact", "goal", "review"]);
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
