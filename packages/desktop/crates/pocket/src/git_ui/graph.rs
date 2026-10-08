mod lanes;
mod row;

use crate::desktop::Desktop;
use crate::desktop::chrome::{SEAM, Side, empty};
use crate::git_ui::diff::changed;
use git::graph::{GraphRow, Lanes};
use git::{CommitFile, GraphCommit, Tips};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use theme::*;
use workspace::Doc;

const PAGE: usize = 50;
const ROW_GROUP: &str = "graph-row";
const REVEAL: Duration = Duration::from_millis(200);
const HEADER: f32 = 28.;
/// The open graph's top padding, header and three rows.
const MIN_GRAPH: f32 = 4. + HEADER + 3. * lanes::H;
const MIN_CHANGES: f32 = 120.;

/// What the seam above the open graph drags.
#[derive(Clone, Copy)]
struct GraphSeam;

/// The graph's share of the `height` it splits with the Changes list when dragged to `graph_height`; `None` when both can't fit.
fn graph_share(graph_height: f32, height: f32) -> Option<f32> {
    let max = height - MIN_CHANGES;
    (max >= MIN_GRAPH).then(|| graph_height.clamp(MIN_GRAPH, max) / height)
}

/// A row of the graph list: a commit, a file of an expanded commit, or the row that loads the next page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Item {
    Commit(usize),
    File(usize, usize),
    More,
}

/// `n` commits of the history of `tips` from `skip` on, read for worktree `cwd`.
pub struct Page {
    cwd: String,
    tips: Tips,
    skip: usize,
    n: usize,
    commits: Vec<GraphCommit>,
}

fn read_page(cwd: String, tips: Tips, skip: usize, n: usize) -> Page {
    let commits = if tips.head.is_empty() { Vec::new() } else { git::log_graph(&cwd, &tips.revs(), skip, n) };
    Page { cwd, tips, skip, n, commits }
}

pub struct GraphState {
    pub(crate) open: bool,
    /// The open graph's dragged share of the height it splits with the Changes list; `None` splits it evenly.
    pub(crate) share: Option<f32>,
    pub(crate) list: ListState,
    cwd: Option<String>,
    tips: Tips,
    pub(crate) commits: Vec<GraphCommit>,
    rows: Vec<GraphRow>,
    lanes: Lanes,
    more: bool,
    refreshing: bool,
    loading: bool,
    expanded: HashSet<String>,
    files: HashMap<String, Vec<CommitFile>>,
    items: Vec<Item>,
    reveal: Option<(String, Option<Instant>)>,
}

impl Default for GraphState {
    fn default() -> Self {
        Self {
            open: true,
            share: None,
            list: ListState::new(0, ListAlignment::Top, px(400.)),
            cwd: None,
            tips: Tips::default(),
            commits: Vec::new(),
            rows: Vec::new(),
            lanes: Lanes::default(),
            more: false,
            refreshing: false,
            loading: false,
            expanded: HashSet::new(),
            files: HashMap::new(),
            items: Vec::new(),
            reveal: None,
        }
    }
}

impl GraphState {
    /// Starts with the `share` saved last launch, dropping one outside (0, 1) that would break the split.
    pub fn new(share: Option<f32>) -> Self {
        Self { share: share.filter(|s| *s > 0. && *s < 1.), ..Self::default() }
    }

    /// Drags the graph to `graph_height` of the `height` it splits with the Changes list, returning whether its share changed.
    pub fn drag(&mut self, graph_height: f32, height: f32) -> bool {
        let share = graph_share(graph_height, height);
        let moved = share.is_some() && share != self.share;
        if moved {
            self.share = share;
        }
        moved
    }

    pub fn busy(&self) -> bool {
        self.refreshing || self.loading
    }

    /// What a refresh for worktree `cwd` reads: the tips already shown, which skip the read when unchanged, and how many commits.
    pub fn reread(&self, cwd: &str) -> (Option<Tips>, usize) {
        if self.cwd.as_deref() == Some(cwd) {
            (Some(self.tips.clone()), self.commits.len().max(PAGE))
        } else {
            (None, PAGE)
        }
    }

