//! What a host must already have before anything is sent to it. Anything missing is fixed by hand on the host.

use crate::{Host, quoted, run};

/// What the handoff needs the host to have.
pub struct Wanted<'a> {
    /// This Mac's checkout's origin; `None` matches the host's project by folder name.
    pub origin: Option<&'a str>,
    pub repo: &'a str,
    pub branch: &'a str,
    /// The agent that will start there; `None` skips its sign-in.
    pub agent: Option<&'a str>,
}

/// One thing the host lacks, with the command that fixes it there.
#[derive(Clone, Debug, PartialEq)]
pub enum Problem {
    NoProject { repo: String, origin: Option<String> },
    NotSignedIn { provider: String },
    /// The branch is on the host with commits this Mac lacks, or checked out there.
    BranchTaken { branch: String },
    /// The host's pocketd didn't answer.
    Offline,
}

impl Problem {
    /// The problem as the sheet titles it, on host `host`, for agent `agent`.
    pub fn title(&self, host: &str, agent: &str) -> String {
        match self {
            Problem::NoProject { repo, .. } => format!("{repo} isn’t on {host}"),
            Problem::NotSignedIn { .. } => format!("{agent} isn’t signed in on {host}"),
            Problem::BranchTaken { branch } => format!("{branch} is already on {host}"),
            Problem::Offline => format!("{host} isn’t answering"),
        }
    }

    pub fn why(&self) -> Option<&'static str> {
        matches!(self, Problem::BranchTaken { .. }).then_some("It has commits this Mac doesn’t.")
    }

    /// The command to run on the host; `None` when there's nothing to clone from.
    pub fn fix(&self) -> Option<String> {
        match self {
            Problem::NoProject { repo, origin } => origin.as_ref().map(|o| format!("git clone {} ~/code/{repo}", quoted(o))),
            Problem::NotSignedIn { provider } => Some(sign_in(provider).into()),
            Problem::BranchTaken { branch } => Some(format!("git branch -m {branch} {branch}-old")),
            Problem::Offline => None,
        }
    }
}

fn sign_in(provider: &str) -> &'static str {
    match provider {
        "claude" => "claude auth login",
        _ => "codex login",
    }
}

/// What a host was found to have.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Probe {
    /// The host's checkout of the project.
    pub project: Option<String>,
    pub problems: Vec<Problem>,
}

impl Probe {
    pub fn ready(&self) -> bool {
        self.problems.is_empty() && self.project.is_some()
    }
}

/// The origin URL without what differs between two clones of one repository: a trailing `.git` or `/`.
fn same_origin(a: &str, b: &str) -> bool {
    let norm = |u: &str| u.trim().trim_end_matches('/').trim_end_matches(".git").to_lowercase();
    norm(a) == norm(b)
}

fn basename(path: &str) -> &str {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or(path)
}

/// Finds the host's checkout among `projects` (paths on the host), then where `Problem::fix` clones it: the one with the
/// same origin, else the same folder name.
fn find_project(host: &dyn Host, projects: &[String], w: &Wanted) -> Option<String> {
    let mut candidates = projects.to_vec();
    if let Ok(home) = host.home() {
        let cloned = format!("{home}/code/{}", w.repo);
        if !candidates.contains(&cloned) && host.size(&cloned).ok().flatten().is_some() {
            candidates.push(cloned);
        }
    }
    match w.origin {
        Some(origin) => candidates.into_iter().find(|p| run(host, p, &["git", "remote", "get-url", "origin"]).is_ok_and(|u| same_origin(&u, origin))),
        None => candidates.into_iter().find(|p| basename(p) == w.repo),
    }
}

/// Whether `provider` is signed in on the host. Agents with no status command pass.
fn signed_in(host: &dyn Host, provider: &str) -> bool {
    let argv: &[&str] = match provider {
        "claude" => &["claude", "auth", "status"],
        "codex" => &["codex", "login", "status"],
        _ => return true,
    };
    run(host, "", argv).is_ok()
}

/// The branch's commit on the host and whether a worktree there has it checked out; `None` when it isn't there.
fn branch_on(host: &dyn Host, project: &str, branch: &str) -> Option<(String, bool)> {
    let oid = run(host, project, &["git", "rev-parse", "-q", "--verify", &format!("refs/heads/{branch}")]).ok()?;
    let list = run(host, project, &["git", "worktree", "list", "--porcelain"]).unwrap_or_default();
    Some((oid, list.lines().any(|l| l == format!("branch refs/heads/{branch}"))))
}

