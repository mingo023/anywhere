//! Handing a session on this Mac off to a host: what the host must have first, packing the worktree, placing it there,
//! carrying the conversation, and the summary a session starts from when its conversation can't move.

pub mod checks;
pub mod pack;
pub mod place;
pub mod summary;
pub mod transcript;

use std::io;
use std::time::Duration;

pub use git::Output;

/// What a handoff does on the host: run commands and write files there. Every path is the host's.
pub trait Host: Send + Sync {
    fn exec(&self, cwd: &str, argv: &[&str], timeout: Duration) -> io::Result<Output>;
    fn write(&self, path: &str, data: &[u8]) -> io::Result<()>;
    fn append(&self, path: &str, data: &[u8]) -> io::Result<()>;
    /// Adds what zstd `frame` decodes to at the end of the file at `path`.
    fn append_frame(&self, path: &str, frame: &[u8]) -> io::Result<()>;
    fn mkdir(&self, path: &str) -> io::Result<()>;
    /// The size of the file at `path`, `None` when nothing is there.
    fn size(&self, path: &str) -> io::Result<Option<u64>>;
    fn remove(&self, path: &str) -> io::Result<()>;
    fn home(&self) -> io::Result<String>;
    /// The home every channel on the host shares, which holds its worktrees.
    fn shared(&self) -> io::Result<String>;
}

impl Host for hosts::remote::Client {
    fn exec(&self, cwd: &str, argv: &[&str], timeout: Duration) -> io::Result<Output> {
        git::Exec::exec(self, cwd, argv, None, timeout)
    }

    fn write(&self, path: &str, data: &[u8]) -> io::Result<()> {
        hosts::remote::Client::write(self, path, data, false)
    }

