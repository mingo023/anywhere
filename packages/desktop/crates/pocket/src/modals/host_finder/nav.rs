//! Where the host finder stands: the folder picked on a host, the columns down to it, and their listings as they come back.

use crate::modals::add_project::host_folder::{child, parent};
use crate::util::folder;
use hosts::remote::Entry;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// A file bigger than this isn't read for its preview.
pub(crate) const PREVIEW_MAX: usize = 256 * 1024;
/// Lines of a preview kept.
const PREVIEW_LINES: usize = 400;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Row {
    pub name: String,
    pub dir: bool,
    pub ignored: bool,
    pub size: u64,
}

/// One folder's entries, folders first by name, and whether a `.git` sits in it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Listing {
    all: Vec<Row>,
    pub shown: Rc<Vec<Row>>,
    pub repo: bool,
}

impl Listing {
    fn new(entries: Vec<Entry>, hidden: bool) -> Self {
        // A worktree's `.git` is a file; it's a repository all the same.
        let repo = entries.iter().any(|e| e.name == ".git");
        let mut all: Vec<Row> = entries.into_iter().map(|e| Row { name: e.name, dir: e.dir, ignored: e.ignored, size: e.size }).collect();
        all.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        let mut listing = Self { all, shown: Rc::default(), repo };
        listing.show(hidden);
        listing
    }

