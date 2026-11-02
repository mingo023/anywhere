use crate::desktop::chrome::id;
use crate::util::basename;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::time::Instant;
use theme::Token;

/// What a rename changed: a worktree's given name, or its branch.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Slot {
    Name,
    Branch,
}

struct Renamed {
    from: String,
    to: String,
    at: Instant,
}

impl Renamed {
    fn fresh(&self, now: Instant) -> bool {
        now < self.at + ui::scramble_duration(&self.to)
    }
}

/// Names and branches pocketd just gave worktrees, decoding on screen out of the ones they replaced.
#[derive(Default)]
pub(crate) struct Renames {
    shown: HashMap<(String, Slot), Renamed>,
    /// The pocketds whose names arrived since they connected; the first list restates what was already there.
    heard: HashSet<Option<String>>,
}

impl Renames {
    /// pocketd `source` connected: its next names list is everything it holds, not a rename.
    pub(crate) fn connected(&mut self, source: Option<&str>) {
        self.heard.remove(&source.map(str::to_string));
    }

    /// `source`'s names went from `before` to `after`; `place` is what a tree shows while it has no name.
    pub(crate) fn names(&mut self, source: Option<&str>, before: &HashMap<String, String>, after: &HashMap<String, String>, place: impl Fn(&str) -> Option<String>, now: Instant) {
        if self.heard.insert(source.map(str::to_string)) {
            return;
        }
        for (path, to) in after {
            let was = before.get(path).filter(|n| !n.is_empty());
            if to.is_empty() || was == Some(to) {
                continue;
            }
            if let Some(from) = was.cloned().or_else(|| place(path)) {
                self.start(path, Slot::Name, from, to.clone(), now);
            }
        }
    }

    /// Worktrees were relisted. A branch moving off its folder's name is pocketd renaming a new tree's placeholder; other switches aren't renames.
    pub(crate) fn branches<'a>(&mut self, before: impl IntoIterator<Item = &'a git::Worktree>, after: impl IntoIterator<Item = &'a git::Worktree>, now: Instant) {
        let was: HashMap<&str, &str> = before.into_iter().map(|w| (w.path.as_str(), w.branch.as_str())).collect();
        for w in after {
            if let Some(&from) = was.get(w.path.as_str())
                && from != w.branch
                && from == basename(&w.path)
            {
                self.start(&w.path, Slot::Branch, from.to_string(), w.branch.clone(), now);
            }
        }
    }

    fn start(&mut self, path: &str, slot: Slot, from: String, to: String, now: Instant) {
        self.shown.retain(|_, r| r.fresh(now));
        self.shown.insert((path.to_string(), slot), Renamed { from, to, at: now });
    }

    /// What `path`'s `slot` showed before it became `text`, while that rename still decodes.
    pub(crate) fn from(&self, path: &str, slot: Slot, text: &str, now: Instant) -> Option<&str> {
        self.shown.get(&(path.to_string(), slot)).filter(|r| r.to == text && r.fresh(now)).map(|r| r.from.as_str())
    }

    /// `branch`, decoding out of the placeholder it was renamed from while that's fresh.
    pub(crate) fn branch(&self, key: &str, tree: &str, branch: &str, base: Token, reduce_motion: bool) -> AnyElement {
        match self.from(tree, Slot::Branch, branch, Instant::now()).filter(|_| !reduce_motion) {
            Some(from) => ui::scramble(id(format!("{key}:{tree}:{branch}")), from.to_string().into(), branch.to_string().into(), base).into_any_element(),
            None => branch.to_string().into_any_element(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Renames, Slot};
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    fn names(list: &[(&str, &str)]) -> HashMap<String, String> {
        list.iter().map(|(p, n)| (p.to_string(), n.to_string())).collect()
    }

    fn tree(path: &str, branch: &str) -> git::Worktree {
        git::Worktree { path: path.into(), branch: branch.into(), main: false }
    }

    fn placed(path: &str) -> Option<String> {
        (path == "/w/calm-otter").then(|| "calm-otter".into())
    }

    #[test]
    fn a_name_pocketd_gives_after_connecting_decodes_out_of_the_branch_it_replaces() {
        let (mut r, t0) = (Renames::default(), Instant::now());
        r.names(None, &names(&[]), &names(&[]), placed, t0);
        r.names(None, &names(&[]), &names(&[("/w/calm-otter", "Fix login redirect")]), placed, t0);
        assert_eq!(r.from("/w/calm-otter", Slot::Name, "Fix login redirect", t0), Some("calm-otter"));
    }

    #[test]
    fn the_names_list_a_connection_opens_with_is_not_a_rename() {
        let (mut r, t0) = (Renames::default(), Instant::now());
        r.names(None, &names(&[]), &names(&[("/w/calm-otter", "Fix login redirect")]), placed, t0);
        assert_eq!(r.from("/w/calm-otter", Slot::Name, "Fix login redirect", t0), None);
        r.connected(None);
        r.names(None, &names(&[]), &names(&[("/w/calm-otter", "Fix login redirect")]), placed, t0);
        assert_eq!(r.from("/w/calm-otter", Slot::Name, "Fix login redirect", t0), None);
    }

    #[test]
    fn a_name_already_shown_stays_still_when_pocketd_echoes_it() {
        let (mut r, t0) = (Renames::default(), Instant::now());
        let named = names(&[("/w/calm-otter", "Fix login redirect")]);
        r.names(None, &named, &named, placed, t0);
        r.names(None, &named, &named, placed, t0);
        assert_eq!(r.from("/w/calm-otter", Slot::Name, "Fix login redirect", t0), None);
    }

    #[test]
    fn a_branch_renamed_off_its_folder_s_name_decodes_until_it_settles() {
        let (mut r, t0) = (Renames::default(), Instant::now());
        r.branches(&[tree("/w/calm-otter", "calm-otter")], &[tree("/w/calm-otter", "fix-login")], t0);
        assert_eq!(r.from("/w/calm-otter", Slot::Branch, "fix-login", t0), Some("calm-otter"));
        assert_eq!(r.from("/w/calm-otter", Slot::Branch, "fix-login", t0 + Duration::from_secs(2)), None);
    }

    #[test]
    fn switching_a_named_branch_is_not_a_rename() {
        let (mut r, t0) = (Renames::default(), Instant::now());
        r.branches(&[tree("/w/calm-otter", "fix-login")], &[tree("/w/calm-otter", "main")], t0);
        assert_eq!(r.from("/w/calm-otter", Slot::Branch, "main", t0), None);
    }
}
