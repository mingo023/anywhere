//! Carrying a conversation: its transcript file on this Mac, written where the same agent on the host looks for
//! it, with this Mac's folder swapped for the host's.

use crate::{Host, run};
use std::path::{Path, PathBuf};

/// A conversation's transcript on this Mac.
#[derive(Clone, Debug, PartialEq)]
pub struct Transcript {
    pub provider: String,
    pub id: String,
    pub file: PathBuf,
    /// Codex's path under its `sessions` folder, kept on the host; Claude's goes under the host folder's own name.
    pub rel: String,
}

/// The folders each agent keeps its transcripts in on this Mac: its own, then one per Anywhere account.
fn roots(provider: &str, home: &Path) -> Vec<PathBuf> {
    let (own, sub) = match provider {
        "claude" => (home.join(".claude"), "projects"),
        _ => (home.join(".codex"), "sessions"),
    };
    let mut list = vec![own.join(sub)];
    if let Ok(accounts) = std::fs::read_dir(daemon::home::accounts_dir(home).join(provider)) {
        list.extend(accounts.flatten().map(|a| a.path().join(sub)));
    }
    list
}

fn walk(dir: &Path, found: &mut dyn FnMut(&Path) -> bool) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else { return false };
    for e in entries.flatten() {
        let path = e.path();
        let hit = if path.is_dir() { walk(&path, found) } else { found(&path) };
        if hit {
            return true;
        }
    }
    false
}

/// Finds conversation `id` of `provider` under `home` (this Mac's home).
pub fn find(provider: &str, id: &str, home: &Path) -> Option<Transcript> {
    let claude = format!("{id}.jsonl");
    let codex = format!("-{id}.jsonl");
    for root in roots(provider, home) {
        let mut hit = None;
        walk(&root, &mut |p: &Path| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            let ok = match provider {
                "claude" => name == claude,
                _ => name.starts_with("rollout-") && name.ends_with(&codex),
            };
            if ok {
                hit = Some(p.to_path_buf());
            }
            ok
        });
        if let Some(file) = hit {
            let rel = file.strip_prefix(&root).ok()?.iter().map(|c| c.to_str()).collect::<Option<Vec<_>>>()?.join("/");
            return Some(Transcript { provider: provider.into(), id: id.into(), file, rel });
        }
    }
    None
}