    // A pocketd without fs.append only replaces a file with fs.write, so the bytes go through a shell's stdin.
    fn append(&self, path: &str, data: &[u8]) -> io::Result<()> {
        let out = git::Exec::exec(self, "", &["sh", "-c", r#"cat >> "$1""#, "sh", path], Some(data), TIMEOUT)?;
        if out.success() { Ok(()) } else { Err(io::Error::other(String::from_utf8_lossy(&out.stderr).trim().to_string())) }
    }

    fn append_frame(&self, path: &str, frame: &[u8]) -> io::Result<()> {
        hosts::remote::Client::append_frame(self, path, frame)
    }

    fn mkdir(&self, path: &str) -> io::Result<()> {
        hosts::remote::Client::mkdir(self, path, true)
    }

    fn size(&self, path: &str) -> io::Result<Option<u64>> {
        Ok(git::Exec::stat(self, path)?.map(|s| s.size))
    }

    fn remove(&self, path: &str) -> io::Result<()> {
        hosts::remote::Client::remove(self, path, false)
    }

    fn home(&self) -> io::Result<String> {
        hosts::remote::Client::home(self)
    }

    fn shared(&self) -> io::Result<String> {
        self.shared_dir()
    }
}

/// How long one command on the host may take; a fetch of a large bundle is the slowest.
pub const TIMEOUT: Duration = Duration::from_secs(120);

/// The handoff's steps in the order they run, as the progress sheet lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Interrupt the agent's turn.
    Stop,
    Pack,
    Send,
    Worktree,
    Conversation,
    Start,
    /// The host's session is up with its conversation.
    Check,
}

/// `working`: the agent is mid-turn; `conversation`: its conversation moves, else a new session starts from a summary.
pub fn steps(working: bool, conversation: bool) -> Vec<Step> {
    let mut list = Vec::new();
    if working {
        list.push(Step::Stop);
    }
    list.extend([Step::Pack, Step::Send, Step::Worktree]);
    if conversation {
        list.push(Step::Conversation);
    }
    list.push(Step::Start);
    if conversation {
        list.push(Step::Check);
    }
    list
}

impl Step {
    pub fn label(self, host: &str, agent: &str) -> String {
        match self {
            Step::Stop => "Stop this turn".into(),
            Step::Pack => "Pack the code".into(),
            Step::Send => format!("Send to {host}"),
            Step::Worktree => "Make the worktree".into(),
            Step::Conversation => "Bring the conversation".into(),
            Step::Start => format!("Start {agent}"),
            Step::Check => "Check the history".into(),
        }
    }
}

/// The agents whose conversation moves to another machine: their transcripts are files, and they fork from one.
pub fn carries_conversation(provider: &str) -> bool {
    matches!(provider, "claude" | "codex")
}

/// Why `provider`'s session with `conversation` (its provider session id) can't continue on a host; `None` when it can.
pub fn why_not(provider: &str, agent: &str, conversation: Option<&str>) -> Option<String> {
    if conversation.is_none() {
        return Some("Anywhere didn’t start this session, so it can’t read the conversation.".into());
    }
    (!carries_conversation(provider)).then(|| format!("{agent}’s conversation can’t move to another machine yet."))
}

/// `s` as one shell word.
pub(crate) fn quoted(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Runs `argv` on `host` in `cwd`, or its home when `cwd` is empty; its stdout, trimmed, or what it printed on failure.
pub(crate) fn run(host: &dyn Host, cwd: &str, argv: &[&str]) -> Result<String, String> {
    let home;
    let cwd = if cwd.is_empty() {
        home = host.home().map_err(|e| e.to_string())?;
        home.as_str()
    } else {
        cwd
    };
    let out = host.exec(cwd, argv, TIMEOUT).map_err(|e| e.to_string())?;
    if out.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() { format!("{} failed", argv.join(" ")) } else { err })
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::{Host, Output};
    use std::io;
    use std::path::Path;
    use std::process::Command;
    use std::time::Duration;

    /// A host that is a folder on this Mac: commands run here, in a scratch home.
    pub struct Folder {
        pub home: String,
    }

    impl Host for Folder {
        fn exec(&self, cwd: &str, argv: &[&str], _: Duration) -> io::Result<Output> {
            // As pocketd's exec does.
            if !Path::new(cwd).is_absolute() {
                return Err(io::Error::other("paths must be absolute"));
            }
            let out = Command::new(argv[0]).args(&argv[1..]).env("HOME", &self.home).current_dir(cwd).output()?;
            Ok(Output { code: out.status.code().unwrap_or(-1), stdout: out.stdout, stderr: out.stderr })
        }

        fn write(&self, path: &str, data: &[u8]) -> io::Result<()> {
            std::fs::write(path, data)
        }

        fn append(&self, path: &str, data: &[u8]) -> io::Result<()> {
            use std::io::Write;
            std::fs::OpenOptions::new().append(true).open(path)?.write_all(data)
        }

        fn append_frame(&self, path: &str, frame: &[u8]) -> io::Result<()> {
            self.append(path, &zstd::decode_all(frame)?)
        }

        fn mkdir(&self, path: &str) -> io::Result<()> {
            std::fs::create_dir_all(path)
        }

        fn size(&self, path: &str) -> io::Result<Option<u64>> {
            Ok(std::fs::metadata(path).ok().map(|m| m.len()))
        }

        fn remove(&self, path: &str) -> io::Result<()> {
            std::fs::remove_file(path)
        }

        fn home(&self) -> io::Result<String> {
            Ok(self.home.clone())
        }

        fn shared(&self) -> io::Result<String> {
            Ok(format!("{}/.zoes", self.home))
        }
    }

    pub fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("handoff-{name}-{}", std::process::id()));
        _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // On Windows it would come back as `\\?\C:\…`, which git reads as a host name.
        if cfg!(windows) { dir } else { dir.canonicalize().unwrap() }
    }

    pub fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "init.defaultBranch=main"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// An origin with one commit, a Mac clone of it on `branch` with a commit not pushed, and a host clone at `host_home/code/app`.
    pub fn repos(name: &str, branch: &str) -> (std::path::PathBuf, String, String) {
        let root = scratch(name);
        let origin = root.join("origin.git");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "-q", "--bare"]);
        let mac = root.join("mac/app");
        std::fs::create_dir_all(&mac).unwrap();
        git(&mac, &["init", "-q"]);
        std::fs::write(mac.join("a.txt"), "one\n").unwrap();
        git(&mac, &["add", "."]);
        git(&mac, &["commit", "-qm", "first"]);
        git(&mac, &["remote", "add", "origin", origin.to_str().unwrap()]);
        git(&mac, &["push", "-q", "origin", "main"]);
        git(&mac, &["checkout", "-qb", branch]);
        std::fs::write(mac.join("a.txt"), "two\n").unwrap();
        git(&mac, &["commit", "-qam", "second"]);
        let host_home = root.join("host");
        std::fs::create_dir_all(host_home.join("code")).unwrap();
        git(&host_home.join("code"), &["clone", "-q", origin.to_str().unwrap(), "app"]);
        (root.clone(), mac.to_str().unwrap().into(), host_home.to_str().unwrap().into())
    }
}

#[cfg(test)]
mod tests {
    use super::{Step, steps, why_not};

    #[test]
    fn a_working_agent_is_stopped_first_and_only_a_moved_conversation_is_checked() {
        assert_eq!(steps(true, true), vec![Step::Stop, Step::Pack, Step::Send, Step::Worktree, Step::Conversation, Step::Start, Step::Check]);
        assert_eq!(steps(false, false), vec![Step::Pack, Step::Send, Step::Worktree, Step::Start]);
    }

    #[test]
    fn only_claude_and_codex_with_a_conversation_carry_it() {
        assert_eq!(why_not("claude", "Claude Code", Some("abc")), None);
        assert_eq!(why_not("codex", "Codex", Some("abc")), None);
        assert!(why_not("grok", "Grok", Some("abc")).unwrap().contains("Grok"));
        assert!(why_not("claude", "Claude Code", None).unwrap().contains("didn’t start"));
    }
}
