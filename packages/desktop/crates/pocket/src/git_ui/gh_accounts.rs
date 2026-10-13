use crate::desktop::Desktop;
use git::github::{self, GhStatus};
use gpui_kit::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use store::RepoConfig;

/// A comparison that hangs, as on a keychain prompt, gives up its turn after this.
const DETECT_TURN: Duration = Duration::from_secs(60);

/// Which signed-in github.com account `gh` acts as in each project.
#[derive(Default)]
pub(crate) struct GhAccounts {
    /// Every account `gh` is signed in to, as the last check found them.
    pub(crate) accounts: Vec<String>,
    pub(crate) active: String,
    tokens: Tokens,
    /// The project whose accounts are being compared, and since when; one at a time, as each account runs a login shell.
    detecting: Option<(String, Instant)>,
    /// Projects compared since the last check, so one no account can see isn't compared on every poll.
    tried: HashSet<String>,
}

impl GhAccounts {
    /// Takes the accounts a check of `gh` found; projects no account could see are compared again.
    pub(crate) fn checked(&mut self, status: &GhStatus) {
        (self.accounts, self.active) = match status {
            GhStatus::Ready { login, accounts, .. } => (accounts.clone(), login.clone()),
            _ => Default::default(),
        };
        self.tried.clear();
    }

    /// The account chosen for `project`, while it's signed in.
    pub(crate) fn chosen<'a>(&self, repos: &'a BTreeMap<String, RepoConfig>, project: &str) -> Option<&'a str> {
        repos.get(project).map(|r| r.gh_account.as_str()).filter(|a| self.accounts.iter().any(|s| s == a))
    }

    /// The next project to compare accounts for: one with none chosen, when there's more than one to choose from.
    pub(crate) fn due(&self, projects: &[String], repos: &BTreeMap<String, RepoConfig>, now: Instant) -> Option<String> {
        if self.detecting.as_ref().is_some_and(|(_, at)| now.duration_since(*at) < DETECT_TURN) || self.accounts.len() < 2 {
            return None;
        }
        projects.iter().find(|p| !self.tried.contains(*p) && self.chosen(repos, p).is_none()).cloned()
    }

    pub(crate) fn detecting(&self, project: &str) -> bool {
        self.detecting.as_ref().is_some_and(|(p, _)| p == project)
    }

    pub(crate) fn start(&mut self, project: &str, now: Instant) {
        self.detecting = Some((project.to_string(), now));
        self.tried.insert(project.to_string());
    }

    /// Saves the account found for `project` unless one was chosen meanwhile. True when it did.
    pub(crate) fn finish(&mut self, project: &str, found: Option<String>, repos: &mut BTreeMap<String, RepoConfig>) -> bool {
        if self.detecting(project) {
            self.detecting = None;
        }
        let Some(found) = found.filter(|_| self.chosen(repos, project).is_none()) else { return false };
        repos.entry(project.to_string()).or_default().gh_account = found;
        true
    }

    fn gh(&self, account: Option<String>) -> Gh {
        Gh { account, tokens: self.tokens.clone() }
    }
}

/// Each account's token, asked of `gh` once and shared with the tasks that run it.
#[derive(Clone, Default)]
struct Tokens(Arc<Mutex<HashMap<String, String>>>);

impl Tokens {
    fn get(&self, account: &str) -> Option<String> {
        if let Some(t) = self.0.lock().unwrap().get(account) {
            return Some(t.clone());
        }
        let token = daemon::run_login(&github::token_argv(account), "/", "").ok().filter(|t| !t.is_empty())?;
        self.0.lock().unwrap().insert(account.to_string(), token.clone());
        Some(token)
    }

    fn forget(&self, account: &str) {
        self.0.lock().unwrap().remove(account);
    }
}

/// Runs `gh` as one account, or as the active one without it.
#[derive(Clone)]
pub(crate) struct Gh {
    account: Option<String>,
    tokens: Tokens,
}

impl Gh {
    pub(crate) fn account(&self) -> Option<&str> {
        self.account.as_deref()
    }

    /// Runs `argv` in `dir` with `input` on stdin; a token GitHub refuses is asked of `gh` again once.
    pub(crate) fn run(&self, argv: &[impl AsRef<str>], dir: &str, input: &str) -> Result<String, String> {
        let argv: Vec<&str> = argv.iter().map(AsRef::as_ref).collect();
        // A host's gh signs in on the host, where this Mac's accounts mean nothing.
        let Some(account) = self.account.as_ref().filter(|_| hosts::split(dir).0.is_none()) else { return crate::util::run_login(&argv, dir, input) };
        let run = || daemon::run_login_env(&argv, dir, input, self.tokens.get(account).map(|t| ("GH_TOKEN".to_string(), t)).into_iter().collect());
        match run() {
            Err(e) if github::unauthorized(&e) => {
                self.tokens.forget(account);
                run()
            }
            out => out,
        }
    }
}

/// The account with the most access to the repo in `dir`, asking `gh` as each in turn.
fn detect(dir: &str, accounts: &[Gh], active: &str) -> Option<String> {
    let ranks: Vec<(&str, u8)> = accounts.iter().filter_map(|gh| Some((gh.account.as_deref()?, github::permission(gh.run(github::PERMISSION, dir, ""))))).collect();
    github::best(&ranks, active).map(str::to_string)
}

