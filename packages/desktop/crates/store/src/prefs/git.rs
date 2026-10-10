use serde::{Deserialize, Serialize};

/// How an AI-written commit message reads.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    /// Follows the repository's recent subjects.
    #[default]
    Recent,
    Conventional,
    Plain,
    Custom,
}

/// Git and GitHub.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(default)]
pub struct Git {
    /// Where pushes and pull requests go; git's own pick when the repo has no remote by this name.
    pub remote: String,
    pub untracked: bool,
    /// With nothing staged, a commit takes every change instead of stopping.
    pub commit_all: bool,
    /// Off by default: on, every Push may replace a rebased or amended remote branch, though never over commits not yet fetched.
    pub force_with_lease: bool,
    pub ai: bool,
    pub agent: String,
    /// Empty runs the agent's own default.
    pub model: String,
    #[serde(deserialize_with = "crate::lenient")]
    pub style: Style,
    pub prompt: String,
    pub draft: bool,
    /// Opens pull requests in a browser tab instead of the system browser.
    pub in_app: bool,
}

impl Default for Git {
    fn default() -> Self {
        Self {
            remote: "origin".into(),
            untracked: true,
            commit_all: true,
            force_with_lease: false,
            ai: true,
            agent: "claude".into(),
            model: "haiku".into(),
            style: Style::Recent,
            prompt: String::new(),
            draft: false,
            in_app: false,
        }
    }
}

const RECENT: &str = "Write a git commit message for the diff on stdin, in the style of the recent subjects. Reply with the message only.";
const CONVENTIONAL: &str = "Write a git commit message for the diff on stdin as a Conventional Commit, like `feat(scope): summary`. Reply with the message only.";
const PLAIN: &str = "Write a git commit message for the diff on stdin: one plain sentence saying what changed, with no prefix. Reply with the message only.";
const PR: &str = "Write a pull request title and description for the commits and diff on stdin. Reply with the title alone on the first line, a blank line, then the description in Markdown: what changed and why, briefly. Nothing else.";

impl Git {
    /// What the agent is asked; an empty custom prompt falls back to the recent subjects.
    pub fn message_prompt(&self) -> &str {
        match self.style {
            Style::Conventional => CONVENTIONAL,
            Style::Plain => PLAIN,
            Style::Custom if !self.prompt.trim().is_empty() => self.prompt.trim(),
            Style::Recent | Style::Custom => RECENT,
        }
    }

    /// Runs the picked agent once through `command`, reading the commit context on stdin and printing only the message.
    pub fn message_argv(&self, command: &str) -> Vec<String> {
        self.agent_argv(command, self.message_prompt())
    }

    /// Like [`Self::message_argv`], for a PR's title and description from `git::pr_context`.
    pub fn pr_argv(&self, command: &str) -> Vec<String> {
        self.agent_argv(command, PR)
    }

    fn agent_argv(&self, command: &str, prompt: &str) -> Vec<String> {
        let mut argv: Vec<String> = match self.agent.as_str() {
            "codex" => [command, "exec", "-s", "read-only", "--skip-git-repo-check", "--color", "never"].map(String::from).to_vec(),
            _ => vec![command.into(), "-p".into()],
        };
        if !self.model.is_empty() {
            let flag = if self.agent == "codex" { "-m" } else { "--model" };
            argv.extend([flag.into(), self.model.clone()]);
        }
        argv.push(prompt.into());
        argv
    }

    /// Picks the agent; a model chosen for the other one goes back to this one's default.
    pub fn set_agent(&mut self, agent: &str) {
        if agent != self.agent {
            self.model = if agent == "claude" { Git::default().model } else { String::new() };
            self.agent = agent.into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn by_default_claude_haiku_writes_in_the_style_of_recent_subjects() {
        assert_eq!(Git::default().message_argv("claude"), ["claude", "-p", "--model", "haiku", RECENT]);
    }

    #[test]
    fn codex_runs_read_only_and_claude_without_a_model_uses_its_own() {
        let codex = Git { agent: "codex".into(), model: String::new(), style: Style::Plain, ..Git::default() };
        assert_eq!(codex.message_argv("codex"), ["codex", "exec", "-s", "read-only", "--skip-git-repo-check", "--color", "never", PLAIN]);
        let claude = Git { model: String::new(), style: Style::Conventional, ..Git::default() };
        assert_eq!(claude.message_argv("/opt/claude"), ["/opt/claude", "-p", CONVENTIONAL]);
    }

    #[test]
    fn codex_takes_a_model_too_and_switching_agent_drops_the_other_one_s() {
        let mut git = Git::default();
        git.set_agent("codex");
        assert_eq!(git.model, "");
        git.model = "gpt-5.6-sol".into();
        assert_eq!(git.message_argv("codex")[7..], ["-m", "gpt-5.6-sol", RECENT]);
        git.set_agent("claude");
        assert_eq!(git.model, "haiku");
    }

    #[test]
    fn a_pr_is_written_by_the_same_agent_and_model_as_commit_messages() {
        assert_eq!(Git::default().pr_argv("claude"), ["claude", "-p", "--model", "haiku", PR]);
        let codex = Git { agent: "codex".into(), model: "gpt-5.6-sol".into(), ..Git::default() };
        assert_eq!(codex.pr_argv("codex")[7..], ["-m", "gpt-5.6-sol", PR]);
    }

    #[test]
    fn a_custom_style_asks_with_its_prompt_unless_it_is_empty() {
        let custom = Git { style: Style::Custom, prompt: " Use the imperative mood. ".into(), ..Git::default() };
        assert_eq!(custom.message_prompt(), "Use the imperative mood.");
        assert_eq!(Git { prompt: " ".into(), ..custom }.message_prompt(), RECENT);
    }

    #[test]
    fn an_unknown_style_reads_as_the_default_and_missing_fields_keep_theirs() {
        let git: Git = serde_json::from_str(r#"{"style":"haiku-poem","draft":true}"#).unwrap();
        assert_eq!(git, Git { draft: true, ..Git::default() });
    }
}