/// Checks `host` for `w`. `holds` says whether this Mac's branch already has a commit, so the host's copy of the
/// branch can move up to it.
pub fn probe(host: &dyn Host, projects: &[String], w: &Wanted, holds: &dyn Fn(&str) -> bool) -> Probe {
    let mut problems = Vec::new();
    let project = find_project(host, projects, w);
    match &project {
        None => problems.push(Problem::NoProject { repo: w.repo.into(), origin: w.origin.map(Into::into) }),
        Some(p) => {
            if let Some((oid, out)) = branch_on(host, p, w.branch)
                && (out || !holds(&oid))
            {
                problems.push(Problem::BranchTaken { branch: w.branch.into() });
            }
        }
    }
    if let Some(agent) = w.agent.filter(|a| !signed_in(host, a)) {
        problems.push(Problem::NotSignedIn { provider: agent.into() });
    }
    Probe { project, problems }
}

#[cfg(test)]
mod tests {
    use super::{Problem, Wanted, probe, same_origin};
    use crate::pack::holds;
    use crate::testing::{Folder, git, repos};
    use std::path::Path;

    fn wanted<'a>(origin: &'a str) -> Wanted<'a> {
        Wanted { origin: Some(origin), repo: "app", branch: "feat", agent: None }
    }

    #[test]
    fn origins_match_without_a_trailing_git_or_slash() {
        assert!(same_origin("git@github.com:me/app.git", "git@github.com:me/app"));
        assert!(same_origin("https://github.com/Me/App/", "https://github.com/me/app.git"));
        assert!(!same_origin("git@github.com:me/app", "git@github.com:me/api"));
    }

    #[test]
    fn a_host_with_a_clone_of_the_same_origin_is_ready() {
        let (root, mac, home) = repos("ready", "feat");
        let origin = root.join("origin.git");
        let host = Folder { home: home.clone() };
        let p = probe(&host, &[format!("{home}/elsewhere"), format!("{home}/code/app")], &wanted(origin.to_str().unwrap()), &|o| holds(&mac, o));
        assert_eq!(p.project.as_deref(), Some(format!("{home}/code/app").as_str()));
        assert!(p.ready(), "{p:?}");
    }

    #[test]
    fn a_clone_made_by_the_fix_counts_though_the_host_has_no_such_project() {
        let (root, mac, home) = repos("handcloned", "feat");
        let host = Folder { home: home.clone() };
        let p = probe(&host, &[], &wanted(root.join("origin.git").to_str().unwrap()), &|o| holds(&mac, o));
        assert_eq!(p.project, Some(format!("{home}/code/app")));
    }

    #[test]
    fn a_host_without_the_project_says_how_to_clone_it() {
        let (_root, mac, home) = repos("noproject", "feat");
        let host = Folder { home: home.clone() };
        let p = probe(&host, &[format!("{home}/code/app")], &wanted("git@github.com:me/other.git"), &|o| holds(&mac, o));
        assert_eq!(p.problems, vec![Problem::NoProject { repo: "app".into(), origin: Some("git@github.com:me/other.git".into()) }]);
        assert_eq!(p.problems[0].fix().unwrap(), "git clone 'git@github.com:me/other.git' ~/code/app");
    }

    #[test]
    fn a_branch_on_the_host_that_this_mac_already_holds_can_move_up_but_a_diverged_one_is_taken() {
        let (root, mac, home) = repos("branch", "feat");
        let origin = root.join("origin.git");
        let clone = Path::new(&home).join("code/app");
        let main = git(&clone, &["rev-parse", "main"]);
        git(&clone, &["branch", "feat", &main]);
        let host = Folder { home: home.clone() };
        let projects = [clone.to_str().unwrap().to_string()];
        assert!(probe(&host, &projects, &wanted(origin.to_str().unwrap()), &|o| holds(&mac, o)).ready());
        git(&clone, &["checkout", "-q", "feat"]);
        std::fs::write(clone.join("b.txt"), "b").unwrap();
        git(&clone, &["add", "."]);
        git(&clone, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "theirs"]);
        git(&clone, &["checkout", "-q", "main"]);
        let p = probe(&host, &projects, &wanted(origin.to_str().unwrap()), &|o| holds(&mac, o));
        assert_eq!(p.problems, vec![Problem::BranchTaken { branch: "feat".into() }]);
    }

    #[test]
    fn a_branch_checked_out_on_the_host_is_taken_even_when_this_mac_holds_it() {
        let (root, mac, home) = repos("checkedout", "feat");
        let clone = Path::new(&home).join("code/app");
        git(&clone, &["checkout", "-qb", "feat"]);
        let host = Folder { home: home.clone() };
        let p = probe(&host, &[clone.to_str().unwrap().into()], &wanted(root.join("origin.git").to_str().unwrap()), &|o| holds(&mac, o));
        assert_eq!(p.problems, vec![Problem::BranchTaken { branch: "feat".into() }]);
    }

    #[test]
    fn an_agent_without_a_status_command_passes_its_sign_in() {
        let (root, mac, home) = repos("signin", "feat");
        let host = Folder { home: home.clone() };
        let origin = root.join("origin.git");
        let w = Wanted { agent: Some("grok"), ..wanted(origin.to_str().unwrap()) };
        assert!(probe(&host, &[format!("{home}/code/app")], &w, &|o| holds(&mac, o)).ready());
    }
}