impl Desktop {
    /// How `gh` runs in `dir`: as its project's account, while that's signed in.
    pub(crate) fn gh_in(&self, dir: &str) -> Gh {
        let projects = self.projects();
        let account = self.project_of(dir, &projects).and_then(|p| self.gh_accounts.chosen(&self.store.repos, p)).map(str::to_string);
        self.gh_accounts.gh(account)
    }

    /// Compares the accounts' access to the next project with none chosen, off the UI thread.
    pub(crate) fn detect_gh_account(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.gh_accounts.due(&self.store.projects, &self.store.repos, Instant::now()) else { return };
        self.gh_accounts.start(&project, Instant::now());
        let accounts: Vec<Gh> = self.gh_accounts.accounts.iter().map(|a| self.gh_accounts.gh(Some(a.clone()))).collect();
        let (dir, active) = (project.clone(), self.gh_accounts.active.clone());
        let task = cx.background_executor().spawn(async move { detect(&dir, &accounts, &active) });
        cx.spawn(async move |this, cx| {
            let found = task.await;
            this.update(cx, |d, cx| {
                let found = found.filter(|_| d.store.projects.contains(&project));
                if d.gh_accounts.finish(&project, found, &mut d.store.repos) {
                    d.gh_account_changed(&project, cx);
                }
                d.detect_gh_account(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn pick_gh_account(&mut self, project: &str, account: String, cx: &mut Context<Self>) {
        self.store.repos.entry(project.to_string()).or_default().gh_account = account;
        self.gh_account_changed(project, cx);
    }

    /// Asks `gh` about the project's PRs again, as the account now chosen.
    fn gh_account_changed(&mut self, project: &str, cx: &mut Context<Self>) {
        for w in self.listed_trees(project).unwrap_or_default() {
            self.prs.forget(&w.path);
        }
        self.save_soon(cx);
        self.poll_prs(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::GhAccounts;
    use git::github::GhStatus;
    use std::collections::BTreeMap;
    use std::time::{Duration, Instant};
    use store::RepoConfig;

    fn signed_in(accounts: &[&str]) -> GhAccounts {
        let mut gh = GhAccounts::default();
        gh.checked(&GhStatus::Ready { version: "2.95.0".into(), login: accounts[0].into(), accounts: accounts.iter().map(|a| a.to_string()).collect() });
        gh
    }

    fn repos(chosen: &[(&str, &str)]) -> BTreeMap<String, RepoConfig> {
        chosen.iter().map(|(p, a)| (p.to_string(), RepoConfig { gh_account: a.to_string(), ..Default::default() })).collect()
    }

    fn projects() -> Vec<String> {
        vec!["/a".into(), "/b".into()]
    }

    #[test]
    fn accounts_are_compared_only_when_there_is_more_than_one() {
        assert_eq!(signed_in(&["me"]).due(&projects(), &repos(&[]), Instant::now()), None);
        assert_eq!(signed_in(&["me", "work"]).due(&projects(), &repos(&[]), Instant::now()).as_deref(), Some("/a"));
    }

    #[test]
    fn a_project_whose_account_signed_out_is_compared_again() {
        let gh = signed_in(&["me", "work"]);
        assert_eq!(gh.due(&projects(), &repos(&[("/a", "work"), ("/b", "gone")]), Instant::now()).as_deref(), Some("/b"));
        assert_eq!(gh.chosen(&repos(&[("/b", "gone")]), "/b"), None);
    }

    #[test]
    fn projects_are_compared_one_at_a_time_and_one_no_account_sees_waits_for_the_next_check() {
        let mut gh = signed_in(&["me", "work"]);
        let mut chosen = repos(&[]);
        gh.start("/a", Instant::now());
        assert_eq!(gh.due(&projects(), &chosen, Instant::now()), None);
        assert!(!gh.finish("/a", None, &mut chosen));
        assert_eq!(gh.due(&projects(), &chosen, Instant::now()).as_deref(), Some("/b"));
        gh.start("/b", Instant::now());
        assert!(gh.finish("/b", Some("work".into()), &mut chosen));
        assert_eq!(gh.due(&projects(), &chosen, Instant::now()), None);
        gh.checked(&GhStatus::Ready { version: "2.95.0".into(), login: "me".into(), accounts: vec!["me".into(), "work".into()] });
        assert_eq!(gh.due(&projects(), &chosen, Instant::now()).as_deref(), Some("/a"));
    }

    #[test]
    fn an_account_picked_while_comparing_is_kept() {
        let mut gh = signed_in(&["me", "work"]);
        let mut chosen = repos(&[]);
        gh.start("/a", Instant::now());
        chosen.insert("/a".into(), RepoConfig { gh_account: "me".into(), ..Default::default() });
        assert!(!gh.finish("/a", Some("work".into()), &mut chosen));
        assert_eq!(gh.chosen(&chosen, "/a"), Some("me"));
    }

    #[test]
    fn a_comparison_that_hangs_gives_up_its_turn_after_a_minute() {
        let mut gh = signed_in(&["me", "work"]);
        let now = Instant::now();
        gh.start("/a", now);
        assert_eq!(gh.due(&projects(), &repos(&[]), now + Duration::from_secs(59)), None);
        assert_eq!(gh.due(&projects(), &repos(&[]), now + Duration::from_secs(60)).as_deref(), Some("/b"));
    }
}
