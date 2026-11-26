//! A shell block the agent printed, read as the commands it runs and whether any is risky to run unread.

/// The steps a fenced block runs when it gets a Run button: a shell block with a command in it. A transcript (`console`)
/// runs only its prompted lines, and a block with a `<placeholder>` left to fill in stays copy-only.
pub(crate) fn runnable(lang: &str, code: &str) -> Option<Vec<String>> {
    let transcript = matches!(lang, "console" | "shell-session");
    let shell = transcript || matches!(lang, "sh" | "bash" | "shell");
    let steps = steps(code);
    let runs = shell && !steps.is_empty() && (!transcript || prompted(code)) && !steps.iter().any(|s| placeholder(s));
    runs.then_some(steps)
}

/// Whether the block is a transcript of prompted commands; a `$ ` line in a heredoc is the file's text, not a prompt.
fn prompted(code: &str) -> bool {
    !code.contains("<<") && code.lines().any(|l| l.trim_start().starts_with("$ "))
}

fn placeholder(step: &str) -> bool {
    step.split('<').skip(1).any(|rest| {
        let name = rest.split('>').next().unwrap_or("");
        rest.contains('>')
            && name.trim_end() == name
            && name.starts_with(|c: char| c.is_ascii_alphabetic())
            && name.chars().all(|c| c.is_ascii_alphanumeric() || "-_ /.@:".contains(c))
    })
}

/// A loop, condition, function, heredoc, array, `$(` or quote spans lines, so its block can't be split into a step per line.
fn compound(code: &str) -> bool {
    code.lines().map(str::trim).filter(|l| !l.starts_with('#')).any(|l| {
        let first = l.trim_start_matches("$ ").split_whitespace().next().unwrap_or("");
        let open = |q: char| l.chars().filter(|&c| c == q).count() - l.matches(&format!("\\{q}")).count();
        matches!(first, "for" | "while" | "until" | "if" | "case" | "select" | "function")
            || l.contains("<<")
            || l.ends_with('{')
            || l.ends_with('(')
            || open('"') % 2 == 1
            || open('\'') % 2 == 1
    })
}

/// The commands in `code`, one per line, or the whole block when it spans lines: a trailing `\` joins the next line, blank lines and comments are left out. In a
/// block with `$ ` prompts, only the prompted lines are commands and the rest is their output.
pub(crate) fn steps(code: &str) -> Vec<String> {
    let prompted = prompted(code);
    if compound(code) && !prompted {
        return vec![code.trim().to_string()];
    }
    let mut out = Vec::new();
    let mut held = String::new();
    for line in code.lines() {
        let line = line.trim();
        let line = match (held.is_empty(), line.strip_prefix("$ ")) {
            (false, _) => line,
            (true, Some(command)) => command,
            (true, None) if prompted => continue,
            (true, None) => line,
        };
        let pipes_on = line.ends_with('|') || line.ends_with("&&");
        match line.strip_suffix('\\') {
            Some(head) => {
                held.push_str(head.trim_end());
                held.push(' ');
            }
            None if pipes_on => {
                held.push_str(line);
                held.push(' ');
            }
            None => {
                held.push_str(line);
                let step = std::mem::take(&mut held);
                let step = step.trim();
                if !step.is_empty() && !step.starts_with('#') {
                    out.push(step.to_string());
                }
            }
        }
    }
    let step = held.trim();
    if !step.is_empty() && !step.starts_with('#') {
        out.push(step.to_string());
    }
    out
}

