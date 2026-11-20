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
    /// Every account `gh` is signed in to on this Mac, as the last check found them.
    mac: Signed,
    tokens: Tokens,
    /// The project whose accounts are being compared, and since when; one at a time, as each account runs a login shell.
    detecting: Option<(String, Instant)>,
    /// Projects compared since the last check, so one no account can see isn't compared on every poll.
    tried: HashSet<String>,
    /// Each host's, read there while comparing one of its projects: a host's `gh` knows nothing of this Mac's.
    hosts: HashMap<String, Signed>,
}

/// The github.com accounts `gh` is signed in to on one machine, and its active one.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Signed {
    pub(crate) accounts: Vec<String>,
    pub(crate) active: String,
}

impl From<&GhStatus> for Signed {
    fn from(status: &GhStatus) -> Self {
        match status {
            GhStatus::Ready { login, accounts, .. } => Signed { accounts: accounts.clone(), active: login.clone() },
            _ => Signed::default(),
        }
    }
}

impl GhAccounts {
    /// Takes the accounts a check of `gh` found; projects no account could see are compared again.
    pub(crate) fn checked(&mut self, status: &GhStatus) {
        self.mac = status.into();
        self.tried.clear();
        self.hosts.clear();
    }

    /// The accounts on the machine holding `key`; `None` on a host whose accounts aren't read yet.
    pub(crate) fn accounts_of(&self, key: &str) -> Option<&Signed> {
        match hosts::split(key).0 {
            None => Some(&self.mac),
            Some(host) => self.hosts.get(host),
        }
    }

    pub(crate) fn any_choice(&self) -> bool {
        std::iter::once(&self.mac).chain(self.hosts.values()).any(|s| s.accounts.len() > 1)
    }

    /// The account chosen for `project`, while it's signed in on the machine holding it.
    pub(crate) fn chosen<'a>(&self, repos: &'a BTreeMap<String, RepoConfig>, project: &str) -> Option<&'a str> {
        repos.get(project).map(|r| r.gh_account.as_str()).filter(|a| self.accounts_of(project).is_some_and(|s| s.accounts.iter().any(|s| s == a)))
    }

    fn comparable(&self, project: &str) -> bool {
        self.accounts_of(project).is_none_or(|s| s.accounts.len() > 1)
    }

    pub(crate) fn read_host(&mut self, project: &str, signed: Signed) {
        if let (Some(host), _) = hosts::split(project) {
            self.hosts.insert(host.to_string(), signed);
        }
    }

    pub(crate) fn reconnected(&mut self, id: &str) {
        self.hosts.remove(id);
        self.tried.retain(|p| hosts::split(p).0 != Some(id));
    }

    /// The next project to compare accounts for: one with none chosen, when there's more than one to choose from.
    pub(crate) fn due(&self, projects: &[String], repos: &BTreeMap<String, RepoConfig>, now: Instant) -> Option<String> {
        if self.detecting.as_ref().is_some_and(|(_, at)| now.duration_since(*at) < DETECT_TURN) {
            return None;
        }
        projects.iter().find(|p| !self.tried.contains(*p) && self.chosen(repos, p).is_none() && self.comparable(p)).cloned()
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
        let Some(account) = &self.account else { return crate::util::run_login(&argv, dir, input) };
        if hosts::split(dir).0.is_some() {
            let argv = github::as_account(account, &argv);
            return crate::util::run_login(&argv.iter().map(String::as_str).collect::<Vec<_>>(), dir, input);
        }
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
        let known = self.gh_accounts.accounts_of(&project).cloned();
        let (dir, base) = (project.clone(), self.gh_accounts.gh(None));
        let task = cx.background_executor().spawn(async move {
            let read = known.is_none();
            let signed = known.unwrap_or_else(|| (&github::status(crate::util::run_login(github::VERSION, &dir, ""), crate::util::run_login(github::AUTH, &dir, ""))).into());
            let accounts: Vec<Gh> = signed.accounts.iter().map(|a| Gh { account: Some(a.clone()), ..base.clone() }).collect();
            let found = (accounts.len() > 1).then(|| detect(&dir, &accounts, &signed.active)).flatten();
            (read.then_some(signed), found)
        });
        cx.spawn(async move |this, cx| {
            let (signed, found) = task.await;
            this.update(cx, |d, cx| {
                if let Some(signed) = signed {
                    d.gh_accounts.read_host(&project, signed);
                }
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
    use super::{GhAccounts, Signed};
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
    fn a_host_projects_account_is_one_signed_in_on_that_host() {
        let (mut gh, now) = (signed_in(&["me", "mac-work"]), Instant::now());
        let host: Vec<String> = vec!["//h/app".into()];
        let mut chosen = repos(&[("//h/app", "mac-work")]);
        assert_eq!(gh.chosen(&chosen, "//h/app"), None);
        assert_eq!(gh.due(&host, &chosen, now).as_deref(), Some("//h/app"));
        gh.start("//h/app", now);
        gh.read_host("//h/app", Signed { accounts: vec!["me".into(), "host-work".into()], active: "me".into() });
        chosen.clear();
        assert!(gh.finish("//h/app", Some("host-work".into()), &mut chosen));
        assert_eq!(gh.chosen(&chosen, "//h/app"), Some("host-work"));
        assert_eq!(gh.chosen(&repos(&[("//h/app", "mac-work")]), "//h/app"), None);
        assert_eq!(gh.accounts_of("//h/app").map(|s| s.active.as_str()), Some("me"));
    }

    #[test]
    fn a_host_with_one_account_is_not_compared_until_it_reconnects() {
        let (mut gh, now) = (signed_in(&["me", "work"]), Instant::now());
        let host: Vec<String> = vec!["//h/app".into()];
        gh.start("//h/app", now);
        gh.read_host("//h/app", Signed { accounts: vec!["me".into()], active: "me".into() });
        gh.finish("//h/app", None, &mut repos(&[]));
        assert_eq!(gh.due(&host, &repos(&[]), now), None);
        gh.reconnected("h");
        assert_eq!(gh.due(&host, &repos(&[]), now).as_deref(), Some("//h/app"));
    }

    #[test]
    fn a_host_tree_waits_for_its_hosts_accounts_before_gh_is_asked_about_it() {
        let mut gh = signed_in(&["me", "work"]);
        assert!(gh.accounts_of("/a/tree").is_some());
        assert!(gh.accounts_of("//h/app-login").is_none());
        gh.read_host("//h/app", Signed { accounts: vec!["me".into(), "work".into()], active: "me".into() });
        assert!(gh.accounts_of("//h/app-login").is_some());
        gh.reconnected("h");
        assert!(gh.accounts_of("//h/app-login").is_none());
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