    /// Shows `page`: one read from the top replaces the graph, a later one extends it unless the worktree, tips or length moved on since.
    pub fn apply(&mut self, page: Page) -> bool {
        if page.skip == 0 {
            if self.cwd.as_ref() != Some(&page.cwd) {
                self.expanded.clear();
                self.files.clear();
                self.items.clear();
                self.list.reset(0);
            }
            self.cwd = Some(page.cwd);
            self.tips = page.tips;
            self.commits.clear();
            self.rows.clear();
            self.lanes = Lanes::default();
        } else if self.cwd.as_ref() != Some(&page.cwd) || self.tips != page.tips || page.skip != self.commits.len() {
            return false;
        }
        self.more = page.commits.len() == page.n;
        for c in &page.commits {
            self.rows.push(self.lanes.push(&c.sha, &c.parents, |s| self.tips.color(s)));
        }
        self.commits.extend(page.commits);
        self.relist();
        true
    }

    fn relist(&mut self) {
        let mut items = Vec::with_capacity(self.items.len());
        for (i, c) in self.commits.iter().enumerate() {
            items.push(Item::Commit(i));
            if self.expanded.contains(&c.sha)
                && let Some(files) = self.files.get(&c.sha)
            {
                items.extend((0..files.len()).map(|j| Item::File(i, j)));
            }
        }
        if self.more {
            items.push(Item::More);
        }
        let (range, count) = changed(&self.items, &items);
        self.list.splice(range, count);
        self.items = items;
        if let Some((sha, at @ None)) = &mut self.reveal
            && self.files.contains_key(sha)
        {
            *at = Some(Instant::now());
        }
    }

    /// Expands or folds commit `i`; true when it expanded and its files still have to be read.
    pub fn toggle(&mut self, i: usize) -> bool {
        let Some(sha) = self.commits.get(i).map(|c| c.sha.clone()) else { return false };
        if self.expanded.insert(sha.clone()) {
            self.reveal = Some((sha.clone(), None));
        } else {
            self.expanded.remove(&sha);
        }
        self.relist();
        self.expanded.contains(&sha) && !self.files.contains_key(&sha)
    }

    /// Keeps the files of commit `sha` read in worktree `cwd`, unless the graph shows another worktree now.
    pub fn set_files(&mut self, cwd: &str, sha: String, files: Vec<CommitFile>) -> bool {
        if self.cwd.as_deref() != Some(cwd) {
            return false;
        }
        self.files.insert(sha, files);
        self.relist();
        true
    }

    /// How far the files of commit `sha` have grown in at `now`, from 0 to 1, timed from when its last expansion listed them.
    pub fn grown(&self, sha: &str, now: Instant) -> f32 {
        match &self.reveal {
            Some((s, Some(at))) if s == sha => (now.saturating_duration_since(*at).as_secs_f32() / REVEAL.as_secs_f32()).min(1.),
            _ => 1.,
        }
    }

    pub fn commit(&self, sha: &str) -> Option<&GraphCommit> {
        self.commits.iter().find(|c| c.sha == sha)
    }

    pub fn file(&self, sha: &str, path: &str) -> Option<&CommitFile> {
        self.files.get(sha)?.iter().find(|f| f.path == path)
    }

    pub fn file_doc(&self, i: usize, j: usize) -> Option<Doc> {
        let sha = &self.commits.get(i)?.sha;
        Some(Doc::CommitFile { sha: sha.clone(), path: self.files.get(sha)?.get(j)?.path.clone() })
    }
}