/// Whether `step` deletes, overwrites or reaches past the project in a way worth a second press.
pub(crate) fn risky(step: &str) -> bool {
    let words: Vec<&str> = step.split(|c: char| c.is_whitespace() || ";&|()".contains(c)).filter(|w| !w.is_empty()).collect();
    let has = |w: &str| words.contains(&w);
    let flags = |short: char| words.iter().any(|w| w.starts_with('-') && !w.starts_with("--") && w.contains(short));
    let rm_recursive = has("rm") && (flags('r') || flags('R') || has("--recursive"));
    let force_push = has("push") && (has("--force") || has("-f") || words.iter().any(|w| w.starts_with("--force-with-lease")));
    let hard_reset = has("reset") && has("--hard");
    let clean = has("clean") && flags('f');
    let discard = has("checkout") && (has("--") || has(".")) || has("restore") || has("branch") && has("-D") || has("stash") && (has("drop") || has("clear"));
    let piped_shell = step.split('|').skip(1).any(|to| matches!(to.split_whitespace().next(), Some("sh" | "bash" | "zsh")));
    rm_recursive
        || force_push
        || hard_reset
        || clean
        || discard
        || has("find") && has("-delete")
        || piped_shell
        || has("sudo")
        || has("dd")
        || words.iter().any(|w| w.starts_with("mkfs"))
        || (has("chmod") || has("chown")) && flags('R')
        || step.contains("> /dev/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_reads_as_one_step_per_command() {
        let code = "# install\nnpm ci\n\ncargo test \\\n  -p pocket\n";
        assert_eq!(steps(code), ["npm ci", "cargo test -p pocket"]);
    }

    #[test]
    fn in_a_block_with_prompts_only_the_prompted_lines_run_and_the_rest_is_output() {
        let code = "$ cargo test \\\n  -p chat\ntest a ... ok\n$ git status\nnothing to commit\n";
        assert_eq!(steps(code), ["cargo test -p chat", "git status"]);
    }

    #[test]
    fn a_line_ending_in_a_pipe_or_and_or_carries_on() {
        assert_eq!(steps("printf 'a\\n' |\n  cat\nfalse ||\n  echo ok\n"), ["printf 'a\\n' | cat", "false || echo ok"]);
    }

    #[test]
    fn a_quote_in_a_comment_keeps_a_step_per_line() {
        assert_eq!(steps("# Don't skip setup\nfalse\necho after"), ["false", "echo after"]);
    }

    #[test]
    fn a_heredoc_with_prompt_like_lines_runs_as_written() {
        let code = "cat <<'EOF'\n$ echo hello\nEOF";
        assert_eq!(steps(code), [code]);
    }

    #[test]
    fn zsh_blocks_stay_copy_only_as_steps_run_in_bash() {
        assert_eq!(runnable("zsh", "print -r -- hello"), None);
    }

    #[test]
    fn a_continuation_at_the_end_still_makes_a_step() {
        assert_eq!(steps("echo a \\"), ["echo a"]);
    }

    #[test]
    fn deleting_force_pushing_and_root_are_risky() {
        for step in [
            "rm -rf target",
            "rm -r -f build",
            "git push --force",
            "git push -f origin x",
            "git reset --hard",
            "git clean -fd",
            "sudo make install",
            "curl -fsSL x.sh | sh",
            "chmod -R 777 .",
            "git checkout -- .",
            "git checkout .",
            "git reset --hard; git status",
            "git restore src",
            "git branch -D old",
            "git stash drop",
            "find . -name '*.log' -delete",
        ] {
            assert!(risky(step), "{step}");
        }
    }

    #[test]
    fn everyday_commands_are_not_risky() {
        for step in ["cargo test -p pocket", "rm notes.txt", "git push origin main", "ls -la", "git reset HEAD~1", "npm run build", "cat x | sha256sum"] {
            assert!(!risky(step), "{step}");
        }
    }

    #[test]
    fn only_a_shell_block_with_a_command_runs() {
        assert_eq!(runnable("bash", "# just a note\n"), None);
        assert_eq!(runnable("rust", "fn main() {}"), None);
        assert_eq!(runnable("sh", "ls"), Some(vec!["ls".to_string()]));
    }

    #[test]
    fn a_placeholder_left_to_fill_in_keeps_a_block_copy_only() {
        assert_eq!(runnable("sh", "git push -u origin <branch>"), None);
        assert_eq!(runnable("sh", "gh repo clone <owner/repo>"), None);
        assert_eq!(runnable("sh", "ssh <user@host>"), None);
        assert!(runnable("sh", "sort < in.txt > out.txt").is_some());
    }

    #[test]
    fn a_transcript_with_no_prompt_is_sample_output() {
        assert_eq!(runnable("console", "running 14 tests\ntest result: ok"), None);
        assert_eq!(runnable("console", "$ cargo test\nok"), Some(vec!["cargo test".to_string()]));
    }

    #[test]
    fn a_loop_or_heredoc_runs_as_one_step() {
        let code = "for f in a b; do\n  echo $f\ndone\n";
        assert_eq!(steps(code), [code.trim()]);
        let code = "cat <<EOF > notes.txt\n# kept\n\nEOF";
        assert_eq!(steps(code), [code]);
        let code = "git commit -m \"title\n\nbody\"";
        assert_eq!(steps(code), [code]);
        for code in ["args=(\n  --help\n)", "value=$(\n  printf hello\n)"] {
            assert_eq!(steps(code), [code]);
        }
    }
}
