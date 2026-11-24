use super::fold::Fold;
use super::sections::{Block, Item, Listed};
use crate::desktop::Desktop;
use gpui_kit::component::input::InputState;
use gpui_kit::*;

pub(super) const FIELD_H: f32 = 28.;

/// A row the filter lists: a project, or one of its trees by key.
pub(crate) type Hit = (String, Option<String>);

/// The Projects header's filter; while it's open, its field stands in for the header.
pub(crate) struct Filter {
    pub(crate) input: Entity<InputState>,
    pub(crate) open: bool,
    /// The row Enter picks.
    pub(crate) lit: Option<Hit>,
}

impl Filter {
    pub fn new<T: 'static>(window: &mut Window, cx: &mut Context<T>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Filter projects & worktrees"));
        Self { input, open: false, lit: None }
    }

    /// The filter's text, lowercased; empty while it's closed.
    pub(crate) fn query(&self, cx: &App) -> String {
        if self.open { self.input.read(cx).value().trim().to_lowercase() } else { String::new() }
    }

    pub(crate) fn lit(&self, project: &str, tree: Option<&str>) -> bool {
        self.lit.as_ref().is_some_and(|(p, t)| p == project && t.as_deref() == tree)
    }
}

/// Every row `listed` shows that matches the filter, in order.
fn hits(listed: &Listed) -> Vec<Hit> {
    let block = |b: &Block| {
        b.named.then(|| (b.project.clone(), None)).into_iter().chain(b.only.iter().flatten().map(|k| (b.project.clone(), Some(k.clone())))).collect::<Vec<_>>()
    };
    let pinned = listed.pinned.iter().flat_map(|i| match i {
        Item::Project(b) => block(b),
        Item::Tree { project, key } => vec![(project.clone(), Some(key.clone()))],
    });
    pinned.chain(listed.groups.iter().flat_map(|(_, bs)| bs.iter().flat_map(block))).chain(listed.other.iter().flat_map(block)).collect()
}

/// The hit after `lit`, or before it going up, wrapping round.
fn step(hits: &[Hit], lit: Option<&Hit>, down: bool) -> Option<Hit> {
    let n = hits.len();
    let at = lit.and_then(|l| hits.iter().position(|h| h == l));
    let next = match (at, down) {
        (None, _) => 0,
        (Some(i), true) => (i + 1) % n.max(1),
        (Some(i), false) => (i + n - 1) % n.max(1),
    };
    hits.get(next).cloned()
}

impl Desktop {
    /// Empty while the query is.
    pub(super) fn filter_hits(&self, cx: &App) -> Vec<Hit> {
        let query = self.sidebar.read(cx).query(cx);
        if query.is_empty() { vec![] } else { hits(&self.listed(&query, cx)) }
    }

    /// Starts shutting the folded folds a query held open.
    pub(super) fn shut_filtered_folds(&self, cx: &mut App) {
        let sections = &self.store.sections;
        let groups = sections.groups.iter().filter(|g| g.folded).map(|g| Fold::Group(g.id));
        let projects = self.store.collapsed.iter().map(|p| Fold::Project(p.clone()));
        let folds: Vec<Fold> =
            groups.chain(sections.pins_folded.then_some(Fold::Pinned)).chain(self.store.scratch_folded.then_some(Fold::Scratch)).chain(projects).collect();
        self.sidebar.update(cx, |s, _| folds.into_iter().for_each(|fold| s.closing.start(fold)));
    }

    /// Answers the filter's text changing: the row Enter picks is the first it lists.
    pub(super) fn filtered(&mut self, cx: &mut Context<Self>) {
        let lit = self.filter_hits(cx).into_iter().next();
        self.set_lit(lit, cx);
        if self.sidebar.read(cx).query(cx).is_empty() {
            self.shut_filtered_folds(cx);
        }
    }

    fn set_lit(&mut self, lit: Option<Hit>, cx: &mut Context<Self>) {
        self.sidebar.read(cx).header.clone().update(cx, |h, _| h.filter.lit = lit);
    }

    /// Answers up, down or Enter in the filter's field.
    pub(super) fn filter_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        let hits = self.filter_hits(cx);
        let lit = self.sidebar.read(cx).header.read(cx).filter.lit.clone();
        match key {
            "enter" => {
                if let Some(hit) = hits.iter().find(|h| Some(*h) == lit.as_ref()).or(hits.first()).cloned() {
                    self.pick_hit(hit, window, cx);
                }
            }
            _ => self.set_lit(step(&hits, lit.as_ref(), key == "down"), cx),
        }
    }

    /// Shows `hit`'s row, unfolded, and closes the filter.
    fn pick_hit(&mut self, (p, tree): Hit, window: &mut Window, cx: &mut Context<Self>) {
        self.shut_filtered_folds(cx);
        self.sidebar.read(cx).header.clone().update(cx, |h, cx| h.close_filter(window, cx));
        self.store.sections.reveal(&p, tree.as_deref());
        self.store.collapsed.remove(&p);
        self.store.save();
        match tree {
            Some(key) => {
                let main = self.tree_of(&p, cx).unwrap_or_else(|| p.clone());
                self.select_tree(p, (key != main).then_some(key), cx);
            }
            None => self.select_project(p, cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::sections::{Block, Item, Listed};
    use super::{hits, step};

    #[test]
    fn hits_run_through_pins_groups_then_the_rest_with_each_projects_trees_after_it() {
        let block = |p: &str, only: &[&str]| Block { ix: 0, project: p.into(), only: Some(only.iter().map(|s| s.to_string()).collect()), named: true };
        let listed = Listed {
            pinned: vec![Item::Tree { project: "/a".into(), key: "/wt".into() }, Item::Project(block("/b", &[]))],
            groups: vec![(1, vec![block("/c", &["/c"])])],
            other: vec![block("/d", &[])],
        };
        let got = hits(&listed);
        let want: Vec<(String, Option<String>)> =
            vec![("/a".into(), Some("/wt".into())), ("/b".into(), None), ("/c".into(), None), ("/c".into(), Some("/c".into())), ("/d".into(), None)];
        assert_eq!(got, want);
    }

    #[test]
    fn a_project_listed_only_for_its_matching_trees_is_not_a_hit_itself() {
        let listed =
            Listed { pinned: vec![], groups: vec![], other: vec![Block { ix: 0, project: "/a".into(), only: Some(vec!["/wt".into()]), named: false }] };
        let got = hits(&listed);
        let want: Vec<(String, Option<String>)> = vec![("/a".into(), Some("/wt".into()))];
        assert_eq!(got, want);
    }

    #[test]
    fn stepping_wraps_round_and_starts_on_the_first_hit() {
        let hits: Vec<(String, Option<String>)> = vec![("/a".into(), None), ("/b".into(), None)];
        assert_eq!(step(&hits, None, true), Some(hits[0].clone()));
        assert_eq!(step(&hits, Some(&hits[1]), true), Some(hits[0].clone()));
        assert_eq!(step(&hits, Some(&hits[0]), false), Some(hits[1].clone()));
        assert_eq!(step(&[], None, true), None);
    }
}
