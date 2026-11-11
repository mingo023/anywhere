use crate::desktop::chrome::id;
use crate::util::basename;
use agents::{Agents, Event};
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::time::Instant;
use theme::Token;

/// What a rename changed: a tree's given name, or its branch.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Slot {
    Name,
    Branch,
}

/// Where pocketd sends tree names: worktrees' in `worktree.names`, Locals' in their list.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Feed {
    Names,
    Locals,
}

impl Feed {
    pub(crate) fn of(ev: &Event) -> Option<Feed> {
        match ev {
            Event::Names(_) => Some(Feed::Names),
            Event::Locals(_) => Some(Feed::Locals),
            _ => None,
        }
    }

    /// The names this feed has given, by tree key.
    pub(crate) fn read(self, agents: &Agents) -> HashMap<String, String> {
        match self {
            Feed::Names => agents.names.clone(),
            Feed::Locals => agents.locals.iter().map(|l| (l.id.clone(), l.name.clone())).collect(),
        }
    }
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

/// Names and branches pocketd just gave trees, decoding on screen out of the ones they replaced.
#[derive(Default)]
pub(crate) struct Renames {
    names: HashMap<String, Renamed>,
    branches: HashMap<String, Renamed>,
    /// The feeds heard from each pocketd since it connected; a feed's first list restates what was already there.
    heard: HashSet<(Feed, Option<String>)>,
}

impl Renames {
    /// pocketd `source` connected: its next lists hold everything it has, not renames.
    pub(crate) fn connected(&mut self, source: Option<&str>) {
        self.heard.retain(|(_, s)| s.as_deref() != source);
    }

    /// `source`'s `feed` went from `before` to `after`; `unnamed` is what a tree reads while it has no name.
    pub(crate) fn named(&mut self, feed: Feed, source: Option<&str>, before: &HashMap<String, String>, after: &HashMap<String, String>, unnamed: impl Fn(&str) -> Option<String>, now: Instant) {
        if self.heard.insert((feed, source.map(str::to_string))) {
            return;
        }
        for (key, to) in after {
            let was = before.get(key).filter(|n| !n.is_empty());
            if to.is_empty() || was == Some(to) {
                continue;
            }
            if let Some(from) = was.cloned().or_else(|| unnamed(key)) {
                start(&mut self.names, key, from, to.clone(), now);
            }
        }
    }

    /// Worktrees were relisted. A branch moving off its folder's name is pocketd renaming a new tree's placeholder; other switches aren't renames.
    pub(crate) fn relisted<'a>(&mut self, before: impl IntoIterator<Item = &'a git::Worktree>, after: impl IntoIterator<Item = &'a git::Worktree>, now: Instant) {
        let was: HashMap<&str, &str> = before.into_iter().map(|w| (w.path.as_str(), w.branch.as_str())).collect();
        for w in after {
            if let Some(&from) = was.get(w.path.as_str())
                && from != w.branch
                && from == basename(&w.path)
            {
                start(&mut self.branches, &w.path, from.to_string(), w.branch.clone(), now);
            }
        }
    }

    /// What `key`'s `slot` read before it became `text`, and when, while that rename still decodes.
    pub(crate) fn was(&self, key: &str, slot: Slot, text: &str, now: Instant) -> Option<(&str, Instant)> {
        let renames = match slot {
            Slot::Name => &self.names,
            Slot::Branch => &self.branches,
        };
        renames.get(key).filter(|r| r.to == text && r.fresh(now)).map(|r| (r.from.as_str(), r.at))
    }
}

fn start(renames: &mut HashMap<String, Renamed>, key: &str, from: String, to: String, now: Instant) {
    renames.retain(|_, r| r.fresh(now));
    renames.insert(key.to_string(), Renamed { from, to, at: now });
}

/// `text` in `base`, decoding out of what it replaced while a rename is fresh; `None` otherwise, or with motion reduced.
pub(crate) fn decoding(key: String, was: Option<(&str, Instant)>, text: &str, base: Token, reduce_motion: bool) -> Option<AnyElement> {
    let (from, at) = was.filter(|_| !reduce_motion)?;
    Some(ui::scramble(id(key), from.to_string().into(), text.to_string().into(), base, at).into_any_element())
}