    fn show(&mut self, hidden: bool) {
        self.shown = Rc::new(self.all.iter().filter(|r| hidden || !r.name.starts_with('.')).cloned().collect());
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Preview {
    Loading,
    Text(Rc<Vec<String>>),
    Binary,
    TooBig(u64),
    Failed(String),
}

impl Preview {
    fn of(bytes: &[u8]) -> Self {
        match std::str::from_utf8(bytes) {
            Ok(text) if !text.contains('\0') => Self::Text(Rc::new(text.lines().take(PREVIEW_LINES).map(|l| l.replace('\t', "    ")).collect())),
            _ => Self::Binary,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct Repo {
    pub branch: Option<String>,
    pub remote: Option<String>,
}

/// The branch `.git/HEAD` names; none when it's detached.
pub(crate) fn branch_of(head: &str) -> Option<String> {
    head.trim().strip_prefix("ref: refs/heads/").map(str::to_string)
}

/// origin's url in `.git/config`, else the first remote's.
pub(crate) fn remote_of(config: &str) -> Option<String> {
    let (mut section, mut first) = ("", None);
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') {
            section = line;
            continue;
        }
        let Some(url) = line.strip_prefix("url").map(str::trim_start).and_then(|r| r.strip_prefix('=')).map(str::trim) else { continue };
        if section == "[remote \"origin\"]" {
            return Some(url.into());
        }
        if section.starts_with("[remote ") {
            first.get_or_insert_with(|| url.to_string());
        }
    }
    first
}

/// Whether `dir` is `base` or inside it.
fn within(base: &str, dir: &str) -> bool {
    let base = base.trim_end_matches('/');
    dir == base || dir.starts_with(&format!("{base}/")) || base.is_empty()
}

/// The root `dir` is under: `/`, or a Windows drive's.
pub(crate) fn root_of(dir: &str) -> String {
    let mut d = dir.to_string();
    loop {
        let up = parent(&d);
        if up == d {
            return up;
        }
        d = up;
    }
}

pub(crate) struct Finder {
    pub host: String,
    /// The host's home folder, once its pocketd said.
    pub home: Option<String>,
    /// Why the host couldn't be browsed.
    pub error: Option<String>,
    /// The first column's folder: home, or the root for a folder outside it.
    pub base: String,
    /// The folder picked; its entries are the last column.
    pub dir: String,
    pub file: Option<String>,
    pub hidden: bool,
    listings: HashMap<String, Result<Listing, String>>,
    asked: HashSet<String>,
    pub preview: Option<(String, Preview)>,
    /// The picked repository's branch and remote, `None` until read.
    pub repo: Option<(String, Option<Repo>)>,
}

impl Finder {
    pub fn new(host: &str) -> Self {
        Self {
            host: host.into(),
            home: None,
            error: None,
            base: String::new(),
            dir: String::new(),
            file: None,
            hidden: false,
            listings: HashMap::new(),
            asked: HashSet::new(),
            preview: None,
            repo: None,
        }
    }

    /// Learns the host's home and stands at `start`, else home.
    pub fn homed(&mut self, home: String, start: Option<String>) {
        let start = start.filter(|s| !s.is_empty()).unwrap_or_else(|| home.clone());
        self.home = Some(home);
        self.go(start);
    }

    /// Jumps to `dir`, its columns starting at home when it's inside it, else at its root.
    pub fn go(&mut self, dir: String) {
        self.base = match &self.home {
            Some(home) if within(home, &dir) => home.clone(),
            _ => root_of(&dir),
        };
        self.dir = dir;
        self.file = None;
    }

    /// Each column's folder, from the base down to the picked one.
    pub fn chain(&self) -> Vec<String> {
        if self.dir.is_empty() {
            return Vec::new();
        }
        let mut chain = vec![self.dir.clone()];
        loop {
            let last = chain.last().unwrap();
            let up = parent(last);
            if *last == self.base || up == *last {
                break;
            }
            chain.push(up);
        }
        chain.reverse();
        chain
    }

    /// The name picked in the column of `chain[i]`.
    pub fn picked_in(&self, chain: &[String], i: usize) -> Option<String> {
        match chain.get(i + 1) {
            Some(next) => Some(folder(next).to_string()),
            None => self.file.clone(),
        }
    }

    pub fn listing(&self, dir: &str) -> Option<&Result<Listing, String>> {
        self.listings.get(dir)
    }

    fn rows(&self, dir: &str) -> Option<&[Row]> {
        self.listings.get(dir)?.as_ref().ok().map(|l| l.shown.as_slice())
    }

    pub fn is_repo(&self, dir: &str) -> bool {
        self.listings.get(dir).is_some_and(|l| l.as_ref().is_ok_and(|l| l.repo))
    }

    /// The columns' folders not yet asked for, each handed out once.
    pub fn wanted(&mut self) -> Vec<String> {
        let wanted: Vec<String> = self.chain().into_iter().filter(|d| !self.listings.contains_key(d) && !self.asked.contains(d)).collect();
        self.asked.extend(wanted.iter().cloned());
        wanted
    }

    /// Takes `dir`'s listing on `host`, unless the finder has moved to another host since asking.
    pub fn listed(&mut self, host: &str, dir: &str, entries: Result<Vec<Entry>, String>) -> bool {
        if host != self.host {
            return false;
        }
        self.asked.remove(dir);
        self.listings.insert(dir.into(), entries.map(|e| Listing::new(e, self.hidden)));
        true
    }

    /// Picks `name` in the column of `column`: a folder opens the next column, a file is previewed.
    pub fn select(&mut self, column: &str, name: &str) {
        let Some(dir) = self.rows(column).and_then(|rows| rows.iter().find(|r| r.name == name)).map(|r| r.dir) else { return };
        if dir {
            self.dir = child(column, name);
            self.file = None;
        } else {
            self.dir = column.into();
            self.file = Some(name.into());
        }
    }

    /// The column the keyboard moves in and what's picked there.
    fn current(&self) -> Option<(String, Option<String>)> {
        if self.dir.is_empty() {
            return None;
        }
        if let Some(f) = &self.file {
            return Some((self.dir.clone(), Some(f.clone())));
        }
        if self.dir != self.base {
            return Some((parent(&self.dir), Some(folder(&self.dir).to_string())));
        }
        Some((self.dir.clone(), None))
    }

    /// Where the keyboard's pick is: its column's folder and row.
    pub fn cursor(&self) -> Option<(String, usize)> {
        let (column, picked) = self.current()?;
        let at = self.rows(&column)?.iter().position(|r| Some(&r.name) == picked.as_ref())?;
        Some((column, at))
    }

    /// Moves the pick for an arrow key: up and down in its column, right into a folder, left back out. False when it can't.
    pub fn step(&mut self, key: &str) -> bool {
        let Some((column, picked)) = self.current() else { return false };
        match key {
            "up" | "down" => {
                let Some(rows) = self.rows(&column).filter(|r| !r.is_empty()) else { return false };
                let at = picked.and_then(|p| rows.iter().position(|r| r.name == p));
                let to = match at {
                    None => 0,
                    Some(i) if key == "down" => (i + 1).min(rows.len() - 1),
                    Some(i) => i.saturating_sub(1),
                };
                let name = rows[to].name.clone();
                self.select(&column, &name);
            }
            "right" => {
                let dir = self.dir.clone();
                let first = self.rows(&dir).and_then(|r| r.first()).map(|r| r.name.clone());
                let (None, Some(first)) = (&self.file, first) else { return false };
                self.select(&dir, &first);
            }
            "left" => {
                if self.file.take().is_none() {
                    if self.dir == self.base {
                        return false;
                    }
                    self.dir = parent(&self.dir);
                }
            }
            _ => return false,
        }
        true
    }

    /// Shows or hides dot files; a hidden one picked gives way to the folder holding it.
    pub fn toggle_hidden(&mut self) {
        self.hidden = !self.hidden;
        for listing in self.listings.values_mut().filter_map(|l| l.as_mut().ok()) {
            listing.show(self.hidden);
        }
        if self.hidden {
            return;
        }
        if let Some(first) = self.chain().into_iter().skip(1).find(|d| folder(d).starts_with('.')) {
            self.dir = parent(&first);
            self.file = None;
        }
        if self.file.as_deref().is_some_and(|f| f.starts_with('.')) {
            self.file = None;
        }
    }

    /// The picked file's path when it's small enough and not read yet; one too big is marked so instead.
    pub fn preview_wanted(&mut self) -> Option<String> {
        let file = self.file.clone()?;
        let path = child(&self.dir, &file);
        if self.preview.as_ref().is_some_and(|(p, _)| *p == path) {
            return None;
        }
        let size = self.rows(&self.dir)?.iter().find(|r| r.name == file)?.size;
        if size as usize > PREVIEW_MAX {
            self.preview = Some((path, Preview::TooBig(size)));
            return None;
        }
        self.preview = Some((path.clone(), Preview::Loading));
        Some(path)
    }

    /// Takes `path`'s bytes, unless another file is previewed by now.
    pub fn previewed(&mut self, host: &str, path: &str, read: Result<Vec<u8>, String>) {
        if host == self.host && self.preview.as_ref().is_some_and(|(p, _)| p == path) {
            self.preview = Some((path.into(), read.map_or_else(Preview::Failed, |b| Preview::of(&b))));
        }
    }

    /// The picked folder, when it's a repository whose branch and remote aren't read yet.
    pub fn repo_wanted(&mut self) -> Option<String> {
        if self.file.is_some() || !self.is_repo(&self.dir) || self.repo.as_ref().is_some_and(|(d, _)| *d == self.dir) {
            return None;
        }
        self.repo = Some((self.dir.clone(), None));
        Some(self.dir.clone())
    }

    pub fn repo_read(&mut self, host: &str, dir: &str, repo: Repo) {
        if host == self.host && self.repo.as_ref().is_some_and(|(d, _)| d == dir) {
            self.repo = Some((dir.into(), Some(repo)));
        }
    }

    /// The path bar's parts as `(label, folder)`: from `~` when the pick is inside home, else from the root.
    pub fn crumbs(&self) -> Vec<(String, String)> {
        if self.dir.is_empty() {
            return Vec::new();
        }
        let home = self.home.as_deref().filter(|h| within(h, &self.dir));
        let mut parts = Vec::new();
        let mut d = self.dir.clone();
        loop {
            if Some(d.as_str()) == home {
                parts.push(("~".to_string(), d));
                break;
            }
            let up = parent(&d);
            if up == d {
                let label = match d.trim_end_matches('/') {
                    "" => "/".to_string(),
                    drive => drive.to_string(),
                };
                parts.push((label, d));
                break;
            }
            parts.push((folder(&d).to_string(), d));
            d = up;
        }
        parts.reverse();
        parts
    }

    /// The pick as the path bar would print it, `~` for home.
    pub fn pretty(&self, dir: &str) -> String {
        match self.home.as_deref() {
            Some(home) if within(home, dir) => format!("~{}", &dir[home.trim_end_matches('/').len()..]),
            _ => dir.to_string(),
        }
    }
}

/// What the finder's footer says of the folder picked.
#[derive(Debug, PartialEq)]
pub(crate) enum Hint {
    /// The folder is already a project.
    Taken,
    Repo,
    Plain,
    /// Picking where to clone: the repository's name goes under the folder.
    Clones(String),
}

/// How the finder is worded, and whether its primary button can be pressed.
#[derive(Debug, PartialEq)]
pub(crate) struct Ask {
    pub title: &'static str,
    pub primary: &'static str,
    pub hint: Option<Hint>,
    pub ready: bool,
}

/// The footer for the folder `dir` on a host that `online` is or not. `clone` is the repository's name when the finder picks where to clone, where a folder that is a project is fine; `taken` and `repo` say what `dir` is.
pub(crate) fn ask(clone: Option<&str>, online: bool, dir: &str, taken: bool, repo: bool) -> Ask {
    let picked = online && !dir.is_empty();
    match clone {
        Some(name) => Ask { title: "Clone into", primary: "Clone here", hint: picked.then(|| Hint::Clones(name.to_string())), ready: picked },
        None => {
            let hint = picked.then_some(if taken { Hint::Taken } else if repo { Hint::Repo } else { Hint::Plain });
            Ask { title: "Choose a folder", primary: "Use this folder", hint, ready: picked && !taken }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, dir: bool) -> Entry {
        Entry { name: name.into(), dir, size: 10, ..Default::default() }
    }

    /// A finder on devbox standing in `/home/me/code`, with its columns listed.
    fn finder() -> Finder {
        let mut f = Finder::new("devbox");
        f.homed("/home/me".into(), Some("/home/me/code".into()));
        f.listed("devbox", "/home/me", Ok(vec![entry("code", true), entry(".ssh", true), entry("notes.md", false)]));
        f.listed("devbox", "/home/me/code", Ok(vec![entry("app", true), entry("api", true), entry("todo.txt", false)]));
        f.listed("devbox", "/home/me/code/app", Ok(vec![entry(".git", true), entry("src", true), entry("README.md", false)]));
        f
    }

    #[test]
    fn picking_where_to_clone_says_clone_here_and_takes_a_folder_that_is_a_project() {
        let pick = ask(Some("zed"), true, "/home/me/code", true, true);
        assert_eq!((pick.title, pick.primary, pick.ready), ("Clone into", "Clone here", true));
        assert_eq!(pick.hint, Some(Hint::Clones("zed".into())));
        let open = ask(None, true, "/home/me/code", true, true);
        assert_eq!((open.title, open.primary, open.ready, open.hint), ("Choose a folder", "Use this folder", false, Some(Hint::Taken)));
    }

    #[test]
    fn the_footer_waits_for_a_folder_on_a_host_that_answers() {
        assert_eq!(ask(Some("zed"), true, "", false, false), Ask { title: "Clone into", primary: "Clone here", hint: None, ready: false });
        assert!(!ask(Some("zed"), false, "/home/me", false, false).ready);
        assert_eq!(ask(None, true, "/home/me/app", false, true).hint, Some(Hint::Repo));
        assert_eq!(ask(None, true, "/home/me/code", false, false).hint, Some(Hint::Plain));
    }

    #[test]
    fn columns_run_from_home_down_to_the_picked_folder() {
        let f = finder();
        assert_eq!(f.chain(), ["/home/me", "/home/me/code"]);
        assert_eq!(f.picked_in(&f.chain(), 0).as_deref(), Some("code"));
        assert_eq!(f.picked_in(&f.chain(), 1), None);
    }

    #[test]
    fn a_folder_outside_home_starts_the_columns_at_the_root() {
        let mut f = finder();
        f.go("/etc/ssh".into());
        assert_eq!(f.chain(), ["/", "/etc", "/etc/ssh"]);
        let mut pc = Finder::new("pc");
        pc.homed("C:/Users/me".into(), Some("D:/work".into()));
        assert_eq!(pc.chain(), ["D:/", "D:/work"]);
    }

    #[test]
    fn folders_come_first_and_dot_files_stay_out_until_asked_for() {
        let mut f = finder();
        let names = |f: &Finder| f.rows("/home/me").unwrap().iter().map(|r| r.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&f), ["code", "notes.md"]);
        f.toggle_hidden();
        assert_eq!(names(&f), [".ssh", "code", "notes.md"]);
    }

    #[test]
    fn hiding_dot_files_again_leaves_a_hidden_folder_for_the_one_holding_it() {
        let mut f = finder();
        f.toggle_hidden();
        f.select("/home/me", ".ssh");
        f.toggle_hidden();
        assert_eq!((f.dir.as_str(), f.file.as_deref()), ("/home/me", None));
    }

    #[test]
    fn a_folder_holding_dot_git_is_a_repository_even_when_dot_git_is_a_file() {
        let mut f = finder();
        assert!(f.is_repo("/home/me/code/app"));
        assert!(!f.is_repo("/home/me/code"));
        f.listed("devbox", "/home/me/code/wt", Ok(vec![entry(".git", false)]));
        assert!(f.is_repo("/home/me/code/wt"));
    }

    #[test]
    fn each_folder_is_asked_for_once_and_a_listing_from_another_host_is_dropped() {
        let mut f = Finder::new("devbox");
        f.homed("/home/me".into(), Some("/home/me/code".into()));
        assert_eq!(f.wanted(), ["/home/me", "/home/me/code"]);
        assert!(f.wanted().is_empty(), "still being listed");
        assert!(!f.listed("other", "/home/me", Ok(vec![entry("x", true)])));
        assert!(f.listing("/home/me").is_none());
        assert!(f.listed("devbox", "/home/me", Err("denied".into())));
        assert_eq!(f.listing("/home/me"), Some(&Err("denied".into())));
    }

    #[test]
    fn a_file_left_before_its_read_lands_shows_that_read_when_picked_again() {
        let mut f = finder();
        f.select("/home/me/code", "todo.txt");
        let path = f.preview_wanted().unwrap();
        f.select("/home/me", "code");
        f.previewed("devbox", &path, Ok(b"done\n".to_vec()));
        f.select("/home/me/code", "todo.txt");
        assert_eq!(f.preview_wanted(), None);
        assert_eq!(f.preview.unwrap().1, Preview::Text(Rc::new(vec!["done".into()])));
    }

    #[test]
    fn picking_a_file_previews_it_and_keeps_its_folder_as_the_pick() {
        let mut f = finder();
        f.select("/home/me/code", "todo.txt");
        assert_eq!((f.dir.as_str(), f.file.as_deref()), ("/home/me/code", Some("todo.txt")));
        assert_eq!(f.preview_wanted().as_deref(), Some("/home/me/code/todo.txt"));
        assert_eq!(f.preview_wanted(), None, "asked once");
        f.previewed("devbox", "/home/me/code/todo.txt", Ok(b"one\ttab\ntwo\n".to_vec()));
        assert_eq!(f.preview.unwrap().1, Preview::Text(Rc::new(vec!["one    tab".into(), "two".into()])));
    }

    #[test]
    fn a_file_too_big_is_not_read_and_a_binary_one_has_no_text() {
        let mut f = finder();
        f.listed("devbox", "/home/me/dl", Ok(vec![Entry { name: "node.tar".into(), size: 30_000_000, ..Default::default() }]));
        f.go("/home/me/dl".into());
        f.select("/home/me/dl", "node.tar");
        assert_eq!(f.preview_wanted(), None);
        assert_eq!(f.preview.as_ref().unwrap().1, Preview::TooBig(30_000_000));
        assert_eq!(Preview::of(&[0x89, 0x50, 0, 1]), Preview::Binary);
    }

    #[test]
    fn a_preview_for_a_file_another_file_replaced_is_dropped() {
        let mut f = finder();
        f.listed("devbox", "/home/me/code", Ok(vec![entry("todo.txt", false), entry("notes.txt", false)]));
        f.select("/home/me/code", "todo.txt");
        f.preview_wanted();
        f.select("/home/me/code", "notes.txt");
        f.preview_wanted();
        f.previewed("devbox", "/home/me/code/todo.txt", Ok(b"late".to_vec()));
        assert_eq!(f.preview.unwrap(), ("/home/me/code/notes.txt".into(), Preview::Loading));
    }

    #[test]
    fn arrow_keys_walk_down_into_and_back_out_of_folders() {
        let mut f = finder();
        assert!(f.step("right"));
        assert_eq!(f.dir, "/home/me/code/api");
        assert!(f.step("down"));
        assert_eq!(f.dir, "/home/me/code/app");
        assert!(f.step("down"));
        assert_eq!((f.dir.as_str(), f.file.as_deref()), ("/home/me/code", Some("todo.txt")));
        assert!(!f.step("right"), "a file has nothing inside");
        assert!(f.step("left"));
        assert_eq!((f.dir.as_str(), f.file.as_deref()), ("/home/me/code", None));
        assert_eq!(f.cursor(), Some(("/home/me".into(), 0)));
        assert!(f.step("left"));
        assert!(!f.step("left"), "home is the first column");
    }

    #[test]
    fn a_repository_picked_is_read_for_its_branch_and_remote_once() {
        let mut f = finder();
        f.select("/home/me/code", "app");
        assert_eq!(f.repo_wanted().as_deref(), Some("/home/me/code/app"));
        assert_eq!(f.repo_wanted(), None);
        f.repo_read("devbox", "/home/me/code/app", Repo { branch: Some("main".into()), remote: None });
        assert_eq!(f.repo, Some(("/home/me/code/app".into(), Some(Repo { branch: Some("main".into()), remote: None }))));
        f.select("/home/me/code", "api");
        assert_eq!(f.repo_wanted(), None, "not a repository");
    }

    #[test]
    fn the_branch_comes_from_head_and_the_remote_prefers_origin() {
        assert_eq!(branch_of("ref: refs/heads/feat/x\n").as_deref(), Some("feat/x"));
        assert_eq!(branch_of("3f2a9c1\n"), None);
        let config = "[core]\n\tbare = false\n[remote \"upstream\"]\n\turl = git@github.com:up/app.git\n[remote \"origin\"]\n\turl = https://github.com/me/app.git\n";
        assert_eq!(remote_of(config).as_deref(), Some("https://github.com/me/app.git"));
        assert_eq!(remote_of("[remote \"fork\"]\n url=git@x:y.git\n").as_deref(), Some("git@x:y.git"));
        assert_eq!(remote_of("[core]\n\turl = nope\n"), None);
    }

    #[test]
    fn the_path_bar_starts_at_home_inside_it_and_at_the_root_outside() {
        let f = finder();
        assert_eq!(f.crumbs(), [("~".to_string(), "/home/me".to_string()), ("code".into(), "/home/me/code".into())]);
        assert_eq!(f.pretty("/home/me/code"), "~/code");
        let mut f = finder();
        f.go("/etc".into());
        assert_eq!(f.crumbs(), [("/".to_string(), "/".to_string()), ("etc".into(), "/etc".into())]);
        let mut pc = Finder::new("pc");
        pc.homed("C:/Users/me".into(), Some("C:/tools".into()));
        assert_eq!(pc.crumbs(), [("C:".to_string(), "C:/".to_string()), ("tools".into(), "C:/tools".into())]);
    }
}