/// Claude's folder name for a project at `path`: each character that isn't a letter or digit becomes `-`.
pub fn claude_slug(path: &str) -> String {
    path.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

/// `text` with `from`, and each path under it, pointing at `to` instead, wherever it appears: as a JSON value or
/// inside one, such as a message naming a file. A longer name that starts the same, like `from2`, stays.
pub fn moved(text: &str, from: &str, to: &str) -> String {
    let json = |s: &str| serde_json::to_string(s).unwrap_or_default();
    let (from, to) = (json(from), json(to));
    let (from, to) = (&from[1..from.len() - 1], &to[1..to.len() - 1]);
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(from) {
        let after = &rest[at + from.len()..];
        let whole = !after.starts_with(|c: char| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'));
        out.push_str(&rest[..at]);
        out.push_str(if whole { to } else { from });
        rest = after;
    }
    out + rest
}

/// Where `t` goes on the host, for a session in `cwd` there.
fn destination(host: &dyn Host, t: &Transcript, cwd: &str) -> Result<String, String> {
    match t.provider.as_str() {
        "claude" => {
            let status = run(host, "", &["claude", "auth", "status"]).unwrap_or_default();
            let projects = serde_json::from_str::<serde_json::Value>(&status).ok().and_then(|v| v["projectsDirectory"].as_str().map(String::from));
            let projects = match projects {
                Some(p) => p,
                None => format!("{}/.claude/projects", host.home().map_err(|e| e.to_string())?),
            };
            Ok(format!("{projects}/{}/{}.jsonl", claude_slug(cwd), t.id))
        }
        _ => {
            let codex = run(host, "", &["sh", "-c", "printf %s \"${CODEX_HOME:-$HOME/.codex}\""])?;
            Ok(format!("{codex}/sessions/{}", t.rel))
        }
    }
}

/// How much of a transcript goes to the host in one write when it isn't framed. A transcript runs to a hundred
/// megabytes, and one write holding it all keeps every other call to the host waiting, with nothing to show how far it
/// got.
pub const CHUNK: usize = 4 << 20;

/// How much of a transcript one zstd frame holds. Its images come twice, in a message and in its tool result, too far
/// apart for zstd's usual window; a window as big as the frame finds them, which makes it about 4× smaller.
pub const FRAME: usize = 16 << 20;

/// Writes `t` on the host for a session in `to` there, moved from `from` on this Mac, and checks every byte landed.
/// `framed` sends it as zstd frames, for a host whose pocketd has `fs.append`. `sent(done, total)` after each write.
pub fn carry(host: &dyn Host, t: &Transcript, from: &[&str], to: &str, framed: bool, sent: &mut dyn FnMut(usize, usize)) -> Result<String, String> {
    let mut text = String::from_utf8(std::fs::read(&t.file).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    for f in from {
        text = moved(&text, f, to);
    }
    let path = destination(host, t, to)?;
    let parent = path.rsplit_once('/').map_or("", |(dir, _)| dir);
    host.mkdir(parent).map_err(|e| e.to_string())?;
    host.write(&path, &[]).map_err(|e| e.to_string())?;
    let mut done = 0;
    for piece in text.as_bytes().chunks(if framed { FRAME } else { CHUNK }) {
        if framed { frame(piece).and_then(|f| host.append_frame(&path, &f)) } else { host.append(&path, piece) }.map_err(|e| e.to_string())?;
        done += piece.len();
        sent(done, text.len());
    }
    match host.size(&path).map_err(|e| e.to_string())? {
        Some(n) if n == text.len() as u64 => Ok(path),
        _ => Err("The conversation didn’t arrive whole".into()),
    }
}

fn frame(piece: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut zstd = zstd::bulk::Compressor::new(1)?;
    zstd.set_parameter(zstd::zstd_safe::CParameter::WindowLog(FRAME.ilog2()))?;
    zstd.set_parameter(zstd::zstd_safe::CParameter::EnableLongDistanceMatching(true))?;
    zstd.compress(piece)
}

#[cfg(test)]
mod tests {
    use super::{CHUNK, FRAME, carry, claude_slug, find, moved};
    use crate::testing::{Folder, scratch};

    #[test]
    fn claude_names_a_project_folder_by_its_path_with_dashes() {
        assert_eq!(claude_slug("/Users/me/.anywhere/worktrees/app/feat-x"), "-Users-me--anywhere-worktrees-app-feat-x");
    }

    #[test]
    fn a_moved_transcript_points_at_the_host_folder_and_paths_under_it_even_in_messages_but_not_lookalikes() {
        let line = r#"{"cwd":"/Users/me/app","file":"/Users/me/app/src/a.rs","other":"/Users/me/app2","dot":"/Users/me/app.old","said":"Fixed in /Users/me/app/src/a.rs."}"#;
        assert_eq!(
            moved(line, "/Users/me/app", "/home/dev/w"),
            r#"{"cwd":"/home/dev/w","file":"/home/dev/w/src/a.rs","other":"/Users/me/app2","dot":"/Users/me/app.old","said":"Fixed in /home/dev/w/src/a.rs."}"#
        );
    }

    #[test]
    fn transcripts_are_found_in_the_agents_own_folder_and_in_account_folders() {
        let home = scratch("find");
        let claude = home.join(".anywhere/accounts/claude/acc1/projects/-x");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(claude.join("abc.jsonl"), "{}").unwrap();
        let codex = home.join(".codex/sessions/2026/11/21");
        std::fs::create_dir_all(&codex).unwrap();
        std::fs::write(codex.join("rollout-2026-11-21T10-00-00-th1.jsonl"), "{}").unwrap();
        let c = find("claude", "abc", &home).unwrap();
        assert_eq!(c.file, claude.join("abc.jsonl"));
        let x = find("codex", "th1", &home).unwrap();
        assert_eq!(x.rel, "2026/11/21/rollout-2026-11-21T10-00-00-th1.jsonl");
        assert_eq!(find("claude", "nope", &home), None);
    }

    #[test]
    #[cfg_attr(windows, ignore = "the stand-in host runs Git Bash, whose paths are MSYS's, not a unix host's")]
    fn a_codex_conversation_lands_under_the_hosts_codex_home_with_its_folder_moved() {
        let mac = scratch("carry-mac");
        let dir = mac.join(".codex/sessions/2026/11/21");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("rollout-x-th1.jsonl"), "{\"cwd\":\"/Users/me/app\"}\n").unwrap();
        let t = find("codex", "th1", &mac).unwrap();
        let host_home = scratch("carry-host");
        let host = Folder { home: host_home.to_str().unwrap().into() };
        let path = carry(&host, &t, &["/Users/me/app"], "/home/dev/w", false, &mut |_, _| {}).unwrap();
        assert_eq!(path, format!("{}/.codex/sessions/2026/11/21/rollout-x-th1.jsonl", host_home.display()));
        assert_eq!(std::fs::read_to_string(path).unwrap(), "{\"cwd\":\"/home/dev/w\"}\n");
    }

    #[test]
    fn a_transcript_longer_than_one_write_lands_whole_and_says_how_far_it_got() {
        let mac = scratch("chunks-mac");
        let dir = mac.join(".codex/sessions/2026/11/22");
        std::fs::create_dir_all(&dir).unwrap();
        let line = format!("{{\"cwd\":\"/Users/me/app\",\"pad\":\"{}\"}}\n", "x".repeat(1000));
        let body = line.repeat(2 * CHUNK / line.len() + 100);
        std::fs::write(dir.join("rollout-x-th2.jsonl"), &body).unwrap();
        let t = find("codex", "th2", &mac).unwrap();
        let host_home = scratch("chunks-host");
        let host = Folder { home: host_home.to_str().unwrap().into() };
        let mut heard = vec![];
        let path = carry(&host, &t, &["/Users/me/app"], "/home/dev/w", false, &mut |done, of| heard.push((done, of))).unwrap();
        let landed = std::fs::read_to_string(path).unwrap();
        assert_eq!(landed, moved(&body, "/Users/me/app", "/home/dev/w"));
        let total = landed.len();
        assert!(total > 2 * CHUNK);
        assert_eq!(heard, vec![(CHUNK, total), (2 * CHUNK, total), (total, total)]);
    }

    #[test]
    fn a_framed_transcript_lands_whole_and_says_how_far_it_got() {
        let mac = scratch("frames-mac");
        let dir = mac.join(".codex/sessions/2026/11/22");
        std::fs::create_dir_all(&dir).unwrap();
        let line = format!("{{\"cwd\":\"/Users/me/app\",\"pad\":\"{}\"}}\n", "x".repeat(1000));
        let body = line.repeat(FRAME / line.len() + 100);
        std::fs::write(dir.join("rollout-x-th3.jsonl"), &body).unwrap();
        let t = find("codex", "th3", &mac).unwrap();
        let host_home = scratch("frames-host");
        let host = Folder { home: host_home.to_str().unwrap().into() };
        let mut heard = vec![];
        let path = carry(&host, &t, &["/Users/me/app"], "/home/dev/w", true, &mut |done, of| heard.push((done, of))).unwrap();
        let landed = std::fs::read_to_string(path).unwrap();
        assert_eq!(landed, moved(&body, "/Users/me/app", "/home/dev/w"));
        let total = landed.len();
        assert!(total > FRAME);
        assert_eq!(heard, vec![(FRAME, total), (total, total)]);
    }
}