/// `tree`'s `branch`, decoding out of the placeholder it was renamed from while that's fresh.
pub(crate) fn branch_label(renames: &Renames, key: &str, tree: &str, branch: &str, base: Token, reduce_motion: bool) -> AnyElement {
    let was = renames.was(tree, Slot::Branch, branch, Instant::now());
    decoding(format!("{key}:{tree}:{branch}"), was, branch, base, reduce_motion).unwrap_or_else(|| branch.to_string().into_any_element())
}

#[cfg(test)]
mod tests {
    use super::{Feed, Renames, Slot};
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    fn names(list: &[(&str, &str)]) -> HashMap<String, String> {
        list.iter().map(|(p, n)| (p.to_string(), n.to_string())).collect()
    }

    fn tree(path: &str, branch: &str) -> git::Worktree {
        git::Worktree { path: path.into(), branch: branch.into(), main: false, made: false }
    }

    fn unnamed(path: &str) -> Option<String> {
        (path == "/w/calm-otter").then(|| "calm-otter".into())
    }

    fn from(r: &Renames, slot: Slot, text: &str, now: Instant) -> Option<String> {
        r.was("/w/calm-otter", slot, text, now).map(|(from, _)| from.to_string())
    }

    #[test]
    fn a_name_pocketd_gives_after_connecting_decodes_out_of_the_branch_it_replaces() {
        let (mut r, t0) = (Renames::default(), Instant::now());
        r.named(Feed::Names, None, &names(&[]), &names(&[]), unnamed, t0);
        r.named(Feed::Names, None, &names(&[]), &names(&[("/w/calm-otter", "Fix login redirect")]), unnamed, t0);
        assert_eq!(from(&r, Slot::Name, "Fix login redirect", t0).as_deref(), Some("calm-otter"));
    }

    #[test]
    fn a_second_name_decodes_out_of_the_first() {
        let (mut r, t0) = (Renames::default(), Instant::now());
        let first = names(&[("/w/calm-otter", "Fix login")]);
        r.named(Feed::Names, None, &first, &first, unnamed, t0);
        r.named(Feed::Names, None, &first, &names(&[("/w/calm-otter", "Fix login redirect")]), unnamed, t0);
        assert_eq!(from(&r, Slot::Name, "Fix login redirect", t0).as_deref(), Some("Fix login"));
    }

    #[test]
    fn the_lists_a_connection_opens_with_are_not_renames() {
        let (mut r, t0) = (Renames::default(), Instant::now());
        let named = names(&[("/w/calm-otter", "Fix login redirect")]);
        r.named(Feed::Names, None, &names(&[]), &named, unnamed, t0);
        r.named(Feed::Locals, None, &names(&[]), &names(&[("l1", "Local 2")]), unnamed, t0);
        assert_eq!(from(&r, Slot::Name, "Fix login redirect", t0), None);
        r.connected(None);
        r.named(Feed::Names, None, &names(&[]), &named, unnamed, t0);
        assert_eq!(from(&r, Slot::Name, "Fix login redirect", t0), None);
    }

    #[test]
    fn a_name_already_shown_stays_still_when_pocketd_echoes_it() {
        let (mut r, t0) = (Renames::default(), Instant::now());
        let named = names(&[("/w/calm-otter", "Fix login redirect")]);
        r.named(Feed::Names, None, &named, &named, unnamed, t0);
        r.named(Feed::Names, None, &named, &named, unnamed, t0);
        assert_eq!(from(&r, Slot::Name, "Fix login redirect", t0), None);
    }

    #[test]
    fn a_branch_renamed_off_its_folder_s_name_decodes_until_it_settles() {
        let (mut r, t0) = (Renames::default(), Instant::now());
        r.relisted(&[tree("/w/calm-otter", "calm-otter")], &[tree("/w/calm-otter", "fix-login")], t0);
        assert_eq!(from(&r, Slot::Branch, "fix-login", t0).as_deref(), Some("calm-otter"));
        assert_eq!(from(&r, Slot::Branch, "fix-login", t0 + Duration::from_secs(2)), None);
    }

    #[test]
    fn switching_a_named_branch_is_not_a_rename() {
        let (mut r, t0) = (Renames::default(), Instant::now());
        r.relisted(&[tree("/w/calm-otter", "fix-login")], &[tree("/w/calm-otter", "main")], t0);
        assert_eq!(from(&r, Slot::Branch, "main", t0), None);
    }
}