impl Desktop {
    /// Reads the graph again when the worktree on screen moved its branch tips; a graph already shown keeps its length.
    pub(crate) fn refresh_graph(&mut self, cx: &mut Context<Self>) {
        if self.side != Side::Changes || !self.graph.open || self.graph.refreshing {
            return;
        }
        let Some(cwd) = self.cwd() else { return };
        let (shown, n) = self.graph.reread(&cwd);
        self.graph.refreshing = true;
        let task = cx.background_executor().spawn({
            let cwd = cwd.clone();
            async move {
                let tips = git::tips(&cwd).unwrap_or_default();
                (shown.as_ref() != Some(&tips)).then(|| read_page(cwd, tips, 0, n))
            }
        });
        cx.spawn(async move |this, cx| {
            let page = task.await;
            this.update(cx, |d, cx| {
                d.graph.refreshing = false;
                if d.cwd().as_ref() != Some(&cwd) {
                    d.refresh_graph(cx);
                } else if page.is_some_and(|p| d.graph.apply(p)) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn load_graph_page(&mut self, cx: &mut Context<Self>) {
        if !self.graph.more || self.graph.busy() {
            return;
        }
        let Some(cwd) = self.graph.cwd.clone() else { return };
        let (tips, skip) = (self.graph.tips.clone(), self.graph.commits.len());
        self.graph.loading = true;
        let task = cx.background_executor().spawn(async move { read_page(cwd, tips, skip, PAGE) });
        cx.spawn(async move |this, cx| {
            let page = task.await;
            this.update(cx, |d, cx| {
                d.graph.loading = false;
                if d.graph.apply(page) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn toggle_commit(&mut self, i: usize, cx: &mut Context<Self>) {
        if self.graph.toggle(i)
            && let Some(cwd) = self.graph.cwd.clone()
        {
            let commit = &self.graph.commits[i];
            let (sha, parent) = (commit.sha.clone(), commit.parents.first().cloned());
            let task = cx.background_executor().spawn({
                let (cwd, sha) = (cwd.clone(), sha.clone());
                async move { git::commit_files(&cwd, &sha, parent.as_deref()) }
            });
            cx.spawn(async move |this, cx| {
                let files = task.await;
                this.update(cx, |d, cx| {
                    if d.graph.set_files(&cwd, sha, files) {
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
        }
        cx.notify();
    }

    /// `changes` above the collapsible graph, splitting the height between them while it's open; drag the seam between to resize.
    pub(crate) fn with_graph(&mut self, changes: Div, cx: &mut Context<Self>) -> Div {
        div().flex_1().min_h_0().flex().flex_col().child(changes).child(self.graph_section(cx)).on_drag_move(cx.listener(
            |this, e: &DragMoveEvent<GraphSeam>, _, cx| {
                // A drag move comes every frame, moved or not; notifying on each would redraw forever.
                if this.graph.drag(f32::from(e.bounds.bottom() - e.event.position.y), f32::from(e.bounds.size.height)) {
                    this.save_soon(cx);
                    cx.notify();
                }
            },
        ))
    }

    fn graph_section(&mut self, cx: &mut Context<Self>) -> Div {
        let fresh = self.graph.cwd.is_some() && self.graph.cwd == self.cwd();
        if self.graph.open && !fresh {
            self.refresh_graph(cx);
        }
        let open = self.graph.open;
        let header = div()
            .id("graph-header")
            .h(px(HEADER))
            .pl(px(6.))
            .pr(px(6.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(8.))
            .cursor_pointer()
            .hover(|s| s.bg(FILL_1))
            .child(icon(if open { "chevron-down" } else { "chevron-right" }, 12., TEXT_4))
            .child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_3).child("Graph"))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.graph.open = !this.graph.open;
                this.refresh_graph(cx);
                cx.notify();
            }));
        let section = div().flex().flex_col().border_t(px(0.5)).border_color(SEPARATOR).pt(px(4.)).child(div().px(px(8.)).child(header));
        if !open {
            return section.flex_none().pb(px(4.));
        }
        let body = if !fresh {
            div()
        } else if self.graph.items.is_empty() && !self.graph.busy() {
            empty("No commits.")
        } else {
            let rows = list(self.graph.list.clone(), cx.processor(|this, ix, window, cx| this.graph_item(ix, window, cx))).size_full().pb(px(8.));
            div().relative().flex_1().min_h_0().px(px(8.)).child(rows).vertical_scrollbar(&self.graph.list)
        };
        let section = section.flex_1().min_h_0().relative().child(body).children(self.graph_seam(cx));
        match self.graph.share {
            Some(share) => section.flex_grow(share / (1. - share)),
            None => section,
        }
    }

    /// The seam on the open graph's top edge: drag it to resize, double-click it to split evenly again.
    fn graph_seam(&self, cx: &mut Context<Self>) -> Option<Deferred> {
        // The deferred handle paints above overlays, so it would steal their clicks.
        if self.overlay.is_some() {
            return None;
        }
        let line = div().absolute().left_0().right_0().top(px(SEAM / 2.)).h(px(1.)).group_hover("seam", |s| s.bg(SEPARATOR_STRONG));
        let handle = div()
            .id("graph-seam")
            .group("seam")
            .absolute()
            .left_0()
            .right_0()
            .top(px(-SEAM / 2.))
            .h(px(SEAM))
            .occlude()
            .cursor_row_resize()
            .child(line)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, _, cx| {
                    if e.click_count == 2 {
                        this.graph.share = None;
                        this.save_soon(cx);
                        cx.notify();
                    }
                }),
            )
            .on_drag(GraphSeam, |_, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| EmptyView)
            });
        // Deferred so the handle straddles the edge above the section.
        Some(deferred(handle))
    }

    fn graph_item(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match self.graph.items.get(ix).copied() {
            Some(Item::Commit(i)) => self.commit_row(i, cx).into_any_element(),
            Some(Item::File(i, j)) => {
                let row = self.file_row(ix, i, j, cx);
                let grown = if cx.reduce_motion() { 1. } else { self.graph.grown(&self.graph.commits[i].sha, Instant::now()) };
                if grown < 1. {
                    window.request_animation_frame();
                    div().h(px(lanes::H * ease_out_quint()(grown))).overflow_hidden().child(row).into_any_element()
                } else {
                    row.into_any_element()
                }
            }
            Some(Item::More) => {
                self.load_graph_page(cx);
                self.more_row(cx).into_any_element()
            }
            None => Empty.into_any_element(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{GraphState, Item, MIN_CHANGES, MIN_GRAPH, PAGE, Page, REVEAL, graph_share};
    use git::graph::Lanes;
    use git::{CommitFile, GraphCommit, Tips};
    use gpui_kit::{ListOffset, px};
    use std::time::Instant;
    use workspace::Doc;

    fn commit(sha: &str, parents: &[&str]) -> GraphCommit {
        GraphCommit { sha: sha.into(), parents: parents.iter().map(|p| p.to_string()).collect(), author: "t".into(), subject: sha.into() }
    }

    fn tips(head: &str) -> Tips {
        Tips { head: head.into(), ..Tips::default() }
    }

    fn page(skip: usize, n: usize, commits: &[GraphCommit]) -> Page {
        Page { cwd: "/r".into(), tips: tips("m"), skip, n, commits: commits.to_vec() }
    }

    fn history() -> Vec<GraphCommit> {
        vec![commit("m", &["a", "b"]), commit("b", &["a"]), commit("a", &["r"]), commit("r", &[])]
    }

    #[test]
    fn lanes_pushed_page_by_page_match_lanes_pushed_at_once() {
        let h = history();
        let mut g = GraphState::default();
        assert!(g.apply(page(0, 2, &h[..2])));
        assert!(g.apply(page(2, 2, &h[2..])));
        let (mut lanes, t) = (Lanes::default(), tips("m"));
        let all: Vec<_> = h.iter().map(|c| lanes.push(&c.sha, &c.parents, |s| t.color(s))).collect();
        assert_eq!(g.rows, all);
    }

    #[test]
    fn a_page_read_for_an_older_history_is_dropped() {
        let h = history();
        let mut g = GraphState::default();
        g.apply(page(0, 2, &h[..2]));
        assert!(!g.apply(page(3, 2, &h[2..])));
        assert!(!g.apply(Page { tips: tips("x"), ..page(2, 2, &h[2..]) }));
        assert!(!g.apply(Page { cwd: "/other".into(), ..page(2, 2, &h[2..]) }));
        assert_eq!(g.commits.len(), 2);
    }

    #[test]
    fn a_short_page_ends_the_history() {
        let h = history();
        let mut g = GraphState::default();
        g.apply(page(0, 2, &h[..2]));
        assert_eq!(g.items.last(), Some(&Item::More));
        g.apply(page(2, 3, &h[2..]));
        assert_eq!(g.items.last(), Some(&Item::Commit(3)));
    }

    #[test]
    fn an_expanded_commit_lists_its_files_under_it() {
        let h = history();
        let mut g = GraphState::default();
        g.apply(page(0, 5, &h));
        let file = |p: &str| CommitFile { path: p.into(), old_path: None, status: 'M' };
        assert!(g.toggle(1));
        assert!(g.set_files("/r", "b".into(), vec![file("x"), file("y")]));
        assert_eq!(g.items[..4], [Item::Commit(0), Item::Commit(1), Item::File(1, 0), Item::File(1, 1)]);
        assert_eq!(g.items.len(), 6);
        assert!(!g.toggle(1));
        assert_eq!(g.items.len(), 4);
        assert!(!g.toggle(1));
        assert_eq!(g.items.len(), 6);
        assert!(!g.set_files("/other", "b".into(), Vec::new()));
    }

    #[test]
    fn a_refresh_rereads_as_many_commits_as_are_shown() {
        let h = history();
        let mut g = GraphState::default();
        assert_eq!(g.reread("/r"), (None, PAGE));
        g.apply(page(0, 2, &h[..2]));
        assert_eq!(g.reread("/r"), (Some(tips("m")), PAGE));
        let long: Vec<_> = (0..PAGE + 1).map(|k| commit(&k.to_string(), &[])).collect();
        g.apply(page(0, PAGE + 1, &long));
        assert_eq!(g.reread("/r").1, PAGE + 1);
        assert_eq!(g.reread("/other"), (None, PAGE));
    }

    #[test]
    fn another_worktree_starts_at_the_top() {
        let h = history();
        let mut g = GraphState::default();
        g.apply(page(0, 5, &h));
        g.list.scroll_to(ListOffset { item_ix: 2, offset_in_item: px(0.) });
        g.apply(page(0, 5, &h));
        assert_eq!(g.list.logical_scroll_top().item_ix, 2);
        g.apply(Page { cwd: "/other".into(), ..page(0, 5, &h) });
        assert_eq!(g.list.logical_scroll_top().item_ix, 0);
    }

    #[test]
    fn an_expanded_commits_files_grow_in_from_when_they_are_listed() {
        let h = history();
        let mut g = GraphState::default();
        g.apply(page(0, 5, &h));
        g.toggle(1);
        let read = Instant::now();
        g.set_files("/r", "b".into(), vec![CommitFile { path: "x".into(), old_path: None, status: 'M' }]);
        let at = g.reveal.as_ref().and_then(|r| r.1).unwrap();
        assert!(at >= read);
        assert_eq!(g.grown("b", at), 0.);
        assert_eq!(g.grown("b", at + REVEAL / 2), 0.5);
        assert_eq!(g.grown("b", at + REVEAL * 2), 1.);
        assert_eq!(g.grown("a", at), 1.);
    }

    #[test]
    fn a_file_row_opens_that_file_at_its_commit() {
        let h = history();
        let mut g = GraphState::default();
        g.apply(page(0, 5, &h));
        g.toggle(1);
        g.set_files("/r", "b".into(), vec![CommitFile { path: "src/x".into(), old_path: None, status: 'M' }]);
        assert_eq!(g.file_doc(1, 0), Some(Doc::CommitFile { sha: "b".into(), path: "src/x".into() }));
        assert_eq!(g.file_doc(0, 0), None);
    }

    #[test]
    fn a_dragged_graph_leaves_room_for_itself_and_the_changes_list() {
        assert_eq!(graph_share(200., 800.), Some(0.25));
        assert_eq!(graph_share(0., 800.), Some(MIN_GRAPH / 800.));
        assert_eq!(graph_share(800., 800.), Some((800. - MIN_CHANGES) / 800.));
    }

    #[test]
    fn a_graph_too_short_to_split_ignores_the_drag() {
        let mut g = GraphState::new(Some(0.3));
        assert!(!g.drag(100., MIN_GRAPH + MIN_CHANGES - 1.));
        assert_eq!(g.share, Some(0.3));
    }

    #[test]
    fn a_drag_to_where_the_graph_already_is_changes_nothing() {
        let mut g = GraphState::default();
        assert_eq!((g.drag(200., 800.), g.drag(200., 800.)), (true, false));
    }

    #[test]
    fn a_saved_share_that_would_break_the_split_is_dropped() {
        assert_eq!([0., 1., -0.2, 0.4].map(|s| GraphState::new(Some(s)).share), [None, None, None, Some(0.4)]);
    }
}
